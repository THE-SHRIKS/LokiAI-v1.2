//! In-memory per-device lease table.
//!
//! Deliberately **not** a new SQLite table/migration: this node's writable
//! scope is `crates/loki-jobs/**` (plus one `Cargo.toml` members line) and
//! explicitly excludes `crates/loki-storage/src/**`
//! (`Docs/graph/tasks/JOB-001.md` §2) — and every repository this crate
//! could add a table's persistence *behind* would itself have to live in
//! `loki_storage::Storage` (its connection is `pub(crate)` to that crate
//! only), which this node cannot touch. Rather than reshape `DAT-001`'s
//! schema unilaterally (an explicit stop condition,
//! `Docs/graph/tasks/JOB-001.md` §7), lease *state* lives here, in the
//! process that runs the engine, and lease *durability across a crash* is
//! achieved differently: [`crate::engine::JobEngine::reconcile`] treats
//! "no in-memory lease currently references this job" as the operational
//! definition of "this process cannot vouch for whether this job is still
//! running" — which is exactly true immediately after a restart (a fresh
//! [`LeaseTable`] starts empty) and quarantines any job a prior process left
//! in a non-terminal, in-flight state instead of guessing. See
//! `Docs/graph/handoffs/JOB-001-1.md` for the full rationale.
//!
//! Correctness for "two concurrent attempts for the same device cannot both
//! hold a lease" comes from a single [`std::sync::Mutex`] guarding
//! check-and-insert atomically — not from any weaker check-then-act pattern.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use loki_storage::ids::{DeviceId, JobId};

use crate::error::JobsError;

/// Opaque proof of current lease ownership: a monotonically increasing
/// per-[`LeaseTable`] "fencing token" (not a random ID — proving "you are
/// the most recent acquirer of this device" needs only a counter, not a UUID
/// crate; `AGENTS.md` rule 11). A stale holder's token no longer matches the
/// table's current entry for that device once someone else has
/// acquired/reclaimed it, so a delayed renew/release from the stale holder
/// is rejected rather than silently mutating the new holder's lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseToken(u64);

struct LeaseEntry {
    holder_job_id: JobId,
    token: LeaseToken,
    expires_at_millis: u64,
}

pub(crate) struct LeaseTable {
    inner: Mutex<Inner>,
}

struct Inner {
    next_token: u64,
    leases: HashMap<String, LeaseEntry>,
}

