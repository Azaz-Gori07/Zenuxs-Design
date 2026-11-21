//! Image assets from public libraries, as data and rules.
//!
//! FigCraft never commits a third-party picture. Instead the agent may search public libraries,
//! some of which need no key at all (Openverse, Wikimedia Commons), and the bytes it finds are
//! checked by a Vision model before they are placed — the rules in [`crate::vision`] always win.
//!
//! This module holds the parts that can be tested without a network: the normalized asset, the
//! licence policy, deduplication and ranking, the URL safety rules for a fetched asset, and the
//! attribution line that must travel with a placed image. The two no-key providers themselves live
//! in the desktop transport (`figcraft-ai-net`); they only turn a provider's JSON into these types.
//!
//! The rule that matters most: **metadata is for filtering and ordering, never for approval.** A
//! permissive licence and a perfect title only decide which pixels the Vision model looks at
//! first; only its verdict over the real bytes can place an image.

use serde::{Deserialize, Serialize};

use crate::media::Orientation;

/// The public Openverse images endpoint (no credentials needed).
pub const OPENVERSE_ENDPOINT: &str = "https://api.openverse.org/v1/images/";
/// The public Wikimedia Commons MediaWiki endpoint (no credentials needed).
pub const WIKIMEDIA_ENDPOINT: &str = "https://commons.wikimedia.org/w/api.php";
/// Providers are asked to identify themselves; this is FigCraft's own string.
pub const USER_AGENT: &str = "FigCraft/0.18 (https://github.com/storytold/figcraft)";
/// The largest asset the downloader will read into memory (12 MiB).
pub const MAX_ASSET_BYTES: usize = 12 * 1024 * 1024;
/// The most candidates one search may inspect across every provider.
pub const MAX_TOTAL_CANDIDATES: usize = 20;
/// The most Vision calls one search may spend.
pub const MAX_VISION_CALLS: usize = 6;

/// Whether an asset is a raster image, a vector document, or neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssetKind {
    Raster,
    Svg,
    Unknown,
}

/// One candidate image, normalized from whatever a provider returned.
///
/// Fields beyond the id/provider/url triple are optional: providers report metadata
/// inconsistently and the pipeline must work with partial data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageAsset {
    /// The provider-scoped id.
    pub id: String,
    /// The provider's name (`openverse`, `wikimedia`, `pexels`, `local`).
    pub provider: String,
    /// The original URL, used for the safe download and the dedupe key.
    pub url: String,
    /// A lower-resolution preview, when the provider gave one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    /// The human-readable page describing the image and its licence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_page: Option<String>,
    /// The licence code (Openverse `by-sa`, Commons `cc-by-sa`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_url: Option<String>,
    /// A ready-to-use credit line that must travel with the placed image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// Permitted by the reported licence (never a guarantee — see [`crate::vision`]).
    pub commercial_use: bool,
    /// Whether the reported licence allows derivatives.
    pub modification_allowed: bool,
    /// Whether the reported licence requires crediting the creator.
    pub requires_attribution: bool,
    pub kind: AssetKind,
}

impl ImageAsset {
    /// The credit that rides along with an imported image: creator, licence and source page.
    /// The empty string when the licence asks for none (e.g. CC0 / public domain).
    pub fn credit_line(&self) -> String {
        if !self.requires_attribution {
            return String::new();
        }
        let mut parts = Vec::new();
        if let Some(who) = self.attribution.as_deref().filter(|s| !s.is_empty()) {
            parts.push(who.to_string());
        } else if let Some(creator) = self.creator.as_deref().filter(|s| !s.is_empty()) {
            parts.push(creator.to_string());
        }
        if let Some(lic) = self.license.as_deref().filter(|s| !s.is_empty()) {
            parts.push(lic.to_uppercase());
        }
        if let Some(page) = self.source_page.as_deref().filter(|s| !s.is_empty()) {
            parts.push(page.to_string());
        }
        parts.join(" — ")
    }

    /// Where the bytes for a placement are fetched from.
    ///
    /// The preview, when the provider gave one: a UI slot is a few hundred pixels, and a
    /// library's original is often a 30 MB scan that would be refused by the size cap and
    /// wasted on a screen. The original is the fallback for a provider with no preview.
    pub fn download_url(&self) -> &str {
        match self.thumbnail_url.as_deref().filter(|u| !u.is_empty()) {
            Some(thumb) => thumb,
            None => &self.url,
        }
    }

