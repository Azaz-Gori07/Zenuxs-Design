//! The AI & Agents window: the models, the keys, the standing instructions and the usage.
//!
//! The window reads the settings through the [`ai.*`](figcraft_engine::cmd::ai) commands and
//! acts through them too, so what a person changes here is what an agent changes over the
//! control channel.
//!
//! A key is typed into the connection's own field and goes to the credential store at once; the
//! field never holds one afterwards, and the status is what the settings show.

use egui::{Align2, CornerRadius, Sense, TextEdit, Ui, vec2};
use serde_json::json;

use figcraft_ai::providers;

use crate::FigApp;
use crate::i18n::{tr, tr_noop, trf};
use crate::theme::{Tokens, regular};
use crate::widgets;

#[allow(dead_code)]
const UNUSED_AI_CATALOG_KEYS: &[&str] = &[
    tr_noop("Add a connection with its API key, choose a model, then assign it to the Design role. A Vision model verifies sourced images before they are placed."),
    tr_noop("Add model"),
    tr_noop("Assignments"),
    tr_noop("Base URL"),
    tr_noop("Choose a connection…"),
    tr_noop("Connections"),
    tr_noop("Describe what you want to build…"),
    tr_noop("Design agent"),
    tr_noop("Fast tasks"),
    tr_noop("Key saved"),
    tr_noop("Model ID"),
    tr_noop("Needs key"),
    tr_noop("New model"),
    tr_noop("No key store"),
    tr_noop("No models yet"),
    tr_noop("Review"),
    tr_noop("The model that answers"),
    tr_noop("The model that answers: AI & Agents"),
    tr_noop("Vision"),
];

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
    /// Zenuxs Design style quick model configuration fields.
    pub quick_name: String,
    pub quick_provider: String,
    pub quick_url: String,
    pub quick_model: String,
    pub quick_key: String,
    pub quick_tools: bool,
    pub quick_vision: bool,
    /// A GitHub URL being entered to install a skill.
    pub github_skill_url: String,
    /// Status of the skill installation: (kind, message).
    pub skill_install_status: Option<(String, String)>,
    /// An in-progress skill install job.
    pub skill_install_job: Option<SkillInstallJob>,
}

/// An in-progress skill installation from a GitHub repository or URL.
pub struct SkillInstallJob {
    pub candidates: Vec<String>,
    pub current: usize,
    pub future: Option<crate::services::TextFetchFuture>,
    pub original_url: String,
}

impl PartialEq for SkillInstallJob {
    fn eq(&self, other: &Self) -> bool {
        self.candidates == other.candidates && self.current == other.current && self.original_url == other.original_url
    }
}

impl std::fmt::Debug for SkillInstallJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkillInstallJob")
            .field("candidates", &self.candidates)
            .field("current", &self.current)
            .field("original_url", &self.original_url)
            .finish()
    }
}

impl Clone for SkillInstallJob {
    fn clone(&self) -> Self {
        Self {
            candidates: self.candidates.clone(),
            current: self.current,
            future: None,
            original_url: self.original_url.clone(),
        }
    }
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
        .default_width(500.0)
        .default_height(560.0)
        .min_width(420.0)
        .min_height(350.0)
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.set_width(ui.available_width().max(480.0));
            drain_storage_job(app, &mut editor);
            drain_account_job(app, &mut editor);
            // The tabs row with horizontal scroll support.
            egui::ScrollArea::horizontal()
                .id_salt("ai_settings_tabs_scroll")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let min_w = 640.0f32.max(ui.available_width());
                    ui.set_width(min_w);
                    let tabs = Tab::all().iter().map(|t| t.label()).collect::<Vec<_>>();
                    if let Some(i) = widgets::segmented_text(ui, &tabs, editor.tab_index()) {
                        editor.set_tab(Tab::all()[i]);
                    }
                });
            ui.add_space(8.0);

            // Tab contents wrapped in vertical ScrollArea so users can scroll smoothly through all settings
            egui::ScrollArea::vertical()
                .id_salt("ai_settings_content_scroll")
                .auto_shrink([false, true])
                .show(ui, |ui| {
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
        "Configure an AI model using Quick Model Setup. All design, review, fast, and vision tasks are handled by the active model.",
        t.text_secondary,
    );
}

// ---------- Models ----------

