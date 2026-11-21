//! Image vectorization providers, as data.
//!
//! Turning a bitmap into an editable vector is a different service from the model provider: two
//! are known here (Recraft and fal.ai), each with a fixed endpoint and its own key. Nothing is
//! kept but the choice and the credential reference; the key lives in the credential store.

use serde::{Deserialize, Serialize};

/// A vectorization service.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VectorizeProvider {
    /// Recraft, the default.
    #[default]
    /// Recraft's vectorize API.
    Recraft,
    /// fal.ai's hosted Recraft vectorizer.
    Fal,
}

impl VectorizeProvider {
    /// The provider's id.
    pub fn id(self) -> &'static str {
        match self {
            Self::Recraft => "recraft",
            Self::Fal => "fal",
        }
    }

    /// The provider with this id.
    pub fn from_id(id: &str) -> Self {
        if id.eq_ignore_ascii_case("fal") { Self::Fal } else { Self::Recraft }
    }

    /// The name shown in the picker.
    pub fn name(self) -> &'static str {
        match self {
            Self::Recraft => "Recraft",
            Self::Fal => "fal.ai",
        }
    }

    /// Where a person gets a key.
    pub fn key_url(self) -> &'static str {
        match self {
            Self::Recraft => "https://www.recraft.ai/profile/api",
            Self::Fal => "https://fal.ai/dashboard/keys",
        }
    }

    /// The fixed endpoint the service uses.
    pub fn endpoint(self) -> &'static str {
        match self {
            Self::Recraft => "https://external.api.recraft.ai/v1/images/vectorize",
            Self::Fal => "https://fal.run/fal-ai/recraft/vectorize",
        }
    }

    /// The credential reference for this provider's key.
    pub fn credential_ref(self) -> crate::CredentialRef {
        crate::CredentialRef::new(&format!("vectorize-{}", self.id()), "api-key", "default")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        assert_eq!(VectorizeProvider::from_id("recraft"), VectorizeProvider::Recraft);
        assert_eq!(VectorizeProvider::from_id("FAL"), VectorizeProvider::Fal);
        assert_eq!(VectorizeProvider::from_id("nope"), VectorizeProvider::Recraft);
    }

    #[test]
    fn each_provider_has_an_endpoint_and_a_key_url() {
        for provider in [VectorizeProvider::Recraft, VectorizeProvider::Fal] {
            assert!(provider.endpoint().starts_with("https://"));
            assert!(provider.key_url().starts_with("https://"));
            assert!(provider.credential_ref().storage_key().contains(provider.id()));
        }
    }
}
