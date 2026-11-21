//! The provider registry: every provider FigCraft can talk to, as data.
//!
//! A provider is a named endpoint with a way to get a key and a list of models that work with it.
//! The list is the single source of truth for the settings (the model picker reads it), for the
//! connection test (it knows what to ask for) and for the image tools (a provider that ships no
//! image model simply has no image model).
//!
//! Two providers are *compatible* ones: `openai-compatible` and `anthropic-compatible` take a
//! base URL and a model id the user types, which is how a self-hosted or proxy endpoint is used.
//! They carry no models of their own.

/// A provider FigCraft can talk to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderId {
    /// Any endpoint that speaks the OpenAI chat-completions protocol.
    OpenaiCompatible,
    /// Any endpoint that speaks the Anthropic messages protocol.
    AnthropicCompatible,
    Openai,
    Anthropic,
    Google,
    Deepseek,
    Openrouter,
    /// Z.ai (the GLM family), on the Anthropic protocol.
    Zai,
    /// MiniMax, on the OpenAI protocol.
    Minimax,
    /// An external agent that speaks the Agent Client Protocol over a pipe.
    Acp,
}

impl ProviderId {
    /// The provider's id as it appears in settings and on the wire.
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenaiCompatible => "openai-compatible",
            Self::AnthropicCompatible => "anthropic-compatible",
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::Deepseek => "deepseek",
            Self::Openrouter => "openrouter",
            Self::Zai => "zai",
            Self::Minimax => "minimax",
            Self::Acp => "acp",
        }
    }

    /// The provider with this id, if it is one FigCraft knows.
    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "openai-compatible" => Self::OpenaiCompatible,
            "anthropic-compatible" => Self::AnthropicCompatible,
            "openai" => Self::Openai,
            "anthropic" => Self::Anthropic,
            "google" => Self::Google,
            "deepseek" => Self::Deepseek,
            "openrouter" => Self::Openrouter,
            "zai" => Self::Zai,
            "minimax" => Self::Minimax,
            "acp" => Self::Acp,
            _ => return None,
        })
    }

    /// Whether the provider takes a base URL the user types.
    pub fn supports_custom_base_url(self) -> bool {
        matches!(self, Self::OpenaiCompatible | Self::AnthropicCompatible)
    }

    /// Whether the provider takes a model id the user types.
    pub fn supports_custom_model(self) -> bool {
        matches!(self, Self::OpenaiCompatible | Self::AnthropicCompatible | Self::Openrouter)
    }

    /// Whether the provider is an external agent rather than a model endpoint.
    pub fn is_agent(self) -> bool {
        matches!(self, Self::Acp)
    }

    /// The wire protocol the provider speaks.
    pub fn protocol(self) -> Protocol {
        match self {
            Self::Anthropic | Self::AnthropicCompatible | Self::Zai => Protocol::Anthropic,
            Self::Google => Protocol::Google,
            Self::Openai | Self::OpenaiCompatible | Self::Deepseek | Self::Openrouter | Self::Minimax | Self::Acp => Protocol::Openai,
        }
    }

    /// The provider's default endpoint, unless it is a compatible one (those take the user's).
    pub fn default_base_url(self) -> Option<&'static str> {
        Some(match self {
            Self::Openai => "https://api.openai.com/v1",
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::Google => "https://generativelanguage.googleapis.com/v1beta",
            Self::Deepseek => "https://api.deepseek.com/v1",
            Self::Openrouter => "https://openrouter.ai/api/v1",
            Self::Zai => "https://api.z.ai/api/anthropic",
            Self::Minimax => "https://api.minimax.io/v1",
            Self::OpenaiCompatible | Self::AnthropicCompatible | Self::Acp => return None,
        })
    }

    /// The image-generation model the provider ships, if it ships one.
    ///
    /// The image model is a different product from the chat model: a text model id is never valid
    /// here, so the id is fixed per provider rather than chosen from the chat list.
    pub fn image_model(self) -> Option<&'static str> {
        Some(match self {
            Self::Openai => "gpt-image-1",
            Self::Google => "imagen-4.0-generate-001",
            Self::Openrouter => "google/imagen-4",
            _ => return None,
        })
    }
}

/// The wire protocol a provider speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    /// An external agent over the Agent Client Protocol on its pipes.
    Acp,
    /// The OpenAI chat-completions protocol (`POST /chat/completions`).
    Openai,
    /// The Anthropic messages protocol (`POST /messages`).
    Anthropic,
    /// The Google `generateContent` protocol.
    Google,
}

