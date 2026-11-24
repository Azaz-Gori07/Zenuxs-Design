//! The OpenAI wire protocol: the chat-completions request and its streamed answer.
//!
//! `POST {base}/chat/completions` with `stream: true` answers as unnamed SSE events whose data
//! is a chunk with a `choices` list; each chunk's `delta` carries the text, the reasoning and
//! the tool calls as they are assembled. `data: [DONE]` ends the stream (the SSE reader stops
//! on it before this parser is called).
//!
//! The Responses API (`customAPIType: responses`) is the other shape the compatible providers
//! speak; it is parsed in [`super::responses`].

use figcraft_ai::transport::{ChatMessage, ChatRequest, ChatResponse, Chunk, Role, Tool, ToolCall};
use serde_json::{Value, json};

/// Why a stream could not be read.
pub type ParseError = String;

/// Build the request body.
pub fn request_body(request: &ChatRequest) -> Value {
    let mut msgs = Vec::new();
    if !request.system.trim().is_empty() {
        msgs.push(json!({"role": "system", "content": request.system.trim()}));
    }
    msgs.extend(messages(&request.messages));
    let mut body = json!({
        "model": request.model,
        "messages": msgs,
        "stream": true,
        "stream_options": {"include_usage": true},
    });
    if request.max_output_tokens > 0 {
        body["max_tokens"] = json!(request.max_output_tokens);
    }
    if let Some(effort) = &request.reasoning_effort
        && !effort.trim().is_empty()
    {
        body["reasoning_effort"] = json!(effort.trim());
    }
    if !request.tools.is_empty() {
        body["tools"] = json!(request.tools.iter().map(tool_json).collect::<Vec<_>>());
    }
    body
}

/// One tool, as the protocol names it.
fn tool_json(tool: &Tool) -> Value {
    json!({"type": "function", "function": {"name": tool.name, "description": tool.description, "parameters": tool.input_schema}})
}

/// The conversation, in the protocol's own message shapes.
fn messages(messages: &[ChatMessage]) -> Vec<Value> {
    let mut out = Vec::new();
    for message in messages {
        match message.role {
            Role::User if !message.images.is_empty() => {
                let mut parts = Vec::new();
                if !message.content.is_empty() {
                    parts.push(json!({"type": "text", "text": message.content}));
                }
                for img in &message.images {
                    let url = if img.starts_with("data:") || img.starts_with("http:") || img.starts_with("https:") {
                        img.clone()
                    } else {
                        format!("data:image/png;base64,{img}")
                    };
                    parts.push(json!({"type": "image_url", "image_url": {"url": url}}));
                }
                out.push(json!({"role": "user", "content": parts}));
            }
            Role::User => out.push(json!({"role": "user", "content": message.content})),
            Role::Assistant if message.tool_calls.is_empty() => out.push(json!({"role": "assistant", "content": message.content})),
            Role::Assistant => {
                let mut m = json!({"role": "assistant"});
                if !message.content.is_empty() {
                    m["content"] = json!(message.content);
                }
                m["tool_calls"] = json!(
                    message
                        .tool_calls
                        .iter()
                        .map(|c| json!({
                            "id": c.id, "type": "function",
                            "function": {"name": c.name, "arguments": c.arguments.to_string()},
                        }))
                        .collect::<Vec<_>>()
                );
                out.push(m);
            }
            Role::Tool => out.push(json!({
                "role": "tool",
                "tool_call_id": message.tool_call_id.clone().unwrap_or_default(),
                "content": message.content,
            })),
        }
    }
    out
}

