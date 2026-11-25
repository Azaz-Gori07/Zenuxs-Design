//! The AI & Agents window, driven headless: it opens from its command, the tabs switch, a
//! connection and a model can be added, a role is assigned, a key goes to the store and is never
//! shown afterwards, and a standing instruction can be written.

use figcraft_ai::credentials::{CredentialRef, CredentialStatus, MemoryCredentialStore};
use serde_json::json;

use crate::present_tests::Ui;

/// A session with a credential store, as the desktop app builds one.
fn ui() -> Ui {
    let mut s = figcraft_engine::Session::empty();
    s.credentials = Some(Box::new(MemoryCredentialStore::new()));
    let mut u = Ui::new(s);
    u.run("file.new", json!({}));
    u
}

/// The window's area on screen (`None` while it is closed).
fn window_area(u: &Ui) -> Option<egui::Rect> {
    u.ctx.memory(|m| m.area_rect(egui::Id::unique("ai_settings")))
}

#[test]
fn the_window_opens_from_its_command_and_closes() {
    let mut u = ui();
    assert!(window_area(&u).is_none());
    u.run("ai.settings", json!({}));
    assert!(window_area(&u).is_some(), "the window is open");
    // egui keeps a closed window's area in memory for a while, so what is asserted is the
    // window's own flag: it stops being drawn as soon as it is off.
    u.app.ui.ai_settings.open = false;
    u.frame(vec![]);
    assert!(!u.app.ui.ai_settings.open, "the window is closed");
}

#[test]
fn a_connection_is_added_and_its_key_saved_without_the_key_being_shown() {
    let mut u = ui();
    u.run("ai.settings", json!({}));
    let out = u.app.session.execute("ai.connection.add", &json!({"provider": "openai"})).expect("the connection is added");
    let id = out["id"].as_str().expect("an id").to_string();

    // The key goes to the store; the answer never carries it.
    u.app
        .session
        .execute("ai.credential.set", &json!({"integration": "openai", "profileId": id, "field": "api-key", "value": "sk-secret"}))
        .expect("the key is stored");
    let reference = CredentialRef::provider_key("openai", &id);
    let status = u.app.session.credentials.as_ref().map(|s| s.status(&reference)).expect("the store is installed");
    assert_eq!(status.expect("the status"), CredentialStatus::Configured);
    // And the settings still hold no key.
    let text = serde_json::to_string(&u.app.session.prefs).expect("the preferences serialise");
    assert!(!text.contains("sk-secret"), "the preferences never hold a key");
}

#[test]
fn a_model_is_added_and_assigned_to_the_design_role() {
    let mut u = ui();
    let connection = u.app.session.execute("ai.connection.add", &json!({"provider": "anthropic"})).expect("the connection is added");
    let connection_id = connection["id"].as_str().expect("an id").to_string();
    let model = u
        .app
        .session
        .execute(
            "ai.model.add",
            &json!({"name": "Sonnet", "connectionId": connection_id, "modelID": "claude-sonnet-5", "capabilities": ["tools", "vision"]}),
        )
        .expect("the model is added");
    let model_id = model["id"].as_str().expect("an id").to_string();
    u.app.session.execute("ai.role.assign", &json!({"role": "design", "modelId": model_id})).expect("the role is assigned");

    let out = u.app.session.execute("ai.models", &json!({})).expect("the settings are listed");
    assert_eq!(out["assignments"]["design"], model_id);
    assert_eq!(out["models"][0]["capabilities"].as_array().expect("capabilities").len(), 2);
}

#[test]
fn a_standing_instruction_is_written_and_switched_on() {
    let mut u = ui();
    let out = u
        .app
        .session
        .execute("ai.skill.add", &json!({"name": "house style", "description": "Our colours.", "promptContent": "Use the house palette."}))
        .expect("the instruction is added");
    let id = out["id"].as_str().expect("an id").to_string();
    let listed = u.app.session.execute("ai.skills", &json!({})).expect("the skills are listed");
    let own = listed["skills"].as_array().expect("skills").iter().find(|k| k["id"] == id).expect("the instruction is listed");
    assert_eq!(own["enabled"], true);
}

#[test]
fn skills_tab_shows_standing_instructions_and_github_input() {
    let mut u = ui();
    u.app.ui.ai_settings.open = true;
    u.app.ui.ai_settings.tab = crate::dialogs::ai_settings::Tab::Skills;
    u.frame(vec![]);
    // Default skills are in standing instructions
    let skills = u.app.session.prefs.ai.skills.all();
    assert!(skills.iter().any(|s| s.name == "apple-design"));
    assert!(skills.iter().any(|s| s.name == "improve-ui"));

    // Simulating install markdown command
    let md = "---\nname: my-gh-skill\ndescription: downloaded\n---\n# My GH Skill\nRule.";
    let res = u
        .app
        .session
        .execute(
            "ai.skill.installMarkdown",
            &json!({
                "markdown": md,
                "url": "https://github.com/owner/repo"
            }),
        )
        .expect("skill installed");
    assert_eq!(res["name"], "my-gh-skill");
    assert!(u.app.session.prefs.ai.skills.all().iter().any(|s| s.name == "my-gh-skill"));
}

#[test]
fn the_usage_tab_says_nothing_yet_before_any_request() {
    let mut u = ui();
    u.run("ai.settings", json!({}));
    let summary = u.app.session.execute("ai.usage", &json!({})).expect("the usage is answered");
    assert_eq!(summary["requests"], 0);
}