    /// The dedupe key: provider plus id.
    pub fn key(&self) -> String {
        format!("{}:{}", self.provider, self.id)
    }
}

/// What every provider honours when it can.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetSearchOptions {
    /// Maximum results a provider may return.
    pub limit: usize,
    pub orientation: Orientation,
    /// Include licences that forbid commercial use.
    pub allow_non_commercial: bool,
    /// Require licences that allow derivatives.
    pub require_modification: bool,
}

/// One provider's contribution to a search round. A failed provider contributes an `error` and
/// an empty list; the other providers' results still stand.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetSearchOutcome {
    pub provider: String,
    pub assets: Vec<ImageAsset>,
    pub error: Option<String>,
}

/// The licence policy a project searches under. By default FigCraft only wants images a product
/// can use commercially and modify, which is what the two no-key providers are asked to filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LicensePolicy {
    pub allow_non_commercial: bool,
    pub require_modification: bool,
}

impl Default for LicensePolicy {
    fn default() -> Self {
        Self { allow_non_commercial: false, require_modification: true }
    }
}

/// A licence code allows commercial use unless it is a NoCommercial (`-nc`) one.
pub fn license_allows_commercial_use(license: &str) -> bool {
    !license.trim().is_empty() && !license.to_ascii_lowercase().contains("-nc")
}

/// The `nd` (NoDerivatives) licences forbid modification.
pub fn license_allows_modification(license: &str) -> bool {
    !license.trim().is_empty() && !license.to_ascii_lowercase().contains("-nd")
}

/// CC0 and public-domain marks need no credit; the others do.
pub fn license_requires_attribution(license: &str) -> bool {
    let code = license.trim().to_ascii_lowercase();
    !code.is_empty() && code != "cc0" && code != "pdm" && code != "public domain"
}

/// Whether an asset may be used under this policy. Metadata filtering only — a pass here never
/// means the image is approved; that is the Vision model's job.
pub fn passes_license_policy(asset: &ImageAsset, policy: &LicensePolicy) -> bool {
    if !policy.allow_non_commercial && !asset.commercial_use {
        return false;
    }
    if policy.require_modification && !asset.modification_allowed {
        return false;
    }
    if let Some(mime) = asset.mime_type.as_deref()
        && !mime.starts_with("image/")
    {
        return false;
    }
    // A tiny thumbnail is not worth sending to Vision.
    if matches!(asset.kind, AssetKind::Raster)
        && let (Some(w), Some(h)) = (asset.width, asset.height)
        && (w < 120 || h < 120)
    {
        return false;
    }
    true
}

/// Strip the tracking parameters that would otherwise defeat deduplication.
pub fn canonicalize_url(raw: &str) -> String {
    let Ok(mut url) = url::Url::parse(raw) else { return raw.to_string() };
    let tracking = ["utm_source", "utm_medium", "utm_campaign", "utm_content", "utm_term", "utm_id"];
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| !tracking.contains(&k.to_ascii_lowercase().as_str()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.query_pairs_mut().clear().extend_pairs(kept);
    url.set_fragment(None);
    url.to_string()
}

/// Hosts that never resolve outside a local network. A provider-supplied URL pointing at one of
/// these is refused before any request is made (SSRF).
pub fn is_private_hostname(host: &str) -> bool {
    let h = host.trim().to_ascii_lowercase().trim_start_matches('[').trim_end_matches(']').to_string();
    if h.is_empty() || ["localhost", "::1", "::", "0.0.0.0"].contains(&h.as_str()) {
        return true;
    }
    if [".localhost", ".local", ".internal"].iter().any(|s| h.ends_with(s)) {
        return true;
    }
    if h.starts_with("fc") || h.starts_with("fd") || h.starts_with("fe80:") {
        return true;
    }
    let parts: Vec<&str> = h.split('.').collect();
    if parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok()) {
        let a: u8 = parts[0].parse().unwrap_or(0);
        let b: u8 = parts[1].parse().unwrap_or(0);
        if a == 10 || a == 127 || a == 169 || a == 0 {
            return true;
        }
        if a == 172 && (16..=31).contains(&b) {
            return true;
        }
        if a == 192 && (b == 168 || b == 0) {
            return true;
        }
        if a == 198 && (b == 18 || b == 19) {
            return true;
        }
        if a == 100 && (64..=127).contains(&b) {
            return true;
        }
    }
    // A bare hostname with no dot and no colon can only resolve locally.
    !h.contains('.') && !h.contains(':')
}

