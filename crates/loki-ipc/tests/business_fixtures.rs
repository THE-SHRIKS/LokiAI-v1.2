use loki_ipc::business::{
    AuthenticatedPrincipal, BusinessEvent, BusinessRpcResult, BusinessStreamParams,
};
use loki_ipc::protocol::{Method, business_result_matches_method, business_result_matches_request};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    requests: Vec<Method>,
    results: Vec<BusinessRpcResult>,
    streams: Vec<BusinessStreamParams>,
    events: Vec<BusinessEvent>,
    principals: Vec<AuthenticatedPrincipal>,
    invalid_results: Vec<serde_json::Value>,
    binding_negatives: Vec<BindingNegative>,
}

#[derive(Deserialize)]
struct BindingNegative {
    method: String,
    field: String,
    value: serde_json::Value,
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
    assert_eq!(fixture.invalid_results.len(), 6);
    assert_eq!(fixture.binding_negatives.len(), 6);
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
    assert!(
        fixture
            .invalid_results
            .into_iter()
            .all(|value| serde_json::from_value::<BusinessRpcResult>(value).is_err())
    );
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
        "idempotency_key": "idem-1",
        "artifact_digest": null,
        "request_reason": null,
        "approval_id": null,
        "job_id": null,
        "bound_request_digest": null
        ,"scope_ids": ["device:dev-1"]
    }))
    .expect("syntactically valid result");
    assert!(!oversized.validate_contract());
}

#[test]
fn every_result_variant_and_nested_projection_rejects_unknown_fields() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../packages/contracts/fixtures/business-rpc.json"
    ))
    .expect("fixture json");
    for result in fixture["results"].as_array().expect("results") {
        let mut extra = result.clone();
        extra
            .as_object_mut()
            .expect("result object")
            .insert("unexpected".to_string(), serde_json::json!(true));
        assert!(serde_json::from_value::<BusinessRpcResult>(extra).is_err());
    }
    let mut nested = fixture["results"][1].clone();
    nested["meta"]["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<BusinessRpcResult>(nested).is_err());
    let mut nested_device = fixture["results"][1].clone();
    nested_device["devices"] = serde_json::json!([{
        "device_id": "dev-1",
        "lifecycle_state": "ready",
        "connector_id": "connector-1",
        "last_seen_at": "2026-08-24T12:00:00Z"
    }]);
    nested_device["devices"][0]["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<BusinessRpcResult>(nested_device).is_err());
    for invalid in fixture["invalid_results"]
        .as_array()
        .expect("shared invalid results")
    {
        assert!(serde_json::from_value::<BusinessRpcResult>(invalid.clone()).is_err());
    }
}

#[test]
fn every_method_has_a_request_coupled_result_and_entity_mismatches_fail_closed() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../packages/contracts/fixtures/business-rpc.json"
    ))
    .expect("canonical fixture");

    for method in &fixture.requests {
        let result = fixture
            .results
            .iter()
            .find(|result| business_result_matches_request(method, result, "workspace-1"))
            .unwrap_or_else(|| panic!("missing request-coupled result for {method:?}"));
        let mut mismatched = result.clone();
        let has_request_identity = match (method, &mut mismatched) {
            (Method::DeviceGet(_), BusinessRpcResult::DeviceGet { device, .. }) => {
                device.as_mut().expect("fixture device").device_id = "device-other".to_string();
                true
            }
            (
                Method::DeviceGetHealth(_) | Method::DeviceReadLogs(_),
                BusinessRpcResult::DeviceHealth { device_id, .. }
                | BusinessRpcResult::DeviceLogs { device_id, .. },
            )
            | (
                Method::DeviceQueryTelemetry(_),
                BusinessRpcResult::DeviceTelemetry { device_id, .. },
            ) => {
                *device_id = "device-other".to_string();
                true
            }
            (Method::IncidentGet(_), BusinessRpcResult::IncidentGet { incident, .. }) => {
                incident.as_mut().expect("fixture incident").incident_id =
                    "incident-other".to_string();
                true
            }
            (Method::JobGet(_), BusinessRpcResult::JobGet { job, .. }) => {
                job.as_mut().expect("fixture job").job_id = "job-other".to_string();
                true
            }
            (Method::AgentRunUpdate(_), BusinessRpcResult::AgentRun { run_id, .. }) => {
                *run_id = "run-other".to_string();
                true
            }
            (
                Method::WorkIntentAcknowledge(_),
                BusinessRpcResult::WorkIntentAcknowledged {
                    work_intent_id,
                    run_id,
                    ..
                },
            ) => {
                *work_intent_id = "intent-other".to_string();
                *run_id = "run-other".to_string();
                true
            }
            (_, BusinessRpcResult::Operation { scope_ids, .. }) => {
                *scope_ids = std::collections::BTreeSet::from(["workspace:other".to_string()]);
                true
            }
            _ => false,
        };
        if has_request_identity {
            assert!(
                !business_result_matches_request(method, &mismatched, "workspace-1"),
                "mismatched producer identity must be rejected for {method:?}"
            );
        }
    }
}