/// Parse one stream of data payloads into chunks, and the answer that comes out of them.
///
/// A tool call arrives as pieces (`index`, a name, argument text) that are assembled here; its
/// arguments are JSON, and a piece that never made valid JSON is an error rather than a call
/// with a truncated name.
pub fn parse_stream(data: &[String]) -> Result<(Vec<Chunk>, ChatResponse), ParseError> {
    let mut chunks = Vec::new();
    let mut response = ChatResponse::default();
    let mut calls: Vec<Assembling> = Vec::new();

    for payload in data {
        let chunk: Value = serde_json::from_str(payload).map_err(|e| format!("a stream chunk was not JSON: {e}"))?;
        if let Some(err) = chunk.get("error") {
            let msg = err.get("message").and_then(Value::as_str).unwrap_or("unknown provider error");
            return Err(format!("provider error: {msg}"));
        }
        if let Some(usage) = chunk.get("usage").filter(|u| !u.is_null()) {
            response.input_tokens = usage.get("prompt_tokens").and_then(Value::as_u64).or(response.input_tokens);
            response.output_tokens = usage.get("completion_tokens").and_then(Value::as_u64).or(response.output_tokens);
            if let Some(details) = usage.get("prompt_tokens_details") {
                response.cache_read_tokens = details.get("cached_tokens").and_then(Value::as_u64).or(response.cache_read_tokens);
            }
        }
        let Some(choice) = chunk.get("choices").and_then(Value::as_array).and_then(|c| c.first()) else { continue };
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            response.finish_reason = Some(reason.to_string());
        }
        let Some(delta) = choice.get("delta") else { continue };
        if let Some(text) = delta.get("content").and_then(Value::as_str)
            && !text.is_empty()
        {
            chunks.push(Chunk::Text(text.to_string()));
            response.text.push_str(text);
        }
        if let Some(reasoning) = delta.get("reasoning_content").or_else(|| delta.get("reasoning")).and_then(Value::as_str)
            && !reasoning.is_empty()
        {
            chunks.push(Chunk::Reasoning(reasoning.to_string()));
        }
        if let Some(list) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in list {
                let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                while calls.len() <= index {
                    calls.push(Assembling::default());
                }
                let entry = &mut calls[index];
                if let Some(id) = call.get("id").and_then(Value::as_str) {
                    entry.id = Some(id.to_string());
                }
                if let Some(name) = call.get("function").and_then(|f| f.get("name")).and_then(Value::as_str) {
                    entry.name.push_str(name);
                }
                if let Some(args) = call.get("function").and_then(|f| f.get("arguments")).and_then(Value::as_str) {
                    entry.arguments.push_str(args);
                }
            }
        }
    }

    for (index, call) in calls.into_iter().enumerate() {
        let id = call.id.clone().unwrap_or_else(|| format!("call_{index}"));
        let arguments: Value = if call.arguments.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&call.arguments).map_err(|e| format!("the arguments to {id} were not JSON: {e}"))?
        };
        let finished = ToolCall { id, name: call.name.clone(), arguments };
        chunks.push(Chunk::ToolCall(finished.clone()));
        response.tool_calls.push(finished);
    }
    Ok((chunks, response))
}

