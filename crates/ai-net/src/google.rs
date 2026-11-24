//! The Google wire protocol: `streamGenerateContent` and its streamed answer.
//!
//! `POST {base}/models/{model}:streamGenerateContent?alt=sse` answers as named SSE events whose
//! data is a GenerateContentResponse. The parts of `candidates[0].content` carry text or a
//! `functionCall`; a call has no id of its own, so one is made up and kept consistent between
//! the call and its answer.
//!
//! The system instructions go in `systemInstruction`, the conversation's history in `contents`,
//! and a tool answer is a `functionResponse` part in a user turn.

use figcraft_ai::transport::{ChatMessage, ChatRequest, ChatResponse, Chunk, Role, Tool, ToolCall};
use serde_json::{Value, json};

pub type ParseError = String;

/// Build the request body.
pub fn request_body(request: &ChatRequest) -> Value {
    let (mut system, contents) = system_and_contents(&request.messages);
    if system.is_empty() {
        system = request.system.trim().to_string();
    }
    let mut body = json!({ "contents": contents });
    if !system.is_empty() {
        body["systemInstruction"] = json!({"parts": [{"text": system}]});
    }
    if request.max_output_tokens > 0 {
        body["generationConfig"] = json!({"maxOutputTokens": request.max_output_tokens});
    }
    if let Some(effort) = &request.reasoning_effort
        && !effort.trim().is_empty()
    {
        let thinking = json!({"thinkingConfig": {"thinkingBudget": budget_for(effort.trim())}});
        match body.get_mut("generationConfig").and_then(Value::as_object_mut) {
            Some(g) => g.extend(thinking.as_object().cloned().unwrap_or_default()),
            None => body["generationConfig"] = thinking,
        }
    }
    if !request.tools.is_empty() {
        body["tools"] = json!([{"functionDeclarations": request.tools.iter().map(tool_json).collect::<Vec<_>>()}]);
    }
    body
}

/// The URL a request goes to.
pub fn request_url(base_url: &str, model: &str) -> String {
    let base = base_url.trim_end_matches('/');
    format!("{base}/models/{model}:streamGenerateContent?alt=sse")
}

/// The thinking budget for an effort word: Google takes tokens, and zero means "the model decides".
fn budget_for(effort: &str) -> u32 {
    match effort {
        "off" | "none" => 0,
        "minimal" | "low" => 1024,
        "high" | "xhigh" => 8192,
        // "medium" and anything else: the model decides.
        _ => -1_i32 as u32,
    }
}

/// One tool, as the protocol names it.
fn tool_json(tool: &Tool) -> Value {
    json!({"name": tool.name, "description": tool.description, "parameters": tool.input_schema})
}

fn parse_data_url(data_url: &str) -> (&str, &str) {
    if let Some(rest) = data_url.strip_prefix("data:")
        && let Some((header, data)) = rest.split_once(";base64,")
    {
        return (header, data);
    }
    ("image/png", data_url)
}

/// The system text and the conversation, in the protocol's own shapes.
fn system_and_contents(messages: &[ChatMessage]) -> (String, Vec<Value>) {
    let system = String::new();
    let mut out = Vec::new();
    for message in messages {
        match message.role {
            Role::User if !message.images.is_empty() => {
                let mut parts = Vec::new();
                if !message.content.is_empty() {
                    parts.push(json!({"text": message.content}));
                }
                for img in &message.images {
                    let (mime, data) = parse_data_url(img);
                    parts.push(json!({"inline_data": {"mime_type": mime, "data": data}}));
                }
                out.push(json!({"role": "user", "parts": parts}));
            }
            Role::User => out.push(json!({"role": "user", "parts": [{"text": message.content}]})),
            Role::Assistant if message.tool_calls.is_empty() => out.push(json!({"role": "model", "parts": [{"text": message.content}]})),
            Role::Assistant => {
                let mut parts = Vec::new();
                if !message.content.is_empty() {
                    parts.push(json!({"text": message.content}));
                }
                for call in &message.tool_calls {
                    parts.push(json!({"functionCall": {"name": call.name, "args": call.arguments}}));
                }
                out.push(json!({"role": "model", "parts": parts}));
            }
            // The protocol answers a call with a functionResponse in a user turn.
            Role::Tool => {
                let call = messages.iter().rev().find_map(|m| m.tool_calls.iter().find(|c| Some(&c.id) == message.tool_call_id.as_ref()));
                let (name, args) = call.map(|c| (c.name.clone(), c.arguments.clone())).unwrap_or_default();
                out.push(json!({"role": "user", "parts": [{"functionResponse": {"name": name, "response": {"result": message.content}}}]}));
                let _ = args;
            }
        }
    }
    (system, out)
}

