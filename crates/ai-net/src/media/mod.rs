//! The media half of the AI transport: image search, image generation, downloads and the
//! Vision call, split so both builds share one protocol.
//!
//! Two builds, one pipeline. The desktop build sends over HTTPS with a blocking client
//! ([`HttpWire`](crate::media::HttpWire)); the browser build cannot block, so it sends through
//! the page's own `fetch` ([`BrowserWire`](crate::media::BrowserWire)) and its promise is driven
//! by the event loop. Both are a [`Wire`]: one request in, one reply out, as a future. Everything
//! else — which URL a search goes to, what a request body says, how an answer is read, which
//! candidates a round offers — lives here and is tested in every build, so a search that works in
//! the app works in the page.
//!
//! The pieces that are pure are public: a URL, a body and a parser can be checked without a
//! socket or a browser.

use base64::Engine as _;
use figcraft_ai::media::BoxFuture;
use figcraft_ai::transport::TransportError;
use figcraft_ai::{AssetSearchOptions, AssetSearchOutcome, ImageAsset, MediaSession, Orientation, StockPhoto, VectorizeProvider, VisionConfig};
use serde_json::{Value, json};

#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(not(target_arch = "wasm32"))]
mod http;

#[cfg(target_arch = "wasm32")]
pub use browser::{BrowserMediaFactory, BrowserWire};
#[cfg(not(target_arch = "wasm32"))]
pub use http::{HttpWire, MediaClientFactory};

/// How a request leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// What the caller expects back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accept {
    /// A JSON document, read as text.
    Json,
    /// The body itself, as bytes.
    Bytes,
}

/// One request the media pipeline makes.
///
/// A wire does not decide anything: it sends exactly this and returns what came back. That is
/// what makes the two builds comparable — the same `Request` on both sides is the same HTTP
/// request, and a header added here reaches both.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<Value>,
    pub accept: Accept,
}

impl Request {
    /// A `GET` for a JSON document.
    pub fn get_json(url: impl Into<String>) -> Self {
        Self { method: Method::Get, url: url.into(), headers: Vec::new(), body: None, accept: Accept::Json }
    }

    /// A `GET` for bytes.
    pub fn get_bytes(url: impl Into<String>) -> Self {
        Self { method: Method::Get, url: url.into(), headers: Vec::new(), body: None, accept: Accept::Bytes }
    }

    /// A `POST` of a JSON body, answered with JSON.
    pub fn post_json(url: impl Into<String>, body: Value) -> Self {
        let mut request = Self { method: Method::Post, url: url.into(), headers: Vec::new(), body: Some(body), accept: Accept::Json };
        request.headers.push(("content-type", "application/json".into()));
        request
    }

    /// The same request with one more header.
    pub fn header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }
}

/// What came back, whatever the status.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Reply {
    /// A successful reply, for tests and for a wire that has the bytes already.
    pub fn ok(body: Vec<u8>) -> Self {
        Self { status: 200, body }
    }

    /// The body as text, lossily: a provider's error page is often not valid UTF-8, and the words
    /// are still worth reading.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// The body as JSON.
    pub fn json(&self) -> Result<Value, TransportError> {
        serde_json::from_slice(&self.body).map_err(|e| TransportError::BadResponse(e.to_string()))
    }

    /// Whether the request succeeded.
    pub fn is_ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// What a provider's failure means, from its status and its words.
///
/// Shared by both builds: a 401 says "check the key" in the app and in the page alike.
pub fn status_error(status: u16, detail: &str) -> TransportError {
    let detail = detail.trim();
    if detail.is_empty() { crate::transport_error(&status.to_string()) } else { crate::transport_error(&format!("{status} {detail}")) }
}

/// The socket half, as each build has it.
///
/// A page has no threads, so this is a future rather than a call: the desktop's is ready on the
/// first poll, and the page's resolves when `fetch` does.
pub trait Wire: Send {
    /// Send one request and read the reply.
    fn call(&mut self, request: Request) -> BoxFuture<'_, Result<Reply, TransportError>>;
}

/// A media session over a wire.
///
/// The keys are the ones the caller was given: the design provider's for its image model, the
/// stock service's for its search, and the Vision model's for inspecting what a search found.
pub struct WireMedia<W: Wire> {
    wire: W,
    api_key: Option<String>,
    stock_key: Option<String>,
    vision: Option<VisionConfig>,
    vectorize: Option<(VectorizeProvider, String)>,
}

impl<W: Wire> WireMedia<W> {
    /// A session with its wire and the keys it was given.
    pub fn new(
        wire: W,
        api_key: Option<String>,
        stock_key: Option<String>,
        vision: Option<VisionConfig>,
        vectorize: Option<(VectorizeProvider, String)>,
    ) -> Self {
        Self { wire, api_key, stock_key, vision, vectorize }
    }

    /// A key, when it is present and not blank.
    fn key(key: &Option<String>) -> Option<String> {
        key.as_ref().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
    }
}

