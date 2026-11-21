//! The cloud sync outbox: the jobs that keep saved documents on the server.
//!
//! Saving a file here is local; if it also belongs to a cloud account, a `put` job is queued.
//! A worker in the interface takes one job at a time to the server (it is the only half that
//! speaks the network). A failed attempt retries with exponential backoff and a little jitter,
//! but a refusal that will not change — a missing key, a disabled account — parks the job instead
//! of retrying it forever. A newer save of the same document replaces the older job still waiting:
//! nobody wants a queue of every intermediate version.
//!
//! This type is pure state and rules; it knows no clock and no network, so the engine can keep it
//! and test it without either.

use serde::{Deserialize, Serialize};

/// The most attempts one job gets before it is parked.
pub const MAX_ATTEMPTS: usize = 8;

/// The first wait, in milliseconds, after a failed attempt.
pub const BASE_BACKOFF_MS: u64 = 1500;

/// The longest wait.
pub const MAX_BACKOFF_MS: u64 = 60_000;

/// The most jobs kept.
pub const MAX_JOBS: usize = 200;

/// What a job asks the server to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncKind {
    /// Keep this document's bytes.
    Put,
    /// Forget it.
    Delete,
}

/// One pending change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SyncJob {
    /// The job's id.
    pub id: String,
    /// The document it concerns (its id or its path).
    pub document: String,
    /// What to do.
    pub kind: SyncKind,
    /// How many times it was tried.
    #[serde(default)]
    pub attempts: usize,
    /// When to try it next (milliseconds since the epoch), or `u64::MAX` when parked.
    #[serde(default = "parked")]
    pub next_attempt_at: u64,
    /// The last failure's own words, when there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

fn parked() -> u64 {
    u64::MAX
}

/// The sync state the settings and the control channel show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncState {
    /// Nothing is out.
    #[default]
    Idle,
    /// A job is on its way.
    Syncing,
    /// There is no connection.
    Offline,
    /// A job is parked and needs the person's attention.
    Error,
}