impl LeaseTable {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                next_token: 0,
                leases: HashMap::new(),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| {
            // Matches `loki_storage::Storage::lock`'s stance: a prior panic
            // elsewhere shouldn't cascade into a second, less-informative
            // panic here; recover and let this crate's own `Result`-based
            // error handling surface anything actually wrong.
            poisoned.into_inner()
        })
    }

    /// Attempts to acquire the lease for `device_id` on behalf of `job_id`.
    /// Succeeds if no lease is currently held, or if the held lease has
    /// expired as of `now_millis` — in either case this is atomic with the
    /// check under one lock, so a genuine race between two callers has
    /// exactly one winner. Fails with [`JobsError::LeaseHeld`] if another,
    /// still-unexpired attempt already holds it.
    pub(crate) fn acquire(
        &self,
        device_id: &DeviceId,
        job_id: &JobId,
        now_millis: u64,
        ttl_millis: u64,
    ) -> Result<LeaseToken, JobsError> {
        let mut inner = self.lock();
        let key = device_id.as_str().to_string();

        if let Some(existing) = inner.leases.get(&key)
            && existing.expires_at_millis > now_millis
        {
            return Err(JobsError::LeaseHeld {
                device_id: device_id.clone(),
                holder: existing.holder_job_id.clone(),
            });
        }

        inner.next_token += 1;
        let token = LeaseToken(inner.next_token);
        inner.leases.insert(
            key,
            LeaseEntry {
                holder_job_id: job_id.clone(),
                token,
                expires_at_millis: now_millis.saturating_add(ttl_millis),
            },
        );
        Ok(token)
    }

    /// Extends an already-held, still-valid lease. Requires `token` to
    /// match the current holder — a lease that has already expired (and so
    /// may have been reclaimed by someone else) cannot be renewed by the
    /// original holder; it must re-[`LeaseTable::acquire`].
    pub(crate) fn renew(
        &self,
        device_id: &DeviceId,
        token: LeaseToken,
        now_millis: u64,
        ttl_millis: u64,
    ) -> Result<(), JobsError> {
        let mut inner = self.lock();
        let key = device_id.as_str().to_string();
        match inner.leases.get_mut(&key) {
            Some(entry) if entry.expires_at_millis <= now_millis => Err(JobsError::LeaseNotHeld {
                device_id: device_id.clone(),
            }),
            Some(entry) if entry.token != token => Err(JobsError::LeaseTokenMismatch {
                device_id: device_id.clone(),
            }),
            Some(entry) => {
                entry.expires_at_millis = now_millis.saturating_add(ttl_millis);
                Ok(())
            }
            None => Err(JobsError::LeaseNotHeld {
                device_id: device_id.clone(),
            }),
        }
    }

    /// Releases a held lease early (e.g. a job finished before its lease
    /// would naturally expire). Requires `token` to match the current
    /// holder for the same reason [`LeaseTable::renew`] does.
    pub(crate) fn release(&self, device_id: &DeviceId, token: LeaseToken) -> Result<(), JobsError> {
        let mut inner = self.lock();
        let key = device_id.as_str().to_string();
        match inner.leases.get(&key) {
            Some(entry) if entry.token == token => {
                inner.leases.remove(&key);
                Ok(())
            }
            Some(_) => Err(JobsError::LeaseTokenMismatch {
                device_id: device_id.clone(),
            }),
            None => Err(JobsError::LeaseNotHeld {
                device_id: device_id.clone(),
            }),
        }
    }

    /// The set of job IDs that currently hold a non-expired lease as of
    /// `now_millis` — used by
    /// [`crate::engine::JobEngine::reconcile`] to tell "still legitimately
    /// in-flight in this process" apart from "this process has no memory of
    /// this job at all" (the crash-and-restart case).
    pub(crate) fn active_holders(&self, now_millis: u64) -> HashSet<JobId> {
        let inner = self.lock();
        inner
            .leases
            .values()
            .filter(|entry| entry.expires_at_millis > now_millis)
            .map(|entry| entry.holder_job_id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str) -> DeviceId {
        DeviceId::new(id).unwrap()
    }

    fn job(id: &str) -> JobId {
        JobId::new(id).unwrap()
    }

    #[test]
    fn acquire_succeeds_when_unheld() {
        let table = LeaseTable::new();
        assert!(table.acquire(&device("d1"), &job("j1"), 0, 1_000).is_ok());
    }

    #[test]
    fn second_acquire_before_expiry_is_rejected() {
        let table = LeaseTable::new();
        table.acquire(&device("d1"), &job("j1"), 0, 10_000).unwrap();
        let err = table
            .acquire(&device("d1"), &job("j2"), 100, 10_000)
            .unwrap_err();
        assert!(matches!(err, JobsError::LeaseHeld { holder, .. } if holder == job("j1")));
    }

    #[test]
    fn acquire_after_expiry_is_reclaimable_by_another_attempt() {
        let table = LeaseTable::new();
        table.acquire(&device("d1"), &job("j1"), 0, 1_000).unwrap();
        // j1's lease expires at t=1000; j2 tries at t=1500.
        let result = table.acquire(&device("d1"), &job("j2"), 1_500, 1_000);
        assert!(result.is_ok());
    }

    #[test]
    fn renew_extends_a_valid_lease() {
        let table = LeaseTable::new();
        let token = table.acquire(&device("d1"), &job("j1"), 0, 1_000).unwrap();
        table.renew(&device("d1"), token, 900, 1_000).unwrap();
        // Without the renewal this would already have expired by t=1500.
        assert!(
            table
                .acquire(&device("d1"), &job("j2"), 1_500, 1_000)
                .is_err()
        );
    }

    #[test]
    fn renew_with_wrong_token_is_rejected() {
        let table = LeaseTable::new();
        table.acquire(&device("d1"), &job("j1"), 0, 1_000).unwrap();
        let wrong_token = LeaseToken(9_999);
        let err = table
            .renew(&device("d1"), wrong_token, 100, 1_000)
            .unwrap_err();
        assert!(matches!(err, JobsError::LeaseTokenMismatch { .. }));
    }

    #[test]
    fn renew_after_expiry_is_rejected_not_silently_extended() {
        let table = LeaseTable::new();
        let token = table.acquire(&device("d1"), &job("j1"), 0, 1_000).unwrap();
        let err = table.renew(&device("d1"), token, 2_000, 1_000).unwrap_err();
        assert!(matches!(err, JobsError::LeaseNotHeld { .. }));
    }

    #[test]
    fn renew_unknown_device_is_rejected() {
        let table = LeaseTable::new();
        let err = table
            .renew(&device("d1"), LeaseToken(1), 0, 1_000)
            .unwrap_err();
        assert!(matches!(err, JobsError::LeaseNotHeld { .. }));
    }

    #[test]
    fn release_frees_the_device_for_immediate_reacquisition() {
        let table = LeaseTable::new();
        let token = table.acquire(&device("d1"), &job("j1"), 0, 10_000).unwrap();
        table.release(&device("d1"), token).unwrap();
        assert!(table.acquire(&device("d1"), &job("j2"), 0, 10_000).is_ok());
    }

    #[test]
    fn release_with_wrong_token_is_rejected_and_lease_remains_held() {
        let table = LeaseTable::new();
        table.acquire(&device("d1"), &job("j1"), 0, 10_000).unwrap();
        let err = table.release(&device("d1"), LeaseToken(9_999)).unwrap_err();
        assert!(matches!(err, JobsError::LeaseTokenMismatch { .. }));
        // Still held by j1.
        assert!(table.acquire(&device("d1"), &job("j2"), 0, 10_000).is_err());
    }

    #[test]
    fn active_holders_excludes_expired_leases() {
        let table = LeaseTable::new();
        table.acquire(&device("d1"), &job("j1"), 0, 1_000).unwrap();
        assert!(table.active_holders(500).contains(&job("j1")));
        assert!(!table.active_holders(1_500).contains(&job("j1")));
    }

    #[test]
    fn concurrent_acquire_for_the_same_device_has_exactly_one_winner() {
        use std::sync::Arc;
        use std::thread;

        let table = Arc::new(LeaseTable::new());
        let device_id = device("contested-device");
        let mut handles = Vec::new();
        for i in 0..16 {
            let table = Arc::clone(&table);
            let device_id = device_id.clone();
            handles.push(thread::spawn(move || {
                table.acquire(&device_id, &job(&format!("attempt-{i}")), 0, 60_000)
            }));
        }

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let winners = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(
            winners, 1,
            "exactly one concurrent attempt must win the lease"
        );
    }
}