impl<W: Wire> MediaSession for WireMedia<W> {
    fn search_assets<'a>(&'a mut self, query: &'a str, options: &'a AssetSearchOptions) -> BoxFuture<'a, Vec<AssetSearchOutcome>> {
        Box::pin(async move {
            let mut outcomes = Vec::new();
            for provider in search_requests(query, options) {
                let Provider { name, request, entries, map } = provider;
                let outcome = match self.wire.call(request).await {
                    Ok(reply) if reply.is_ok() => match reply.json() {
                        Ok(payload) => {
                            AssetSearchOutcome { provider: name.into(), assets: entries(&payload).iter().filter_map(map).collect(), error: None }
                        }
                        Err(e) => AssetSearchOutcome { provider: name.into(), assets: Vec::new(), error: Some(e.message()) },
                    },
                    Ok(reply) => {
                        AssetSearchOutcome { provider: name.into(), assets: Vec::new(), error: Some(format!("{} from {name}", reply.status)) }
                    }
                    Err(e) => AssetSearchOutcome { provider: name.into(), assets: Vec::new(), error: Some(e.message()) },
                };
                outcomes.push(outcome);
            }
            outcomes
        })
    }

    fn generate_image<'a>(&'a mut self, prompt: &'a str, orientation: Orientation) -> BoxFuture<'a, Result<Vec<u8>, TransportError>> {
        Box::pin(async move {
            let Some(key) = Self::key(&self.api_key) else {
                return Err(TransportError::Other("The connection has no API key, so no image can be generated.".into()));
            };
            let reply = self.wire.call(generate_request(&key, prompt, orientation)).await?;
            if !reply.is_ok() {
                return Err(status_error(reply.status, &reply.text()));
            }
            decode_image(&reply.json()?)
        })
    }

    fn remove_background<'a>(&'a mut self, png: &'a [u8]) -> BoxFuture<'a, Result<Vec<u8>, TransportError>> {
        Box::pin(async move {
            let Some(key) = Self::key(&self.api_key) else {
                return Err(TransportError::Other("The connection has no API key, so the background cannot be removed.".into()));
            };
            let reply = self.wire.call(remove_background_request(&key, png)).await?;
            if !reply.is_ok() {
                return Err(status_error(reply.status, &reply.text()));
            }
            decode_image(&reply.json()?)
        })
    }

    fn stock_photo<'a>(&'a mut self, query: &'a str, orientation: Orientation, index: usize) -> BoxFuture<'a, Result<StockPhoto, TransportError>> {
        Box::pin(async move {
            let Some(key) = Self::key(&self.stock_key) else {
                return Err(TransportError::Other(
                    "No stock photo key is saved, so the keyed service cannot be used. Call source_image instead: it searches \
                     Openverse and Wikimedia Commons, which need no key, and verifies what it finds with the Vision model."
                        .into(),
                ));
            };
            let reply = self.wire.call(stock_request(&key, query, orientation, index)).await?;
            if !reply.is_ok() {
                return Err(status_error(reply.status, &reply.text()));
            }
            pick_photo(&reply.json()?, query, index)
        })
    }

    fn download<'a>(&'a mut self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, TransportError>> {
        Box::pin(async move {
            // A provider supplies this URL, so validate it before any request is made. A private
            // address or an embedded credential is refused outright (SSRF).
            figcraft_ai::validate_remote_url(url).map_err(TransportError::Other)?;
            let reply = self.wire.call(download_request(url)).await?;
            if !reply.is_ok() {
                return Err(TransportError::BadResponse(format!("{} fetching the image", reply.status)));
            }
            check_image_bytes(reply.body)
        })
    }

    fn inspect_image<'a>(&'a mut self, bytes: &'a [u8], mime: &'a str, prompt: &'a str) -> BoxFuture<'a, Result<String, TransportError>> {
        Box::pin(async move {
            let Some(vision) = self.vision.clone() else {
                return Err(TransportError::Other("no Vision model is configured; add one in AI & Agents".into()));
            };
            let reply = self.wire.call(vision_request(&vision, bytes, mime, prompt)?).await?;
            if !reply.is_ok() {
                return Err(status_error(reply.status, &reply.text()));
            }
            vision_text(&reply.json()?)
        })
    }

    fn vectorize<'a>(&'a mut self, png: &'a [u8]) -> BoxFuture<'a, Result<String, TransportError>> {
        Box::pin(async move {
            let Some((provider, key)) = self.vectorize.clone() else {
                return Err(TransportError::Other("no vectorization service is configured; add a key in AI & Agents".into()));
            };
            let reply = self.wire.call(vectorize_request(provider, &key, png)).await?;
            if !reply.is_ok() {
                return Err(status_error(reply.status, &reply.text()));
            }
            svg_of(&reply.json()?)
        })
    }
}

impl<W: Wire> Wire for &mut W {
    fn call(&mut self, request: Request) -> BoxFuture<'_, Result<Reply, TransportError>> {
        (**self).call(request)
    }
}

/// One provider of a search round: where its request goes and how its answer is read.
pub struct Provider {
    /// The provider's name, as the outcome reports it.
    pub name: &'static str,
    pub request: Request,
    /// The provider's results, in its own shape.
    pub entries: fn(&Value) -> Vec<Value>,
    /// One result, as a normalized asset.
    pub map: fn(&Value) -> Option<ImageAsset>,
}

