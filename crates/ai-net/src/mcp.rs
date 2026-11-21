//! The MCP client: what speaks to an external MCP server.
//!
//! Streamable HTTP: a `POST` carries one JSON-RPC request and the answer comes back as JSON (or
//! as an event stream, read with the same SSE reader the providers use). A session opens with
//! `initialize`, the server's session id comes back in a header, and every later request
//! carries it. `tools/list` names the tools, `tools/call` runs one.
//!
//! Everything a server says is untrusted: a tool's schema is passed to the model as it arrived,
//! and a call's result is read as text blocks, bounded.

use figcraft_ai::transport::{Tool, TransportError};
use serde_json::{Value, json};

use crate::sse;

/// The protocol version this client speaks, and the ones it accepts.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// The most a server's answer may be.
const MAX_REPLY: usize = 4 << 20;

use figcraft_ai::mcp::McpSession;

/// One server, as the client sees it.
pub struct Server {
    /// The connection's URL (`https://host/mcp`).
    pub url: String,
    /// The bearer token, when the server wants one.
    pub bearer: Option<String>,
}

/// What a server offers and how it is reached.
pub struct Client {
    server: Server,
    /// The session id the server gave at `initialize`.
    session: Option<String>,
}

impl McpSession for Client {
    fn initialize(&mut self) -> Result<(), TransportError> {
        self.initialize_session()
    }

    fn tools(&mut self) -> Result<Vec<Tool>, TransportError> {
        self.list_tools()
    }

    fn call(&mut self, name: &str, arguments: &Value) -> Result<String, TransportError> {
        self.call_tool(name, arguments)
    }
}

impl Client {
    /// A client for one server.
    pub fn new(server: Server) -> Self {
        Self { server, session: None }
    }

    /// Open the session: `initialize`, then the handshake the protocol asks for.
    fn initialize_session(&mut self) -> Result<(), TransportError> {
        let answer = self.call_rpc(
            "initialize",
            Some(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "figcraft", "version": figcraft_version()},
            })),
        )?;
        // The server's own version may be one this client also speaks; anything else is
        // refused now rather than answered with misread tools later.
        let offered = answer.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
        if !offered.is_empty() && offered != PROTOCOL_VERSION {
            return Err(TransportError::Other(format!("the server speaks MCP {offered}; this build speaks {PROTOCOL_VERSION}")));
        }
        self.session = Some(answer.get("sessionId").and_then(Value::as_str).unwrap_or_default().to_string());
        // The protocol's handshake: the client says it is done initializing.
        let _ = self.call_rpc("notifications/initialized", None);
        Ok(())
    }

    /// The tools the server offers.
    fn list_tools(&mut self) -> Result<Vec<Tool>, TransportError> {
        let answer = self.call_rpc("tools/list", Some(json!({})))?;
        let Some(list) = answer.get("tools").and_then(Value::as_array) else { return Ok(Vec::new()) };
        Ok(list.iter().filter_map(tool_of).collect())
    }

    /// Run one tool, as its JSON arguments, and read its answer as text.
    fn call_tool(&mut self, name: &str, arguments: &Value) -> Result<String, TransportError> {
        let answer = self.call_rpc("tools/call", Some(json!({"name": name, "arguments": arguments})))?;
        let mut out = String::new();
        if let Some(content) = answer.get("content").and_then(Value::as_array) {
            for block in content {
                if block.get("type").and_then(Value::as_str) == Some("text")
                    && let Some(text) = block.get("text").and_then(Value::as_str)
                {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(text);
                }
            }
        }
        if answer.get("isError").and_then(Value::as_bool) == Some(true) && out.is_empty() {
            return Err(TransportError::Other("the tool reported an error without saying what it was".into()));
        }
        Ok(out)
    }

    /// One JSON-RPC request, and the answer's `result`.
    fn call_rpc(&self, method: &str, params: Option<Value>) -> Result<Value, TransportError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut request = ureq::post(self.server.url.as_str())
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", PROTOCOL_VERSION);
            if let Some(token) = &self.server.bearer {
                request = request.header("authorization", format!("Bearer {token}"));
            }
            if let Some(session) = &self.session
                && !session.is_empty()
            {
                request = request.header("mcp-session-id", session);
            }
            let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params.unwrap_or_else(|| json!({}))});
            let response = request.send_json(&body).map_err(rpc_error)?;
            let status = response.status().as_u16();
            if status != 200 {
                let mut body = response.into_body();
                let text = body.read_to_string().unwrap_or_default();
                return Err(TransportError::BadResponse(format!("{status} {text}")));
            }
            read_result(response.into_body().into_reader())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (method, params);
            Err(TransportError::Other("the browser build has no MCP client".into()))
        }
    }
}

