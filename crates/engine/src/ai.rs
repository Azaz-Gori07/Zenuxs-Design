//! The AI settings the engine keeps: the models, the skills and the usage.
//!
//! The engine holds the settings, not the models themselves: a key lives in the credential
//! store (which the platform provides), and the conversation lives in the interface. What the
//! engine owns is what must survive a restart and be readable by an agent: which models exist,
//! which one handles which kind of work, which skills are switched on, and what the models have
//! been used for.
//!
//! Everything here is a command ([`crate::cmd::ai`]), so the interface, the CLI, the control
//! channel and the MCP server all reach the same settings the same way.

use figcraft_ai::credentials::{CredentialRef, CredentialStatus, CredentialStore};
use figcraft_ai::models::{AIModelRole, AIModelSettings};
use figcraft_ai::skills::prompt::PromptSkillStore;
use figcraft_ai::usage::UsageEvent;
use serde::{Deserialize, Serialize};

/// The AI settings the preferences keep.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AISettings {
    /// The model connections, the model profiles and the role assignments.
    #[serde(default)]
    pub models: AIModelSettings,
    /// The standing instructions a person wrote.
    #[serde(default)]
    pub skills: PromptSkillStore,
    /// The MCP servers the agent may call tools on.
    #[serde(default)]
    pub mcp: figcraft_ai::mcp::MCPSettings,
    /// The cloud server the account uses (`http://host:47901`), when one is set.
    #[serde(default)]
    pub account_server: String,
    /// The pending cloud uploads and their state.
    #[serde(default)]
    pub sync: figcraft_ai::SyncOutbox,
    /// The last list of the account's cloud documents, kept so the settings can show it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cloud_documents: Vec<figcraft_ai::cloud::CloudDocument>,
    /// The saved chat conversations.
    #[serde(default)]
    pub chat_history: figcraft_ai::ChatHistory,
    /// How the model's reasoning is shown.
    #[serde(default)]
    pub reasoning_display: figcraft_ai::ReasoningDisplay,
    /// Whether the tools' steps show in the transcript.
    #[serde(default = "yes")]
    pub show_tools: bool,
    /// The image vectorization service.
    #[serde(default)]
    pub vectorize_provider: figcraft_ai::VectorizeProvider,
    /// The cloud storage provider.
    #[serde(default)]
    pub storage_provider: figcraft_ai::StorageProvider,
    /// The S3-compatible service's settings.
    #[serde(default)]
    pub s3: figcraft_ai::S3Settings,
    /// The last cloud workspace listing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub storage_documents: Vec<figcraft_ai::StorageDocument>,
    /// The bounded AI diagnostics log.
    #[serde(default)]
    pub diagnostics: figcraft_ai::DiagnosticLog,
}

fn yes() -> bool {
    true
}

impl Default for AISettings {
    /// A fresh install shows tool steps, the same as a preferences file that omits the field
    /// (`#[serde(default = "yes")]` above). Keeping the two paths in step avoids a checkbox that
    /// changes on its own the first time the preferences are saved and loaded.
    fn default() -> Self {
        Self {
            models: AIModelSettings::default(),
            skills: PromptSkillStore::default(),
            mcp: figcraft_ai::mcp::MCPSettings::default(),
            account_server: String::new(),
            sync: figcraft_ai::SyncOutbox::default(),
            cloud_documents: Vec::new(),
            chat_history: figcraft_ai::ChatHistory::default(),
            reasoning_display: figcraft_ai::ReasoningDisplay::default(),
            show_tools: true,
            vectorize_provider: figcraft_ai::VectorizeProvider::default(),
            storage_provider: figcraft_ai::StorageProvider::default(),
            s3: figcraft_ai::S3Settings::default(),
            storage_documents: Vec::new(),
            diagnostics: figcraft_ai::DiagnosticLog::default(),
        }
    }
}

/// The most usage events kept; the oldest are dropped.
pub const MAX_USAGE_EVENTS: usize = 500;

/// The AI state a session keeps while it runs.
#[derive(Default)]
pub struct AIState {
    /// What the models have been used for, oldest first.
    pub usage: Vec<UsageEvent>,
}

impl AIState {
    /// An empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one model step.
    pub fn record(&mut self, event: UsageEvent) {
        self.usage.push(event);
        if self.usage.len() > MAX_USAGE_EVENTS {
            let excess = self.usage.len() - MAX_USAGE_EVENTS;
            self.usage.drain(..excess);
        }
    }

    /// Forget everything recorded.
    pub fn clear(&mut self) {
        self.usage.clear();
    }

    /// What the models have been used for.
    pub fn summary(&self) -> figcraft_ai::usage::UsageSummary {
        figcraft_ai::usage::summarize(&self.usage)
    }
}

/// Milliseconds since the epoch, for the sync backoff (`web_time` works in the browser too).
pub fn now_ms() -> u64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// A little jitter for the backoff: the low bits of the clock, in 0..1.
pub fn jitter() -> f64 {
    (now_ms().rem_euclid(1000) as f64) / 1000.0
}

/// The credential reference a connection's key is stored under.
///
/// The key belongs to the provider and to the connection, so two connections on one provider
/// keep their own keys.
pub fn connection_credential_ref(provider: &str, connection_id: &str) -> CredentialRef {
    CredentialRef::provider_key(provider, connection_id)
}

/// Whether a key is stored for a reference.
pub fn credential_status(store: &dyn CredentialStore, reference: &CredentialRef) -> CredentialStatus {
    store.status(reference).unwrap_or(CredentialStatus::Unavailable)
}

