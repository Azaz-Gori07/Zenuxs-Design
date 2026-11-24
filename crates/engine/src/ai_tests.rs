//! The AI settings commands: the models, the keys, the standing instructions and the usage.
//!
//! What is checked here is what the commands promise: a model needs a connection that exists, a
//! role points at a model that exists, removing a model clears the roles it held, a key is
//! never answered, and a skill that does not exist is refused rather than added.

use figcraft_ai::credentials::{CredentialRef, CredentialStatus, MemoryCredentialStore};
use serde_json::json;

use crate::Session;

/// A session with a credential store a key can go into.
fn session() -> Session {
    let mut s = Session::empty();
    s.credentials = Some(Box::new(MemoryCredentialStore::new()));
    s
}

/// The command's answer, or the error it gave.
fn run(s: &mut Session, id: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    s.execute(id, &params).map_err(|e| e.to_string())
}

#[test]
fn the_provider_registry_lists_every_provider_with_its_models() {
    let mut s = session();
    let out = run(&mut s, "ai.providers", json!({})).expect("the registry is listed");
    let providers = out["providers"].as_array().expect("a list of providers");
    assert!(providers.len() >= 9);
    let openai = providers.iter().find(|p| p["id"] == "openai").expect("OpenAI is listed");
    assert_eq!(openai["name"], "OpenAI");
    assert!(!openai["models"].as_array().expect("models").is_empty());
    // The compatible providers take a base URL and list no models of their own.
    let compatible = providers.iter().find(|p| p["id"] == "openai-compatible").expect("the compatible provider is listed");
    assert_eq!(compatible["supportsCustomBaseURL"], true);
    assert!(compatible["models"].as_array().expect("models").is_empty());
}

#[test]
fn a_model_needs_a_connection_that_exists() {
    let mut s = session();
    let err = run(&mut s, "ai.model.add", json!({"name": "Main", "connectionId": "connection-9", "modelID": "gpt-5.6"}))
        .expect_err("a missing connection is refused");
    assert!(err.contains("connection-9"), "{err}");
}

#[test]
fn a_connection_makes_a_model_possible() {
    let mut s = session();
    let connection = run(&mut s, "ai.connection.add", json!({"provider": "openai"})).expect("the connection is added");
    let id = connection["id"].as_str().expect("an id").to_string();
    assert!(id.starts_with("connection-"));

    let model = run(&mut s, "ai.model.add", json!({"name": "Main", "connectionId": id, "modelID": "gpt-5.6", "capabilities": ["tools"]}))
        .expect("the model is added");
    let model_id = model["id"].as_str().expect("an id").to_string();
    assert!(model_id.starts_with("model-"));

    let out = run(&mut s, "ai.models", json!({})).expect("the models are listed");
    assert_eq!(out["connections"].as_array().expect("connections").len(), 1);
    assert_eq!(out["models"].as_array().expect("models").len(), 1);
    assert_eq!(out["models"][0]["capabilities"][0], "tools");
}

#[test]
fn a_compatible_provider_needs_a_base_url() {
    let mut s = session();
    let err = run(&mut s, "ai.connection.add", json!({"provider": "openai-compatible"})).expect_err("the missing URL is refused");
    assert!(err.contains("base URL"), "{err}");
    let out = run(&mut s, "ai.connection.add", json!({"provider": "openai-compatible", "customBaseURL": "http://localhost:11434/v1"}))
        .expect("the connection is added");
    assert!(out["id"].as_str().expect("an id").starts_with("connection-"));
}

#[test]
fn an_unknown_provider_is_refused() {
    let mut s = session();
    let err = run(&mut s, "ai.connection.add", json!({"provider": "nope"})).expect_err("an unknown provider is refused");
    assert!(err.contains("nope"), "{err}");
}

