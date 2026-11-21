//! The MCP server connections: what the AI agent can reach besides the document.
//!
//! A connection is a streamable-HTTP MCP server — an external one the person names, with an
//! optional bearer token. The agent's tool set grows by whatever tools those servers list,
//! prefixed `mcp__<server>__`, and calls on them go to the server rather than to the engine.
//!
//! The rules here are the trust rules: a connection speaks HTTPS unless it is on this machine
//! (a loopback address needs no eavesdropping), a URL may not carry its own credentials, and
//! the name `figcraft` is reserved for this app's own server.

use serde::{Deserialize, Serialize};

/// One MCP server the agent can call tools on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MCPConnection {
    /// The connection's id (`mcp-…`), unique among the connections.
    pub id: String,
    /// The name the tools are prefixed with and the settings show.
    pub name: String,
    /// The server's URL (`https://host/mcp`).
    pub url: String,
    /// Whether the server wants a bearer token.
    #[serde(default)]
    pub authentication: MCPAuthentication,
    /// Whether the connection is used.
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// How a connection authenticates.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MCPAuthentication {
    /// Nothing: the server is open.
    #[default]
    None,
    /// A bearer token, from the credential store.
    Bearer,
}

/// Everything the MCP settings hold.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MCPSettings {
    /// The settings' version, so a later one can migrate them.
    pub version: u32,
    /// The connections.
    #[serde(default)]
    pub connections: Vec<MCPConnection>,
}

/// The name this app's own server keeps.
pub const RESERVED_NAME: &str = "figcraft";

/// The most connections kept.
pub const MAX_CONNECTIONS: usize = 20;

/// Why a connection's definition is not one it can be.
///
/// Each case is a sentence the settings show.
pub fn check_connection(name: &str, url: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("the connection needs a name".into());
    }
    if name.eq_ignore_ascii_case(RESERVED_NAME) {
        return Err(format!("\"{RESERVED_NAME}\" is this app's own server; pick another name"));
    }
    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.') {
        return Err("the name is letters, numbers, dashes, dots and underscores".into());
    }
    let url = url.trim();
    let parsed = url::Url::parse(url).map_err(|_| "the URL is not one (`https://host/mcp`)".to_string())?;
    if parsed.username() != "" || parsed.password().is_some() {
        return Err("the URL carries its own credentials; put the token in the connection's key instead".into());
    }
    let loopback = matches!(parsed.host_str(), Some(h) if is_loopback(h));
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if loopback => Ok(()),
        "http" => Err("only this machine's servers may use plain http; others need https".into()),
        _ => Err("the URL is http or https".into()),
    }
}

/// Whether a host is this machine: localhost, or a loopback address.
fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host.trim_start_matches('[').trim_end_matches(']').parse::<std::net::IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false)
}

/// Whether an id looks like one of ours (`mcp-` and safe characters, as the settings store them).
pub fn id_is_well_formed(id: &str) -> bool {
    id.len() > 4
        && id.starts_with("mcp-")
        && id[4..].bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'_' || b == b'-')
}

/// One MCP server's session, as the agent uses it.
///
/// The trait is the seam: the desktop build implements it over HTTPS, the browser build has
/// nothing behind it. A session runs on one thread at a time, so a fresh one per request keeps
/// that obvious.
pub trait McpSession: Send {
    /// Open the session: the handshake the protocol asks for.
    fn initialize(&mut self) -> Result<(), crate::transport::TransportError>;
    /// The tools the server offers.
    fn tools(&mut self) -> Result<Vec<crate::transport::Tool>, crate::transport::TransportError>;
    /// Run one tool, and read its answer as text.
    fn call(&mut self, name: &str, arguments: &serde_json::Value) -> Result<String, crate::transport::TransportError>;
}

/// The factory a worker thread makes its session from.
pub trait McpFactory: Send + Sync {
    /// A session with the server at `url`, authenticated with `bearer` when it wants one.
    fn make(&self, url: String, bearer: Option<String>) -> Box<dyn McpSession>;
}

/// The factory with nothing behind it.
#[derive(Default)]
pub struct NoMcp;

impl McpFactory for NoMcp {
    fn make(&self, _url: String, _bearer: Option<String>) -> Box<dyn McpSession> {
        Box::new(NoMcpSession)
    }
}

/// The session the default factory makes: there is nothing here.
#[derive(Default)]
struct NoMcpSession;

impl McpSession for NoMcpSession {
    fn initialize(&mut self) -> Result<(), crate::transport::TransportError> {
        Err(crate::transport::TransportError::Other("this build has no MCP client".into()))
    }

    fn tools(&mut self) -> Result<Vec<crate::transport::Tool>, crate::transport::TransportError> {
        Err(crate::transport::TransportError::Other("this build has no MCP client".into()))
    }

    fn call(&mut self, _name: &str, _arguments: &serde_json::Value) -> Result<String, crate::transport::TransportError> {
        Err(crate::transport::TransportError::Other("this build has no MCP client".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_https_url_is_fine() {
        assert!(check_connection("notes", "https://example.com/mcp").is_ok());
    }

    #[test]
    fn a_loopback_server_may_be_plain_http() {
        assert!(check_connection("local", "http://localhost:7600/mcp").is_ok());
        assert!(check_connection("local", "http://127.0.0.1:7600/mcp").is_ok());
        assert!(check_connection("local", "http://[::1]:7600/mcp").is_ok());
    }

    #[test]
    fn a_plain_http_server_off_this_machine_is_refused() {
        let err = check_connection("web", "http://example.com/mcp").expect_err("plain http is refused");
        assert!(err.contains("https"), "{err}");
    }

    #[test]
    fn a_url_with_its_own_credentials_is_refused() {
        let err = check_connection("notes", "https://user:pass@example.com/mcp").expect_err("credentials in the URL");
        assert!(err.contains("credentials"), "{err}");
    }

    #[test]
    fn the_app_s_own_name_is_reserved() {
        let err = check_connection("FigCraft", "https://example.com/mcp").expect_err("the reserved name");
        assert!(err.contains("figcraft"), "{err}");
    }

    #[test]
    fn a_name_is_letters_numbers_and_separators() {
        assert!(check_connection("my-notes.v2", "https://example.com/mcp").is_ok());
        let err = check_connection("my notes", "https://example.com/mcp").expect_err("a space in the name");
        assert!(err.contains("letters"), "{err}");
    }

    #[test]
    fn an_unreadable_url_is_refused() {
        assert!(check_connection("notes", "not a url").is_err());
        assert!(check_connection("", "https://example.com/mcp").is_err());
    }

    #[test]
    fn an_id_is_ours_when_it_looks_like_one() {
        assert!(id_is_well_formed("mcp-notes"));
        assert!(id_is_well_formed("mcp-a1.b_c"));
        assert!(!id_is_well_formed("notes"));
        assert!(!id_is_well_formed("mcp-"));
        assert!(!id_is_well_formed("mcp-UPPER"));
    }
}
