//! The ACP transport: an external agent CLI speaking JSON-RPC over its pipes.
//!
//! The app starts the command, sends `initialize` and `session/new`, then streams the prompt.
//! The agent's `session/update` notifications arrive as text chunks; a `session/request_permission`
//! is answered automatically (a programmatic chat cannot open a dialog). Everything a subprocess
//! writes is untrusted: a line over the cap ends the run, and a malformed JSON-RPC answer is an
//! error the chat shows, never a panic.
//!
//! The protocol itself — framing and the mapping of session updates into chunks — is pure and
//! tested here; the process runner is the only part that touches the OS.

use std::io::{BufRead, BufReader, Write as _};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use figcraft_ai::acp::PROTOCOL_VERSION;
use figcraft_ai::transport::{ChatRequest, ChatResponse, Chunk, TransportError};
use serde_json::{Value, json};

/// The largest line from the agent, in bytes.
const MAX_LINE: usize = 4 << 20;

/// One ACP session with a running process.
pub struct AcpSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    session_id: String,
    cwd: String,
}

impl AcpSession {
    /// Start the agent in `cwd` and open a session.
    pub fn start(command: &str, args: &[String], cwd: &str) -> Result<Self, TransportError> {
        let mut child =
            Command::new(command).args(args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(
                |e| {
                    let message = e.to_string();
                    let install =
                        if message.to_lowercase().contains("not found") { " Install it with the command the provider settings show." } else { "" };
                    TransportError::Other(format!("{command} could not start: {message}.{install}"))
                },
            )?;
        let stdin = child.stdin.take().ok_or_else(|| TransportError::Other("the agent's input pipe is missing".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| TransportError::Other("the agent's output pipe is missing".into()))?;
        let mut session = Self { child, stdin, stdout: BufReader::new(stdout), next_id: 0, session_id: String::new(), cwd: cwd.to_string() };
        session.initialize()?;
        Ok(session)
    }

    /// The handshake: `initialize`, then `session/new`.
    fn initialize(&mut self) -> Result<(), TransportError> {
        let answer = self.call("initialize", json!({"protocolVersion": PROTOCOL_VERSION, "clientCapabilities": {}}))?;
        let offered = answer.get("protocolVersion").and_then(Value::as_u64).unwrap_or(0);
        if offered != PROTOCOL_VERSION as u64 {
            return Err(TransportError::Other(format!("the agent speaks ACP {offered}; this build speaks {PROTOCOL_VERSION}")));
        }
        let created = self.call("session/new", json!({"cwd": self.cwd, "mcpServers": []}))?;
        self.session_id = created.get("sessionId").and_then(Value::as_str).unwrap_or_default().to_string();
        if self.session_id.is_empty() {
            return Err(TransportError::BadResponse("the agent opened no session".into()));
        }
        Ok(())
    }

    /// Send one JSON-RPC request and read its answer, handling the agent's requests along the way.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, TransportError> {
        self.next_id += 1;
        let id = self.next_id;
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|e| TransportError::Other(format!("pipe error: {e}")))?;
        loop {
            let message = self.read_message()?;
            if message.get("method").and_then(Value::as_str) == Some("session/request_permission") {
                // A chat is programmatic: there is no one at a dialog. Deny by default.
                let request_id = message.get("id").cloned().unwrap_or(Value::Null);
                let reply = json!({"jsonrpc": "2.0", "id": request_id, "result": {
                    "outcome": {"outcome": "reject", "reason": "programmatic chat"}
                }})
                .to_string();
                let _ = self.stdin.write_all(reply.as_bytes()).and_then(|_| self.stdin.write_all(b"\n"));
                continue;
            }
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = message.get("error") {
                    let text = error.get("message").and_then(Value::as_str).unwrap_or("the agent refused the request");
                    return Err(TransportError::Other(text.to_string()));
                }
                return message.get("result").cloned().ok_or_else(|| TransportError::BadResponse("the answer had no result".into()));
            }
        }
    }

    /// Read one JSON-RPC message from the agent, with a bounded line.
    fn read_message(&mut self) -> Result<Value, TransportError> {
        let mut line = String::new();
        let bytes = self.stdout.read_line(&mut line).map_err(|e| TransportError::Other(format!("pipe error: {e}")))?;
        if bytes == 0 {
            return Err(TransportError::Other("the agent exited".into()));
        }
        if bytes > MAX_LINE {
            return Err(TransportError::BadResponse("the agent wrote a line that was too long".into()));
        }
        serde_json::from_str(&line).map_err(|e| TransportError::BadResponse(format!("the agent wrote invalid JSON-RPC: {e}")))
    }

    /// Send a prompt and collect its streamed updates into an answer.
    pub fn prompt(&mut self, text: &str, on_chunk: &mut dyn FnMut(Chunk)) -> Result<ChatResponse, TransportError> {
        let mut response = ChatResponse::default();
        self.next_id += 1;
        let id = self.next_id;
        let line = json!({"jsonrpc": "2.0", "id": id, "method": "session/prompt", "params": {
            "sessionId": self.session_id,
            "prompt": [{"type": "text", "text": text}],
        }})
        .to_string();
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|e| TransportError::Other(format!("pipe error: {e}")))?;
        loop {
            let message = self.read_message()?;
            if message.get("method").and_then(Value::as_str) == Some("session/update") {
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                let chunks = map_update(&params);
                for chunk in chunks {
                    if let Chunk::Text(ref t) = chunk {
                        response.text.push_str(t);
                    }
                    on_chunk(chunk);
                }
                continue;
            }
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = message.get("error") {
                    let text = error.get("message").and_then(Value::as_str).unwrap_or("the prompt was refused");
                    return Err(TransportError::Other(text.to_string()));
                }
                response.finish_reason = Some("stop".into());
                return Ok(response);
            }
        }
    }
}

