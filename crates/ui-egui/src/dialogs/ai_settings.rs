//! The AI & Agents window: the models, the keys, the standing instructions and the usage.
//!
//! The window reads the settings through the [`ai.*`](figcraft_engine::cmd::ai) commands and
//! acts through them too, so what a person changes here is what an agent changes over the
//! control channel.
//!
//! A key is typed into the connection's own field and goes to the credential store at once; the
//! field never holds one afterwards, and the status is what the settings show.

use egui::{TextEdit, Ui};
use serde_json::json;

use figcraft_ai::credentials::{CredentialRef, CredentialStatus};
use figcraft_ai::models::{AIModelCapability, AIModelConnection, AIModelProfile, AIModelRole};
use figcraft_ai::providers;

use crate::FigApp;
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::widgets;

/// The window's tabs, in the order they show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    /// Chat behaviour and interface settings.
    #[default]
    General,
    /// The model connections, the profiles and the role assignments.
    Models,
    /// The standing instructions.
    Skills,
    /// Stock photo keys and image vectorization.
    Media,
    /// The curated plugin directory.
    Plugins,
    /// The cloud storage workspace.
    Storage,
    /// The MCP servers the agent may reach.
    Automation,
    /// The cloud account.
    Account,
    /// What the models have been used for.
    Usage,
    /// The AI event log.
    Diagnostics,
}

impl Tab {
    /// The tab's label.
    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Models => "Models",
            Self::Skills => "Skills",
            Self::Media => "Media",
            Self::Plugins => "Plugins",
            Self::Storage => "Storage",
            Self::Automation => "Automation",
            Self::Account => "Account",
            Self::Usage => "Usage",
            Self::Diagnostics => "Diagnostics",
        }
    }

    /// Every tab, in order.
    fn all() -> &'static [Tab] {
        &[
            Self::General,
            Self::Models,
            Self::Skills,
            Self::Media,
            Self::Plugins,
            Self::Storage,
            Self::Automation,
            Self::Account,
            Self::Usage,
            Self::Diagnostics,
        ]
    }
}

/// What the window is editing while a field is being typed into.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Editor {
    // (`account_job`, below, is deliberately not derived: it owns a channel end.)
    /// Whether the window shows.
    pub open: bool,
    /// The tab in front.
    pub tab: Tab,
    /// A connection's key being typed, by connection id.
    pub key_drafts: std::collections::BTreeMap<String, String>,
    /// A standing instruction being written.
    pub skill_draft: Option<SkillDraft>,
    /// The connection a new model goes onto, chosen but not yet added.
    pub new_model_connection: String,
    /// The provider a new connection goes onto.
    pub new_connection_provider: String,
    /// The base URL of a new compatible connection.
    pub new_connection_url: String,
    /// An MCP connection being written.
    pub mcp_name: String,
    pub mcp_url: String,
    pub mcp_bearer: bool,
    /// A stock photo key being typed.
    pub pexels_draft: String,
    /// Media keys being typed, by integration.
    pub media_key_drafts: std::collections::BTreeMap<String, String>,
    /// Storage keys being typed, by field.
    pub storage_key_drafts: std::collections::BTreeMap<String, String>,
    /// What the storage calls answered.
    pub storage_results: Vec<(String, String)>,
    /// The account's fields while they are being typed into.
    pub account_email: String,
    pub account_password: String,
    pub account_name: String,
    /// What the account calls answered, newest last.
    pub account_results: Vec<(String, String)>,
}

/// The account call on its worker thread.
pub struct AccountJob {
    /// The answer as it lands.
    pub rx: std::sync::mpsc::Receiver<Result<AccountReply, String>>,
    /// The thread the call runs on.
    pub _thread: Option<std::thread::JoinHandle<()>>,
}

/// A standing instruction being written.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SkillDraft {
    pub name: String,
    pub description: String,
    pub prompt: String,
}

/// The window, shown while `app.ui.ai_settings.open`.
pub fn window(app: &mut FigApp, ctx: &egui::Context) {
    let mut open = app.ui.ai_settings.open;
    let t = Tokens::get(ctx);
    let mut editor = std::mem::take(&mut app.ui.ai_settings);
    let mut ran: Vec<(String, serde_json::Value)> = Vec::new();
    let mut saved_skills = false;

    egui::Window::new(tr("AI & Agents"))
        .open(&mut open)
        .id(egui::Id::unique("ai_settings"))
        .default_width(460.0)
        .default_height(520.0)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.set_width(460.0);
            drain_storage_job(app, &mut editor);
            drain_account_job(app, &mut editor);
            // The tabs row.
            let tabs = Tab::all().iter().map(|t| t.label()).collect::<Vec<_>>();
            if let Some(i) = widgets::segmented_text(ui, &tabs, editor.tab_index()) {
                editor.set_tab(Tab::all()[i]);
            }
            ui.add_space(6.0);

            match editor.tab {
                Tab::General => general_tab(app, ui, &mut ran, &t),
                Tab::Models => models_tab(app, ui, &mut editor, &mut ran, &t),
                Tab::Media => media_tab(app, ui, &mut editor, &mut ran, &t),
                Tab::Plugins => plugins_tab(app, ui, &t),
                Tab::Storage => storage_tab(app, ui, &mut editor, &mut ran, &t),
                Tab::Automation => automation_tab(app, ui, &mut editor, &mut ran, &t),
                Tab::Account => account_tab(app, ui, &mut editor, &mut ran, &t),
                Tab::Skills => {
                    saved_skills = skills_tab(app, ui, &mut editor, &mut ran, &t);
                }
                Tab::Usage => usage_tab(app, ui, &t),
                Tab::Diagnostics => diagnostics_tab(app, ui, &mut ran, &t),
            }
        });

    app.ui.ai_settings = editor;
    app.ui.ai_settings.open = open;
    for (id, params) in ran {
        app.run(&id, params);
    }
    if saved_skills {
        // The drafts are the window's own; the settings were written as they were typed.
    }
    if !open {
        app.ui.ai_settings.open = false;
    }
}

/// Where the current tab sits in the tab list.
impl Editor {
    fn tab_index(&self) -> usize {
        Tab::all().iter().position(|t| *t == self.tab).unwrap_or(0)
    }

    fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }
}

/// The field in the UiState the window lives in.
impl FigApp {
    /// Open the AI & Agents window.
    pub fn open_ai_settings(&mut self) {
        self.ui.ai_settings.open = true;
    }
}

// ---------- General ----------