/// Validate a provider-supplied URL before it is fetched. Returns the reason it was refused.
pub fn validate_remote_url(raw: &str) -> Result<(), String> {
    let url = url::Url::parse(raw).map_err(|_| format!("Invalid asset URL: {raw}"))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(format!("Unsupported asset URL scheme: {}", url.scheme()));
    }
    if url.username() != "" || url.password().is_some() {
        return Err("An asset URL must not carry credentials".into());
    }
    let host = url.host_str().unwrap_or("");
    if is_private_hostname(host) {
        return Err(format!("Refusing to fetch a private address: {host}"));
    }
    Ok(())
}

/// Content types the downloader is willing to import.
pub fn is_allowed_image_content_type(content_type: &str) -> bool {
    let kind = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    ["image/jpeg", "image/png", "image/webp", "image/gif", "image/avif", "image/svg+xml", "image/bmp", "image/tiff"].contains(&kind.as_str())
}

/// Detect the real type from an image's leading bytes, never from its name.
pub fn sniff_image_content_type(bytes: &[u8]) -> Option<&'static str> {
    let starts = |magic: &[u8]| bytes.len() >= magic.len() && &bytes[..magic.len()] == magic;
    if starts(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if starts(&[0x89, b'P', b'N', b'G']) {
        return Some("image/png");
    }
    if starts(b"GIF8") {
        return Some("image/gif");
    }
    if starts(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return Some("image/webp");
    }
    if starts(b"BM") {
        return Some("image/bmp");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
    if head.contains("<svg") {
        return Some("image/svg+xml");
    }
    None
}

/// Strip the HTML Commons stores inside metadata values.
pub fn strip_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut skip = 0usize;
    for ch in value.chars() {
        if skip > 0 {
            if ch == '>' {
                skip -= 1;
            }
            continue;
        }
        if ch == '<' {
            skip += 1;
            continue;
        }
        out.push(ch);
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Remove candidates that repeat a provider id or the same canonical URL across providers.
pub fn dedupe_assets(assets: Vec<ImageAsset>) -> Vec<ImageAsset> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for asset in assets {
        let url_key = format!("url:{}", canonicalize_url(&asset.url));
        if seen.insert(asset.key()) && seen.insert(url_key) {
            out.push(asset);
        }
    }
    out
}

/// What a candidate gains for sitting high in the library's own ordering.
///
/// Sealed to a few points: it breaks ties between pictures that all name the subject without
/// letting the library's first result beat a picture whose metadata is a better match.
fn rank_bonus(position: usize) -> i32 {
    match position {
        0 => 4,
        1 | 2 => 3,
        3..=5 => 2,
        6..=9 => 1,
        _ => 0,
    }
}

/// Lowercase a query into the words worth matching in a title/description.
pub fn tokenize(query: &str) -> Vec<String> {
    query.to_ascii_lowercase().split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| w.len() > 2).map(str::to_string).collect()
}

/// A metadata-only score, used only to decide which pixels Vision inspects first.
pub fn score_asset(asset: &ImageAsset, query: &str, orientation: Orientation) -> i32 {
    let mut score = 0i32;
    let haystack = format!("{} {}", asset.title.as_deref().unwrap_or(""), asset.description.as_deref().unwrap_or("")).to_ascii_lowercase();
    for token in tokenize(query) {
        if haystack.contains(&token) {
            score += 2;
        }
    }
    if let (Some(w), Some(h)) = (asset.width, asset.height) {
        let ratio = w as f64 / h as f64;
        let matched = match orientation {
            Orientation::Landscape => ratio > 1.15,
            Orientation::Portrait => ratio < 0.87,
            Orientation::Square => (0.87..1.15).contains(&ratio),
        };
        score += if matched { 3 } else { -2 };
    }
    if asset.commercial_use {
        score += 1;
    }
    if !asset.requires_attribution {
        score += 1;
    }
    if matches!(asset.kind, AssetKind::Raster) && asset.width.is_some_and(|w| w >= 800) {
        score += 1;
    }
    if asset.provider == "local" {
        score -= 2;
    }
    // A museum scan is a bitmap like a photograph, so nothing in the file tells them apart — but
    // a design calls for photography, and a dark oil painting in a hero slot reads as a mistake.
    // Commons answers "lighthouse on a rocky coast" with a storm-battered maritime painting,
    // which the verifier then rejects for being unreadable at thumbnail size, spending a
    // Vision call to learn what the metadata already said.
    if names_artwork(&haystack) {
        score -= 6;
    }
    score
}