/// What a model can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// The model calls tools (it can act on the document).
    Tools,
    /// The model reads images.
    Vision,
}

/// A model a provider offers.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelOption {
    /// The id the provider expects.
    pub id: &'static str,
    /// The name shown in the model picker.
    pub name: &'static str,
    /// A short note ("Fast", "Free"), shown beside the name.
    pub tag: Option<&'static str>,
    /// What the model can do.
    pub capabilities: &'static [Capability],
}

/// A provider and the models it offers.
#[derive(Clone, Debug, PartialEq)]
pub struct AIProviderDef {
    pub id: ProviderId,
    /// The name shown in the provider picker.
    pub name: &'static str,
    /// What the key field shows as its placeholder ("sk-ant-…").
    pub key_placeholder: &'static str,
    /// Where a person gets a key, opened from the settings.
    pub key_url: &'static str,
    /// The model a fresh connection starts on.
    pub default_model: &'static str,
    /// The models the provider offers.
    pub models: &'static [ModelOption],
}

impl AIProviderDef {
    /// Whether the provider takes a base URL the user types.
    pub fn supports_custom_base_url(&self) -> bool {
        self.id.supports_custom_base_url()
    }

    /// Whether the provider takes a model id the user types.
    pub fn supports_custom_model(&self) -> bool {
        self.id.supports_custom_model()
    }

    /// The model with this id, if the provider offers it.
    pub fn model(&self, id: &str) -> Option<&'static ModelOption> {
        self.models.iter().find(|m| m.id == id)
    }
}

/// Every provider, in the order the picker shows them.
pub fn providers() -> &'static [AIProviderDef] {
    PROVIDERS
}

/// The provider with this id.
pub fn provider(id: ProviderId) -> Option<&'static AIProviderDef> {
    PROVIDERS.iter().find(|p| p.id == id)
}