#[test]
fn a_role_points_at_a_model_that_exists() {
    let mut s = session();
    let connection = run(&mut s, "ai.connection.add", json!({"provider": "openai"})).expect("the connection is added");
    let connection_id = connection["id"].as_str().expect("an id").to_string();
    let model =
        run(&mut s, "ai.model.add", json!({"name": "Main", "connectionId": connection_id, "modelID": "gpt-5.6"})).expect("the model is added");
    let model_id = model["id"].as_str().expect("an id").to_string();

    let err = run(&mut s, "ai.role.assign", json!({"role": "design", "modelId": "model-9"})).expect_err("a missing model is refused");
    assert!(err.contains("model-9"), "{err}");

    run(&mut s, "ai.role.assign", json!({"role": "design", "modelId": model_id})).expect("the role is assigned");
    let out = run(&mut s, "ai.models", json!({})).expect("the models are listed");
    assert_eq!(out["assignments"]["design"], model_id);
}

#[test]
fn removing_a_model_clears_the_roles_it_held() {
    let mut s = session();
    let connection = run(&mut s, "ai.connection.add", json!({"provider": "openai"})).expect("the connection is added");
    let connection_id = connection["id"].as_str().expect("an id").to_string();
    let model =
        run(&mut s, "ai.model.add", json!({"name": "Main", "connectionId": connection_id, "modelID": "gpt-5.6"})).expect("the model is added");
    let model_id = model["id"].as_str().expect("an id").to_string();
    run(&mut s, "ai.role.assign", json!({"role": "design", "modelId": model_id})).expect("the role is assigned");

    run(&mut s, "ai.model.remove", json!({"id": model_id})).expect("the model is removed");
    let out = run(&mut s, "ai.models", json!({})).expect("the models are listed");
    assert_eq!(out["assignments"]["design"], "");
}

#[test]
fn removing_a_connection_removes_the_models_that_used_it() {
    let mut s = session();
    let connection = run(&mut s, "ai.connection.add", json!({"provider": "openai"})).expect("the connection is added");
    let connection_id = connection["id"].as_str().expect("an id").to_string();
    run(&mut s, "ai.model.add", json!({"name": "Main", "connectionId": connection_id, "modelID": "gpt-5.6"})).expect("the model is added");

    let out = run(&mut s, "ai.connection.remove", json!({"id": connection_id})).expect("the connection is removed");
    assert_eq!(out["modelsRemoved"].as_array().expect("the removed models").len(), 1);
    let models = run(&mut s, "ai.models", json!({})).expect("the models are listed");
    assert!(models["models"].as_array().expect("models").is_empty());
}

#[test]
fn a_key_is_stored_and_never_answered() {
    let mut s = session();
    let out =
        run(&mut s, "ai.credential.set", json!({"integration": "openai", "profileId": "connection-1", "field": "api-key", "value": "sk-secret"}))
            .expect("the key is stored");
    assert_eq!(out["status"], "configured");
    // The answer never carries the key.
    assert!(!out.to_string().contains("sk-secret"), "{out}");

    let status = run(&mut s, "ai.credential.status", json!({"integration": "openai", "profileId": "connection-1", "field": "api-key"}))
        .expect("the status is answered");
    assert_eq!(status["status"], "configured");
}

#[test]
fn a_key_can_be_forgotten() {
    let mut s = session();
    run(&mut s, "ai.credential.set", json!({"integration": "openai", "profileId": "connection-1", "field": "api-key", "value": "sk-secret"}))
        .expect("the key is stored");
    let out = run(&mut s, "ai.credential.clear", json!({"integration": "openai", "profileId": "connection-1", "field": "api-key"}))
        .expect("the key is forgotten");
    assert_eq!(out["status"], "missing");
}

#[test]
fn a_build_without_a_store_says_so() {
    let mut s = Session::empty();
    let out =
        run(&mut s, "ai.credential.status", json!({"integration": "openai", "profileId": "c", "field": "api-key"})).expect("the status is answered");
    assert_eq!(out["status"], "unavailable");
}

