//! Structured error types for this crate.

use std::fmt;

pub use loki_connectors_sdk::error::ValidationError;

/// Every fallible operation this crate exposes returns this error.
#[derive(Debug)]
pub enum MonitoringError {
    /// A caller-constructed value violated a documented constraint
    /// (`AGENTS.md` rule 4 — types alone are not runtime validation).
    Validation(ValidationError),
    /// The durable store rejected a read/write. Carries the underlying
    /// `rusqlite` error's rendered message rather than the error itself, so
    /// this crate's public API surface does not leak a third-party error
    /// type it does not otherwise expose.
    Storage(String),
    /// A durable store was opened at a path/connection that has not had
    /// this crate's own migrations applied yet — an honest "cannot serve
    /// this request" state (`AGENTS.md` rule 1), never silently treated as
    /// an empty-but-working store.
    NotMigrated,
}

impl fmt::Display for MonitoringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MonitoringError::Validation(e) => write!(f, "invalid input: {e}"),
            MonitoringError::Storage(msg) => write!(f, "storage error: {msg}"),
            MonitoringError::NotMigrated => {
                write!(f, "telemetry store has not had its schema migrated yet")
            }
        }
    }
}

impl std::error::Error for MonitoringError {}

impl From<ValidationError> for MonitoringError {
    fn from(value: ValidationError) -> Self {
        MonitoringError::Validation(value)
    }
}

impl From<rusqlite::Error> for MonitoringError {
    fn from(value: rusqlite::Error) -> Self {
        MonitoringError::Storage(value.to_string())
    }
}