static PROVIDERS: &[AIProviderDef] = &[
    AIProviderDef {
        id: ProviderId::OpenaiCompatible,
        name: "OpenAI-compatible",
        key_placeholder: "API key",
        key_url: "",
        default_model: "",
        models: &[],
    },
    AIProviderDef {
        id: ProviderId::AnthropicCompatible,
        name: "Anthropic-compatible",
        key_placeholder: "API key",
        key_url: "",
        default_model: "",
        models: &[],
    },
    AIProviderDef {
        id: ProviderId::Openai,
        name: "OpenAI",
        key_placeholder: "sk-…",
        key_url: "https://platform.openai.com/api-keys",
        default_model: "gpt-5.6",
        models: &[
            ModelOption { id: "gpt-5.6", name: "GPT-5.6", tag: Some("Best"), capabilities: &[Capability::Tools, Capability::Vision] },
            ModelOption { id: "gpt-5.5", name: "GPT-5.5", tag: None, capabilities: &[Capability::Tools, Capability::Vision] },
            ModelOption { id: "gpt-5.4-mini", name: "GPT-5.4 mini", tag: Some("Fast"), capabilities: &[Capability::Tools, Capability::Vision] },
            ModelOption { id: "gpt-5.4-nano", name: "GPT-5.4 nano", tag: Some("Cheap"), capabilities: &[Capability::Tools, Capability::Vision] },
        ],
    },
    AIProviderDef {
        id: ProviderId::Anthropic,
        name: "Anthropic",
        key_placeholder: "sk-ant-…",
        key_url: "https://console.anthropic.com/settings/keys",
        default_model: "claude-sonnet-5",
        models: &[
            ModelOption {
                id: "claude-sonnet-5",
                name: "Claude Sonnet 5",
                tag: Some("Best for design"),
                capabilities: &[Capability::Tools, Capability::Vision],
            },
            ModelOption { id: "claude-opus-5", name: "Claude Opus 5", tag: Some("Smartest"), capabilities: &[Capability::Tools, Capability::Vision] },
            ModelOption {
                id: "claude-fable-5-1",
                name: "Claude Fable 5.1",
                tag: Some("Latest"),
                capabilities: &[Capability::Tools, Capability::Vision],
            },
        ],
    },
    AIProviderDef {
        id: ProviderId::Google,
        name: "Google AI",
        key_placeholder: "AIza…",
        key_url: "https://aistudio.google.com/apikey",
        default_model: "gemini-3.8-flash",
        models: &[
            ModelOption {
                id: "gemini-3.8-flash",
                name: "Gemini 3.8 Flash",
                tag: Some("Latest"),
                capabilities: &[Capability::Tools, Capability::Vision],
            },
            ModelOption { id: "gemini-3.7-flash", name: "Gemini 3.7 Flash", tag: None, capabilities: &[Capability::Tools, Capability::Vision] },
            ModelOption {
                id: "gemini-flash-latest",
                name: "Gemini Flash Latest",
                tag: Some("Alias"),
                capabilities: &[Capability::Tools, Capability::Vision],
            },
            ModelOption {
                id: "gemini-3.1-pro-preview",
                name: "Gemini 3.1 Pro",
                tag: Some("Pro"),
                capabilities: &[Capability::Tools, Capability::Vision],
            },
        ],
    },
    AIProviderDef {
        id: ProviderId::Deepseek,
        name: "DeepSeek",
        key_placeholder: "sk-…",
        key_url: "https://platform.deepseek.com/api_keys",
        default_model: "deepseek-v4-flash",
        models: &[
            ModelOption { id: "deepseek-v4-flash", name: "DeepSeek V4 Flash", tag: Some("Fast"), capabilities: &[] },
            ModelOption { id: "deepseek-v4-pro", name: "DeepSeek V4 Pro", tag: Some("Reasoning"), capabilities: &[] },
        ],
    },
    AIProviderDef {
        id: ProviderId::Openrouter,
        name: "OpenRouter",
        key_placeholder: "sk-or-…",
        key_url: "https://openrouter.ai/keys",
        default_model: "anthropic/claude-sonnet-5",
        models: &[
            ModelOption {
                id: "anthropic/claude-sonnet-5",
                name: "Claude Sonnet 5",
                tag: Some("Best for design"),
                capabilities: &[Capability::Tools, Capability::Vision],
            },
            ModelOption {
                id: "anthropic/claude-opus-5",
                name: "Claude Opus 5",
                tag: Some("Smartest"),
                capabilities: &[Capability::Tools, Capability::Vision],
            },
            ModelOption {
                id: "anthropic/claude-fable-5.1",
                name: "Claude Fable 5.1",
                tag: Some("Latest Anthropic"),
                capabilities: &[Capability::Tools, Capability::Vision],
            },
            ModelOption { id: "openai/gpt-5.6", name: "GPT-5.6", tag: Some("Latest OpenAI"), capabilities: &[Capability::Tools, Capability::Vision] },
            ModelOption {
                id: "google/gemini-3.8-flash",
                name: "Gemini 3.8 Flash",
                tag: Some("Fast"),
                capabilities: &[Capability::Tools, Capability::Vision],
            },
            ModelOption { id: "z-ai/glm-5.3", name: "GLM-5.3", tag: None, capabilities: &[Capability::Tools] },
            ModelOption { id: "deepseek/deepseek-v4-pro", name: "DeepSeek V4 Pro", tag: Some("Reasoning"), capabilities: &[Capability::Tools] },
            ModelOption {
                id: "moonshotai/kimi-k3",
                name: "Kimi K3",
                tag: Some("Vision + code"),
                capabilities: &[Capability::Tools, Capability::Vision],
            },
            ModelOption { id: "qwen/qwen3-coder:free", name: "Qwen3 Coder", tag: Some("Free"), capabilities: &[] },
            ModelOption { id: "openai/gpt-oss-120b:free", name: "GPT-OSS 120B", tag: Some("Free"), capabilities: &[] },
        ],
    },
    AIProviderDef {
        id: ProviderId::Zai,
        name: "Z.ai",
        key_placeholder: "API key",
        key_url: "https://docs.z.ai/devpack/quick-start",
        default_model: "glm-5.3",
        models: &[
            ModelOption { id: "glm-5.3", name: "GLM-5.3", tag: Some("Best"), capabilities: &[] },
            ModelOption { id: "glm-5.3-flash", name: "GLM-5.3-Flash", tag: Some("Fast"), capabilities: &[] },
            ModelOption { id: "glm-5.2", name: "GLM-5.2", tag: None, capabilities: &[] },
            ModelOption { id: "glm-5v-turbo", name: "GLM-5V-Turbo", tag: Some("Vision"), capabilities: &[Capability::Tools, Capability::Vision] },
            ModelOption { id: "glm-5.1", name: "GLM-5.1", tag: None, capabilities: &[] },
            ModelOption { id: "glm-5", name: "GLM-5", tag: None, capabilities: &[] },
            ModelOption { id: "glm-5-code", name: "GLM-5-Code", tag: None, capabilities: &[] },
            ModelOption { id: "glm-4.7", name: "GLM-4.7", tag: None, capabilities: &[] },
            ModelOption { id: "glm-4.7-flashx", name: "GLM-4.7-FlashX", tag: None, capabilities: &[] },
            ModelOption { id: "glm-4.6", name: "GLM-4.6", tag: None, capabilities: &[] },
            ModelOption { id: "glm-4.5", name: "GLM-4.5", tag: None, capabilities: &[] },
            ModelOption { id: "glm-4.5-x", name: "GLM-4.5-X", tag: None, capabilities: &[] },
            ModelOption { id: "glm-4.5-air", name: "GLM-4.5-Air", tag: None, capabilities: &[] },
            ModelOption { id: "glm-4.5-airx", name: "GLM-4.5-AirX", tag: None, capabilities: &[] },
            ModelOption { id: "glm-4-32b-0414-128k", name: "GLM-4-32B-0414-128K", tag: None, capabilities: &[] },
            ModelOption { id: "glm-4.7-flash", name: "GLM-4.7-Flash", tag: Some("Free"), capabilities: &[] },
            ModelOption { id: "glm-4.5-flash", name: "GLM-4.5-Flash", tag: Some("Free"), capabilities: &[] },
        ],
    },
    AIProviderDef {
        id: ProviderId::Minimax,
        name: "MiniMax",
        key_placeholder: "API key",
        key_url: "https://platform.minimax.io/user-center/basic-information/interface-key",
        default_model: "minimax-m3",
        models: &[
            ModelOption { id: "minimax-m3", name: "MiniMax-M3", tag: Some("Best"), capabilities: &[] },
            ModelOption { id: "minimax-m2.7", name: "MiniMax-M2.7", tag: None, capabilities: &[] },
            ModelOption { id: "minimax-m2.7-highspeed", name: "MiniMax-M2.7-highspeed", tag: Some("Fast"), capabilities: &[] },
            ModelOption { id: "minimax-m2.5", name: "MiniMax-M2.5", tag: None, capabilities: &[] },
            ModelOption { id: "minimax-m2.5-highspeed", name: "MiniMax-M2.5 Highspeed", tag: Some("Fast"), capabilities: &[] },
            ModelOption { id: "minimax-m2.1", name: "MiniMax-M2.1", tag: None, capabilities: &[] },
            ModelOption { id: "minimax-m2.1-highspeed", name: "MiniMax-M2.1 Highspeed", tag: None, capabilities: &[] },
            ModelOption { id: "minimax-m2", name: "MiniMax-M2", tag: None, capabilities: &[] },
        ],
    },
    AIProviderDef { id: ProviderId::Acp, name: "External agent", key_placeholder: "API key", key_url: "", default_model: "", models: &[] },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_has_a_stable_id_that_round_trips() {
        for p in providers() {
            assert_eq!(ProviderId::from_id(p.id.id()), Some(p.id), "{}", p.id.id());
        }
    }

    #[test]
    fn the_registry_has_no_duplicate_providers_or_models() {
        let mut ids = Vec::new();
        for p in providers() {
            assert!(!ids.contains(&p.id), "{} is listed twice", p.name);
            ids.push(p.id);
            let mut seen = Vec::new();
            for m in p.models {
                assert!(!seen.contains(&m.id), "{} lists {} twice", p.name, m.id);
                seen.push(m.id);
            }
        }
    }

    #[test]
    fn a_default_model_is_one_the_provider_offers() {
        for p in providers() {
            if p.default_model.is_empty() {
                continue;
            }
            assert!(p.model(p.default_model).is_some(), "{} defaults on an unlisted model", p.name);
        }
    }

    #[test]
    fn only_compatible_providers_take_a_base_url() {
        for p in providers() {
            assert_eq!(p.supports_custom_base_url(), p.id.supports_custom_base_url());
            if p.supports_custom_base_url() {
                assert!(p.id.default_base_url().is_none(), "{} takes a base URL, so it has no default", p.name);
                assert!(p.models.is_empty(), "{} takes any model, so it lists none", p.name);
            }
        }
    }

    #[test]
    fn a_model_endpoint_has_a_default_url_and_an_agent_has_none() {
        for p in providers() {
            if p.id.is_agent() || p.supports_custom_base_url() {
                assert!(p.id.default_base_url().is_none(), "{} takes its own URL", p.name);
            } else {
                assert!(p.id.default_base_url().is_some(), "{} needs a default endpoint", p.name);
            }
        }
    }

    #[test]
    fn only_openai_google_and_openrouter_ship_an_image_model() {
        for p in providers() {
            assert_eq!(p.id.image_model().is_some(), matches!(p.id, ProviderId::Openai | ProviderId::Google | ProviderId::Openrouter));
        }
    }
}
