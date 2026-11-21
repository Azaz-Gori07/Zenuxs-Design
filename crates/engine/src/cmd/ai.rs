//! The AI settings commands: the models, the keys, the skills and the usage.
//!
//! Everything the settings can be is a command here, so the interface, the CLI, the control
//! channel and the MCP server all reach the same settings the same way, and an agent can
//! configure a model the way a person does.
//!
//! A key is never part of the settings: `ai.credential.set` writes it to the credential store
//! the platform installed, and the command answers with the status, never with the key.

use figcraft_ai::credentials::{CredentialRef, CredentialStatus};
use figcraft_ai::models::{AIModelCapability, AIModelConnection, AIModelProfile, AIModelRole, APIType};
use figcraft_ai::providers::{self, ProviderId};
use figcraft_ai::skills::prompt::PromptSkill;
use serde_json::{Value, json};

use super::{CommandSpec, ok};
use crate::ai::{AIState, connection_credential_ref};
use crate::params::{boolean, get, req_string, string};
use crate::{Result, Session, bad, spec};

/// The most models and connections the settings keep.
const MAX_MODELS: usize = 50;
const MAX_CONNECTIONS: usize = 50;
/// The most standing instructions a person may write.
const MAX_PROMPT_SKILLS: usize = 50;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!("ai.settings", "AI & Agents…", "{}  opens the AI settings window", false, |s, _| {
            s.request_ui("dialog.aiSettings", serde_json::json!({}));
            ok()
        }),
        spec!(
            "ai.providers",
            "List AI providers",
            "{} → {providers: [{id, name, keyPlaceholder, keyURL, defaultModel, models: [{id, name, tag, capabilities}]}]}",
            false,
            providers_list
        ),
        spec!(
            "ai.models",
            "List AI models",
            "{} → {connections: [{id, provider, customBaseURL, customAPIType}], models: [{id, name, connectionId, modelID, customModelID, maxOutputTokens, capabilities}], assignments: {design, review, fast, vision}}",
            false,
            models_list
        ),
        spec!(
            "ai.model.add",
            "Add AI model",
            "{name, connectionId, modelID, customModelID?, maxOutputTokens?, capabilities?: [tools|vision]} → {id}",
            false,
            model_add
        ),
        spec!(
            "ai.model.update",
            "Update AI model",
            "{id, name?, connectionId?, modelID?, customModelID?, maxOutputTokens?, capabilities?: [tools|vision]}",
            false,
            model_update
        ),
        spec!("ai.model.remove", "Remove AI model", "{id}", false, model_remove),
        spec!(
            "ai.connection.add",
            "Add AI connection",
            "{provider, customBaseURL?, customAPIType?: completions|responses} → {id}",
            false,
            connection_add
        ),
        spec!("ai.connection.remove", "Remove AI connection", "{id}", false, connection_remove),
        spec!(
            "ai.role.assign",
            "Assign a model to a role",
            "{role: design|review|fast|vision, modelId} (an empty modelId clears the role)",
            false,
            role_assign
        ),
        spec!(
            "ai.credential.set",
            "Set an AI credential",
            "{integration, profileId, field, value} → {status} (the key itself is never answered)",
            false,
            credential_set
        ),
        spec!("ai.credential.clear", "Clear an AI credential", "{integration, profileId, field} → {status}", false, credential_clear),
        spec!("ai.credential.status", "Check an AI credential", "{integration, profileId, field} → {status}", false, credential_status),
        spec!(
            "ai.skills",
            "List AI skills",
            "{} → {skills: [{id, name, displayName, description, version, tags, isDefault, enabled}], commands: [{command, description}]}",
            false,
            skills_list
        ),
        spec!(
            "ai.skill.toggle",
            "Switch an AI skill on or off",
            "{id, enabled?} (without `enabled` the skill is switched the other way) → {id, enabled}",
            false,
            skill_toggle
        ),
        spec!("ai.skill.add", "Add a standing instruction", "{name, promptContent, description?, tags?} → {id}", false, skill_add),
        spec!("ai.skill.remove", "Remove a standing instruction", "{id}", false, skill_remove),
        spec!(
            "ai.chat.send",
            "Send an AI chat message",
            "{text, role?: user (default)}  the interface sends the last queued message on its worker thread; `ai.chat.status` says when it is out",
            false,
            |s, p| {
                let cmd = "ai.chat.send";
                let text = req_string(cmd, p, "text")?;
                s.chat_append(crate::chat::message_of(cmd, &json!({"role": "user", "content": text}))?)?;
                s.chat.pending_send = true;
                s.request_ui("panel.ai", json!({}));
                ok()
            }
        ),
        spec!("ai.storage.settings", "Cloud storage settings", "{} → {provider, endpoint?, bucket?, region?, configured}", false, |s, _| Ok(
            json!({
                "provider": s.prefs.ai.storage_provider.id(),
                "endpoint": s.prefs.ai.s3.endpoint,
                "bucket": s.prefs.ai.s3.bucket,
                "region": s.prefs.ai.s3.region,
                "configured": match s.prefs.ai.storage_provider {
                    figcraft_ai::StorageProvider::S3Compatible => s.prefs.ai.s3.complete()
                        && matches!(s.credentials.as_ref().and_then(|st| st.status(&figcraft_ai::CredentialRef::new("s3", "default", "access-key-id")).ok()), Some(figcraft_ai::CredentialStatus::Configured))
                        && matches!(s.credentials.as_ref().and_then(|st| st.status(&figcraft_ai::CredentialRef::new("s3", "default", "secret-access-key")).ok()), Some(figcraft_ai::CredentialStatus::Configured)),
                    figcraft_ai::StorageProvider::FigcraftServer => matches!(
                        s.credentials.as_ref().and_then(|st| st.status(&figcraft_ai::CredentialRef::new("cloud", "session", "token")).ok()),
                        Some(figcraft_ai::CredentialStatus::Configured)
                    ),
                },
            })
        )),
        spec!("ai.storage.provider", "Set the storage provider", "{provider: figcraft-server|s3-compatible}", false, |s, p| {
            let id = req_string("ai.storage.provider", p, "provider")?;
            s.prefs.ai.storage_provider = figcraft_ai::StorageProvider::from_id(&id);
            s.prefs_unsaved = true;
            Ok(json!({"provider": s.prefs.ai.storage_provider.id()}))
        }),
        spec!("ai.storage.s3.set", "Set the S3 endpoint, bucket and region", "{endpoint?, bucket?, region?}", false, |s, p| {
            if let Some(v) = string(p, "endpoint") {
                s.prefs.ai.s3.endpoint = v.trim().to_string();
            }
            if let Some(v) = string(p, "bucket") {
                s.prefs.ai.s3.bucket = v.trim().to_string();
            }
            if let Some(v) = string(p, "region") {
                s.prefs.ai.s3.region = v.trim().to_string();
            }
            s.prefs_unsaved = true;
            Ok(json!({"endpoint": s.prefs.ai.s3.endpoint, "bucket": s.prefs.ai.s3.bucket, "region": s.prefs.ai.s3.region}))
        }),
        spec!("ai.storage.documents.set", "Keep the cloud document list", "{documents: [{id, name, updatedAt}]}", false, |s, p| {
            let mut out = Vec::new();
            for d in p.get("documents").and_then(Value::as_array).cloned().unwrap_or_default() {
                let id = d.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                let name = d.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                let updated_at = d.get("updatedAt").and_then(Value::as_str).unwrap_or_default().to_string();
                if !id.is_empty() {
                    out.push(figcraft_ai::StorageDocument { id, name, updated_at });
                }
            }
            s.prefs.ai.storage_documents = out;
            s.prefs_unsaved = true;
            Ok(json!({"documents": s.prefs.ai.storage_documents.len()}))
        }),
        spec!("ai.storage.cors", "CORS configuration", "{} → {json}", false, |_, _| {
            Ok(json!({"json": figcraft_ai::storage::cors_json(&["https://*.figcraft.app".into()])}))
        }),
        spec!("ai.media.settings", "Media settings", "{} → {vectorizeProvider, stockProviders: [pexels, unsplash]}", false, |s, _| Ok(json!({
            "vectorizeProvider": s.prefs.ai.vectorize_provider.id(),
            "stockProviders": ["pexels", "unsplash"],
        }))),
        spec!("ai.media.vectorize.provider", "Set the vectorization service", "{provider: recraft|fal}", false, |s, p| {
            let id = req_string("ai.media.vectorize.provider", p, "provider")?;
            s.prefs.ai.vectorize_provider = figcraft_ai::VectorizeProvider::from_id(&id);
            s.prefs_unsaved = true;
            Ok(json!({"provider": s.prefs.ai.vectorize_provider.id()}))
        }),
        spec!("ai.chat.preferences", "Chat preferences", "{} → {reasoningDisplay: collapsed|while-thinking|expanded, showTools}", false, |s, _| Ok(
            json!({
                "reasoningDisplay": s.prefs.ai.reasoning_display.id(),
                "showTools": s.prefs.ai.show_tools,
            })
        )),
        spec!(
            "ai.chat.preferences.set",
            "Set chat preferences",
            "{reasoningDisplay?: collapsed|while-thinking|expanded, showTools?: bool}",
            false,
            |s, p| {
                if let Some(mode) = string(p, "reasoningDisplay") {
                    s.prefs.ai.reasoning_display = figcraft_ai::ReasoningDisplay::from_id(&mode);
                }
                if let Some(value) = boolean(p, "showTools") {
                    s.prefs.ai.show_tools = value;
                }
                s.prefs_unsaved = true;
                Ok(json!({
                    "reasoningDisplay": s.prefs.ai.reasoning_display.id(),
                    "showTools": s.prefs.ai.show_tools,
                }))
            }
        ),
        spec!(
            "ai.diagnostics.settings",
            "Set the AI diagnostics settings",
            "{enabled?: bool, retention?: 100|500|1000} → {enabled, retention}",
            false,
            |s, p| {
                let log = &mut s.prefs.ai.diagnostics;
                if let Some(enabled) = boolean(p, "enabled") {
                    log.enabled = enabled;
                }
                if let Some(value) = p.get("retention").and_then(Value::as_u64) {
                    let value = value as usize;
                    log.retention =
                        if figcraft_ai::diagnostics::RETENTION.contains(&value) { value } else { figcraft_ai::diagnostics::DEFAULT_RETENTION };
                    log.prune();
                }
                s.prefs_unsaved = true;
                Ok(json!({"enabled": log.enabled, "retention": log.retention}))
            }
        ),
        spec!("ai.diagnostics.clear", "Clear the AI event log", "{}", false, |s, _| {
            s.prefs.ai.diagnostics.clear();
            s.prefs_unsaved = true;
            ok()
        }),
        spec!(
            "ai.plugins.catalog",
            "The curated plugin directory",
            "{} → {plugins: [{id, name, version, author, description, category, permissions, runtime}]}",
            false,
            |_, _| {
                let plugins = figcraft_ai::plugin_catalog::catalog()
                    .iter()
                    .map(|p| {
                        json!({
                            "id": p.id,
                            "name": p.name,
                            "version": p.version,
                            "author": p.author,
                            "description": p.description,
                            "category": p.category.label(),
                            "permissions": p.permissions,
                            "runtime": match &p.runtime {
                                figcraft_ai::PluginRuntime::Wasm { module } => json!({"kind": "wasm", "module": module}),
                                figcraft_ai::PluginRuntime::Unavailable { reason } => json!({"kind": "unavailable", "reason": reason}),
                            },
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(json!({"plugins": plugins}))
            }
        ),
        spec!(
            "ai.chat.history",
            "List saved chats",
            "{query? (search titles and message text)} → {active, thisDocument, chats: [{id, title, document?, updatedAt, count}]}",
            false,
            |s, p| {
                let h = &s.prefs.ai.chat_history;
                let doc = s.ds().ok().map(|d| d.uid.to_string());
                let found: Vec<&figcraft_ai::ChatConversation> = match string(p, "query") {
                    Some(ref q) if !q.trim().is_empty() => h.search(q),
                    _ => h.visible(doc.as_deref()),
                };
                Ok(json!({
                    "active": h.active,
                    "thisDocument": h.this_document,
                    "chats": found.iter().map(|c| json!({
                        "id": c.id, "title": c.title, "document": c.document,
                        "updatedAt": c.updated_at, "count": c.messages.len(),
                    })).collect::<Vec<_>>(),
                }))
            }
        ),
        spec!("ai.chat.scope", "Chat conversation scope", "{thisDocument} (true: show this document's chats; false: all)", false, |s, p| {
            let on = boolean(p, "thisDocument").ok_or_else(|| bad("ai.chat.scope", "`thisDocument` must be true or false"))?;
            s.prefs.ai.chat_history.this_document = on;
            s.prefs_unsaved = true;
            Ok(json!({"thisDocument": on}))
        }),
        spec!("ai.chat.new", "New AI chat", "{} → {id} (the current one is saved first)", false, |s, _| {
            let document = s.ds().ok().map(|d| d.uid.to_string());
            let current = s.conversation(document.as_deref());
            let id = s.prefs.ai.chat_history.new_conversation(current.as_ref(), document.as_deref(), crate::ai::now_ms());
            s.chat.clear();
            s.chat.id = id.clone();
            s.chat.document = document;
            s.prefs_unsaved = true;
            Ok(json!({"id": id}))
        }),
        spec!("ai.chat.open", "Open a saved AI chat", "{id} → {id, count}", false, |s, p| {
            let cmd = "ai.chat.open";
            let id = req_string(cmd, p, "id")?;
            let conversation = s.prefs.ai.chat_history.open(&id).ok_or_else(|| bad(cmd, format!("there is no chat \"{id}\"")))?;
            let count = conversation.messages.len();
            s.chat_load(conversation);
            s.prefs_unsaved = true;
            Ok(json!({"id": id, "count": count}))
        }),
        spec!("ai.chat.rename", "Rename a saved AI chat", "{id, title}", false, |s, p| {
            let cmd = "ai.chat.rename";
            let id = req_string(cmd, p, "id")?;
            let title = req_string(cmd, p, "title")?;
            if !s.prefs.ai.chat_history.rename(&id, &title) {
                return Err(bad(cmd, format!("there is no chat \"{id}\"")));
            }
            s.prefs_unsaved = true;
            Ok(json!({"id": id, "title": title}))
        }),
        spec!("ai.chat.delete", "Delete a saved AI chat", "{id}", false, |s, p| {
            let cmd = "ai.chat.delete";
            let id = req_string(cmd, p, "id")?;
            if !s.prefs.ai.chat_history.remove(&id) {
                return Err(bad(cmd, format!("there is no chat \"{id}\"")));
            }
            s.prefs_unsaved = true;
            Ok(json!({"removed": id}))
        }),
        spec!(
            "ai.chat.messages",
            "AI chat conversation",
            "{} → {busy, context, messages: [{role, content, toolCalls?, toolCallId?}]}",
            false,
            |s, _| Ok(crate::chat::chat_json(&s.chat))
        ),
        spec!(
            "ai.chat.append",
            "Add an AI chat message",
            "{message: {role: user|assistant|tool, content, toolCallId? (a tool answer names the call it answers)}}",
            false,
            |s, p| {
                let message = crate::chat::message_of("ai.chat.append", p.get("message").unwrap_or(&Value::Null))?;
                s.chat_append(message)?;
                ok()
            }
        ),
        spec!(
            "ai.chat.context",
            "Set the AI chat's design context",
            "{context} (the selection and page as text, refreshed before each send)",
            false,
            |s, p| {
                s.chat.context = string(p, "context").unwrap_or_default();
                ok()
            }
        ),
        spec!("ai.chat.clear", "Clear the AI chat", "{}", false, |s, _| {
            s.chat.clear();
            ok()
        }),
        spec!(
            "ai.chat.tool",
            "Run a tool for the AI",
            "{tool, arguments} (an engine command by id; dialogs are refused) → the command's answer",
            false,
            |s, p| {
                let tool = req_string("ai.chat.tool", p, "tool")?;
                let arguments = p.get("arguments").cloned().unwrap_or_else(|| serde_json::json!({}));
                crate::chat::run_tool(s, &tool, &arguments)
            }
        ),
        spec!(
            "ai.chat.model",
            "The model a chat role resolves to",
            "{role?: design|review|fast|vision (default design)} → {provider, modelID, baseURL, maxOutputTokens, reasoningEffort?} without the key",
            false,
            chat_model
        ),
        spec!("ai.mcp.connections", "List MCP connections", "{} → {connections: [{id, name, url, enabled, authentication}]}", false, mcp_list),
        spec!("ai.mcp.add", "Add an MCP connection", "{name, url, bearer? (whether the server wants a token), enabled?} → {id}", false, mcp_add),
        spec!("ai.mcp.update", "Update an MCP connection", "{id, name?, url?, bearer?, enabled?}", false, mcp_update),
        spec!("ai.mcp.remove", "Remove an MCP connection", "{id}", false, mcp_remove),
        spec!("ai.mcp.credential.set", "Set an MCP connection's bearer token", "{profileId, value} → {status}", false, mcp_credential_set),
        spec!("ai.account.server", "Set the cloud server", "{url?} (empty clears it) → {url}", false, |s, p| {
            let url = string(p, "url").unwrap_or_default();
            s.prefs.ai.account_server = url.trim().to_string();
            s.prefs_unsaved = true;
            Ok(json!({"url": s.prefs.ai.account_server}))
        }),
        spec!("ai.account.token.set", "Keep the cloud session token", "{value} → {status} (the token itself is never answered)", false, |s, p| {
            let cmd = "ai.account.token.set";
            let value = req_string(cmd, p, "value")?;
            let Some(store) = s.credentials.as_mut() else {
                return Err(bad(cmd, "this build has nowhere to keep a key"));
            };
            let reference = figcraft_ai::credentials::CredentialRef::new("cloud", "session", "token");
            store.write(&reference, &value).map_err(|e| bad(cmd, e.to_string()))?;
            Ok(json!({"status": status_name(crate::ai::credential_status(&**store, &reference))}))
        }),
        spec!("ai.account.token.clear", "Forget the cloud session token", "{} → {status}", false, |s, _| {
            let cmd = "ai.account.token.clear";
            let Some(store) = s.credentials.as_mut() else {
                return Err(bad(cmd, "this build has nowhere to keep a key"));
            };
            let reference = figcraft_ai::credentials::CredentialRef::new("cloud", "session", "token");
            store.remove(&reference).map_err(|e| bad(cmd, e.to_string()))?;
            Ok(json!({"status": status_name(crate::ai::credential_status(&**store, &reference))}))
        }),
        spec!("ai.account.token.status", "Check the cloud session token", "{} → {status}", false, |s, _| {
            let reference = figcraft_ai::credentials::CredentialRef::new("cloud", "session", "token");
            let status =
                s.credentials.as_ref().map(|st| st.status(&reference)).unwrap_or(Ok(figcraft_ai::credentials::CredentialStatus::Unavailable));
            Ok(json!({"status": status_name(status.unwrap_or(figcraft_ai::credentials::CredentialStatus::Unavailable))}))
        }),
        spec!("ai.cloud.sync.enqueue", "Queue a cloud upload", "{document, kind?: put|delete (default put)} → {id}", false, |s, p| {
            let cmd = "ai.cloud.sync.enqueue";
            let document = req_string(cmd, p, "document")?;
            let now = crate::ai::now_ms();
            let jitter = crate::ai::jitter();
            let id = match string(p, "kind").as_deref() {
                None | Some("put") => s.prefs.ai.sync.queue_put(&document, now, jitter),
                Some("delete") => s.prefs.ai.sync.queue_delete(&document, now),
                Some(other) => return Err(bad(cmd, format!("there is no sync kind \"{other}\""))),
            };
            s.prefs_unsaved = true;
            Ok(json!({"id": id}))
        }),
        spec!("ai.cloud.sync.begin", "A cloud job is out", "{id}", false, |s, p| {
            let id = req_string("ai.cloud.sync.begin", p, "id")?;
            s.prefs.ai.sync.begin(&id);
            s.prefs_unsaved = true;
            ok()
        }),
        spec!(
            "ai.cloud.sync.status",
            "Cloud sync status",
            "{} → {state, pending, jobs: [{id, document, kind, attempts, nextAttemptAt, lastError?}], lastError?}",
            false,
            |s, _| {
                let out = &s.prefs.ai.sync;
                Ok(json!({
                    "state": match out.state {
                        figcraft_ai::SyncState::Idle => "idle",
                        figcraft_ai::SyncState::Syncing => "syncing",
                        figcraft_ai::SyncState::Offline => "offline",
                        figcraft_ai::SyncState::Error => "error",
                    },
                    "pending": out.pending(),
                    "jobs": out.jobs.iter().map(|j| json!({
                        "id": j.id, "document": j.document,
                        "kind": match j.kind { figcraft_ai::SyncKind::Put => "put", figcraft_ai::SyncKind::Delete => "delete" },
                        "attempts": j.attempts, "nextAttemptAt": j.next_attempt_at,
                        "lastError": j.last_error,
                    })).collect::<Vec<_>>(),
                    "lastError": out.last_error,
                }))
            }
        ),
        spec!("ai.cloud.sync.done", "A cloud job finished", "{id, ok, error?} (the interface's worker reports back)", false, |s, p| {
            let cmd = "ai.cloud.sync.done";
            let id = req_string(cmd, p, "id")?;
            let ok = boolean(p, "ok").unwrap_or(false);
            let now = crate::ai::now_ms();
            if ok {
                s.prefs.ai.sync.done(&id);
            } else {
                let message = string(p, "error").unwrap_or_else(|| "the upload failed".into());
                let permanent = figcraft_ai::sync::permanent_failure(&message);
                s.prefs.ai.sync.failed(&id, &message, permanent, now, crate::ai::jitter());
            }
            s.prefs_unsaved = true;
            Ok(json!({"id": id}))
        }),
        spec!("ai.cloud.sync.resume", "Retry parked cloud jobs", "{}", false, |s, _| {
            s.prefs.ai.sync.resume(crate::ai::now_ms());
            s.prefs_unsaved = true;
            ok()
        }),
        spec!("ai.cloud.sync.clear", "Clear the cloud sync queue", "{}", false, |s, _| {
            s.prefs.ai.sync.clear();
            s.prefs_unsaved = true;
            ok()
        }),
        spec!(
            "ai.usage",
            "AI usage",
            "{} → {requests, completedRequests, failedRequests, inputTokens, outputTokens, cacheReadTokens, cacheWriteTokens, models: [{provider, model, requests}]}",
            false,
            usage
        ),
        spec!("ai.usage.clear", "Clear AI usage", "{}", false, usage_clear),
    ]
}

/// The provider registry, as the settings show it.
fn providers_list(_s: &mut Session, _p: &Value) -> Result<Value> {
    let list: Vec<Value> = providers::providers()
        .iter()
        .map(|p| {
            json!({
                "id": p.id.id(),
                "name": p.name,
                "keyPlaceholder": p.key_placeholder,
                "keyURL": p.key_url,
                "defaultModel": p.default_model,
                "supportsCustomBaseURL": p.supports_custom_base_url(),
                "supportsCustomModel": p.supports_custom_model(),
                "imageModel": p.id.image_model(),
                "models": p.models.iter().map(|m| json!({
                    "id": m.id,
                    "name": m.name,
                    "tag": m.tag,
                    "capabilities": m.capabilities.iter().map(|c| match c {
                        figcraft_ai::providers::Capability::Tools => "tools".to_string(),
                        figcraft_ai::providers::Capability::Vision => "vision".to_string(),
                    }).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({"providers": list}))
}

/// The configured models, connections and role assignments.
fn models_list(s: &mut Session, _p: &Value) -> Result<Value> {
    let settings = &s.prefs.ai.models;
    let connections: Vec<Value> = settings.connections.iter().map(connection_json).collect();
    let models: Vec<Value> = settings.models.iter().map(model_json).collect();
    Ok(json!({
        "connections": connections,
        "models": models,
        "assignments": {
            "design": settings.assignments.design,
            "review": settings.assignments.review,
            "fast": settings.assignments.fast,
            "vision": settings.assignments.vision,
        }
    }))
}

/// A connection as the settings show it.
fn connection_json(c: &AIModelConnection) -> Value {
    json!({
        "id": c.id,
        "provider": c.provider.id(),
        "customBaseURL": c.custom_base_url,
        "customAPIType": c.custom_api_type.id(),
    })
}

/// A model profile as the settings show it.
fn model_json(m: &AIModelProfile) -> Value {
    json!({
        "id": m.id,
        "name": m.name,
        "connectionId": m.connection_id,
        "modelID": m.model_id,
        "customModelID": m.custom_model_id,
        "maxOutputTokens": m.max_output_tokens,
        "reasoningEffort": m.reasoning_effort,
        "capabilities": m.capabilities.iter().map(capability_id).collect::<Vec<_>>(),
    })
}

/// A capability's id.
fn capability_id(c: &AIModelCapability) -> &'static str {
    match c {
        AIModelCapability::Tools => "tools",
        AIModelCapability::Vision => "vision",
    }
}

/// The capabilities a list names.
fn capabilities(cmd: &str, p: &Value, key: &str) -> Result<Vec<AIModelCapability>> {
    let Some(list) = get(p, key).and_then(Value::as_array) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for value in list {
        let id = value.as_str().ok_or_else(|| bad(cmd, format!("`{key}` must hold capability names")))?;
        out.push(match id {
            "tools" => AIModelCapability::Tools,
            "vision" => AIModelCapability::Vision,
            other => return Err(bad(cmd, format!("`{key}` names no capability called \"{other}\""))),
        });
    }
    Ok(out)
}

/// Add a model profile.
fn model_add(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.model.add";
    let name = req_string(cmd, p, "name")?;
    let connection_id = req_string(cmd, p, "connectionId")?;
    let model_id = string(p, "modelID").unwrap_or_default();
    let custom_model_id = string(p, "customModelID").unwrap_or_default();
    if model_id.trim().is_empty() && custom_model_id.trim().is_empty() {
        return Err(bad(cmd, "a model needs a modelID"));
    }
    if !s.prefs.ai.models.connections.iter().any(|c| c.id == connection_id) {
        return Err(bad(cmd, format!("there is no connection \"{connection_id}\"")));
    }
    if s.prefs.ai.models.models.len() >= MAX_MODELS {
        return Err(bad(cmd, format!("there are already {MAX_MODELS} models")));
    }
    let max_output_tokens = p.get("maxOutputTokens").and_then(Value::as_u64).unwrap_or(8192).clamp(1, 1_000_000) as u32;
    let id = unique_id("model-", &s.prefs.ai.models.models.iter().map(|m| m.id.clone()).collect::<Vec<_>>());
    s.prefs.ai.models.models.push(AIModelProfile {
        id: id.clone(),
        name,
        connection_id,
        model_id,
        custom_model_id,
        max_output_tokens,
        reasoning_effort: None,
        capabilities: capabilities(cmd, p, "capabilities")?,
    });
    s.prefs_unsaved = true;
    Ok(json!({"id": id}))
}

/// Change a model profile.
fn model_update(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.model.update";
    let id = req_string(cmd, p, "id")?;
    let Some(model) = s.prefs.ai.models.models.iter_mut().find(|m| m.id == id) else {
        return Err(bad(cmd, format!("there is no model \"{id}\"")));
    };
    if let Some(name) = string(p, "name") {
        model.name = name;
    }
    if let Some(connection_id) = string(p, "connectionId") {
        model.connection_id = connection_id;
    }
    if let Some(model_id) = string(p, "modelID") {
        model.model_id = model_id;
    }
    if let Some(custom_model_id) = string(p, "customModelID") {
        model.custom_model_id = custom_model_id;
    }
    if let Some(tokens) = p.get("maxOutputTokens").and_then(Value::as_u64) {
        model.max_output_tokens = tokens.clamp(1, 1_000_000) as u32;
    }
    if get(p, "capabilities").is_some() {
        model.capabilities = capabilities(cmd, p, "capabilities")?;
    }
    s.prefs_unsaved = true;
    Ok(json!({"id": id}))
}

/// Remove a model profile, and clear the roles it held.
fn model_remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.model.remove";
    let id = req_string(cmd, p, "id")?;
    let before = s.prefs.ai.models.models.len();
    s.prefs.ai.models.models.retain(|m| m.id != id);
    if s.prefs.ai.models.models.len() == before {
        return Err(bad(cmd, format!("there is no model \"{id}\"")));
    }
    let assignments = &mut s.prefs.ai.models.assignments;
    for role in [&mut assignments.design, &mut assignments.review, &mut assignments.fast, &mut assignments.vision] {
        if *role == id {
            role.clear();
        }
    }
    s.prefs_unsaved = true;
    Ok(json!({"removed": id}))
}

/// Add a connection.
fn connection_add(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.connection.add";
    let provider_id = req_string(cmd, p, "provider")?;
    let Some(provider) = ProviderId::from_id(&provider_id) else {
        return Err(bad(cmd, format!("there is no provider \"{provider_id}\"")));
    };
    if s.prefs.ai.models.connections.len() >= MAX_CONNECTIONS {
        return Err(bad(cmd, format!("there are already {MAX_CONNECTIONS} connections")));
    }
    let custom_base_url = string(p, "customBaseURL").unwrap_or_default();
    if provider.supports_custom_base_url() && custom_base_url.trim().is_empty() {
        return Err(bad(cmd, "this provider needs a base URL"));
    }
    let api_type = match string(p, "customAPIType").as_deref() {
        None | Some("completions") => APIType::Completions,
        Some("responses") => APIType::Responses,
        Some(other) => return Err(bad(cmd, format!("`{other}` is not an API type"))),
    };
    let id = unique_id("connection-", &s.prefs.ai.models.connections.iter().map(|c| c.id.clone()).collect::<Vec<_>>());
    s.prefs.ai.models.connections.push(AIModelConnection {
        id: id.clone(),
        provider,
        custom_base_url,
        custom_api_type: api_type,
        credential_profile_id: id.clone(),
    });
    s.prefs_unsaved = true;
    Ok(json!({"id": id}))
}

/// Remove a connection, and the models that used it.
fn connection_remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.connection.remove";
    let id = req_string(cmd, p, "id")?;
    let before = s.prefs.ai.models.connections.len();
    s.prefs.ai.models.connections.retain(|c| c.id != id);
    if s.prefs.ai.models.connections.len() == before {
        return Err(bad(cmd, format!("there is no connection \"{id}\"")));
    }
    let gone: Vec<String> = s.prefs.ai.models.models.iter().filter(|m| m.connection_id == id).map(|m| m.id.clone()).collect();
    s.prefs.ai.models.models.retain(|m| m.connection_id != id);
    let assignments = &mut s.prefs.ai.models.assignments;
    for role in [&mut assignments.design, &mut assignments.review, &mut assignments.fast, &mut assignments.vision] {
        if gone.contains(role) {
            role.clear();
        }
    }
    s.prefs_unsaved = true;
    Ok(json!({"removed": id, "modelsRemoved": gone}))
}

/// Say which model handles which kind of work.
fn role_assign(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.role.assign";
    let role_id = req_string(cmd, p, "role")?;
    let Some(role) = AIModelRole::from_id(&role_id) else {
        return Err(bad(cmd, format!("there is no role \"{role_id}\"")));
    };
    let model_id = string(p, "modelId").unwrap_or_default();
    if !model_id.is_empty() && !s.prefs.ai.models.models.iter().any(|m| m.id == model_id) {
        return Err(bad(cmd, format!("there is no model \"{model_id}\"")));
    }
    match role {
        AIModelRole::Design => s.prefs.ai.models.assignments.design = model_id.clone(),
        AIModelRole::Review => s.prefs.ai.models.assignments.review = model_id.clone(),
        AIModelRole::Fast => s.prefs.ai.models.assignments.fast = model_id.clone(),
        AIModelRole::Vision => s.prefs.ai.models.assignments.vision = model_id.clone(),
    }
    s.prefs_unsaved = true;
    Ok(json!({"role": role.id(), "modelId": model_id}))
}

/// The reference a credential command addresses.
fn credential_ref(cmd: &str, p: &Value) -> Result<CredentialRef> {
    let integration = req_string(cmd, p, "integration")?;
    let profile_id = req_string(cmd, p, "profileId")?;
    let field = req_string(cmd, p, "field")?;
    Ok(CredentialRef::new(&integration, &profile_id, &field))
}

/// Store a key.
fn credential_set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.credential.set";
    let reference = credential_ref(cmd, p)?;
    let value = req_string(cmd, p, "value")?;
    let Some(store) = s.credentials.as_mut() else {
        return Err(bad(cmd, "this build has nowhere to keep a key"));
    };
    store.write(&reference, &value).map_err(|e| bad(cmd, e.to_string()))?;
    Ok(json!({"status": status_name(crate::ai::credential_status(&**store, &reference))}))
}

/// Forget a key.
fn credential_clear(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.credential.clear";
    let reference = credential_ref(cmd, p)?;
    let Some(store) = s.credentials.as_mut() else {
        return Err(bad(cmd, "this build has nowhere to keep a key"));
    };
    store.remove(&reference).map_err(|e| bad(cmd, e.to_string()))?;
    Ok(json!({"status": status_name(crate::ai::credential_status(&**store, &reference))}))
}

/// Say whether a key is there.
fn credential_status(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.credential.status";
    let reference = credential_ref(cmd, p)?;
    let Some(store) = s.credentials.as_ref() else {
        return Ok(json!({"status": "unavailable"}));
    };
    Ok(json!({"status": status_name(crate::ai::credential_status(&**store, &reference))}))
}

/// A status as the settings show it.
fn status_name(status: CredentialStatus) -> &'static str {
    match status {
        CredentialStatus::Configured => "configured",
        CredentialStatus::Missing => "missing",
        CredentialStatus::Unavailable => "unavailable",
    }
}

/// The skills and the commands they answer to.
fn skills_list(s: &mut Session, _p: &Value) -> Result<Value> {
    let router = figcraft_ai::skills::SkillRouter::default();
    let standing = s.prefs.ai.skills.all();
    let skills: Vec<Value> = standing
        .iter()
        .map(|skill| {
            json!({
                "id": skill.id,
                "name": skill.name,
                "displayName": skill.display_name,
                "description": skill.description,
                "version": skill.version,
                "tags": skill.tags,
                "isDefault": skill.is_default,
                "enabled": skill.enabled,
            })
        })
        .collect();
    let commands: Vec<Value> = router.commands().iter().map(|c| json!({"command": c.command, "description": c.description})).collect();
    Ok(json!({"skills": skills, "commands": commands}))
}

/// Switch a standing instruction on or off.
fn skill_toggle(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.skill.toggle";
    let id = req_string(cmd, p, "id")?;
    let enabled = boolean(p, "enabled");
    let current = s.prefs.ai.skills.all().iter().find(|s| s.id == id || s.name == id).map(|s| s.enabled).unwrap_or(false);
    let next = enabled.unwrap_or(!current);
    if !s.prefs.ai.skills.set_enabled(&id, next) {
        return Err(bad(cmd, format!("there is no skill \"{id}\"")));
    }
    s.prefs_unsaved = true;
    Ok(json!({"id": id, "enabled": next}))
}

/// Add a standing instruction.
fn skill_add(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.skill.add";
    let name = req_string(cmd, p, "name")?;
    let prompt_content = req_string(cmd, p, "promptContent")?;
    if s.prefs.ai.skills.all().len() >= MAX_PROMPT_SKILLS {
        return Err(bad(cmd, format!("there are already {MAX_PROMPT_SKILLS} standing instructions")));
    }
    let skill = s.prefs.ai.skills.add(PromptSkill {
        id: String::new(),
        name,
        display_name: string(p, "displayName").unwrap_or_default(),
        description: string(p, "description").unwrap_or_default(),
        version: String::new(),
        author: String::new(),
        source_url: String::new(),
        tags: get(p, "tags").and_then(Value::as_array).map(|t| t.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default(),
        is_default: false,
        enabled: boolean(p, "enabled").unwrap_or(true),
        prompt_content,
    });
    s.prefs_unsaved = true;
    Ok(json!({"id": skill.id}))
}

/// Remove a standing instruction.
fn skill_remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.skill.remove";
    let id = req_string(cmd, p, "id")?;
    if !s.prefs.ai.skills.remove(&id) {
        return Err(bad(cmd, format!("there is no skill \"{id}\"")));
    }
    s.prefs_unsaved = true;
    Ok(json!({"removed": id}))
}

/// What the models have been used for.
fn usage(s: &mut Session, _p: &Value) -> Result<Value> {
    Ok(usage_json(&s.ai))
}

/// Forget what the models have been used for.
fn usage_clear(s: &mut Session, _p: &Value) -> Result<Value> {
    s.ai.clear();
    ok()
}

/// What the models have been used for, as the settings show it.
fn usage_json(state: &AIState) -> Value {
    let summary = state.summary();
    json!({
        "requests": summary.requests,
        "completedRequests": summary.completed_requests,
        "failedRequests": summary.failed_requests,
        "inputTokens": summary.input_tokens,
        "outputTokens": summary.output_tokens,
        "cacheReadTokens": summary.cache_read_tokens,
        "cacheWriteTokens": summary.cache_write_tokens,
        "unavailableCacheTelemetry": summary.unavailable_cache_telemetry,
        "models": summary.models.iter().map(|m| json!({"provider": m.provider, "model": m.model, "requests": m.requests})).collect::<Vec<_>>(),
    })
}

/// An id no other one has: `prefix` plus the first free number.
fn unique_id(prefix: &str, taken: &[String]) -> String {
    for n in 1..10_000 {
        let id = format!("{prefix}{n}");
        if !taken.contains(&id) {
            return id;
        }
    }
    format!("{prefix}{}", taken.len() + 1)
}

/// The model a chat role resolves to, without its key.
fn chat_model(s: &mut Session, p: &Value) -> Result<Value> {
    let role = match string(p, "role").as_deref() {
        None | Some("design") => figcraft_ai::models::AIModelRole::Design,
        Some("review") => figcraft_ai::models::AIModelRole::Review,
        Some("fast") => figcraft_ai::models::AIModelRole::Fast,
        Some("vision") => figcraft_ai::models::AIModelRole::Vision,
        Some(other) => return Err(bad("ai.chat.model", format!("there is no role \"{other}\""))),
    };
    let (resolved, _key) = s.chat_model(role)?;
    Ok(serde_json::json!({
        "provider": resolved.connection.provider.id(),
        "modelID": figcraft_ai::models::AIModelSettings::effective_model_id(&resolved.profile),
        "baseURL": if resolved.connection.custom_base_url.is_empty() {
            resolved.connection.provider.default_base_url().unwrap_or_default()
        } else {
            resolved.connection.custom_base_url.as_str()
        },
        "maxOutputTokens": resolved.profile.max_output_tokens,
        "reasoningEffort": resolved.profile.reasoning_effort,
        "responsesAPI": resolved.connection.custom_api_type == figcraft_ai::models::APIType::Responses,
    }))
}

/// The MCP connections, as the settings show them.
fn mcp_list(s: &mut Session, _p: &Value) -> Result<Value> {
    let connections: Vec<Value> = s
        .prefs
        .ai
        .mcp
        .connections
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "url": c.url,
                "enabled": c.enabled,
                "authentication": match c.authentication {
                    figcraft_ai::mcp::MCPAuthentication::None => "none",
                    figcraft_ai::mcp::MCPAuthentication::Bearer => "bearer",
                },
            })
        })
        .collect();
    Ok(json!({"connections": connections}))
}