/// Words that name a picture of a painting rather than a photograph of a thing.
///
/// Whole words, matched the same way the audit matches a name: "art" must not fire inside
/// "earth", and "oil" must not fire inside "oilseed".
fn names_artwork(haystack: &str) -> bool {
    const ARTWORK: [&str; 26] = [
        "painting",
        "paintings",
        "oil",
        "canvas",
        "watercolour",
        "watercolor",
        "engraving",
        "etching",
        "lithograph",
        "woodcut",
        "drawing",
        "sketch",
        "illustration",
        "manuscript",
        "fresco",
        "mural",
        "poster",
        "caricature",
        "artwork",
        "artworks",
        "museum",
        "gallery",
        "collection",
        "sculpture",
        "tapestry",
        "printmaking",
    ];
    if haystack.split(|c: char| !c.is_alphanumeric()).any(|w| !w.is_empty() && ARTWORK.contains(&w.to_ascii_lowercase().as_str())) {
        return true;
    }
    // A museum titles a picture after its painter and their dates — "Benjamin Barker II
    // (1776-1838) - Seascape with a Rocky Coast and a Lighthouse". The name and the dates say
    // nothing about the subject, so every query word still matched and the painting outranked
    // five photographs of the same coast.
    dated_artist(haystack)
}

/// Whether the title carries a painter's lifespan, `(1776-1838)` or `(1776–1838)`.
fn dated_artist(text: &str) -> bool {
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        rest = rest.get(open + 1..).unwrap_or_default();
        let Some(close) = rest.find(')') else { return false };
        let inner = rest.get(..close).unwrap_or_default();
        let years: Vec<&str> = inner.split(['-', '\u{2013}', '\u{2014}']).map(str::trim).collect();
        if years.len() == 2 && years.iter().all(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit())) {
            return true;
        }
        rest = rest.get(close + 1..).unwrap_or_default();
    }
    false
}

/// Merge one round of provider results into the ranked candidates to inspect.
pub fn rank_candidates(
    outcomes: &[AssetSearchOutcome],
    policy: &LicensePolicy,
    query: &str,
    orientation: Orientation,
    limit: usize,
) -> Vec<ImageAsset> {
    let mut all = Vec::new();
    for outcome in outcomes {
        if outcome.error.is_none() {
            all.extend(outcome.assets.iter().cloned());
        }
    }
    // The library's own order is a better relevance ranking than matching words in a title: it
    // searched the whole description, the categories and the file's own text. Without it a tie
    // was broken alphabetically, which put a museum's painting of a rocky coast ahead of five
    // photographs of the same coast because "Benjamin" sorts before "Rocky".
    let ranked: Vec<(usize, ImageAsset)> = dedupe_assets(all).into_iter().enumerate().collect();
    let mut scored: Vec<(i32, ImageAsset)> = ranked
        .into_iter()
        .filter(|(_, a)| passes_license_policy(a, policy))
        .map(|(position, a)| (score_asset(&a, query, orientation) + rank_bonus(position), a))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.key().cmp(&b.1.key())));
    scored.into_iter().take(limit).map(|(_, a)| a).collect()
}

