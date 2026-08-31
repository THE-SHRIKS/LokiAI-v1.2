//! Rust/TypeScript shared fixture parity (`Docs/graph/tasks/
//! P1-RPC-CONTRACT-001-R1.md` Deliverable 9 / acceptance criterion: "Rust
//! and TypeScript reject the same malformed/oversized/extra-field
//! payloads (shared negative fixture set, both sides run against it)").
//!
//! This file and `packages/contracts/tests/businessRpc.test.ts` read the
//! exact same `packages/contracts/fixtures/business-rpc.json` — every
//! `requests`/`results`/`streams`/`events`/`principals` entry must decode
//! successfully on both sides, and every `negative` entry must be rejected
//! on both sides.

use loki_ipc::business::{
    AuthenticatedPrincipal, BusinessEvent, BusinessRpcResult, BusinessStreamParams,
};
use loki_ipc::protocol::{Method, WireMessage};
use serde::Deserialize;
use serde_json::Value;

const FIXTURE: &str = include_str!("../../../packages/contracts/fixtures/business-rpc.json");

#[derive(Deserialize)]
struct NegativeCase {
    description: String,
    kind: String,
    value: Value,
}

#[derive(Deserialize)]
struct BusinessFixture {
    requests: Vec<Value>,
    results: Vec<Value>,
    streams: Vec<Value>,
    events: Vec<Value>,
    principals: Vec<Value>,
    negative: Vec<NegativeCase>,
}

fn fixture() -> BusinessFixture {
    serde_json::from_str(FIXTURE).expect("fixture file must itself be well-formed JSON")
}

#[test]
fn fixture_covers_the_complete_23_method_vocabulary() {
    let fixture = fixture();
    assert_eq!(
        fixture.requests.len(),
        23,
        "cover the complete unary method table"
    );
    for request in &fixture.requests {
        let decoded: Result<Method, _> = serde_json::from_value(request.clone());
        assert!(
            decoded.is_ok(),
            "fixture request must decode as a real Method: {request} ({decoded:?})"
        );
        let method: Method = decoded.unwrap();
        // Every fixture request must also pass this crate's own wire-level
        // contract validation (bounds/format), not merely deserialize.
        let wrapped = WireMessage::Call {
            correlation_id: uuid::Uuid::nil(),
            deadline_unix_millis: 0,
            method,
        };
        assert!(
            wrapped.validate_contract().is_ok(),
            "fixture request failed contract validation: {request}"
        );
    }
}

#[test]
fn fixture_results_streams_events_and_principals_all_decode() {
    let fixture = fixture();
    assert!(!fixture.results.is_empty());
    for result in &fixture.results {
        let decoded: Result<BusinessRpcResult, _> = serde_json::from_value(result.clone());
        assert!(
            decoded
                .as_ref()
                .is_ok_and(BusinessRpcResult::validate_contract),
            "fixture result must decode and pass contract validation: {result} ({decoded:?})"
        );
    }
    assert_eq!(fixture.streams.len(), 10, "cover all 10 business streams");
    for stream in &fixture.streams {
        let decoded: Result<BusinessStreamParams, _> = serde_json::from_value(stream.clone());
        assert!(decoded.is_ok(), "fixture stream must decode: {stream}");
    }
    for event in &fixture.events {
        let decoded: Result<BusinessEvent, _> = serde_json::from_value(event.clone());
        assert!(
            decoded.as_ref().is_ok_and(BusinessEvent::validate_contract),
            "fixture event must decode and pass contract validation: {event}"
        );
    }
    for principal in &fixture.principals {
        let decoded: Result<AuthenticatedPrincipal, _> = serde_json::from_value(principal.clone());
        assert!(
            decoded.is_ok(),
            "fixture principal must decode: {principal}"
        );
    }
}

#[test]
fn every_negative_fixture_is_rejected() {
    let fixture = fixture();
    assert!(
        fixture.negative.len() >= 8,
        "cover a real shared negative-fixture set, not a token one or two cases"
    );
    for case in fixture.negative {
        let rejected = match case.kind.as_str() {
            "request" => {
                let decoded: Result<Method, _> = serde_json::from_value(case.value.clone());
                match decoded {
                    Err(_) => true,
                    Ok(method) => WireMessage::Call {
                        correlation_id: uuid::Uuid::nil(),
                        deadline_unix_millis: 0,
                        method,
                    }
                    .validate_contract()
                    .is_err(),
                }
            }
            "result" => match serde_json::from_value::<BusinessRpcResult>(case.value.clone()) {
                Err(_) => true,
                Ok(result) => !result.validate_contract(),
            },
            "stream" => {
                let decoded: Result<BusinessStreamParams, _> =
                    serde_json::from_value(case.value.clone());
                match decoded {
                    Err(_) => true,
                    Ok(method) => WireMessage::CallStream {
                        correlation_id: uuid::Uuid::nil(),
                        deadline_unix_millis: 0,
                        method: loki_ipc::protocol::StreamMethod::BusinessEvents(method),
                    }
                    .validate_contract()
                    .is_err(),
                }
            }
            "event" => match serde_json::from_value::<BusinessEvent>(case.value.clone()) {
                Err(_) => true,
                Ok(event) => !event.validate_contract(),
            },
            "principal" => {
                serde_json::from_value::<AuthenticatedPrincipal>(case.value.clone()).is_err()
            }
            other => panic!("unknown negative fixture kind: {other}"),
        };
        assert!(
            rejected,
            "negative fixture must be rejected: {} ({:?})",
            case.description, case.value
        );
    }
}
