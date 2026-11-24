//! The page's `fetch`, and the factory that starts it.
//!
//! Everything here needs a browser. The request is built by the code the desktop build uses
//! ([`protocol_body`](fn@crate::protocol_body), [`request_headers`](fn@crate::request_headers))
//! and the answer is parsed by the code the desktop build uses
//! ([`parse_events`](fn@crate::parse_events)); this module only moves the bytes.

use figcraft_ai::providers::Protocol;
use figcraft_ai::transport::{ChatRequest, ChatResponse, StreamEvent, Transport, TransportError, TransportFactory};
use js_sys::{Reflect, Uint8Array};
use wasm_bindgen::{JsCast as _, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, Headers, Request, RequestInit, Response};

use super::{Lines, MAX_REPLY, absolute_url, cap, finish};
use crate::{Endpoint, protocol_body, request_headers, request_url, transport_error};

/// The transport the page hands to the panel, and the seam it starts requests through.
///
/// `TransportFactory` has to be `Send + Sync`, so this holds nothing: a `web_sys` handle is
/// neither, and every per-request object is made inside the future instead. The page's event
/// loop drives that future, which is why [`start`] answers `true` — there is no thread to spawn.
#[derive(Default)]
pub struct BrowserFactory;

impl TransportFactory for BrowserFactory {
    fn make(&self, _request: &ChatRequest) -> Box<dyn Transport> {
        Box::new(super::BrowserTransport)
    }

    fn start(&self, request: ChatRequest, mut on_event: Box<dyn FnMut(StreamEvent) + Send>) -> bool {
        wasm_bindgen_futures::spawn_local(async move {
            // `ask` reports `Done` itself, last, whatever the outcome.
            drop(ask(request, &mut *on_event).await);
        });
        true
    }
}

/// Ask a provider through the page, reporting each piece as it arrives.
///
/// The returned future resolves once, with the whole answer. Every [`StreamEvent::Text`] and
/// [`StreamEvent::Reasoning`] is reported as it arrives so the panel streams exactly as it does
/// on the desktop; [`StreamEvent::Done`] is last, always.
pub async fn ask(request: ChatRequest, on_event: &mut dyn FnMut(StreamEvent)) -> Result<ChatResponse, TransportError> {
    let result = ask_inner(&request, on_event).await;
    on_event(StreamEvent::Done(result.clone()));
    result
}

async fn ask_inner(request: &ChatRequest, on_event: &mut dyn FnMut(StreamEvent)) -> Result<ChatResponse, TransportError> {
    if request.protocol == Protocol::Acp {
        return Err(TransportError::Other("An external agent runs its own program, which a page cannot start.".into()));
    }
    let endpoint = Endpoint {
        base_url: request.base_url.clone(),
        api_key: request.api_key.clone(),
        protocol: request.protocol,
        responses_api: request.responses_api,
    };
    let body = protocol_body(&endpoint, request)?;
    let url = absolute_url(&request_url(&endpoint, request));
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_body(&JsValue::from_str(&body.to_string()));
    let headers = Headers::new().map_err(|_| TransportError::Network)?;
    for (name, value) in request_headers(&endpoint) {
        headers.append(name, &value).map_err(|_| TransportError::Network)?;
    }
    init.set_headers(&headers);
    // The panel's Stop drops the receiver; the promise would otherwise keep reading a stream
    // nobody shows. The controller is dropped with the future, which aborts the request.
    let controller = AbortController::new().ok();
    if let Some(controller) = &controller {
        init.set_signal(Some(&controller.signal()));
    }
    let window = web_sys::window().ok_or(TransportError::Network)?;
    let request_object = Request::new_with_str_and_init(&url, &init).map_err(|e| refuse(&url, &e))?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request_object))
        .await
        .map_err(|e| refuse(&url, &e))?
        .dyn_into()
        .map_err(|_| TransportError::Network)?;
    let status = response.status();
    if !(200..300).contains(&status) {
        // The provider's own words explain a failure better than its status does.
        let text = JsFuture::from(response.text().map_err(|_| TransportError::Network)?).await.ok().and_then(|v| v.as_string()).unwrap_or_default();
        return Err(transport_error(&format!("{status} {}", cap(&text))));
    }
    let body_stream = response.body().ok_or_else(|| TransportError::BadResponse("the answer carried no stream".into()))?;
    let reader = body_stream.get_reader().dyn_into::<web_sys::ReadableStreamDefaultReader>().map_err(|_| TransportError::Network)?;
    let mut lines = Lines::default();
    let mut read = 0usize;
    let mut collected: Vec<u8> = Vec::new();
    loop {
        let Ok(chunk) = JsFuture::from(reader.read()).await else { break };
        let Ok(value) = Reflect::get(&chunk, &JsValue::from_str("value")) else { break };
        if value.is_undefined() || value.is_null() {
            break;
        }
        let Ok(bytes) = value.dyn_into::<Uint8Array>() else { break };
        read += bytes.length() as usize;
        if read > MAX_REPLY {
            drop(reader.cancel());
            return Err(TransportError::Other(format!("the answer was larger than the {MAX_REPLY} byte limit")));
        }
        let piece = bytes.to_vec();
        collected.extend_from_slice(&piece);
        // A line can straddle two network pieces, so the split happens here, not at the socket:
        // a partial line waits for the rest of itself before it is read.
        for event in lines.push(&piece) {
            on_event(event);
        }
    }
    // Whatever the stream held without a closing newline still counts.
    for event in lines.finish() {
        on_event(event);
    }
    reader.release_lock();
    finish(request.protocol, &collected)
}

/// What a failed `fetch` means in words.
///
/// The browser refuses a request before it leaves, for reasons that are the person's to fix — a
/// mixed-content block, a refused preflight, a server without CORS — and the failure itself says
/// only "Failed to fetch". Naming the likely cause is the difference between a chat that looks
/// broken and one that says what to change.
fn refuse(url: &str, error: &JsValue) -> TransportError {
    let mut detail = error.as_string().unwrap_or_default();
    if let Ok(exception) = error.clone().dyn_into::<web_sys::DomException>() {
        detail = exception.message();
    }
    let page = web_sys::window().and_then(|w| w.location().protocol().ok()).unwrap_or_default();
    let loopback = url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost") || url.starts_with("http://[::1]");
    if page == "https:" && url.starts_with("http://") && !loopback {
        return TransportError::Other(format!(
            "{url} is an http:// address and this page is https://, so the browser blocks the request as mixed \
             content. Use an https:// endpoint, or open FigCraft over http://."
        ));
    }
    TransportError::Other(format!(
        "Could not reach {url}: {detail}. The endpoint must allow this page's origin (CORS) and answer the \
         request's preflight."
    ))
}
