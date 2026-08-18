//! Errors [`crate::engine::JobEngine`] can return. Every fallible operation
//! returns `Result<T, JobsError>` — this crate never turns a failure into a
//! fabricated success value (`AGENTS.md` rule 1), and every state transition
//! this crate refuses is a named, inspectable variant rather than a
//! swallowed `bool`/`Option` (`AGENTS.md` rule 6).

use std::fmt;

use loki_storage::StorageError;
use loki_storage::ids::{DeviceId, JobId};

/// Errors this crate's [`crate::engine::JobEngine`] can return.
#[derive(Debug)]
pub enum JobsError {
    /// The underlying `loki_storage` repository call failed (SQL error,
    /// invalid ID/timestamp/state shape, etc.) — propagated, not swallowed.
    Storage(StorageError),
    /// No job exists with the given ID.
    NotFound { job_id: JobId },
    /// A write was refused because it would violate a rule this engine
    /// enforces beyond raw storage constraints (e.g. restarting a job that
    /// is not in a restart-eligible state, or an idempotency key already
    /// claimed by a *different* job ID than the caller supplied).
    Conflict { detail: String },
    /// The requested job-state move is not part of this engine's lifecycle
    /// (`Docs/graph/tasks/JOB-001.md` Deliverable 1) — see
    /// `crate::transitions` for the allowed set.
    InvalidTransition {
        job_id: JobId,
        from: &'static str,
        to: &'static str,
    },
    /// `device_id` is already leased by another, still-unexpired attempt
    /// (`holder`) — the caller lost the race for this device.
    LeaseHeld { device_id: DeviceId, holder: JobId },
    /// The caller attempted to renew/release a lease for `device_id` that no
    /// attempt currently holds (never acquired, already released, or
    /// already expired and reclaimed by someone else).
    LeaseNotHeld { device_id: DeviceId },
    /// The caller's lease token for `device_id` does not match the token of
    /// whoever currently holds the lease — a stale/superseded holder trying
    /// to renew or release a lease it no longer owns.
    LeaseTokenMismatch { device_id: DeviceId },
}

impl fmt::Display for JobsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobsError::Storage(e) => write!(f, "storage error: {e}"),
            JobsError::NotFound { job_id } => write!(f, "job '{job_id}' not found"),
            JobsError::Conflict { detail } => write!(f, "jobs conflict: {detail}"),
            JobsError::InvalidTransition { job_id, from, to } => write!(
                f,
                "job '{job_id}' cannot move from state '{from}' to '{to}'"
            ),
            JobsError::LeaseHeld { device_id, holder } => write!(
                f,
                "device '{device_id}' is already leased by job '{holder}'"
            ),
            JobsError::LeaseNotHeld { device_id } => {
                write!(f, "no active lease is held for device '{device_id}'")
            }
            JobsError::LeaseTokenMismatch { device_id } => write!(
                f,
                "lease token for device '{device_id}' does not match the current holder"
            ),
        }
    }
}

impl std::error::Error for JobsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JobsError::Storage(e) => Some(e),
            _ => None,
        }
    }
}

impl From<StorageError> for JobsError {
    fn from(e: StorageError) -> Self {
        JobsError::Storage(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_job_id() -> JobId {
        JobId::new("job-1").unwrap()
    }

    fn sample_device_id() -> DeviceId {
        DeviceId::new("dev-1").unwrap()
    }

    #[test]
    fn display_does_not_panic_for_every_variant() {
        let samples: Vec<JobsError> = vec![
            JobsError::Storage(StorageError::UnsupportedCipher),
            JobsError::NotFound {
                job_id: sample_job_id(),
            },
            JobsError::Conflict { detail: "d".into() },
            JobsError::InvalidTransition {
                job_id: sample_job_id(),
                from: "queued",
                to: "succeeded",
            },
            JobsError::LeaseHeld {
                device_id: sample_device_id(),
                holder: sample_job_id(),
            },
            JobsError::LeaseNotHeld {
                device_id: sample_device_id(),
            },
            JobsError::LeaseTokenMismatch {
                device_id: sample_device_id(),
            },
        ];
        for err in samples {
            let _ = format!("{err}");
            let _: &dyn std::error::Error = &err;
        }
    }

    #[test]
    fn storage_error_source_is_preserved() {
        let err = JobsError::from(StorageError::UnsupportedCipher);
        assert!(matches!(err, JobsError::Storage(_)));
        assert!(std::error::Error::source(&err).is_some());
    }
}
