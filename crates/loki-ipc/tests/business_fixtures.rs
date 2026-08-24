use loki_ipc::business::{
    AuthenticatedPrincipal, BusinessEvent, BusinessRpcResult, BusinessStreamParams,
};
use loki_ipc::protocol::{Method, business_result_matches_method};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    requests: Vec<Method>,
    results: Vec<BusinessRpcResult>,
    streams: Vec<BusinessStreamParams>,
    events: Vec<BusinessEvent>,
    principals: Vec<AuthenticatedPrincipal>,
}

#[test]
fn canonical_typescript_fixture_deserializes_in_rust_without_drift() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../packages/contracts/fixtures/business-rpc.json"
    ))
    .expect("canonical business RPC fixture must deserialize in Rust");
    assert_eq!(fixture.requests.len(), 23);
    assert_eq!(fixture.results.len(), 22);
    assert_eq!(fixture.streams.len(), 10);
    assert_eq!(fixture.events.len(), 1);
    assert_eq!(fixture.principals.len(), 2);
    assert!(
        fixture
            .results
            .iter()
            .all(BusinessRpcResult::validate_contract)
    );
    assert!(fixture.events.iter().all(BusinessEvent::validate_contract));
    assert!(fixture.requests.iter().all(|method| {
        fixture
            .results
            .iter()
            .any(|result| business_result_matches_method(method, result))
    }));
}

#[test]
fn generic_invoke_is_not_in_the_rust_method_vocabulary() {
    let value = r#"{"method":"invoke","params":{"command":"flash"}}"#;
    assert!(serde_json::from_str::<Method>(value).is_err());
}

#[test]
fn client_cannot_smuggle_authority_or_unknown_fields_into_a_method() {
    let value = r#"{"method":"restart_request","params":{"device_id":"dev-1","idempotency_key":"idem-1","authority":["decide_approval"]}}"#;
    assert!(serde_json::from_str::<Method>(value).is_err());
}

#[test]
fn nullable_optional_fields_accept_both_omission_and_explicit_null() {
    let omitted = r#"{"method":"devices_list","params":{"limit":10}}"#;
    let explicit_null = r#"{"method":"devices_list","params":{"limit":10,"cursor":null}}"#;
    assert!(serde_json::from_str::<Method>(omitted).is_ok());
    assert!(serde_json::from_str::<Method>(explicit_null).is_ok());
}

#[test]
fn malformed_output_timestamps_and_utf8_byte_overflows_are_rejected() {
    let malformed: BusinessRpcResult = serde_json::from_str(
        r#"{"kind":"runtime_status","meta":{"source_runtime_id":"runtime-1","observed_at":"2026-08-24T12:00:00+05:30","correlation_id":"corr-1"},"state":"connected","runtime_id":"runtime-1"}"#,
    )
    .expect("syntactically valid result");
    assert!(!malformed.validate_contract());
    let oversized: BusinessRpcResult = serde_json::from_value(serde_json::json!({
        "kind": "operation",
        "method": "restart_request",
        "outcome": "rejected",
        "correlation_id": "corr-1",
        "policy_reason": "😀".repeat(501),
        "approval_id": null,
        "job_id": null,
        "bound_request_digest": null
    }))
    .expect("syntactically valid result");
    assert!(!oversized.validate_contract());
}