/// The chat's own settings: how reasoning shows and whether tool steps are visible.
fn general_tab(app: &mut FigApp, ui: &mut Ui, ran: &mut Vec<(String, serde_json::Value)>, t: &Tokens) {
    section_header(ui, tr("Chat"), t);
    widgets::label(ui, tr("How the model's reasoning appears in the conversation."), t.text_secondary);
    ui.add_space(4.0);
    let mode = app.session.prefs.ai.reasoning_display;
    let options = [tr("Collapsed by default").to_string(), tr("Expand while thinking").to_string(), tr("Expanded by default").to_string()];
    let ids = ["collapsed", "while-thinking", "expanded"];
    let index = ids.iter().position(|id| *id == mode.id()).unwrap_or(0);
    if widgets::combo_field(ui, egui::Id::unique("ai_reasoning_display"), &options[index], 180.0, &options)
        .and_then(|choice| options.iter().position(|o| *o == choice).map(|i| ids[i]))
        .map(|id| ran.push(("ai.chat.preferences.set".into(), serde_json::json!({"reasoningDisplay": id}))))
        .is_some()
    {
        return;
    }
    if widgets::checkbox(ui, app.session.prefs.ai.show_tools, tr("Show tool steps")) {
        ran.push(("ai.chat.preferences.set".into(), serde_json::json!({"showTools": !app.session.prefs.ai.show_tools})));
    }
    widgets::label(ui, tr("Chat conversations are kept with the preferences. New chat, search and scope are on the AI tab."), t.text_secondary);
    ui.add_space(10.0);
    section_header(ui, tr("Provider setup"), t);
    widgets::label(
        ui,
        tr(
            "Add a connection with its API key, choose a model, then assign it to the Design role. A Vision model verifies sourced images before they are placed.",
        ),
        t.text_secondary,
    );
}

// ---------- Models ----------

fn models_tab(app: &mut FigApp, ui: &mut Ui, editor: &mut Editor, ran: &mut Vec<(String, serde_json::Value)>, t: &Tokens) {
    let settings = app.session.prefs.ai.models.clone();
    let registry = providers::providers();

    section_header(ui, tr("Connections"), t);
    for connection in &settings.connections {
        let Some(def) = registry.iter().find(|p| p.id == connection.provider) else { continue };
        connection_row(app, ui, editor, ran, t, connection, def);
        ui.add_space(6.0);
    }

    // Add a connection.
    ui.add_space(4.0);
    let provider_ids: Vec<String> = registry.iter().map(|p| p.id.id().to_string()).collect();
    let names: Vec<String> = registry.iter().map(|p| p.name.to_string()).collect();
    let chosen = registry.iter().find(|p| p.id.id() == editor.new_connection_provider).map(|p| p.id);
    let index = chosen.and_then(|c| registry.iter().position(|p| p.id == c)).unwrap_or(0);
    ui.horizontal(|ui| {
        if widgets::combo_field(ui, egui::Id::unique("ai_new_connection"), &names[index].clone(), 160.0, &names)
            .and_then(|name| registry.iter().find(|p| p.name == name).map(|p| p.id.id().to_string()))
            .map(|id| editor.new_connection_provider = id)
            .is_some()
        {
            return;
        }
        if let Some(def) = registry.get(index) {
            if def.supports_custom_base_url() {
                let url = editor.new_connection_url.clone();
                if widgets::text_field(ui, egui::Id::unique("ai_new_connection_url"), &url, 150.0, tr("Base URL"))
                    .map(|v| editor.new_connection_url = v)
                    .is_some()
                {
                    return;
                }
            }
            if widgets::button(ui, tr("Add connection"), false, true).clicked() {
                let mut params = serde_json::json!({"provider": def.id.id()});
                if def.supports_custom_base_url() && !editor.new_connection_url.trim().is_empty() {
                    params["customBaseURL"] = serde_json::json!(editor.new_connection_url.trim());
                }
                ran.push(("ai.connection.add".into(), params));
                editor.new_connection_url.clear();
            }
        }
        let _ = &provider_ids;
    });

    ui.add_space(10.0);
    section_header(ui, tr("Models"), t);
    for model in &settings.models {
        model_row(ui, ran, t, model, &settings);
        ui.add_space(6.0);
    }

    // Add a model.
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let options: Vec<String> =
            std::iter::once(tr("Choose a connection…").to_string()).chain(settings.connections.iter().map(|c| c.id.clone())).collect();
        let current = if editor.new_model_connection.is_empty() { options[0].clone() } else { editor.new_model_connection.clone() };
        if widgets::combo_field(ui, egui::Id::unique("ai_new_model_connection"), &current, 170.0, &options)
            .and_then(|v| (v != options[0]).then_some(v))
            .map(|v| editor.new_model_connection = v)
            .is_some()
        {
            return;
        }
        if widgets::button(ui, tr("Add model"), false, !editor.new_model_connection.is_empty()).clicked() {
            ran.push(("ai.model.add".into(), serde_json::json!({"name": tr("New model"), "connectionId": editor.new_model_connection.clone()})));
            editor.new_model_connection.clear();
        }
    });

    ui.add_space(10.0);
    section_header(ui, tr("Assignments"), t);
    for role in AIModelRole::all() {
        role_row(ui, ran, t, &settings, *role);
    }
}

/// One connection: its name, its key field and its status.
fn connection_row(
    app: &mut FigApp,
    ui: &mut Ui,
    editor: &mut Editor,
    ran: &mut Vec<(String, serde_json::Value)>,
    t: &Tokens,
    connection: &AIModelConnection,
    def: &providers::AIProviderDef,
) {
    ui.horizontal(|ui| {
        ui.label(def.name);
        if connection.provider.supports_custom_base_url() && !connection.custom_base_url.is_empty() {
            ui.label(text_secondary(t, &connection.custom_base_url));
        }
        // The key's status, from the store the platform installed.
        let reference = CredentialRef::provider_key(connection.provider.id(), &connection.id);
        let status = app.session.credentials.as_ref().map(|s| s.status(&reference)).unwrap_or_else(|| Ok(CredentialStatus::Unavailable));
        let (label, colour) = match status.as_ref().copied().unwrap_or(CredentialStatus::Unavailable) {
            CredentialStatus::Configured => (tr("Key saved"), t.success),
            CredentialStatus::Missing => (tr("Needs key"), t.warning),
            CredentialStatus::Unavailable => (tr("No key store"), t.text_tertiary),
        };
        widgets::label(ui, label, colour);
        if !def.key_url.is_empty() && widgets::button(ui, tr("Get a key"), false, true).clicked() {
            app.services.open_url(def.key_url);
        }
        if widgets::button(ui, tr("Remove"), false, false).clicked() {
            ran.push(("ai.connection.remove".into(), serde_json::json!({"id": connection.id})));
        }
    });

    // The key field: typed here, sent to the store, never kept in the draft afterwards.
    ui.horizontal(|ui| {
        let draft = editor.key_drafts.entry(connection.id.clone()).or_default().clone();
        ui.label(tr("API key"));
        let mut typed = draft;
        widgets::text_input(ui, egui::Id::unique(("ai_key", connection.id.as_str())), &mut typed, 200.0, tr("API key"));
        editor.key_drafts.insert(connection.id.clone(), typed.clone());
        let draft = typed;
        if widgets::button(ui, tr("Save key"), false, !draft.trim().is_empty()).clicked() {
            ran.push((
                "ai.credential.set".into(),
                serde_json::json!({
                    "integration": connection.provider.id(),
                    "profileId": connection.id,
                    "field": "api-key",
                    "value": draft.trim(),
                }),
            ));
            editor.key_drafts.remove(&connection.id);
        }
    });
}

