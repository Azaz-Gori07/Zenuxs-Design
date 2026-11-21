//! The account: signing in to a figcraft-server's cloud, and the documents it keeps.
//!
//! Like the transport and the MCP sessions, the trait is the seam and the desktop build fills
//! it: a sign-in or an upload runs on a worker thread, and the token the server hands back goes
//! to the credential store, never to the settings.

use crate::transport::TransportError;

/// One document the account keeps.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudDocument {
    /// The id it is kept under.
    pub id: String,
    /// The name it was saved with.
    pub name: String,
}

/// One account's cloud, as the interface uses it.
pub trait CloudSession: Send {
    /// Make an account. Answers with the account's id.
    fn register(&mut self, email: &str, password: &str, name: &str) -> Result<String, TransportError>;
    /// Sign in. Answers with the session token, which the caller keeps.
    fn login(&mut self, email: &str, password: &str) -> Result<String, TransportError>;
    /// Who the session belongs to, when it is alive.
    fn session(&mut self) -> Result<(String, String), TransportError>;
    /// End the session.
    fn logout(&mut self) -> Result<(), TransportError>;
    /// The documents the account keeps.
    fn list(&mut self) -> Result<Vec<CloudDocument>, TransportError>;
    /// Keep a document.
    fn put(&mut self, id: &str, bytes: &[u8]) -> Result<(), TransportError>;
    /// A document's bytes.
    fn get(&mut self, id: &str) -> Result<Vec<u8>, TransportError>;
    /// Forget a document.
    fn delete(&mut self, id: &str) -> Result<(), TransportError>;
}

/// The factory a worker thread makes its session from.
pub trait CloudFactory: Send + Sync {
    /// A session with the server at `url`, carrying `token` when one is kept.
    fn make(&self, url: String, token: Option<String>) -> Box<dyn CloudSession>;
}

/// The factory with nothing behind it.
#[derive(Default)]
pub struct NoCloud;

impl CloudFactory for NoCloud {
    fn make(&self, _url: String, _token: Option<String>) -> Box<dyn CloudSession> {
        Box::new(NoCloudSession)
    }
}

/// The session the default factory makes: there is nothing here.
#[derive(Default)]
struct NoCloudSession;

impl CloudSession for NoCloudSession {
    fn register(&mut self, _: &str, _: &str, _: &str) -> Result<String, TransportError> {
        Err(TransportError::Other("this build has no cloud".into()))
    }
    fn login(&mut self, _: &str, _: &str) -> Result<String, TransportError> {
        Err(TransportError::Other("this build has no cloud".into()))
    }
    fn session(&mut self) -> Result<(String, String), TransportError> {
        Err(TransportError::Other("this build has no cloud".into()))
    }
    fn logout(&mut self) -> Result<(), TransportError> {
        Err(TransportError::Other("this build has no cloud".into()))
    }
    fn list(&mut self) -> Result<Vec<CloudDocument>, TransportError> {
        Err(TransportError::Other("this build has no cloud".into()))
    }
    fn put(&mut self, _: &str, _: &[u8]) -> Result<(), TransportError> {
        Err(TransportError::Other("this build has no cloud".into()))
    }
    fn get(&mut self, _: &str) -> Result<Vec<u8>, TransportError> {
        Err(TransportError::Other("this build has no cloud".into()))
    }
    fn delete(&mut self, _: &str) -> Result<(), TransportError> {
        Err(TransportError::Other("this build has no cloud".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_factory_says_what_it_cannot_do() {
        let mut session = NoCloud.make(String::new(), None);
        assert!(session.login("a@b.com", "pw").is_err());
        assert!(session.list().is_err());
    }
}
