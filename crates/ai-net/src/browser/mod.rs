//! The browser build's transport: the page's own `fetch`, read as a stream.
//!
//! A page has no threads and cannot block, so a request is a promise and its answer arrives in
//! pieces as the network delivers them. [`fetch`] starts the request, reads the response body
//! through a `ReadableStreamDefaultReader`, and reports each piece to the caller as it arrives;
//! the caller (`figcraft_ai::TransportFactory::start`) keeps the future alive on the event loop.
//!
//! What is sent and how the answer is read is shared with the desktop build
//! ([`protocol_body`](fn@crate::protocol_body), [`request_headers`](fn@crate::request_headers),
//! [`request_url`](fn@crate::request_url), [`parse_events`](fn@crate::parse_events)): only the
//! socket differs, so a provider that works in the app works in the page.
//!
//! Everything read is bounded. A page cannot trust a response's length, so the reader stops
//! itself [`MAX_REPLY`] bytes in and says so, the same way the SSE reader caps a line.
//!
//! The parts that are not about the socket — the URL, the line split, the final parse — live
//! here and are tested in every build. Only [`fetch`] needs a page.

// The socket half is a page's, so in the desktop build these are only reached by the tests below.
// They are compiled there on purpose: the parsing the page relies on is checked by the same
// `cargo test` the app runs, instead of only by opening a browser.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use figcraft_ai::providers::Protocol;
use figcraft_ai::transport::{ChatRequest, ChatResponse, Chunk, StreamEvent, Transport, TransportError};

use crate::parse_events;

#[cfg(target_arch = "wasm32")]
mod fetch;

#[cfg(target_arch = "wasm32")]
pub use fetch::{BrowserFactory, ask};

/// The most one answer may be.
///
/// A provider's streamed answer is a few hundred kilobytes; this cap exists so a hostile or
/// broken endpoint cannot fill the tab's memory.
pub const MAX_REPLY: usize = 16 << 20;

/// The browser build's transport: the page's `fetch`.
///
/// [`Transport::complete`] cannot be implemented here — a promise cannot be waited on — so it
/// says so, and the page asks through [`BrowserFactory`] instead, which starts the request and
/// reports as it arrives.
#[derive(Default)]
pub struct BrowserTransport;

impl Transport for BrowserTransport {
    fn available(&self) -> bool {
        true
    }

    fn complete(&self, _request: &ChatRequest, _on_chunk: &mut dyn FnMut(Chunk)) -> Result<ChatResponse, TransportError> {
        Err(TransportError::Other("The browser build asks the provider through the page's own fetch, which cannot be waited on from here.".into()))
    }
}

/// One request's URL: the provider's base, or whatever the person typed, made absolute.
///
/// A page resolves a relative URL against itself, so a base the person entered without a scheme
/// (`127.0.0.1:20129/v1`) has to be given one first, or the request goes to the FigCraft page.
pub fn absolute_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    // The page is served over `http://` or `https://`; a self-hosted endpoint on this machine
    // usually has no certificate, so the page's own scheme is the best guess available.
    let scheme = page_scheme();
    let scheme = if scheme == "http:" { "http" } else { "https" };
    format!("{scheme}://{}", trimmed.trim_start_matches("//"))
}

/// The scheme the FigCraft page was served over, or `https:` when there is no page.
#[cfg(target_arch = "wasm32")]
fn page_scheme() -> String {
    web_sys::window().and_then(|w| w.location().protocol().ok()).unwrap_or_else(|| "https:".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn page_scheme() -> String {
    "https:".into()
}

/// Everything the stream said, as chunks and an answer, using the desktop build's parsers.
///
/// The SSE reader is pure `std::io::Read`, so the whole body goes through it. It is already
/// bounded — [`MAX_REPLY`] — which is what the desktop client's socket does a piece at a time.
fn finish(protocol: Protocol, bytes: &[u8]) -> Result<ChatResponse, TransportError> {
    let mut events: Vec<(Option<String>, String)> = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let mut reader = std::io::Cursor::new(bytes.to_vec());
    let result = crate::sse::read_events(&mut reader, &|| false, &mut |event| {
        match protocol {
            Protocol::Openai => data.push(event.data),
            _ => events.push((event.event, event.data)),
        }
        true
    });
    if let Err(e) = result {
        return Err(TransportError::BadResponse(e.to_string()));
    }
    parse_events(protocol, &data, &events, &mut |_| {})
}

/// The most of a provider's error body that is repeated back.
fn cap(text: &str) -> String {
    text.chars().take(600).collect()
}

/// The lines a streamed body has completed so far.
///
/// The answer itself is parsed once at the end — the parsers assemble tool calls across the
/// whole stream and report their chunks in order — but the text is shown as it arrives, so a
/// long model step is visibly alive rather than a frozen frame.
#[derive(Default)]
struct Lines {
    pending: Vec<u8>,
}

impl Lines {
    /// The complete lines in `piece`, as the progress they carry.
    fn push(&mut self, piece: &[u8]) -> Vec<StreamEvent> {
        let mut out = Vec::new();
        self.pending.extend_from_slice(piece);
        while let Some(at) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=at).collect();
            out.extend(progress(&line));
        }
        // A provider that never sends a newline must not grow this without bound.
        if self.pending.len() > crate::sse::MAX_LINE {
            self.pending.clear();
        }
        out
    }

    /// The last line, which the stream may have ended without closing.
    fn finish(&mut self) -> Vec<StreamEvent> {
        let line = std::mem::take(&mut self.pending);
        progress(&line).into_iter().collect()
    }
}

