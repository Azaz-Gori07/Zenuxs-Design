//! Model connections, profiles and role assignments.
//!
//! Three things are kept apart on purpose:
//!
//! - a **connection** is *where* a model lives: a provider, a base URL, an API type and the
//!   credential the connection uses. Two models on the same key share one connection.
//! - a **profile** is *which* model: a name, the model id and what the model can do.
//! - the **assignments** say *which* profile handles which kind of work: the design agent, the
//!   reviewer, the fast tasks and the vision model.
//!
//! Keeping them apart is what lets a person add a second model on one key without touching the
//! first, and swap the model behind a role without re-entering the key.

use serde::{Deserialize, Serialize};

use crate::providers::{Capability, ProviderId};

/// What a model profile can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AIModelCapability {
    /// The model calls tools (it can act on the document).
    Tools,
    /// The model reads images.
    Vision,
}

impl From<AIModelCapability> for Capability {
    fn from(c: AIModelCapability) -> Self {
        match c {
            AIModelCapability::Tools => Capability::Tools,
            AIModelCapability::Vision => Capability::Vision,
        }
    }
}

/// The kind of work a model is assigned to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AIModelRole {
    /// The agent that talks to the user and changes the document.
    Design,
    /// The reviewer that checks a plan or a design.
    Review,
    /// Cheap background work.
    Fast,
    /// Reading screenshots and images.
    Vision,
}

impl AIModelRole {
    /// The role's id, as it appears in settings and on the wire.
    pub fn id(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Review => "review",
            Self::Fast => "fast",
            Self::Vision => "vision",
        }
    }

    /// The role with this id.
    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "design" => Self::Design,
            "review" => Self::Review,
            "fast" => Self::Fast,
            "vision" => Self::Vision,
            _ => return None,
        })
    }

    /// Every role, in the order the settings list them.
    pub fn all() -> &'static [Self] {
        &[Self::Design, Self::Review, Self::Fast, Self::Vision]
    }
}

/// Which API shape a compatible endpoint speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum APIType {
    /// `POST /chat/completions`.
    Completions,
    /// `POST /responses`.
    Responses,
}

impl APIType {
    /// The API type's id.
    pub fn id(self) -> &'static str {
        match self {
            Self::Completions => "completions",
            Self::Responses => "responses",
        }
    }

    /// The API type with this id.
    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "completions" => Self::Completions,
            "responses" => Self::Responses,
            _ => return None,
        })
    }
}

/// A place a model lives: a provider, an endpoint and a key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AIModelConnection {
    /// The connection's id (`connection-…`), unique among the connections.
    pub id: String,
    /// The provider the connection talks to.
    pub provider: ProviderId,
    /// The endpoint, for a compatible provider; the provider's own for the others.
    #[serde(default)]
    pub custom_base_url: String,
    /// Which API shape a compatible endpoint speaks.
    #[serde(default = "default_api_type")]
    pub custom_api_type: APIType,
    /// The credential the connection uses (`v1:<integration>:<profile>:<field>`).
    #[serde(default)]
    pub credential_profile_id: String,
}

fn default_api_type() -> APIType {
    APIType::Completions
}

/// A model the user configured.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AIModelProfile {
    /// The profile's id (`model-…`), unique among the profiles.
    pub id: String,
    /// The name shown in the model picker.
    pub name: String,
    /// The connection the profile uses.
    pub connection_id: String,
    /// The provider's own model id.
    #[serde(default)]
    pub model_id: String,
    /// A model id the user typed, for a provider that takes one.
    #[serde(default)]
    pub custom_model_id: String,
    /// The most the model may write in one answer.
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32,
    /// The provider's own reasoning setting, when it takes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// What the model can do.
    #[serde(default)]
    pub capabilities: Vec<AIModelCapability>,
}

fn default_max_output_tokens() -> u32 {
    8192
}

/// Which profile handles which kind of work.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AIModelAssignments {
    /// The design agent's profile.
    pub design: String,
    /// The reviewer's profile, or empty when none is set.
    #[serde(default)]
    pub review: String,
    /// The fast tasks' profile, or empty when none is set.
    #[serde(default)]
    pub fast: String,
    /// The vision model's profile, or empty when none is set.
    #[serde(default)]
    pub vision: String,
}

/// Everything the AI settings hold.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AIModelSettings {
    /// The settings' version, so a later one can migrate them.
    pub version: u32,
    /// The connections.
    #[serde(default)]
    pub connections: Vec<AIModelConnection>,
    /// The model profiles.
    #[serde(default)]
    pub models: Vec<AIModelProfile>,
    /// The role assignments.
    #[serde(default)]
    pub assignments: AIModelAssignments,
}

impl Default for AIModelSettings {
    fn default() -> Self {
        Self { version: 1, connections: Vec::new(), models: Vec::new(), assignments: AIModelAssignments::default() }
    }
}

/// A profile and the connection behind it, resolved for a role.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedAIModelRole {
    /// The role that was asked for.
    pub role: AIModelRole,
    /// The profile that handles it.
    pub profile: AIModelProfile,
    /// The connection the profile uses.
    pub connection: AIModelConnection,
}

