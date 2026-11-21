//! The accounts API end to end, against the client the app uses: a person registers, signs in,
//! keeps a document, lists it, reads it back whole, deletes it, and signs out. The trust rules
//! hold over the wire too.

use figcraft_ai_net::cloud::Cloud;
use figcraft_server::accounts;
use figcraft_server::config::Config;

#[test]
fn register_sign_in_and_documents_round_trip() {
    let dir = std::env::temp_dir().join(format!("figcraft-cloud-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cfg = Config { data: dir.clone(), accounts_listen: Some("127.0.0.1:0".into()), ..Config::default() };
    cfg.check().expect("the config is fine");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let addr = listener.local_addr().expect("the address");
    let serve_dir = dir.clone();
    std::thread::spawn(move || accounts::serve(listener, serve_dir).expect("the accounts serve"));

    let mut client = Cloud::new(format!("http://{addr}"));

    // Register: the email is matched without its case on the way in.
    // A probe with a broken body shows the server's own answer over the wire.
    let err = client.register("", "12345678", "").expect_err("an unreadable email is refused");
    eprintln!("SERVER PROBE: {}", err.message());
    assert!(err.message().contains("email"), "{}", err.message());
    let user = client.register("Ada@Example.com", "correct horse", "Ada").expect("the account is made");
    assert!(!user.is_empty());

    // A second one is refused, with the server's own words.
    let err = client.register("ada@example.com", "another password", "Ada").expect_err("the email is taken");
    assert!(err.message().contains("already exists"), "{}", err.message());

    // Sign in.
    client.login("ada@example.com", "correct horse").expect("signed in");
    let (email, name) = client.session().expect("the session is alive");
    assert_eq!(email, "ada@example.com");
    assert_eq!(name, "Ada");

    // Keep a document, list it, read it back whole.
    let bytes: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
    client.put("poster-1", &bytes).expect("the document is kept");
    let listed = client.list().expect("the documents are listed");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "poster-1");
    assert_eq!(listed[0].name, "ada@example.com");
    assert_eq!(client.get("poster-1").expect("the document is read"), bytes);

    // A wrong password is refused, and the server's answer is what the client says.
    let mut other = Cloud::new(format!("http://{addr}"));
    let err = other.login("ada@example.com", "wrong password").expect_err("the wrong password");
    assert!(err.message().contains("password"), "{}", err.message());

    // Delete, then sign out: the session is over.
    client.delete("poster-1").expect("the document is forgotten");
    assert!(client.list().expect("the documents are listed").is_empty());
    client.logout().expect("the session is over");
    assert!(client.session().is_err(), "the token is dead");

    let _ = std::fs::remove_dir_all(dir);
}