/// Add an MCP connection, after its definition passes the trust rules.
fn mcp_add(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.mcp.add";
    let name = req_string(cmd, p, "name")?;
    let url = req_string(cmd, p, "url")?;
    figcraft_ai::mcp::check_connection(&name, &url).map_err(|e| bad(cmd, e))?;
    if s.prefs.ai.mcp.connections.len() >= figcraft_ai::mcp::MAX_CONNECTIONS {
        return Err(bad(cmd, format!("there are already {} connections", figcraft_ai::mcp::MAX_CONNECTIONS)));
    }
    if s.prefs.ai.mcp.connections.iter().any(|c| c.name == name) {
        return Err(bad(cmd, format!("there is already a connection called \"{name}\"")));
    }
    let authentication =
        if boolean(p, "bearer").unwrap_or(false) { figcraft_ai::mcp::MCPAuthentication::Bearer } else { figcraft_ai::mcp::MCPAuthentication::None };
    let id = unique_id("mcp-", &s.prefs.ai.mcp.connections.iter().map(|c| c.id.clone()).collect::<Vec<_>>());
    s.prefs.ai.mcp.connections.push(figcraft_ai::mcp::MCPConnection {
        id: id.clone(),
        name,
        url,
        authentication,
        enabled: boolean(p, "enabled").unwrap_or(true),
    });
    s.prefs_unsaved = true;
    Ok(json!({"id": id}))
}

