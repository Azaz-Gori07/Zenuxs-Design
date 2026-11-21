//! The Anthropic wire protocol: the messages request and its streamed answer.
//!
//! `POST {base}/messages` answers as named SSE events: `message_start` carries the input
//! usage, `content_block_start` opens a text, thinking or tool-use block, `content_block_delta`
//! fills it (`text_delta`, `thinking_delta`, `input_json_delta`), and `message_delta` carries
//! the stop reason and the output usage.
//!
//! Blocks are told apart by index, so two tool calls in one answer stay separate.

use figcraft_ai::transport::{ChatMessage, ChatRequest, ChatResponse, Chunk, Role, Tool, ToolCall};
use serde_json::{Value, json};

pub type ParseError = String;

/// Build the request body.
pub fn request_body(request: &ChatRequest) -> Value {
    let (mut system, messages) = system_and_messages(&request.messages);
    if !request.system.trim().is_empty() {
        system.push(json!({"type": "text", "text": request.system.trim()}));
    }
    let mut body = json!({
        "model": request.model,
        "max_tokens": if request.max_output_tokens > 0 { request.max_output_tokens } else { 4096 },
        "stream": true,
    });
    if !system.is_empty() {
        body["system"] = json!(system);
    }
    body["messages"] = json!(messages);
    if let Some(effort) = &request.reasoning_effort
        && !effort.trim().is_empty()
    {
        body["reasoning"] = json!({"type": "enabled", "budget_tokens": budget_for(effort.trim(), request.max_output_tokens)});
    }
    if !request.tools.is_empty() {
        body["tools"] = json!(request.tools.iter().map(tool_json).collect::<Vec<_>>());
    }
    body
}

/// The reasoning budget: the effort's word turned into tokens, never above the output limit and
/// never so small the model cannot think at all.
fn budget_for(effort: &str, max_output_tokens: u32) -> u32 {
    let share = match effort {
        "low" | "minimal" => 0.15,
        "medium" => 0.4,
        "high" | "xhigh" => 0.8,
        _ => 0.4,
    };
    let limit = if max_output_tokens > 0 { max_output_tokens } else { 4096 };
    let budget = (limit as f64 * share).round() as u32;
    budget.clamp(1024, limit.max(1024))
}

/// One tool, as the protocol names it.
fn tool_json(tool: &Tool) -> Value {
    json!({"name": tool.name, "description": tool.description, "input_schema": tool.input_schema})
}

/// The system text and the conversation, in the protocol's own shapes.
///
/// The system text is its own field here, not a message.
fn system_and_messages(messages: &[ChatMessage]) -> (Vec<Value>, Vec<Value>) {
    let system = Vec::new();
    let mut out = Vec::new();
    for message in messages {
        match message.role {
            Role::User => out.push(json!({"role": "user", "content": [{"type": "text", "text": message.content}]})),
            Role::Assistant if message.tool_calls.is_empty() => {
                out.push(json!({"role": "assistant", "content": [{"type": "text", "text": message.content}]}))
            }
            Role::Assistant => {
                let mut content = Vec::new();
                if !message.content.is_empty() {
                    content.push(json!({"type": "text", "text": message.content}));
                }
                for call in &message.tool_calls {
                    content.push(json!({"type": "tool_use", "id": call.id, "name": call.name, "input": call.arguments}));
                }
                out.push(json!({"role": "assistant", "content": content}));
            }
            // A tool answer is a user turn carrying one tool_result per call.
            Role::Tool => out.push(json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": message.tool_call_id.clone().unwrap_or_default(), "content": message.content}]})),
        }
    }
    (system, out)
}

/// Parse one stream of events into chunks, and the answer that comes out of them.
pub fn parse_stream(events: &[(Option<String>, String)]) -> Result<(Vec<Chunk>, ChatResponse), ParseError> {
    let mut chunks = Vec::new();
    let mut response = ChatResponse::default();
    // One entry per open block, by index: text blocks stream text, thinking blocks stream
    // reasoning, and tool blocks assemble arguments from input_json_delta pieces.
    let mut blocks: Vec<(usize, Block)> = Vec::new();

    for (kind, payload) in events {
        let event: Value = serde_json::from_str(payload).map_err(|e| format!("a stream event was not JSON: {e}"))?;
        match kind.as_deref().unwrap_or("") {
            "message_start" => {
                let usage = event.pointer("/message/usage");
                response.input_tokens = usage.and_then(|u| u.get("input_tokens")).and_then(Value::as_u64).or(response.input_tokens);
                response.cache_read_tokens =
                    usage.and_then(|u| u.get("cache_read_input_tokens")).and_then(Value::as_u64).or(response.cache_read_tokens);
                response.cache_write_tokens =
                    usage.and_then(|u| u.get("cache_creation_input_tokens")).and_then(Value::as_u64).or(response.cache_write_tokens);
            }
            "content_block_start" => {
                let index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(block) = event.pointer("/content_block/type").and_then(Value::as_str) {
                    let entry = match block {
                        "tool_use" => {
                            let id = event.pointer("/content_block/id").and_then(Value::as_str).unwrap_or_default().to_string();
                            let name = event.pointer("/content_block/name").and_then(Value::as_str).unwrap_or_default().to_string();
                            Block::Tool { id, name, arguments: String::new(), started: true }
                        }
                        _ => Block::Other,
                    };
                    blocks.push((index, entry));
                }
            }
            "content_block_delta" => {
                let index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let Some((_, block)) = blocks.iter_mut().find(|(i, _)| *i == index) else { continue };
                let kind = event.pointer("/delta/type").and_then(Value::as_str).unwrap_or("");
                match (block, kind) {
                    (Block::Other, "text_delta") => {
                        if let Some(text) = event.pointer("/delta/text").and_then(Value::as_str) {
                            chunks.push(Chunk::Text(text.to_string()));
                            response.text.push_str(text);
                        }
                    }
                    (Block::Other, "thinking_delta") => {
                        if let Some(text) = event.pointer("/delta/thinking").and_then(Value::as_str) {
                            chunks.push(Chunk::Reasoning(text.to_string()));
                        }
                    }
                    (Block::Tool { arguments, .. }, "input_json_delta") => {
                        if let Some(text) = event.pointer("/delta/partial_json").and_then(Value::as_str) {
                            arguments.push_str(text);
                        }
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(reason) = event.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    response.finish_reason = Some(reason.to_string());
                }
                if let Some(tokens) = event.pointer("/usage/output_tokens").and_then(Value::as_u64) {
                    response.output_tokens = Some(tokens);
                }
            }
            _ => {}
        }
    }

    // The tool calls, in the order their blocks were opened.
    for (_, block) in blocks {
        let Block::Tool { id, name, arguments, started } = block else { continue };
        if !started || name.is_empty() {
            continue;
        }
        let arguments: Value = if arguments.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&arguments).map_err(|e| format!("the arguments to {name} were not JSON: {e}"))?
        };
        let finished = ToolCall { id, name, arguments };
        chunks.push(Chunk::ToolCall(finished.clone()));
        response.tool_calls.push(finished);
    }
    Ok((chunks, response))
}

