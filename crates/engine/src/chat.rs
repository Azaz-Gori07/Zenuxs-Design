//! The conversation the AI chat is having, as the engine holds it.
//!
//! The state is deliberately plain: the messages so far, the design context that rides along,
//! and which model the design role resolves to. What the engine does *not* hold is the network:
//! a request runs on a worker thread the interface owns, because [`Session`] is not shareable
//! across threads and a model answer takes seconds.
//!
//! Everything is a command (`ai.chat.*`), so the interface, the CLI, the control channel and the
//! MCP server all see the same conversation the same way.

use figcraft_ai::models::AIModelRole;
use figcraft_ai::transport::ChatMessage;
use figcraft_ai::usage::UsageEvent;
use serde_json::{Value, json};

use crate::{Result, Session, bad};

/// The most messages a conversation keeps; the oldest user/assistant turns go first (tool
/// answers go with the call they answer, so a pair is never split).
pub const MAX_MESSAGES: usize = 200;

/// The steps one send may take before it stops: model answer → tools → model answer …
///
/// A whole screen takes many commands, and one step is one model answer with every tool call it
/// asks for, so this bounds a runaway loop rather than a design. A brief that names a *product*
/// ("an e-commerce web app") asks for several screens with a shared shell — a header, a footer,
/// a home, a listing, a detail, each with real copy — and the run measured on that brief used
/// most of forty steps on the shell alone. The budget is what a real request costs, not the
/// cheapest one, and the transcript's Continue control is what a person uses to spend more.
pub const MAX_STEPS: usize = 120;

/// The most a render handed to the Vision model may weigh. Past this the picture is rendered
/// again at half the scale: a provider refuses a request that size, and the layout is the
/// question, not the pixel count.
const MAX_RENDER_BYTES: usize = 4 * 1024 * 1024;

impl Session {
    /// Append a message to the conversation in front.
    ///
    /// A tool answer must answer a tool call the conversation holds, and a user or assistant
    /// message must not carry tool parts of its own: the shapes are checked here, so a caller
    /// that mixes them up is told rather than producing a request a provider refuses.
    pub fn chat_append(&mut self, message: ChatMessage) -> Result<()> {
        let cmd = "ai.chat.append";
        if message.role == figcraft_ai::transport::Role::Tool {
            let Some(id) = &message.tool_call_id else {
                return Err(bad(cmd, "a tool answer names the call it answers (`toolCallId`)"));
            };
            let answers = self
                .chat
                .messages
                .iter()
                .rev()
                .take_while(|m| m.role != figcraft_ai::transport::Role::User)
                .any(|m| m.tool_calls.iter().any(|c| &c.id == id));
            if !answers {
                return Err(bad(cmd, format!("no open tool call \"{id}\" to answer")));
            }
        } else if message.role == figcraft_ai::transport::Role::User && !message.tool_calls.is_empty() {
            return Err(bad(cmd, "a person's message carries no tool calls"));
        }
        if message.role == figcraft_ai::transport::Role::User {
            self.chat.pending_tool_calls.clear();
        }
        self.chat.messages.push(message);
        self.chat.trim();
        // Keep the saved copy in step with the live one.
        self.save_chat();
        Ok(())
    }

    /// The conversation in front.
    pub fn chat_messages(&self) -> &[ChatMessage] {
        &self.chat.messages
    }

    /// The tool calls the model asked for on its last answer, not yet answered.
    pub fn chat_pending_tool_calls(&self) -> &[figcraft_ai::transport::ToolCall] {
        &self.chat.pending_tool_calls
    }

    /// The conversation in front, as a saved one, for `ai.chat.new` to shelve.
    pub fn conversation(&self, document: Option<&str>) -> Option<figcraft_ai::ChatConversation> {
        if self.chat.messages.is_empty() {
            return None;
        }
        Some(figcraft_ai::ChatConversation {
            id: self.chat.id.clone(),
            title: figcraft_ai::ChatConversation::title_of(&self.chat.messages),
            document: document.map(str::to_string).or_else(|| self.chat.document.clone()),
            messages: self.chat.messages.clone(),
            updated_at: crate::ai::now_ms(),
        })
    }

    /// Save the conversation in front into the history.
    pub fn save_chat(&mut self) {
        if self.chat.messages.is_empty() || self.chat.id.is_empty() {
            return;
        }
        let Some(conversation) = self.conversation(None) else { return };
        self.prefs.ai.chat_history.save(conversation, crate::ai::now_ms());
        self.prefs_unsaved = true;
    }

