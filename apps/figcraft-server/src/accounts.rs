//! Accounts: email and password sign-in, sessions, and per-account cloud storage.
//!
//! Off unless `accounts_listen` is set (its own port, plain HTTP/1.1 like `/healthz`; a reverse
//! proxy terminates TLS in front of it). A person registers with an email and a password, signs
//! in for a session token, and the token lets the app keep that person's documents on the
//! server: one folder per account, named by the account, not shared, not listed to anyone else.
//!
//! The password is never stored: its Argon2 hash is, and the token is what travels. Sessions
//! expire; the files stay until the account is deleted.

use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use figcraft_collab_net::token;
use password_hash::SaltString;
use rand_core::OsRng;
use serde_json::{Value, json};

/// How long a session lives.
pub const SESSION_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The most a request body may be: a document (the app's files are capped at 64 MiB too).
pub const MAX_BODY: usize = 64 << 20;

/// The most accounts one server keeps.
pub const MAX_ACCOUNTS: usize = 10_000;

/// The most documents one account keeps.
pub const MAX_DOCUMENTS: usize = 500;

/// How long a wrong password marks the address as guessing (at most 5 tries in a minute).
const LOCKOUT: Duration = Duration::from_secs(60);
const MAX_TRIES: usize = 5;

/// One account.
#[derive(Clone, Debug)]
struct Account {
    /// The account's id, which names its folder.
    id: String,
    email: String,
    name: String,
    /// The password's Argon2 hash (PHC format).
    hash: String,
    created: u64,
}

/// The accounts and their sessions.
pub struct Accounts {
    dir: PathBuf,
    accounts: HashMap<String, Account>,
    /// The session token (its SHA-256, hex) to who holds it and until when.
    sessions: HashMap<String, (String, Instant)>,
    /// The addresses that are guessing passwords, until when.
    locked: HashMap<std::net::IpAddr, Instant>,
    /// The wrong passwords each address has tried since its last success.
    failures: HashMap<std::net::IpAddr, usize>,
}

impl Accounts {
    /// The accounts the data directory holds, or an empty one.
    pub fn open(dir: &std::path::Path) -> Self {
        let dir = dir.join("accounts");
        let mut accounts = HashMap::new();
        if let Ok(text) = std::fs::read_to_string(dir.join("accounts.json"))
            && let Ok(Value::Array(list)) = serde_json::from_str::<Value>(&text)
        {
            for entry in list {
                let id = entry.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                let email = entry.get("email").and_then(Value::as_str).unwrap_or_default().to_string();
                let name = entry.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                let hash = entry.get("hash").and_then(Value::as_str).unwrap_or_default().to_string();
                let created = entry.get("created").and_then(Value::as_u64).unwrap_or(0);
                if id.is_empty() || email.is_empty() || hash.is_empty() {
                    continue;
                }
                accounts.insert(email.to_lowercase(), Account { id, email, name, hash, created });
            }
        }
        Self { dir, accounts, sessions: HashMap::new(), locked: HashMap::new(), failures: HashMap::new() }
    }

    /// How many accounts are kept.
    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    /// Whether the server keeps no accounts.
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    /// Write the accounts out (the passwords are hashes; the file is owner-only).
    fn persist(&self) {
        let list: Vec<Value> =
            self.accounts.values().map(|a| json!({"id": a.id, "email": a.email, "name": a.name, "hash": a.hash, "created": a.created})).collect();
        let tmp = self.dir.join("accounts.json.tmp");
        let text = serde_json::to_string_pretty(&json!(list)).unwrap_or_default();
        if std::fs::create_dir_all(&self.dir)
            .and_then(|()| write_private(&tmp, text.as_bytes()))
            .and_then(|()| std::fs::rename(&tmp, self.dir.join("accounts.json")))
            .is_err()
        {
            log::warn!("accounts: couldn't save them");
        }
    }

    /// A new account, or why not.
    fn register(&mut self, email: &str, password: &str, name: &str) -> Result<String, String> {
        let email = email.trim().to_lowercase();
        let name = name.trim();
        if !email.contains('@') || email.len() > 254 || email.split('@').nth(1).is_none_or(|d| d.is_empty() || !d.contains('.')) {
            return Err("that is not an email address".into());
        }
        if password.len() < 8 {
            return Err("the password needs at least 8 characters".into());
        }
        if password.len() > 1024 {
            return Err("the password is too long".into());
        }
        if self.accounts.len() >= MAX_ACCOUNTS {
            return Err("this server keeps no more accounts".into());
        }
        if self.accounts.contains_key(&email) {
            return Err("an account with this email already exists".into());
        }
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default().hash_password(password.as_bytes(), &salt).map_err(|e| e.to_string())?.to_string();
        let id = token::file_id()?;
        std::fs::create_dir_all(self.dir.join(&id)).map_err(|e| format!("the account's folder could not be made: {e}"))?;
        self.accounts.insert(email.clone(), Account { id: id.clone(), email, name: name.to_string(), hash, created: now() });
        self.persist();
        Ok(id)
    }

    /// A session token for this email and password, or why not.
    fn login(&mut self, email: &str, password: &str, from: std::net::IpAddr) -> Result<(String, String), String> {
        if self.locked.get(&from).is_some_and(|until| *until > Instant::now()) {
            return Err("too many tries; wait a minute".into());
        }
        let Some(account) = self.accounts.get(&email.trim().to_lowercase()).cloned() else {
            self.note_failure(from);
            return Err("no account with this email, or the password is wrong".into());
        };
        let ok = PasswordHash::new(&account.hash).and_then(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed)).is_ok();
        if !ok {
            self.note_failure(from);
            return Err("no account with this email, or the password is wrong".into());
        }
        self.locked.remove(&from);
        self.failures.remove(&from);
        let token = token::secret()?;
        let hashed = crate::store::hash(&token);
        self.sessions.insert(hashed, (account.id.clone(), Instant::now() + SESSION_LIFETIME));
        Ok((token, account.id))
    }

    /// A wrong password counts; at the fifth within the window the address waits a minute.
    fn note_failure(&mut self, from: std::net::IpAddr) {
        if self.locked.get(&from).is_some_and(|until| *until > Instant::now()) {
            return;
        }
        self.failures.entry(from).and_modify(|n| *n += 1).or_insert(1);
        if self.failures.get(&from).copied().unwrap_or(0) >= MAX_TRIES {
            self.failures.remove(&from);
            self.locked.insert(from, Instant::now() + LOCKOUT);
        }
    }

    /// The account a token names, if the session is alive.
    fn account_of(&self, token: &str) -> Option<&Account> {
        let hashed = crate::store::hash(token);
        let (user, until) = self.sessions.get(&hashed)?;
        if *until < Instant::now() {
            return None;
        }
        self.accounts.values().find(|a| a.id == *user)
    }

    /// End a session.
    fn logout(&mut self, token: &str) {
        self.sessions.remove(&crate::store::hash(token));
    }

    /// The documents an account keeps.
    fn list(&self, account: &str) -> Vec<Value> {
        let mut out = Vec::new();
        let dir = self.dir.join(account);
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|x| x != "figcraft") {
                    continue;
                }
                let id = entry.file_name().to_string_lossy().trim_end_matches(".figcraft").to_string();
                let meta = std::fs::read_to_string(path.with_extension("json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
                let name = meta.as_ref().and_then(|m| m.get("name")).and_then(Value::as_str).unwrap_or("Untitled").to_string();
                let updated = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0).to_string())
                    .unwrap_or_default();
                out.push(json!({"id": id, "name": name, "updatedAt": updated}));
            }
        }
        out.sort_by(|a, b| a["id"].as_str().unwrap_or_default().cmp(b["id"].as_str().unwrap_or_default()));
        out
    }

    /// Keep a document for an account.
    fn put(&self, account: &str, id: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        if !document_id_ok(id) {
            return Err("the document's id is not one this server keeps".into());
        }
        let dir = self.dir.join(account);
        let count = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
        if count >= MAX_DOCUMENTS && !dir.join(format!("{id}.figcraft")).exists() {
            return Err("this account keeps no more documents".into());
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let tmp = dir.join(format!("{id}.tmp"));
        write_private(&tmp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, dir.join(format!("{id}.figcraft"))).map_err(|e| e.to_string())?;
        write_private(&dir.join(format!("{id}.json")), json!({"name": name}).to_string().as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// A document's bytes.
    fn get(&self, account: &str, id: &str) -> Result<Vec<u8>, String> {
        if !document_id_ok(id) {
            return Err("the document's id is not one this server keeps".into());
        }
        std::fs::read(self.dir.join(account).join(format!("{id}.figcraft"))).map_err(|_| "there is no document with this id".to_string())
    }

    /// Forget a document.
    fn delete(&self, account: &str, id: &str) -> Result<(), String> {
        if !document_id_ok(id) {
            return Err("the document's id is not one this server keeps".into());
        }
        let removed = std::fs::remove_file(self.dir.join(account).join(format!("{id}.figcraft"))).is_ok();
        let _ = std::fs::remove_file(self.dir.join(account).join(format!("{id}.json")));
        if removed { Ok(()) } else { Err("there is no document with this id".into()) }
    }
}

/// Whether a document id is one a request may name: short, and safe in a file name.
fn document_id_ok(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') && !id.starts_with('.')
}

/// `write_private` as the store does: a file only its owner reads, from before the first byte.
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let mut f = o.open(path)?;
    #[cfg(unix)]
    f.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    f.write_all(bytes)
}

/// The seconds since the epoch (the store's own clock is the same shape).
fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Serve the accounts API on its own listener.
pub fn serve(listener: TcpListener, dir: PathBuf) -> Result<(), String> {
    let mut accounts = Accounts::open(&dir);
    log::info!("accounts: {} on {}", accounts.len(), listener.local_addr().map(|a| a.to_string()).unwrap_or_default());
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let peer = stream.peer_addr().ok().map(|a| a.ip());
        handle(stream, peer, &mut accounts);
    }
    Ok(())
}

