//! The cloud client: what talks to a figcraft-server's accounts API.
//!
//! A person signs in once; the token the server hands back is kept in the credential store,
//! and every later call carries it. The documents are the account's own: kept on the server,
//! listed to the account, fetched back whole.

use figcraft_ai::cloud::{CloudDocument, CloudSession};
use figcraft_ai::transport::TransportError;
use serde_json::{Value, json};

/// One server's cloud, as the client reaches it.
pub struct Cloud {
    /// The accounts API's address (`https://host:47901` or `http://127.0.0.1:47901`).
    pub url: String,
    /// The session token, once signed in.
    pub token: Option<String>,
}

/// One document the account keeps.
#[derive(Clone, Debug, PartialEq)]
pub struct CloudDocumentWire {
    /// The id it is kept under.
    pub id: String,
    /// The name it was saved with.
    pub name: String,
}

impl From<CloudDocumentWire> for CloudDocument {
    fn from(d: CloudDocumentWire) -> Self {
        Self { id: d.id, name: d.name }
    }
}

impl CloudSession for Cloud {
    fn register(&mut self, email: &str, password: &str, name: &str) -> Result<String, TransportError> {
        Cloud::register(self, email, password, name)
    }
    fn login(&mut self, email: &str, password: &str) -> Result<String, TransportError> {
        Cloud::login(self, email, password)
    }
    fn session(&mut self) -> Result<(String, String), TransportError> {
        Cloud::session(self)
    }
    fn logout(&mut self) -> Result<(), TransportError> {
        Cloud::logout(self)
    }
    fn list(&mut self) -> Result<Vec<CloudDocument>, TransportError> {
        Ok(Cloud::list(self)?.into_iter().map(CloudDocument::from).collect())
    }
    fn put(&mut self, id: &str, bytes: &[u8]) -> Result<(), TransportError> {
        Cloud::put(self, id, bytes)
    }
    fn get(&mut self, id: &str) -> Result<Vec<u8>, TransportError> {
        Cloud::get(self, id)
    }
    fn delete(&mut self, id: &str) -> Result<(), TransportError> {
        Cloud::delete(self, id)
    }
}

/// The desktop build's cloud factory.
#[derive(Default)]
pub struct CloudClientFactory;

impl figcraft_ai::CloudFactory for CloudClientFactory {
    fn make(&self, url: String, token: Option<String>) -> Box<dyn figcraft_ai::CloudSession> {
        Box::new(Cloud { url, token })
    }
}

