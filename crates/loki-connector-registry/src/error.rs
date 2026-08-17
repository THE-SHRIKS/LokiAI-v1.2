//! Registry-level errors — distinct from `loki_connectors_sdk::error`'s
//! `ValidationError`/`ConnectorError`, which describe malformed SDK values
//! and connector-reported operation failures respectively. This module's
//! `RegistryError` describes registry *bookkeeping* failures (double
//! registration, operating on an id that was never registered or was
//! already deregistered) — a different failure class the SDK has no
//! opinion on, since it defines no registry.

use std::fmt;

use loki_connectors_sdk::ids::ConnectorId;

/// A registry bookkeeping failure. Never fabricated: every variant carries
/// the exact `ConnectorId` the caller asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// `register` was called with a `ConnectorId` that is already present.
    /// A caller that wants to replace a registration must `deregister` it
    /// first — this crate never silently overwrites an existing entry.
    AlreadyRegistered(ConnectorId),
    /// `deregister`, `identify_one`, `health_check_one`, or a summary lookup
    /// was called with a `ConnectorId` this registry has no entry for
    /// (never registered, or already deregistered).
    NotFound(ConnectorId),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryError::AlreadyRegistered(id) => {
                write!(f, "connector {id} is already registered")
            }
            RegistryError::NotFound(id) => write!(f, "connector {id} is not registered"),
        }
    }
}

impl std::error::Error for RegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_the_connector_id() {
        let id = ConnectorId::new("loki-mock-connector").unwrap();
        assert!(
            RegistryError::AlreadyRegistered(id.clone())
                .to_string()
                .contains("loki-mock-connector")
        );
        assert!(
            RegistryError::NotFound(id)
                .to_string()
                .contains("is not registered")
        );
    }
}
