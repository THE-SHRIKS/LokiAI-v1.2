//! Builds this connector's six declared [`CapabilityDescriptor`]s —
//! `Docs/CONNECTOR_SPEC.md` §6, scoped by this node's packet header to
//! exactly `identify, read_health, restart, flash_artifact, verify_artifact,
//! rollback` (excludes `build_artifact`, `BLD-001`/`ART-001` scope).
//!
//! Every risk/required-state/exclusive-access/expected-interruption/
//! timeout/verification/recovery value below is this connector's own
//! considered judgment for the confirmed ESP32-S3 profile (`Docs/CONTEXT.md`
//! OQ-001), documented per field rather than left to guesswork — see each
//! capability's own comment for the specific reasoning, especially
//! `rollback`'s `recovery_supported: false` (open question 2: no
//! known-good-artifact/recovery-access policy exists yet — see
//! `src/connector.rs`'s `dispatch_rollback` module doc).

use std::num::NonZeroU32;

use loki_connectors_sdk::capability::CapabilityDescriptor;
use loki_connectors_sdk::ids::CapabilityId;
use loki_connectors_sdk::vocabulary::{CapabilityRiskLevel, DeviceLifecycleState};

fn capability(
    id: &str,
    schema_ref: &str,
    risk: CapabilityRiskLevel,
    required_state: Vec<DeviceLifecycleState>,
    timeout_ms: u32,
) -> CapabilityDescriptor {
    CapabilityDescriptor::new(
        CapabilityId::new(id).expect("literal capability id is a valid slug"),
        schema_ref,
        "1.0.0",
        risk,
        required_state,
        NonZeroU32::new(timeout_ms).expect("literal timeout is nonzero"),
    )
    .expect("literal capability descriptor inputs are valid")
}

/// Builds all six capability descriptors, in the order the manifest
/// declares them.
pub fn build_capabilities() -> Vec<CapabilityDescriptor> {
    vec![
        // `identify`: read-only chip-ID interrogation. Runs on a
        // just-discovered candidate, before any stable state is assigned —
        // `Discovered` is the only state that makes sense as a precondition.
        // Low risk (no device mutation), not exclusive (multiple identify
        // calls do not conflict with each other the way a flash does), no
        // interruption expected, no recovery: a failed identify has nothing
        // to roll back.
        capability(
            "identify",
            "schema://loki-connector-esp32-s3/identify",
            CapabilityRiskLevel::Low,
            vec![DeviceLifecycleState::Discovered],
            5_000,
        )
        .with_verification_required(true),
        // `read_health`: read-only, low risk, usable whenever the device is
        // reachable in a non-busy state.
        capability(
            "read_health",
            "schema://loki-connector-esp32-s3/read_health",
            CapabilityRiskLevel::Low,
            vec![DeviceLifecycleState::Ready, DeviceLifecycleState::Degraded],
            5_000,
        )
        .with_verification_required(true),
        // `restart`: device-changing (interrupts whatever the device was
        // doing), medium risk (reversible, no data loss, but the device is
        // briefly unreachable), exclusive (must not race a concurrent
        // flash/rollback), verified via post-restart re-identification, and
        // has a real (if best-effort) recovery attempt — see
        // `dispatch_restart_recover`.
        capability(
            "restart",
            "schema://loki-connector-esp32-s3/restart",
            CapabilityRiskLevel::Medium,
            vec![DeviceLifecycleState::Ready, DeviceLifecycleState::Degraded],
            15_000,
        )
        .with_exclusive_access(true)
        .with_expected_interruption(true)
        .with_verification_required(true)
        .with_recovery_supported(true),
        // `flash_artifact`: the highest-risk capability — rewrites device
        // flash. Exclusive, expects interruption (device reboots into and
        // out of bootloader mode), always verified (digest/readback), and
        // has a real (if best-effort, non-guaranteed) recovery attempt.
        // Usable from `Ready` (normal update) or `Degraded` (recovery
        // re-flash).
        capability(
            "flash_artifact",
            "schema://loki-connector-esp32-s3/flash_artifact",
            CapabilityRiskLevel::High,
            vec![DeviceLifecycleState::Ready, DeviceLifecycleState::Degraded],
            120_000,
        )
        .with_exclusive_access(true)
        .with_expected_interruption(true)
        .with_verification_required(true)
        .with_recovery_supported(true),
        // `verify_artifact`: read-only re-confirmation of what is currently
        // flashed. Low risk, not exclusive, no interruption, no recovery (a
        // failed read has nothing to roll back).
        capability(
            "verify_artifact",
            "schema://loki-connector-esp32-s3/verify_artifact",
            CapabilityRiskLevel::Low,
            vec![DeviceLifecycleState::Ready, DeviceLifecycleState::Degraded],
            30_000,
        )
        .with_verification_required(true),
        // `rollback`: structurally declared and dispatchable (this node's
        // acceptance criteria require that), but this connector has no
        // known-good-artifact/recovery-access policy to execute against
        // (open question 2 — not silently resolved). `recovery_supported:
        // false` is this capability's own honest declaration that a failed
        // rollback attempt has no further recovery mechanism behind it,
        // matching what `dispatch_rollback` actually does (preflight always
        // fails with a policy-gap reason, never fabricating a rollback this
        // connector cannot honestly perform).
        capability(
            "rollback",
            "schema://loki-connector-esp32-s3/rollback",
            CapabilityRiskLevel::High,
            vec![DeviceLifecycleState::Degraded, DeviceLifecycleState::Ready],
            60_000,
        )
        .with_exclusive_access(true)
        .with_expected_interruption(true)
        .with_verification_required(true)
        .with_recovery_supported(false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declares_exactly_the_six_scoped_capabilities() {
        let ids: Vec<String> = build_capabilities()
            .into_iter()
            .map(|c| c.capability_id.as_str().to_string())
            .collect();
        assert_eq!(
            ids,
            vec![
                "identify",
                "read_health",
                "restart",
                "flash_artifact",
                "verify_artifact",
                "rollback",
            ]
        );
    }

    #[test]
    fn no_capability_declares_build_artifact() {
        assert!(
            build_capabilities()
                .iter()
                .all(|c| c.capability_id.as_str() != "build_artifact")
        );
    }

    #[test]
    fn rollback_is_dispatchable_but_declares_no_recovery_of_its_own_failure() {
        let rollback = build_capabilities()
            .into_iter()
            .find(|c| c.capability_id.as_str() == "rollback")
            .expect("rollback is declared");
        assert!(!rollback.recovery_supported);
        assert!(rollback.verification_required);
    }
}