/// One line's progress, when it carries any.
///
/// Only what the panel can show while the step runs is reported here: the text and the
/// reasoning. The tool calls need the whole stream to assemble, so they come from the final
/// parse instead, and reporting a half-assembled one would be a lie the panel acted on.
fn progress(line: &[u8]) -> Option<StreamEvent> {
    let text = String::from_utf8_lossy(line);
    let payload = text.trim().strip_prefix("data:")?.trim();
    if payload.is_empty() || payload == "[DONE]" {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    // OpenAI, and the endpoints that speak its protocol: `choices[0].delta`.
    if let Some(delta) = value.get("choices").and_then(|c| c.as_array()).and_then(|c| c.first()).and_then(|c| c.get("delta")) {
        if let Some(reasoning) = delta.get("reasoning_content").or_else(|| delta.get("reasoning")).and_then(|r| r.as_str())
            && !reasoning.is_empty()
        {
            return Some(StreamEvent::Reasoning(reasoning.to_string()));
        }
        if let Some(content) = delta.get("content").and_then(|c| c.as_str())
            && !content.is_empty()
        {
            return Some(StreamEvent::Text(content.to_string()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_provider_address_without_a_scheme_gets_one() {
        // Without this the page would resolve the address against itself and the request would
        // go to the FigCraft page instead of the provider.
        assert_eq!(absolute_url("http://127.0.0.1:20129/v1"), "http://127.0.0.1:20129/v1");
        assert_eq!(absolute_url(" https://api.example.com/v1 "), "https://api.example.com/v1");
        assert_eq!(absolute_url("127.0.0.1:20129/v1"), "https://127.0.0.1:20129/v1");
    }

    #[test]
    fn a_streamed_piece_of_text_reaches_the_panel_as_it_arrives() {
        let line = format!("data: {}\n", json!({"choices": [{"delta": {"content": "Hel"}}]}));
        assert!(matches!(progress(line.as_bytes()), Some(StreamEvent::Text(t)) if t == "Hel"));
    }

    #[test]
    fn reasoning_arrives_before_the_text_it_leads_to() {
        let line = format!("data: {}\n", json!({"choices": [{"delta": {"reasoning_content": "thinking"}}]}));
        assert!(matches!(progress(line.as_bytes()), Some(StreamEvent::Reasoning(r)) if r == "thinking"));
    }

    #[test]
    fn a_tool_call_is_left_to_the_final_parse() {
        // A half-assembled call reported now would be one the panel acted on.
        let line = format!("data: {}\n", json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "function": {"name": "shape"}}]}}]}));
        assert!(progress(line.as_bytes()).is_none());
    }

    #[test]
    fn the_end_of_the_stream_and_a_blank_line_say_nothing() {
        assert!(progress(b"data: [DONE]\n").is_none());
        assert!(progress(b"\n").is_none());
        assert!(progress(b": keep-alive\n").is_none());
    }

    #[test]
    fn a_provider_error_body_is_repeated_back_but_capped() {
        let long = "x".repeat(2000);
        assert_eq!(cap(&long).len(), 600);
    }

    #[test]
    fn a_whole_answer_is_read_by_the_same_parser_the_desktop_build_uses() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" there\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let answer = finish(Protocol::Openai, body.as_bytes()).expect("the stream reads");
        assert_eq!(answer.text, "Hello there");
        assert_eq!(answer.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn a_character_split_across_two_network_pieces_is_not_mangled() {
        // The reader collects the whole body before decoding, so a multi-byte character split
        // by the network is still one character. This is the reason for collecting.
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"glaze — wabi\"}}]}\n\n";
        let answer = finish(Protocol::Openai, body.as_bytes()).expect("the stream reads");
        assert_eq!(answer.text, "glaze — wabi");
    }
}
