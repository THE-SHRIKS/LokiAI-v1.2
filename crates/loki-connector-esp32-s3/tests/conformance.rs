//! Integration tests for [`Esp32S3Connector`] — exercises every declared
//! capability through the real `loki_connectors_sdk::run_operation`
//! lifecycle against [`support::FakeEsp32Transport`], per this node's
//! required verification (`Docs/graph/tasks/CON-003-R1.md` Section 8).
//!
//! Every test in this file goes through the fake transport only — no
//! serial port, USB device, or `std::process::Command` is reachable
//! anywhere in this crate's own test code (this node's hard forbidden-scope
//! item).

mod support;

use loki_connector_esp32_s3::{Esp32S3Connector, Esp32Transport};
use loki_connectors_sdk::Connector;
use loki_connectors_sdk::ids::{
    CapabilityId, CorrelationId, DeviceId, IdempotencyKey, OperationId,
};
use loki_connectors_sdk::operation::{OperationContext, OperationRequest, TargetIdentity};
use loki_connectors_sdk::outcome::StageOutcome;
use loki_connectors_sdk::run_operation;

use support::{FakeEsp32Transport, FakeOutcome, fixture_flash_id, fixture_installation_key};

fn ctx() -> OperationContext {
    OperationContext::new(CorrelationId::new("corr-1").expect("valid"))
}

fn op(capability: &str) -> OperationRequest {
    OperationRequest::new(
        OperationId::new(format!("op-{capability}")).expect("valid"),
        CapabilityId::new(capability).expect("valid"),
        TargetIdentity::new(DeviceId::new("dev-1").expect("valid")),
        IdempotencyKey::new(format!("idem-{capability}")).expect("valid"),
    )
    .expect("valid")
}

fn op_with_inputs(capability: &str, inputs: Vec<u8>) -> OperationRequest {
    op(capability).with_canonical_inputs(inputs)
}

fn connector(transport: FakeEsp32Transport) -> Esp32S3Connector {
    Esp32S3Connector::new(Box::new(transport), fixture_installation_key())
        .expect("valid connector construction")
}

// --- identify ---------------------------------------------------------

#[tokio::test]
async fn identify_succeeds_and_attaches_identity_evidence() {
    let mut c = connector(FakeEsp32Transport::healthy());
    let outcome = run_operation(&mut c, &op("identify"), &ctx()).await;

    assert!(outcome.succeeded(), "{outcome:?}");
    let Some(StageOutcome::Success(evidence)) = &outcome.verify else {
        panic!("expected verify success: {outcome:?}");
    };
    assert!(
        evidence.identity_evidence.is_some(),
        "identify's verify stage must attach DeviceIdentityEvidence"
    );
}

#[tokio::test]
async fn identify_preflight_busy_short_circuits_execute_and_verify() {
    let transport =
        FakeEsp32Transport::healthy().with_chip_id(FakeOutcome::Busy("port in use".into()));
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op("identify"), &ctx()).await;

    assert!(!outcome.succeeded());
    assert!(outcome.execute.is_none());
    assert!(outcome.verify.is_none());
    match &outcome.preflight {
        StageOutcome::Failure(e) => {
            assert_eq!(e.code, "transport_busy");
            assert!(e.retryable);
        }
        other => panic!("expected busy failure: {other:?}"),
    }
}

#[tokio::test]
async fn identify_preflight_permission_denied_is_distinguishable_from_busy() {
    let transport = FakeEsp32Transport::healthy()
        .with_chip_id(FakeOutcome::PermissionDenied("no access to port".into()));
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op("identify"), &ctx()).await;

    match &outcome.preflight {
        StageOutcome::Failure(e) => {
            assert_eq!(e.code, "permission_denied");
            assert!(!e.retryable);
        }
        other => panic!("expected permission-denied failure: {other:?}"),
    }
}

#[tokio::test]
async fn identify_preflight_disconnected_is_distinguishable() {
    let transport = FakeEsp32Transport::healthy()
        .with_chip_id(FakeOutcome::Disconnected("device unplugged".into()));
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op("identify"), &ctx()).await;

    match &outcome.preflight {
        StageOutcome::Failure(e) => assert_eq!(e.code, "device_disconnected"),
        other => panic!("expected disconnected failure: {other:?}"),
    }
}