/// The queue and what it is doing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncOutbox {
    /// The jobs, oldest first.
    #[serde(default)]
    pub jobs: Vec<SyncJob>,
    /// What the sync is doing.
    #[serde(default)]
    pub state: SyncState,
    /// The most recent upload's progress, from 0 to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// The last error, for the status line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl SyncOutbox {
    /// A new job id: the first number none of the current jobs uses.
    fn new_id(&self) -> String {
        for n in 1..(MAX_JOBS + 2) {
            let id = format!("sync-{n}");
            if !self.jobs.iter().any(|j| j.id == id) {
                return id;
            }
        }
        format!("sync-{}", self.jobs.len() + 1)
    }

    /// Queue a save. An older `put` for the same document goes: its bytes are stale.
    ///
    /// `now` is milliseconds since the epoch, and `jitter` is 0..1 (the caller supplies both so
    /// this type needs neither a clock nor random numbers).
    pub fn queue_put(&mut self, document: &str, now: u64, _jitter: f64) -> String {
        // Drop the stale puts, keeping any delete (it is still meaningful).
        self.jobs.retain(|j| !(j.kind == SyncKind::Put && j.document == document));
        let id = self.new_id();
        self.jobs.push(SyncJob {
            id: id.clone(),
            document: document.to_string(),
            kind: SyncKind::Put,
            attempts: 0,
            next_attempt_at: now,
            last_error: None,
        });
        self.trim();
        self.state = SyncState::Idle;
        self.last_error = None;
        id
    }

    /// Queue a deletion. Older jobs for the document go: there is nothing left to keep.
    pub fn queue_delete(&mut self, document: &str, now: u64) -> String {
        self.jobs.retain(|j| j.document != document);
        let id = self.new_id();
        self.jobs.push(SyncJob {
            id: id.clone(),
            document: document.to_string(),
            kind: SyncKind::Delete,
            attempts: 0,
            next_attempt_at: now,
            last_error: None,
        });
        self.trim();
        self.state = SyncState::Idle;
        self.last_error = None;
        id
    }

    /// Keep the queue bounded, dropping the oldest overdue jobs first.
    fn trim(&mut self) {
        while self.jobs.len() > MAX_JOBS {
            let oldest = self.jobs.iter().enumerate().min_by_key(|(_, j)| j.next_attempt_at).map(|(i, _)| i);
            if let Some(i) = oldest {
                self.jobs.remove(i);
            } else {
                break;
            }
        }
    }

    /// The next job due at `now`, if any. A job that is out (`Syncing`) is skipped.
    pub fn next(&self, now: u64) -> Option<&SyncJob> {
        self.jobs.iter().filter(|j| j.next_attempt_at <= now).min_by_key(|j| j.next_attempt_at)
    }

    /// Mark the job at the head of the queue as out.
    pub fn begin(&mut self, id: &str) {
        self.state = SyncState::Syncing;
        self.progress = Some(0.0);
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
            job.last_error = None;
        }
    }

    /// The attempt succeeded: the job is done.
    pub fn done(&mut self, id: &str) {
        self.jobs.retain(|j| j.id != id);
        self.progress = None;
        self.state = SyncState::Idle;
        self.last_error = None;
    }

    /// The attempt failed. Retry with backoff, or park a permanent refusal.
    ///
    /// `permanent` is the transport's own judgement: a missing key or a refused login will not
    /// change by trying again in a minute.
    pub fn failed(&mut self, id: &str, message: &str, permanent: bool, now: u64, jitter: f64) {
        self.progress = None;
        self.last_error = Some(message.to_string());
        let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) else { return };
        job.attempts += 1;
        job.last_error = Some(message.to_string());
        if permanent || job.attempts >= MAX_ATTEMPTS {
            job.next_attempt_at = u64::MAX;
            self.state = SyncState::Error;
        } else {
            job.next_attempt_at = now.saturating_add(backoff_ms(job.attempts, jitter));
            self.state = SyncState::Idle;
        }
    }

    /// Wake a parked job (the person fixed the key, or signed in again).
    pub fn resume(&mut self, now: u64) {
        for job in &mut self.jobs {
            if job.next_attempt_at == u64::MAX {
                job.next_attempt_at = now;
                job.attempts = 0;
                job.last_error = None;
            }
        }
        self.state = SyncState::Idle;
        self.last_error = None;
    }

    /// Forget every job.
    pub fn clear(&mut self) {
        self.jobs.clear();
        self.state = SyncState::Idle;
        self.progress = None;
        self.last_error = None;
    }

    /// When the caller should next wake the worker, in milliseconds from now. `None`: no jobs.
    pub fn next_wake_delay(&self, now: u64) -> Option<u64> {
        let next = self.jobs.iter().map(|j| j.next_attempt_at).min()?;
        if next == u64::MAX {
            return None;
        }
        Some(next.saturating_sub(now).max(250))
    }

    /// How many jobs are waiting.
    pub fn pending(&self) -> usize {
        self.jobs.len()
    }
}

/// The wait after attempt `attempts`: 1.5s, 3s, 6s… capped at a minute, plus up to 20% jitter.
pub fn backoff_ms(attempts: usize, jitter: f64) -> u64 {
    let shift = attempts.saturating_sub(1).min(20);
    let base = BASE_BACKOFF_MS.saturating_mul(1u64 << shift).min(MAX_BACKOFF_MS);
    let extra = (base as f64 * jitter.clamp(0.0, 1.0) * 0.2) as u64;
    base.saturating_add(extra).min(MAX_BACKOFF_MS)
}

