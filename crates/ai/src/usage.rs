//! Usage accounting: what the models have been asked for, and what they cost in tokens.
//!
//! Every model step is recorded as an event, and the summary is folded from the events. Keeping
//! the events rather than a running total means a summary can be rebuilt, and a run that was
//! interrupted still shows what it spent.

use serde::{Deserialize, Serialize};

/// One model step's usage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageEvent {
    /// The provider the step went to.
    pub provider: String,
    /// The model that answered.
    pub model: String,
    /// The tokens read.
    #[serde(default)]
    pub input_tokens: u64,
    /// The tokens written.
    #[serde(default)]
    pub output_tokens: u64,
    /// The tokens read from the provider's cache.
    #[serde(default)]
    pub cache_read_tokens: Option<u64>,
    /// The tokens written to the provider's cache.
    #[serde(default)]
    pub cache_write_tokens: Option<u64>,
    /// Whether the step ended well.
    #[serde(default = "yes")]
    pub ok: bool,
}

fn yes() -> bool {
    true
}

impl UsageEvent {
    /// A step that went well.
    pub fn step(provider: &str, model: &str, input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            input_tokens,
            output_tokens,
            cache_read_tokens: None,
            cache_write_tokens: None,
            ok: true,
        }
    }
}

/// How many steps one model took.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageModelSummary {
    /// The provider the steps went to.
    pub provider: String,
    /// The model that answered.
    pub model: String,
    /// How many steps it took.
    pub requests: u64,
}

/// What the models have been used for.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageSummary {
    /// The steps recorded.
    pub requests: u64,
    /// The steps that ended well.
    pub completed_requests: u64,
    /// The steps that failed.
    pub failed_requests: u64,
    /// The tokens read, when the provider reported them.
    pub input_tokens: Option<u64>,
    /// The tokens written, when the provider reported them.
    pub output_tokens: Option<u64>,
    /// The tokens read from the provider's cache, when the provider reported them.
    pub cache_read_tokens: Option<u64>,
    /// The tokens written to the provider's cache, when the provider reported them.
    pub cache_write_tokens: Option<u64>,
    /// The steps whose provider reported no cache telemetry.
    pub unavailable_cache_telemetry: u64,
    /// The steps, by provider and model, busiest first.
    pub models: Vec<UsageModelSummary>,
}

/// Fold the steps into a summary.
///
/// A token count the provider did not report stays `None` rather than zero: zero would read as
/// "the model used nothing", which is a different statement from "the provider told us nothing".
pub fn summarize(events: &[UsageEvent]) -> UsageSummary {
    let mut summary = UsageSummary::default();
    let mut models: Vec<UsageModelSummary> = Vec::new();
    // Whether the provider reported the count at all.
    let mut reported = false;

    for event in events {
        summary.requests += 1;
        if event.ok {
            summary.completed_requests += 1;
        } else {
            summary.failed_requests += 1;
        }

        match models.iter_mut().find(|m| m.provider == event.provider && m.model == event.model) {
            Some(m) => m.requests += 1,
            None => models.push(UsageModelSummary { provider: event.provider.clone(), model: event.model.clone(), requests: 1 }),
        }

        summary.input_tokens = add(summary.input_tokens, event.input_tokens);
        summary.output_tokens = add(summary.output_tokens, event.output_tokens);
        summary.cache_read_tokens = add(summary.cache_read_tokens, event.cache_read_tokens.unwrap_or(0));
        summary.cache_write_tokens = add(summary.cache_write_tokens, event.cache_write_tokens.unwrap_or(0));
        reported = true;
        if event.cache_read_tokens.is_none() && event.cache_write_tokens.is_none() {
            summary.unavailable_cache_telemetry += 1;
        }
    }

    models.sort_by(|a, b| b.requests.cmp(&a.requests).then_with(|| a.provider.cmp(&b.provider)).then_with(|| a.model.cmp(&b.model)));
    summary.models = models;
    if !reported {
        // Nothing was recorded, so no count is known.
        summary.input_tokens = None;
        summary.output_tokens = None;
        summary.cache_read_tokens = None;
        summary.cache_write_tokens = None;
    }
    summary
}

/// Add a count to a total that may be unknown.
///
/// The first count seeds the total: a provider that reported tokens gives a sum, and one that
/// reported none leaves the sum unknown.
fn add(total: Option<u64>, value: u64) -> Option<u64> {
    Some(total.unwrap_or(0).saturating_add(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_history_summarises_to_nothing() {
        let s = summarize(&[]);
        assert_eq!(s.requests, 0);
        assert_eq!(s.input_tokens, None);
        assert!(s.models.is_empty());
    }

    #[test]
    fn steps_are_counted_and_token_totals_added() {
        let events = vec![
            UsageEvent::step("openai", "gpt-5.6", 100, 20),
            UsageEvent::step("openai", "gpt-5.6", 50, 10),
            UsageEvent::step("anthropic", "claude-sonnet-5", 30, 5),
        ];
        let s = summarize(&events);
        assert_eq!(s.requests, 3);
        assert_eq!(s.completed_requests, 3);
        assert_eq!(s.failed_requests, 0);
        assert_eq!(s.input_tokens, Some(180));
        assert_eq!(s.output_tokens, Some(35));
    }

    #[test]
    fn models_are_listed_busiest_first() {
        let events = vec![
            UsageEvent::step("openai", "gpt-5.6", 1, 1),
            UsageEvent::step("anthropic", "claude-sonnet-5", 1, 1),
            UsageEvent::step("anthropic", "claude-sonnet-5", 1, 1),
        ];
        let s = summarize(&events);
        assert_eq!(s.models.len(), 2);
        assert_eq!(s.models[0].provider, "anthropic");
        assert_eq!(s.models[0].requests, 2);
        assert_eq!(s.models[1].provider, "openai");
    }

    #[test]
    fn a_failed_step_is_counted_as_failed() {
        let mut e = UsageEvent::step("openai", "gpt-5.6", 10, 0);
        e.ok = false;
        let s = summarize(&[e]);
        assert_eq!(s.completed_requests, 0);
        assert_eq!(s.failed_requests, 1);
    }

    #[test]
    fn steps_without_cache_telemetry_are_counted() {
        let events = vec![UsageEvent::step("openai", "gpt-5.6", 1, 1)];
        let s = summarize(&events);
        assert_eq!(s.unavailable_cache_telemetry, 1);
        assert_eq!(s.cache_read_tokens, Some(0));
    }
}