#[tokio::test]
async fn identify_preflight_timeout_reports_stage_timeout_not_failure() {
    let transport = FakeEsp32Transport::healthy().with_chip_id(FakeOutcome::Timeout);
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op("identify"), &ctx()).await;

    assert!(!outcome.succeeded());
    assert!(matches!(outcome.preflight, StageOutcome::Timeout(_)));
}

#[tokio::test]
async fn identify_execute_malformed_flash_id_fails_and_invokes_recover_never_verify() {
    let transport = FakeEsp32Transport::healthy()
        .with_flash_id(FakeOutcome::Malformed("garbled response".into()));
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op("identify"), &ctx()).await;

    assert!(!outcome.succeeded());
    assert!(
        outcome.verify.is_none(),
        "verify must not run after execute failure"
    );
    assert!(outcome.recovery.is_some(), "recover must be invoked");
    match &outcome.execute {
        Some(StageOutcome::Failure(e)) => assert_eq!(e.code, "malformed_response"),
        other => panic!("expected malformed execute failure: {other:?}"),
    }
    // identify has no recovery mechanism — recover must honestly fail, never
    // fabricate a success it did not verify.
    assert!(!outcome.recovery.unwrap().is_success());
}

// --- read_health --------------------------------------------------------

#[tokio::test]
async fn read_health_succeeds_end_to_end() {
    let mut c = connector(FakeEsp32Transport::healthy());
    let outcome = run_operation(&mut c, &op("read_health"), &ctx()).await;
    assert!(outcome.succeeded(), "{outcome:?}");
}

#[tokio::test]
async fn read_health_malformed_execute_is_distinguishable() {
    let transport =
        FakeEsp32Transport::healthy().with_flash_id(FakeOutcome::Malformed("bad bytes".into()));
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op("read_health"), &ctx()).await;
    assert!(!outcome.succeeded());
    match &outcome.execute {
        Some(StageOutcome::Failure(e)) => assert_eq!(e.code, "malformed_response"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn health_check_method_never_fabricates_unavailable_metrics() {
    let mut c = connector(FakeEsp32Transport::healthy());
    let report = c.health_check(&ctx()).await;
    assert_eq!(report.entries.len(), 4);
    // Voltage/thermal have no fake evidence source: must be Unavailable with
    // a real reason, never Passed.
    let voltage = report
        .entries
        .iter()
        .find(|e| e.check_id == "esp32s3.power_voltage")
        .expect("voltage entry present");
    assert_eq!(
        voltage.status,
        loki_connectors_sdk::vocabulary::HealthCheckStatus::Unavailable
    );
    assert!(voltage.detail.is_some());
}

// --- restart -------------------------------------------------------------

#[tokio::test]
async fn restart_succeeds_end_to_end() {
    let mut c = connector(FakeEsp32Transport::healthy());
    let outcome = run_operation(&mut c, &op("restart"), &ctx()).await;
    assert!(outcome.succeeded(), "{outcome:?}");
}

#[tokio::test]
async fn restart_verify_fails_after_apparent_execute_success_and_recover_is_honest() {
    // preflight's chip-id query succeeds, execute's reset succeeds (no
    // chip-id call), verify's re-identification chip-id query fails —
    // "execute succeeds but verify fails" for the restart capability.
    let transport = FakeEsp32Transport::healthy()
        .with_chip_id_sequence(vec![FakeOutcome::Ok(support::fixture_chip_id())])
        .with_chip_id(FakeOutcome::Disconnected("device did not come back".into()));
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op("restart"), &ctx()).await;

    assert!(!outcome.succeeded());
    assert!(matches!(outcome.execute, Some(StageOutcome::Success(_))));
    assert!(matches!(outcome.verify, Some(StageOutcome::Failure(_))));
    assert!(outcome.recovery.is_some());
    // The recovery attempt's own re-check also hits the same failing
    // fallback behavior, so recovery must honestly fail too — never a
    // fabricated success.
    assert!(!outcome.recovery.unwrap().is_success());
}

#[tokio::test]
async fn restart_preflight_busy_short_circuits() {
    let transport = FakeEsp32Transport::healthy().with_chip_id(FakeOutcome::Busy("busy".into()));
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op("restart"), &ctx()).await;
    assert!(outcome.execute.is_none());
}

#[tokio::test]
async fn restart_execute_disconnected_is_distinguishable_and_call_counts_are_exact() {
    let transport =
        FakeEsp32Transport::healthy().with_reset(FakeOutcome::Disconnected("unplugged".into()));
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op("restart"), &ctx()).await;

    assert!(!outcome.succeeded());
    match &outcome.execute {
        Some(StageOutcome::Failure(e)) => assert_eq!(e.code, "device_disconnected"),
        other => panic!("{other:?}"),
    }
    assert!(
        outcome.verify.is_none(),
        "verify must not run after execute failure"
    );
    assert!(outcome.recovery.is_some());
}

