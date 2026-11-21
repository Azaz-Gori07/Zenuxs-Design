//! The MCP connection commands and the trust rules they enforce.
//!
//! What is checked here is what a person can get wrong and what an attacker would try: a URL
//! that carries its own credentials, a plain-http server off this machine, the app's own name,
//! and a connection that keeps working when the settings round-trip.

use serde_json::json;

use crate::Session;

/// The command's answer, or the error it gave.
fn run(s: &mut Session, id: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    s.execute(id, &params).map_err(|e| e.to_string())
}

#[test]
fn a_connection_is_added_listed_and_removed() {
    let mut s = Session::empty();
    let out = run(&mut s, "ai.mcp.add", json!({"name": "notes", "url": "https://example.com/mcp"})).expect("the connection is added");
    let id = out["id"].as_str().expect("an id").to_string();
    assert!(id.starts_with("mcp-"));

    let listed = run(&mut s, "ai.mcp.connections", json!({})).expect("the connections are listed");
    let connection = listed["connections"].as_array().expect("connections").iter().find(|c| c["id"] == id).expect("the connection is listed");
    assert_eq!(connection["name"], "notes");
    assert_eq!(connection["enabled"], true);

    run(&mut s, "ai.mcp.remove", json!({"id": id})).expect("the connection is removed");
    let listed = run(&mut s, "ai.mcp.connections", json!({})).expect("the connections are listed");
    assert!(listed["connections"].as_array().expect("connections").is_empty());
}

#[test]
fn a_url_with_its_own_credentials_is_refused() {
    let mut s = Session::empty();
    let err = run(&mut s, "ai.mcp.add", json!({"name": "notes", "url": "https://user:pass@example.com/mcp"}))
        .expect_err("credentials in the URL are refused");
    assert!(err.contains("credentials"), "{err}");
}

#[test]
fn a_plain_http_server_off_this_machine_is_refused_and_a_local_one_is_not() {
    let mut s = Session::empty();
    let err = run(&mut s, "ai.mcp.add", json!({"name": "web", "url": "http://example.com/mcp"})).expect_err("plain http is refused");
    assert!(err.contains("https"), "{err}");
    run(&mut s, "ai.mcp.add", json!({"name": "local", "url": "http://127.0.0.1:7600/mcp"})).expect("a loopback server is fine");
}

#[test]
fn the_app_s_own_name_is_reserved() {
    let mut s = Session::empty();
    let err = run(&mut s, "ai.mcp.add", json!({"name": "figcraft", "url": "https://example.com/mcp"})).expect_err("the reserved name");
    assert!(err.contains("figcraft"), "{err}");
}

#[test]
fn two_connections_cannot_share_a_name() {
    let mut s = Session::empty();
    run(&mut s, "ai.mcp.add", json!({"name": "notes", "url": "https://example.com/mcp"})).expect("the first is added");
    let err = run(&mut s, "ai.mcp.add", json!({"name": "notes", "url": "https://other.com/mcp"})).expect_err("the name is taken");
    assert!(err.contains("notes"), "{err}");
}

#[test]
fn a_connection_can_be_disabled_without_being_forgotten() {
    let mut s = Session::empty();
    let out = run(&mut s, "ai.mcp.add", json!({"name": "notes", "url": "https://example.com/mcp"})).expect("the connection is added");
    let id = out["id"].as_str().expect("an id").to_string();
    run(&mut s, "ai.mcp.update", json!({"id": id, "enabled": false})).expect("the connection is switched off");
    let listed = run(&mut s, "ai.mcp.connections", json!({})).expect("the connections are listed");
    let connection = listed["connections"].as_array().expect("connections")[0].clone();
    assert_eq!(connection["enabled"], false);
}

#[test]
fn a_bearer_connection_keeps_its_token_in_the_store() {
    let mut s = Session::empty();
    s.credentials = Some(Box::new(figcraft_ai::credentials::MemoryCredentialStore::new()));
    let out = run(&mut s, "ai.mcp.add", json!({"name": "notes", "url": "https://example.com/mcp", "bearer": true})).expect("the connection is added");
    let id = out["id"].as_str().expect("an id").to_string();
    let status = run(&mut s, "ai.mcp.credential.set", json!({"profileId": id, "value": "tok-1"})).expect("the token is stored");
    assert_eq!(status["status"], "configured");
    // The settings hold no token.
    let text = serde_json::to_string(&s.prefs).expect("the preferences serialise");
    assert!(!text.contains("tok-1"), "the preferences never hold a token");
}

#[test]
fn the_connections_survive_a_round_trip_through_the_preferences() {
    let mut s = Session::empty();
    run(&mut s, "ai.mcp.add", json!({"name": "notes", "url": "https://example.com/mcp"})).expect("the connection is added");
    let text = serde_json::to_string(&s.prefs).expect("the preferences serialise");
    let back: crate::Prefs = serde_json::from_str(&text).expect("the preferences load");
    assert_eq!(back.ai.mcp.connections.len(), 1);
    assert_eq!(back.ai.mcp.connections[0].name, "notes");
}

#[test]
fn an_update_to_a_missing_connection_is_refused() {
    let mut s = Session::empty();
    let err = run(&mut s, "ai.mcp.update", json!({"id": "mcp-none", "enabled": false})).expect_err("no such connection");
    assert!(err.contains("mcp-none"), "{err}");
}
