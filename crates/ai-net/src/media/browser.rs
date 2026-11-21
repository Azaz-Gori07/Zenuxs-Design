//! The page's wire: the browser's own `fetch`.
//!
//! A page cannot block and has no threads, so the request is a promise the event loop drives.
//! The request itself is built by the code the desktop build sends ([`super::Request`]) and the
//! answer is read by the same parsers ([`super::Reply`]); this module only moves the bytes.
//!
//! What differs is what a page is allowed to do. A cross-origin request only succeeds when the
//! server allows this page's origin, and the answer is only readable when it does. Wikimedia's
//! API needs `origin=*` for that (the request builder adds it); Openverse sends the header. A
//! refused request says so in words that name the cause, never a bare "network error".

use figcraft_ai::media::{BoxFuture, MediaFactory, MediaSession};
use figcraft_ai::transport::TransportError;
use js_sys::Reflect;
use wasm_bindgen::{JsCast as _, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, RequestInit, Response};

use super::{Accept, Method, Reply, Request, Wire, WireMedia};

/// A wire that sends through the page.
#[derive(Default)]
pub struct BrowserWire;

impl Wire for BrowserWire {
    fn call(&mut self, request: Request) -> BoxFuture<'_, Result<Reply, TransportError>> {
        Box::pin(async move { send(&request).await })
    }
}

/// One request, sent through the page and read.
async fn send(request: &Request) -> Result<Reply, TransportError> {
    let url = super::super::browser::absolute_url(&request.url);
    let init = RequestInit::new();
    init.set_method(match request.method {
        Method::Get => "GET",
        Method::Post => "POST",
    });
    let headers = Headers::new().map_err(|_| TransportError::Network)?;
    for (name, value) in &request.headers {
        headers.append(name, value).map_err(|_| TransportError::Network)?;
    }
    if let Some(body) = &request.body {
        init.set_body(&JsValue::from_str(&body.to_string()));
    }
    init.set_headers(&headers);
    let window = web_sys::window().ok_or(TransportError::Network)?;
    let object = web_sys::Request::new_with_str_and_init(&url, &init).map_err(|e| refuse(&url, &e))?;
    let response: Response =
        JsFuture::from(window.fetch_with_request(&object)).await.map_err(|e| refuse(&url, &e))?.dyn_into().map_err(|_| TransportError::Network)?;
    let status = response.status();
    let body = match request.accept {
        Accept::Json => {
            let text = JsFuture::from(response.text().map_err(|_| TransportError::Network)?).await.map_err(|_| TransportError::Network)?;
            text.as_string().unwrap_or_default().into_bytes()
        }
        Accept::Bytes => {
            let buffer = JsFuture::from(response.array_buffer().map_err(|_| TransportError::Network)?).await.map_err(|_| TransportError::Network)?;
            let bytes = js_sys::Uint8Array::new(&buffer);
            // The cap is checked here too: a body is never allowed to grow without bound, and a
            // page that has already read it must not hand a giant buffer on.
            if bytes.length() as usize > figcraft_ai::MAX_ASSET_BYTES {
                return Err(TransportError::Other(format!("the image is larger than the {} byte limit", figcraft_ai::MAX_ASSET_BYTES)));
            }
            bytes.to_vec()
        }
    };
    Ok(Reply { status, body })
}

/// Why a page's `fetch` failed, in words that name the cause.
fn refuse(url: &str, error: &JsValue) -> TransportError {
    let mut detail = error.as_string().unwrap_or_default();
    if detail.is_empty()
        && let Ok(exception) = error.clone().dyn_into::<web_sys::DomException>()
    {
        detail = exception.message();
    }
    if detail.is_empty() {
        detail = Reflect::get(error, &JsValue::from_str("message")).ok().and_then(|v| v.as_string()).unwrap_or_default();
    }
    let page = web_sys::window().and_then(|w| w.location().protocol().ok()).unwrap_or_default();
    let loopback = url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost") || url.starts_with("http://[::1]");
    if page == "https:" && url.starts_with("http://") && !loopback {
        return TransportError::Other(format!(
            "{url} is an http:// address and this page is https://, so the browser blocks the request as mixed \
             content. Use an https:// endpoint, or open FigCraft over http://."
        ));
    }
    TransportError::Other(format!("Could not reach {url}: {detail}. The service must allow this page's origin (CORS) to be read from a browser."))
}

/// The page's media factory.
///
/// `MediaFactory` has to be `Send + Sync`, so this holds nothing: a `web_sys` handle is neither,
/// and every per-request object is made inside the future instead.
#[derive(Default)]
pub struct BrowserMediaFactory;

impl MediaFactory for BrowserMediaFactory {
    fn make(
        &self,
        api_key: Option<String>,
        stock_key: Option<String>,
        vision: Option<figcraft_ai::VisionConfig>,
        vectorize: Option<(figcraft_ai::VectorizeProvider, String)>,
    ) -> Box<dyn MediaSession> {
        Box::new(WireMedia::new(BrowserWire, api_key, stock_key, vision, vectorize))
    }
}