/// Describe a hand-thrown cream stoneware cup in a library's own words.
///
/// The subject is what the picture is *of*; the query is what to type. A query built from the
/// first few words of a sentence — "Open shelf with" from "Open shelf with hand-thrown ceramics" —
/// is the first few words of a sentence, not a search: the adjectives that carry the subject are
/// at the end, and a library answers "Open shelf with" with twenty pictures of shelves.
///
/// So the subject wins. Its words are ranked and the two or three that name a thing are kept,
/// with the ones that only describe it dropped: "ceramic mug" finds mugs, "hand thrown ceramic
/// mug" finds fewer, and "a" finds everything. Describing words are dropped last, so a subject
/// that is *only* descriptive still searches with something.
pub fn broaden_query(query: &str, subject: &str, round: usize) -> String {
    let keep = if round <= 1 { 3 } else { 2 };
    // A subject is the better sentence; a query is the model's own shorter attempt at one. Use
    // the subject unless there is none.
    let source = if subject.trim().is_empty() { query } else { subject };
    let mut words: Vec<String> = source
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_ascii_lowercase())
        .filter(|w| !w.is_empty() && !STOP_WORDS.contains(&w.as_str()))
        .collect();
    if words.is_empty() {
        words = query.split_whitespace().take(keep).map(|w| w.to_ascii_lowercase()).collect();
    }
    // Keep the most specific words, in the order they were written: length is the one signal a
    // search has that separates the noun a description is about from the words describing it.
    // "Stacked handmade ceramic plates on a table" narrows to "handmade ceramic plates"; "a wall"
    // keeps its own two words because there is nothing longer to prefer.
    if words.len() > keep {
        let mut ranked: Vec<(usize, &String)> = words.iter().enumerate().collect();
        ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
        let mut kept: Vec<usize> = ranked.into_iter().take(keep).map(|(i, _)| i).collect();
        kept.sort_unstable();
        words = kept.into_iter().filter_map(|i| words.get(i).cloned()).collect();
    }
    words.join(" ")
}

