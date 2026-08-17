//! Runs `loki_connectors_sdk::conformance`'s reusable suite against
//! `MockConnector` — proof (Deliverable 5 of `Docs/graph/tasks/CON-001.md`)
//! that the harness is real and exercises a real (fake, but real trait-
//! object-based) connector, and the acceptance-criteria proof that "the
//! trait cannot report an operation successful from process-exit alone".

mod support;

use std::time::{Duration, SystemTime};

use loki_connectors_sdk::conformance;
use loki_connectors_sdk::ids::{
    CapabilityId, CorrelationId, DeviceId, IdempotencyKey, OperationId,
};
use loki_connectors_sdk::operation::{
    OperationContext, OperationRequest, OperationStage, TargetIdentity,
};
use loki_connectors_sdk::outcome::StageOutcome;
use loki_connectors_sdk::run_operation;
use support::{Behavior, MockConnector};

fn op() -> OperationRequest {
    OperationRequest::new(
        OperationId::new("op-conformance-1").unwrap(),
        CapabilityId::new("restart").unwrap(),
        TargetIdentity::new(DeviceId::new("mock-device-1").unwrap()),
        IdempotencyKey::new("idem-conformance-1").unwrap(),
    )
    .unwrap()
}

fn ctx() -> OperationContext {
    OperationContext::new(CorrelationId::new("corr-conformance-1").unwrap())
}

// --- Scenario 1: successful operation lifecycle -------------------------

#[tokio::test]
async fn successful_lifecycle() -> Result<(), Box<conformance::ConformanceFailure>> {
    let mut connector = MockConnector::healthy();
    let outcome = conformance::expect_successful_lifecycle(&mut connector, &op(), &ctx()).await?;

    assert!(outcome.succeeded());
    let calls = connector.calls();
    assert_eq!(calls.preflight, 1);
    assert_eq!(calls.execute, 1);
    assert_eq!(calls.verify, 1);
    assert_eq!(
        calls.recover, 0,
        "recover must not run when the operation succeeds"
    );
    Ok(())
}

// --- Scenario 2: preflight failure short-circuits execute ----------------

#[tokio::test]
async fn preflight_failure_short_circuits_execute()
-> Result<(), Box<conformance::ConformanceFailure>> {
    let mut connector = MockConnector::healthy()
        .with_preflight(Behavior::Fail("simulated preflight rejection".to_string()));

    conformance::expect_preflight_failure_short_circuits_execute(&mut connector, &op(), &ctx())
        .await?;

    // Instance-level proof, not just structural: execute/verify were never
    // even called on this connector.
    let calls = connector.calls();
    assert_eq!(calls.preflight, 1);
    assert_eq!(
        calls.execute, 0,
        "execute must not be called after preflight fails"
    );
    assert_eq!(
        calls.verify, 0,
        "verify must not be called after preflight fails"
    );
    assert_eq!(
        calls.recover, 0,
        "nothing to recover from — execute never ran"
    );
    Ok(())
}

// --- Scenario 3: a mid-execute failure ------------------------------------

#[tokio::test]
async fn mid_execute_failure_skips_verify_and_triggers_recovery()
-> Result<(), Box<conformance::ConformanceFailure>> {
    let mut connector = MockConnector::healthy().with_execute(Behavior::Fail(
        "simulated failure partway through execute (e.g. transfer ok, apply failed)".to_string(),
    ));

    conformance::expect_execute_failure(&mut connector, &op(), &ctx()).await?;

    let calls = connector.calls();
    assert_eq!(calls.preflight, 1);
    assert_eq!(calls.execute, 1);
    assert_eq!(calls.verify, 0, "verify must not run after execute fails");
    assert_eq!(calls.recover, 1, "recover must run after execute fails");
    Ok(())
}

// --- Scenario 4: verify failure after apparently-successful execute ------
// This is the packet's explicit acceptance-criteria proof: "The trait
// cannot report an operation successful from process-exit alone — proven by
// a test where the mock connector exits 0 but fails verify."

