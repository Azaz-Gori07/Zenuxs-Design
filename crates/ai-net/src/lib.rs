//! FigCraft's AI transport: what asks a model and reads its answer.
//!
//! The crate is the transport half of the seam [`figcraft_ai::Transport`] draws: the request
//! goes out, the answer is read as server-sent events and turned into the chunks the chat
//! runtime shows. The four wire protocols a provider can speak are in [`openai`], [`responses`],
//! [`anthropic`] and [`google`]; which one a request uses is the provider's own choice, read from
//! the registry.
//!
//! Two builds, one protocol. The desktop build sends over HTTPS and blocks on a worker thread
//! ([`HttpTransport`]). The browser build cannot block and has no threads, so it sends through
//! the page's own `fetch` and reports through the event loop ([`BrowserTransport`]). Both build
//! the same request from the same [`request_body`](fn@protocol_body) and read the same answer
//! with the same parsers, so the two builds agree on the wire and only the socket differs.
//!
//! The stock-photo, Vision, cloud, storage and MCP halves stay desktop-only: they are separate
//! `ureq` clients, not part of asking a model.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

// The wire protocols are pure: both builds share them.
pub mod anthropic;
pub mod google;
pub mod openai;
pub mod responses;
pub mod sse;

// The desktop build's socket half, and what hangs off it.
#[cfg(not(target_arch = "wasm32"))]
pub mod acp;
#[cfg(not(target_arch = "wasm32"))]
pub mod cloud;
#[cfg(not(target_arch = "wasm32"))]
pub mod mcp;
#[cfg(not(target_arch = "wasm32"))]
pub mod s3;

// The browser build's socket half: the page's `fetch`. The parts that are not about the socket
// compile everywhere, so the browser's URL handling, line splitting and final parse are tested
// by the same `cargo test` the desktop build runs.
pub mod browser;

// The media half — image search, generation, downloads and the Vision call — is split the same
// way: one protocol, two wires. The requests and the parsers compile everywhere and are tested
// in both builds; only the socket differs.
pub mod media;

use figcraft_ai::providers::Protocol;
#[cfg(not(target_arch = "wasm32"))]
use figcraft_ai::transport::Transport;
use figcraft_ai::transport::{ChatRequest, ChatResponse, Chunk, TransportError};

pub use browser::BrowserTransport;

/// The browser build's transport, and the seam the chat starts a request through.
#[cfg(target_arch = "wasm32")]
pub use browser::BrowserFactory;

/// The desktop transport: HTTPS to the provider, streaming back through the callback.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub struct HttpTransport;

/// Which transport this build offers.
#[cfg(not(target_arch = "wasm32"))]
pub fn transport() -> Box<dyn Transport> {
    Box::new(HttpTransport)
}

