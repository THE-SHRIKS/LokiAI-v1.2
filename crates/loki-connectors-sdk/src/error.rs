//! Structured error types.
//!
//! Two distinct error types exist, deliberately not merged into one:
//!
//! - [`ValidationError`]: raised by this crate's own constructors when a
//!   caller (a future connector, this crate's tests, or the runtime once it
//!   consumes this crate) tries to build a value that violates a documented
//!   constraint (e.g. an empty capability id). It never crosses a connector
//!   boundary; it is a programming-error signal for whoever is constructing
//!   SDK values, per `AGENTS.md` rule 4.
//! - [`ConnectorError`]: the structured, redaction-aware error shape a
//!   connector implementation returns to describe an operation failure —
//!   the "terminal structured errors" `Docs/CONNECTOR_SPEC.md` §8 requires
//!   every operation to declare, and the "structured error and redaction"
//!   `Docs/CONNECTOR_SPEC.md` §11 requires the conformance suite to check.
//!   `message` is expected to already be redacted by the connector that
//!   produced it (`Docs/CONNECTOR_SPEC.md` §9: "arguments with secrets
//!   redacted"; `AGENTS.md` rule 9) — this crate has no way to redact a
//!   message after the fact, so it documents the obligation rather than
//!   inventing a redaction pass it cannot correctly perform generically.

use std::fmt;

use crate::validate::non_empty_bounded;

/// A constructor-time validation failure raised by this crate's own types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub field: &'static str,
    pub reason: String,
}

impl ValidationError {
    pub fn new(field: &'static str, reason: impl Into<String>) -> Self {
        Self {
            field,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {}", self.field, self.reason)
    }
}

impl std::error::Error for ValidationError {}

/// A structured, machine-readable error a connector returns to describe why
/// a stage did not succeed.
///
/// `code` is a stable, connector-defined identifier (e.g.
/// `"serial_port_busy"`) a caller can match on without parsing `message`.
/// `Docs/CONNECTOR_SPEC.md` does not enumerate a closed set of error codes
/// (unlike, say, health-check status in §7), so — following the same
/// principle `packages/contracts/src/connector.ts` applies to open
/// capability ids — `code` is validated only for shape (non-empty, bounded),
/// not against a fabricated closed vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub evidence_ref: Option<String>,
}

impl ConnectorError {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let code = code.into();
        non_empty_bounded("code", &code, 128)?;
        let message = message.into();
        non_empty_bounded("message", &message, 2000)?;
        Ok(Self {
            code,
            message,
            retryable: false,
            evidence_ref: None,
        })
    }

    /// Marks this error as one the caller may safely retry the whole
    /// operation for (e.g. a transient transport busy condition), as opposed
    /// to one that requires a different plan/approval/target.
    #[must_use]
    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }

    #[must_use]
    pub fn with_evidence_ref(mut self, evidence_ref: impl Into<String>) -> Self {
        self.evidence_ref = Some(evidence_ref.into());
        self
    }
}

impl fmt::Display for ConnectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ConnectorError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_code_or_message() {
        assert!(ConnectorError::new("", "message").is_err());
        assert!(ConnectorError::new("code", "").is_err());
    }

    #[test]
    fn builder_methods_set_optional_fields() {
        let err = ConnectorError::new("transport_busy", "port in use")
            .expect("valid")
            .retryable()
            .with_evidence_ref("evidence://123");
        assert!(err.retryable);
        assert_eq!(err.evidence_ref.as_deref(), Some("evidence://123"));
        assert_eq!(err.to_string(), "[transport_busy] port in use");
    }
}
