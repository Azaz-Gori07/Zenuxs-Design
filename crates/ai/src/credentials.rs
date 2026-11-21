//! The credential store: where API keys live.
//!
//! A key is never part of the model settings. The settings name the credential to use
//! ([`CredentialRef`]); the store holds the value. That is what lets a person keep their keys in
//! the operating system's keychain on the desktop and in memory in the browser, without the
//! settings file — which is copied around and synced — ever holding a secret.
//!
//! A reference names three things: the integration the key belongs to (`openai`, `pexels`,
//! `mcp-…`), the profile it was entered for (a connection id, or `default` for a provider-wide
//! key) and the field (usually `api-key`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A key's address in the store.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CredentialRef {
    /// The integration the key belongs to.
    pub integration_id: String,
    /// The profile the key was entered for.
    pub profile_id: String,
    /// The field the key fills.
    pub field: String,
}

impl CredentialRef {
    /// A reference to a key.
    pub fn new(integration_id: &str, profile_id: &str, field: &str) -> Self {
        Self { integration_id: integration_id.to_string(), profile_id: profile_id.to_string(), field: field.to_string() }
    }

    /// The reference a provider's key is stored under, for a connection.
    pub fn provider_key(provider: &str, profile_id: &str) -> Self {
        Self::new(provider, profile_id, "api-key")
    }

    /// The reference an MCP connection's bearer token is stored under.
    pub fn mcp_token(profile_id: &str) -> Self {
        Self::new("mcp", profile_id, "bearer-token")
    }

    /// The reference a stock-photo service's key is stored under.
    pub fn media_key(service: &str, profile_id: &str) -> Self {
        Self::new(service, profile_id, "api-key")
    }

    /// The key the store holds this reference under.
    ///
    /// The format is versioned, so a later store can read an older one and know what it is
    /// looking at.
    pub fn storage_key(&self) -> String {
        format!("v1:{}:{}:{}", self.integration_id, self.profile_id, self.field)
    }
}

/// Whether a key is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialStatus {
    /// A key is stored.
    Configured,
    /// No key is stored.
    Missing,
    /// The store cannot be reached (a locked keychain, a browser without storage).
    Unavailable,
}

/// Why a credential operation failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CredentialStoreError {
    /// The reference is not one the store can hold.
    #[error("the credential reference is not one the store can hold")]
    InvalidReference,
    /// The store is not reachable.
    #[error("the credential store is not reachable")]
    Unavailable,
    /// The store refused the write.
    #[error("{0}")]
    Failed(String),
}

/// Where keys live.
///
/// Writing takes `&mut self`: a store is the one thing the settings own outright, and a key
/// that changes is a change to it.
pub trait CredentialStore {
    /// Whether a key is stored for this reference.
    fn status(&self, reference: &CredentialRef) -> Result<CredentialStatus, CredentialStoreError>;

    /// The key for this reference, or `None` when there is none.
    fn read(&self, reference: &CredentialRef) -> Result<Option<String>, CredentialStoreError>;

    /// Store a key, replacing the one there.
    fn write(&mut self, reference: &CredentialRef, value: &str) -> Result<(), CredentialStoreError>;

    /// Remove a key.
    fn remove(&mut self, reference: &CredentialRef) -> Result<(), CredentialStoreError>;
}

/// A store that keeps keys in memory only.
///
/// The browser build starts here: a page's keys live as long as the page, and nothing is written
/// to disk. The desktop build replaces it with one that writes to the keychain.
#[derive(Clone, Default)]
pub struct MemoryCredentialStore {
    keys: BTreeMap<String, String>,
}

impl MemoryCredentialStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of keys held.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the store holds no keys.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

impl CredentialStore for MemoryCredentialStore {
    fn status(&self, reference: &CredentialRef) -> Result<CredentialStatus, CredentialStoreError> {
        Ok(if self.keys.contains_key(&reference.storage_key()) { CredentialStatus::Configured } else { CredentialStatus::Missing })
    }

    fn read(&self, reference: &CredentialRef) -> Result<Option<String>, CredentialStoreError> {
        Ok(self.keys.get(&reference.storage_key()).cloned())
    }

