//! The media the agent can make or fetch: generated images and stock photos.
//!
//! Like the transport and the MCP sessions, the trait is the seam and the desktop build fills
//! it: an image is asked for on a worker thread, and what comes back are the bytes the document
//! places. A stock photo needs a key of its own (the stock services are not the model's
//! provider); a generated image uses the connection the design model already has.

use crate::transport::TransportError;
use std::future::Future;
use std::pin::Pin;

/// A boxed future, so a session can be a seam without an async runtime.
///
/// The desktop build's session finishes on its first poll (it blocks inside a worker thread);
/// the page's resolves when `fetch` does, driven by the browser's event loop. Neither needs an
/// executor of its own: whoever awaits it already has one — a thread that may block, or the page.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Run a future to completion on this thread.
///
/// The desktop's media session blocks inside its own request, so every future here is ready on
/// its first poll and this never parks. It exists so the desktop and the page can share one
/// async pipeline: the page drives the same future with `spawn_local`, this drives it here.
///
/// Not available in the browser: a page must never block, and it has no threads to park.
#[cfg(not(target_arch = "wasm32"))]
pub fn block_on<F: Future>(future: F) -> F::Output {
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    /// Wake the thread that is waiting on this future.
    struct ThreadWaker(std::thread::Thread);

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }

    let mut future = std::pin::pin!(future);
    let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park(),
        }
    }
}

/// The shape an image is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    /// Wider than tall.
    Landscape,
    /// Taller than wide.
    Portrait,
    /// Square.
    Square,
}

impl Orientation {
    /// The orientation's id, as the tools name it.
    pub fn id(self) -> &'static str {
        match self {
            Self::Landscape => "landscape",
            Self::Portrait => "portrait",
            Self::Square => "square",
        }
    }

    /// The orientation with this id; anything else is landscape.
    pub fn from_id(id: &str) -> Self {
        match id {
            "portrait" => Self::Portrait,
            "square" => Self::Square,
            _ => Self::Landscape,
        }
    }
}

/// One stock photo a search found.
#[derive(Clone, Debug, PartialEq)]
pub struct StockPhoto {
    /// Where the photo itself is fetched from.
    pub image_url: String,
    /// The page the photo came from, for the credit.
    pub source_page: String,
    /// Who took it.
    pub photographer: String,
    /// Its size in pixels.
    pub width: u32,
    pub height: u32,
    /// The service that answered.
    pub provider: String,
}

/// What the media session offers.
///
/// Every call is a future because the page has no threads to block: a search, a download and a
/// Vision call are all a `fetch` whose promise the event loop drives. The desktop's session is
/// ready on the first poll (see [`block_on`]), so the same pipeline runs in both builds.
pub trait MediaSession: Send {
    /// Make an image: the picture's bytes, as PNG.
    fn generate_image<'a>(&'a mut self, prompt: &'a str, orientation: Orientation) -> BoxFuture<'a, Result<Vec<u8>, TransportError>>;

    /// Search the public libraries that need no key (Openverse and Wikimedia Commons).
    ///
    /// A failed provider contributes an outcome with an `error`; the other providers' results
    /// still stand. Providers that are not configured here return an empty list, never a failure.
    fn search_assets<'a>(&'a mut self, query: &'a str, options: &'a crate::AssetSearchOptions) -> BoxFuture<'a, Vec<crate::AssetSearchOutcome>>;

    /// Search a stock service, and read the photo at `index` of what it found.
    fn stock_photo<'a>(&'a mut self, query: &'a str, orientation: Orientation, index: usize) -> BoxFuture<'a, Result<StockPhoto, TransportError>>;

    /// Fetch a photo's bytes from its URL, after checking the URL is safe.
    fn download<'a>(&'a mut self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, TransportError>>;

    /// Ask the Vision model to inspect actual image bytes, with a prompt.
    ///
    /// The answer is the model's plain text; the caller parses it into a verdict. A build with
    /// no Vision model configured returns an error, which is a failed inspection, never a pass.
    fn inspect_image<'a>(&'a mut self, bytes: &'a [u8], mime: &'a str, prompt: &'a str) -> BoxFuture<'a, Result<String, TransportError>>;

    /// Vectorize an image (PNG bytes), returning SVG text. Uses the configured vectorization
    /// service and its key.
    fn vectorize<'a>(&'a mut self, png: &'a [u8]) -> BoxFuture<'a, Result<String, TransportError>>;

    /// Remove an image's background (PNG bytes in, transparent PNG bytes out). Uses the design
    /// provider's image-edit model and its key. The caller places the result as a new layer.
    fn remove_background<'a>(&'a mut self, png: &'a [u8]) -> BoxFuture<'a, Result<Vec<u8>, TransportError>>;
}

