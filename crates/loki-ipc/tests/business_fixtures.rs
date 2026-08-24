use loki_ipc::business::{BusinessEvent, BusinessRpcResult, BusinessStreamParams};
use loki_ipc::protocol::Method;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    requests: Vec<Method>,
    results: Vec<BusinessRpcResult>,
    streams: Vec<BusinessStreamParams>,
    events: Vec<BusinessEvent>,
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
