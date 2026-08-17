//! Reusable connector conformance harness — this node's Deliverable 5:
//! scenario-assertion functions any future concrete connector (`CON-002`,
//! `CON-003`+) can call from its own `tests/*.rs` against its own
//! implementation, the same way this crate's own `tests/conformance.rs`
//! calls them against `MockConnector`.
//!
//! Each function here takes an **already-configured** connector instance —
//! i.e. the caller has already arranged for `connector` to behave the way
//! that scenario needs (a real connector might do this with a fault-
//! injecting test transport; `MockConnector` does it with an explicit
//! `Behavior` field per stage). This harness does not, and structurally
//! cannot, force an arbitrary connector implementation to fail in a
//! specific way from the outside — its job is to verify that
//! [`crate::lifecycle::run_operation`]'s *contract* holds for whatever
//! connector it is given: the right stages ran, the right stages were
//! skipped, and [`crate::outcome::LifecycleOutcome::succeeded`] reflects
//! only a real, independent `verify` pass.
//!
//! # Example
//!
//! ```rust,ignore
//! use loki_connectors_sdk::conformance;
//!
//! #[tokio::test]
//! async fn preflight_failure_short_circuits_execute() -> Result<(), Box<conformance::ConformanceFailure>> {
//!     let mut connector = MyConnector::configured_to_fail_preflight();
//!     conformance::expect_preflight_failure_short_circuits_execute(
//!         &mut connector,
//!         &my_operation_request(),
//!         &my_operation_context(),
//!     )
//!     .await?;
//!     Ok(())
//! }
//! ```

use std::fmt;

use crate::connector::Connector;
use crate::lifecycle::run_operation;
use crate::operation::{OperationContext, OperationRequest, OperationStage};
use crate::outcome::{LifecycleOutcome, StageOutcome};

/// A conformance scenario's expectation was not met. Carries the full
/// [`LifecycleOutcome`] so a failing test prints exactly what actually
/// happened, not just that something did not match.
#[derive(Debug)]
pub struct ConformanceFailure {
    pub reason: String,
    pub outcome: LifecycleOutcome,
}

impl fmt::Display for ConformanceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (outcome: {:?})", self.reason, self.outcome)
    }
}

impl std::error::Error for ConformanceFailure {}

/// Boxed so every `Result<LifecycleOutcome, _>` this module returns stays a
/// small, cheap-to-move `Err` variant (`ConformanceFailure` embeds a full
/// [`LifecycleOutcome`] for diagnostics, which is not small — `clippy`'s
/// `result_large_err` correctly flags an unboxed version of this).
fn fail(reason: impl Into<String>, outcome: LifecycleOutcome) -> Box<ConformanceFailure> {
    Box::new(ConformanceFailure {
        reason: reason.into(),
        outcome,
    })
}

/// Scenario 1/5: a connector that passes preflight, execute, and verify
/// must produce a [`LifecycleOutcome`] where every stage ran and
/// `succeeded()` is `true`.
pub async fn expect_successful_lifecycle<C: Connector + ?Sized>(
    connector: &mut C,
    op: &OperationRequest,
    ctx: &OperationContext,
) -> Result<LifecycleOutcome, Box<ConformanceFailure>> {
    let outcome = run_operation(connector, op, ctx).await;
    if !outcome.preflight.is_success() {
        return Err(fail("expected preflight to succeed", outcome));
    }
    match &outcome.execute {
        Some(e) if e.is_success() => {}
        _ => return Err(fail("expected execute to run and succeed", outcome)),
    }
    match &outcome.verify {
        Some(v) if v.is_success() => {}
        _ => return Err(fail("expected verify to run and succeed", outcome)),
    }
    if !outcome.succeeded() {
        return Err(fail(
            "expected LifecycleOutcome::succeeded() to be true",
            outcome,
        ));
    }
    Ok(outcome)
}

/// Scenario 2/5: a connector whose preflight fails (or times out) must
/// never have `execute` or `verify` called — proof that
/// [`crate::lifecycle::run_operation`] does not "infer success from process
/// exit alone" (`AGENTS.md` rule 6) by racing ahead regardless of preflight.
pub async fn expect_preflight_failure_short_circuits_execute<C: Connector + ?Sized>(
    connector: &mut C,
    op: &OperationRequest,
    ctx: &OperationContext,
) -> Result<LifecycleOutcome, Box<ConformanceFailure>> {
    let outcome = run_operation(connector, op, ctx).await;
    if outcome.preflight.as_failure().is_none() {
        return Err(fail(
            "expected preflight to fail or time out for this scenario",
            outcome,
        ));
    }
    if outcome.execute.is_some() {
        return Err(fail(
            "execute must not run after a preflight failure",
            outcome,
        ));
    }
    if outcome.verify.is_some() {
        return Err(fail(
            "verify must not run after a preflight failure",
            outcome,
        ));
    }
    if outcome.succeeded() {
        return Err(fail("outcome must not report success", outcome));
    }
    Ok(outcome)
}

