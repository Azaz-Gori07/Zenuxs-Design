//! The cloud sync runtime: the interface's worker that drains the engine's outbox.
//!
//! The engine owns the jobs (they are preferences and travel with them). This module owns the
//! network half: when a job is due it takes the document's bytes, sends them on a worker thread
//! through the platform's cloud factory, and reports the outcome back to the engine, which moves
//! the job on or backs it off.
//!
//! The worker runs one job at a time. A saved file is encoded before it goes, so a large document
//! does not hold the window; the bytes themselves never enter the preferences.

use std::sync::mpsc::{Receiver, channel};

use figcraft_ai::{SyncJob, SyncKind, SyncOutbox, SyncState};
use serde_json::json;

use crate::FigApp;

/// The sync worker's state, held by the interface.
#[derive(Default)]
pub struct SyncRuntime {
    /// The reply from the job in flight, when one is out.
    reply: Option<Receiver<SyncReply>>,
    /// The id of the job in flight.
    job: Option<String>,
    /// The next time to wake, in milliseconds since the epoch (0: as soon as possible).
    /// When to next wake the worker, in milliseconds since the epoch.
    pub next_wake_ms: u64,
}

/// What the worker answers.
struct SyncReply {
    id: String,
    result: Result<(), String>,
}

impl SyncRuntime {
    /// Try the queue on the next tick, not after the backoff.
    pub fn wake_now(&mut self) {
        self.next_wake_ms = 0;
    }
}

/// A document's bytes for a job, taken from the file on disk when there is one, or from the open
/// document in memory.
pub fn document_bytes(app: &FigApp, job: &SyncJob) -> Result<Vec<u8>, String> {
    // A path names a saved file; a bare id names an open document.
    if std::path::Path::new(&job.document).components().count() > 1 || job.document.contains(std::path::MAIN_SEPARATOR) {
        return std::fs::read(&job.document).map_err(|e| format!("the saved file could not be read: {e}"));
    }
    let doc_uid = job.document.parse::<u64>().map_err(|_| "the document id is not one this app uses".to_string())?;
    let ds = app.session.docs.iter().find(|d| d.uid == doc_uid).ok_or("the document is no longer open")?;
    figcraft_engine::cmd::file::save::file_bytes(ds).map_err(|e| e.to_string())
}

/// Start a job on its worker thread, when one is due and none is running.
pub fn start_due(app: &mut FigApp) -> bool {
    if app.ui.sync_worker.reply.is_some() {
        return false;
    }
    let now = figcraft_engine::ai::now_ms();
    if now < app.ui.sync_worker.next_wake_ms {
        return false;
    }
    let Some(job) = app.session.prefs.ai.sync.next(now).cloned() else {
        app.ui.sync_worker.next_wake_ms = 0;
        return false;
    };

    // A delete needs no bytes; a put does. A document that cannot be read is a permanent failure
    // for this job (the document may have been closed), so it parks instead of retrying forever.
    let payload = if job.kind == SyncKind::Put {
        match document_bytes(app, &job) {
            Ok(bytes) => bytes,
            Err(e) => {
                app.run("ai.cloud.sync.done", json!({"id": job.id, "ok": false, "error": e}));
                return true;
            }
        }
    } else {
        Vec::new()
    };

    let server = app.session.prefs.ai.account_server.trim().to_string();
    let token = app
        .session
        .credentials
        .as_ref()
        .and_then(|s| s.read(&figcraft_ai::credentials::CredentialRef::new("cloud", "session", "token")).ok().flatten());
    if server.is_empty() || token.is_none() {
        app.run("ai.cloud.sync.done", json!({"id": job.id, "ok": false, "error": "no cloud account is signed in"}));
        return true;
    }

    let (tx, rx) = channel();
    let factory = app.cloud_factory.clone();
    let id = job.id.clone();
    let kind = job.kind;
    let document = job.document.clone();
    let spawn = std::thread::Builder::new().name("figcraft-cloud-sync".into()).spawn(move || {
        let mut session = factory.make(server, token);
        let result = match kind {
            SyncKind::Put => session.put(&document, &payload).map_err(|e| e.message()),
            SyncKind::Delete => session.delete(&document).map_err(|e| e.message()),
        };
        let _ = tx.send(SyncReply { id: id.clone(), result });
    });
    match spawn {
        Ok(thread) => {
            // The handle is kept until the reply lands; dropping it detaches the worker.
            let _ = thread;
            app.ui.sync_worker.reply = Some(rx);
            app.ui.sync_worker.job = Some(job.id.clone());
            app.run("ai.cloud.sync.begin", json!({"id": job.id}));
            true
        }
        Err(e) => {
            app.run("ai.cloud.sync.done", json!({"id": job.id, "ok": false, "error": format!("the worker could not start: {e}")}));
            true
        }
    }
}

/// Take the worker's answer, if it has landed, and hand it to the engine.
pub fn drain(app: &mut FigApp) -> bool {
    let Some(receiver) = app.ui.sync_worker.reply.as_ref() else { return false };
    let Ok(reply) = receiver.try_recv() else { return false };
    app.ui.sync_worker.reply = None;
    let id = app.ui.sync_worker.job.take().unwrap_or_else(|| reply.id.clone());
    match reply.result {
        Ok(()) => {
            app.run("ai.cloud.sync.done", json!({"id": id, "ok": true}));
            app.toast(crate::i18n::tr("The document was saved to the cloud.").to_string());
        }
        Err(e) => {
            app.run("ai.cloud.sync.done", json!({"id": id, "ok": false, "error": e}));
        }
    }
    // The next wake is driven by the engine's backoff.
    app.ui.sync_worker.next_wake_ms = figcraft_engine::ai::now_ms();
    true
}

/// A line for the settings: what the sync is doing.
pub fn status_text(outbox: &SyncOutbox) -> String {
    match outbox.state {
        SyncState::Idle if outbox.pending() == 0 => crate::i18n::tr("Nothing to sync.").to_string(),
        SyncState::Idle => crate::i18n::trf("{n} waiting", &[("n", &outbox.pending())]),
        SyncState::Syncing => crate::i18n::tr("Uploading…").to_string(),
        SyncState::Offline => crate::i18n::tr("Offline.").to_string(),
        SyncState::Error => outbox.last_error.clone().unwrap_or_else(|| crate::i18n::tr("Sync needs attention.").to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_outbox_without_jobs_is_idle() {
        assert_eq!(status_text(&SyncOutbox::default()), crate::i18n::tr("Nothing to sync."));
    }

    #[test]
    fn a_saved_document_is_in_the_prefs_not_the_settings_file_alone() {
        let mut app = crate::present_tests::Ui::new(figcraft_engine::Session::empty()).app;
        app.run("ai.cloud.sync.enqueue", json!({"document": "1"}));
        assert_eq!(app.session.prefs.ai.sync.pending(), 1);
        let text = serde_json::to_string(&app.session.prefs).expect("the preferences serialise");
        assert!(text.contains("sync"), "the outbox is in the preferences");
    }
}