#[test]
fn the_standing_instructions_are_listed_and_switched() {
    let mut s = session();
    let out = run(&mut s, "ai.skills", json!({})).expect("the skills are listed");
    let skills = out["skills"].as_array().expect("a list of skills");
    assert!(!skills.is_empty());
    assert!(!out["commands"].as_array().expect("the commands").is_empty());

    let switched = run(&mut s, "ai.skill.toggle", json!({"id": "design-craft"})).expect("the skill is switched");
    assert_eq!(switched["enabled"], true);
    let out = run(&mut s, "ai.skills", json!({})).expect("the skills are listed");
    let design = out["skills"].as_array().expect("skills").iter().find(|k| k["name"] == "design-craft").expect("the skill is listed");
    assert_eq!(design["enabled"], true);
}

#[test]
fn switching_a_skill_that_does_not_exist_is_refused() {
    let mut s = session();
    let err = run(&mut s, "ai.skill.toggle", json!({"id": "nope"})).expect_err("an unknown skill is refused");
    assert!(err.contains("nope"), "{err}");
}

#[test]
fn a_person_can_write_a_standing_instruction() {
    let mut s = session();
    let out =
        run(&mut s, "ai.skill.add", json!({"name": "house style", "promptContent": "Use the house colours."})).expect("the instruction is added");
    assert!(out["id"].as_str().expect("an id").starts_with("custom-"));

    let listed = run(&mut s, "ai.skills", json!({})).expect("the skills are listed");
    let own = listed["skills"].as_array().expect("skills").iter().find(|k| k["name"] == "house style").expect("the instruction is listed");
    assert_eq!(own["enabled"], true);
    assert_eq!(own["isDefault"], false);
}

#[test]
fn install_skill_from_markdown_parses_and_registers() {
    let mut s = session();
    let md = "---\nname: github-skill\ndescription: A downloaded skill\n---\n# GitHub Skill\n\nRule 1: Always polish UI.";
    let out = run(&mut s, "ai.skill.installMarkdown", json!({"markdown": md, "url": "https://github.com/test/repo"})).expect("installed");
    assert_eq!(out["name"], "github-skill");
    assert_eq!(out["description"], "A downloaded skill");

    let listed = run(&mut s, "ai.skills", json!({})).expect("skills listed");
    let found = listed["skills"].as_array().expect("skills").iter().find(|k| k["name"] == "github-skill").expect("found");
    assert_eq!(found["enabled"], true);
    assert_eq!(found["isDefault"], false);
}

#[test]
fn the_usage_starts_empty_and_can_be_cleared() {
    let mut s = session();
    let out = run(&mut s, "ai.usage", json!({})).expect("the usage is answered");
    assert_eq!(out["requests"], 0);
    assert_eq!(out["inputTokens"], json!(null));

    s.ai.record(figcraft_ai::usage::UsageEvent::step("openai", "gpt-5.6", 100, 20));
    let out = run(&mut s, "ai.usage", json!({})).expect("the usage is answered");
    assert_eq!(out["requests"], 1);
    assert_eq!(out["inputTokens"], 100);
    assert_eq!(out["models"][0]["model"], "gpt-5.6");

    run(&mut s, "ai.usage.clear", json!({})).expect("the usage is cleared");
    let out = run(&mut s, "ai.usage", json!({})).expect("the usage is answered");
    assert_eq!(out["requests"], 0);
}

#[test]
fn the_settings_survive_a_round_trip_through_the_preferences() {
    let mut s = session();
    run(&mut s, "ai.connection.add", json!({"provider": "openai"})).expect("the connection is added");
    let prefs = s.prefs.clone();
    let text = serde_json::to_string(&prefs).expect("the preferences serialise");
    let back: crate::Prefs = serde_json::from_str(&text).expect("the preferences load");
    assert_eq!(back.ai.models.connections.len(), 1);
}

#[test]
fn no_command_id_is_registered_twice() {
    let all = crate::all_commands();
    let mut ids = Vec::new();
    for c in all {
        assert!(!ids.contains(&c.id), "{} twice", c.id);
        ids.push(c.id);
    }
}

#[test]
fn the_commands_are_registered_with_their_documentation() {
    let all = crate::all_commands();
    for id in ["ai.providers", "ai.models", "ai.model.add", "ai.role.assign", "ai.credential.set", "ai.skills", "ai.usage"] {
        let spec = all.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("{id} is registered"));
        assert!(!spec.params.is_empty(), "{id} documents its parameters");
        // The settings are not document edits, so they are not undo steps.
        assert!(!spec.history, "{id} changes no document");
    }
}