/// A model's effective id, for the ACP agent picker.
fn effective_id(model: &AIModelProfile) -> String {
    figcraft_ai::models::AIModelSettings::effective_model_id(model).to_string()
}

/// One model: its name, its provider model and what it can do.
fn model_row(
    ui: &mut Ui,
    ran: &mut Vec<(String, serde_json::Value)>,
    t: &Tokens,
    model: &AIModelProfile,
    settings: &figcraft_ai::models::AIModelSettings,
) {
    let Some(connection) = settings.connection(&model.connection_id) else { return };
    let Some(def) = providers::provider(connection.provider) else { return };
    ui.horizontal(|ui| {
        ui.label(model.name.clone());
        ui.label(text_secondary(t, figcraft_ai::models::AIModelSettings::effective_model_id(model)));
        if widgets::button(ui, tr("Remove"), false, false).clicked() {
            ran.push(("ai.model.remove".into(), serde_json::json!({"id": model.id})));
        }
    });
    if connection.provider.is_agent() {
        ui.horizontal(|ui| {
            ui.label(tr("Agent"));
            let agents = figcraft_ai::acp::agents();
            let names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
            let current =
                agents.iter().find(|a| a.id == effective_id(model)).map(|a| a.name.clone()).unwrap_or_else(|| effective_id(model).to_string());
            let options = std::iter::once(current.clone()).chain(names.iter().cloned()).collect::<Vec<_>>();
            if widgets::combo_field(ui, egui::Id::unique(("ai_acp_agent", model.id.as_str())), &current, 160.0, &options)
                .and_then(|name| agents.iter().find(|a| a.name == name).map(|a| a.id.clone()))
                .map(|id| ran.push(("ai.model.update".into(), serde_json::json!({"id": model.id, "customModelID": id}))))
                .is_some()
            {
                return;
            }
            let _ = options;
        });
    }
    ui.horizontal(|ui| {
        // The model picker: the provider's own models, or a typed id for a compatible one.
        let effective = figcraft_ai::models::AIModelSettings::effective_model_id(model).to_string();
        if def.models.is_empty() {
            ui.label(tr("Model ID"));
            if widgets::text_field(ui, egui::Id::unique(("ai_model_id", model.id.as_str())), &effective, 140.0, tr("Model ID"))
                .map(|v| {
                    ran.push(("ai.model.update".into(), serde_json::json!({"id": model.id, "customModelID": v.trim()})));
                })
                .is_some()
            {
                return;
            }
        } else {
            let names: Vec<String> = def.models.iter().map(|m| m.name.to_string()).collect();
            let index = def.models.iter().position(|m| m.id == effective).unwrap_or(0);
            if widgets::combo_field(
                ui,
                egui::Id::unique(("ai_model_pick", model.id.as_str())),
                &names[index.min(names.len().saturating_sub(1))].clone(),
                150.0,
                &names,
            )
            .and_then(|name| def.models.iter().find(|m| m.name == name).map(|m| m.id.to_string()))
            .map(|id| {
                ran.push(("ai.model.update".into(), serde_json::json!({"id": model.id, "modelID": id, "customModelID": ""})));
            })
            .is_some()
            {
                return;
            }
        }
        // What the model can do.
        for (capability, label) in [(AIModelCapability::Tools, tr("Tools")), (AIModelCapability::Vision, tr("Vision"))] {
            let on = model.capabilities.contains(&capability);
            if widgets::checkbox(ui, on, label) {
                let mut next = model.capabilities.clone();
                if on {
                    next.retain(|c| *c != capability);
                } else {
                    next.push(capability);
                }
                ran.push((
                    "ai.model.update".into(),
                    serde_json::json!({"id": model.id, "capabilities": next.iter().map(|c| match c {
                        AIModelCapability::Tools => "tools",
                        AIModelCapability::Vision => "vision",
                    }).collect::<Vec<_>>()}),
                ));
            }
        }
    });
}

/// One role: which model handles it.
fn role_row(ui: &mut Ui, ran: &mut Vec<(String, serde_json::Value)>, t: &Tokens, settings: &figcraft_ai::models::AIModelSettings, role: AIModelRole) {
    let assigned = match role {
        AIModelRole::Design => &settings.assignments.design,
        AIModelRole::Review => &settings.assignments.review,
        AIModelRole::Fast => &settings.assignments.fast,
        AIModelRole::Vision => &settings.assignments.vision,
    }
    .clone();
    let label = match role {
        AIModelRole::Design => tr("Design agent"),
        AIModelRole::Review => tr("Review"),
        AIModelRole::Fast => tr("Fast tasks"),
        AIModelRole::Vision => tr("Vision"),
    };
    ui.horizontal(|ui| {
        ui.label(label);
        let mut options: Vec<String> = settings.models.iter().map(|m| m.name.clone()).collect();
        if options.is_empty() {
            widgets::label(ui, tr("No models yet"), t.text_tertiary);
            return;
        }
        let none = tr("None").to_string();
        options.insert(0, none.clone());
        let shown = settings.models.iter().find(|m| m.id == assigned).map(|m| m.name.clone()).unwrap_or(none);
        if widgets::combo_field(ui, egui::Id::unique(("ai_role", role.id())), &shown, 150.0, &options)
            .and_then(|name| settings.models.iter().find(|m| m.name == name).map(|m| m.id.clone()))
            .map(|id| ran.push(("ai.role.assign".into(), serde_json::json!({"role": role.id(), "modelId": id}))))
            .is_some()
        {
            return;
        }
        if widgets::button(ui, tr("Clear"), false, !assigned.is_empty()).clicked() {
            ran.push(("ai.role.assign".into(), serde_json::json!({"role": role.id(), "modelId": ""})));
        }
    });
}

// ---------- Media ----------