/// The two keyless libraries, and the exact request each one takes.
///
/// Both are public and ask clients to identify themselves; neither needs a key, which is why a
/// screen can be given real photography with nothing configured.
pub fn search_requests(query: &str, options: &AssetSearchOptions) -> Vec<Provider> {
    let limit = options.limit.min(20);
    let orientation = match options.orientation {
        Orientation::Landscape => "wide",
        Orientation::Portrait => "tall",
        Orientation::Square => "square",
    };
    // The two licence filters are combined into one `license_type` value, as the API expects.
    let mut license_types = Vec::new();
    if !options.allow_non_commercial {
        license_types.push("commercial");
    }
    if options.require_modification {
        license_types.push("modification");
    }
    let mut openverse =
        vec![("q", query.to_string()), ("page_size", limit.to_string()), ("mature", "false".into()), ("aspect_ratio", orientation.into())];
    if !license_types.is_empty() {
        openverse.push(("license_type", license_types.join(",")));
    }
    let openverse = Request::get_json(query_url(figcraft_ai::OPENVERSE_ENDPOINT, &openverse))
        .header("accept", "application/json")
        .header("user-agent", figcraft_ai::USER_AGENT);

    // Wikimedia Commons: the MediaWiki API, also public and keyless. `origin=*` is what lets a
    // page read the answer: without it the browser refuses the response as cross-origin.
    let commons = Request::get_json(query_url(
        figcraft_ai::WIKIMEDIA_ENDPOINT,
        &[
            ("action", "query".into()),
            ("generator", "search".into()),
            ("gsrsearch", format!("{query} filetype:bitmap")),
            ("gsrnamespace", "6".into()),
            ("gsrlimit", limit.to_string()),
            ("prop", "imageinfo".into()),
            ("iiprop", "url|mime|size|extmetadata".into()),
            ("iiurlwidth", "960".into()),
            ("format", "json".into()),
            ("origin", "*".into()),
        ],
    ))
    .header("accept", "application/json")
    .header("user-agent", figcraft_ai::USER_AGENT);

    vec![
        // Commons first: it answers a browser with `origin=*`, so a page can read the result.
        // Openverse refuses a browser's request outright — a throttled answer carries no
        // `Access-Control-Allow-Origin`, so every search from the page fails as a CORS block —
        // and asking it first spent each slot's one search on a provider that could not answer.
        Provider { name: "wikimedia", request: commons, entries: commons_entries, map: map_commons },
        Provider { name: "openverse", request: openverse, entries: openverse_entries, map: map_openverse },
    ]
}

/// A URL with its query string, encoded the way a browser and a client both read it.
pub fn query_url(base: &str, params: &[(&str, String)]) -> String {
    let Ok(mut url) = url::Url::parse(base) else { return base.to_string() };
    {
        let mut pairs = url.query_pairs_mut();
        for (name, value) in params {
            pairs.append_pair(name, value);
        }
    }
    url.to_string()
}

/// The GPT Image model used for generation and edits. The public OpenAI Images API retired
/// `gpt-image-1` in 2026; this is a current model that supports transparent backgrounds.
const IMAGE_MODEL: &str = "gpt-image-2.5-flare";

/// Where an image is generated.
const GENERATE_URL: &str = "https://api.openai.com/v1/images/generations";

/// Where an image is edited (a background removed).
const EDIT_URL: &str = "https://api.openai.com/v1/images/edits";

/// Where a stock photo is searched.
const STOCK_URL: &str = "https://api.pexels.com/v1/search";

/// The size an image is asked for, from its orientation.
fn size_of(orientation: Orientation) -> &'static str {
    match orientation {
        Orientation::Landscape => "1536x1024",
        Orientation::Portrait => "1024x1536",
        Orientation::Square => "1024x1024",
    }
}

/// The request that makes one image.
pub fn generate_request(key: &str, prompt: &str, orientation: Orientation) -> Request {
    Request::post_json(GENERATE_URL, json!({"model": IMAGE_MODEL, "prompt": prompt, "size": size_of(orientation), "n": 1}))
        .header("authorization", format!("Bearer {key}"))
}

/// The request body for a background removal, as the public `images/edits` API defines it: a
/// base64 data URL for the source image and a prompt asking for transparency. Kept separate from
/// the network call so its shape is tested without a key.
pub fn remove_background_body(png: &[u8]) -> Value {
    let data_url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png));
    json!({
        "model": IMAGE_MODEL,
        "images": [{"image_url": data_url}],
        "prompt": "Remove the background of this image. Keep the main subject exactly as it is and make the background fully transparent.",
        "background": "transparent",
        "output_format": "png",
        "n": 1,
    })
}

/// The request that removes one image's background.
pub fn remove_background_request(key: &str, png: &[u8]) -> Request {
    Request::post_json(EDIT_URL, remove_background_body(png)).header("authorization", format!("Bearer {key}"))
}

/// The request that searches a stock service.
pub fn stock_request(key: &str, query: &str, orientation: Orientation, index: usize) -> Request {
    // A few more than the one asked for, so a refetch at a neighbouring index works.
    let per_page = (index + 3).min(15);
    let url =
        query_url(STOCK_URL, &[("query", query.to_string()), ("per_page", per_page.to_string()), ("orientation", orientation.id().to_string())]);
    Request::get_json(url).header("authorization", key.to_string())
}

/// The request that fetches an image's bytes.
pub fn download_request(url: &str) -> Request {
    Request::get_bytes(url).header("accept", "image/*,*/*;q=0.8").header("user-agent", figcraft_ai::USER_AGENT)
}

