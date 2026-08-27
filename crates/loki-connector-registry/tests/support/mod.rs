//! `TestConnector` — this crate's own connector test double, implementing
//! `loki_connectors_sdk::Connector` for real (not a hand-rolled fake that
//! only mimics discovery output without exercising the trait). Mirrors the
//! same shape `crates/loki-connectors-sdk/tests/support/mod.rs`'s
//! `MockConnector` already established, for the same reason: `CON-001`'s own
//! `MockConnector` is deliberately private to its own crate's tests (its
//! task packet: "used only by this crate's own tests"), and this node's
//! packet forbids touching `crates/loki-connectors-sdk` to make it public.
//! So this registry gets its own connector implementation to register and
//! orchestrate against — the same "real trait dispatch, not fabricated
//! discovery/health data" property `MockConnector` proves for `CON-001`,
//! proven again here for `CON-002-R1`'s own candidate/identity/reconciliation
//! logic.
//!
//! `CON-002-R1` extends this double (over its `CON-002` shape) with:
//! - `Behavior::FailWithCode` — a configurable connector-reported error
//!   `code`, so tests can exercise this crate's own busy/permission/
//!   disconnect/unsupported classification convention
//!   (`crate::discovery::DiscoveryFailureClass`).
//! - Runtime-mutable stage behavior via [`BehaviorSlot`] (a small
//!   `Arc<Mutex<Behavior>>` wrapper), so a test can flip a connector from
//!   "busy" to "recovered" between two discovery cycles against the *same*
//!   registered connector instance — exactly the "later cycle recovers once
//!   cleared" scenario the task packet's required verification table names.
//! - Per-target identity evidence
//!   (`with_default_identity_evidence`/`set_identity_evidence_for_target`),
//!   so a connector's `verify` stage can attach real
//!   `loki_connectors_sdk::identity::DeviceIdentityEvidence` to its
//!   `StageEvidence` — looked up by the provisional `DeviceId` the
//!   `identify` request was addressed to (i.e. the candidate's ephemeral
//!   id), so two different candidates against the same connector instance
//!   can report two different (or the same) fingerprint digest, exercising
//!   both the duplicate-candidate-reconciles and
//!   conflicting-digest-fails-closed scenarios without needing two separate
//!   connector instances.
//!
//! Included from both this crate's integration tests (`tests/*.rs`, via
//! `mod support;`) and its own `src/`-level unit tests (via
//! `src/lib.rs`'s `#[path = "../tests/support/mod.rs"] mod test_support;`)
//! so both layers exercise the exact same connector implementation, never
//! two subtly different doubles.
//!
//! This file is compiled independently as two separate binaries (the
//! `src/`-level unit-test binary and each `tests/*.rs` integration-test
//! binary) — each one only exercises the subset of this double's API its
//! own tests happen to need, so `dead_code` is expected and allowed here
//! rather than papered over by adding artificial call sites for coverage
//! this double intentionally over-provides.
#![allow(dead_code)]

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::SystemTime;

use async_trait::async_trait;
use loki_connectors_sdk::Connector;
use loki_connectors_sdk::capability::CapabilityDescriptor;
use loki_connectors_sdk::error::ConnectorError;
use loki_connectors_sdk::health::{HealthCheckEntry, HealthReport};
use loki_connectors_sdk::identity::{
    DeviceIdentityEvidence, DeviceIdentityEvidenceInput, IdentityConfidence,
    SensitiveUnstableLocator, TransportCandidate,
};
use loki_connectors_sdk::ids::{CapabilityId, ConnectorId};
use loki_connectors_sdk::manifest::{ConnectorManifest, Provenance};
use loki_connectors_sdk::operation::{OperationContext, OperationRequest, OperationStage};
use loki_connectors_sdk::outcome::{StageEvidence, StageFailure, StageOutcome, StageTimeout};
use loki_connectors_sdk::vocabulary::{
    CapabilityRiskLevel, DeviceLifecycleState, HealthCheckStatus,
};
use semver::{Version, VersionReq};