/// The desktop build's MCP factory: one client per server, on the thread that runs it.
#[derive(Default)]
pub struct McpClientFactory;

impl figcraft_ai::mcp::McpFactory for McpClientFactory {
    fn make(&self, url: String, bearer: Option<String>) -> Box<dyn figcraft_ai::mcp::McpSession> {
        Box::new(Client::new(Server { url, bearer }))
    }
}

/// This app's version, as the handshake names it.
fn figcraft_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Read one answer's JSON-RPC `result`, from JSON or from an event stream.
#[cfg(not(target_arch = "wasm32"))]
fn read_result(reader: impl std::io::Read) -> Result<Value, TransportError> {
    // An answer may be one JSON document, or an event stream whose `message` event carries it.
    let mut text = Vec::new();
    let mut bounded = reader.take(MAX_REPLY as u64);
    use std::io::Read as _;
    match bounded.read_to_end(&mut text) {
        Ok(_) => {}
        Err(_) => return Err(TransportError::Network),
    }
    if text.len() >= MAX_REPLY {
        return Err(TransportError::BadResponse("the server's answer was too large".into()));
    }
    let text = String::from_utf8_lossy(&text);
    if text.trim_start().starts_with('{') {
        return result_of(&text).ok_or_else(|| TransportError::BadResponse("the answer was not a JSON-RPC result".into()));
    }
    // An event stream: the data payloads are JSON-RPC answers.
    let mut events: Vec<String> = Vec::new();
    let mut cursor = std::io::Cursor::new(text.into_owned().into_bytes());
    sse::read_events(&mut cursor, &|| false, &mut |e| {
        events.push(e.data);
        true
    })
    .map_err(|_| TransportError::Network)?;
    for payload in &events {
        if let Some(result) = result_of(payload) {
            return Ok(result);
        }
    }
    Err(TransportError::BadResponse("the answer was not a JSON-RPC result".into()))
}

/// One JSON document's `result`, or its `error` as a transport error.
#[cfg(not(target_arch = "wasm32"))]
fn result_of(text: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(text).ok()?;
    if let Some(error) = value.get("error") {
        let message = error.get("message").and_then(Value::as_str).unwrap_or("the server refused the request");
        // An error is an answer, not a failure of the transport: the model reads it and can
        // correct its call. It comes back as a result-shaped error object.
        return Some(json!({"isError": true, "content": [{"type": "text", "text": message}]}));
    }
    value.get("result").cloned()
}

/// What a server's refusal means.
#[cfg(not(target_arch = "wasm32"))]
fn rpc_error(e: ureq::Error) -> TransportError {
    crate::transport_error(&e.to_string())
}

/// One tool of a server's list, as the model may call it.
///
/// A tool without a name, or whose schema is not an object, is left out rather than offered.
fn tool_of(value: &Value) -> Option<Tool> {
    let name = value.get("name").and_then(Value::as_str)?.to_string();
    let description = value.get("description").and_then(Value::as_str).unwrap_or_default().to_string();
    let schema = value.get("inputSchema").cloned().unwrap_or_else(|| json!({"type": "object"}));
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return None;
    }
    Some(Tool { name, description, input_schema: schema })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_without_a_name_is_left_out() {
        assert!(tool_of(&json!({"description": "x"})).is_none());
    }

    #[test]
    fn a_tool_whose_schema_is_not_an_object_is_left_out() {
        assert!(tool_of(&json!({"name": "x", "inputSchema": {"type": "string"}})).is_none());
    }

    #[test]
    fn a_tool_without_a_schema_takes_an_empty_object() {
        let tool = tool_of(&json!({"name": "search"})).expect("the tool is listed");
        assert_eq!(tool.name, "search");
        assert_eq!(tool.input_schema["type"], "object");
    }

    #[test]
    fn a_tool_s_description_may_be_empty() {
        let tool = tool_of(&json!({"name": "x", "description": "", "inputSchema": {"type": "object"}})).expect("the tool is listed");
        assert_eq!(tool.description, "");
    }

    #[test]
    fn the_client_speaks_one_protocol_version() {
        assert_eq!(PROTOCOL_VERSION, "2025-06-18");
    }
}