#[test]
fn the_credential_store_is_reachable_through_the_session() {
    let mut s = session();
    let store = s.credentials.as_mut().expect("the store is installed");
    let reference = CredentialRef::new("openai", "connection-1", "api-key");
    store.write(&reference, "sk-1").expect("the key is stored");
    assert_eq!(store.status(&reference).expect("the status"), CredentialStatus::Configured);
}

#[test]
fn the_chat_preferences_are_set_through_a_command() {
    let mut s = session();
    let out = run(&mut s, "ai.chat.preferences", json!({})).expect("the preferences are answered");
    assert_eq!(out["reasoningDisplay"], "collapsed");
    assert_eq!(out["showTools"], true);

    let out = run(&mut s, "ai.chat.preferences.set", json!({"reasoningDisplay": "expanded", "showTools": false})).expect("the preferences are set");
    assert_eq!(out["reasoningDisplay"], "expanded");
    assert_eq!(out["showTools"], false);
    assert_eq!(s.prefs.ai.reasoning_display, figcraft_ai::ReasoningDisplay::Expanded);
}

#[test]
fn the_diagnostics_log_can_be_configured_and_cleared() {
    let mut s = session();
    let event = figcraft_ai::DiagnosticEvent {
        name: "chat.failed".into(),
        category: figcraft_ai::DiagnosticCategory::Ai,
        level: figcraft_ai::DiagnosticLevel::Error,
        timestamp: 1,
        attributes: vec![],
    };
    s.prefs.ai.diagnostics.record(event);
    assert_eq!(s.prefs.ai.diagnostics.events.len(), 1);

    let out = run(&mut s, "ai.diagnostics.settings", json!({"enabled": false, "retention": 100})).expect("the settings are set");
    assert_eq!(out["enabled"], false);
    assert_eq!(out["retention"], 100);
    assert!(!s.prefs.ai.diagnostics.enabled);
    assert_eq!(s.prefs.ai.diagnostics.retention, 100);

    // An unknown retention falls back to the default rather than being kept.
    let out = run(&mut s, "ai.diagnostics.settings", json!({"retention": 7})).expect("the settings are set");
    assert_eq!(out["retention"], figcraft_ai::diagnostics::DEFAULT_RETENTION);

    run(&mut s, "ai.diagnostics.clear", json!({})).expect("the log is cleared");
    assert!(s.prefs.ai.diagnostics.events.is_empty());
}

#[test]
fn the_plugin_directory_is_exposed_through_a_command() {
    let mut s = session();
    let out = run(&mut s, "ai.plugins.catalog", json!({})).expect("the catalog is listed");
    let plugins = out["plugins"].as_array().expect("a list of plugins");
    assert!(!plugins.is_empty());
    for plugin in plugins {
        assert!(plugin["id"].is_string());
        assert!(plugin["name"].is_string());
        assert!(plugin["category"].is_string());
        assert!(plugin["runtime"]["kind"].is_string());
    }
}

#[test]
fn storage_and_media_settings_round_trip_through_commands() {
    let mut s = session();
    run(&mut s, "ai.storage.provider", json!({"provider": "s3-compatible"})).expect("the provider is set");
    run(&mut s, "ai.storage.s3.set", json!({"endpoint": "https://s3.example.com", "bucket": "docs", "region": "eu-west-1"}))
        .expect("the fields are set");
    let out = run(&mut s, "ai.storage.settings", json!({})).expect("the settings are answered");
    assert_eq!(out["provider"], "s3-compatible");
    assert_eq!(out["bucket"], "docs");
    assert_eq!(out["configured"], false, "no keys are saved yet");

    run(&mut s, "ai.media.vectorize.provider", json!({"provider": "fal"})).expect("the provider is set");
    let out = run(&mut s, "ai.media.settings", json!({})).expect("the media settings are answered");
    assert_eq!(out["vectorizeProvider"], "fal");
}