impl AIModelSettings {
    /// The connection with this id.
    pub fn connection(&self, id: &str) -> Option<&AIModelConnection> {
        self.connections.iter().find(|c| c.id == id)
    }

    /// The profile with this id.
    pub fn profile(&self, id: &str) -> Option<&AIModelProfile> {
        self.models.iter().find(|m| m.id == id)
    }

    /// The profile and connection that handle `role`, if the role is assigned and both exist.
    ///
    /// The role falls back to the design profile for the roles a person may leave empty: a
    /// reviewer that is not set is reviewed by the design model, which is the model already
    /// configured. The vision role does *not* fall back: a text model cannot read an image, and
    /// pretending it can would attach a picture the model never saw.
    pub fn resolve(&self, role: AIModelRole) -> Option<ResolvedAIModelRole> {
        let id = match role {
            AIModelRole::Design => self.assignments.design.as_str(),
            AIModelRole::Review if self.assignments.review.is_empty() => self.assignments.design.as_str(),
            AIModelRole::Fast if self.assignments.fast.is_empty() => self.assignments.design.as_str(),
            AIModelRole::Review => self.assignments.review.as_str(),
            AIModelRole::Fast => self.assignments.fast.as_str(),
            AIModelRole::Vision => self.assignments.vision.as_str(),
        };
        if id.is_empty() {
            return None;
        }
        let profile = self.profile(id)?.clone();
        let connection = self.connection(&profile.connection_id)?.clone();
        Some(ResolvedAIModelRole { role, profile, connection })
    }

    /// The model id the provider expects for a profile: the custom id when the user typed one,
    /// the provider's own otherwise.
    pub fn effective_model_id(profile: &AIModelProfile) -> &str {
        let custom = profile.custom_model_id.trim();
        if custom.is_empty() { profile.model_id.trim() } else { custom }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> AIModelSettings {
        AIModelSettings {
            version: 1,
            connections: vec![AIModelConnection {
                id: "connection-1".into(),
                provider: ProviderId::Openai,
                custom_base_url: String::new(),
                custom_api_type: APIType::Completions,
                credential_profile_id: "connection-1".into(),
            }],
            models: vec![
                AIModelProfile {
                    id: "model-1".into(),
                    name: "Main".into(),
                    connection_id: "connection-1".into(),
                    model_id: "gpt-5.6".into(),
                    custom_model_id: String::new(),
                    max_output_tokens: 8192,
                    reasoning_effort: None,
                    capabilities: vec![AIModelCapability::Tools],
                },
                AIModelProfile {
                    id: "model-2".into(),
                    name: "Reader".into(),
                    connection_id: "connection-1".into(),
                    model_id: "gpt-5.6".into(),
                    custom_model_id: String::new(),
                    max_output_tokens: 8192,
                    reasoning_effort: None,
                    capabilities: vec![AIModelCapability::Vision],
                },
            ],
            assignments: AIModelAssignments { design: "model-1".into(), review: String::new(), fast: String::new(), vision: "model-2".into() },
        }
    }

    #[test]
    fn a_role_resolves_to_its_profile_and_connection() {
        let s = settings();
        let r = s.resolve(AIModelRole::Design).expect("the design role is assigned");
        assert_eq!(r.profile.id, "model-1");
        assert_eq!(r.connection.id, "connection-1");
    }

    #[test]
    fn an_unset_review_falls_back_to_the_design_model_but_vision_does_not() {
        let s = settings();
        assert_eq!(s.resolve(AIModelRole::Review).map(|r| r.profile.id), Some("model-1".into()));
        assert_eq!(s.resolve(AIModelRole::Fast).map(|r| r.profile.id), Some("model-1".into()));
        assert_eq!(s.resolve(AIModelRole::Vision).map(|r| r.profile.id), Some("model-2".into()));

        let mut s = settings();
        s.assignments.vision = String::new();
        assert!(s.resolve(AIModelRole::Vision).is_none(), "a text model must not be used for images");
    }

    #[test]
    fn a_role_assigned_to_a_missing_profile_resolves_to_nothing() {
        let mut s = settings();
        s.assignments.design = "model-gone".into();
        assert!(s.resolve(AIModelRole::Design).is_none());
    }

    #[test]
    fn a_custom_model_id_wins_over_the_provider_model() {
        let mut p = settings().models.remove(0);
        assert_eq!(AIModelSettings::effective_model_id(&p), "gpt-5.6");
        p.custom_model_id = "  llama-3.3-70b ".into();
        assert_eq!(AIModelSettings::effective_model_id(&p), "llama-3.3-70b");
    }

    #[test]
    fn settings_survive_a_round_trip_through_json() {
        let s = settings();
        let text = serde_json::to_string(&s).expect("settings serialise");
        let back: AIModelSettings = serde_json::from_str(&text).expect("settings deserialise");
        assert_eq!(s, back);
    }

    #[test]
    fn an_old_settings_file_without_connections_still_loads() {
        let s: AIModelSettings = serde_json::from_str(r#"{"version":1}"#).expect("settings load");
        assert!(s.connections.is_empty());
        assert_eq!(s.assignments, AIModelAssignments::default());
    }
}
