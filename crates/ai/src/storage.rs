//! Cloud storage settings, as data.
//!
//! Two providers are known: this app's own server (no fields — the Account tab's server is used)
//! and any S3-compatible service (endpoint, bucket, region and two keys). The settings hold the
//! fields and the *references* to the keys; the keys stay in the credential store.

use serde::{Deserialize, Serialize};

/// The provider a cloud workspace uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StorageProvider {
    /// The app's own server, from the Account tab.
    #[default]
    FigcraftServer,
    /// Any S3-compatible service.
    S3Compatible,
}

impl StorageProvider {
    /// The provider's id.
    pub fn id(self) -> &'static str {
        match self {
            Self::FigcraftServer => "figcraft-server",
            Self::S3Compatible => "s3-compatible",
        }
    }

    /// The provider with this id.
    pub fn from_id(id: &str) -> Self {
        if id.eq_ignore_ascii_case("s3-compatible") { Self::S3Compatible } else { Self::FigcraftServer }
    }

    /// The name shown in the picker.
    pub fn label(self) -> &'static str {
        match self {
            Self::FigcraftServer => "FigCraft server",
            Self::S3Compatible => "S3 storage",
        }
    }

    /// The description shown under the name.
    pub fn description(self) -> &'static str {
        match self {
            Self::FigcraftServer => "Documents on the FigCraft server you sign in to.",
            Self::S3Compatible => "AWS S3, Backblaze B2, Cloudflare R2, MinIO and compatible storage.",
        }
    }
}

/// The S3 preferences a person enters.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct S3Settings {
    /// The service endpoint (`https://s3.amazonaws.com` or a compatible one).
    #[serde(default)]
    pub endpoint: String,
    /// The bucket that holds the documents.
    #[serde(default)]
    pub bucket: String,
    /// The signing region (empty: the service default).
    #[serde(default)]
    pub region: String,
}

impl S3Settings {
    /// Whether the required fields are filled in.
    pub fn complete(&self) -> bool {
        !self.endpoint.trim().is_empty() && !self.bucket.trim().is_empty()
    }
}

/// One cloud workspace's calls, made on a worker thread.
pub trait StorageSession: Send {
    /// Test the connection. An ok result carries the service's message.
    fn test_connection(&self) -> Result<String, crate::TransportError>;
    /// List the stored documents.
    fn list(&self) -> Result<Vec<StorageDocument>, crate::TransportError>;
}

/// The factory for a workspace session, built from the settings and their keys.
pub trait StorageFactory: Send + Sync {
    /// A session for S3 with these settings and keys.
    fn make_s3(&self, settings: S3Settings, access_key: String, secret_key: String) -> Box<dyn StorageSession>;
    /// A session for this app's server, when it is signed in.
    fn make_server(&self, url: String, token: String) -> Box<dyn StorageSession>;
}

/// The factory with nothing behind it.
#[derive(Default)]
pub struct NoStorage;

impl StorageFactory for NoStorage {
    fn make_s3(&self, _: S3Settings, _: String, _: String) -> Box<dyn StorageSession> {
        Box::new(NoStorageSession)
    }
    fn make_server(&self, _: String, _: String) -> Box<dyn StorageSession> {
        Box::new(NoStorageSession)
    }
}

struct NoStorageSession;

impl StorageSession for NoStorageSession {
    fn test_connection(&self) -> Result<String, crate::TransportError> {
        Err(crate::TransportError::Other("this build has no storage client".into()))
    }
    fn list(&self) -> Result<Vec<StorageDocument>, crate::TransportError> {
        Err(crate::TransportError::Other("this build has no storage client".into()))
    }
}

/// A document in a cloud workspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageDocument {
    /// The id it is kept under.
    pub id: String,
    /// The name shown in the workspace.
    pub name: String,
    /// When it was last changed, as the service reported it.
    #[serde(default)]
    pub updated_at: String,
}

/// The namespace and key prefixes used for documents in a bucket.
pub const NAMESPACE: &str = "figcraft_storage";
pub const MARKER: &str = "figcraft_storage/.namespace";
pub const CANVASES: &str = "figcraft_storage/canvases/";

/// The CORS configuration a person pastes into their bucket, as JSON.
pub fn cors_json(origins: &[String]) -> String {
    let mut list = vec!["http://localhost:*".to_string(), "http://127.0.0.1:*".to_string()];
    list.extend(origins.iter().cloned());
    list.sort();
    list.dedup();
    let origins = list.iter().map(|o| format!("\"{}\"", o.replace('"', "\\\""))).collect::<Vec<_>>().join(", ");
    format!(
        "[{{\"AllowedHeaders\":[\"*\"],\"AllowedMethods\":[\"GET\",\"PUT\",\"POST\",\"DELETE\",\"HEAD\"],\"AllowedOrigins\":[{origins}],\"ExposeHeaders\":[\"ETag\",\"x-amz-request-id\",\"x-amz-id-2\",\"x-amz-version-id\"],\"MaxAgeSeconds\":3600}}]"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_round_trip() {
        assert_eq!(StorageProvider::from_id("s3-compatible"), StorageProvider::S3Compatible);
        assert_eq!(StorageProvider::from_id("nope"), StorageProvider::FigcraftServer);
        assert_eq!(StorageProvider::S3Compatible.label(), "S3 storage");
    }

    #[test]
    fn s3_needs_an_endpoint_and_bucket() {
        assert!(!S3Settings::default().complete());
        assert!(!S3Settings { endpoint: "https://s3.example.com".into(), bucket: String::new(), region: String::new() }.complete());
        assert!(S3Settings { endpoint: "https://s3.example.com".into(), bucket: "docs".into(), region: String::new() }.complete());
    }

    #[test]
    fn the_cors_json_is_valid_and_deduplicates_origins() {
        let json = cors_json(&["https://example.com".into(), "https://example.com".into()]);
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let origins = value[0]["AllowedOrigins"].as_array().expect("a list");
        assert!(origins.iter().any(|o| o == "https://example.com"));
        assert_eq!(origins.iter().filter(|o| *o == "https://example.com").count(), 1);
        assert_eq!(value[0]["MaxAgeSeconds"], 3600);
    }
}
