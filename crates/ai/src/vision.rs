//! Vision verification and image sourcing, as data and rules.
//!
//! The rule the design agent works under: **nothing is imported without a vision model having
//! looked at the actual pixels**. A verification is a verdict from the Vision role — approved, a
//! relevance score, the requirements it matched and the ones it failed. Anything else (a malformed
//! answer, a model that is not configured) is a failed inspection, never an approval.
//!
//! The request parser and the route decision are here too, so a test can check them without a
//! network. The pipeline itself runs on the tools' worker in the interface.

use serde::{Deserialize, Serialize};

/// The state one sourced image ended in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourcingState {
    /// A verified image from a public library.
    VerifiedFromSearch,
    /// A verified bundled image.
    VerifiedFromLocal,
    /// A verified generated image.
    VerifiedGenerated,
    /// No image passed verification.
    NoVerifiedMatch,
    /// No Vision model is configured, so nothing could be verified.
    VisionUnavailable,
    /// The image could not be generated.
    GenerationUnavailable,
}

impl SourcingState {
    /// Whether this state means the placeholder was filled.
    pub fn applied(self) -> bool {
        matches!(self, Self::VerifiedFromSearch | Self::VerifiedFromLocal | Self::VerifiedGenerated)
    }
}

/// How an image is sourced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageRoute {
    /// Search a public library first; generate when nothing is found.
    Auto,
    /// Only search.
    Search,
    /// Only generate.
    Generate,
    /// Try one cheap search round, then generate.
    ProbeThenGenerate,
}

/// What one image must show.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VisualIntent {
    /// What the image has to show (plain nouns, required).
    pub subject: String,
    /// The kind of image: photo, flat_2d_vector, cartoon, illustration, icon, abstract.
    #[serde(default)]
    pub style: String,
    /// Search terms; the subject when absent.
    #[serde(default)]
    pub query: String,
    /// The shape of the slot.
    #[serde(default)]
    pub orientation: String,
    /// A colour palette to keep.
    #[serde(default)]
    pub palette: String,
    /// A mood to keep.
    #[serde(default)]
    pub mood: String,
    /// Hard constraints ("no photographs").
    #[serde(default)]
    pub exclusions: Vec<String>,
    /// Force a library search.
    #[serde(default)]
    pub use_stock: bool,
    /// Force generation.
    #[serde(default)]
    pub must_generate: bool,
    /// Never generate.
    #[serde(default)]
    pub do_not_generate: bool,
}

/// One request from the model's `source_image` call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceRequest {
    /// The leaf placeholder node's id.
    pub id: String,
    /// What the image has to show.
    pub intent: VisualIntent,
}

/// The verifier's answer about one candidate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VisionVerdict {
    /// Whether the candidate may be placed.
    pub approved: bool,
    /// 0..1 relevance.
    #[serde(default)]
    pub relevance_score: f64,
    /// The requirements the image met.
    #[serde(default)]
    pub matched_requirements: Vec<String>,
    /// The requirements it failed.
    #[serde(default)]
    pub failed_requirements: Vec<String>,
    /// Why.
    #[serde(default)]
    pub reason: String,
}

/// The minimum relevance to accept.
pub const VERIFICATION_THRESHOLD: f64 = 0.7;

/// The relevance a candidate needs to be kept as a fallback when nothing passed outright.
///
/// A design brief is written the way a designer writes one — "a woman in her thirties by a tall
/// window, soft Nordic daylight, plain pale wall" — and a public library holds a generic stock
/// photograph of a woman near a window in flat light. The verifier is right that the picture is
/// not the brief, and rejecting every candidate for that reason left the screen with grey boxes,
/// which is worse than a near photograph in the slot. A candidate at this bar is usable: the
/// subject is recognisably right and nothing is broken. The slot takes the best of them when the
/// run finds no outright match.
pub const FALLBACK_THRESHOLD: f64 = 0.45;

/// Parse the model's verdict from plain text: the first complete JSON object, with a scanner that
/// respects strings. A model that answers prose around the JSON is fine; one that never answers
/// with JSON is a failed inspection.
pub fn parse_verdict(text: &str) -> Result<VisionVerdict, String> {
    let bytes = text.as_bytes();
    let mut start = None;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0
                    && let Some(begin) = start
                    && let Ok(verdict) = serde_json::from_slice::<VisionVerdict>(&bytes[begin..=i])
                {
                    return Ok(verdict);
                }
            }
            _ => {}
        }
    }
    Err("the vision model did not answer with a JSON verdict".into())
}

/// A requirement is mandatory when the prompt says so, or when it names a hard constraint.
pub fn mandatory(id: &str) -> bool {
    ["subject", "style", "no-unwanted-text", "no-artifacts", "usable-in-ui"].contains(&id) || id.starts_with("exclusion:")
}