/// 32-byte fixture installation key. Real installation keys are opaque,
/// OS-protected secrets (`Docs/ARCHITECTURE.md`); this crate never
/// generates one, so tests use a fixed, clearly-fake value, matching the
/// precedent `crates/loki-connectors-sdk/src/identity.rs`'s own tests
/// already established.
pub const FIXTURE_INSTALLATION_KEY: &[u8; 32] = b"fixture-installation-key-32bytes";

/// Builds a real, validated [`DeviceIdentityEvidence`] fixture — the shape a
/// connector's own `identify` implementation would build internally (see
/// `crates/loki-connectors-sdk/src/outcome.rs`'s `StageEvidence.identity_evidence`
/// doc comment) before attaching it to its `verify` stage's `StageEvidence`.
#[must_use]
pub fn identity_evidence(connector_id: &str, fingerprint_material: &str) -> DeviceIdentityEvidence {
    DeviceIdentityEvidence::new(DeviceIdentityEvidenceInput {
        evidence_type: "hardware_uid".into(),
        fingerprint_material: fingerprint_material.into(),
        installation_key: FIXTURE_INSTALLATION_KEY,
        confidence: IdentityConfidence::Strong,
        connector_id: connector_id.into(),
        connector_version: "1.0.0".into(),
        observed_at: "2026-08-26T12:00:00.000Z".into(),
    })
    .expect("fixture evidence input is always valid")
}

/// Builds a real, validated [`TransportCandidate`] fixture for one
/// connector/candidate/locator combination.
#[must_use]
pub fn candidate_for(
    connector_id: &str,
    candidate_id: &str,
    locator_value: &str,
) -> TransportCandidate {
    TransportCandidate::new(
        candidate_id,
        connector_id,
        "1.0.0",
        "fixture_transport",
        SensitiveUnstableLocator::new(locator_value).expect("fixture locator is always valid"),
        "2026-08-26T12:00:00.000Z",
    )
    .expect("fixture candidate is always valid")
}

/// What a lifecycle stage should do when invoked — identical shape to
/// `loki-connectors-sdk`'s own `MockConnector::Behavior`, reproduced here
/// rather than imported because it is private to that crate's tests.
#[derive(Debug, Clone)]
pub enum Behavior {
    Succeed,
    Fail(String),
    /// Like `Fail`, but with a caller-chosen `code` instead of this double's
    /// default `"test_double_configured_failure"` — used to exercise this
    /// crate's own busy/permission/disconnect/unsupported classification
    /// convention (`crate::discovery::DiscoveryFailureClass::classify`).
    FailWithCode(&'static str, String),
    Timeout,
}

impl Behavior {
    fn resolve(&self, stage: OperationStage) -> StageOutcome {
        match self {
            Behavior::Succeed => StageOutcome::Success(
                StageEvidence::new(
                    format!("{stage} succeeded (test double)"),
                    SystemTime::now(),
                )
                .expect("literal summary is always valid"),
            ),
            Behavior::Fail(reason) => StageOutcome::Failure(
                ConnectorError::new("test_double_configured_failure", reason.clone())
                    .expect("Behavior::Fail is only constructed with a non-empty reason"),
            ),
            Behavior::FailWithCode(code, reason) => StageOutcome::Failure(
                ConnectorError::new(*code, reason.clone())
                    .expect("Behavior::FailWithCode is only constructed with valid code/reason"),
            ),
            Behavior::Timeout => StageOutcome::Timeout(StageTimeout {
                stage,
                observed_at: SystemTime::now(),
                deadline: SystemTime::now(),
            }),
        }
    }
}

/// A runtime-mutable [`Behavior`] slot, so a test can change how a
/// *currently registered* connector responds between two discovery/health
/// cycles (e.g. "busy, then cleared") without re-registering a new
/// connector instance.
#[derive(Clone)]
pub struct BehaviorSlot(Arc<StdMutex<Behavior>>);

impl BehaviorSlot {
    fn new(initial: Behavior) -> Self {
        Self(Arc::new(StdMutex::new(initial)))
    }

    fn resolve(&self, stage: OperationStage) -> StageOutcome {
        self.0
            .lock()
            .expect("test double mutex is never poisoned")
            .resolve(stage)
    }