/// The words a library search is better without: articles, prepositions, and the words that name
/// the medium rather than the subject. Kept small on purpose — dropping a word that is meaningful
/// in a subject costs a result, and this list holds only words that are never the thing itself.
const STOP_WORDS: [&str; 32] = [
    "a", "an", "the", "of", "with", "and", "or", "for", "in", "on", "at", "to", "from", "by", "as", "that", "this", "its", "it", "is", "are", "be",
    "into", "over", "under", "near", "image", "images", "photo", "photos", "picture", "shot",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_museum_scan_of_a_painting_ranks_below_a_photograph_of_the_same_subject() {
        // Commons answers "lighthouse on a rocky coast" with a maritime oil painting, which the
        // verifier rejects for being unreadable at thumbnail size — a Vision call spent learning
        // what the title already said.
        let mut painting = asset("wikimedia", "1", "https://x.test/p.jpg");
        painting.title = Some("Storm off a rocky coast, oil on canvas".into());
        painting.width = Some(1600);
        painting.height = Some(1000);
        let mut photo = asset("wikimedia", "2", "https://x.test/q.jpg");
        photo.title = Some("Lighthouse on a rocky coast".into());
        photo.width = Some(1600);
        photo.height = Some(1000);
        let ranked = rank_candidates(
            &[AssetSearchOutcome { provider: "wikimedia".into(), assets: vec![painting, photo], error: None }],
            &LicensePolicy::default(),
            "lighthouse rocky coast",
            Orientation::Landscape,
            10,
        );
        assert_eq!(ranked.first().map(|a| a.id.as_str()), Some("2"), "the photograph comes first");
    }

    #[test]
    fn a_word_inside_another_word_does_not_make_a_picture_artwork() {
        assert!(!names_artwork("oilseed rape field in spring"));
        assert!(!names_artwork("an earth road near a river"));
        assert!(names_artwork("Rocky coast, oil on canvas"));
        assert!(names_artwork("watercolor study of a harbour"));
    }

    #[test]
    fn a_museum_title_naming_a_painter_and_dates_is_artwork() {
        // Commons titled it "Benjamin Barker II (1776-1838) - Seascape with a Rocky Coast and a
        // Lighthouse - A110 - Holburne Museum". Every query word matched, so the painting beat
        // five photographs of the same coast.
        assert!(names_artwork("Benjamin Barker II (1776-1838) - Seascape with a Rocky Coast and a Lighthouse"));
        assert!(dated_artist("Some Painter (1776\u{2013}1838) - A Harbour"));
        assert!(!dated_artist("Lighthouse on a rocky coast (2007)"));
        assert!(!dated_artist("Rocky coast below Killantringan lighthouse"));
        assert!(!dated_artist("unclosed (1776-1838"));
    }

    #[test]
    fn the_librarys_own_order_breaks_a_tie_between_pictures_that_name_the_subject() {
        // Both titles carry the query words, so both score the same on metadata. The library put
        // the photograph first; sorting by title would have put the painting first.
        let mut photograph = asset("wikimedia", "1", "https://x.test/a.jpg");
        photograph.title = Some("Rocky coast below the lighthouse".into());
        let mut painting = asset("wikimedia", "2", "https://x.test/b.jpg");
        painting.title = Some("Rocky coast and lighthouse".into());
        let ranked = rank_candidates(
            &[AssetSearchOutcome { provider: "wikimedia".into(), assets: vec![photograph, painting], error: None }],
            &LicensePolicy::default(),
            "lighthouse rocky coast",
            Orientation::Landscape,
            10,
        );
        assert_eq!(ranked.first().map(|a| a.id.as_str()), Some("1"), "the library's first result wins the tie");
    }

    fn asset(provider: &str, id: &str, url: &str) -> ImageAsset {
        ImageAsset {
            id: id.into(),
            provider: provider.into(),
            url: url.into(),
            thumbnail_url: None,
            title: None,
            description: None,
            mime_type: Some("image/jpeg".into()),
            width: Some(640),
            height: Some(480),
            creator: None,
            source_page: None,
            license: Some("by".into()),
            license_url: None,
            attribution: None,
            commercial_use: true,
            modification_allowed: true,
            requires_attribution: true,
            kind: AssetKind::Raster,
        }
    }

    #[test]
    fn licence_codes_drive_the_policy() {
        assert!(license_allows_commercial_use("by-sa"));
        assert!(!license_allows_commercial_use("by-nc-sa"));
        assert!(license_allows_modification("by-sa"));
        assert!(!license_allows_modification("by-nd"));
        assert!(license_requires_attribution("by"));
        assert!(!license_requires_attribution("cc0"));
        assert!(!license_requires_attribution("pdm"));
    }

    #[test]
    fn a_cc0_asset_needs_no_credit_but_a_by_asset_does() {
        let mut a = asset("openverse", "1", "https://example.com/a.jpg");
        a.creator = Some("Ada".into());
        a.license = Some("by".into());
        a.source_page = Some("https://example.com/page".into());
        assert!(a.credit_line().contains("Ada"));
        assert!(a.credit_line().contains("BY"));
        a.license = Some("cc0".into());
        a.requires_attribution = license_requires_attribution("cc0");
        assert_eq!(a.credit_line(), "");
    }

    #[test]
    fn the_download_is_the_preview_when_the_library_has_one() {
        // A museum original is often a 30 MB scan: over the download cap, and far more than a
        // design slot shows. The preview is the same picture at a sensible size.
        let mut a = asset("wikimedia", "1", "https://example.com/original.jpg");
        assert_eq!(a.download_url(), "https://example.com/original.jpg");
        a.thumbnail_url = Some("https://example.com/thumb.jpg".into());
        assert_eq!(a.download_url(), "https://example.com/thumb.jpg");
        // A provider that reports an empty preview means "no preview", not "download nothing".
        a.thumbnail_url = Some(String::new());
        assert_eq!(a.download_url(), "https://example.com/original.jpg");
    }

    #[test]
    fn dedupe_keeps_one_of_each_id_and_canonical_url() {
        let mut a = asset("openverse", "1", "https://example.com/a.jpg?utm_source=x");
        a.title = Some("a".into());
        let b = asset("wikimedia", "9", "https://example.com/a.jpg");
        let c = asset("openverse", "2", "https://example.com/b.jpg");
        let out = dedupe_assets(vec![a, b, c]);
        assert_eq!(out.len(), 2, "the same URL from two providers is one candidate");
    }

    #[test]
    fn canonicalize_drops_tracking_and_fragments() {
        let got = canonicalize_url("https://x.test/p?utm_source=a&q=fox#top");
        assert!(!got.contains("utm_source"));
        assert!(!got.contains('#'));
        assert!(got.contains("q=fox"));
    }

    #[test]
    fn private_and_credentialed_urls_are_refused() {
        for url in [
            "http://localhost/x",
            "http://127.0.0.1/x",
            "https://10.0.0.5/x",
            "file:///etc/passwd",
            "https://u:p@example.com/x",
            "https://printer.local/x",
        ] {
            assert!(validate_remote_url(url).is_err(), "{url} must be refused");
        }
        assert!(validate_remote_url("https://upload.wikimedia.org/a.jpg").is_ok());
        assert!(is_private_hostname("192.168.1.4"));
        assert!(!is_private_hostname("upload.wikimedia.org"));
    }

    #[test]
    fn content_type_is_sniffed_from_magic_bytes() {
        assert_eq!(sniff_image_content_type(b"\x89PNG\r\n\x1a\nxxx"), Some("image/png"));
        assert_eq!(sniff_image_content_type(b"\xff\xd8\xffxxx"), Some("image/jpeg"));
        assert_eq!(sniff_image_content_type(b"<svg xmlns='x'></svg>"), Some("image/svg+xml"));
        assert_eq!(sniff_image_content_type(b"not an image"), None);
        assert!(is_allowed_image_content_type("image/png; charset=x"));
        assert!(!is_allowed_image_content_type("text/html"));
    }

    #[test]
    fn non_commercial_and_non_modifiable_assets_are_filtered() {
        let mut nc = asset("openverse", "1", "https://x.test/a.jpg");
        nc.commercial_use = false;
        let mut nd = asset("openverse", "2", "https://x.test/b.jpg");
        nd.modification_allowed = false;
        let policy = LicensePolicy::default();
        assert!(!passes_license_policy(&nc, &policy));
        assert!(!passes_license_policy(&nd, &policy));
        let relaxed = LicensePolicy { allow_non_commercial: true, require_modification: false };
        assert!(passes_license_policy(&nc, &relaxed));
    }

    #[test]
    fn ranking_puts_the_best_candidate_first_and_skips_failed_providers() {
        let mut plain = asset("openverse", "1", "https://x.test/a.jpg");
        plain.title = Some("a fox".into());
        let mut better = asset("wikimedia", "2", "https://x.test/b.jpg");
        better.title = Some("a red fox in the snow".into());
        better.width = Some(1600);
        let outcomes = vec![
            AssetSearchOutcome { provider: "openverse".into(), assets: vec![plain], error: None },
            AssetSearchOutcome { provider: "wikimedia".into(), assets: vec![better.clone()], error: None },
            AssetSearchOutcome {
                provider: "pexels".into(),
                assets: vec![asset("pexels", "3", "https://x.test/c.jpg")],
                error: Some("no key".into()),
            },
        ];
        let ranked = rank_candidates(&outcomes, &LicensePolicy::default(), "red fox", Orientation::Landscape, 5);
        assert_eq!(ranked[0].id, better.id, "the richer match ranks first");
        assert!(ranked.iter().all(|a| a.provider != "pexels"), "a failed provider contributes nothing");
    }

    #[test]
    fn html_is_stripped_from_commons_metadata() {
        assert_eq!(strip_html("<b>Ada</b> &amp;Bob  <i>Lovell</i>"), "Ada &Bob Lovell");
    }

    #[test]
    fn broadening_keeps_the_subject_words() {
        assert_eq!(broaden_query("a red fox in the snow", "a red fox", 1), "red fox");
        assert_eq!(broaden_query("a red fox in the snow", "a red fox", 2), "red fox");
    }

    #[test]
    fn broadening_a_sentence_keeps_the_nouns_and_drops_the_filler() {
        // The failure this guards: the query was "Open shelf with hand-thrown ceramics" and the
        // broadened round searched the library for "Open shelf with" — the first three words of a
        // sentence, which is not a search. Twenty pictures of shelves came back and the slot went
        // out empty. The subject's own words replace it.
        let round_one = broaden_query("Open shelf with hand-thrown ceramics", "Stacked ceramic plates on a table", 1);
        // What it must not be is the failure: the first three words of the sentence, which is what
        // a library answers with pictures of open shelves.
        assert_ne!(round_one, "open shelf with");
        assert_eq!(round_one, "stacked ceramic plates");
        let round_two = broaden_query("Open shelf with hand-thrown ceramics", "Stacked ceramic plates on a table", 2);
        assert!(!round_two.contains("the") && !round_two.contains(" on "), "no filler survives: {round_two}");
        assert_ne!(round_two, round_one, "the second round searches something new");
    }

    #[test]
    fn a_subject_of_nothing_but_filler_falls_back_to_the_query() {
        // A model that writes "the image of a" has said nothing, but a library query of "" returns
        // everything, which is worse than returning some. The query is the better sentence here.
        assert_eq!(broaden_query("stoneware mug", "the image of a", 1), "stoneware mug");
    }
}