    /// Load a saved conversation into the session.
    pub fn chat_load(&mut self, conversation: figcraft_ai::ChatConversation) {
        self.chat.id = conversation.id.clone();
        self.chat.document = conversation.document.clone();
        self.chat.messages = conversation.messages;
        self.chat.context = String::new();
        self.chat.pending_tool_calls.clear();
        self.chat.busy = false;
        self.chat.pending_send = false;
    }

    /// A layer's image bytes as base64, for a tool that runs off the session's thread.
    pub fn chat_image_data(&self, node_id: &str) -> Result<String> {
        let id = node_id.parse::<u64>().map_err(|_| bad("ai.chat.image", "the layer id is not a number"))?;
        let doc = self.doc()?;
        let node = doc.node(figcraft_doc::NodeId(id)).ok_or_else(|| bad("ai.chat.image", "there is no layer with this id"))?;
        let hash = node.fills.iter().find_map(|fill| match &fill.kind {
            figcraft_color::PaintKind::Image(image) => Some(image.image.as_str()),
            _ => None,
        });
        let Some(hash) = hash else {
            return Err(bad("ai.chat.image", "this layer has no image fill"));
        };
        let image = doc.images.get(hash).ok_or_else(|| bad("ai.chat.image", "the image's data is missing"))?;
        use base64::Engine as _;
        Ok(base64::engine::general_purpose::STANDARD.encode(image.bytes.as_slice()))
    }

    /// What layers look like, as PNG bytes, for a tool that runs off the session's thread: the
    /// chat's `screenshot` and `review_design` hand these pixels to the Vision model, so the agent
    /// can see its own work. With no `ids`, the page's top-level frames.
    ///
    /// The bytes are capped: a whole page at 2× is tens of megabytes, and a vision request that
    /// size is refused by the provider and useless to the model anyway.
    pub fn chat_render_png(&self, ids: &[figcraft_doc::NodeId], scale: f64) -> Result<Vec<u8>> {
        let doc = self.doc()?;
        let ids = if ids.is_empty() {
            let page = doc.node(self.ds()?.page).ok_or_else(|| bad("ai.chat.render", "there is no page to render"))?;
            page.children.clone()
        } else {
            ids.to_vec()
        };
        if ids.is_empty() {
            return Err(bad("ai.chat.render", "there is nothing on the page to look at yet"));
        }
        let scale = if scale.is_finite() { scale.clamp(0.25, 4.0) } else { 1.0 };
        let bytes = crate::cmd::file::export_bytes(doc, &ids, figcraft_doc::ExportFormat::Png, scale)
            .map_err(|e| bad("ai.chat.render", format!("the layers could not be rendered: {e}")))?;
        if bytes.len() > MAX_RENDER_BYTES {
            // Too big to send: a smaller render still shows the layout, which is the question.
            return crate::cmd::file::export_bytes(doc, &ids, figcraft_doc::ExportFormat::Png, scale / 2.0)
                .map_err(|e| bad("ai.chat.render", format!("the layers could not be rendered: {e}")));
        }
        Ok(bytes)
    }

    /// The model a role resolves to, as the chat needs it: the profile, its connection and the
    /// key the connection uses.
    ///
    /// `Err` says what to fix, in the words the chat shows.
    pub fn chat_model(&self, role: AIModelRole) -> Result<(figcraft_ai::models::ResolvedAIModelRole, String)> {
        let resolved = self.prefs.ai.models.resolve(role).ok_or_else(|| {
            crate::EngineError::Other(match role {
                AIModelRole::Design => "No model is configured for the Design role. Add one in AI & Agents.".into(),
                other => format!("No model is assigned to the {} role. Choose one in AI & Agents.", other.id()),
            })
        })?;
        let store = self.credentials.as_ref().ok_or_else(|| crate::EngineError::Other("This build has nowhere to keep a key.".into()))?;
        let reference = crate::ai::connection_credential_ref(resolved.connection.provider.id(), &resolved.connection.id);
        let key = store
            .read(&reference)
            .map_err(|_| crate::EngineError::Other("The key store is not reachable.".into()))?
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| crate::EngineError::Other("The connection has no API key saved. Add it in AI & Agents.".into()))?;
        Ok((resolved, key))
    }
}