    /// Changes what this slot resolves to on every subsequent call.
    pub fn set(&self, behavior: Behavior) {
        *self.0.lock().expect("test double mutex is never poisoned") = behavior;
    }
}

/// Whether `health_check` should report a clean, all-passed report or one
/// with a `Failed`/`Warning` entry — used by the registry's health-cycle
/// tests to prove an unhealthy connector is reflected, not masked.
#[derive(Debug, Clone, Copy)]
pub enum HealthBehavior {
    AllPassed,
    OneFailed,
    OneWarning,
}

pub struct TestConnector {
    manifest: ConnectorManifest,
    capabilities: Vec<CapabilityDescriptor>,
    preflight: BehaviorSlot,
    execute: BehaviorSlot,
    verify: BehaviorSlot,
    recover: BehaviorSlot,
    health: HealthBehavior,
    /// Identity evidence attached to a successful `verify`'s `StageEvidence`,
    /// looked up by the provisional `DeviceId` the request was addressed to
    /// (i.e. `crate::discovery::ObservedCandidate::candidate_id`, echoed
    /// back via `OperationRequest::target::device_id`). Falls back to
    /// `default_identity_evidence` when no per-target entry matches, so the
    /// common single-candidate case only needs one call to configure.
    identity_evidence_by_target: HashMap<String, DeviceIdentityEvidence>,
    default_identity_evidence: Option<DeviceIdentityEvidence>,
}

impl TestConnector {
    /// A connector that registers cleanly, identifies successfully
    /// (preflight/execute/verify all succeed), and reports all-passed
    /// health by default. Does not attach identity evidence to its `verify`
    /// stage unless `with_default_identity_evidence`/
    /// `set_identity_evidence_for_target` configures it — a `verify` success
    /// without identity evidence is itself a real, honest scenario this
    /// crate's `interpret_identify_outcome` must handle (`Unknown`, not
    /// fabricated `Verified`).
    #[must_use]
    pub fn identifiable(connector_id: &str) -> Self {
        let identify = CapabilityDescriptor::new(
            CapabilityId::new("identify").expect("valid slug"),
            "schema://test/identify",
            "1.0.0",
            CapabilityRiskLevel::Low,
            vec![DeviceLifecycleState::Discovered],
            NonZeroU32::new(5_000).expect("nonzero"),
        )
        .expect("valid capability descriptor")
        .with_verification_required(true);

        let manifest = ConnectorManifest::new(
            ConnectorId::new(connector_id).expect("valid connector id"),
            Version::parse("1.0.0").expect("valid semver"),
            "LokiAI",
            Provenance::new(
                "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                "in-memory-test-double",
            )
            .expect("valid provenance"),
            VersionReq::parse(">=1.0.0, <2.0.0").expect("valid range"),
        )
        .expect("valid manifest")
        .with_capability(identify.clone());

        Self {
            manifest,
            capabilities: vec![identify],
            preflight: BehaviorSlot::new(Behavior::Succeed),
            execute: BehaviorSlot::new(Behavior::Succeed),
            verify: BehaviorSlot::new(Behavior::Succeed),
            recover: BehaviorSlot::new(Behavior::Succeed),
            health: HealthBehavior::AllPassed,
            identity_evidence_by_target: HashMap::new(),
            default_identity_evidence: None,
        }
    }

    /// A connector that registers fine but whose `health_check` reports a
    /// `Failed` entry — for proving an unhealthy connector is reflected
    /// honestly by the registry's health orchestration, not silently
    /// upgraded to healthy.
    #[must_use]
    pub fn unhealthy(connector_id: &str) -> Self {
        Self {
            health: HealthBehavior::OneFailed,
            ..Self::identifiable(connector_id)
        }
    }

    #[must_use]
    pub fn with_identify_verify(self, behavior: Behavior) -> Self {
        self.verify.set(behavior);
        self
    }

    pub fn set_identify_verify(&mut self, behavior: Behavior) {
        self.verify.set(behavior);
    }

    #[must_use]
    pub fn with_identify_execute(self, behavior: Behavior) -> Self {
        self.execute.set(behavior);
        self
    }

    pub fn set_preflight(&mut self, behavior: Behavior) {
        self.preflight.set(behavior);
    }