/// One open block of the answer.
enum Block {
    Tool { id: String, name: String, arguments: String, started: bool },
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage::user("hello")],
            system: "be brief".into(),
            tools: vec![Tool { name: "run_command".into(), description: "Run a command.".into(), input_schema: json!({"type": "object"}) }],
            model: "claude-sonnet-5".into(),
            base_url: "https://api.example.com/v1".into(),
            api_key: "sk-test".into(),
            protocol: figcraft_ai::providers::Protocol::Openai,
            responses_api: false,
            max_output_tokens: 4096,
            reasoning_effort: Some("high".into()),
        }
    }

    #[test]
    fn the_request_sends_the_system_as_its_own_field() {
        let body = request_body(&request());
        assert_eq!(body["system"][0]["text"], "be brief");
        assert_eq!(body["system"][0]["type"], "text");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["tools"][0]["name"], "run_command");
    }

    #[test]
    fn the_reasoning_budget_is_a_share_of_the_output_limit() {
        assert_eq!(budget_for("low", 4096), 1024);
        assert_eq!(budget_for("high", 4096), 3277);
        // Never above the limit itself.
        assert!(budget_for("xhigh", 1024) <= 1024);
    }

    #[test]
    fn a_tool_call_and_its_answer_round_trip() {
        let mut r = request();
        r.messages.push(ChatMessage::assistant_calls(
            "",
            vec![ToolCall { id: "t1".into(), name: "run_command".into(), arguments: json!({"command": "node.create"}) }],
        ));
        r.messages.push(ChatMessage::tool_result("t1", "{\"ok\":true}"));
        let body = request_body(&r);
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(body["messages"][2]["role"], "user");
    }

    #[test]
    fn text_reasoning_and_a_tool_come_out_in_order() {
        let events = vec![
            (Some("message_start".into()), "{\"message\":{\"usage\":{\"input_tokens\":10}}}".into()),
            (Some("content_block_start".into()), "{\"index\":0,\"content_block\":{\"type\":\"text\"}}".into()),
            (Some("content_block_delta".into()), "{\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}".into()),
            (Some("content_block_start".into()), "{\"index\":1,\"content_block\":{\"type\":\"thinking\"}}".into()),
            (Some("content_block_delta".into()), "{\"index\":1,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"hmm\"}}".into()),
            (
                Some("content_block_start".into()),
                "{\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"run_command\"}}".into(),
            ),
            (
                Some("content_block_delta".into()),
                "{\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"command\\\":\"}}".into(),
            ),
            (
                Some("content_block_delta".into()),
                "{\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"node.create\\\"}\"}}".into(),
            ),
            (Some("message_delta".into()), "{\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":25}}".into()),
        ];
        let (chunks, response) = parse_stream(&events).expect("the stream parses");
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], Chunk::Text("Hel".into()));
        assert_eq!(chunks[1], Chunk::Reasoning("hmm".into()));
        assert_eq!(
            chunks[2],
            Chunk::ToolCall(ToolCall { id: "t1".into(), name: "run_command".into(), arguments: json!({"command": "node.create"}) })
        );
        assert_eq!(response.input_tokens, Some(10));
        assert_eq!(response.output_tokens, Some(25));
        assert_eq!(response.finish_reason.as_deref(), Some("tool_use"));
    }

    #[test]
    fn two_tool_calls_stay_separate() {
        let events = vec![
            (Some("content_block_start".into()), "{\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"a\"}}".into()),
            (Some("content_block_start".into()), "{\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t2\",\"name\":\"b\"}}".into()),
            (Some("content_block_delta".into()), "{\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}".into()),
        ];
        let (_, response) = parse_stream(&events).expect("the stream parses");
        assert_eq!(response.tool_calls.len(), 2);
        assert_eq!(response.tool_calls[1].name, "b");
    }

    #[test]
    fn arguments_that_are_not_json_are_an_error() {
        let events = vec![
            (Some("content_block_start".into()), "{\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"a\"}}".into()),
            (Some("content_block_delta".into()), "{\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"nope\"}}".into()),
        ];
        let err = parse_stream(&events).expect_err("bad arguments are refused");
        assert!(err.contains("a"), "{err}");
    }
}