/// One tool call arriving in pieces.
#[derive(Default)]
struct Assembling {
    id: Option<String>,
    name: String,
    arguments: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage::user("hello")],
            system: "be brief".into(),
            tools: vec![Tool { name: "run_command".into(), description: "Run a command.".into(), input_schema: json!({"type": "object"}) }],
            model: "gpt-5.6".into(),
            base_url: "https://api.example.com/v1".into(),
            api_key: "sk-test".into(),
            protocol: figcraft_ai::providers::Protocol::Openai,
            responses_api: false,
            max_output_tokens: 512,
            reasoning_effort: Some("low".into()),
        }
    }

    #[test]
    fn the_request_names_the_model_and_the_tools() {
        let body = request_body(&request());
        assert_eq!(body["model"], "gpt-5.6");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 512);
        assert_eq!(body["reasoning_effort"], "low");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "be brief");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["tools"][0]["function"]["name"], "run_command");
    }

    #[test]
    fn a_tool_call_and_its_answer_round_trip() {
        let mut r = request();
        r.messages.push(ChatMessage::assistant_calls(
            "",
            vec![ToolCall { id: "c1".into(), name: "run_command".into(), arguments: json!({"command": "node.create"}) }],
        ));
        r.messages.push(ChatMessage::tool_result("c1", "{\"ok\":true}"));
        let body = request_body(&r);
        assert_eq!(body["messages"][2]["tool_calls"][0]["id"], "c1");
        assert_eq!(body["messages"][3]["tool_call_id"], "c1");
        assert_eq!(body["messages"][3]["role"], "tool");
    }

    #[test]
    fn text_and_reasoning_come_out_in_order() {
        let data = vec![
            "{\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}".into(),
            "{\"choices\":[{\"delta\":{\"reasoning_content\":\"hmm\"}}]}".into(),
            "{\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}".into(),
            "{\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":3}}".into(),
        ];
        let (chunks, response) = parse_stream(&data).expect("the stream parses");
        assert_eq!(chunks, vec![Chunk::Text("Hel".into()), Chunk::Reasoning("hmm".into()), Chunk::Text("lo".into())]);
        assert_eq!(response.text, "Hello");
        assert_eq!(response.input_tokens, Some(10));
        assert_eq!(response.output_tokens, Some(3));
        assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn a_tool_call_arriving_in_pieces_is_assembled() {
        let data = vec![
            "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"run_\"}}]}}]}".into(),
            "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"command\",\"arguments\":\"{\\\"comm\"}}]}}]}".into(),
            "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"and\\\":\\\"node.create\\\"}\"}}]}}]}".into(),
        ];
        let (chunks, response) = parse_stream(&data).expect("the stream parses");
        assert_eq!(chunks.len(), 1);
        assert_eq!(response.tool_calls[0].id, "c1");
        assert_eq!(response.tool_calls[0].name, "run_command");
        assert_eq!(response.tool_calls[0].arguments["command"], "node.create");
    }

    #[test]
    fn a_call_without_an_id_gets_one_from_its_index() {
        let data = vec!["{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"x\",\"arguments\":\"{}\"}}]}}]}".into()];
        let (_, response) = parse_stream(&data).expect("the stream parses");
        assert_eq!(response.tool_calls[0].id, "call_0");
    }

    #[test]
    fn two_calls_in_one_answer_are_kept_apart() {
        let data = vec![
            "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"a\",\"arguments\":\"{}\"}},{\"index\":1,\"id\":\"c2\",\"function\":{\"name\":\"b\",\"arguments\":\"{}\"}}]}}]}".into(),
        ];
        let (_, response) = parse_stream(&data).expect("the stream parses");
        assert_eq!(response.tool_calls.len(), 2);
        assert_eq!(response.tool_calls[1].name, "b");
    }

    #[test]
    fn arguments_that_are_not_json_are_an_error() {
        let data = vec![
            "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"a\",\"arguments\":\"nope\"}}]}}]}".into(),
        ];
        let err = parse_stream(&data).expect_err("bad arguments are refused");
        assert!(err.contains("c1"), "{err}");
    }

    #[test]
    fn cached_tokens_come_through_as_cache_read() {
        let data = vec![
            "{\"choices\":[],\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":1,\"prompt_tokens_details\":{\"cached_tokens\":80}}}".into(),
        ];
        let (_, response) = parse_stream(&data).expect("the stream parses");
        assert_eq!(response.cache_read_tokens, Some(80));
    }

    #[test]
    fn a_user_message_with_images_formats_as_multimodal() {
        let mut r = request();
        r.messages = vec![ChatMessage::user_with_images(
            "Check this design",
            vec!["data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".into()],
        )];
        let body = request_body(&r);
        assert_eq!(body["messages"][0]["role"], "system");
        let content = &body["messages"][1]["content"];
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Check this design");
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(content[1]["image_url"]["url"], "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==");
    }
}
