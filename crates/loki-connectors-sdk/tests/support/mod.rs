//! `MockConnector` — the minimal in-memory fake connector proving
//! `loki_connectors_sdk::Connector` is actually implementable and testable
//! (Deliverable 4 of `Docs/graph/tasks/CON-001.md`).
//!
//! Deliberately **not** part of `loki-connectors-sdk`'s published library
//! surface (see `src/lib.rs`'s module map note) — per the task packet, this
//! is "used only by this crate's own tests", so it lives here under
//! `tests/support/`, mirroring the exact pattern `crates/loki-runtime`
//! already established (`tests/process_behavior.rs` +
//! `tests/support/mod.rs`).
//!
//! Every stage (`preflight`/`execute`/`verify`/`recover`) is independently
//! configurable via [`Behavior`], and every call is counted, so a test can
//! assert not only *what* [`loki_connectors_sdk::run_operation`] returned
//! but *whether a stage method was invoked at all* — the strongest available
//! proof that, e.g., a preflight failure really does short-circuit
//! `execute` at the mock-instance level, not only "by construction" of
//! `run_operation`'s source.

use std::num::NonZeroU32;
use std::time::SystemTime;

use async_trait::async_trait;
use loki_connectors_sdk::Connector;
use loki_connectors_sdk::capability::CapabilityDescriptor;
use loki_connectors_sdk::error::ConnectorError;
use loki_connectors_sdk::health::HealthReport;
use loki_connectors_sdk::ids::{CapabilityId, ConnectorId};
use loki_connectors_sdk::manifest::{ConnectorManifest, Provenance};
use loki_connectors_sdk::operation::{OperationContext, OperationRequest, OperationStage};
use loki_connectors_sdk::outcome::{StageEvidence, StageFailure, StageOutcome, StageTimeout};
use loki_connectors_sdk::vocabulary::{CapabilityRiskLevel, DeviceLifecycleState};
use semver::{Version, VersionReq};

/// What a single lifecycle stage should do when the mock's method for that
/// stage is called.
#[derive(Debug, Clone)]
pub enum Behavior {
    Succeed,
    Fail(String),
    /// The connector itself detects (e.g. against its own progress and
    /// `ctx.deadline`) that it cannot finish in time and reports a timeout
    /// from *within* an already-started stage — distinct from
    /// `run_operation`'s own pre-stage deadline check
    /// (`OperationContext::deadline_passed`), which is covered separately in
    /// `src/lifecycle.rs`'s and `src/operation.rs`'s unit tests.
    Timeout,
}

impl Behavior {
    fn resolve(&self, stage: OperationStage) -> StageOutcome {
        match self {
            Behavior::Succeed => StageOutcome::Success(
                StageEvidence::new(format!("{stage} succeeded (mock)"), SystemTime::now())
                    .expect("literal, non-empty summary is always valid"),
            ),
            Behavior::Fail(reason) => StageOutcome::Failure(
                ConnectorError::new("mock_configured_failure", reason.clone())
                    .expect("Behavior::Fail is only ever constructed with a non-empty reason"),
            ),
            Behavior::Timeout => StageOutcome::Timeout(StageTimeout {
                stage,
                observed_at: SystemTime::now(),
                deadline: SystemTime::now(),
            }),
        }
    }
}

/// Call counters, exposed read-only so a test can assert exactly which
/// stage methods were actually invoked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallCounts {
    pub preflight: u32,
    pub execute: u32,
    pub verify: u32,
    pub recover: u32,
    pub health_check: u32,
}

pub struct MockConnector {
    manifest: ConnectorManifest,
    capabilities: Vec<CapabilityDescriptor>,
    preflight_behavior: Behavior,
    execute_behavior: Behavior,
    verify_behavior: Behavior,
    recover_behavior: Behavior,
    calls: CallCounts,
}

impl MockConnector {
    /// A connector configured to succeed at every stage.
    pub fn healthy() -> Self {
        let capability = CapabilityDescriptor::new(
            CapabilityId::new("restart").expect("valid slug"),
            "schema://mock/restart",
            "1.0.0",
            CapabilityRiskLevel::Medium,
            vec![DeviceLifecycleState::Ready],
            NonZeroU32::new(30_000).expect("nonzero"),
        )
        .expect("valid capability descriptor")
        .with_verification_required(true)
        .with_recovery_supported(true);

        let manifest = ConnectorManifest::new(
            ConnectorId::new("loki-mock-connector").expect("valid id"),
            Version::parse("1.0.0").expect("valid semver"),
            "LokiAI",
            Provenance::new(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "in-memory-mock",
            )
            .expect("valid provenance"),
            VersionReq::parse(">=1.0.0, <2.0.0").expect("valid range"),
        )
        .expect("valid manifest")
        .with_capability(capability.clone());

        Self {
            manifest,
            capabilities: vec![capability],
            preflight_behavior: Behavior::Succeed,
            execute_behavior: Behavior::Succeed,
            verify_behavior: Behavior::Succeed,
            recover_behavior: Behavior::Succeed,
            calls: CallCounts::default(),
        }
    }

    #[must_use]
    pub fn with_preflight(mut self, behavior: Behavior) -> Self {
        self.preflight_behavior = behavior;
        self
    }

    #[must_use]
    pub fn with_execute(mut self, behavior: Behavior) -> Self {
        self.execute_behavior = behavior;
        self
    }

    #[must_use]
    pub fn with_verify(mut self, behavior: Behavior) -> Self {
        self.verify_behavior = behavior;
        self
    }

    #[must_use]
    pub fn with_recover(mut self, behavior: Behavior) -> Self {
        self.recover_behavior = behavior;
        self
    }

    pub fn calls(&self) -> CallCounts {
        self.calls
    }
}

#[async_trait]
impl Connector for MockConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn capabilities(&self) -> &[CapabilityDescriptor] {
        &self.capabilities
    }

    async fn health_check(&mut self, _ctx: &OperationContext) -> HealthReport {
        self.calls.health_check += 1;
        HealthReport::empty(SystemTime::now())
    }

    async fn preflight(&mut self, _op: &OperationRequest, _ctx: &OperationContext) -> StageOutcome {
        self.calls.preflight += 1;
        self.preflight_behavior.resolve(OperationStage::Preflight)
    }

    async fn execute(&mut self, _op: &OperationRequest, _ctx: &OperationContext) -> StageOutcome {
        self.calls.execute += 1;
        self.execute_behavior.resolve(OperationStage::Execute)
    }

    async fn verify(&mut self, _op: &OperationRequest, _ctx: &OperationContext) -> StageOutcome {
        self.calls.verify += 1;
        self.verify_behavior.resolve(OperationStage::Verify)
    }

    async fn recover(
        &mut self,
        _op: &OperationRequest,
        _ctx: &OperationContext,
        _failure: &StageFailure,
    ) -> StageOutcome {
        self.calls.recover += 1;
        self.recover_behavior.resolve(OperationStage::Recover)
    }
}
