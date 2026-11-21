//! The AI diagnostics log: one bounded ring of events the settings can show.
//!
//! Events are small and fixed-shape: who said what happened, at which level, with a handful of
//! attributes. Recording is silent — a full log drops its oldest entries — and nothing is ever
//! logged with a key or a message's content.

use serde::{Deserialize, Serialize};

/// What part of the app an event came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticCategory {
    Ai,
    Storage,
    Mcp,
    Document,
}

/// How important an event is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticLevel {
    Debug,
    Info,
    Warning,
    Error,
}

/// One event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    /// The event's name (`model.step.completed`, `chat.failed`, …).
    pub name: String,
    /// Where it came from.
    pub category: DiagnosticCategory,
    /// How important it is.
    pub level: DiagnosticLevel,
    /// The seconds since the epoch.
    pub timestamp: u64,
    /// Safe detail (no keys, no message content).
    #[serde(default)]
    pub attributes: Vec<(String, String)>,
}

/// The retention choices the settings offer.
pub const RETENTION: [usize; 3] = [100, 500, 1000];

/// The default retention.
pub const DEFAULT_RETENTION: usize = 500;

/// The bounded log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticLog {
    /// The events, oldest first.
    #[serde(default)]
    pub events: Vec<DiagnosticEvent>,
    /// How many are kept.
    #[serde(default = "default_retention")]
    pub retention: usize,
    /// Whether recording is on.
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn default_retention() -> usize {
    DEFAULT_RETENTION
}
fn yes() -> bool {
    true
}

impl Default for DiagnosticLog {
    fn default() -> Self {
        Self { events: Vec::new(), retention: DEFAULT_RETENTION, enabled: true }
    }
}

impl DiagnosticLog {
    /// Record one event, dropping the oldest when full.
    pub fn record(&mut self, event: DiagnosticEvent) {
        if !self.enabled {
            return;
        }
        self.events.push(event);
        let keep = self.retention.clamp(RETENTION[0], RETENTION[2]);
        while self.events.len() > keep {
            self.events.remove(0);
        }
    }

    /// A helper for a model step's usage, recorded when a step finishes.
    pub fn model_step(&mut self, provider: &str, model: &str, input: Option<u64>, output: Option<u64>, now: u64) {
        let mut attributes = vec![("provider".into(), provider.to_string()), ("model".into(), model.to_string())];
        if let Some(value) = input {
            attributes.push(("inputTokens".into(), value.to_string()));
        }
        if let Some(value) = output {
            attributes.push(("outputTokens".into(), value.to_string()));
        }
        self.record(DiagnosticEvent {
            name: "model.step.completed".into(),
            category: DiagnosticCategory::Ai,
            level: DiagnosticLevel::Info,
            timestamp: now,
            attributes,
        });
    }

    /// A helper for a failed chat run (the error's name, never its message).
    pub fn chat_failed(&mut self, error_name: &str, now: u64) {
        self.record(DiagnosticEvent {
            name: "chat.failed".into(),
            category: DiagnosticCategory::Ai,
            level: DiagnosticLevel::Error,
            timestamp: now,
            attributes: vec![("errorName".into(), error_name.to_string())],
        });
    }

    /// A storage failure's operation and the error's name.
    pub fn storage_failed(&mut self, operation: &str, error_name: &str, now: u64) {
        self.record(DiagnosticEvent {
            name: "storage.operation.failed".into(),
            category: DiagnosticCategory::Storage,
            level: DiagnosticLevel::Error,
            timestamp: now,
            attributes: vec![("operation".into(), operation.to_string()), ("errorName".into(), error_name.to_string())],
        });
    }

    /// An MCP connection failure.
    pub fn mcp_failed(&mut self, operation: &str, error: &str, now: u64) {
        self.record(DiagnosticEvent {
            name: "mcp.connection.failed".into(),
            category: DiagnosticCategory::Mcp,
            level: DiagnosticLevel::Error,
            timestamp: now,
            attributes: vec![("operation".into(), operation.to_string()), ("error".into(), error.chars().take(200).collect())],
        });
    }

    /// Forget all events.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Drop the oldest events beyond retention.
    pub fn prune(&mut self) {
        let keep = self.retention.clamp(RETENTION[0], RETENTION[2]);
        if self.events.len() > keep {
            self.events.drain(..self.events.len() - keep);
        }
    }

    /// The log exported as JSON.
    pub fn export(&self) -> String {
        serde_json::to_string_pretty(&self.events).unwrap_or_else(|_| "[]".into())
    }

    /// The most recent events first, for the panel.
    pub fn recent(&self, limit: usize) -> Vec<&DiagnosticEvent> {
        self.events.iter().rev().take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(name: &str) -> DiagnosticEvent {
        DiagnosticEvent { name: name.into(), category: DiagnosticCategory::Ai, level: DiagnosticLevel::Info, timestamp: 1, attributes: vec![] }
    }

    #[test]
    fn the_log_is_bounded() {
        let mut log = DiagnosticLog { retention: 100, ..Default::default() };
        for i in 0..200 {
            log.record(event(&format!("e{i}")));
        }
        assert_eq!(log.events.len(), 100);
        assert_eq!(log.events[0].name, "e100");
    }

    #[test]
    fn recording_can_be_turned_off() {
        let mut log = DiagnosticLog { enabled: false, ..Default::default() };
        log.model_step("openai", "gpt", Some(10), Some(2), 5);
        assert!(log.events.is_empty());
    }

    #[test]
    fn model_steps_and_failures_are_recorded() {
        let mut log = DiagnosticLog::default();
        log.model_step("openai", "gpt", Some(100), Some(20), 5);
        log.chat_failed("TimeoutError", 6);
        log.storage_failed("upload", "NetworkError", 7);
        assert_eq!(log.events.len(), 3);
        assert_eq!(log.events[0].name, "model.step.completed");
        assert_eq!(log.events[0].attributes[0].1, "openai");
        assert_eq!(log.events[1].name, "chat.failed");
        assert_eq!(log.events[2].category, DiagnosticCategory::Storage);
    }

    #[test]
    fn clear_prune_export_and_recent() {
        let mut log = DiagnosticLog::default();
        for i in 0..5 {
            log.record(event(&format!("e{i}")));
        }
        assert_eq!(log.recent(2).len(), 2);
        assert_eq!(log.recent(2)[0].name, "e4");
        assert!(log.export().contains("e4"));
        log.retention = 100;
        log.prune();
        assert_eq!(log.events.len(), 5);
        log.clear();
        assert!(log.events.is_empty());
    }
}