impl Cloud {
    /// A client for one server.
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into().trim_end_matches('/').to_string(), token: None }
    }

    /// The answer's JSON, or the server's own error message.
    fn call(&self, method: &str, path: &str, body: Option<Value>, bytes: Option<&[u8]>) -> Result<Value, TransportError> {
        let (status, body) = self.call_raw(method, path, body, bytes)?;
        if status != 200 {
            return Err(Self::error_of(status, &body));
        }
        serde_json::from_slice(&body).map_err(|e| TransportError::BadResponse(format!("the answer was not JSON: {e}")))
    }

    /// One call: the status, the body.
    fn call_raw(&self, method: &str, path: &str, body: Option<Value>, bytes: Option<&[u8]>) -> Result<(u16, Vec<u8>), TransportError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let url = format!("{}{path}", self.url);
            // A status the server refuses with is an answer, not a failure of the transport:
            // the error's own words are what the settings show.
            let agent = ureq::Agent::new_with_config(ureq::config::Config::builder().http_status_as_error(false).build());
            let auth = |r: ureq::RequestBuilder<ureq::typestate::WithBody>| -> ureq::RequestBuilder<ureq::typestate::WithBody> {
                match &self.token {
                    Some(token) => r.header("authorization", format!("Bearer {token}")),
                    None => r,
                }
            };
            let authless = |r: ureq::RequestBuilder<ureq::typestate::WithoutBody>| -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
                match &self.token {
                    Some(token) => r.header("authorization", format!("Bearer {token}")),
                    None => r,
                }
            };
            let response = match (method, bytes, body) {
                // The bodyful calls.
                ("POST", _, Some(b)) => auth(agent.post(url))
                    .header("content-type", "application/json")
                    .send_json(&b)
                    .map_err(|e| crate::transport_error(&e.to_string()))?,
                ("PUT", Some(bytes), _) => auth(agent.put(url))
                    .header("content-type", "application/octet-stream")
                    .send(bytes)
                    .map_err(|e| crate::transport_error(&e.to_string()))?,
                ("DELETE", _, _) => authless(agent.delete(url)).call().map_err(|e| crate::transport_error(&e.to_string()))?,
                // The bodiless ones.
                ("GET", _, _) => authless(agent.get(url)).call().map_err(|e| crate::transport_error(&e.to_string()))?,
                _ => return Err(TransportError::Other("there is no such call here".into())),
            };
            let status = response.status().as_u16();
            // The body is bytes: a document's answer is not text, so it is never read as one.
            Ok((status, response.into_body().read_to_vec().map_err(|_| TransportError::Network)?))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (method, path, body, bytes);
            Err(TransportError::Other("the browser build has no cloud client".into()))
        }
    }

    /// The error a status carries, when it carries one.
    fn error_of(status: u16, body: &[u8]) -> TransportError {
        let message = serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| format!("the server answered {status}"));
        TransportError::Other(message)
    }

    /// Make an account. The server answers with the account's id.
    pub fn register(&self, email: &str, password: &str, name: &str) -> Result<String, TransportError> {
        let (status, body) = self.call_raw("POST", "/account/register", Some(json!({"email": email, "password": password, "name": name})), None)?;
        if status != 200 {
            return Err(Self::error_of(status, &body));
        }
        let answer: Value = serde_json::from_slice(&body).map_err(|e| TransportError::BadResponse(e.to_string()))?;
        Ok(answer.get("userId").and_then(Value::as_str).unwrap_or_default().to_string())
    }

    /// Sign in, keeping the token for the calls that follow.
    pub fn login(&mut self, email: &str, password: &str) -> Result<String, TransportError> {
        let (status, body) = self.call_raw("POST", "/account/login", Some(json!({"email": email, "password": password})), None)?;
        if status != 200 {
            return Err(Self::error_of(status, &body));
        }
        let answer: Value = serde_json::from_slice(&body).map_err(|e| TransportError::BadResponse(e.to_string()))?;
        let token = answer.get("token").and_then(Value::as_str).unwrap_or_default().to_string();
        if token.is_empty() {
            return Err(TransportError::BadResponse("the answer held no token".into()));
        }
        self.token = Some(token.clone());
        Ok(token)
    }

    /// Who is signed in.
    pub fn session(&self) -> Result<(String, String), TransportError> {
        let answer = self.call("GET", "/account/session", None, None)?;
        let email = answer.get("email").and_then(Value::as_str).unwrap_or_default().to_string();
        let name = answer.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        Ok((email, name))
    }

    /// End the session.
    pub fn logout(&mut self) -> Result<(), TransportError> {
        let _ = self.call("POST", "/account/logout", Some(json!({})), None);
        self.token = None;
        Ok(())
    }

    /// The documents the account keeps.
    pub fn list(&self) -> Result<Vec<CloudDocumentWire>, TransportError> {
        let answer = self.call("GET", "/cloud/list", None, None)?;
        let mut out = Vec::new();
        for document in answer.get("documents").and_then(Value::as_array).cloned().unwrap_or_default() {
            let id = document.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
            let name = document.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            if !id.is_empty() {
                out.push(CloudDocumentWire { id, name });
            }
        }
        Ok(out)
    }

    /// Keep a document (the whole file's bytes).
    pub fn put(&self, id: &str, bytes: &[u8]) -> Result<(), TransportError> {
        let (status, body) = self.call_raw("PUT", &format!("/cloud/put/{id}"), None, Some(bytes))?;
        if status != 200 {
            return Err(Self::error_of(status, &body));
        }
        Ok(())
    }

    /// A document's bytes.
    pub fn get(&self, id: &str) -> Result<Vec<u8>, TransportError> {
        let (status, body) = self.call_raw("GET", &format!("/cloud/get/{id}"), None, None)?;
        if status != 200 {
            return Err(Self::error_of(status, &body));
        }
        Ok(body)
    }

    /// Forget a document.
    pub fn delete(&self, id: &str) -> Result<(), TransportError> {
        let (status, body) = self.call_raw("DELETE", &format!("/cloud/delete/{id}"), None, None)?;
        if status != 200 {
            return Err(Self::error_of(status, &body));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};

    /// One canned answer, with its length counted (a stub that guesses it truncates itself).
    fn reply(status: &str, body: &[u8]) -> String {
        format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())
            + std::str::from_utf8(body).unwrap_or_default()
    }

    #[test]
    fn a_client_names_its_server_without_a_trailing_slash() {
        let c = Cloud::new("http://127.0.0.1:47901/");
        assert_eq!(c.url, "http://127.0.0.1:47901");
    }

    #[test]
    fn a_client_talks_to_a_server_over_the_wire() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let addr = listener.local_addr().expect("the address");
        let registered = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut s = stream;
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = match s.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let head = String::from_utf8_lossy(&buf).into_owned();
                // The body's length comes from the headers.
                let length = head
                    .split("\r\n")
                    .find_map(|l| l.to_lowercase().strip_prefix("content-length: ").and_then(|v| v.trim().parse::<usize>().ok()))
                    .unwrap_or(0);
                let mut body = Vec::new();
                while body.len() < length {
                    match s.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => body.extend_from_slice(&chunk[..n]),
                    }
                }
                // The first register makes the account; every later one is a duplicate.
                let first = !registered.swap(true, std::sync::atomic::Ordering::Relaxed);
                let answer = if head.starts_with("POST /account/register") && first {
                    reply("200 OK", br#"{"userId":"user-1"}"#)
                } else {
                    reply("409 Conflict", br#"{"error":"an account with this email already exists"}"#)
                };
                let _ = s.write_all(answer.as_bytes());
                let _ = s.shutdown(std::net::Shutdown::Write);
            }
        });

        let client = Cloud::new(format!("http://{addr}"));
        let user = client.register("a@b.com", "12345678", "A").expect("the account is made");
        assert_eq!(user, "user-1");
        let err = client.register("a@b.com", "12345678", "A").expect_err("the email is taken");
        assert!(err.message().contains("already exists"), "{}", err.message());
    }

    #[test]
    fn a_server_s_error_message_becomes_the_answer() {
        let err = Cloud::error_of(409, br#"{"error":"an account with this email already exists"}"#);
        assert!(err.message().contains("already exists"), "{}", err.message());
        // A status without a JSON body still says what happened.
        let err = Cloud::error_of(500, b"");
        assert!(err.message().contains("500"), "{}", err.message());
    }
}