/// The conversation a session holds.
#[derive(Default)]
pub struct ChatState {
    /// The messages, oldest first.
    pub messages: Vec<ChatMessage>,
    /// The tool calls the model asked for that have not been answered.
    pub pending_tool_calls: Vec<figcraft_ai::transport::ToolCall>,
    /// The design context of the conversation in front (the selection as text), refreshed by the
    /// interface before it sends.
    pub context: String,
    /// Whether a request is out right now.
    pub busy: bool,
    /// A message was queued by a command and the interface has not sent it yet.
    pub pending_send: bool,
    /// The id of the conversation in front.
    pub id: String,
    /// The document this conversation belongs to, remembered when it was made.
    pub document: Option<String>,
}

impl ChatState {
    /// Keep the conversation bounded: whole user/assistant/tool groups go, never a tool answer
    /// without the call it answers.
    fn trim(&mut self) {
        while self.messages.len() > MAX_MESSAGES {
            // The first droppable index: after the oldest user message (a tool message hangs
            // off the assistant turn before it, so dropping from a user message keeps pairs).
            let cut = self.messages.iter().position(|m| m.role == figcraft_ai::transport::Role::User).unwrap_or(0);
            if cut == 0 && self.messages.first().is_some_and(|m| m.role == figcraft_ai::transport::Role::Tool) {
                break;
            }
            self.messages.drain(..=cut.min(self.messages.len() - 1));
        }
    }

    /// Forget the whole conversation.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.pending_tool_calls.clear();
        self.busy = false;
    }

    /// What a step just cost, recorded once per model answer.
    pub fn record_usage(&mut self, provider: &str, model: &str, response: &figcraft_ai::transport::ChatResponse) -> UsageEvent {
        let mut event = UsageEvent::step(provider, model, response.input_tokens.unwrap_or(0), response.output_tokens.unwrap_or(0));
        event.cache_read_tokens = response.cache_read_tokens;
        event.cache_write_tokens = response.cache_write_tokens;
        event
    }
}

/// The conversation as the commands return it.
pub fn chat_json(state: &ChatState) -> Value {
    json!({
        "busy": state.busy,
        "context": state.context,
        "messages": state.messages.iter().map(message_json).collect::<Vec<_>>(),
    })
}

/// One message as the conversation shows it.
pub fn message_json(message: &ChatMessage) -> Value {
    let mut out = json!({"role": match message.role {
        figcraft_ai::transport::Role::User => "user",
        figcraft_ai::transport::Role::Assistant => "assistant",
        figcraft_ai::transport::Role::Tool => "tool",
    }, "content": message.content});
    if !message.tool_calls.is_empty() {
        out["toolCalls"] =
            json!(message.tool_calls.iter().map(|c| json!({"id": c.id, "name": c.name, "arguments": c.arguments})).collect::<Vec<_>>());
    }
    if let Some(id) = &message.tool_call_id {
        out["toolCallId"] = json!(id);
    }
    if !message.images.is_empty() {
        out["images"] = json!(message.images);
    }
    out
}

/// A message from the conversation's JSON.
pub fn message_of(cmd: &str, value: &Value) -> Result<ChatMessage> {
    let role = value.get("role").and_then(Value::as_str).unwrap_or("");
    let content = value.get("content").and_then(Value::as_str).unwrap_or("").to_string();
    let tool_call_id = value.get("toolCallId").and_then(Value::as_str).map(str::to_string);
    let tool_calls: Vec<figcraft_ai::transport::ToolCall> = value
        .get("toolCalls")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|c| figcraft_ai::transport::ToolCall {
                    id: c.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
                    name: c.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                    arguments: c.get("arguments").cloned().unwrap_or_else(|| json!({})),
                })
                .collect()
        })
        .unwrap_or_default();
    let images: Vec<String> = value
        .get("images")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let role = match role {
        "user" => figcraft_ai::transport::Role::User,
        "assistant" => figcraft_ai::transport::Role::Assistant,
        "tool" => figcraft_ai::transport::Role::Tool,
        other => return Err(bad(cmd, format!("\"{other}\" is not a message role"))),
    };
    Ok(ChatMessage { role, content, tool_call_id, tool_calls, images })
}

