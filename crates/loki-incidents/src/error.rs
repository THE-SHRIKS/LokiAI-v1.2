//! Structured error types for this crate.

use std::fmt;

pub use loki_connectors_sdk::error::ValidationError;

/// Every fallible operation this crate exposes returns this error.
#[derive(Debug)]
pub enum IncidentError {
    /// A caller-constructed value violated a documented constraint
    /// (`AGENTS.md` rule 4 — types alone are not runtime validation).
    Validation(ValidationError),
    /// The durable store rejected a read/write. Carries the underlying
    /// `rusqlite` error's rendered message rather than the error itself, so
    /// this crate's public API surface does not leak a third-party error
    /// type it does not otherwise expose.
    Storage(String),
    /// No incident exists for the given `IncidentId`.
    NotFound,
    /// The requested state-machine transition is not reachable from the
    /// incident's current state per `Docs/SPEC.md` §4.6's diagram
    /// (`Docs/graph/tasks/INC-001.md` Deliverable 2: "no edge not in the
    /// diagram is reachable"). Carries the exact current state and the
    /// transition that was rejected, so a caller/test can assert precisely
    /// what was denied.
    InvalidTransition {
        from: &'static str,
        transition: &'static str,
    },
    /// `crate::engine::IncidentEngine::ingest_fired_outcome` was called with
    /// an `EvaluationOutcome` variant that is not `Fired`/`MissingDataAlert`
    /// — this crate only ever creates/correlates incidents from one of those
    /// two, never from `NotMet`/`Pending`/`Suppressed`/`Disabled`/
    /// `OutOfScope` (`Docs/graph/tasks/INC-001.md` §1: "turns a ...
    /// Fired/MissingDataAlert outcome into a ... incident").
    NotAFiringOutcome,
    /// A caller-supplied timestamp plus policy duration cannot be
    /// represented by `SystemTime`. Reported as an error instead of letting
    /// `SystemTime`'s `Add` implementation panic the runtime process.
    TimeOverflow { field: &'static str },
    /// A durable store was opened at a path/connection that has not had this
    /// crate's own migrations applied yet — an honest "cannot serve this
    /// request" state (`AGENTS.md` rule 1), never silently treated as an
    /// empty-but-working store.
    NotMigrated,
}

impl fmt::Display for IncidentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IncidentError::Validation(e) => write!(f, "invalid input: {e}"),
            IncidentError::Storage(msg) => write!(f, "storage error: {msg}"),
            IncidentError::NotFound => write!(f, "incident not found"),
            IncidentError::InvalidTransition { from, transition } => write!(
                f,
                "transition {transition:?} is not reachable from state {from:?} per Docs/SPEC.md §4.6"
            ),
            IncidentError::NotAFiringOutcome => write!(
                f,
                "only Fired/MissingDataAlert evaluation outcomes create or correlate an incident"
            ),
            IncidentError::TimeOverflow { field } => {
                write!(f, "time calculation overflowed for {field}")
            }
            IncidentError::NotMigrated => {
                write!(f, "incident store has not had its schema migrated yet")
            }
        }
    }
}

impl std::error::Error for IncidentError {}

impl From<ValidationError> for IncidentError {
    fn from(value: ValidationError) -> Self {
        IncidentError::Validation(value)
    }
}

impl From<rusqlite::Error> for IncidentError {
    fn from(value: rusqlite::Error) -> Self {
        IncidentError::Storage(value.to_string())
    }
}