/// Whether a verdict passes: approved, no mandatory failure, and enough relevance.
pub fn accepts(verdict: &VisionVerdict) -> bool {
    verdict.approved && verdict.relevance_score >= VERIFICATION_THRESHOLD && !verdict.failed_requirements.iter().any(|r| mandatory(r))
}

/// Whether a rejected candidate is worth keeping as a near miss: the picture is recognisably the
/// subject and nothing about it is broken, even though it is not the brief.
///
/// A mandatory failure that is not `subject` rules it out — a watermark, a rendering artifact or
/// lettering is a defect a person would complain about, and a grey box at least looks deliberate.
/// A low `subject` score is the near miss this keeps.
pub fn fallback_worthy(verdict: &VisionVerdict) -> bool {
    verdict.relevance_score >= FALLBACK_THRESHOLD && !verdict.failed_requirements.iter().any(|r| mandatory(r) && r != "subject")
}

/// The prompt the Vision model receives for one candidate.
///
/// It is judged against the words a library was searched with, not against the model's whole art
/// direction. A brief reads "large format landscape photograph of a small fishing harbour in thick
/// fog, wooden boats, still water, cold northern light" and a public library holds "a harbour in
/// fog"; asking the verifier whether the picture is the first sentence rejects every real
/// photograph and leaves the slot a grey box. The query is what was asked for; the style, palette
/// and mood are preferences the library cannot be held to.
pub fn inspection_prompt(intent: &VisualIntent) -> String {
    let asked = if intent.query.trim().is_empty() { &intent.subject } else { &intent.query };
    let mut requirements = vec![
        "subject: does the image show the named subject?".to_string(),
        "no-unwanted-text: no stray lettering or watermarks".to_string(),
        "no-artifacts: no visible rendering artifacts".to_string(),
        "usable-in-ui: it works at the target size".to_string(),
    ];
    for exclusion in &intent.exclusions {
        requirements.push(format!("exclusion:{exclusion}: this must not be present"));
    }
    let mut preferences = Vec::new();
    if !intent.style.is_empty() {
        preferences.push(format!("style: it is {}", intent.style));
    }
    if !intent.palette.is_empty() {
        preferences.push(format!("palette: {}", intent.palette));
    }
    if !intent.mood.is_empty() {
        preferences.push(format!("mood: {}", intent.mood));
    }
    let prefs = if preferences.is_empty() {
        String::new()
    } else {
        format!("\n\nPreferences (weigh them, never fail the image for them alone):\n- {}", preferences.join("\n- "))
    };
    format!(
        "Inspect this image against the request. Reply with ONE JSON object only: {{\"approved\": bool, \"relevance_score\": 0..1, \"matched_requirements\": [ids], \"failed_requirements\": [ids], \"reason\": string}}. The request: {asked}. Requirements:\n- {list}{prefs}",
        list = requirements.join("\n- ")
    )
}

/// Parse a `source_image` call's `requests` argument (a JSON array of objects).
///
/// A model writing eight image briefs runs out of output tokens part-way through the eighth, and
/// the array never closes. Refusing the whole call loses the seven briefs that were complete and
/// spends another step on the same words; the well-formed entries are kept, and the answer says
/// which slots are missing so the model asks for those.
pub fn parse_requests(value: &str) -> Result<Vec<SourceRequest>, String> {
    let list: serde_json::Value = match serde_json::from_str(value) {
        Ok(v) => v,
        Err(e) => match salvage_objects(value) {
            Some(v) if !v.is_empty() => serde_json::Value::Array(v),
            _ => return Err(format!("the image requests are not JSON: {e}")),
        },
    };
    let Some(array) = list.as_array() else { return Err("the image requests must be a JSON array".into()) };
    let mut out = Vec::new();
    for entry in array {
        let id = entry.get("id").and_then(serde_json::Value::as_str).unwrap_or_default().trim().to_string();
        if id.is_empty() {
            return Err("an image request has no target node id".into());
        }
        let subject = entry.get("subject").or_else(|| entry.get("query")).and_then(serde_json::Value::as_str).unwrap_or_default().trim().to_string();
        let intent = VisualIntent {
            subject,
            style: text_of(entry, "style"),
            query: text_of(entry, "query"),
            orientation: text_of(entry, "orientation"),
            palette: text_of(entry, "palette"),
            mood: text_of(entry, "mood"),
            exclusions: list_of(entry, "exclusions"),
            use_stock: flag_of(entry, "use_stock"),
            must_generate: flag_of(entry, "must_generate"),
            do_not_generate: flag_of(entry, "do_not_generate"),
        };
        out.push(SourceRequest { id, intent });
    }
    Ok(out)
}