/// Run the command `tool` with `arguments` on the session, as the AI's tools do.
///
/// The tool set is the engine's own command set: the model calls a command by id with its
/// parameters, exactly as the control channel and the MCP server do. A command that opens a
/// dialog is refused here (programmatic calls never open dialogs), and everything else runs as
/// the person running it would.
pub fn run_tool(s: &mut Session, tool: &str, arguments: &Value) -> Result<Value> {
    if tool.starts_with("dialog.") {
        return Err(bad("ai.tool", "programmatic calls do not open dialogs"));
    }
    s.execute(tool, arguments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use figcraft_ai::transport::ToolCall;

    #[test]
    fn a_user_message_can_be_appended() {
        let mut s = Session::empty();
        s.chat_append(ChatMessage::user("hello")).expect("the message is appended");
        assert_eq!(s.chat_messages().len(), 1);
        assert_eq!(s.chat_messages()[0].role, figcraft_ai::transport::Role::User);
    }

    #[test]
    fn a_tool_answer_needs_the_call_it_answers() {
        let mut s = Session::empty();
        let err = s.chat_append(ChatMessage::tool_result("c1", "done")).expect_err("an unanswered call is refused");
        assert!(err.to_string().contains("c1"), "{err}");

        let call = ToolCall { id: "c1".into(), name: "node.create".into(), arguments: json!({}) };
        s.chat.pending_tool_calls.push(call.clone());
        s.chat.messages.push(ChatMessage::assistant_calls("", vec![call]));
        s.chat_append(ChatMessage::tool_result("c1", "done")).expect("the answer is appended");
        assert_eq!(s.chat_messages().len(), 2);
    }

    #[test]
    fn only_the_model_carries_tool_calls() {
        let mut s = Session::empty();
        let err = s
            .chat_append(ChatMessage {
                role: figcraft_ai::transport::Role::User,
                content: "x".into(),
                tool_call_id: None,
                tool_calls: vec![ToolCall { id: "c".into(), name: "n".into(), arguments: json!({}) }],
                images: Vec::new(),
            })
            .expect_err("a user message with tool calls is refused");
        assert!(err.to_string().contains("tool calls"), "{err}");
    }

    #[test]
    fn a_new_user_message_clears_the_stale_pending_calls() {
        let mut s = Session::empty();
        s.chat.pending_tool_calls.push(ToolCall { id: "c1".into(), name: "n".into(), arguments: json!({}) });
        s.chat_append(ChatMessage::user("try again")).expect("the message is appended");
        assert!(s.chat_pending_tool_calls().is_empty(), "the old calls are not pending");
    }

    #[test]
    fn the_conversation_stays_bounded_without_splitting_a_tool_pair() {
        let mut s = Session::empty();
        for i in 0..(MAX_MESSAGES + 20) {
            if i % 3 == 0 {
                let _ = s.chat_append(ChatMessage::user(format!("m{i}")));
            } else if i % 3 == 1 {
                let _ = s.chat_append(ChatMessage::assistant("ok"));
            } else {
                let _ = s.chat_append(ChatMessage::tool_result("c", "{}"));
            }
        }
        assert!(s.chat_messages().len() <= MAX_MESSAGES);
        // The first message is never a tool answer without its call.
        assert_ne!(s.chat_messages().first().map(|m| m.role), Some(figcraft_ai::transport::Role::Tool));
    }

    #[test]
    fn a_message_round_trips_through_its_json() {
        let original = ChatMessage::assistant_calls("looking", vec![ToolCall { id: "c1".into(), name: "n".into(), arguments: json!({"a": 1}) }]);
        let json = message_json(&original);
        let back = message_of("test", &json).expect("the message reads back");
        assert_eq!(back, original);

        let with_img = ChatMessage::user_with_images("look", vec!["data:image/png;base64,xyz".into()]);
        let json_img = message_json(&with_img);
        let back_img = message_of("test", &json_img).expect("the message with images reads back");
        assert_eq!(back_img, with_img);
    }

    #[test]
    fn an_unknown_role_is_refused() {
        let err = message_of("test", &json!({"role": "system", "content": ""})).expect_err("an unknown role is refused");
        assert!(err.to_string().contains("system"), "{err}");
    }

    #[test]
    fn a_tool_runs_as_a_command_and_dialogs_are_refused() {
        let mut s = Session::empty();
        let _ = s.execute("file.new", &json!({}));
        let out = run_tool(&mut s, "node.create", &json!({"type": "rectangle", "width": 10, "height": 10})).expect("the tool runs");
        assert!(out.get("id").is_some());
        let err = run_tool(&mut s, "dialog.preferences", &json!({})).expect_err("a dialog is refused");
        assert!(err.to_string().contains("dialogs"), "{err}");
    }
}