/// Change an MCP connection.
fn mcp_update(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.mcp.update";
    let id = req_string(cmd, p, "id")?;
    let Some(connection) = s.prefs.ai.mcp.connections.iter_mut().find(|c| c.id == id) else {
        return Err(bad(cmd, format!("there is no connection \"{id}\"")));
    };
    if let Some(name) = string(p, "name") {
        connection.name = name;
    }
    if let Some(url) = string(p, "url") {
        connection.url = url;
    }
    figcraft_ai::mcp::check_connection(&connection.name, &connection.url).map_err(|e| bad(cmd, e))?;
    if let Some(bearer) = boolean(p, "bearer") {
        connection.authentication = if bearer { figcraft_ai::mcp::MCPAuthentication::Bearer } else { figcraft_ai::mcp::MCPAuthentication::None };
    }
    if let Some(enabled) = boolean(p, "enabled") {
        connection.enabled = enabled;
    }
    s.prefs_unsaved = true;
    Ok(json!({"id": id}))
}

/// Remove an MCP connection.
fn mcp_remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.mcp.remove";
    let id = req_string(cmd, p, "id")?;
    let before = s.prefs.ai.mcp.connections.len();
    s.prefs.ai.mcp.connections.retain(|c| c.id != id);
    if s.prefs.ai.mcp.connections.len() == before {
        return Err(bad(cmd, format!("there is no connection \"{id}\"")));
    }
    s.prefs_unsaved = true;
    Ok(json!({"removed": id}))
}

/// An MCP connection's bearer token, into the credential store under its own reference.
fn mcp_credential_set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.mcp.credential.set";
    let profile_id = req_string(cmd, p, "profileId")?;
    let value = req_string(cmd, p, "value")?;
    let reference = figcraft_ai::credentials::CredentialRef::mcp_token(&profile_id);
    let Some(store) = s.credentials.as_mut() else {
        return Err(bad(cmd, "this build has nowhere to keep a key"));
    };
    store.write(&reference, &value).map_err(|e| bad(cmd, e.to_string()))?;
    Ok(json!({"status": status_name(crate::ai::credential_status(&**store, &reference))}))
}

/// The reference a connection's key is stored under, for the settings.
pub fn credential_ref_for(connection: &AIModelConnection) -> CredentialRef {
    connection_credential_ref(connection.provider.id(), &connection.id)
}