    fn write(&mut self, reference: &CredentialRef, value: &str) -> Result<(), CredentialStoreError> {
        if value.trim().is_empty() {
            return self.remove(reference);
        }
        self.keys.insert(reference.storage_key(), value.to_string());
        Ok(())
    }

    fn remove(&mut self, reference: &CredentialRef) -> Result<(), CredentialStoreError> {
        self.keys.remove(&reference.storage_key());
        Ok(())
    }
}

/// A store that keeps keys in a JSON file, readable only by its owner.
///
/// This is the desktop store's fallback when the operating system has no keychain to ask. The file
/// holds the keys in the clear, so it is written with owner-only permissions and never copied
/// into the settings or into a log.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FileCredentialStore {
    /// The keys, by their storage key.
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
}

impl FileCredentialStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// The store's contents as the JSON the file holds.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// The store's contents from a file's JSON; an unreadable file is an empty store.
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }
}

impl CredentialStore for FileCredentialStore {
    fn status(&self, reference: &CredentialRef) -> Result<CredentialStatus, CredentialStoreError> {
        Ok(if self.keys.contains_key(&reference.storage_key()) { CredentialStatus::Configured } else { CredentialStatus::Missing })
    }

    fn read(&self, reference: &CredentialRef) -> Result<Option<String>, CredentialStoreError> {
        Ok(self.keys.get(&reference.storage_key()).cloned())
    }

    fn write(&mut self, reference: &CredentialRef, value: &str) -> Result<(), CredentialStoreError> {
        if value.trim().is_empty() {
            return self.remove(reference);
        }
        self.keys.insert(reference.storage_key(), value.to_string());
        Ok(())
    }

    fn remove(&mut self, reference: &CredentialRef) -> Result<(), CredentialStoreError> {
        self.keys.remove(&reference.storage_key());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_names_its_key_by_integration_profile_and_field() {
        let r = CredentialRef::provider_key("openai", "connection-1");
        assert_eq!(r.storage_key(), "v1:openai:connection-1:api-key");
        assert_eq!(CredentialRef::mcp_token("mcp-1").storage_key(), "v1:mcp:mcp-1:bearer-token");
    }

    #[test]
    fn the_memory_store_holds_and_forgets_a_key() {
        let mut store = MemoryCredentialStore::new();
        let r = CredentialRef::provider_key("openai", "c1");
        assert_eq!(store.status(&r).expect("status"), CredentialStatus::Missing);
        store.write(&r, "sk-123").expect("write");
        assert_eq!(store.status(&r).expect("status"), CredentialStatus::Configured);
        assert_eq!(store.read(&r).expect("read").as_deref(), Some("sk-123"));
        store.remove(&r).expect("remove");
        assert_eq!(store.status(&r).expect("status"), CredentialStatus::Missing);
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn writing_a_blank_key_clears_it() {
        let mut store = MemoryCredentialStore::new();
        let r = CredentialRef::provider_key("openai", "c1");
        store.write(&r, "sk-123").expect("write");
        store.write(&r, "   ").expect("write");
        assert_eq!(store.status(&r).expect("status"), CredentialStatus::Missing);
    }

    #[test]
    fn two_profiles_on_one_integration_keep_separate_keys() {
        let mut store = MemoryCredentialStore::new();
        store.write(&CredentialRef::provider_key("openai", "c1"), "a").expect("write");
        store.write(&CredentialRef::provider_key("openai", "c2"), "b").expect("write");
        assert_eq!(store.read(&CredentialRef::provider_key("openai", "c1")).expect("read").as_deref(), Some("a"));
        assert_eq!(store.read(&CredentialRef::provider_key("openai", "c2")).expect("read").as_deref(), Some("b"));
    }

    #[test]
    fn the_file_store_survives_a_round_trip_and_ignores_a_broken_file() {
        let mut store = FileCredentialStore::new();
        store.keys.insert("v1:openai:c1:api-key".into(), "sk-123".into());
        let back = FileCredentialStore::from_json(&store.to_json());
        assert_eq!(back, store);
        assert!(FileCredentialStore::from_json("not json").keys.is_empty());
    }
}