/// The stock-photo keys and the vectorization service.
fn media_tab(app: &mut FigApp, ui: &mut Ui, editor: &mut Editor, ran: &mut Vec<(String, serde_json::Value)>, t: &Tokens) {
    section_header(ui, tr("Stock photos"), t);
    widgets::label(ui, tr("The stock_photo tool searches these services. Keys go to the credential store, not to the settings."), t.text_secondary);
    ui.add_space(4.0);
    for (integration, field, label, url) in [
        ("pexels", "api-key", tr("Pexels API Key"), "https://www.pexels.com/api/"),
        ("unsplash", "access-key", tr("Unsplash Access Key"), "https://unsplash.com/oauth/applications"),
    ] {
        let reference = figcraft_ai::CredentialRef::new(integration, "default", field);
        let configured = app.session.credentials.as_ref().and_then(|s| s.status(&reference).ok()) == Some(figcraft_ai::CredentialStatus::Configured);
        ui.horizontal(|ui| {
            ui.label(label);
            let draft = editor.media_key_drafts.entry(integration.to_string()).or_default().clone();
            let mut typed = draft.clone();
            widgets::text_input(
                ui,
                egui::Id::unique(("ai_media_key", integration)),
                &mut typed,
                160.0,
                if configured { tr("Saved — replace") } else { tr("API key") },
            );
            editor.media_key_drafts.insert(integration.to_string(), typed.clone());
            let _ = url;
            if widgets::button(ui, tr("Save key"), false, !typed.trim().is_empty()).clicked() {
                ran.push((
                    "ai.credential.set".into(),
                    json!({"integration": integration, "profileId": "default", "field": field, "value": typed.trim()}),
                ));
                editor.media_key_drafts.remove(integration);
            }
            if configured && widgets::button(ui, tr("Clear"), false, false).clicked() {
                ran.push(("ai.credential.clear".into(), json!({"integration": integration, "profileId": "default", "field": field})));
            }
            if widgets::button(ui, tr("Get a key"), false, true).on_hover_text(url).clicked() {
                app.services.open_url(url);
            }
        });
    }

    ui.add_space(10.0);
    section_header(ui, tr("Image vectorization"), t);
    widgets::label(ui, tr("Turn an image layer into editable vectors. The key goes to the credential store."), t.text_secondary);
    ui.horizontal(|ui| {
        ui.label(tr("Vectorization service"));
        let provider = app.session.prefs.ai.vectorize_provider;
        let names = [tr("Recraft").to_string(), tr("fal.ai").to_string()];
        let current = names[if provider == figcraft_ai::VectorizeProvider::Fal { 1 } else { 0 }].clone();
        if widgets::combo_field(ui, egui::Id::unique("ai_vectorize_provider"), &current, 140.0, &names)
            .and_then(|name| (name == names[1]).then_some("fal"))
            .map(|id| ran.push(("ai.media.vectorize.provider".into(), json!({"provider": id}))))
            .is_some()
        {
            return;
        }
        if widgets::button(ui, tr("Get a key"), false, true).on_hover_text(provider.key_url()).clicked() {
            app.services.open_url(provider.key_url());
        }
    });
    let reference = app.session.prefs.ai.vectorize_provider.credential_ref();
    let configured = app.session.credentials.as_ref().and_then(|s| s.status(&reference).ok()) == Some(figcraft_ai::CredentialStatus::Configured);
    ui.horizontal(|ui| {
        ui.label(tr("API key"));
        let draft = editor.media_key_drafts.entry("vectorize".to_string()).or_default().clone();
        let mut typed = draft.clone();
        widgets::text_input(
            ui,
            egui::Id::unique("ai_vectorize_key"),
            &mut typed,
            160.0,
            if configured { tr("Saved — replace") } else { tr("API key") },
        );
        editor.media_key_drafts.insert("vectorize".to_string(), typed.clone());
        if widgets::button(ui, tr("Save key"), false, !typed.trim().is_empty()).clicked() {
            ran.push((
                "ai.credential.set".into(),
                json!({"integration": reference.integration_id, "profileId": reference.profile_id, "field": reference.field, "value": typed.trim()}),
            ));
            editor.media_key_drafts.remove("vectorize");
        }
        if configured && widgets::button(ui, tr("Clear"), false, false).clicked() {
            ran.push((
                "ai.credential.clear".into(),
                json!({"integration": reference.integration_id, "profileId": reference.profile_id, "field": reference.field}),
            ));
        }
    });
}

// ---------- Plugins ----------

/// The curated plugin directory: what is available, its category and whether it can run here.
fn plugins_tab(app: &mut FigApp, ui: &mut Ui, t: &Tokens) {
    section_header(ui, tr("Plugin directory"), t);
    widgets::label(ui, tr("A curated directory of plugins. Plugins run in FigCraft's sandbox; install only ones you trust."), t.text_secondary);
    ui.add_space(6.0);
    let installed: Vec<&str> = app.session.prefs.plugins.iter().map(|p| p.manifest.id.as_str()).collect();
    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
        for entry in figcraft_ai::plugin_catalog::catalog() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&entry.name).strong());
                    ui.label(egui::RichText::new(format!("v{}", entry.version)).color(t.text_secondary).size(11.0));
                });
                ui.label(egui::RichText::new(entry.category.label()).color(t.text_tertiary).size(11.0));
                ui.label(&entry.description);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(tr("Permissions")).color(t.text_secondary).size(11.0));
                    for permission in &entry.permissions {
                        ui.label(egui::RichText::new(permission).color(t.text_tertiary).size(10.0));
                    }
                });
                let status = match &entry.runtime {
                    figcraft_ai::PluginRuntime::Wasm { .. } => {
                        if installed.contains(&entry.id.as_str()) {
                            tr("Installed").to_string()
                        } else {
                            tr("Available").to_string()
                        }
                    }
                    figcraft_ai::PluginRuntime::Unavailable { reason } => trf("Unavailable: {reason}", &[("reason", reason)]).to_string(),
                };
                ui.label(egui::RichText::new(status).color(t.text_secondary).size(11.0));
            });
            ui.add_space(4.0);
        }
    });
}

// ---------- Storage ----------

