//! Capability descriptor — `Docs/CONNECTOR_SPEC.md` §6: "An advertised
//! capability includes schema, risk, required state, exclusive-access mode,
//! expected interruption, timeout, verification, and recovery contract."
//!
//! This is a field-for-field mirror of
//! `packages/contracts/src/connector.ts`'s `ConnectorCapabilityDescriptor`
//! (that module's own doc comment names this shape as vocabulary "CON-001
//! builds on"), kept flat (not regrouped into sub-structs) specifically so
//! it stays trivially diffable against the TS source:
//!
//! | §6 concept              | TS field                          | Rust field              |
//! |--------------------------|------------------------------------|--------------------------|
//! | schema                   | `schema_ref` + `schema_version`    | same                    |
//! | risk                     | `risk`                             | same                    |
//! | required state           | `required_state`                   | same                    |
//! | exclusive-access mode     | `exclusive_access`                 | same                    |
//! | expected interruption     | `expected_interruption`            | same                    |
//! | timeout                  | `timeout_ms: number` (`.positive()`) | `timeout_ms: NonZeroU32` (stronger: compile-time-enforced positivity, not just a runtime check) |
//! | verification contract     | `verification_required`            | same                    |
//! | recovery contract         | `recovery_supported`               | same                    |

use std::num::NonZeroU32;

use crate::error::ValidationError;
use crate::ids::CapabilityId;
use crate::validate::non_empty_bounded;
use crate::vocabulary::{CapabilityRiskLevel, DeviceLifecycleState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDescriptor {
    pub capability_id: CapabilityId,
    pub schema_ref: String,
    pub schema_version: String,
    pub risk: CapabilityRiskLevel,
    pub required_state: Vec<DeviceLifecycleState>,
    pub exclusive_access: bool,
    pub expected_interruption: bool,
    pub timeout_ms: NonZeroU32,
    pub verification_required: bool,
    pub recovery_supported: bool,
}

impl CapabilityDescriptor {
    /// `schema_ref` (1-200 bytes) and `schema_version` (1-50 bytes) bounds
    /// match `connector.ts`'s `z.string().min(1).max(200)` /
    /// `.min(1).max(50)` exactly. `required_state` must be non-empty,
    /// matching `.array(...).min(1)`.
    pub fn new(
        capability_id: CapabilityId,
        schema_ref: impl Into<String>,
        schema_version: impl Into<String>,
        risk: CapabilityRiskLevel,
        required_state: Vec<DeviceLifecycleState>,
        timeout_ms: NonZeroU32,
    ) -> Result<Self, ValidationError> {
        let schema_ref = schema_ref.into();
        non_empty_bounded("schema_ref", &schema_ref, 200)?;
        let schema_version = schema_version.into();
        non_empty_bounded("schema_version", &schema_version, 50)?;
        if required_state.is_empty() {
            return Err(ValidationError::new(
                "required_state",
                "must declare at least one required device-lifecycle state (Docs/CONNECTOR_SPEC.md §6)",
            ));
        }
        Ok(Self {
            capability_id,
            schema_ref,
            schema_version,
            risk,
            required_state,
            exclusive_access: false,
            expected_interruption: false,
            timeout_ms,
            verification_required: false,
            recovery_supported: false,
        })
    }

    #[must_use]
    pub fn with_exclusive_access(mut self, exclusive_access: bool) -> Self {
        self.exclusive_access = exclusive_access;
        self
    }

    #[must_use]
    pub fn with_expected_interruption(mut self, expected_interruption: bool) -> Self {
        self.expected_interruption = expected_interruption;
        self
    }

    #[must_use]
    pub fn with_verification_required(mut self, verification_required: bool) -> Self {
        self.verification_required = verification_required;
        self
    }

    #[must_use]
    pub fn with_recovery_supported(mut self, recovery_supported: bool) -> Self {
        self.recovery_supported = recovery_supported;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_id() -> CapabilityId {
        CapabilityId::new("flash_artifact").unwrap()
    }

    #[test]
    fn constructs_with_documented_defaults() {
        let d = CapabilityDescriptor::new(
            valid_id(),
            "schema://flash_artifact",
            "1.0.0",
            CapabilityRiskLevel::High,
            vec![DeviceLifecycleState::Ready],
            NonZeroU32::new(60_000).unwrap(),
        )
        .expect("valid inputs");
        assert!(!d.exclusive_access);
        assert!(!d.expected_interruption);
        assert!(!d.verification_required);
        assert!(!d.recovery_supported);
    }

    #[test]
    fn rejects_empty_required_state() {
        let result = CapabilityDescriptor::new(
            valid_id(),
            "schema://x",
            "1.0.0",
            CapabilityRiskLevel::Low,
            vec![],
            NonZeroU32::new(1_000).unwrap(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn rejects_oversized_schema_ref() {
        let result = CapabilityDescriptor::new(
            valid_id(),
            "x".repeat(201),
            "1.0.0",
            CapabilityRiskLevel::Low,
            vec![DeviceLifecycleState::Ready],
            NonZeroU32::new(1_000).unwrap(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn builder_methods_flip_flags_independently() {
        let d = CapabilityDescriptor::new(
            valid_id(),
            "schema://x",
            "1.0.0",
            CapabilityRiskLevel::Medium,
            vec![DeviceLifecycleState::Ready],
            NonZeroU32::new(5_000).unwrap(),
        )
        .unwrap()
        .with_exclusive_access(true)
        .with_verification_required(true);
        assert!(d.exclusive_access);
        assert!(d.verification_required);
        assert!(!d.expected_interruption);
        assert!(!d.recovery_supported);
    }
}
