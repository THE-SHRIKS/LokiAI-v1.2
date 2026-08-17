//! Validated identifier newtypes shared across this crate's public API.
//!
//! None of these *mint*/generate identifiers. `Docs/CONNECTOR_SPEC.md` §4
//! says a connector *receives* "stable job context" and "validated ...
//! inputs" from the runtime; identifier generation is the runtime/job
//! engine's responsibility (`Docs/ARCHITECTURE.md` §10: "persist job"
//! happens before preflight/execute, and issues the durable job's identity).
//! These newtypes only give each identifier its own Rust type — so, e.g., a
//! `CorrelationId` cannot be passed where an `OperationId` is expected — and
//! validate the property every identifier textually must have, per
//! `AGENTS.md` rule 4 ("Types alone are not runtime validation.") and rule 5
//! ("Use stable IDs ... correlation IDs, and idempotency keys").
//!
//! Two validation strengths are used, deliberately:
//!
//! - [`ConnectorId`], [`OperationId`], [`CorrelationId`], [`IdempotencyKey`],
//!   and [`DeviceId`] validate only non-emptiness and a conservative length
//!   bound. `Docs/CONNECTOR_SPEC.md` requires each of these concepts to
//!   exist but never specifies a charset for them, so this crate does not
//!   invent one — doing so would be an undocumented, fabricated constraint a
//!   real runtime-issued id could later violate for no product reason.
//! - [`CapabilityId`] validates the exact shape
//!   `packages/contracts/src/connector.ts`'s `ConnectorCapabilityIdSchema`
//!   defines (`^[a-z][a-z0-9_]*$`, 1–64 bytes), because that *is* a
//!   documented, precise rule this crate mirrors rather than invents. See
//!   `Docs/CONNECTOR_SPEC.md` §6.

use std::fmt;

use crate::error::ValidationError;
use crate::validate::non_empty_bounded;

/// Stable connector package identifier (`Docs/CONNECTOR_SPEC.md` §3
/// "connector ID and semantic version").
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectorId(String);

impl ConnectorId {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        non_empty_bounded("connector_id", &value, 128)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConnectorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Stable identifier for one durable operation/job instance
/// (`Docs/CONNECTOR_SPEC.md` §8's declared operation; `Docs/ARCHITECTURE.md`
/// §10's durable job). Issued by the runtime, not this crate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperationId(String);

impl OperationId {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        non_empty_bounded("operation_id", &value, 128)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Cross-component correlation id for one request/operation, per
/// `AGENTS.md` rule 5. Issued by the runtime, threaded through unchanged so
/// evidence/logs from this operation can be joined across process
/// boundaries.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CorrelationId(String);

impl CorrelationId {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        non_empty_bounded("correlation_id", &value, 128)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Idempotency key for an operation that may be retried, per `AGENTS.md`
/// rule 5. A connector must treat re-invocation with the same key as the
/// same logical operation, not a new one — enforcing that is the runtime's
/// job (it decides whether to re-dispatch); this type only carries the
/// value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        non_empty_bounded("idempotency_key", &value, 200)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Stable device identity (`Docs/CONNECTOR_SPEC.md` §5: "COM port, IP
/// address, BLE address, or probe index is not stable identity"). This type
/// cannot itself detect a caller passing a transport address instead of a
/// real stable id — that is a semantic rule, not a shape rule — but its
/// distinct type at least stops a raw transport-address `String` from being
/// used purely by accident where a `TargetIdentity` field is expected.
/// Real identity evidence (method, confidence, duplicate reconciliation) is
/// `CON-002` (discovery) scope, not this crate's.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        non_empty_bounded("device_id", &value, 128)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `Docs/CONNECTOR_SPEC.md` §6's initial, explicitly non-exhaustive
/// capability vocabulary ("Initial vocabulary may include"), decomposed to
/// slug form exactly as `packages/contracts/src/connector.ts`'s
/// `KNOWN_CONNECTOR_CAPABILITY_IDS` does (slash-combined bullets such as
/// "stream/query declared telemetry" become two entries). This is a
/// reference/convenience list, not a closed set — [`CapabilityId::new`]
/// accepts any value with the right shape, not only these sixteen.
pub const KNOWN_CAPABILITY_IDS: &[&str] = &[
    "identify",
    "read_health",
    "stream_telemetry",
    "query_telemetry",
    "read_logs",
    "run_diagnostics",
    "run_preflight",
    "build_artifact",
    "transfer_artifact",
    "flash_artifact",
    "restart",
    "run_test_suite",
    "update_configuration",
    "deploy_model",
    "verify_artifact",
    "rollback",
];

/// Capability identifier. Mirrors
/// `packages/contracts/src/connector.ts`'s `ConnectorCapabilityIdSchema`
/// exactly: a lowercase snake_case slug, 1–64 bytes, matching
/// `^[a-z][a-z0-9_]*$`. See `Docs/CONNECTOR_SPEC.md` §6.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilityId(String);

impl CapabilityId {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        if value.is_empty() || value.len() > 64 {
            return Err(ValidationError::new(
                "capability_id",
                format!("must be 1-64 bytes (was {})", value.len()),
            ));
        }
        if !is_lower_snake_slug(&value) {
            return Err(ValidationError::new(
                "capability_id",
                "must be a lowercase snake_case slug matching ^[a-z][a-z0-9_]*$",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True if this id is one of `Docs/CONNECTOR_SPEC.md` §6's named
    /// examples. A `false` result is not an error — §6's vocabulary is
    /// explicitly non-exhaustive ("may include") — this is informational
    /// only.
    pub fn is_known(&self) -> bool {
        KNOWN_CAPABILITY_IDS.contains(&self.0.as_str())
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn is_lower_snake_slug(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_id_rejects_empty() {
        assert!(ConnectorId::new("").is_err());
        assert!(ConnectorId::new("esp32-s3-ref").is_ok());
    }

    #[test]
    fn connector_id_allows_any_charset_since_spec_defines_none() {
        // Deliberately not restricted to a fabricated charset.
        assert!(ConnectorId::new("com.lokiai.esp32-s3/v1").is_ok());
    }

    #[test]
    fn capability_id_accepts_known_examples() {
        for id in KNOWN_CAPABILITY_IDS {
            let parsed = CapabilityId::new(*id).expect("known ids are valid slugs");
            assert!(parsed.is_known());
        }
    }

    #[test]
    fn capability_id_rejects_bad_shape() {
        assert!(CapabilityId::new("").is_err(), "empty");
        assert!(CapabilityId::new("Read_Health").is_err(), "uppercase");
        assert!(CapabilityId::new("1read_health").is_err(), "leading digit");
        assert!(
            CapabilityId::new("read-health").is_err(),
            "hyphen not underscore"
        );
        assert!(
            CapabilityId::new("a".repeat(65)).is_err(),
            "over max length"
        );
    }

    #[test]
    fn capability_id_accepts_undeclared_but_well_shaped_ids() {
        // Docs/CONNECTOR_SPEC.md §6: vocabulary "may include" — not closed.
        let custom = CapabilityId::new("read_ambient_light").expect("well-shaped slug");
        assert!(!custom.is_known());
    }

    #[test]
    fn ids_display_their_inner_value() {
        assert_eq!(OperationId::new("op-1").unwrap().to_string(), "op-1");
        assert_eq!(CorrelationId::new("corr-1").unwrap().to_string(), "corr-1");
        assert_eq!(IdempotencyKey::new("idem-1").unwrap().to_string(), "idem-1");
        assert_eq!(DeviceId::new("dev-1").unwrap().to_string(), "dev-1");
    }
}