/// Scenario 3/5: a connector whose preflight passes but whose `execute`
/// fails mid-way must skip `verify` entirely and invoke `recover`.
pub async fn expect_execute_failure<C: Connector + ?Sized>(
    connector: &mut C,
    op: &OperationRequest,
    ctx: &OperationContext,
) -> Result<LifecycleOutcome, Box<ConformanceFailure>> {
    let outcome = run_operation(connector, op, ctx).await;
    if !outcome.preflight.is_success() {
        return Err(fail(
            "expected preflight to succeed for this scenario",
            outcome,
        ));
    }
    match &outcome.execute {
        Some(e) if e.as_failure().is_some() => {}
        _ => return Err(fail("expected execute to fail or time out", outcome)),
    }
    if outcome.verify.is_some() {
        return Err(fail(
            "verify must not run after an execute failure",
            outcome,
        ));
    }
    if outcome.recovery.is_none() {
        return Err(fail(
            "recover must be invoked after an execute failure",
            outcome,
        ));
    }
    if outcome.succeeded() {
        return Err(fail("outcome must not report success", outcome));
    }
    Ok(outcome)
}

/// Scenario 4/5: the central acceptance proof for this node — a connector
/// whose `execute` reports [`StageOutcome::Success`] (i.e. "exits 0") but
/// whose independent `verify` then fails must still produce an overall
/// failed [`LifecycleOutcome`], with `recover` invoked.
pub async fn expect_verify_failure_after_apparent_execute_success<C: Connector + ?Sized>(
    connector: &mut C,
    op: &OperationRequest,
    ctx: &OperationContext,
) -> Result<LifecycleOutcome, Box<ConformanceFailure>> {
    let outcome = run_operation(connector, op, ctx).await;
    if !outcome.preflight.is_success() {
        return Err(fail(
            "expected preflight to succeed for this scenario",
            outcome,
        ));
    }
    match &outcome.execute {
        Some(e) if e.is_success() => {}
        _ => {
            return Err(fail(
                "expected execute to apparently succeed for this scenario",
                outcome,
            ));
        }
    }
    match &outcome.verify {
        Some(v) if v.as_failure().is_some() => {}
        _ => return Err(fail("expected verify to fail for this scenario", outcome)),
    }
    if outcome.recovery.is_none() {
        return Err(fail(
            "recover must be invoked after a verify failure",
            outcome,
        ));
    }
    if outcome.succeeded() {
        return Err(fail(
            "a successful-looking execute must never make succeeded() true on its own \
             (Docs/CONNECTOR_SPEC.md §9: exit success is stage completion only)",
            outcome,
        ));
    }
    Ok(outcome)
}

/// Scenario 5/5: whichever stage times out, the outcome must record a
/// [`StageOutcome::Timeout`] at that stage and must never report success.
pub async fn expect_timeout<C: Connector + ?Sized>(
    connector: &mut C,
    op: &OperationRequest,
    ctx: &OperationContext,
    expected_stage: OperationStage,
) -> Result<LifecycleOutcome, Box<ConformanceFailure>> {
    let outcome = run_operation(connector, op, ctx).await;
    let timed_out_at_expected_stage = match expected_stage {
        OperationStage::Preflight => matches!(outcome.preflight, StageOutcome::Timeout(_)),
        OperationStage::Execute => matches!(outcome.execute, Some(StageOutcome::Timeout(_))),
        OperationStage::Verify => matches!(outcome.verify, Some(StageOutcome::Timeout(_))),
        OperationStage::Recover => matches!(outcome.recovery, Some(StageOutcome::Timeout(_))),
    };
    if !timed_out_at_expected_stage {
        return Err(fail(
            format!("expected a Timeout outcome at the {expected_stage} stage"),
            outcome,
        ));
    }
    if outcome.succeeded() {
        return Err(fail("outcome must not report success", outcome));
    }
    Ok(outcome)
}
