//! The desktop build's wire: a blocking HTTPS client.
//!
//! FigCraft has no async runtime, and on the desktop it does not need one: the media tools run
//! on a worker thread, so a blocking call there is a call that finishes. Every future this wire
//! returns is ready on its first poll, which is what lets [`block_on`](figcraft_ai::media::block_on)
//! drive the same pipeline the page drives with `spawn_local`.

use figcraft_ai::media::{BoxFuture, MediaFactory, MediaSession};
use figcraft_ai::transport::TransportError;

use super::{Accept, Method, Reply, Request, Wire, WireMedia};

/// A wire that sends over HTTPS and waits for the answer.
#[derive(Default)]
pub struct HttpWire;

impl Wire for HttpWire {
    fn call(&mut self, request: Request) -> BoxFuture<'_, Result<Reply, TransportError>> {
        Box::pin(async move { send(&request) })
    }
}

/// One request, sent and read.
fn send(request: &Request) -> Result<Reply, TransportError> {
    // A non-2xx answer is a reply to read, not an error: the status and the body say what
    // happened, and `status_error` turns them into the message the person sees.
    let agent = ureq::Agent::new_with_config(ureq::config::Config::builder().http_status_as_error(false).build());
    // `ureq`'s builders are typed by whether a body is attached, so the two shapes are built
    // apart rather than made to meet.
    let sent = match (&request.method, &request.body) {
        (Method::Post, Some(body)) => {
            let mut outgoing = agent.post(&request.url);
            for (name, value) in &request.headers {
                outgoing = outgoing.header(*name, value.as_str());
            }
            outgoing.send_json(body)
        }
        (_, None) => {
            let mut outgoing = agent.get(&request.url);
            for (name, value) in &request.headers {
                outgoing = outgoing.header(*name, value.as_str());
            }
            outgoing.call()
        }
        // A POST with no body is not something the media pipeline asks for; the GET path is the
        // same request without one.
        (Method::Get, Some(_)) => return Err(TransportError::Other("a GET cannot carry a body".into())),
    };
    let response = sent.map_err(|e| crate::transport_error(&e.to_string()))?;
    let status = response.status().as_u16();
    let body = match request.accept {
        Accept::Json => response.into_body().read_to_string().map_err(|_| TransportError::Network)?.into_bytes(),
        Accept::Bytes => {
            // The declared length is a hint, not a promise: read one byte past the cap so a
            // lying server is detected, and never let the body grow without bound.
            let cap = (figcraft_ai::MAX_ASSET_BYTES + 1) as u64;
            use std::io::Read as _;
            let mut bytes = Vec::new();
            response.into_body().into_reader().take(cap).read_to_end(&mut bytes).map_err(|_| TransportError::Network)?;
            bytes
        }
    };
    Ok(Reply { status, body })
}

/// The desktop build's media factory.
#[derive(Default)]
pub struct MediaClientFactory;

impl MediaFactory for MediaClientFactory {
    fn make(
        &self,
        api_key: Option<String>,
        stock_key: Option<String>,
        vision: Option<figcraft_ai::VisionConfig>,
        vectorize: Option<(figcraft_ai::VectorizeProvider, String)>,
    ) -> Box<dyn MediaSession> {
        Box::new(WireMedia::new(HttpWire, api_key, stock_key, vision, vectorize))
    }
}

/// The JSON a `POST` body serializes to, for the tests that read one.
#[cfg(test)]
fn body_of(request: &Request) -> serde_json::Value {
    request.body.clone().unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use figcraft_ai::media::Orientation;

    #[test]
    fn a_get_carries_its_headers() {
        let request = super::super::download_request("https://example.test/a.png");
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.accept, Accept::Bytes);
        assert!(request.headers.iter().any(|(name, value)| *name == "user-agent" && value.contains("FigCraft")));
    }

    #[test]
    fn the_factory_makes_a_session_with_the_keys_it_was_given() {
        // A keyless session still exists; it says what it cannot do rather than panicking.
        let mut session = MediaClientFactory.make(None, None, None, None);
        let err = figcraft_ai::media::block_on(session.generate_image("a fox", Orientation::Square)).expect_err("no key");
        assert!(err.message().contains("key"), "{}", err.message());
    }

    #[test]
    fn a_generation_request_is_a_json_post_with_the_key() {
        let request = super::super::generate_request("sk-test", "a fox", Orientation::Portrait);
        assert_eq!(request.method, Method::Post);
        assert_eq!(request.accept, Accept::Json);
        assert!(request.headers.iter().any(|(name, value)| *name == "authorization" && value == "Bearer sk-test"));
        assert_eq!(body_of(&request)["prompt"], "a fox");
        assert_eq!(body_of(&request)["size"], "1024x1536");
    }
}