#[tokio::test]
async fn verify_failure_after_apparent_execute_success_is_not_reported_successful()
-> Result<(), Box<conformance::ConformanceFailure>> {
    let mut connector = MockConnector::healthy()
        .with_execute(Behavior::Succeed) // "exits 0"
        .with_verify(Behavior::Fail(
            "firmware digest mismatch after flash".to_string(),
        ));

    let outcome = conformance::expect_verify_failure_after_apparent_execute_success(
        &mut connector,
        &op(),
        &ctx(),
    )
    .await?;

    // Spelled out explicitly (not just delegated to the harness) because
    // this is the specific property Docs/graph/tasks/CON-001.md's
    // acceptance criteria calls out by name.
    assert!(
        matches!(outcome.execute, Some(StageOutcome::Success(_))),
        "execute must have appeared to succeed"
    );
    assert!(
        !outcome.succeeded(),
        "an operation whose verify failed must never be reported successful, \
         even though execute reported success"
    );

    let calls = connector.calls();
    assert_eq!(calls.execute, 1);
    assert_eq!(calls.verify, 1);
    assert_eq!(calls.recover, 1, "recover must run after a verify failure");
    Ok(())
}

// --- Scenario 5: timeout ---------------------------------------------------

#[tokio::test]
async fn connector_reported_timeout_during_execute()
-> Result<(), Box<conformance::ConformanceFailure>> {
    let mut connector = MockConnector::healthy().with_execute(Behavior::Timeout);

    conformance::expect_timeout(&mut connector, &op(), &ctx(), OperationStage::Execute).await?;

    let calls = connector.calls();
    assert_eq!(calls.execute, 1);
    assert_eq!(calls.verify, 0, "verify must not run after a timeout");
    assert_eq!(
        calls.recover, 1,
        "recover must run after a timeout mid-operation"
    );
    Ok(())
}

#[tokio::test]
async fn deadline_already_passed_times_out_before_any_stage_is_called()
-> Result<(), Box<conformance::ConformanceFailure>> {
    // A second, independent timeout path: run_operation's own real
    // SystemTime-based pre-stage deadline check (src/lifecycle.rs), rather
    // than the connector self-reporting Behavior::Timeout. Deterministic —
    // the deadline is already in the past, so no real waiting occurs.
    let mut connector = MockConnector::healthy();
    let expired_ctx = ctx().with_deadline(SystemTime::now() - Duration::from_millis(1));

    conformance::expect_timeout(
        &mut connector,
        &op(),
        &expired_ctx,
        OperationStage::Preflight,
    )
    .await?;

    let calls = connector.calls();
    assert_eq!(
        calls.preflight, 0,
        "preflight must not even be called once the deadline has passed"
    );
    Ok(())
}

#[tokio::test]
async fn recover_can_honestly_report_it_has_no_real_recovery_mechanism() {
    // Docs/CONNECTOR_SPEC.md §8/§11 requires a recovery *contract*, not that
    // every connector actually can recover every capability. A connector
    // with nothing real to offer must say so honestly (AGENTS.md rule 1)
    // instead of fabricating a recovered-successfully evidence outcome —
    // exercised here via MockConnector::with_recover.
    let mut connector = MockConnector::healthy()
        .with_execute(Behavior::Fail("simulated execute failure".to_string()))
        .with_recover(Behavior::Fail(
            "this capability declares recovery_supported=false".to_string(),
        ));

    let outcome = run_operation(&mut connector, &op(), &ctx()).await;

    assert!(!outcome.succeeded());
    assert!(
        matches!(outcome.recovery, Some(StageOutcome::Failure(_))),
        "recover's own honest failure must be preserved in the outcome, not silently upgraded"
    );
    assert_eq!(connector.calls().recover, 1);
}

// --- Direct run_operation coverage (not routed through the harness) ------

#[tokio::test]
async fn run_operation_never_marks_success_from_execute_alone_across_every_configuration() {
    // A denser sweep across the acceptance criterion, independent of the
    // conformance module, calling loki_connectors_sdk::run_operation
    // directly.
    let configurations = [
        (
            Behavior::Succeed,
            Behavior::Fail("verify disagrees".to_string()),
        ),
        (Behavior::Succeed, Behavior::Timeout),
    ];

    for (execute_behavior, verify_behavior) in configurations {
        let mut connector = MockConnector::healthy()
            .with_execute(execute_behavior)
            .with_verify(verify_behavior);
        let outcome = run_operation(&mut connector, &op(), &ctx()).await;
        assert!(
            matches!(outcome.execute, Some(StageOutcome::Success(_))),
            "execute must have reported success in this sweep"
        );
        assert!(
            !outcome.succeeded(),
            "verify not succeeding must always override an apparently-successful execute"
        );
    }
}