/// The model that handles a role, if one is assigned and both it and its connection exist.
pub fn role_model(settings: &AIModelSettings, role: AIModelRole) -> Option<figcraft_ai::models::ResolvedAIModelRole> {
    settings.resolve(role)
}

/// The default local proxy endpoint for keyless free AI models.
pub const SEED_ENDPOINT: &str = "http://127.0.0.1:20129/v1";
/// The default free model.
pub const SEED_MODEL: &str = "deepseek-v4.1-flash-free";
/// The key expected by the local proxy.
pub const SEED_KEY: &str = "public";

/// Seed default AI models and credentials if none are configured, and ensure all prompt skills are enabled.
pub fn seed_defaults(prefs: &mut crate::Prefs, credentials: Option<&mut Box<dyn CredentialStore>>) {
    use figcraft_ai::models::{AIModelAssignments, AIModelCapability, AIModelConnection, AIModelProfile, AIModelSettings, APIType};
    use figcraft_ai::providers::ProviderId;

    let models = &mut prefs.ai.models;
    if models.connections.is_empty() || models.models.is_empty() {
        let connection = AIModelConnection {
            id: "connection-page".into(),
            provider: ProviderId::OpenaiCompatible,
            custom_base_url: SEED_ENDPOINT.into(),
            custom_api_type: APIType::Completions,
            credential_profile_id: String::new(),
        };
        let profile = AIModelProfile {
            id: "model-page".into(),
            name: SEED_MODEL.into(),
            connection_id: connection.id.clone(),
            model_id: SEED_MODEL.into(),
            custom_model_id: String::new(),
            max_output_tokens: 8192,
            reasoning_effort: None,
            capabilities: vec![AIModelCapability::Tools, AIModelCapability::Vision],
        };
        *models = AIModelSettings {
            version: 1,
            connections: vec![connection],
            models: vec![profile],
            assignments: AIModelAssignments {
                design: "model-page".into(),
                review: "model-page".into(),
                fast: "model-page".into(),
                vision: "model-page".into(),
            },
        };
    }

    if let Some(store) = credentials {
        for connection in &prefs.ai.models.connections {
            if connection.id == "connection-page" || connection.custom_base_url.contains("20129") {
                let reference = crate::cmd::ai::credential_ref_for(connection);
                if store.read(&reference).ok().flatten().is_none_or(|k| k.trim().is_empty()) {
                    let _ = store.write(&reference, SEED_KEY);
                }
            }
        }
    }

    // Ensure all default prompt skills are enabled unless explicitly disabled
    for default in figcraft_ai::skills::prompt::default_prompt_skills() {
        if !prefs.ai.skills.skills.iter().any(|s| (s.id == default.id || s.name == default.name) && !s.enabled) {
            prefs.ai.skills.set_enabled(&default.id, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use figcraft_ai::ProviderId;
    use figcraft_ai::credentials::MemoryCredentialStore;
    use figcraft_ai::models::{AIModelConnection, AIModelProfile};

    #[test]
    fn the_usage_log_keeps_the_most_recent_events() {
        let mut state = AIState::new();
        for i in 0..(MAX_USAGE_EVENTS + 10) {
            state.record(UsageEvent::step("openai", "gpt-5.6", i as u64, 1));
        }
        assert_eq!(state.usage.len(), MAX_USAGE_EVENTS);
        // The oldest went first.
        assert_eq!(state.usage[0].input_tokens, 10);
    }

    #[test]
    fn clearing_the_usage_forgets_it() {
        let mut state = AIState::new();
        state.record(UsageEvent::step("openai", "gpt-5.6", 1, 1));
        assert_eq!(state.summary().requests, 1);
        state.clear();
        assert_eq!(state.summary().requests, 0);
    }

    #[test]
    fn a_key_belongs_to_the_provider_and_the_connection() {
        let r = connection_credential_ref("openai", "connection-1");
        assert_eq!(r.storage_key(), "v1:openai:connection-1:api-key");
    }

    #[test]
    fn a_missing_key_reads_as_missing_and_a_stored_one_as_configured() {
        let mut store = MemoryCredentialStore::new();
        let r = connection_credential_ref("openai", "connection-1");
        assert_eq!(credential_status(&store, &r), CredentialStatus::Missing);
        store.write(&r, "sk-123").expect("the key is stored");
        assert_eq!(credential_status(&store, &r), CredentialStatus::Configured);
    }

    #[test]
    fn a_role_resolves_only_when_its_model_and_connection_exist() {
        let settings = AIModelSettings {
            version: 1,
            connections: vec![AIModelConnection {
                id: "connection-1".into(),
                provider: ProviderId::Openai,
                custom_base_url: String::new(),
                custom_api_type: figcraft_ai::models::APIType::Completions,
                credential_profile_id: "connection-1".into(),
            }],
            models: vec![AIModelProfile {
                id: "model-1".into(),
                name: "Main".into(),
                connection_id: "connection-1".into(),
                model_id: "gpt-5.6".into(),
                custom_model_id: String::new(),
                max_output_tokens: 8192,
                reasoning_effort: None,
                capabilities: vec![figcraft_ai::models::AIModelCapability::Tools],
            }],
            assignments: figcraft_ai::models::AIModelAssignments { design: "model-1".into(), ..Default::default() },
        };
        assert_eq!(role_model(&settings, AIModelRole::Design).map(|r| r.profile.id), Some("model-1".into()));
        assert!(role_model(&settings, AIModelRole::Vision).is_none());
    }
}