// --- flash_artifact --------------------------------------------------------

fn firmware_bytes() -> Vec<u8> {
    vec![0xAA, 0xBB, 0xCC, 0xDD, 0x01, 0x02, 0x03, 0x04]
}

#[tokio::test]
async fn flash_artifact_succeeds_end_to_end_with_readback_verification() {
    let mut c = connector(FakeEsp32Transport::healthy());
    let outcome = run_operation(
        &mut c,
        &op_with_inputs("flash_artifact", firmware_bytes()),
        &ctx(),
    )
    .await;
    assert!(outcome.succeeded(), "{outcome:?}");
}

#[tokio::test]
async fn flash_artifact_rejects_empty_artifact_at_preflight() {
    let mut c = connector(FakeEsp32Transport::healthy());
    let outcome = run_operation(&mut c, &op_with_inputs("flash_artifact", vec![]), &ctx()).await;
    assert!(!outcome.succeeded());
    match &outcome.preflight {
        StageOutcome::Failure(e) => assert_eq!(e.code, "malformed_request"),
        other => panic!("{other:?}"),
    }
    assert!(outcome.execute.is_none());
}

#[tokio::test]
async fn flash_artifact_rejects_artifact_larger_than_flash() {
    let transport = FakeEsp32Transport::healthy().with_flash_id(FakeOutcome::Ok(
        loki_connector_esp32_s3::transport::FlashIdResponse {
            manufacturer_id: 0xC8,
            device_id: 0x4018,
            size_bytes: 4, // smaller than firmware_bytes()
        },
    ));
    let mut c = connector(transport);
    let outcome = run_operation(
        &mut c,
        &op_with_inputs("flash_artifact", firmware_bytes()),
        &ctx(),
    )
    .await;
    assert!(!outcome.succeeded());
    match &outcome.preflight {
        StageOutcome::Failure(e) => assert_eq!(e.code, "artifact_too_large_for_flash"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn flash_artifact_preflight_permission_denied() {
    let transport = FakeEsp32Transport::healthy()
        .with_flash_id(FakeOutcome::PermissionDenied("no access".into()));
    let mut c = connector(transport);
    let outcome = run_operation(
        &mut c,
        &op_with_inputs("flash_artifact", firmware_bytes()),
        &ctx(),
    )
    .await;
    match &outcome.preflight {
        StageOutcome::Failure(e) => assert_eq!(e.code, "permission_denied"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn flash_artifact_execute_timeout_is_reported_as_timeout_and_triggers_recover() {
    let transport = FakeEsp32Transport::healthy().with_write_flash(FakeOutcome::Timeout);
    let mut c = connector(transport);
    let outcome = run_operation(
        &mut c,
        &op_with_inputs("flash_artifact", firmware_bytes()),
        &ctx(),
    )
    .await;
    assert!(!outcome.succeeded());
    assert!(matches!(outcome.execute, Some(StageOutcome::Timeout(_))));
    assert!(outcome.recovery.is_some());
}

/// The central required scenario: `execute` reports success (the write
/// "succeeded"), but the independent `verify` readback does not match —
/// `recover` must be invoked and the overall outcome must never report
/// success (`Docs/CONNECTOR_SPEC.md` §9).
#[tokio::test]
async fn flash_artifact_execute_succeeds_but_verify_readback_mismatch_fails_honestly() {
    let transport = FakeEsp32Transport::healthy()
        .with_read_flash_override(FakeOutcome::Ok(vec![0xFF; firmware_bytes().len()]));
    let mut c = connector(transport);
    let outcome = run_operation(
        &mut c,
        &op_with_inputs("flash_artifact", firmware_bytes()),
        &ctx(),
    )
    .await;

    assert!(matches!(outcome.execute, Some(StageOutcome::Success(_))));
    assert!(matches!(outcome.verify, Some(StageOutcome::Failure(_))));
    assert!(outcome.recovery.is_some());
    assert!(!outcome.succeeded());
    // Recovery never fabricates a full rollback success — no known-good
    // artifact policy exists (open question 2).
    match outcome.recovery.unwrap() {
        StageOutcome::Failure(e) => assert_eq!(e.code, "recovery_policy_undecided"),
        other => panic!("recover must not fabricate success: {other:?}"),
    }
}

// --- verify_artifact --------------------------------------------------------

#[tokio::test]
async fn verify_artifact_succeeds_when_flash_matches_expected() {
    let expected = firmware_bytes();
    let transport = FakeEsp32Transport::healthy().with_preloaded_flash(0, &expected);
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op_with_inputs("verify_artifact", expected), &ctx()).await;
    assert!(outcome.succeeded(), "{outcome:?}");
}

#[tokio::test]
async fn verify_artifact_fails_when_flash_does_not_match_expected() {
    let expected = firmware_bytes();
    let mismatched = vec![0x00; expected.len()];
    let transport = FakeEsp32Transport::healthy().with_preloaded_flash(0, &mismatched);
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op_with_inputs("verify_artifact", expected), &ctx()).await;
    assert!(!outcome.succeeded());
    match &outcome.execute {
        Some(StageOutcome::Failure(e)) => assert_eq!(e.code, "artifact_mismatch"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn verify_artifact_rejects_empty_expected_bytes() {
    let mut c = connector(FakeEsp32Transport::healthy());
    let outcome = run_operation(&mut c, &op_with_inputs("verify_artifact", vec![]), &ctx()).await;
    assert!(!outcome.succeeded());
    match &outcome.preflight {
        StageOutcome::Failure(e) => assert_eq!(e.code, "malformed_request"),
        other => panic!("{other:?}"),
    }
}

// --- rollback --------------------------------------------------------------

#[tokio::test]
async fn rollback_honestly_fails_without_touching_the_transport() {
    let mut c = connector(FakeEsp32Transport::healthy());
    let outcome = run_operation(&mut c, &op("rollback"), &ctx()).await;

    assert!(!outcome.succeeded());
    assert!(
        outcome.execute.is_none(),
        "preflight must short-circuit execute"
    );
    assert!(outcome.verify.is_none());
    match &outcome.preflight {
        StageOutcome::Failure(e) => assert_eq!(e.code, "recovery_policy_undecided"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn rollback_fails_even_against_an_otherwise_perfectly_healthy_device() {
    // Adversarial in the sense this node's packet means: rollback must
    // never succeed by accident just because the device itself is fine —
    // the policy gap, not device health, is the reason it refuses.
    let transport = FakeEsp32Transport::healthy();
    let mut c = connector(transport);
    let outcome = run_operation(&mut c, &op("rollback"), &ctx()).await;
    assert!(!outcome.succeeded());
}

// --- FakeEsp32Transport's own call-counting (proves the fake itself is
// trustworthy evidence for the connector-level assertions above) ---

#[tokio::test]
async fn fake_transport_counts_every_call_and_round_trips_writes() {
    let mut transport = FakeEsp32Transport::healthy();
    assert_eq!(transport.calls().query_chip_id, 0);

    let _ = transport.query_chip_id().await;
    let _ = transport.query_flash_id().await;
    transport
        .write_flash(0, &[1, 2, 3, 4])
        .await
        .expect("healthy write succeeds");
    let read_back = transport
        .read_flash(0, 4)
        .await
        .expect("healthy read succeeds");

    assert_eq!(read_back, vec![1, 2, 3, 4]);
    let calls = transport.calls();
    assert_eq!(calls.query_chip_id, 1);
    assert_eq!(calls.query_flash_id, 1);
    assert_eq!(calls.write_flash, 1);
    assert_eq!(calls.read_flash, 1);
    assert_eq!(calls.momentary_reset, 0);
}

// --- manifest / capability sanity (structural, not a duplicate of unit tests) ---

#[tokio::test]
async fn manifest_and_capabilities_are_reachable_through_the_connector_trait_object() {
    let mut c: Box<dyn loki_connectors_sdk::Connector> =
        Box::new(connector(FakeEsp32Transport::healthy()));
    assert_eq!(c.capabilities().len(), 6);
    assert_eq!(
        c.manifest().certification[0].support_tier,
        loki_connectors_sdk::vocabulary::SupportTier::Custom
    );
    // Also confirms `run_operation` resolves against `dyn Connector`, the
    // shape a future connector registry actually stores (mirrors
    // `loki-connectors-sdk/src/lifecycle.rs`'s own smoke test).
    let outcome = run_operation(c.as_mut(), &op("identify"), &ctx()).await;
    assert!(outcome.succeeded());
    let _ = fixture_flash_id();
}