impl Drop for AcpSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Map one `session/update` notification's params into chat chunks.
///
/// The agent's updates are a tagged `sessionUpdate` field: `agent_message_chunk` is the answer's
/// text, `agent_thought_chunk` its reasoning, `tool_call` a tool it is using, and `tool_call_update`
/// its result. Anything unknown is ignored, not guessed at.
pub fn map_update(params: &Value) -> Vec<Chunk> {
    let update =
        params.get("sessionUpdate").or_else(|| params.get("update").and_then(|u| u.get("sessionUpdate"))).and_then(Value::as_str).unwrap_or_default();
    let content = params.pointer("/content").or_else(|| params.pointer("/update/content")).cloned().unwrap_or(Value::Null);
    let text = content.get("text").and_then(Value::as_str).unwrap_or_default();
    match update {
        "agent_message_chunk" if !text.is_empty() => vec![Chunk::Text(text.to_string())],
        "agent_thought_chunk" if !text.is_empty() => vec![Chunk::Reasoning(text.to_string())],
        "tool_call" | "tool_call_update" => {
            let name = params.pointer("/title").or_else(|| params.pointer("/update/title")).and_then(Value::as_str).unwrap_or("tool");
            let status = params.pointer("/status").or_else(|| params.pointer("/update/status")).and_then(Value::as_str).unwrap_or("pending");
            vec![Chunk::Text(format!("\n[{} — {}]\n", name, status))]
        }
        _ => Vec::new(),
    }
}

/// The prompt an ACP session receives: the system instructions and the user's message. ACP has
/// no separate system channel, so they travel together.
pub fn prompt_text(request: &ChatRequest) -> String {
    let last = request.messages.iter().rev().find(|m| m.role == figcraft_ai::transport::Role::User).map(|m| m.content.as_str()).unwrap_or_default();
    if request.system.trim().is_empty() { last.to_string() } else { format!("{}\n\n{}", request.system.trim(), last) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use figcraft_ai::providers::Protocol;
    use serde_json::json;

    #[test]
    fn text_thought_and_tool_updates_map_to_chunks() {
        assert_eq!(
            map_update(&json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "hi"}})),
            vec![Chunk::Text("hi".into())]
        );
        assert_eq!(
            map_update(&json!({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": "hmm"}})),
            vec![Chunk::Reasoning("hmm".into())]
        );
        let tool = map_update(&json!({"sessionUpdate": "tool_call", "title": "read", "status": "in_progress"}));
        assert_eq!(tool.len(), 1);
        assert!(matches!(tool[0], Chunk::Text(ref s) if s.contains("read")));
    }

    #[test]
    fn an_unknown_update_is_ignored() {
        assert!(map_update(&json!({"sessionUpdate": "plan", "content": {"text": "x"}})).is_empty());
        assert!(map_update(&json!({})).is_empty());
    }

    #[test]
    fn nested_updates_are_read_too() {
        let nested = json!({"update": {"sessionUpdate": "agent_message_chunk", "content": {"text": "nested"}}});
        assert_eq!(map_update(&nested), vec![Chunk::Text("nested".into())]);
    }

    #[test]
    fn the_prompt_carries_the_system_instructions() {
        let request = ChatRequest {
            model: "claude-code".into(),
            base_url: String::new(),
            api_key: String::new(),
            protocol: Protocol::Openai,
            responses_api: false,
            messages: vec![figcraft_ai::transport::ChatMessage::user("make a frame")],
            system: "you are a designer".into(),
            tools: vec![],
            max_output_tokens: 0,
            reasoning_effort: None,
        };
        let text = prompt_text(&request);
        assert!(text.contains("you are a designer"));
        assert!(text.contains("make a frame"));
    }
}