    #[must_use]
    pub fn with_health(mut self, behavior: HealthBehavior) -> Self {
        self.health = behavior;
        self
    }

    /// A shareable handle to this connector's `preflight` behavior, so a
    /// test can mutate it (e.g. "busy" -> "cleared") after the connector has
    /// already been registered (registration moves the `TestConnector` into
    /// the registry's own storage).
    #[must_use]
    pub fn preflight_slot(&self) -> BehaviorSlot {
        self.preflight.clone()
    }

    /// A shareable handle to this connector's `verify` behavior.
    #[must_use]
    pub fn verify_slot(&self) -> BehaviorSlot {
        self.verify.clone()
    }

    /// Sets the identity evidence attached to every successful `verify`
    /// call whose target has no more specific per-target entry — the common
    /// single-candidate case.
    #[must_use]
    pub fn with_default_identity_evidence(mut self, evidence: DeviceIdentityEvidence) -> Self {
        self.default_identity_evidence = Some(evidence);
        self
    }

    pub fn set_default_identity_evidence(&mut self, evidence: DeviceIdentityEvidence) {
        self.default_identity_evidence = Some(evidence);
    }

    /// Sets the identity evidence attached to a successful `verify` call
    /// whose request was addressed to the given ephemeral candidate id
    /// (`crate::discovery::ObservedCandidate::candidate_id`) — lets one
    /// connector instance report *different* evidence for different
    /// candidates within one discovery cycle (duplicate-candidate and
    /// conflicting-digest scenarios).
    pub fn set_identity_evidence_for_target(
        &mut self,
        candidate_id: &str,
        evidence: DeviceIdentityEvidence,
    ) {
        self.identity_evidence_by_target
            .insert(candidate_id.to_string(), evidence);
    }
}

#[async_trait]
impl Connector for TestConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn capabilities(&self) -> &[CapabilityDescriptor] {
        &self.capabilities
    }

    async fn health_check(&mut self, _ctx: &OperationContext) -> HealthReport {
        let now = SystemTime::now();
        let entries = match self.health {
            HealthBehavior::AllPassed => vec![
                HealthCheckEntry::new(
                    "test.check",
                    "1",
                    "general",
                    HealthCheckStatus::Passed,
                    None,
                    now,
                    now,
                )
                .expect("valid entry"),
            ],
            HealthBehavior::OneFailed => vec![
                HealthCheckEntry::new(
                    "test.check",
                    "1",
                    "general",
                    HealthCheckStatus::Failed,
                    Some("test double configured to report failure".to_string()),
                    now,
                    now,
                )
                .expect("valid entry"),
            ],
            HealthBehavior::OneWarning => vec![
                HealthCheckEntry::new(
                    "test.check",
                    "1",
                    "general",
                    HealthCheckStatus::Warning,
                    Some("test double configured to report a warning".to_string()),
                    now,
                    now,
                )
                .expect("valid entry"),
            ],
        };
        HealthReport::new(entries, now)
    }

    async fn preflight(&mut self, _op: &OperationRequest, _ctx: &OperationContext) -> StageOutcome {
        self.preflight.resolve(OperationStage::Preflight)
    }

    async fn execute(&mut self, _op: &OperationRequest, _ctx: &OperationContext) -> StageOutcome {
        self.execute.resolve(OperationStage::Execute)
    }

    async fn verify(&mut self, op: &OperationRequest, _ctx: &OperationContext) -> StageOutcome {
        match self.verify.resolve(OperationStage::Verify) {
            StageOutcome::Success(evidence) => {
                let target_key = op.target.device_id.as_str();
                let identity = self
                    .identity_evidence_by_target
                    .get(target_key)
                    .cloned()
                    .or_else(|| self.default_identity_evidence.clone());
                let evidence = match identity {
                    Some(identity) => evidence.with_identity_evidence(identity),
                    None => evidence,
                };
                StageOutcome::Success(evidence)
            }
            other => other,
        }
    }

    async fn recover(
        &mut self,
        _op: &OperationRequest,
        _ctx: &OperationContext,
        _failure: &StageFailure,
    ) -> StageOutcome {
        self.recover.resolve(OperationStage::Recover)
    }
}