/// The complete objects in a truncated JSON array: each `{…}` at the top level that parses on its
/// own. A string is skipped over so a brace inside a subject does not count as nesting.
fn salvage_objects(text: &str) -> Option<Vec<serde_json::Value>> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start: Option<usize> = None;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 && start.is_none() {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0
                    && let Some(from) = start.take()
                    && let Some(slice) = text.get(from..=i)
                    && let Ok(v) = serde_json::from_str::<serde_json::Value>(slice)
                {
                    out.push(v);
                }
            }
            _ => {}
        }
    }
    Some(out)
}

fn text_of(entry: &serde_json::Value, key: &str) -> String {
    entry.get(key).and_then(serde_json::Value::as_str).unwrap_or_default().trim().to_string()
}

fn flag_of(entry: &serde_json::Value, key: &str) -> bool {
    entry.get(key).and_then(serde_json::Value::as_bool).unwrap_or(false)
}

fn list_of(entry: &serde_json::Value, key: &str) -> Vec<String> {
    entry
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Decide how to source one image. Explicit user overrides win; a subject that must not be
/// generated (a real person's likeness, a brand mark) goes to search.
pub fn route(intent: &VisualIntent) -> ImageRoute {
    if intent.use_stock {
        return ImageRoute::Search;
    }
    if protected_subject(&intent.subject) || intent.do_not_generate {
        return ImageRoute::Search;
    }
    if intent.must_generate {
        return ImageRoute::Generate;
    }
    // Two or more specifics: no public library is likely to have it.
    let mut signals = 0;
    if !intent.style.is_empty() {
        signals += 1;
    }
    if !intent.palette.is_empty() {
        signals += 1;
    }
    if !intent.mood.is_empty() {
        signals += 1;
    }
    if !intent.exclusions.is_empty() {
        signals += 1;
    }
    match signals {
        0 => ImageRoute::Auto,
        1 => ImageRoute::ProbeThenGenerate,
        _ => ImageRoute::Generate,
    }
}

/// The tiers to try, in order, for a route.
///
/// A route says what the *first* attempt should be, not the only attempt. Generation is the
/// expensive one and the one that needs a key, so when it is not asked for it comes last; when
/// it *is* asked for it still comes first, but a search behind it means a slot is filled by a
/// licensed photograph rather than left as an empty box when the connection has no image model.
pub fn tiers(route: ImageRoute) -> &'static [ImageRoute] {
    match route {
        // Search-only is what a protected subject and `do_not_generate` ask for: never generate.
        ImageRoute::Search => &[ImageRoute::Search],
        ImageRoute::Auto | ImageRoute::ProbeThenGenerate => &[ImageRoute::Search, ImageRoute::Generate],
        ImageRoute::Generate => &[ImageRoute::Generate, ImageRoute::Search],
    }
}

/// Real people and real brands are never generated: only a licensed original is used.
pub fn protected_subject(subject: &str) -> bool {
    let s = subject.to_lowercase();
    ["celebrity", "portrait of", "photo of a real", "logo", "trademark", "brand mark"].iter().any(|needle| s.contains(needle))
}