/// The cloud storage provider, its fields, the connection test and the workspace list.
fn storage_tab(app: &mut FigApp, ui: &mut Ui, editor: &mut Editor, ran: &mut Vec<(String, serde_json::Value)>, t: &Tokens) {
    section_header(ui, tr("Storage provider"), t);
    let provider = app.session.prefs.ai.storage_provider;
    let names = [tr("FigCraft server").to_string(), tr("S3 storage").to_string()];
    let current = names[if provider == figcraft_ai::StorageProvider::S3Compatible { 1 } else { 0 }].clone();
    if widgets::combo_field(ui, egui::Id::unique("ai_storage_provider"), &current, 160.0, &names)
        .and_then(|name| (name == names[1]).then_some("s3-compatible").or_else(|| (name == names[0]).then_some("figcraft-server")))
        .map(|id| ran.push(("ai.storage.provider".into(), json!({"provider": id}))))
        .is_some()
    {
        return;
    }
    widgets::label(ui, provider.description(), t.text_secondary);

    if provider == figcraft_ai::StorageProvider::S3Compatible {
        ui.add_space(6.0);
        for (field, label, value) in [
            ("endpoint", tr("Endpoint"), app.session.prefs.ai.s3.endpoint.clone()),
            ("bucket", tr("Bucket"), app.session.prefs.ai.s3.bucket.clone()),
            ("region", tr("Region"), app.session.prefs.ai.s3.region.clone()),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                if let Some(v) = widgets::text_field(ui, egui::Id::unique(("ai_s3", field)), &value, 180.0, label) {
                    ran.push(("ai.storage.s3.set".into(), json!({field: v.trim()})));
                }
            });
        }
        // Two credential fields, as zenuxs' S3 provider defines them.
        for field in ["access-key-id", "secret-access-key"] {
            let reference = figcraft_ai::CredentialRef::new("s3", "default", field);
            let configured =
                app.session.credentials.as_ref().and_then(|s| s.status(&reference).ok()) == Some(figcraft_ai::CredentialStatus::Configured);
            ui.horizontal(|ui| {
                ui.label(if field == "access-key-id" { tr("Access key ID") } else { tr("Secret access key") });
                let draft = editor.storage_key_drafts.entry(field.to_string()).or_default().clone();
                let mut typed = draft.clone();
                widgets::text_input(
                    ui,
                    egui::Id::unique(("ai_s3_key", field)),
                    &mut typed,
                    160.0,
                    if configured { tr("Saved — replace") } else { tr("API key") },
                );
                editor.storage_key_drafts.insert(field.to_string(), typed.clone());
                if widgets::button(ui, tr("Save key"), false, !typed.trim().is_empty()).clicked() {
                    ran.push((
                        "ai.credential.set".into(),
                        json!({"integration": "s3", "profileId": "default", "field": field, "value": typed.trim()}),
                    ));
                    editor.storage_key_drafts.remove(field);
                }
                if configured && widgets::button(ui, tr("Clear"), false, false).clicked() {
                    ran.push(("ai.credential.clear".into(), json!({"integration": "s3", "profileId": "default", "field": field})));
                }
            });
        }
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if widgets::button(ui, tr("Test connection"), false, true).clicked() {
            start_storage_job(app, StorageJobKind::Test);
        }
        if widgets::button(ui, tr("Refresh workspace"), false, true).clicked() {
            start_storage_job(app, StorageJobKind::List);
        }
        if widgets::button(ui, tr("Copy CORS JSON"), false, true).clicked() {
            let json = figcraft_ai::storage::cors_json(&["https://figcraft.app".into()]);
            app.services.clipboard_set_text(&json);
            app.ui.chat.failure = None;
            editor.account_results.push(("ok".into(), tr("CORS JSON copied.").to_string()));
        }
    });
    for (kind, message) in &editor.storage_results {
        widgets::label(ui, message, if kind == "error" { t.danger } else { t.success });
    }
    ui.add_space(6.0);
    section_header(ui, tr("Stored documents"), t);
    let docs = app.session.prefs.ai.storage_documents.clone();
    for doc in &docs {
        ui.horizontal(|ui| {
            ui.label(&doc.name);
            ui.label(egui::RichText::new(&doc.updated_at).color(t.text_secondary).size(11.0));
        });
    }
    if docs.is_empty() {
        widgets::label(ui, tr("No stored documents yet."), t.text_tertiary);
    }
}

/// A storage call on its worker.
pub struct StorageJob {
    /// The reply channel.
    pub rx: std::sync::mpsc::Receiver<Result<String, String>>,
    /// Keeps the worker alive until the reply lands.
    pub _thread: Option<std::thread::JoinHandle<()>>,
}

#[derive(Clone, Copy)]
pub enum StorageJobKind {
    Test,
    List,
}

/// The Account tab's session token, from the credential store.
fn credentials_read_token(session: &figcraft_engine::Session) -> Option<String> {
    let reference = figcraft_ai::CredentialRef::new("cloud", "session", "token");
    session.credentials.as_ref().and_then(|s| s.read(&reference).ok().flatten())
}

fn start_storage_job(app: &mut FigApp, kind: StorageJobKind) {
    let provider = app.session.prefs.ai.storage_provider;
    let s3 = app.session.prefs.ai.s3.clone();
    let credentials = app.session.credentials.as_ref();
    let read_key = |integration: &str, field: &str| -> Option<String> {
        let reference = figcraft_ai::CredentialRef::new(integration, "default", field);
        credentials.and_then(|s| s.read(&reference).ok().flatten())
    };
    // The store is not thread-safe: read the keys here, before the worker starts.
    let server = (app.session.prefs.ai.account_server.clone(), credentials_read_token(&app.session));
    let access = read_key("s3", "access-key-id");
    let secret = read_key("s3", "secret-access-key");
    let factory = app.storage_factory.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let thread = std::thread::Builder::new().name("figcraft-storage".into()).spawn(move || {
        let session: Box<dyn figcraft_ai::StorageSession> = match provider {
            figcraft_ai::StorageProvider::FigcraftServer => {
                let Some((url, Some(token))) = (!server.0.is_empty()).then_some(server) else { return };
                factory.make_server(url, token)
            }
            figcraft_ai::StorageProvider::S3Compatible => {
                let (Some(access), Some(secret)) = (access, secret) else { return };
                factory.make_s3(s3, access, secret)
            }
        };
        let answer: Result<String, String> = match kind {
            StorageJobKind::Test => session.test_connection().map_err(|e| e.message()),
            StorageJobKind::List => session
                .list()
                .map(|docs| {
                    serde_json::to_string(&docs.iter().map(|d| json!({"id": d.id, "name": d.name, "updatedAt": d.updated_at})).collect::<Vec<_>>())
                        .unwrap_or_default()
                })
                .map_err(|e| e.message()),
        };
        let _ = tx.send(answer);
    });
    if let Ok(thread) = thread {
        app.ui.storage_job = Some(StorageJob { rx, _thread: Some(thread) });
    }
}

/// Take the storage worker's answer.
fn drain_storage_job(app: &mut FigApp, editor: &mut Editor) {
    let Some(job) = &app.ui.storage_job else { return };
    let reply = job.rx.try_recv();
    if reply.is_ok() {
        app.ui.storage_job = None;
    }
    let Ok(reply) = reply else { return };
    match reply {
        Ok(text) => {
            if let Ok(docs) = serde_json::from_str::<Vec<(String, String, String)>>(&text) {
                let _ = docs;
            }
            // A list answer arrives as a JSON array; a test as a sentence.
            if text.trim_start().starts_with('[')
                && let Ok(list) = serde_json::from_str::<Vec<figcraft_ai::StorageDocument>>(&text)
            {
                app.run(
                    "ai.storage.documents.set",
                    json!({"documents": list.iter().map(|d| json!({"id": d.id, "name": d.name, "updatedAt": d.updated_at})).collect::<Vec<_>>()}),
                );
            } else {
                editor.storage_results.push(("ok".into(), text));
            }
        }
        Err(e) => editor.storage_results.push(("error".into(), e)),
    }
}

// ---------- Automation ----------