/// The request that asks the Vision model about one image, in the protocol its endpoint speaks.
pub fn vision_request(vision: &VisionConfig, bytes: &[u8], mime: &str, prompt: &str) -> Result<Request, TransportError> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let base = vision.base_url.trim_end_matches('/');
    let request = match vision.protocol {
        figcraft_ai::providers::Protocol::Openai => Request::post_json(
            format!("{base}/chat/completions"),
            json!({
                "model": vision.model,
                "max_tokens": 800,
                "messages": [{"role": "user", "content": [
                    {"type": "text", "text": prompt},
                    {"type": "image_url", "image_url": {"url": format!("data:{mime};base64,{b64}")}},
                ]}],
            }),
        )
        .header("authorization", format!("Bearer {}", vision.api_key)),
        figcraft_ai::providers::Protocol::Anthropic => Request::post_json(
            format!("{base}/messages"),
            json!({
                "model": vision.model,
                "max_tokens": 800,
                "messages": [{"role": "user", "content": [
                    {"type": "text", "text": prompt},
                    {"type": "image", "source": {"type": "base64", "media_type": mime, "data": b64}},
                ]}],
            }),
        )
        .header("x-api-key", vision.api_key.clone())
        .header("anthropic-version", "2023-06-01"),
        figcraft_ai::providers::Protocol::Google => Request::post_json(
            format!("{base}/models/{}:generateContent", vision.model),
            json!({"contents": [{"parts": [
                {"text": prompt},
                {"inline_data": {"mime_type": mime, "data": b64}},
            ]}]}),
        )
        .header("x-goog-api-key", vision.api_key.clone()),
        _ => return Err(TransportError::Other("this provider cannot inspect images".into())),
    };
    Ok(request)
}

/// The request that vectorizes one image.
pub fn vectorize_request(provider: VectorizeProvider, key: &str, png: &[u8]) -> Request {
    let b64 = base64::engine::general_purpose::STANDARD.encode(png);
    let (body, auth) = match provider {
        VectorizeProvider::Recraft => (json!({"image": format!("data:image/png;base64,{b64}")}), format!("Bearer {key}")),
        VectorizeProvider::Fal => (json!({"image_url": format!("data:image/png;base64,{b64}")}), format!("Key {key}")),
    };
    Request::post_json(provider.endpoint(), body).header("authorization", auth)
}

/// The image bytes in a generation or edit answer.
pub fn decode_image(answer: &Value) -> Result<Vec<u8>, TransportError> {
    let b64 =
        answer.pointer("/data/0/b64_json").and_then(Value::as_str).ok_or_else(|| TransportError::BadResponse("the answer held no image".into()))?;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| TransportError::BadResponse(format!("the image's bytes could not be read: {e}")))
}

/// The Vision model's plain text, whichever protocol answered.
pub fn vision_text(answer: &Value) -> Result<String, TransportError> {
    let text = answer
        .pointer("/choices/0/message/content")
        .or_else(|| answer.pointer("/content/0/text"))
        .or_else(|| answer.pointer("/candidates/0/content/parts/0/text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if text.is_empty() {
        return Err(TransportError::BadResponse("the Vision model returned no text".into()));
    }
    Ok(text)
}

/// The SVG a vectorizer answered with. Recraft returns `{svg: "..."}`; fal returns
/// `{svg: {url}}`, which the caller fetches separately.
pub fn svg_of(answer: &Value) -> Result<String, TransportError> {
    answer
        .get("svg")
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) => o.get("url").and_then(Value::as_str).map(str::to_string),
            _ => None,
        })
        .ok_or_else(|| TransportError::BadResponse("the vectorizer returned no SVG".into()))
}

/// The photo at `index` of a stock search's answer.
pub fn pick_photo(answer: &Value, query: &str, index: usize) -> Result<StockPhoto, TransportError> {
    let photos = answer.get("photos").and_then(Value::as_array).ok_or_else(|| TransportError::BadResponse("the answer held no photos".into()))?;
    let photo = photos.get(index).or_else(|| photos.first()).ok_or_else(|| TransportError::Other(format!("No photo was found for \"{query}\".")))?;
    Ok(StockPhoto {
        image_url: pick_size(photo),
        source_page: photo.get("url").and_then(Value::as_str).unwrap_or_default().to_string(),
        photographer: photo.pointer("/photographer/name").and_then(Value::as_str).unwrap_or_default().to_string(),
        width: photo.get("width").and_then(Value::as_u64).unwrap_or(0) as u32,
        height: photo.get("height").and_then(Value::as_u64).unwrap_or(0) as u32,
        provider: "pexels".into(),
    })
}

/// The largest size of a stock photo that is still sensible for a screen.
fn pick_size(photo: &Value) -> String {
    let sizes = photo.get("src").and_then(Value::as_object).cloned().unwrap_or_default();
    let width = photo.get("width").and_then(Value::as_u64).unwrap_or(0);
    let height = photo.get("height").and_then(Value::as_u64).unwrap_or(0);
    for (key, cap) in [("large2x", 1600), ("large", 800), ("medium", 400)] {
        if let Some(url) = sizes.get(key).and_then(Value::as_str)
            && width.max(height) <= cap * 2
        {
            return url.to_string();
        }
    }
    sizes.get("large").and_then(Value::as_str).unwrap_or_default().to_string()
}

/// A downloaded body, checked as an image before anything tries to place it.
///
/// The bytes are what counts, not the file name or the server's content-type header: a page that
/// answers an HTML error with a `200` is caught here.
pub fn check_image_bytes(bytes: Vec<u8>) -> Result<Vec<u8>, TransportError> {
    if bytes.len() > figcraft_ai::MAX_ASSET_BYTES {
        return Err(TransportError::Other(format!("the image is larger than the {} byte limit", figcraft_ai::MAX_ASSET_BYTES)));
    }
    if figcraft_ai::assets::sniff_image_content_type(&bytes).is_none() {
        return Err(TransportError::BadResponse("the answer was not a recognised image".into()));
    }
    Ok(bytes)
}

/// Read a non-empty string out of an untrusted provider payload.
fn read_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Read a finite positive number out of an untrusted provider payload.
fn read_number(value: Option<&Value>) -> Option<u32> {
    value.and_then(Value::as_f64).filter(|n| n.is_finite() && *n > 0.0).map(|n| n as u32)
}