#[test]
fn consequential_results_bind_changed_request_identity_and_approval_digest() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../packages/contracts/fixtures/business-rpc.json"
    ))
    .expect("canonical fixture");

    for method in &fixture.requests {
        let Some(result) = fixture
            .results
            .iter()
            .find(|result| business_result_matches_request(method, result, "workspace-1"))
        else {
            panic!("missing request-coupled result for {method:?}");
        };
        if !matches!(result, BusinessRpcResult::Operation { .. }) {
            continue;
        }

        let mut changed_idempotency = result.clone();
        if let BusinessRpcResult::Operation {
            idempotency_key, ..
        } = &mut changed_idempotency
        {
            *idempotency_key = "idem-changed".to_string();
        }
        assert!(!business_result_matches_request(
            method,
            &changed_idempotency,
            "workspace-1"
        ));

        let mut changed_artifact = result.clone();
        if let BusinessRpcResult::Operation {
            artifact_digest, ..
        } = &mut changed_artifact
        {
            *artifact_digest = Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string());
        }
        assert!(
            !business_result_matches_request(method, &changed_artifact, "workspace-1"),
            "artifact digest presence/value must match the request for {method:?}"
        );

        if matches!(method, Method::ApprovalGrant(_) | Method::ApprovalDeny(_)) {
            let mut missing_digest = result.clone();
            if let BusinessRpcResult::Operation {
                bound_request_digest,
                ..
            } = &mut missing_digest
            {
                *bound_request_digest = None;
            }
            assert!(!business_result_matches_request(
                method,
                &missing_digest,
                "workspace-1"
            ));

            let mut changed_digest = result.clone();
            if let BusinessRpcResult::Operation {
                bound_request_digest,
                ..
            } = &mut changed_digest
            {
                *bound_request_digest = Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string());
            }
            assert!(!business_result_matches_request(
                method,
                &changed_digest,
                "workspace-1"
            ));
        }

        let mut changed_reason = result.clone();
        if let BusinessRpcResult::Operation { request_reason, .. } = &mut changed_reason {
            *request_reason = Some("changed decision reason".to_string());
        }
        assert!(
            !business_result_matches_request(method, &changed_reason, "workspace-1"),
            "decision reason presence/value must match the request for {method:?}"
        );
    }
}

#[test]
fn shared_changed_request_binding_negatives_fail_in_rust() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../packages/contracts/fixtures/business-rpc.json"
    ))
    .expect("canonical fixture");
    for negative in &fixture.binding_negatives {
        let request = fixture
            .requests
            .iter()
            .find(|method| {
                serde_json::to_value(method)
                    .ok()
                    .and_then(|value| value["method"].as_str().map(str::to_owned))
                    .as_deref()
                    == Some(negative.method.as_str())
            })
            .expect("negative request method");
        let result = fixture
            .results
            .iter()
            .find(|result| business_result_matches_request(request, result, "workspace-1"))
            .expect("positive request-coupled result");
        let mut changed = serde_json::to_value(result).expect("serialize result");
        changed[&negative.field] = negative.value.clone();
        let changed: BusinessRpcResult =
            serde_json::from_value(changed).expect("binding negative remains schema-valid");
        assert!(
            !business_result_matches_request(request, &changed, "workspace-1"),
            "shared negative must fail: {}.{}",
            negative.method,
            negative.field
        );
    }
}