/// The MCP servers the agent may call tools on.
fn automation_tab(app: &mut FigApp, ui: &mut Ui, editor: &mut Editor, ran: &mut Vec<(String, serde_json::Value)>, t: &Tokens) {
    section_header(ui, tr("MCP servers"), t);
    widgets::label(ui, tr("External servers the agent may call tools on. Their tools join the chat as mcp__server__tool."), t.text_secondary);
    ui.add_space(4.0);
    for connection in app.session.prefs.ai.mcp.connections.clone() {
        ui.horizontal(|ui| {
            if widgets::checkbox(ui, connection.enabled, &connection.name) {
                ran.push(("ai.mcp.update".into(), serde_json::json!({"id": connection.id, "enabled": !connection.enabled})));
            }
            ui.label(text_secondary(t, &connection.url));
            let bearer = connection.authentication == figcraft_ai::mcp::MCPAuthentication::Bearer;
            if widgets::button(ui, if bearer { tr("Token saved") } else { tr("No token") }, false, !bearer && false).clicked() {
                // The token goes to the store like any key.
                ran.push(("ai.mcp.credential.set".into(), serde_json::json!({"profileId": connection.id, "value": ""})));
            }
            if widgets::button(ui, tr("Remove"), false, false).clicked() {
                ran.push(("ai.mcp.remove".into(), serde_json::json!({"id": connection.id})));
            }
        });
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(tr("Name"));
        if widgets::text_field(ui, egui::Id::unique("ai_mcp_name"), &editor.mcp_name, 100.0, tr("Name")).map(|v| editor.mcp_name = v).is_some() {
            return;
        }
        ui.label(tr("URL"));
        if widgets::text_field(ui, egui::Id::unique("ai_mcp_url"), &editor.mcp_url, 170.0, tr("URL")).map(|v| editor.mcp_url = v).is_some() {
            return;
        }
        if widgets::checkbox(ui, editor.mcp_bearer, tr("Bearer token")) {
            editor.mcp_bearer = !editor.mcp_bearer;
            return;
        }
        let ready = !editor.mcp_name.trim().is_empty() && !editor.mcp_url.trim().is_empty();
        if widgets::button(ui, tr("Add connection"), false, ready).clicked() {
            ran.push((
                "ai.mcp.add".into(),
                serde_json::json!({"name": editor.mcp_name.trim(), "url": editor.mcp_url.trim(), "bearer": editor.mcp_bearer}),
            ));
            editor.mcp_name.clear();
            editor.mcp_url.clear();
            editor.mcp_bearer = false;
        }
    });

    ui.add_space(10.0);
    section_header(ui, tr("Stock photo key"), t);
    widgets::label(ui, tr("The stock_photo tool searches Pexels; the key goes to the credential store, not to the settings."), t.text_secondary);
    ui.horizontal(|ui| {
        ui.label(tr("Pexels API key"));
        let draft = editor.pexels_draft.clone();
        let mut typed = draft;
        widgets::text_input(ui, egui::Id::unique("ai_pexels_key"), &mut typed, 200.0, tr("Pexels API key"));
        editor.pexels_draft = typed.clone();
        if widgets::button(ui, tr("Save key"), false, !typed.trim().is_empty()).clicked() {
            ran.push((
                "ai.credential.set".into(),
                serde_json::json!({"integration": "pexels", "profileId": "default", "field": "api-key", "value": typed.trim()}),
            ));
            editor.pexels_draft.clear();
        }
    });

    ui.add_space(10.0);
    section_header(ui, tr("This app's own server"), t);
    widgets::label(
        ui,
        tr(
            "figcraft-cli mcp serves this document over the Model Context Protocol, so an agent outside the app can drive it. The control channel (figcraft --control PORT) is the other way in.",
        ),
        t.text_secondary,
    );
}

// ---------- Diagnostics ----------

/// The bounded AI event log: switches, retention, the recent events and export/clear.
fn diagnostics_tab(app: &mut FigApp, ui: &mut Ui, ran: &mut Vec<(String, serde_json::Value)>, t: &Tokens) {
    section_header(ui, tr("Diagnostics"), t);
    let log = app.session.prefs.ai.diagnostics.clone();
    if widgets::checkbox(ui, log.enabled, tr("Record events")) {
        ran.push(("ai.diagnostics.settings".into(), json!({"enabled": !log.enabled})));
    }
    widgets::label(ui, tr("Events are kept locally; no keys or message content are recorded."), t.text_secondary);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(tr("Keep"));
        for n in figcraft_ai::diagnostics::RETENTION {
            let label = format!("{n}");
            let active = log.retention == n;
            if widgets::button(ui, &label, active, true).clicked() {
                ran.push(("ai.diagnostics.settings".into(), json!({"retention": n})));
            }
        }
    });
    ui.add_space(8.0);
    section_header(ui, tr("Recent events"), t);
    for event in log.recent(20) {
        let colour = match event.level {
            figcraft_ai::DiagnosticLevel::Error => t.danger,
            figcraft_ai::DiagnosticLevel::Warning => t.warning,
            _ => t.text_secondary,
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{:?}", event.level)).color(colour).size(11.0));
            ui.label(egui::RichText::new(&event.name).color(t.text).size(11.0));
        });
        if !event.attributes.is_empty() {
            let detail = event.attributes.iter().take(4).map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("  ");
            ui.label(egui::RichText::new(detail).color(t.text_tertiary).size(10.0));
        }
    }
    if log.events.is_empty() {
        widgets::label(ui, tr("No events yet."), t.text_tertiary);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if widgets::button(ui, tr("Copy"), false, !log.events.is_empty()).clicked() {
            app.services.clipboard_set_text(&log.export());
        }
        if widgets::button(ui, tr("Clear"), false, !log.events.is_empty()).clicked() {
            app.run("ai.diagnostics.clear", json!({}));
        }
    });
}

// ---------- Account ----------

/// Take the account call's answer, if it has landed.
fn drain_account_job(app: &mut FigApp, editor: &mut Editor) {
    let Some(job) = &app.ui.account_job else { return };
    let reply = job.rx.try_recv();
    if reply.is_ok() {
        app.ui.account_job = None;
    }
    let Ok(reply) = reply else { return };
    match reply {
        Ok(AccountReply::Token(token)) => {
            editor.account_password.clear();
            editor.account_results.push(("ok".into(), tr("Signed in.").to_string()));
            app.run("ai.account.token.set", json!({"value": token}));
        }
        Ok(AccountReply::Registered) => {
            editor.account_results.push(("ok".into(), tr("The account was made. Sign in now.").to_string()));
        }
        Ok(AccountReply::LoggedOut) => {
            editor.account_results.push(("ok".into(), tr("Signed out.").to_string()));
            app.run("ai.account.token.clear", json!({}));
        }
        Ok(AccountReply::Documents(docs)) => {
            app.run("ai.cloud.documents.set", json!({"documents": docs.iter().map(|d| json!({"id": d.id, "name": d.name})).collect::<Vec<_>>()}));
        }
        Ok(AccountReply::Opened(name, bytes)) => {
            let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
            let file_name = format!("{name}.figcraft");
            let result = app.session.execute("file.open", &json!({"data": data, "name": file_name}));
            match result {
                Ok(_) => editor.account_results.push(("ok".into(), tr("The cloud document is open.").to_string())),
                Err(e) => editor.account_results.push(("error".into(), e.to_string())),
            }
        }
        Ok(AccountReply::Deleted) => {
            editor.account_results.push(("ok".into(), tr("The cloud document was deleted.").to_string()));
        }
        Err(e) => editor.account_results.push(("error".into(), e)),
    }
}