/// The factory a worker thread makes its media session from, keyed by the key it uses.
pub trait MediaFactory: Send + Sync {
    /// A session for the design provider's image model (`api_key`: that provider's key), and
    /// one for the stock services (`stock_key`: the Pexels key, when there is one).
    fn make(
        &self,
        api_key: Option<String>,
        stock_key: Option<String>,
        vision: Option<VisionConfig>,
        vectorize: Option<(crate::VectorizeProvider, String)>,
    ) -> Box<dyn MediaSession>;
}

/// Where the Vision model lives, for inspecting images before they are placed.
#[derive(Clone, Debug, PartialEq)]
pub struct VisionConfig {
    /// The provider's endpoint.
    pub base_url: String,
    /// The provider's key.
    pub api_key: String,
    /// The protocol the endpoint speaks.
    pub protocol: crate::providers::Protocol,
    /// The model id to call.
    pub model: String,
}

/// The factory with nothing behind it.
#[derive(Default)]
pub struct NoMedia;

impl MediaFactory for NoMedia {
    fn make(
        &self,
        _api_key: Option<String>,
        _stock_key: Option<String>,
        _vision: Option<VisionConfig>,
        _vectorize: Option<(crate::VectorizeProvider, String)>,
    ) -> Box<dyn MediaSession> {
        Box::new(NoMediaSession)
    }
}

/// The session the default factory makes: there is nothing here.
#[derive(Default)]
struct NoMediaSession;

impl MediaSession for NoMediaSession {
    fn generate_image<'a>(&'a mut self, _prompt: &'a str, _orientation: Orientation) -> BoxFuture<'a, Result<Vec<u8>, TransportError>> {
        Box::pin(async { Err(TransportError::Other("this build has no image generation".into())) })
    }

    fn search_assets<'a>(&'a mut self, _query: &'a str, _options: &'a crate::AssetSearchOptions) -> BoxFuture<'a, Vec<crate::AssetSearchOutcome>> {
        Box::pin(async { Vec::new() })
    }

    fn stock_photo<'a>(&'a mut self, _query: &'a str, _orientation: Orientation, _index: usize) -> BoxFuture<'a, Result<StockPhoto, TransportError>> {
        Box::pin(async { Err(TransportError::Other("this build has no stock photo service".into())) })
    }

    fn download<'a>(&'a mut self, _url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, TransportError>> {
        Box::pin(async { Err(TransportError::Other("this build cannot fetch images".into())) })
    }

    fn inspect_image<'a>(&'a mut self, _bytes: &'a [u8], _mime: &'a str, _prompt: &'a str) -> BoxFuture<'a, Result<String, TransportError>> {
        Box::pin(async { Err(TransportError::Other("this build has no Vision model".into())) })
    }

    fn vectorize<'a>(&'a mut self, _png: &'a [u8]) -> BoxFuture<'a, Result<String, TransportError>> {
        Box::pin(async { Err(TransportError::Other("this build cannot vectorize images".into())) })
    }

    fn remove_background<'a>(&'a mut self, _png: &'a [u8]) -> BoxFuture<'a, Result<Vec<u8>, TransportError>> {
        Box::pin(async { Err(TransportError::Other("this build cannot remove image backgrounds".into())) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_orientation_round_trips_through_its_id() {
        for o in [Orientation::Landscape, Orientation::Portrait, Orientation::Square] {
            assert_eq!(Orientation::from_id(o.id()), o);
        }
        // Anything unreadable is landscape, the default.
        assert_eq!(Orientation::from_id("nope"), Orientation::Landscape);
    }

    #[test]
    fn the_default_factory_says_what_it_cannot_do() {
        let mut session = NoMedia.make(None, None, None, None);
        let err = block_on(session.generate_image("a fox", Orientation::Square)).expect_err("no generation here");
        assert!(err.message().contains("build"), "{}", err.message());
        let err = block_on(session.remove_background(b"png")).expect_err("no removal here");
        assert!(err.message().contains("build"), "{}", err.message());
        let options =
            crate::AssetSearchOptions { limit: 5, orientation: Orientation::Landscape, allow_non_commercial: false, require_modification: true };
        assert!(block_on(session.search_assets("a fox", &options)).is_empty(), "no providers in this build");
    }
}