/// One request, answered and closed.
fn handle(mut stream: TcpStream, peer: Option<std::net::IpAddr>, accounts: &mut Accounts) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let request = match read_request(&mut stream) {
        Some(r) => r,
        None => return,
    };
    let (method, path, token, body) = request;
    let answer = route(accounts, peer.unwrap_or(ANY), &method, &path, token.as_deref(), &body);
    let (status, body) = answer;
    // A document's answer is its bytes, not JSON; the status carries the length for the
    // content type, and `handle` sends them raw.
    if let Some(length) = status.strip_prefix("200 OK BYTES ") {
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {length}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.write_all(head.as_bytes()).and_then(|_| stream.write_all(&body)).and_then(|_| stream.flush());
    } else {
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes()).and_then(|_| stream.write_all(&body)).and_then(|_| stream.flush());
    }
    let _ = stream.shutdown(std::net::Shutdown::Write);
}

/// One HTTP request: the method, the path, the bearer token and the body.
type Request = (String, String, Option<String>, Vec<u8>);

/// Read one request off the stream, bounded.
fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => buf.extend_from_slice(chunk.get(..n).unwrap_or_default()),
            Err(_) => return None,
        }
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 64 << 10 {
            return None;
        }
    }
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(buf.get(..split).unwrap_or_default()).into_owned();
    let mut body = buf.get(split + 4..).unwrap_or_default().to_vec();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().split('?').next().unwrap_or_default().to_string();
    let mut token = None;
    let mut length = body.len();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let (name, value) = (name.trim().to_lowercase(), value.trim());
        if name == "authorization"
            && let Some(t) = value.strip_prefix("Bearer ")
        {
            token = Some(t.trim().to_string());
        }
        if name == "content-length"
            && let Ok(n) = value.parse::<usize>()
        {
            length = n;
        }
    }
    if length > MAX_BODY {
        return None;
    }
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => body.extend_from_slice(chunk.get(..n).unwrap_or_default()),
            Err(_) => return None,
        }
    }
    body.truncate(length);
    Some((method, path, token, body))
}