/// The cloud account: the server, the sign-in and the documents it keeps.
///
/// The calls run on a worker thread (`AccountJob`), because a server answer takes as long as the
/// network; what came back lands through a channel the window drains.
fn account_tab(app: &mut FigApp, ui: &mut Ui, editor: &mut Editor, ran: &mut Vec<(String, serde_json::Value)>, t: &Tokens) {
    section_header(ui, tr("Cloud account"), t);
    widgets::label(
        ui,
        tr("Sign in to a figcraft-server's accounts API to keep your documents on it. The server's config turns the API on (accounts_listen)."),
        t.text_secondary,
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(tr("Server"));
        if let Some(v) = widgets::text_field(ui, egui::Id::unique("ai_account_server"), &app.session.prefs.ai.account_server, 220.0, tr("Server")) {
            ran.push(("ai.account.server".into(), serde_json::json!({"url": v.trim()})));
        }
    });

    // What the sync is doing, and the buttons that control it.
    ui.add_space(6.0);
    section_header(ui, tr("Cloud sync"), t);
    let out = &app.session.prefs.ai.sync;
    widgets::label(ui, &crate::cloud_sync::status_text(out), t.text_secondary);
    ui.horizontal(|ui| {
        if widgets::button(ui, tr("Sync now"), false, !app.session.prefs.ai.account_server.is_empty()).clicked()
            && let Some(doc) = app.session.ds().ok().map(|d| d.uid.to_string())
        {
            ran.push(("ai.cloud.sync.enqueue".into(), serde_json::json!({"document": doc})));
            app.ui.sync_worker.wake_now();
        }
        if widgets::button(ui, tr("Retry"), false, out.pending() > 0).clicked() {
            ran.push(("ai.cloud.sync.resume".into(), serde_json::json!({})));
            app.ui.sync_worker.wake_now();
        }
        if widgets::button(ui, tr("Clear queue"), false, out.pending() > 0).clicked() {
            ran.push(("ai.cloud.sync.clear".into(), serde_json::json!({})));
        }
    });

    // The job in progress, and what came back.
    if app.ui.account_job.is_some() {
        widgets::label(ui, tr("Working…"), t.text_tertiary);
        return;
    }
    for (kind, message) in &editor.account_results {
        let colour = match kind.as_str() {
            "error" => t.danger,
            _ => t.success,
        };
        widgets::label(ui, message, colour);
    }

    // Signed in: the session's answer shows, and the token is the store's.
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(tr("Email"));
        if let Some(v) = widgets::text_field(ui, egui::Id::unique("ai_account_email"), &editor.account_email, 180.0, tr("Email")) {
            editor.account_email = v;
        }
        ui.label(tr("Password"));
        let mut typed = editor.account_password.clone();
        widgets::text_input(ui, egui::Id::unique("ai_account_password"), &mut typed, 140.0, tr("Password"));
        editor.account_password = typed;
    });
    ui.horizontal(|ui| {
        ui.label(tr("Name"));
        if let Some(v) = widgets::text_field(ui, egui::Id::unique("ai_account_name"), &editor.account_name, 140.0, tr("Name")) {
            editor.account_name = v;
        }
        let ready =
            !editor.account_email.trim().is_empty() && !editor.account_password.is_empty() && !app.session.prefs.ai.account_server.trim().is_empty();
        if widgets::button(ui, tr("Sign in"), false, ready).clicked() {
            start_account_job(app, editor, AccountJobKind::Login);
        }
        if widgets::button(ui, tr("Register"), false, ready).clicked() {
            start_account_job(app, editor, AccountJobKind::Register);
        }
        if widgets::button(ui, tr("Sign out"), false, false).clicked() {
            start_account_job(app, editor, AccountJobKind::Logout);
        }
    });

    // The account's documents.
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        section_header(ui, tr("Cloud documents"), t);
        if widgets::button(ui, tr("Refresh"), false, !app.session.prefs.ai.account_server.is_empty()).clicked() {
            start_account_job(app, editor, AccountJobKind::List);
        }
    });
    let documents = app.session.prefs.ai.cloud_documents.clone();
    for doc in &documents {
        ui.horizontal(|ui| {
            ui.label(&doc.name);
            if widgets::button(ui, tr("Open"), false, true).clicked() {
                start_account_job(app, editor, AccountJobKind::Open(doc.id.clone()));
            }
            if widgets::button(ui, tr("Delete"), false, false).clicked() {
                start_account_job(app, editor, AccountJobKind::Delete(doc.id.clone()));
            }
        });
    }
}

/// The account call a worker thread is running.
pub enum AccountJobKind {
    Login,
    Register,
    Logout,
    /// List the account's cloud documents.
    List,
    /// Fetch a cloud document's bytes and open them.
    Open(String),
    /// Forget a cloud document.
    Delete(String),
}

/// Start one account call on its worker thread.
fn start_account_job(app: &mut FigApp, editor: &mut Editor, kind: AccountJobKind) {
    let url = app.session.prefs.ai.account_server.trim().to_string();
    let email = editor.account_email.trim().to_string();
    let password = editor.account_password.clone();
    let name = editor.account_name.trim().to_string();
    let factory = app.cloud_factory.clone();
    // The token the session keeps, from the store.
    let token = app
        .session
        .credentials
        .as_ref()
        .and_then(|s| s.read(&figcraft_ai::credentials::CredentialRef::new("cloud", "session", "token")).ok().flatten());
    let (tx, rx) = std::sync::mpsc::channel();
    let thread = std::thread::Builder::new().name("figcraft-account".into()).spawn(move || {
        let mut session = factory.make(url, token);
        let answer: Result<AccountReply, String> = match kind {
            AccountJobKind::Login => session.login(&email, &password).map(AccountReply::Token).map_err(|e| e.message()),
            AccountJobKind::Register => session.register(&email, &password, &name).map(|_| AccountReply::Registered).map_err(|e| e.message()),
            AccountJobKind::Logout => session.logout().map(|_| AccountReply::LoggedOut).map_err(|e| e.message()),
            AccountJobKind::List => session.list().map(AccountReply::Documents).map_err(|e| e.message()),
            AccountJobKind::Open(ref id) => session.get(id).map(|bytes| AccountReply::Opened(id.clone(), bytes)).map_err(|e| e.message()),
            AccountJobKind::Delete(ref id) => session.delete(id).map(|_| AccountReply::Deleted).map_err(|e| e.message()),
        };
        let _ = tx.send(answer);
    });
    if let Ok(thread) = thread {
        app.ui.account_job = Some(AccountJob { rx, _thread: Some(thread) });
    }
}

