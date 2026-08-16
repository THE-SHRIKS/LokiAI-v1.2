//! Canonical operation orchestration — the sequencing that turns three
//! independent [`crate::Connector`] method calls into one honest
//! [`LifecycleOutcome`].
//!
//! This is deliberately a free function, not a default trait method: the
//! sequencing rule ("preflight must pass before execute runs; execute must
//! be attempted before verify runs; recover runs whenever execute was
//! attempted but the operation did not end in verified success") is a
//! property of the SDK's contract, not something each connector
//! implementation should be trusted to reimplement correctly — centralizing
//! it here means the "no success from process-exit alone" guarantee
//! (`AGENTS.md` rule 6) holds for every connector automatically, not only
//! for ones that happened to copy it right.

use crate::connector::Connector;
use crate::error::ConnectorError;
use crate::operation::{OperationContext, OperationRequest, OperationStage};
use crate::outcome::{LifecycleOutcome, StageFailure, StageTimeout};

/// If the operation should not even *start* the given stage — because the
/// deadline has already passed, or the operation has already been
/// cancelled/lost its lease (`Docs/CONNECTOR_SPEC.md` §4: must not "continue
/// a device-changing stage after losing the owning job lease") — returns the
/// [`StageFailure`] that stage is deemed to have produced. Returns `None`
/// when the stage should proceed.
///
/// Returns [`StageFailure`] rather than the wider [`StageOutcome`]
/// deliberately: this function can only ever refuse to start a stage, never
/// succeed on its behalf, and encoding that in the return type lets every
/// call site avoid an `unwrap`/`expect` to re-prove it (`AGENTS.md`'s
/// caution against unwrapping on non-obviously-infallible paths) — see
/// [`StageOutcome::from`]`(StageFailure)` for the total, never-panicking
/// conversion back for storage in a [`LifecycleOutcome`].
///
/// This never interrupts a connector call already in progress (see
/// [`crate::operation::CancellationToken`]'s doc comment on why this crate's
/// cancellation/timeout model is cooperative, not preemptive) — it only
/// gates whether the *next* stage begins.
fn short_circuit_before(stage: OperationStage, ctx: &OperationContext) -> Option<StageFailure> {
    if let Some(deadline) = ctx.deadline
        && std::time::SystemTime::now() >= deadline
    {
        return Some(StageFailure::TimedOut(StageTimeout {
            stage,
            observed_at: std::time::SystemTime::now(),
            deadline,
        }));
    }
    if ctx.cancellation.is_cancelled() {
        return Some(StageFailure::Failed(ConnectorError {
            code: "operation_cancelled".to_string(),
            message: format!("cancelled before the {stage} stage started"),
            retryable: false,
            evidence_ref: None,
        }));
    }
    None
}

