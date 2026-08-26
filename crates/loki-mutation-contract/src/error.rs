//! This crate's own honest, panic-free error type.
//!
//! Per `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md` §6 deliverable
//! `MutationContractError`: construction-time validation failures only —
//! this crate performs no I/O, so it has no `Sqlite`/`Io` variant (contrast
//! `loki_storage::StorageError`, `loki_approvals::ApprovalError`, both of
//! which do wrap I/O failures because those crates perform I/O).

use std::fmt;

/// Errors this crate can return. Every variant is a distinct, honest
/// failure mode — this crate never converts a failure into a fabricated
/// success value (`AGENTS.md` rule 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationContractError {
    /// A caller-supplied field failed structural/bounds validation at
    /// construction time (empty, oversized, control characters, malformed
    /// digest shape, non-monotonic timestamps, or a semantically required
    /// pairing that was not satisfied — e.g. an approval-gated method
    /// constructed without a matching approval reference).
    InvalidInput { field: &'static str, reason: String },
}

impl fmt::Display for MutationContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { field, reason } => {
                write!(f, "invalid mutation contract field '{field}': {reason}")
            }
        }
    }
}

impl std::error::Error for MutationContractError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_does_not_panic() {
        let err = MutationContractError::InvalidInput {
            field: "target_id",
            reason: "must not be empty".to_string(),
        };
        let _ = format!("{err}");
        let _: &dyn std::error::Error = &err;
    }
}