/// An id may arrive as a number (Commons `pageid`) or a string (Openverse).
fn read_id(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::Number(n)) => Some(n.to_string()),
        other => read_string(other),
    }
}

/// The MIME type Openverse's short filetype codes stand for.
fn mime_by_filetype(filetype: &str) -> Option<&'static str> {
    match filetype.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "tif" | "tiff" => Some("image/tiff"),
        _ => None,
    }
}

/// The kind of asset a MIME type is.
fn kind_of(mime_type: Option<&str>) -> figcraft_ai::AssetKind {
    if mime_type == Some("image/svg+xml") {
        figcraft_ai::AssetKind::Svg
    } else if mime_type.is_some() {
        figcraft_ai::AssetKind::Raster
    } else {
        figcraft_ai::AssetKind::Unknown
    }
}

/// Turn one Openverse result into a normalized asset. Unusable entries return `None`.
pub fn map_openverse(value: &Value) -> Option<ImageAsset> {
    let url = read_string(value.get("url"))?;
    let license = read_string(value.get("license")).map(|s| s.to_ascii_lowercase());
    let license_code = license.as_deref().unwrap_or("");
    let creator = read_string(value.get("creator"));
    let title = read_string(value.get("title"));
    let attribution = read_string(value.get("attribution"))
        .or_else(|| creator.as_ref().map(|c| format!("{} by {c}", title.as_deref().unwrap_or("Untitled"))).or(title.clone()));
    let tags = value
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|t| read_string(t.get("name"))).collect::<Vec<_>>().join(", "))
        .filter(|s| !s.is_empty());
    let description = tags.or_else(|| read_string(value.get("category")));
    let mime_type = read_string(value.get("filetype")).and_then(|f| mime_by_filetype(&f).map(str::to_string));
    Some(ImageAsset {
        id: read_string(value.get("id")).unwrap_or_else(|| url.clone()),
        provider: "openverse".into(),
        url: figcraft_ai::assets::canonicalize_url(&url),
        thumbnail_url: read_string(value.get("thumbnail")),
        title,
        description,
        kind: kind_of(mime_type.as_deref()),
        mime_type,
        width: read_number(value.get("width")),
        height: read_number(value.get("height")),
        creator,
        source_page: read_string(value.get("foreign_landing_url")),
        license: license.clone(),
        license_url: read_string(value.get("license_url")),
        attribution,
        commercial_use: figcraft_ai::license_allows_commercial_use(license_code),
        modification_allowed: figcraft_ai::license_allows_modification(license_code),
        requires_attribution: figcraft_ai::license_requires_attribution(license_code),
    })
}

/// Read one Wikimedia `extmetadata` value (the API wraps it in `{ value: … }`).
fn commons_meta(info: &Value, key: &str) -> Option<String> {
    info.get("extmetadata").and_then(|m| m.get(key)).and_then(|v| v.get("value")).and_then(|v| read_string(Some(v)))
}

/// Turn one Commons page into a normalized asset. Unusable pages return `None`.
pub fn map_commons(page: &Value) -> Option<ImageAsset> {
    let info = page.get("imageinfo").and_then(Value::as_array).and_then(|a| a.first())?;
    let url = read_string(info.get("url"))?;
    let mime_type = read_string(info.get("mime"));
    let license_code = commons_meta(info, "License").map(|s| s.to_ascii_lowercase());
    let license_name = commons_meta(info, "LicenseShortName");
    let license = license_code.clone().or(license_name);
    let creator = commons_meta(info, "Artist").map(|s| figcraft_ai::assets::strip_html(&s));
    let title = read_string(page.get("title"))
        .map(|t| t.trim_start_matches("File:").trim().rsplit_once('.').map(|(stem, _)| stem.trim().to_string()).unwrap_or(t));
    let attribution = creator.as_ref().map(|c| format!("{} — {c}", title.as_deref().unwrap_or("Untitled"))).or_else(|| title.clone());
    Some(ImageAsset {
        id: read_id(page.get("pageid")).unwrap_or_else(|| url.clone()),
        provider: "wikimedia".into(),
        url: figcraft_ai::assets::canonicalize_url(&url),
        thumbnail_url: read_string(info.get("thumburl")),
        title,
        description: commons_meta(info, "ImageDescription").map(|s| figcraft_ai::assets::strip_html(&s)),
        kind: kind_of(mime_type.as_deref()),
        mime_type,
        width: read_number(info.get("width")),
        height: read_number(info.get("height")),
        creator,
        source_page: read_string(info.get("descriptionurl")),
        license,
        license_url: commons_meta(info, "LicenseUrl"),
        attribution,
        commercial_use: license_code.as_deref().map(figcraft_ai::license_allows_commercial_use).unwrap_or(true),
        modification_allowed: license_code.as_deref().map(figcraft_ai::license_allows_modification).unwrap_or(true),
        requires_attribution: commons_meta(info, "AttributionRequired").map(|s| s.eq_ignore_ascii_case("true")).unwrap_or(false),
    })
}

/// Openverse's results live under `results`, in rank order.
pub fn openverse_entries(payload: &Value) -> Vec<Value> {
    payload.get("results").and_then(Value::as_array).cloned().unwrap_or_default()
}