/// The peer an unparseable one stands for: it matches no rate-limit slot.
const ANY: std::net::IpAddr = std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED);

/// The route table. Every answer is JSON.
fn route(accounts: &mut Accounts, peer: std::net::IpAddr, method: &str, path: &str, token: Option<&str>, body: &[u8]) -> (String, Vec<u8>) {
    let json = |v: Value| ("200 OK".to_string(), serde_json::to_vec(&v).unwrap_or_default());
    let error = |code: &str, message: String| {
        let status = match code {
            "400" => "400 Bad Request",
            "401" => "401 Unauthorized",
            "403" => "403 Forbidden",
            "404" => "404 Not Found",
            "409" => "409 Conflict",
            _ => "500 Internal Server Error",
        };
        (status.to_string(), serde_json::to_vec(&json!({"error": message})).unwrap_or_default())
    };
    let parsed =
        |body: &[u8]| -> Result<Value, (String, Vec<u8>)> { serde_json::from_slice(body).map_err(|_| error("400", "the body is not JSON".into())) };

    match (method, path) {
        ("POST", "/account/register") => match parsed(body) {
            Ok(v) => {
                let email = v.get("email").and_then(Value::as_str).unwrap_or_default().to_string();
                let password = v.get("password").and_then(Value::as_str).unwrap_or_default().to_string();
                let name = v.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                match accounts.register(&email, &password, &name) {
                    Ok(id) => json(json!({"userId": id})),
                    Err(e) => error("409", e),
                }
            }
            Err(e) => e,
        },
        ("POST", "/account/login") => match parsed(body) {
            Ok(v) => {
                let email = v.get("email").and_then(Value::as_str).unwrap_or_default().to_string();
                let password = v.get("password").and_then(Value::as_str).unwrap_or_default().to_string();
                match accounts.login(&email, &password, peer) {
                    Ok((token, id)) => json(json!({"token": token, "userId": id})),
                    Err(e) => error("401", e),
                }
            }
            Err(e) => e,
        },
        ("GET", "/account/session") => match token.and_then(|t| accounts.account_of(t)) {
            Some(a) => json(json!({"email": a.email, "name": a.name, "userId": a.id})),
            None => error("401", "the session has ended; sign in again".into()),
        },
        ("POST", "/account/logout") => {
            if let Some(t) = token {
                accounts.logout(t);
            }
            json(json!({"ok": true}))
        }
        ("GET", "/cloud/list") => match token.and_then(|t| accounts.account_of(t)) {
            Some(a) => json(json!({"documents": accounts.list(&a.id)})),
            None => error("401", "the session has ended; sign in again".into()),
        },
        (m, p) if m == "GET" && p.starts_with("/cloud/get/") => {
            let Some(account) = token.and_then(|t| accounts.account_of(t)) else {
                return error("401", "the session has ended; sign in again".into());
            };
            match accounts.get(&account.id, p.trim_start_matches("/cloud/get/")) {
                Ok(bytes) => bytes_reply(&bytes),
                Err(e) => error("404", e),
            }
        }
        (m, p) if m == "PUT" && p.starts_with("/cloud/put/") => {
            let Some(account) = token.and_then(|t| accounts.account_of(t)) else {
                return error("401", "the session has ended; sign in again".into());
            };
            let id = p.trim_start_matches("/cloud/put/").to_string();
            match accounts.put(&account.id, &id, &account.email, body) {
                Ok(()) => json(json!({"ok": true})),
                Err(e) => error("400", e),
            }
        }
        (m, p) if m == "DELETE" && p.starts_with("/cloud/delete/") => {
            let Some(account) = token.and_then(|t| accounts.account_of(t)) else {
                return error("401", "the session has ended; sign in again".into());
            };
            match accounts.delete(&account.id, p.trim_start_matches("/cloud/delete/")) {
                Ok(()) => json(json!({"ok": true})),
                Err(e) => error("404", e),
            }
        }
        _ => error("404", format!("there is nothing at {path}")),
    }
}

