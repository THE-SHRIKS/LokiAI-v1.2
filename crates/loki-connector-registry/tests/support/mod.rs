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
//! proven again here for `CON-002`'s own orchestration logic.
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

use std::num::NonZeroU32;
use std::time::SystemTime;

use async_trait::async_trait;
use loki_connectors_sdk::Connector;
use loki_connectors_sdk::capability::CapabilityDescriptor;
use loki_connectors_sdk::error::ConnectorError;
use loki_connectors_sdk::health::{HealthCheckEntry, HealthReport};
use loki_connectors_sdk::ids::{CapabilityId, ConnectorId};
use loki_connectors_sdk::manifest::{ConnectorManifest, Provenance};
use loki_connectors_sdk::operation::{OperationContext, OperationRequest, OperationStage};
use loki_connectors_sdk::outcome::{StageEvidence, StageFailure, StageOutcome, StageTimeout};
use loki_connectors_sdk::vocabulary::{
    CapabilityRiskLevel, DeviceLifecycleState, HealthCheckStatus,
};
use semver::{Version, VersionReq};

/// What a lifecycle stage should do when invoked — identical shape to
/// `loki-connectors-sdk`'s own `MockConnector::Behavior`, reproduced here
/// rather than imported because it is private to that crate's tests.
#[derive(Debug, Clone)]
pub enum Behavior {
    Succeed,
    Fail(String),
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
            Behavior::Timeout => StageOutcome::Timeout(StageTimeout {
                stage,
                observed_at: SystemTime::now(),
                deadline: SystemTime::now(),
            }),
        }
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
    preflight: Behavior,
    execute: Behavior,
    verify: Behavior,
    recover: Behavior,
    health: HealthBehavior,
}

impl TestConnector {
    /// A connector that registers cleanly, identifies successfully
    /// (preflight/execute/verify all succeed), and reports all-passed
    /// health by default.
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
            preflight: Behavior::Succeed,
            execute: Behavior::Succeed,
            verify: Behavior::Succeed,
            recover: Behavior::Succeed,
            health: HealthBehavior::AllPassed,
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
    pub fn with_identify_verify(mut self, behavior: Behavior) -> Self {
        self.verify = behavior;
        self
    }

    #[must_use]
    pub fn with_identify_execute(mut self, behavior: Behavior) -> Self {
        self.execute = behavior;
        self
    }

    #[must_use]
    pub fn with_health(mut self, behavior: HealthBehavior) -> Self {
        self.health = behavior;
        self
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

    async fn verify(&mut self, _op: &OperationRequest, _ctx: &OperationContext) -> StageOutcome {
        self.verify.resolve(OperationStage::Verify)
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