/// Where one provider's request goes, and who it is.
pub struct Endpoint {
    /// The base URL (`https://api.openai.com/v1`).
    pub base_url: String,
    /// The API key.
    pub api_key: String,
    /// Which protocol to speak.
    pub protocol: Protocol,
    /// Whether an OpenAI-protocol endpoint takes the Responses API instead.
    pub responses_api: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl Transport for HttpTransport {
    fn available(&self) -> bool {
        true
    }

    fn complete(&self, request: &ChatRequest, on_chunk: &mut dyn FnMut(Chunk)) -> Result<ChatResponse, TransportError> {
        if request.protocol == Protocol::Acp {
            return complete_acp(request, on_chunk);
        }
        let endpoint = Endpoint {
            base_url: request.base_url.clone(),
            api_key: request.api_key.clone(),
            protocol: request.protocol,
            responses_api: request.responses_api,
        };
        complete(&endpoint, request, on_chunk)
    }
}

/// Ask an external ACP agent, using the model id to name a known agent or a command to run.
#[cfg(not(target_arch = "wasm32"))]
fn complete_acp(request: &ChatRequest, on_chunk: &mut dyn FnMut(Chunk)) -> Result<ChatResponse, TransportError> {
    let chosen = request.model.trim();
    let (command, args) = match figcraft_ai::acp::agent(chosen) {
        Some(def) => (def.command.clone(), def.args.clone()),
        None if chosen.contains(std::path::MAIN_SEPARATOR) || chosen.starts_with('.') => (chosen.to_string(), Vec::new()),
        None => {
            return Err(TransportError::Other(format!(
                "\"{chosen}\" is not a known agent. Choose claude-code, codex or gemini-cli, or put a command path in the model id."
            )));
        }
    };
    let cwd = if request.base_url.trim().is_empty() { String::from(".") } else { request.base_url.trim().to_string() };
    let mut session = acp::AcpSession::start(&command, &args, &cwd)?;
    session.prompt(&acp::prompt_text(request), on_chunk)
}

/// One request's body, in the shape its protocol expects.
///
/// Shared by both builds: whichever way the request leaves, it says the same thing.
pub fn protocol_body(endpoint: &Endpoint, request: &ChatRequest) -> Result<serde_json::Value, TransportError> {
    match endpoint.protocol {
        Protocol::Acp => Err(TransportError::Other("an ACP agent does not use an HTTP request body".into())),
        Protocol::Anthropic => Ok(anthropic::request_body(request)),
        Protocol::Google => Ok(google::request_body(request)),
        Protocol::Openai if endpoint.responses_api => Ok(responses::request_body(request)),
        Protocol::Openai => Ok(openai::request_body(request)),
    }
}

/// The headers one request carries, as name/value pairs.
///
/// Shared, for the same reason the body is: a provider that worked on the desktop must work in
/// the page, and a header added to one build only would break that quietly.
pub fn request_headers(endpoint: &Endpoint) -> Vec<(&'static str, String)> {
    match endpoint.protocol {
        Protocol::Acp => Vec::new(),
        Protocol::Anthropic => {
            vec![("x-api-key", endpoint.api_key.clone()), ("anthropic-version", "2023-06-01".into()), ("content-type", "application/json".into())]
        }
        Protocol::Google => vec![("x-goog-api-key", endpoint.api_key.clone()), ("content-type", "application/json".into())],
        Protocol::Openai => vec![("authorization", format!("Bearer {}", endpoint.api_key)), ("content-type", "application/json".into())],
    }
}

/// Turn one answer's events into chunks and an answer.
///
/// Shared by both builds: the parsers are pure, so a stream read from `ureq` and a stream read
/// from the page's `fetch` are turned into the same chunks.
pub fn parse_events(
    protocol: Protocol,
    data: &[String],
    events: &[(Option<String>, String)],
    on_chunk: &mut dyn FnMut(Chunk),
) -> Result<ChatResponse, TransportError> {
    let (chunks, response) = match protocol {
        Protocol::Acp => return Err(TransportError::Other("an ACP agent does not stream over HTTP".into())),
        Protocol::Openai => openai::parse_stream(data),
        Protocol::Anthropic => anthropic::parse_stream(events),
        Protocol::Google => google::parse_stream(events),
    }
    .map_err(TransportError::BadResponse)?;
    for chunk in chunks {
        on_chunk(chunk);
    }
    Ok(response)
}

/// Ask a provider and read the answer, over HTTPS on this thread.
#[cfg(not(target_arch = "wasm32"))]
pub fn complete(endpoint: &Endpoint, request: &ChatRequest, on_chunk: &mut dyn FnMut(Chunk)) -> Result<ChatResponse, TransportError> {
    let body = protocol_body(endpoint, request)?;
    let url = request_url(endpoint, request);
    let response = headers(endpoint, ureq::post(&url)).send_json(&body).map_err(|e| transport_error(&e.to_string()))?;
    let status = response.status().as_u16();
    if status != 200 {
        // The error body is the provider's own words; a few of them say what went wrong
        // more plainly than the status does.
        let mut body = response.into_body();
        let text = body.read_to_string().unwrap_or_default();
        return Err(transport_error(&format!("{status} {text}")));
    }
    read_stream(response.into_body().into_reader(), endpoint.protocol, on_chunk)
}

/// The URL one request goes to.
pub fn request_url(endpoint: &Endpoint, request: &ChatRequest) -> String {
    let base = endpoint.base_url.trim_end_matches('/');
    match endpoint.protocol {
        Protocol::Acp => String::new(),
        Protocol::Anthropic => format!("{base}/messages"),
        Protocol::Google => google::request_url(base, &request.model),
        Protocol::Openai if endpoint.responses_api => format!("{base}/responses"),
        Protocol::Openai => format!("{base}/chat/completions"),
    }
}

/// The headers one request carries, for the desktop client.
#[cfg(not(target_arch = "wasm32"))]
fn headers(endpoint: &Endpoint, http: ureq::RequestBuilder<ureq::typestate::WithBody>) -> ureq::RequestBuilder<ureq::typestate::WithBody> {
    let mut http = http;
    for (name, value) in request_headers(endpoint) {
        http = http.header(name, value);
    }
    http
}

/// Read one answer's stream, turning its events into chunks and an answer.
#[cfg(not(target_arch = "wasm32"))]
fn read_stream(reader: impl std::io::Read, protocol: Protocol, on_chunk: &mut dyn FnMut(Chunk)) -> Result<ChatResponse, TransportError> {
    let mut events: Vec<(Option<String>, String)> = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let result = sse::read_events(&mut { reader }, &|| false, &mut |event| {
        // The parsers take one shape: OpenAI's unnamed data payloads, the others' named
        // events. Reading is stopped by the parsers' own errors, not here.
        match protocol {
            Protocol::Openai => data.push(event.data),
            _ => events.push((event.event, event.data)),
        }
        true
    });
    if let Err(sse::SseError::Io) = result {
        return Err(TransportError::Cancelled);
    }
    if let Err(e) = result {
        return Err(TransportError::BadResponse(e.to_string()));
    }
    parse_events(protocol, &data, &events, on_chunk)
}

/// What a provider's failure means, from its status and its words.
///
/// The chat shows a different sentence for a bad key than for a full rate limit, so the kinds
/// matter; the words are the fallback, because every provider words its own errors.
pub fn transport_error(text: &str) -> TransportError {
    let lower = text.to_lowercase();
    let status = lower.split(' ').next().and_then(|s| s.parse::<u16>().ok());
    match status {
        Some(401) | Some(403) => return TransportError::Authentication,
        Some(404) => return TransportError::ModelNotFound,
        Some(429) => return TransportError::RateLimited,
        Some(402) => return TransportError::InsufficientCredit,
        _ => {}
    }
    if lower.contains("insufficient") && (lower.contains("credit") || lower.contains("quota") || lower.contains("balance")) {
        return TransportError::InsufficientCredit;
    }
    if lower.contains("model") && (lower.contains("not found") || lower.contains("does not exist")) {
        return TransportError::ModelNotFound;
    }
    if lower.contains("responses") || lower.contains("chat/completions") || lower.contains("endpoint") && lower.contains("not") {
        return TransportError::ApiTypeMismatch;
    }
    if lower.contains("rate limit") || lower.contains("too many requests") {
        return TransportError::RateLimited;
    }
    if lower.contains("timeout") || lower.contains("timed out") || lower.contains("connection") || lower.contains("dns error") {
        return TransportError::Network;
    }
    if status.is_some() {
        return TransportError::BadResponse(text.to_string());
    }
    TransportError::Network
}

#[cfg(test)]
mod tests {
    use super::*;
    use figcraft_ai::transport::{ChatMessage, Tool};
    use serde_json::json;