/// A document answer: its bytes as the body, with a marker the caller keeps (a document is not
/// JSON, so `handle` sends its bytes raw when the status says so).
fn bytes_reply(bytes: &[u8]) -> (String, Vec<u8>) {
    (format!("200 OK BYTES {}", bytes.len()), bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accounts() -> Accounts {
        let dir = std::env::temp_dir().join(format!("figcraft-accounts-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Accounts::open(&dir)
    }

    #[test]
    fn an_account_is_registered_signed_in_and_read_back() {
        let mut a = accounts();
        let id = a.register("Ada@example.com", "correct horse", "Ada").expect("the account is registered");
        assert!(!id.is_empty());
        // The email is matched without its case.
        let (token, user) = a
            .login("ada@example.com", "correct horse", "127.0.0.1".parse::<std::net::IpAddr>().expect("a loopback address"))
            .expect("the account signs in");
        assert_eq!(user, id);
        let account = a.account_of(&token).expect("the session is alive");
        assert_eq!(account.email, "ada@example.com");
        assert_eq!(account.name, "Ada");
    }

    #[test]
    fn a_wrong_password_is_refused_and_the_same_error_says_nothing_about_which() {
        let mut a = accounts();
        a.register("ada@example.com", "correct horse", "Ada").expect("registered");
        let err = a
            .login("ada@example.com", "wrong password", "127.0.0.1".parse::<std::net::IpAddr>().expect("a loopback address"))
            .expect_err("the wrong password");
        assert!(err.contains("password"), "{err}");
        // A missing account gives the same answer, so a guesser learns nothing.
        let err = a
            .login("nobody@example.com", "correct horse", "127.0.0.1".parse::<std::net::IpAddr>().expect("a loopback address"))
            .expect_err("no such account");
        assert_eq!(err, "no account with this email, or the password is wrong");
    }

    #[test]
    fn a_short_password_is_refused_at_registration() {
        let mut a = accounts();
        assert!(a.register("ada@example.com", "short", "Ada").is_err());
    }

    #[test]
    fn an_unreadable_email_is_refused() {
        let mut a = accounts();
        assert!(a.register("not-an-email", "correct horse", "").is_err());
        assert!(a.register("a@b", "correct horse", "").is_err());
        assert!(a.register("", "correct horse", "").is_err());
    }

    #[test]
    fn the_same_email_cannot_register_twice() {
        let mut a = accounts();
        a.register("ada@example.com", "correct horse", "Ada").expect("registered");
        assert!(a.register("ada@example.com", "another password", "Ada").is_err());
    }

    #[test]
    fn documents_are_kept_listed_and_forgotten() {
        let mut a = accounts();
        let id = a.register("ada@example.com", "correct horse", "Ada").expect("registered");
        a.put(&id, "poster-1", "Poster", b"document bytes").expect("the document is kept");
        let listed = a.list(&id);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["name"], "Poster");
        assert_eq!(a.get(&id, "poster-1").expect("the document is read"), b"document bytes");
        a.delete(&id, "poster-1").expect("the document is forgotten");
        assert!(a.list(&id).is_empty());
    }

    #[test]
    fn a_document_id_is_checked_before_its_file_is_touched() {
        let mut a = accounts();
        let id = a.register("ada@example.com", "correct horse", "Ada").expect("registered");
        assert!(a.put(&id, "../escape", "x", b"").is_err());
        assert!(a.put(&id, ".hidden", "x", b"").is_err());
        assert!(a.put(&id, "", "x", b"").is_err());
        assert!(a.get(&id, "../../store").is_err());
    }

    #[test]
    fn a_session_ends_when_its_token_is_logged_out() {
        let mut a = accounts();
        a.register("ada@example.com", "correct horse", "Ada").expect("registered");
        let (token, _) =
            a.login("ada@example.com", "correct horse", "127.0.0.1".parse::<std::net::IpAddr>().expect("a loopback address")).expect("signed in");
        assert!(a.account_of(&token).is_some());
        a.logout(&token);
        assert!(a.account_of(&token).is_none());
    }

    #[test]
    fn a_made_up_token_names_nothing() {
        let mut a = accounts();
        a.register("ada@example.com", "correct horse", "Ada").expect("registered");
        assert!(a.account_of("made-up-token").is_none());
    }

    #[test]
    fn the_accounts_survive_a_restart_and_the_password_stays_a_hash() {
        let dir = std::env::temp_dir().join(format!("figcraft-accounts-restart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut a = Accounts::open(&dir);
        a.register("ada@example.com", "correct horse", "Ada").expect("registered");
        assert!(!std::fs::read_to_string(dir.join("accounts").join("accounts.json")).expect("the accounts file").contains("correct horse"));
        let mut b = Accounts::open(&dir);
        assert!(b.login("ada@example.com", "correct horse", "127.0.0.1".parse::<std::net::IpAddr>().expect("a loopback address")).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