fn models_tab(app: &mut FigApp, ui: &mut Ui, editor: &mut Editor, ran: &mut Vec<(String, serde_json::Value)>, t: &Tokens) {
    let settings = app.session.prefs.ai.models.clone();

    // 1. Quick Model Setup Section
    section_header(ui, "Quick Model Setup", t);
    widgets::label(
        ui,
        "Configure and activate an AI model in one click. All tasks (design, review, fast, and vision) are handled by the active model.",
        t.text_secondary,
    );
    ui.add_space(6.0);

    egui::Frame::NONE
        .fill(t.field)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .stroke(egui::Stroke::new(1.0, t.border))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // 1. Model Name
            if editor.quick_name.is_empty() {
                editor.quick_name = "Nemotron Free".to_string();
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Model Name:").font(regular(11.0)).color(t.text_secondary));
                ui.add_space(8.0);
                let _ = widgets::text_field(ui, egui::Id::unique("ai_quick_name"), &editor.quick_name, 220.0, "e.g. Nemotron Free")
                    .map(|v| editor.quick_name = v);
            });
            ui.add_space(4.0);

            // 2. Provider Preset
            let provider_options = vec![
                "OpenCode Free Bridge (Local)".to_string(),
                "OpenAI".to_string(),
                "Anthropic".to_string(),
                "Google Gemini".to_string(),
                "Ollama".to_string(),
                "OpenAI-Compatible".to_string(),
            ];
            if editor.quick_provider.is_empty() {
                editor.quick_provider = provider_options[0].clone();
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Provider:").font(regular(11.0)).color(t.text_secondary));
                ui.add_space(26.0);
                if let Some(p) = widgets::combo_field(ui, egui::Id::unique("ai_quick_provider"), &editor.quick_provider, 220.0, &provider_options) {
                    editor.quick_provider = p.clone();
                    if p.starts_with("OpenCode") {
                        editor.quick_url = "http://127.0.0.1:20129/v1".to_string();
                        editor.quick_model = "nemotron-3.5-lightning-free".to_string();
                        editor.quick_key = "public".to_string();
                    } else if p == "Ollama" {
                        editor.quick_url = "http://localhost:11434/v1".to_string();
                        editor.quick_model = "llama3".to_string();
                        editor.quick_key = "ollama".to_string();
                    } else if p == "OpenAI" {
                        editor.quick_url = "https://api.openai.com/v1".to_string();
                        editor.quick_model = "gpt-4o".to_string();
                        editor.quick_key.clear();
                    } else if p == "Anthropic" {
                        editor.quick_url = "https://api.anthropic.com".to_string();
                        editor.quick_model = "claude-3-7-sonnet".to_string();
                        editor.quick_key.clear();
                    } else if p.contains("Gemini") {
                        editor.quick_url = "https://generativelanguage.googleapis.com".to_string();
                        editor.quick_model = "gemini-2.0-flash".to_string();
                        editor.quick_key.clear();
                    }
                }
            });
            ui.add_space(4.0);

            // 3. Base URL
            if editor.quick_url.is_empty() {
                editor.quick_url = "http://127.0.0.1:20129/v1".to_string();
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Base URL:").font(regular(11.0)).color(t.text_secondary));
                ui.add_space(22.0);
                let _ = widgets::text_field(ui, egui::Id::unique("ai_quick_url"), &editor.quick_url, 220.0, "http://127.0.0.1:20129/v1")
                    .map(|v| editor.quick_url = v);
            });
            ui.add_space(4.0);

            // 4. Model ID
            if editor.quick_model.is_empty() {
                editor.quick_model = "nemotron-3.5-lightning-free".to_string();
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Model ID:").font(regular(11.0)).color(t.text_secondary));
                ui.add_space(22.0);
                let _ = widgets::text_field(ui, egui::Id::unique("ai_quick_model"), &editor.quick_model, 220.0, "nemotron-3.5-lightning-free")
                    .map(|v| editor.quick_model = v);
            });
            ui.add_space(4.0);

            // 5. API Key
            if editor.quick_key.is_empty() && editor.quick_provider.starts_with("OpenCode") {
                editor.quick_key = "public".to_string();
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("API Key:").font(regular(11.0)).color(t.text_secondary));
                ui.add_space(29.0);
                let _ = widgets::text_field(ui, egui::Id::unique("ai_quick_key"), &editor.quick_key, 220.0, "API key (or 'public' for bridge)")
                    .map(|v| editor.quick_key = v);
            });
            ui.add_space(6.0);

            // 6. Connect & Add Model button
            if widgets::button(ui, "Connect & Set as Active Model", true, true).clicked() {
                let prov_id = if editor.quick_provider == "OpenAI" {
                    "openai"
                } else if editor.quick_provider == "Anthropic" {
                    "anthropic"
                } else if editor.quick_provider.contains("Gemini") {
                    "google"
                } else if editor.quick_provider == "Ollama" {
                    "ollama"
                } else {
                    "openai-compatible"
                };

                let conn_id = settings
                    .connections
                    .iter()
                    .find(|c| c.provider.id() == prov_id && (c.custom_base_url.is_empty() || c.custom_base_url == editor.quick_url.trim()))
                    .map(|c| c.id.clone())
                    .unwrap_or_else(|| {
                        let new_id = format!("connection-{}", settings.connections.len() + 1);
                        let mut params = serde_json::json!({
                            "provider": prov_id,
                        });
                        if !editor.quick_url.trim().is_empty() {
                            params["customBaseURL"] = serde_json::json!(editor.quick_url.trim());
                        }
                        ran.push(("ai.connection.add".into(), params));
                        new_id
                    });

                let model_name = if editor.quick_name.trim().is_empty() { "Active Model".to_string() } else { editor.quick_name.trim().to_string() };
                let model_id_val = if editor.quick_model.trim().is_empty() {
                    "nemotron-3.5-lightning-free".to_string()
                } else {
                    editor.quick_model.trim().to_string()
                };

                ran.push((
                    "ai.model.add".into(),
                    serde_json::json!({
                        "name": model_name,
                        "connectionId": conn_id.clone(),
                        "modelID": model_id_val,
                        "capabilities": ["tools", "vision"],
                        "setAsActive": true,
                    }),
                ));

                if !editor.quick_key.trim().is_empty() {
                    let key_val = editor.quick_key.trim().to_string();
                    ran.push((
                        "ai.credential.set".into(),
                        serde_json::json!({
                            "integration": prov_id,
                            "profileId": conn_id,
                            "field": "api-key",
                            "value": key_val,
                        }),
                    ));
                }

                app.toast("Model configured and activated successfully!");
            }
        });

    ui.add_space(14.0);
    ui.separator();
    ui.add_space(10.0);

    // 2. Configured Models Section
    section_header(ui, "Configured Models", t);
    let active_id = settings.assignments.design.clone();

    if settings.models.is_empty() {
        egui::Frame::NONE.fill(t.field).corner_radius(egui::CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
            ui.label(
                egui::RichText::new("No models configured yet. Use Quick Model Setup above to add your model.")
                    .font(regular(11.5))
                    .color(t.text_secondary),
            );
        });
    } else {
        for model in &settings.models {
            let is_active = model.id == active_id;
            let conn_url = settings
                .connections
                .iter()
                .find(|c| c.id == model.connection_id)
                .map(|c| {
                    if c.custom_base_url.is_empty() {
                        providers::provider(c.provider).map(|p| p.name).unwrap_or(c.provider.id()).to_string()
                    } else {
                        c.custom_base_url.clone()
                    }
                })
                .unwrap_or_else(|| model.connection_id.clone());

            egui::Frame::NONE
                .fill(if is_active { t.selected_row } else { t.field })
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .stroke(egui::Stroke::new(1.0, if is_active { t.accent_on } else { t.border }))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        // Left: Name & info
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&model.name).font(crate::theme::semibold(12.0)).color(t.text));
                                if is_active {
                                    ui.add_space(6.0);
                                    let badge_r = ui.allocate_response(vec2(48.0, 18.0), Sense::hover());
                                    ui.painter().rect_filled(badge_r.rect, CornerRadius::same(9), t.accent_on);
                                    ui.painter().text(badge_r.rect.center(), Align2::CENTER_CENTER, "Active", regular(10.0), t.accent_text);
                                }
                            });
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(format!("ID: {}  •  {}", model.model_id, conn_url)).font(regular(10.5)).color(t.text_secondary),
                            );
                        });

                        // Right: Actions (Set Active, Delete)
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::button(ui, tr("Delete"), false, false).clicked() {
                                ran.push(("ai.model.remove".into(), serde_json::json!({"id": model.id})));
                            }
                            if !is_active && widgets::button(ui, "Set as Active", false, true).clicked() {
                                ran.push(("ai.role.assign".into(), serde_json::json!({"role": "all", "modelId": model.id})));
                            }
                        });
                    });
                });
            ui.add_space(6.0);
        }
    }
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
    let names = [tr("sycnUI server").to_string(), tr("S3 storage").to_string()];
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
    if let Some(mut draft) = editor.skill_draft.take() {
        ui.add_space(4.0);
        let mut cancelled = false;
        let mut saved = false;
        ui.horizontal(|ui| {
            ui.label(tr("Name"));
            if widgets::text_field(ui, egui::Id::unique("ai_skill_name"), &draft.name, 140.0, tr("Name")).map(|v| draft.name = v).is_some() {
                edited = true;
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
    } else if widgets::button(ui, tr("Add an instruction"), false, true).clicked() {
        editor.skill_draft = Some(SkillDraft::default());
    }

    // Install from GitHub section
    ui.add_space(10.0);
    section_header(ui, "Install Skill from GitHub", t);
    widgets::label(ui, "Enter a GitHub repository or SKILL.md URL to install as a standing instruction:", t.text_secondary);
    ui.add_space(4.0);

    let busy = editor.skill_install_job.is_some();
    ui.horizontal(|ui| {
        let mut typed = editor.github_skill_url.clone();
        widgets::text_input(
            ui,
            egui::Id::unique("ai_github_skill_url"),
            &mut typed,
            320.0,
            "https://github.com/owner/repo",
        );
        editor.github_skill_url = typed;

        let ready = !editor.github_skill_url.trim().is_empty() && !busy;
        if widgets::button(ui, if busy { "Installing…" } else { "Install Skill" }, false, ready).clicked() {
            let url = editor.github_skill_url.trim().to_string();
            let candidates = figcraft_ai::skills::prompt::github_skill_candidate_urls(&url);
            if candidates.is_empty() {
                editor.skill_install_status = Some(("error".into(), "Please enter a valid GitHub URL.".to_string()));
            } else {
                let fut = app.services.fetch_url(&candidates[0]);
                editor.skill_install_job = Some(SkillInstallJob {
                    candidates,
                    current: 0,
                    future: fut,
                    original_url: url,
                });
                editor.skill_install_status = Some(("info".into(), "Fetching skill from GitHub…".to_string()));
            }
        }
    });

    // Poll active skill installation job
    if let Some(job) = &mut editor.skill_install_job {
        let waker = std::task::Waker::from(std::sync::Arc::new(crate::services::RepaintWaker(ui.ctx().clone())));
        let mut cx = std::task::Context::from_waker(&waker);
        let mut finished = None;
        if let Some(fut) = &mut job.future {
            if let std::task::Poll::Ready(res) = fut.as_mut().poll(&mut cx) {
                match res {
                    Ok(markdown) => {
                        finished = Some(Ok((job.original_url.clone(), markdown)));
                    }
                    Err(_) => {
                        job.current += 1;
                        if job.current < job.candidates.len() {
                            job.future = app.services.fetch_url(&job.candidates[job.current]);
                        } else {
                            finished = Some(Err("Could not find SKILL.md or README.md at this repository.".to_string()));
                        }
                    }
                }
            }
        } else {
            finished = Some(Err("Network fetch is not available in this environment.".to_string()));
        }

        if let Some(res) = finished {
            editor.skill_install_job = None;
            match res {
                Ok((url, markdown)) => {
                    let (name, _, _) = figcraft_ai::skills::prompt::parse_skill_markdown(&markdown, "custom-skill");
                    ran.push((
                        "ai.skill.installMarkdown".into(),
                        serde_json::json!({
                            "name": name,
                            "markdown": markdown,
                            "url": url,
                        }),
                    ));
                    app.toast(format!("Installed skill \"{name}\" from GitHub!"));
                    editor.github_skill_url.clear();
                    editor.skill_install_status = Some(("ok".into(), format!("Installed \"{name}\" successfully!")));
                    edited = true;
                }
                Err(err) => {
                    editor.skill_install_status = Some(("error".into(), err));
                }
            }
        }
    }

    if let Some((kind, msg)) = &editor.skill_install_status {
        let colour = match kind.as_str() {
            "error" => t.danger,
            "ok" => t.success,
            _ => t.accent,
        };
        ui.add_space(2.0);
        widgets::label(ui, msg, colour);
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