/// The account call's answer.
pub enum AccountReply {
    /// A session token, to keep in the store.
    Token(String),
    /// The account was made.
    Registered,
    /// The session ended.
    LoggedOut,
    /// The account's documents.
    Documents(Vec<figcraft_ai::CloudDocument>),
    /// A document's bytes, from the cloud.
    Opened(String, Vec<u8>),
    /// A document was forgotten.
    Deleted,
}

// ---------- Skills ----------

/// The standing instructions, and the commands the semantic skills answer to.
///
/// `true` when a skill's text was edited, so the caller knows the settings changed.
fn skills_tab(app: &mut FigApp, ui: &mut Ui, editor: &mut Editor, ran: &mut Vec<(String, serde_json::Value)>, t: &Tokens) -> bool {
    let mut edited = false;
    section_header(ui, tr("Standing instructions"), t);
    widgets::label(ui, tr("Text that is added to the model's instructions for every request while it is switched on."), t.text_secondary);
    ui.add_space(4.0);

    let store = app.session.prefs.ai.skills.clone();
    for skill in store.all() {
        ui.horizontal(|ui| {
            if widgets::checkbox(ui, skill.enabled, &skill.display_name) {
                ran.push(("ai.skill.toggle".into(), serde_json::json!({"id": skill.id, "enabled": !skill.enabled})));
            }
            if !skill.description.is_empty() {
                ui.label(text_secondary(t, &skill.description));
            }
            if !skill.is_default && widgets::button(ui, tr("Remove"), false, false).clicked() {
                ran.push(("ai.skill.remove".into(), serde_json::json!({"id": skill.id})));
            }
        });
    }

    // A new one, written here.
    ui.add_space(8.0);
    let Some(mut draft) = editor.skill_draft.take() else {
        if widgets::button(ui, tr("Add an instruction"), false, true).clicked() {
            editor.skill_draft = Some(SkillDraft::default());
        }
        return edited;
    };
    ui.add_space(4.0);
    let mut cancelled = false;
    let mut saved = false;
    ui.horizontal(|ui| {
        ui.label(tr("Name"));
        if widgets::text_field(ui, egui::Id::unique("ai_skill_name"), &draft.name, 140.0, tr("Name")).map(|v| draft.name = v).is_some() {
            edited = true;
            return;
        }
        ui.label(tr("Description"));
        if widgets::text_field(ui, egui::Id::unique("ai_skill_desc"), &draft.description, 140.0, tr("Description"))
            .map(|v| draft.description = v)
            .is_some()
        {
            edited = true;
        }
    });
    ui.add_space(4.0);
    let mut prompt = draft.prompt.clone();
    ui.add_sized([ui.available_width(), 120.0], TextEdit::multiline(&mut prompt).hint_text(tr("What the model should do every request…")));
    if prompt != draft.prompt {
        draft.prompt = prompt;
        edited = true;
    }
    ui.horizontal(|ui| {
        let ready = !draft.name.trim().is_empty() && !draft.prompt.trim().is_empty();
        if widgets::button(ui, tr("Save"), false, ready).clicked() {
            ran.push((
                "ai.skill.add".into(),
                serde_json::json!({"name": draft.name.trim(), "description": draft.description.trim(), "promptContent": draft.prompt.trim()}),
            ));
            saved = true;
            edited = true;
        }
        if widgets::button(ui, tr("Cancel"), false, true).clicked() {
            cancelled = true;
        }
    });
    if !saved && !cancelled {
        editor.skill_draft = Some(draft);
    }

    ui.add_space(10.0);
    section_header(ui, tr("Skill commands"), t);
    widgets::label(
        ui,
        tr("Typing /command in a chat message activates the skill's knowledge for that request. Plain language activates one too, by its triggers."),
        t.text_secondary,
    );
    let router = figcraft_ai::skills::SkillRouter::default();
    for command in router.commands() {
        ui.horizontal(|ui| {
            ui.label(format!("/{}", command.command));
            ui.label(text_secondary(t, &command.description));
        });
    }
    edited
}

// ---------- Usage ----------

fn usage_tab(app: &mut FigApp, ui: &mut Ui, t: &Tokens) {
    let summary = app.session.ai.summary();
    if summary.requests == 0 {
        widgets::label(ui, tr("Nothing yet. The models' use shows here once a request has run."), t.text_secondary);
        return;
    }
    let row = |ui: &mut Ui, label: &str, value: String| {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(value);
            });
        });
    };
    row(ui, tr("Requests"), summary.requests.to_string());
    row(ui, tr("Completed"), summary.completed_requests.to_string());
    row(ui, tr("Failed"), summary.failed_requests.to_string());
    if let Some(tokens) = summary.input_tokens {
        row(ui, tr("Input tokens"), thousands(tokens));
    }
    if let Some(tokens) = summary.output_tokens {
        row(ui, tr("Output tokens"), thousands(tokens));
    }
    if let Some(tokens) = summary.cache_read_tokens.filter(|t| *t > 0) {
        row(ui, tr("Cache read"), thousands(tokens));
    }
    if let Some(tokens) = summary.cache_write_tokens.filter(|t| *t > 0) {
        row(ui, tr("Cache write"), thousands(tokens));
    }

    ui.add_space(10.0);
    section_header(ui, tr("By model"), t);
    for model in &summary.models {
        ui.horizontal(|ui| {
            ui.label(format!("{} · {}", model.provider, model.model));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(trf("{n} requests", &[("n", &model.requests)]));
            });
        });
    }
    if summary.unavailable_cache_telemetry > 0 {
        ui.add_space(6.0);
        widgets::label(ui, &trf("{n} steps came back without cache telemetry.", &[("n", &summary.unavailable_cache_telemetry)]), t.text_tertiary);
    }
}

/// A number with thousands separators, so a token count reads at a glance.
fn thousands(n: u64) -> String {
    let text = n.to_string();
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + text.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

/// A section header with a rule under it.
fn section_header(ui: &mut Ui, title: &str, t: &Tokens) {
    ui.add_space(2.0);
    ui.label(egui::RichText::new(title).strong());
    ui.add_space(2.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().line_segment([rect.left_top(), rect.right_top()], egui::Stroke::new(1.0, t.border));
    ui.add_space(4.0);
}

/// Secondary text.
fn text_secondary(t: &Tokens, s: &str) -> egui::RichText {
    egui::RichText::new(s.to_string()).color(t.text_secondary).size(11.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_separates_every_third_digit() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
    }

    #[test]
    fn the_tabs_are_in_a_fixed_order() {
        assert_eq!(
            Tab::all(),
            &[
                Tab::General,
                Tab::Models,
                Tab::Skills,
                Tab::Media,
                Tab::Plugins,
                Tab::Storage,
                Tab::Automation,
                Tab::Account,
                Tab::Usage,
                Tab::Diagnostics
            ]
        );
    }

    #[test]
    fn secondary_text_is_the_tokens_secondary_colour() {
        let t = Tokens::light();
        assert_ne!(t.text_secondary, t.text);
    }
}
