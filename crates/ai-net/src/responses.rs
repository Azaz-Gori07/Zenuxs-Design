//! The OpenAI Responses API: the other shape a compatible endpoint may speak.
//!
//! `POST {base}/responses` with `stream: true` answers as named events: `response.output_text.delta`
//! fills the text, `response.reasoning_*` the reasoning, `response.output_item.added` opens a
//! function call whose arguments arrive through `response.function_call_arguments.delta`, and
//! `response.completed` carries the usage.
//!
//! A compatible provider's `customAPIType` chooses between this and [`super::openai`].

use figcraft_ai::transport::{ChatMessage, ChatRequest, ChatResponse, Chunk, Role, Tool, ToolCall};
use serde_json::{Value, json};

pub type ParseError = String;

/// Build the request body.
pub fn request_body(request: &ChatRequest) -> Value {
    let mut body = json!({
        "model": request.model,
        "input": input(&request.messages),
        "stream": true,
    });
    if !request.system.is_empty() {
        body["instructions"] = json!(request.system);
    }
    if request.max_output_tokens > 0 {
        body["max_output_tokens"] = json!(request.max_output_tokens);
    }
    if let Some(effort) = &request.reasoning_effort
        && !effort.trim().is_empty()
    {
        body["reasoning"] = json!({"effort": effort.trim()});
    }
    if !request.tools.is_empty() {
        body["tools"] = json!(request.tools.iter().map(tool_json).collect::<Vec<_>>());
    }
    body
}

/// One tool, as the protocol names it.
fn tool_json(tool: &Tool) -> Value {
    json!({"type": "function", "name": tool.name, "description": tool.description, "parameters": tool.input_schema})
}

/// The conversation, in the protocol's own input shapes.
fn input(messages: &[ChatMessage]) -> Vec<Value> {
    let mut out = Vec::new();
    for message in messages {
        match message.role {
            Role::User => out.push(json!({"role": "user", "content": [{"type": "input_text", "text": message.content}]})),
            Role::Assistant if message.tool_calls.is_empty() => {
                out.push(json!({"role": "assistant", "content": [{"type": "output_text", "text": message.content}]}))
            }
            Role::Assistant => {
                if !message.content.is_empty() {
                    out.push(json!({"role": "assistant", "content": [{"type": "output_text", "text": message.content}]}));
                }
                for call in &message.tool_calls {
                    out.push(json!({"type": "function_call", "call_id": call.id, "name": call.name, "arguments": call.arguments.to_string()}));
                }
            }
            Role::Tool => out.push(
                json!({"type": "function_call_output", "call_id": message.tool_call_id.clone().unwrap_or_default(), "output": message.content}),
            ),
        }
    }
    out
}