/// Commons returns pages in an arbitrary object order; `index` is the search rank.
pub fn commons_entries(payload: &Value) -> Vec<Value> {
    let mut pages: Vec<Value> =
        payload.get("query").and_then(|q| q.get("pages")).and_then(Value::as_object).map(|o| o.values().cloned().collect()).unwrap_or_default();
    pages.sort_by_key(|p| p.get("index").and_then(Value::as_u64).unwrap_or(u64::MAX));
    pages
}

#[cfg(test)]
mod tests {
    use super::*;
    use figcraft_ai::media::block_on;

    /// A wire that answers from a table, so a whole session is tested without a socket.
    #[derive(Default)]
    struct Fake {
        replies: Vec<Result<Reply, TransportError>>,
        seen: Vec<Request>,
    }

    impl Fake {
        fn with(replies: Vec<Result<Reply, TransportError>>) -> Self {
            Self { replies, seen: Vec::new() }
        }
    }

    impl Wire for Fake {
        fn call(&mut self, request: Request) -> BoxFuture<'_, Result<Reply, TransportError>> {
            self.seen.push(request);
            let reply = if self.replies.is_empty() { Err(TransportError::Network) } else { self.replies.remove(0) };
            Box::pin(async move { reply })
        }
    }

    fn options() -> AssetSearchOptions {
        AssetSearchOptions { limit: 4, orientation: Orientation::Landscape, allow_non_commercial: false, require_modification: true }
    }

    #[test]
    fn an_orientation_asks_for_its_own_size() {
        assert_eq!(size_of(Orientation::Landscape), "1536x1024");
        assert_eq!(size_of(Orientation::Portrait), "1024x1536");
        assert_eq!(size_of(Orientation::Square), "1024x1024");
    }

    #[test]
    fn a_query_is_encoded_once_and_in_order() {
        let url = query_url("https://example.test/v1/images/", &[("q", "a red lamp".into()), ("page_size", "4".into())]);
        assert_eq!(url, "https://example.test/v1/images/?q=a+red+lamp&page_size=4");
    }

    #[test]
    fn a_query_with_no_base_still_answers_something() {
        // A base that cannot be parsed is returned as it came, never dropped: the caller sees a
        // bad URL from the request rather than an empty string from here.
        assert_eq!(query_url("not a url", &[("q", "x".into())]), "not a url");
    }

    #[test]
    fn both_keyless_libraries_are_asked_and_commons_asks_for_cross_origin() {
        let providers = search_requests("a lamp", &options());
        assert_eq!(providers.len(), 2);
        // Commons is asked first: it answers a browser (`origin=*`), and Openverse's refusals
        // carry no CORS header, so a page could never read them.
        assert_eq!(providers[0].name, "wikimedia");
        assert_eq!(providers[1].name, "openverse");
        assert!(providers[0].request.url.contains("origin=*"), "{}", providers[0].request.url);
        assert!(providers[0].request.url.contains("filetype%3Abitmap"), "the search asks for bitmaps only");
        assert!(providers[1].request.url.starts_with("https://api.openverse.org/v1/images/?"));
        assert!(providers[1].request.url.contains("q=a+lamp"));
    }

    #[test]
    fn a_search_round_keeps_the_providers_that_answered() {
        let openverse = Reply::ok(
            serde_json::to_vec(&json!({"results": [{
                "id": "abc", "url": "https://example.test/lamp.jpg", "thumbnail": "https://example.test/t.jpg",
                "title": "Lamp", "creator": "Ada", "license": "by", "license_url": "https://example.test/lic",
                "filetype": "jpg", "width": 800, "height": 600, "foreign_landing_url": "https://example.test/page",
                "attribution": "Lamp by Ada",
            }]}))
            .unwrap_or_default(),
        );
        // Commons answers first, then Openverse.
        let mut wire = Fake::with(vec![Err(TransportError::Network), Ok(openverse)]);
        let mut session = WireMedia::new(&mut wire, None, None, None, None);
        let outcomes = block_on(session.search_assets("a lamp", &options()));
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].provider, "wikimedia");
        assert!(outcomes[0].error.is_some(), "the provider that failed says so");
        assert_eq!(outcomes[1].provider, "openverse");
        assert_eq!(outcomes[1].assets.len(), 1);
        assert_eq!(outcomes[1].assets[0].title.as_deref(), Some("Lamp"));
    }

    #[test]
    fn a_provider_that_answers_with_a_status_is_reported_not_swallowed() {
        let mut wire = Fake::with(vec![Ok(Reply { status: 200, body: b"{}".to_vec() }), Ok(Reply { status: 429, body: Vec::new() })]);
        let mut session = WireMedia::new(&mut wire, None, None, None, None);
        let outcomes = block_on(session.search_assets("a lamp", &options()));
        assert_eq!(outcomes[1].error.as_deref(), Some("429 from openverse"));
    }

    #[test]
    fn generation_without_a_key_says_why() {
        let mut wire = Fake::default();
        let mut session = WireMedia::new(&mut wire, Some("   ".into()), None, None, None);
        let err = block_on(session.generate_image("a fox", Orientation::Square)).expect_err("a blank key is no key");
        assert!(err.message().contains("API key"), "{}", err.message());
    }

    #[test]
    fn a_generated_image_is_decoded_from_its_base64() {
        let body = json!({"data": [{"b64_json": base64::engine::general_purpose::STANDARD.encode(b"png-bytes")}]});
        let mut wire = Fake::with(vec![Ok(Reply::ok(serde_json::to_vec(&body).unwrap_or_default()))]);
        let mut session = WireMedia::new(&mut wire, Some("sk-test".into()), None, None, None);
        let bytes = block_on(session.generate_image("a fox", Orientation::Square)).expect("an image");
        assert_eq!(bytes, b"png-bytes");
    }

    #[test]
    fn a_provider_error_becomes_a_readable_message() {
        let mut wire = Fake::with(vec![Ok(Reply { status: 401, body: b"{\"error\":\"bad key\"}".to_vec() })]);
        let mut session = WireMedia::new(&mut wire, Some("sk-test".into()), None, None, None);
        let err = block_on(session.generate_image("a fox", Orientation::Square)).expect_err("401");
        assert_eq!(err, TransportError::Authentication);
    }

    #[test]
    fn a_background_removal_body_asks_for_transparency() {
        let body = remove_background_body(b"png-bytes");
        assert_eq!(body["background"], "transparent");
        assert_eq!(body["output_format"], "png");
        assert_eq!(
            body["images"][0]["image_url"],
            format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b"png-bytes"))
        );
    }

    #[test]
    fn a_stock_photo_without_a_key_names_the_tool_that_works() {
        let mut wire = Fake::default();
        let mut session = WireMedia::new(&mut wire, None, None, None, None);
        let err = block_on(session.stock_photo("a lamp", Orientation::Landscape, 0)).expect_err("no stock key");
        assert!(err.message().contains("source_image"), "{}", err.message());
    }

    #[test]
    fn a_stock_photo_picks_the_size_that_fits_a_screen() {
        let answer = json!({
            "photos": [{
                "url": "https://example.test/page",
                "width": 1200, "height": 800,
                "photographer": {"name": "Ada"},
                "src": {"large2x": "https://example.test/2x.jpg", "large": "https://example.test/large.jpg", "medium": "https://example.test/medium.jpg"},
            }]
        });
        let photo = pick_photo(&answer, "a lamp", 0).expect("a photo");
        assert_eq!(photo.image_url, "https://example.test/2x.jpg");
        assert_eq!(photo.photographer, "Ada");
        assert_eq!(photo.provider, "pexels");
    }

    #[test]
    fn a_stock_answer_with_no_photos_is_an_error_not_a_panic() {
        assert!(pick_photo(&json!({"photos": []}), "a lamp", 0).is_err());
        assert!(pick_photo(&json!({}), "a lamp", 0).is_err());
        // An index past the end falls back to the first photo rather than failing a slot.
        let one = json!({"photos": [{"src": {"large": "https://example.test/a.jpg"}}]});
        assert_eq!(pick_photo(&one, "a lamp", 7).map(|p| p.image_url).unwrap_or_default(), "https://example.test/a.jpg");
    }

    #[test]
    fn a_download_checks_the_url_before_asking() {
        let mut wire = Fake::default();
        let mut session = WireMedia::new(&mut wire, None, None, None, None);
        let err = block_on(session.download("http://127.0.0.1/secret.png")).expect_err("a private address is refused");
        assert!(!err.message().is_empty());
        assert!(wire.seen.is_empty(), "no request is made for a refused URL");
    }

    #[test]
    fn a_download_that_is_not_an_image_is_refused() {
        let mut wire = Fake::with(vec![Ok(Reply::ok(b"<html>404</html>".to_vec()))]);
        let mut session = WireMedia::new(&mut wire, None, None, None, None);
        let err = block_on(session.download("https://example.test/a.png")).expect_err("not an image");
        assert!(err.message().contains("recognised image"), "{}", err.message());
    }

    #[test]
    fn an_image_over_the_cap_is_refused() {
        let too_big = vec![0u8; figcraft_ai::MAX_ASSET_BYTES + 1];
        let err = check_image_bytes(too_big).expect_err("too large");
        assert!(err.message().contains("larger"), "{}", err.message());
    }

    #[test]
    fn inspection_without_a_vision_model_says_so() {
        let mut wire = Fake::default();
        let mut session = WireMedia::new(&mut wire, None, None, None, None);
        let err = block_on(session.inspect_image(b"png", "image/png", "what is this?")).expect_err("no vision model");
        assert!(err.message().contains("Vision model"), "{}", err.message());
    }

    #[test]
    fn a_vision_request_speaks_each_provider_s_protocol() {
        let bytes = b"png";
        let openai = vision_request(
            &VisionConfig {
                base_url: "https://api.example.test/v1/".into(),
                api_key: "sk".into(),
                protocol: figcraft_ai::providers::Protocol::Openai,
                model: "m".into(),
            },
            bytes,
            "image/png",
            "what is this?",
        )
        .expect("a request");
        assert_eq!(openai.url, "https://api.example.test/v1/chat/completions");
        assert_eq!(
            openai
                .body
                .as_ref()
                .and_then(|b| b.pointer("/messages/0/content/1/image_url/url"))
                .and_then(Value::as_str)
                .map(|u| u.starts_with("data:image/png;base64,")),
            Some(true)
        );

        let anthropic = vision_request(
            &VisionConfig {
                base_url: "https://api.example.test".into(),
                api_key: "sk".into(),
                protocol: figcraft_ai::providers::Protocol::Anthropic,
                model: "m".into(),
            },
            bytes,
            "image/png",
            "what is this?",
        )
        .expect("a request");
        assert_eq!(anthropic.url, "https://api.example.test/messages");
        assert!(anthropic.headers.iter().any(|(name, _)| *name == "anthropic-version"));

        let google = vision_request(
            &VisionConfig {
                base_url: "https://api.example.test/v1beta".into(),
                api_key: "sk".into(),
                protocol: figcraft_ai::providers::Protocol::Google,
                model: "gemini".into(),
            },
            bytes,
            "image/png",
            "what is this?",
        )
        .expect("a request");
        assert_eq!(google.url, "https://api.example.test/v1beta/models/gemini:generateContent");
    }

    #[test]
    fn a_vision_answer_is_read_from_whichever_protocol_answered() {
        assert_eq!(vision_text(&json!({"choices": [{"message": {"content": "a lamp"}}]})).unwrap_or_default(), "a lamp");
        assert_eq!(vision_text(&json!({"content": [{"text": "a lamp"}]})).unwrap_or_default(), "a lamp");
        assert_eq!(vision_text(&json!({"candidates": [{"content": {"parts": [{"text": "a lamp"}]}}]})).unwrap_or_default(), "a lamp");
        assert!(vision_text(&json!({})).is_err());
    }

    #[test]
    fn an_inspection_reads_the_answer_and_reports_a_failure() {
        let mut wire = Fake::with(vec![
            Ok(Reply::ok(serde_json::to_vec(&json!({"choices": [{"message": {"content": "a lamp"}}]})).unwrap_or_default())),
            Ok(Reply { status: 500, body: b"boom".to_vec() }),
        ]);
        let vision = VisionConfig {
            base_url: "https://api.example.test/v1".into(),
            api_key: "sk".into(),
            protocol: figcraft_ai::providers::Protocol::Openai,
            model: "m".into(),
        };
        let mut session = WireMedia::new(&mut wire, None, None, Some(vision), None);
        assert_eq!(block_on(session.inspect_image(b"png", "image/png", "what?")).unwrap_or_default(), "a lamp");
        assert!(block_on(session.inspect_image(b"png", "image/png", "what?")).is_err());
    }

    #[test]
    fn an_svg_is_read_from_either_vectorizer() {
        assert_eq!(svg_of(&json!({"svg": "<svg/>"})).unwrap_or_default(), "<svg/>");
        assert_eq!(svg_of(&json!({"svg": {"url": "https://example.test/a.svg"}})).unwrap_or_default(), "https://example.test/a.svg");
        assert!(svg_of(&json!({})).is_err());
    }

    #[test]
    fn a_vectorizer_request_uses_the_key_its_service_expects() {
        let recraft = vectorize_request(figcraft_ai::VectorizeProvider::Recraft, "k", b"png");
        assert!(recraft.headers.iter().any(|(name, value)| *name == "authorization" && value == "Bearer k"));
        let fal = vectorize_request(figcraft_ai::VectorizeProvider::Fal, "k", b"png");
        assert!(fal.headers.iter().any(|(name, value)| *name == "authorization" && value == "Key k"));
    }

    #[test]
    fn an_openverse_entry_becomes_an_asset_with_its_licence() {
        let asset = map_openverse(&json!({
            "id": "abc", "url": "https://example.test/lamp.jpg", "thumbnail": "https://example.test/t.jpg",
            "title": "Lamp", "creator": "Ada", "license": "by-sa", "license_url": "https://example.test/lic",
            "filetype": "jpg", "width": 800, "height": 600, "foreign_landing_url": "https://example.test/page",
            "attribution": "Lamp by Ada",
        }))
        .expect("an asset");
        assert_eq!(asset.provider, "openverse");
        assert_eq!(asset.mime_type.as_deref(), Some("image/jpeg"));
        assert!(asset.commercial_use, "a CC-BY-SA picture may be used commercially");
        assert!(asset.requires_attribution);
        assert_eq!(asset.attribution.as_deref(), Some("Lamp by Ada"));
    }

    #[test]
    fn an_entry_with_no_url_is_not_an_asset() {
        assert!(map_openverse(&json!({"title": "no url"})).is_none());
        assert!(map_commons(&json!({"title": "no imageinfo"})).is_none());
    }

    #[test]
    fn a_commons_page_becomes_an_asset_with_its_metadata() {
        let asset = map_commons(&json!({
            "pageid": 42, "title": "File:Arc lamp.jpg",
            "imageinfo": [{
                "url": "https://upload.example.test/arc-lamp.jpg",
                "thumburl": "https://upload.example.test/thumb.jpg",
                "mime": "image/jpeg", "width": 1200, "height": 900,
                "descriptionurl": "https://commons.example.test/wiki/File:Arc_lamp.jpg",
                "extmetadata": {
                    "License": {"value": "cc-by-sa-4.0"},
                    "LicenseShortName": {"value": "CC BY-SA 4.0"},
                    "Artist": {"value": "<a href=\"#\">Ada</a>"},
                    "AttributionRequired": {"value": "true"},
                },
            }],
        }))
        .expect("an asset");
        assert_eq!(asset.id, "42");
        assert_eq!(asset.title.as_deref(), Some("Arc lamp"));
        assert_eq!(asset.creator.as_deref(), Some("Ada"), "the artist's markup is stripped");
        assert!(asset.requires_attribution);
        assert!(asset.commercial_use);
    }

    #[test]
    fn a_search_answer_with_no_entries_is_empty_not_a_failure() {
        assert!(openverse_entries(&json!({})).is_empty());
        assert!(commons_entries(&json!({"query": {"pages": {}}})).is_empty());
    }

    #[test]
    fn commons_pages_are_ordered_by_their_rank() {
        let pages = commons_entries(&json!({"query": {"pages": {
            "9": {"index": 2, "title": "File:b.jpg"},
            "3": {"index": 1, "title": "File:a.jpg"},
        }}}));
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0]["title"], "File:a.jpg");
    }
}