/// Whether a failure will not change by retrying: a key that is missing or refused, or a server
/// that says the request itself is wrong.
pub fn permanent_failure(message: &str) -> bool {
    let lower = message.to_lowercase();
    ["401", "403", "not configured", "no api key", "no key", "session has ended", "sign in again", "no account", "insufficient credit"]
        .iter()
        .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_save_is_due_now() {
        let mut out = SyncOutbox::default();
        let id = out.queue_put("doc-1", 1000, 0.0);
        assert_eq!(out.next(1000).map(|j| j.id.as_str()), Some(id.as_str()));
    }

    #[test]
    fn a_newer_save_replaces_an_older_waiting_one() {
        let mut out = SyncOutbox::default();
        out.queue_put("doc-1", 1000, 0.0);
        out.queue_put("doc-1", 2000, 0.0);
        assert_eq!(out.pending(), 1);
        assert_eq!(out.jobs[0].next_attempt_at, 2000);
    }

    #[test]
    fn a_delete_replaces_every_job_for_the_document() {
        let mut out = SyncOutbox::default();
        out.queue_put("doc-1", 1000, 0.0);
        out.queue_delete("doc-1", 1001);
        assert_eq!(out.pending(), 1);
        assert_eq!(out.jobs[0].kind, SyncKind::Delete);
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        assert_eq!(backoff_ms(1, 0.0), 1500);
        assert_eq!(backoff_ms(2, 0.0), 3000);
        assert_eq!(backoff_ms(3, 0.0), 6000);
        assert_eq!(backoff_ms(20, 0.0), MAX_BACKOFF_MS);
        // Jitter is a fifth of the base at most.
        assert_eq!(backoff_ms(1, 1.0), 1500 + 300);
    }

    #[test]
    fn a_failure_retries_later_and_a_permanent_one_is_parked() {
        let mut retry = SyncOutbox::default();
        let id = retry.queue_put("doc", 0, 0.0);
        retry.failed(&id, "connection refused", false, 0, 0.0);
        assert_eq!(retry.jobs[0].next_attempt_at, 1500);
        assert_eq!(retry.state, SyncState::Idle);

        let mut parked = SyncOutbox::default();
        let id = parked.queue_put("doc", 0, 0.0);
        parked.failed(&id, "no API key saved", true, 0, 0.0);
        assert_eq!(parked.jobs[0].next_attempt_at, u64::MAX);
        assert_eq!(parked.state, SyncState::Error);
        assert!(parked.next_wake_delay(0).is_none(), "nothing is due while parked");
    }

    #[test]
    fn the_last_attempt_parks_the_job() {
        let mut out = SyncOutbox::default();
        let id = out.queue_put("doc", 0, 0.0);
        for _ in 0..MAX_ATTEMPTS {
            out.failed(&id, "try again", false, 0, 0.0);
        }
        assert_eq!(out.jobs[0].attempts, MAX_ATTEMPTS);
        assert_eq!(out.jobs[0].next_attempt_at, u64::MAX);
    }

    #[test]
    fn resuming_wakes_every_parked_job() {
        let mut out = SyncOutbox::default();
        let first = out.queue_put("a", 0, 0.0);
        out.failed(&first, "no api key", true, 0, 0.0);
        out.resume(5000);
        assert_eq!(out.jobs[0].next_attempt_at, 5000);
        assert_eq!(out.jobs[0].attempts, 0);
        assert_eq!(out.state, SyncState::Idle);
    }

    #[test]
    fn a_done_job_leaves_the_queue() {
        let mut out = SyncOutbox::default();
        let id = out.queue_put("doc", 0, 0.0);
        out.begin(&id);
        assert_eq!(out.state, SyncState::Syncing);
        out.done(&id);
        assert_eq!(out.pending(), 0);
        assert_eq!(out.state, SyncState::Idle);
    }

    #[test]
    fn the_delay_never_dips_below_a_quarter_second() {
        let mut out = SyncOutbox::default();
        out.queue_put("doc", 10_000, 0.0);
        assert_eq!(out.next_wake_delay(9_999), Some(250));
    }

    #[test]
    fn permanent_failures_are_recognised() {
        assert!(permanent_failure("401 Unauthorized"));
        assert!(permanent_failure("the session has ended; sign in again"));
        assert!(permanent_failure("Storage is not configured"));
        assert!(!permanent_failure("connection refused"));
        assert!(!permanent_failure("429 Too Many Requests"));
    }
}