    fn request(protocol: Protocol) -> ChatRequest {
        ChatRequest {
            model: "gpt-5.6".into(),
            base_url: "https://api.example.com/v1".into(),
            api_key: "sk-test".into(),
            protocol,
            responses_api: false,
            messages: vec![ChatMessage::user("hello")],
            system: "be brief".into(),
            tools: vec![Tool { name: "run_command".into(), description: "Run a command.".into(), input_schema: json!({"type": "object"}) }],
            max_output_tokens: 512,
            reasoning_effort: None,
        }
    }

    #[test]
    fn each_protocol_has_its_own_url() {
        assert_eq!(request_url(&endpoint(Protocol::Openai), &request(Protocol::Openai)), "https://api.example.com/v1/chat/completions");
        assert_eq!(request_url(&endpoint(Protocol::Anthropic), &request(Protocol::Anthropic)), "https://api.example.com/v1/messages");
        assert_eq!(
            request_url(&endpoint(Protocol::Google), &request(Protocol::Google)),
            "https://api.example.com/v1/models/gpt-5.6:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn the_responses_api_changes_the_url_not_the_protocol() {
        let mut endpoint = endpoint(Protocol::Openai);
        endpoint.responses_api = true;
        assert_eq!(request_url(&endpoint, &request(Protocol::Openai)), "https://api.example.com/v1/responses");
    }

    fn endpoint(protocol: Protocol) -> Endpoint {
        Endpoint { base_url: "https://api.example.com/v1/".into(), api_key: "sk-test".into(), protocol, responses_api: false }
    }

    #[test]
    fn statuses_mean_what_they_mean() {
        assert!(matches!(transport_error("401 Unauthorized"), TransportError::Authentication));
        assert!(matches!(transport_error("404 model not found"), TransportError::ModelNotFound));
        assert!(matches!(transport_error("429 Too Many Requests"), TransportError::RateLimited));
        assert!(matches!(transport_error("402 insufficient credits"), TransportError::InsufficientCredit));
    }

    #[test]
    fn the_words_mean_what_they_mean_too() {
        assert!(matches!(transport_error("the account has insufficient credit"), TransportError::InsufficientCredit));
        assert!(matches!(transport_error("model gpt-x does not exist"), TransportError::ModelNotFound));
        assert!(matches!(transport_error("connection refused"), TransportError::Network));
        assert!(matches!(transport_error("500 who knows"), TransportError::BadResponse(_)));
    }

    #[test]
    fn a_status_with_no_words_is_still_a_bad_response() {
        let e = transport_error("503");
        assert!(matches!(e, TransportError::BadResponse(_)));
        assert_eq!(e.message(), "The provider's answer could not be read: 503");
    }

    #[test]
    fn a_status_with_words_keeps_the_words() {
        let e = transport_error("500 who knows");
        assert!(matches!(e, TransportError::BadResponse(_)));
        assert_eq!(e.message(), "The provider's answer could not be read: 500 who knows");
    }

    #[test]
    fn a_request_body_is_built_without_a_socket() {
        // Both builds share this, so the page sends what the app sends.
        let body = protocol_body(&endpoint(Protocol::Openai), &request(Protocol::Openai)).expect("a body");
        assert_eq!(body["model"], "gpt-5.6");
        assert_eq!(body["messages"][0]["content"], "hello");
    }

    #[test]
    fn each_protocol_carries_its_own_headers() {
        let openai = request_headers(&endpoint(Protocol::Openai));
        assert!(openai.iter().any(|(n, v)| *n == "authorization" && v == "Bearer sk-test"));
        let anthropic = request_headers(&endpoint(Protocol::Anthropic));
        assert!(anthropic.iter().any(|(n, v)| *n == "x-api-key" && v == "sk-test"));
        assert!(anthropic.iter().any(|(n, _)| *n == "anthropic-version"));
        let google = request_headers(&endpoint(Protocol::Google));
        assert!(google.iter().any(|(n, _)| *n == "x-goog-api-key"));
    }
}