/// Parse one stream of events into chunks, and the answer that comes out of them.
pub fn parse_stream(events: &[(Option<String>, String)]) -> Result<(Vec<Chunk>, ChatResponse), ParseError> {
    let mut chunks = Vec::new();
    let mut response = ChatResponse::default();
    // Arguments stream per item id; the item's name came with the added event.
    let mut calls: Vec<(String, String, String)> = Vec::new();

    for (kind, payload) in events {
        let event: Value = serde_json::from_str(payload).map_err(|e| format!("a stream event was not JSON: {e}"))?;
        match kind.as_deref().unwrap_or("") {
            "response.output_item.added" => {
                if event.pointer("/item/type").and_then(Value::as_str) == Some("function_call") {
                    let id = event.pointer("/item/call_id").and_then(Value::as_str).unwrap_or_default().to_string();
                    let name = event.pointer("/item/name").and_then(Value::as_str).unwrap_or_default().to_string();
                    calls.push((id, name, String::new()));
                }
            }
            "response.function_call_arguments.delta" => {
                let id = event.get("item_id").and_then(Value::as_str).unwrap_or_default();
                if let Some(delta) = event.get("delta").and_then(Value::as_str)
                    && let Some((_, _, arguments)) = calls.iter_mut().find(|(call, _, _)| *call == id)
                {
                    arguments.push_str(delta);
                }
            }
            "response.output_text.delta" => {
                if let Some(text) = event.get("delta").and_then(Value::as_str)
                    && !text.is_empty()
                {
                    chunks.push(Chunk::Text(text.to_string()));
                    response.text.push_str(text);
                }
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                if let Some(text) = event.get("delta").and_then(Value::as_str)
                    && !text.is_empty()
                {
                    chunks.push(Chunk::Reasoning(text.to_string()));
                }
            }
            "response.completed" => {
                let usage = event.pointer("/response/usage");
                response.input_tokens = usage.and_then(|u| u.get("input_tokens")).and_then(Value::as_u64).or(response.input_tokens);
                response.output_tokens = usage.and_then(|u| u.get("output_tokens")).and_then(Value::as_u64).or(response.output_tokens);
                response.cache_read_tokens =
                    usage.and_then(|u| u.pointer("/input_tokens_details/cached_tokens")).and_then(Value::as_u64).or(response.cache_read_tokens);
                response.finish_reason =
                    event.pointer("/response/status").and_then(Value::as_str).map(str::to_string).or(response.finish_reason.clone());
            }
            _ => {}
        }
    }

    for (id, name, arguments) in calls {
        if name.is_empty() {
            continue;
        }
        let arguments: Value = if arguments.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&arguments).map_err(|e| format!("the arguments to {name} were not JSON: {e}"))?
        };
        let finished = ToolCall { id, name: name.clone(), arguments };
        chunks.push(Chunk::ToolCall(finished.clone()));
        response.tool_calls.push(finished);
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
    fn the_request_sends_instructions_and_the_tools() {
        let body = request_body(&request());
        assert_eq!(body["instructions"], "be brief");
        assert_eq!(body["model"], "gpt-5.6");
        assert_eq!(body["max_output_tokens"], 512);
        assert_eq!(body["reasoning"]["effort"], "low");
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["tools"][0]["name"], "run_command");
    }

    #[test]
    fn a_call_and_its_answer_round_trip() {
        let mut r = request();
        r.messages.push(ChatMessage::assistant_calls(
            "",
            vec![ToolCall { id: "c1".into(), name: "run_command".into(), arguments: json!({"command": "node.create"}) }],
        ));
        r.messages.push(ChatMessage::tool_result("c1", "{\"ok\":true}"));
        let body = request_body(&r);
        assert_eq!(body["input"][1]["type"], "function_call");
        assert_eq!(body["input"][2]["type"], "function_call_output");
        assert_eq!(body["input"][2]["call_id"], "c1");
    }

    #[test]
    fn text_reasoning_and_a_call_come_out_in_order() {
        // The payloads are built with json! so an argument piece's own quotes stay straight.
        let event = |kind: &str, value: Value| (Some(kind.to_string()), value.to_string());
        let events = vec![
            event("response.output_item.added", json!({"item": {"type": "function_call", "call_id": "c1", "name": "run_command"}})),
            event("response.reasoning_summary_text.delta", json!({"delta": "hmm"})),
            event("response.output_text.delta", json!({"delta": "Hel"})),
            event("response.output_text.delta", json!({"delta": "lo"})),
            event("response.function_call_arguments.delta", json!({"item_id": "c1", "delta": "{\"command\""})),
            event("response.function_call_arguments.delta", json!({"item_id": "c1", "delta": ": \"node.create\"}"})),
            event(
                "response.completed",
                json!({"response": {"usage": {"input_tokens": 10, "output_tokens": 3, "input_tokens_details": {"cached_tokens": 8}}, "status": "completed"}}),
            ),
        ];
        let (chunks, response) = parse_stream(&events).expect("the stream parses");
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0], Chunk::Reasoning("hmm".into()));
        assert_eq!(chunks[1], Chunk::Text("Hel".into()));
        assert_eq!(response.text, "Hello");
        assert_eq!(response.tool_calls[0].arguments["command"], "node.create");
        assert_eq!(response.cache_read_tokens, Some(8));
        assert_eq!(response.finish_reason.as_deref(), Some("completed"));
    }

    #[test]
    fn arguments_that_are_not_json_are_an_error() {
        let events = vec![
            (Some("response.output_item.added".into()), "{\"item\":{\"type\":\"function_call\",\"call_id\":\"c1\",\"name\":\"a\"}}".into()),
            (Some("response.function_call_arguments.delta".into()), "{\"item_id\":\"c1\",\"delta\":\"nope\"}".into()),
        ];
        let err = parse_stream(&events).expect_err("bad arguments are refused");
        assert!(err.contains("a"), "{err}");
    }
}