/// Runs a connector through the full preflight → execute → verify lifecycle,
/// invoking [`Connector::recover`] whenever `execute` was attempted but the
/// operation did not end in independently-verified success.
///
/// See [`LifecycleOutcome::succeeded`] for the exact success rule this
/// function is the sole producer of.
pub async fn run_operation<C: Connector + ?Sized>(
    connector: &mut C,
    op: &OperationRequest,
    ctx: &OperationContext,
) -> LifecycleOutcome {
    if let Some(preflight_failure) = short_circuit_before(OperationStage::Preflight, ctx) {
        return LifecycleOutcome {
            stage_reached: OperationStage::Preflight,
            preflight: preflight_failure.into(),
            execute: None,
            verify: None,
            recovery: None,
        };
    }

    let preflight = connector.preflight(op, ctx).await;
    if preflight.as_failure().is_some() {
        return LifecycleOutcome {
            stage_reached: OperationStage::Preflight,
            preflight,
            execute: None,
            verify: None,
            recovery: None,
        };
    }

    if let Some(execute_failure) = short_circuit_before(OperationStage::Execute, ctx) {
        return LifecycleOutcome {
            stage_reached: OperationStage::Execute,
            preflight,
            execute: Some(execute_failure.into()),
            verify: None,
            recovery: None,
        };
    }

    let execute = connector.execute(op, ctx).await;
    if let Some(failure) = execute.as_failure() {
        let recovery = connector.recover(op, ctx, &failure).await;
        return LifecycleOutcome {
            stage_reached: OperationStage::Execute,
            preflight,
            execute: Some(execute),
            verify: None,
            recovery: Some(recovery),
        };
    }

    if let Some(verify_failure) = short_circuit_before(OperationStage::Verify, ctx) {
        let recovery = connector.recover(op, ctx, &verify_failure).await;
        return LifecycleOutcome {
            stage_reached: OperationStage::Verify,
            preflight,
            execute: Some(execute),
            verify: Some(verify_failure.into()),
            recovery: Some(recovery),
        };
    }

    let verify = connector.verify(op, ctx).await;
    let recovery = match verify.as_failure() {
        None => None,
        Some(failure) => Some(connector.recover(op, ctx, &failure).await),
    };

    LifecycleOutcome {
        stage_reached: OperationStage::Verify,
        preflight,
        execute: Some(execute),
        verify: Some(verify),
        recovery,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{CapabilityId, CorrelationId, DeviceId, IdempotencyKey, OperationId};
    use crate::operation::TargetIdentity;
    use crate::outcome::StageOutcome;
    use std::time::{Duration, SystemTime};

    fn ctx() -> OperationContext {
        OperationContext::new(CorrelationId::new("corr-1").unwrap())
    }

    fn op() -> OperationRequest {
        OperationRequest::new(
            OperationId::new("op-1").unwrap(),
            CapabilityId::new("restart").unwrap(),
            TargetIdentity::new(DeviceId::new("dev-1").unwrap()),
            IdempotencyKey::new("idem-1").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn short_circuit_before_is_none_when_healthy() {
        assert!(short_circuit_before(OperationStage::Preflight, &ctx()).is_none());
    }

    #[test]
    fn short_circuit_before_reports_timeout_when_deadline_passed() {
        let expired = ctx().with_deadline(SystemTime::now() - Duration::from_millis(1));
        let outcome = short_circuit_before(OperationStage::Execute, &expired);
        assert!(matches!(outcome, Some(StageFailure::TimedOut(_))));
    }

    #[test]
    fn short_circuit_before_reports_failure_when_cancelled() {
        let c = ctx();
        c.cancellation.cancel();
        let outcome = short_circuit_before(OperationStage::Verify, &c);
        assert!(
            matches!(outcome, Some(StageFailure::Failed(e)) if e.code == "operation_cancelled")
        );
    }

    // Full-lifecycle coverage (successful lifecycle, preflight short-circuit,
    // mid-execute failure, verify failure after apparent execute success,
    // and timeout) lives in `tests/conformance.rs` against the real
    // `MockConnector`, exercising this function through the crate's public
    // API exactly as a future connector crate would. A trivial inline
    // connector is used here only to prove `run_operation` compiles and
    // resolves against `dyn Connector`, not to duplicate that coverage.
    struct AlwaysSucceeds;

    #[async_trait::async_trait]
    impl Connector for AlwaysSucceeds {
        fn manifest(&self) -> &crate::manifest::ConnectorManifest {
            unimplemented!("not exercised by this smoke test")
        }
        fn capabilities(&self) -> &[crate::capability::CapabilityDescriptor] {
            &[]
        }
        async fn health_check(&mut self, _ctx: &OperationContext) -> crate::health::HealthReport {
            crate::health::HealthReport::empty(SystemTime::now())
        }
        async fn preflight(
            &mut self,
            _op: &OperationRequest,
            _ctx: &OperationContext,
        ) -> StageOutcome {
            StageOutcome::Success(
                crate::outcome::StageEvidence::new("ok", SystemTime::now()).unwrap(),
            )
        }
        async fn execute(
            &mut self,
            _op: &OperationRequest,
            _ctx: &OperationContext,
        ) -> StageOutcome {
            StageOutcome::Success(
                crate::outcome::StageEvidence::new("ok", SystemTime::now()).unwrap(),
            )
        }
        async fn verify(
            &mut self,
            _op: &OperationRequest,
            _ctx: &OperationContext,
        ) -> StageOutcome {
            StageOutcome::Success(
                crate::outcome::StageEvidence::new("ok", SystemTime::now()).unwrap(),
            )
        }
        async fn recover(
            &mut self,
            _op: &OperationRequest,
            _ctx: &OperationContext,
            _failure: &crate::outcome::StageFailure,
        ) -> StageOutcome {
            StageOutcome::Failure(ConnectorError::new("no_recovery", "not exercised").unwrap())
        }
    }

    #[tokio::test]
    async fn run_operation_resolves_against_a_dyn_connector() {
        let mut connector: Box<dyn Connector> = Box::new(AlwaysSucceeds);
        let outcome = run_operation(connector.as_mut(), &op(), &ctx()).await;
        assert!(outcome.succeeded());
    }
}