/// Parse one stream of events into chunks, and the answer that comes out of them.
pub fn parse_stream(events: &[(Option<String>, String)]) -> Result<(Vec<Chunk>, ChatResponse), ParseError> {
    let mut chunks = Vec::new();
    let mut response = ChatResponse::default();
    // Google names its calls with none, so one is made up per call in this answer, in order.
    let mut next_call = 0usize;

    for (kind, payload) in events {
        let _ = kind;
        let event: Value = serde_json::from_str(payload).map_err(|e| format!("a stream event was not JSON: {e}"))?;
        if let Some(usage) = event.get("usageMetadata") {
            response.input_tokens = usage.get("promptTokenCount").and_then(Value::as_u64).or(response.input_tokens);
            response.output_tokens = usage.get("candidatesTokenCount").and_then(Value::as_u64).or(response.output_tokens);
            response.cache_read_tokens = usage.get("cachedContentTokenCount").and_then(Value::as_u64).or(response.cache_read_tokens);
        }
        let Some(parts) = event.pointer("/candidates/0/content/parts").and_then(Value::as_array) else { continue };
        for part in parts {
            if let Some(text) = part.get("text").and_then(Value::as_str)
                && !text.is_empty()
            {
                chunks.push(Chunk::Text(text.to_string()));
                response.text.push_str(text);
            }
            if let Some(call) = part.get("functionCall") {
                let name = call.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                let arguments = call.get("args").cloned().unwrap_or_else(|| json!({}));
                let finished = ToolCall { id: format!("call_{next_call}"), name, arguments };
                next_call += 1;
                chunks.push(Chunk::ToolCall(finished.clone()));
                response.tool_calls.push(finished);
            }
            if part.get("thought").and_then(Value::as_bool) == Some(true)
                && let Some(text) = part.get("text").and_then(Value::as_str)
                && !text.is_empty()
            {
                chunks.push(Chunk::Reasoning(text.to_string()));
            }
        }
        if let Some(reason) = event.pointer("/candidates/0/finishReason").and_then(Value::as_str) {
            response.finish_reason = Some(reason.to_string());
        }
    }
    Ok((chunks, response))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage::user("hello")],
            system: "be brief".into(),
            tools: vec![Tool { name: "run_command".into(), description: "Run a command.".into(), input_schema: json!({"type": "object"}) }],
            model: "gemini-3.8-flash".into(),
            base_url: "https://api.example.com/v1".into(),
            api_key: "sk-test".into(),
            protocol: figcraft_ai::providers::Protocol::Google,
            responses_api: false,
            max_output_tokens: 512,
            reasoning_effort: Some("low".into()),
        }
    }

    #[test]
    fn the_url_names_the_model_and_asks_for_sse() {
        assert_eq!(
            request_url("https://generativelanguage.googleapis.com/v1beta/", "gemini-3.8-flash"),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.8-flash:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn a_user_message_with_images_formats_as_multimodal() {
        let mut r = request();
        r.messages = vec![ChatMessage::user_with_images(
            "Check this design",
            vec!["data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".into()],
        )];
        let body = request_body(&r);
        let parts = &body["contents"][0]["parts"];
        assert_eq!(parts[0]["text"], "Check this design");
        assert_eq!(parts[1]["inline_data"]["mime_type"], "image/png");
        assert_eq!(parts[1]["inline_data"]["data"], "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==");
    }

    #[test]
    fn the_request_sends_the_system_and_the_tools() {
        let body = request_body(&request());
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "be brief");
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 512);
        assert_eq!(body["generationConfig"]["thinkingConfig"]["thinkingBudget"], 1024);
        assert_eq!(body["tools"][0]["functionDeclarations"][0]["name"], "run_command");
    }

    #[test]
    fn a_tool_call_and_its_answer_round_trip() {
        let mut r = request();
        r.messages.push(ChatMessage::assistant_calls(
            "",
            vec![ToolCall { id: "call_0".into(), name: "run_command".into(), arguments: json!({"command": "node.create"}) }],
        ));
        r.messages.push(ChatMessage::tool_result("call_0", "{\"ok\":true}"));
        let body = request_body(&r);
        assert_eq!(body["contents"][1]["parts"][0]["functionCall"]["name"], "run_command");
        assert_eq!(body["contents"][2]["parts"][0]["functionResponse"]["name"], "run_command");
    }

    #[test]
    fn a_tool_answer_finds_its_call_by_id() {
        let mut r = request();
        r.messages.push(ChatMessage::assistant_calls(
            "",
            vec![
                ToolCall { id: "call_0".into(), name: "a".into(), arguments: json!({}) },
                ToolCall { id: "call_1".into(), name: "b".into(), arguments: json!({}) },
            ],
        ));
        r.messages.push(ChatMessage::tool_result("call_1", "ok"));
        let body = request_body(&r);
        assert_eq!(body["contents"][2]["parts"][0]["functionResponse"]["name"], "b");
    }

    #[test]
    fn text_and_a_call_come_out_in_order() {
        let events = vec![
            (None, "{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hel\"}]}}]}".into()),
            (None, "{\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"run_command\",\"args\":{\"command\":\"node.create\"}}}]}}]}".into()),
            (None, "{\"candidates\":[{\"content\":{\"parts\":[]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":3}}".into()),
        ];
        let (chunks, response) = parse_stream(&events).expect("the stream parses");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], Chunk::Text("Hel".into()));
        assert_eq!(
            chunks[1],
            Chunk::ToolCall(ToolCall { id: "call_0".into(), name: "run_command".into(), arguments: json!({"command": "node.create"}) })
        );
        assert_eq!(response.input_tokens, Some(10));
        assert_eq!(response.output_tokens, Some(3));
        assert_eq!(response.finish_reason.as_deref(), Some("STOP"));
    }

    #[test]
    fn two_calls_get_two_ids() {
        let events = vec![(
            None,
            "{\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"a\"}},{\"functionCall\":{\"name\":\"b\"}}]}}]}".into(),
        )];
        let (_, response) = parse_stream(&events).expect("the stream parses");
        assert_eq!(response.tool_calls[0].id, "call_0");
        assert_eq!(response.tool_calls[1].id, "call_1");
    }
}