/// The result of one sourcing slot, for the model's answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourcingResult {
    /// The placeholder this concerns.
    pub id: String,
    /// How it ended.
    pub state: SourcingState,
    /// Why, when it did not succeed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The route that was taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<ImageRoute>,
    /// Every candidate the verifier rejected, with the reason.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_near_miss_is_kept_when_the_only_failure_is_the_subject() {
        // A public library rarely holds the exact scene a brief describes. A recognisable
        // photograph of the right kind of thing is a better slot than a grey rectangle, so it is
        // kept as the fallback — but a watermark or a rendering artifact is not.
        let near = VisionVerdict {
            approved: false,
            relevance_score: 0.5,
            matched_requirements: vec!["usable-in-ui".into()],
            failed_requirements: vec!["subject".into()],
            reason: "a woman by a window, not the room the brief described".into(),
        };
        assert!(!accepts(&near), "it is not a pass");
        assert!(fallback_worthy(&near), "but it is worth the slot");

        let watermarked = VisionVerdict { failed_requirements: vec!["no-unwanted-text".into()], ..near.clone() };
        assert!(!fallback_worthy(&watermarked), "a watermark is a defect a person would complain about");

        let irrelevant = VisionVerdict { relevance_score: 0.2, ..near };
        assert!(!fallback_worthy(&irrelevant), "a picture of something else is not a near miss");
    }

    #[test]
    fn a_verdict_is_found_inside_prose() {
        let text = "Here is my answer:\n{\"approved\": true, \"relevance_score\": 0.9, \"matched_requirements\": [\"subject\"], \"failed_requirements\": [], \"reason\": \"it fits\"}\nDone.";
        let verdict = parse_verdict(text).expect("the JSON is found");
        assert!(verdict.approved);
        assert!(accepts(&verdict));
    }

    #[test]
    fn a_brace_inside_a_string_does_not_end_the_object() {
        let text =
            r#"{"approved": true, "relevance_score": 0.8, "matched_requirements": [], "failed_requirements": [], "reason": "a } inside text"}"#;
        assert!(parse_verdict(text).is_ok());
    }

    #[test]
    fn prose_without_json_is_a_failed_inspection() {
        assert!(parse_verdict("I think it looks fine").is_err());
        assert!(parse_verdict("{not json}").is_err());
    }

    #[test]
    fn a_low_score_or_a_failed_mandatory_is_rejected() {
        let low =
            VisionVerdict { approved: true, relevance_score: 0.5, matched_requirements: vec![], failed_requirements: vec![], reason: "".into() };
        assert!(!accepts(&low));
        let failed = VisionVerdict {
            approved: true,
            relevance_score: 0.9,
            matched_requirements: vec![],
            failed_requirements: vec!["subject".into()],
            reason: "".into(),
        };
        assert!(!accepts(&failed));
        let optional = VisionVerdict {
            approved: true,
            relevance_score: 0.9,
            matched_requirements: vec![],
            failed_requirements: vec!["mood".into()],
            reason: "".into(),
        };
        assert!(accepts(&optional));
    }

    #[test]
    fn the_requests_argument_is_parsed() {
        let requests = r#"[{"id":"0:30","subject":"wall street trading floor","style":"photo","orientation":"landscape","exclusions":["no text"]}]"#;
        let parsed = parse_requests(requests).expect("the requests parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "0:30");
        assert_eq!(parsed[0].intent.style, "photo");
        assert_eq!(parsed[0].intent.exclusions, vec!["no text"]);
    }

    #[test]
    fn a_request_without_a_target_is_refused() {
        assert!(parse_requests(r#"[{"subject":"a fox"}]"#).is_err());
    }

    #[test]
    fn a_call_cut_off_by_the_token_limit_still_fills_the_slots_it_finished() {
        // Eight briefs is a long answer, and a model writing the eighth runs out of tokens with
        // the array still open. Refusing the whole call threw away the seven complete briefs and
        // spent another step on the same words: the slot stayed a grey box.
        let cut = r#"[{"id":"20","subject":"a woman by a window","style":"photo"},{"id":"53","subject":"an old woman at a table","query":"kitchen window light"},{"id":"55","subject":"a harbour in fog","mood":"cold, silen"#;
        let parsed = parse_requests(cut).expect("the finished briefs are kept");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, "20");
        assert_eq!(parsed[1].intent.query, "kitchen window light");
    }

    #[test]
    fn a_brace_inside_a_subject_does_not_end_the_entry() {
        let text = r#"[{"id":"7","subject":"a sign reading {OPEN} in a window"}]"#;
        let parsed = parse_requests(text).expect("it parses");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].intent.subject, "a sign reading {OPEN} in a window");
    }

    #[test]
    fn overrides_and_protected_subjects_decide_the_route() {
        let mut intent = VisualIntent { subject: "a mountain".into(), ..Default::default() };
        assert_eq!(route(&intent), ImageRoute::Auto);
        intent.style = "flat_2d_vector".into();
        assert_eq!(route(&intent), ImageRoute::ProbeThenGenerate);
        intent.palette = "warm autumn".into();
        assert_eq!(route(&intent), ImageRoute::Generate);
        intent.must_generate = true;
        assert_eq!(route(&intent), ImageRoute::Generate);
        intent.subject = "a celebrity portrait".into();
        assert_eq!(route(&intent), ImageRoute::Search, "a likeness is never generated");
    }

    #[test]
    fn every_route_but_search_only_keeps_a_fallback_behind_its_first_choice() {
        // Generation is a tier only when a library search is also a tier: a connection with no
        // image model must still be able to fill a slot with a licensed photograph, and a
        // connection with no key at all must not be told the libraries are empty.
        assert_eq!(tiers(ImageRoute::Generate), &[ImageRoute::Generate, ImageRoute::Search]);
        assert_eq!(tiers(ImageRoute::Auto), &[ImageRoute::Search, ImageRoute::Generate]);
        assert_eq!(tiers(ImageRoute::ProbeThenGenerate), &[ImageRoute::Search, ImageRoute::Generate]);
        // Search-only stays search-only: a protected likeness is never generated.
        assert_eq!(tiers(ImageRoute::Search), &[ImageRoute::Search]);
    }

    #[test]
    fn a_sourcing_result_serialises_for_the_model() {
        let result = SourcingResult {
            id: "0:30".into(),
            state: SourcingState::NoVerifiedMatch,
            reason: Some("nothing passed".into()),
            route: Some(ImageRoute::Search),
            rejected: vec![("pexels".into(), "low score".into())],
        };
        let text = serde_json::to_string(&result).expect("the result serialises");
        let back: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(back["state"], "no-verified-match");
    }
}
