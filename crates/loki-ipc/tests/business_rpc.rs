//! Real named-pipe adversarial/coverage suite for the business RPC contract
//! (`P1-RPC-CONTRACT-001-R1`).
//!
//! Every test here drives a real `Server`/`Client` pair over a real,
//! uniquely-named Windows named pipe — never a type-level-only contract
//! test. See `tests/support/business.rs` for the fixture principals/storage
//! and the `TestBusinessService` test double.

mod support;

use std::collections::BTreeSet;
use std::time::Duration;

use loki_ipc::business::{
    ApprovalDenyParams, ApprovalGrantParams, BuildRequestParams, BusinessStreamName,
    BusinessStreamParams, DeploymentRequestParams, DeviceListParams, DevicePageParams,
    DeviceParams, EmptyParams, FlashRequestParams, IdempotentDeviceParams, IncidentParams,
    JobCancelParams, JobParams, PageParams, RestartRequestParams, RollbackRequestParams,
    TelemetryParams, TestOperationParams,
};
use loki_ipc::client::{CallError, Client};
use loki_ipc::protocol::{Method, ProtocolVersion, RpcError};
use loki_ipc::server::Server;
use support::business::{
    DESKTOP_PRINCIPAL_ID, FIXTURE_DEVICE_ID, FIXTURE_RUN_ID, FIXTURE_WORK_INTENT_ID,
    OTHER_DEVICE_ID,
};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);

async fn start_business_server(
    pipe_name: &str,
) -> (
    tokio::task::JoinHandle<()>,
    support::business::SharedCredentialSet,
    std::sync::Arc<loki_storage::Storage>,
) {
    let (config, resolver, storage) = support::business_server_config(pipe_name);
    let mut server = Server::bind(config).expect("bind business server");
    let handle = tokio::spawn(async move {
        if let Ok(connection) = server.accept().await {
            let _ = connection.handle().await;
        }
    });
    (handle, resolver, storage)
}

async fn connect_as(pipe_name: &str, token: &str) -> Client {
    Client::connect(support::client_config_with_token(pipe_name, token))
        .await
        .expect("connect as fixture principal")
}

/// The complete 23-method unary fixture: every method paired with whether
/// it has a real backing result in this node (`Ok`) or is a still-open,
/// permanently-unsupported method (`Err(UnsupportedMethod)`), and which
/// fixture principal (desktop, full authority) is used.
fn backed_unary_methods() -> Vec<Method> {
    vec![
        Method::RuntimeGetStatus(EmptyParams {}),
        Method::DevicesList(DeviceListParams {
            limit: 10,
            after_device_id: None,
        }),
        Method::DeviceGet(DeviceParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
        }),
        Method::DeviceGetHealth(DeviceParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
        }),
    ]
}

fn unbacked_unary_methods() -> Vec<Method> {
    vec![
        Method::DeviceReadLogs(DevicePageParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
            limit: 10,
            cursor: None,
        }),
        Method::DeviceQueryTelemetry(TelemetryParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
            limit: 10,
            cursor: None,
            signal_ids: vec![],
        }),
        Method::IncidentsList(PageParams {
            limit: 10,
            cursor: None,
        }),
        Method::IncidentGet(IncidentParams {
            incident_id: "incident-1".to_string(),
        }),
        Method::JobGet(JobParams {
            job_id: "job-1".to_string(),
        }),
        Method::HealthCheckRequest(IdempotentDeviceParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
            idempotency_key: "idem-health".to_string(),
        }),
        Method::PreflightRequest(IdempotentDeviceParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
            idempotency_key: "idem-preflight".to_string(),
        }),
        Method::JobCancel(JobCancelParams {
            job_id: "job-1".to_string(),
            idempotency_key: "idem-cancel".to_string(),
        }),
        Method::TestOnDeviceRequest(TestOperationParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
            test_suite_id: "suite-1".to_string(),
            idempotency_key: "idem-test".to_string(),
        }),
        Method::DeploymentRequest(DeploymentRequestParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
            artifact_id: "artifact-1".to_string(),
            artifact_digest: "a".repeat(64),
            verification_plan_id: "plan-1".to_string(),
            rollback_plan_id: "rollback-1".to_string(),
            idempotency_key: "idem-deploy".to_string(),
        }),
        Method::RollbackRequest(RollbackRequestParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
            deployment_id: "deploy-1".to_string(),
            recovery_policy_id: "policy-1".to_string(),
            idempotency_key: "idem-rollback".to_string(),
        }),
        Method::ApprovalGrant(ApprovalGrantParams {
            approval_id: "apr-fixture-1".to_string(),
            bound_request_digest: "a".repeat(64),
            idempotency_key: "idem-grant".to_string(),
        }),
        Method::ApprovalDeny(ApprovalDenyParams {
            approval_id: "apr-fixture-1".to_string(),
            bound_request_digest: "a".repeat(64),
            reason: "not needed".to_string(),
            idempotency_key: "idem-deny".to_string(),
        }),
    ]
}

#[tokio::test]
async fn all_four_backed_read_methods_return_real_storage_backed_results() {
    let pipe_name = support::unique_pipe_name("business-reads");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "desktop_full_token").await;

    for method in backed_unary_methods() {
        let result = client
            .business_call(method.clone(), CALL_TIMEOUT)
            .await
            .unwrap_or_else(|err| panic!("expected Ok for {method:?}, got {err:?}"));
        assert!(result.validate_contract());
    }
}

#[tokio::test]
async fn all_thirteen_unbacked_methods_are_authorized_then_fail_closed_as_unsupported() {
    let pipe_name = support::unique_pipe_name("business-unbacked");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "desktop_full_token").await;

    let methods = unbacked_unary_methods();
    assert_eq!(methods.len(), 13, "cover the complete unbacked-method set");
    for method in methods {
        let result = client.business_call(method.clone(), CALL_TIMEOUT).await;
        assert!(
            matches!(
                result,
                Err(CallError::Rpc(RpcError::UnsupportedMethod { .. }))
            ),
            "expected UnsupportedMethod (proving authorization ran, then failed closed) for {method:?}, got {result:?}"
        );
    }
}

#[tokio::test]
async fn agent_run_register_accepts_then_replays_identically_on_the_same_idempotency_key() {
    let pipe_name = support::unique_pipe_name("business-agent-accept");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "agent_full_token").await;

    let method = Method::AgentRunRegister(loki_ipc::business::AgentRunRegisterParams {
        parent_run_id: None,
        parameters_digest: "params-digest-fixture-01".to_string(),
        idempotency_key: "idem-agent-accept-1".to_string(),
    });
    let first = client
        .business_call(method.clone(), CALL_TIMEOUT)
        .await
        .expect("first registration accepted");
    let loki_ipc::business::BusinessRpcResult::Mutation {
        outcome: loki_ipc::business::WireMutationOutcome::Accepted { .. },
        ..
    } = first
    else {
        panic!("expected Accepted, got {first:?}");
    };

    let second = client
        .business_call(method, CALL_TIMEOUT)
        .await
        .expect("replay accepted");
    let loki_ipc::business::BusinessRpcResult::Mutation {
        outcome: loki_ipc::business::WireMutationOutcome::ReplayedIdenticalAccepted { .. },
        ..
    } = second
    else {
        panic!("expected ReplayedIdenticalAccepted, got {second:?}");
    };
}

#[tokio::test]
async fn agent_run_register_with_no_seeded_grant_surfaces_a_real_rejection_not_a_fabricated_success()
 {
    // The desktop principal has no seeded agent-connection row, so this
    // exercises `AcceptanceResult::Rejected(GrantInvalid)` end-to-end over
    // the wire.
    let pipe_name = support::unique_pipe_name("business-agent-reject");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "desktop_full_token").await;

    let method = Method::WorkIntentAcknowledge(loki_ipc::business::WorkIntentAckParams {
        work_intent_id: FIXTURE_WORK_INTENT_ID.to_string(),
        run_id: FIXTURE_RUN_ID.to_string(),
        parameters_digest: "params-digest-fixture-02".to_string(),
        idempotency_key: "idem-wi-reject-1".to_string(),
    });
    let result = client
        .business_call(method, CALL_TIMEOUT)
        .await
        .expect("a business rejection is Ok(Mutation(Rejected(..))), not Err");
    let loki_ipc::business::BusinessRpcResult::Mutation {
        outcome: loki_ipc::business::WireMutationOutcome::Rejected { rejection },
        ..
    } = result
    else {
        panic!("expected Rejected, got {result:?}");
    };
    assert!(matches!(
        rejection,
        loki_ipc::business::WireMutationRejection::GrantInvalid { .. }
    ));
}

/// The real `loki_acceptance_store::accept_mutation` never constructs
/// `AcceptanceResult::ApprovalRequired` (verified by direct inspection —
/// see `crates/loki-ipc/src/server.rs`'s `validate_method` doc comment): a
/// missing `approval_reference` on an approval-gated method is a
/// caller/construction error. This crate's own request validation
/// therefore requires the field up front and rejects its absence as
/// `InvalidRequest` before ever attempting submission — never silently
/// forwarding a request this crate already knows must fail, and never
/// fabricating a graceful-looking `ApprovalRequired` wire result that the
/// real acceptance boundary cannot actually produce today.
#[tokio::test]
async fn restart_request_without_an_approval_reference_is_rejected_before_dispatch() {
    let pipe_name = support::unique_pipe_name("business-restart-approval");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "desktop_full_token").await;

    let method = Method::RestartRequest(RestartRequestParams {
        device_id: FIXTURE_DEVICE_ID.to_string(),
        parameters_digest: "params-digest-fixture-03".to_string(),
        idempotency_key: "idem-restart-1".to_string(),
        approval_reference: None,
    });
    let result = client.business_call(method, CALL_TIMEOUT).await;
    assert!(matches!(
        result,
        Err(CallError::Rpc(RpcError::InvalidRequest { .. }))
    ));
}

#[tokio::test]
async fn flash_request_is_accepted_with_a_real_seeded_and_granted_approval() {
    let pipe_name = support::unique_pipe_name("business-flash-accept");
    let (_server, _resolver, storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "desktop_full_token").await;

    // Compute the real canonical digest this exact request will bind to
    // (deterministic, since `OperationBinding::digest()` is a pure
    // function of the request's own fields), then seed and grant a real
    // `local_approvals` row bound to that exact digest — the actual
    // consumption check `accept_mutation` runs, not a bypass.
    let binding = loki_mutation_contract::digest::OperationBinding::new(
        "S-1-5-21-1111111111-2222222222-3333333333-9100",
        DESKTOP_PRINCIPAL_ID,
        1,
        loki_mutation_contract::MutationMethod::Flash,
        FIXTURE_DEVICE_ID,
        Some("f".repeat(64)),
        "params-digest-fixture-04",
        loki_mutation_contract::IdempotencyKey::new("idem-flash-accept-1").unwrap(),
    )
    .unwrap();
    let digest = binding.digest();
    let now_ms = loki_acceptance_store::clock::unix_millis_now();

    {
        use loki_storage::approvals::{ApprovalsRepository, NewApproval};
        storage
            .insert_approval(&NewApproval {
                approval_id: "apr-flash-1".to_string(),
                request_digest: digest.as_str().to_string(),
                binding_version: loki_storage::approvals::CURRENT_BINDING_VERSION,
                requester_sid: "S-1-5-21-1111111111-2222222222-3333333333-9100".to_string(),
                actor_id: DESKTOP_PRINCIPAL_ID.to_string(),
                grant_generation: 1,
                operation: "flash".to_string(),
                target_id: FIXTURE_DEVICE_ID.to_string(),
                artifact_digest: Some("f".repeat(64)),
                parameters_digest: "params-digest-fixture-04".to_string(),
                idempotency_key: "idem-flash-accept-1".to_string(),
                reason: "test fixture".to_string(),
                requested_at_ms: now_ms - 1_000,
                expires_at_ms: now_ms + 3_600_000,
            })
            .expect("insert real approval row");
        let granted = storage
            .grant_approval(
                "apr-flash-1",
                "S-1-5-21-1111111111-2222222222-3333333333-9100",
                now_ms,
            )
            .expect("grant approval");
        assert!(
            granted,
            "approval must actually be granted, not already-granted"
        );
    }

    let method = Method::FlashRequest(FlashRequestParams {
        device_id: FIXTURE_DEVICE_ID.to_string(),
        artifact_id: "artifact-1".to_string(),
        artifact_digest: "f".repeat(64),
        parameters_digest: "params-digest-fixture-04".to_string(),
        idempotency_key: "idem-flash-accept-1".to_string(),
        approval_reference: Some(loki_ipc::business::ApprovalReferenceParams {
            approval_id: "apr-flash-1".to_string(),
            bound_request_digest: digest.as_str().to_string(),
            expires_at_ms: now_ms + 60_000,
        }),
    });
    let result = client
        .business_call(method, CALL_TIMEOUT)
        .await
        .expect("accepted flash request");
    assert!(matches!(
        result,
        loki_ipc::business::BusinessRpcResult::Mutation {
            outcome: loki_ipc::business::WireMutationOutcome::Accepted { .. },
            ..
        }
    ));
}

/// A present `approval_reference` bound to a digest that has no matching
/// real, granted approval row is rejected as a real
/// `ApprovalNotConsumable`, not silently accepted.
#[tokio::test]
async fn flash_request_with_an_unseeded_approval_reference_is_rejected_not_accepted() {
    let pipe_name = support::unique_pipe_name("business-flash-unseeded-approval");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "desktop_full_token").await;

    let method = Method::FlashRequest(FlashRequestParams {
        device_id: FIXTURE_DEVICE_ID.to_string(),
        artifact_id: "artifact-1".to_string(),
        artifact_digest: "e".repeat(64),
        parameters_digest: "params-digest-fixture-06".to_string(),
        idempotency_key: "idem-flash-unseeded-1".to_string(),
        approval_reference: Some(loki_ipc::business::ApprovalReferenceParams {
            approval_id: "apr-never-seeded".to_string(),
            bound_request_digest: "sha256:".to_string() + &"b".repeat(64),
            expires_at_ms: loki_acceptance_store::clock::unix_millis_now() + 60_000,
        }),
    });
    let result = client
        .business_call(method, CALL_TIMEOUT)
        .await
        .expect("a business rejection is Ok(Mutation(Rejected(..))), not Err");
    let loki_ipc::business::BusinessRpcResult::Mutation {
        outcome: loki_ipc::business::WireMutationOutcome::Rejected { rejection },
        ..
    } = result
    else {
        panic!("expected Rejected, got {result:?}");
    };
    assert_eq!(
        rejection,
        loki_ipc::business::WireMutationRejection::ApprovalNotConsumable
    );
}

#[tokio::test]
async fn build_request_result_is_denied_before_dispatch_when_the_principal_lacks_request_build_authority()
 {
    let pipe_name = support::unique_pipe_name("business-build-denied");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "agent_full_token").await;

    let method = Method::BuildRequest(BuildRequestParams {
        source_snapshot_id: "snap-1".to_string(),
        target_id: "target-1".to_string(),
        recipe_id: "recipe-1".to_string(),
        parameters_digest: "params-digest-fixture-05".to_string(),
        idempotency_key: "idem-build-1".to_string(),
        // A well-formed reference (so `validate_method`'s bounds check
        // passes and authorization, not request shape, is what this test
        // proves) — its digest need not be real/consumable since the
        // request is denied before ever reaching submission.
        approval_reference: Some(loki_ipc::business::ApprovalReferenceParams {
            approval_id: "apr-well-formed".to_string(),
            bound_request_digest: "sha256:".to_string() + &"c".repeat(64),
            expires_at_ms: loki_acceptance_store::clock::unix_millis_now() + 60_000,
        }),
    });
    let result = client.business_call(method, CALL_TIMEOUT).await;
    assert!(matches!(
        result,
        Err(CallError::Rpc(RpcError::PermissionDenied))
    ));
}

#[tokio::test]
async fn cross_identity_device_get_is_rejected_not_silently_served() {
    let pipe_name = support::unique_pipe_name("business-cross-identity");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "desktop_full_token").await;

    let method = Method::DeviceGet(DeviceParams {
        device_id: OTHER_DEVICE_ID.to_string(),
    });
    let result = client.business_call(method, CALL_TIMEOUT).await;
    assert!(matches!(
        result,
        Err(CallError::Rpc(RpcError::PermissionDenied))
    ));
}

#[tokio::test]
async fn hostile_v1_0_client_cannot_negotiate_the_business_capability() {
    let pipe_name = support::unique_pipe_name("business-downgrade");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;

    let mut config = support::client_config_with_token(&pipe_name, "desktop_full_token");
    config.protocol_version = ProtocolVersion::new(1, 0);
    config.capabilities = {
        let mut caps = BTreeSet::new();
        caps.insert(loki_ipc::protocol::CORE_CAPABILITY.to_string());
        // Hostile: a real v1.0 client would never declare this, but this
        // proves the server — not the client's self-declaration — is the
        // authority on what v1.0 may negotiate.
        caps.insert(loki_ipc::business::BUSINESS_CAPABILITY.to_string());
        caps
    };
    let client = Client::connect(config)
        .await
        .expect("v1.0 handshake itself still succeeds (core-only)");
    assert!(
        !client
            .negotiated_capabilities()
            .contains(loki_ipc::business::BUSINESS_CAPABILITY),
        "a v1.0 client must never be granted the business capability, even if it self-advertises it"
    );

    let result = client
        .business_call(Method::RuntimeGetStatus(EmptyParams {}), CALL_TIMEOUT)
        .await;
    assert!(matches!(
        result,
        Err(CallError::Rpc(RpcError::MissingCapability { .. }))
    ));
}

#[tokio::test]
async fn revoked_credential_denies_the_very_next_call_not_only_a_future_hello() {
    let pipe_name = support::unique_pipe_name("business-revoke-call");
    let (_server, resolver, _storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "desktop_full_token").await;

    let ok = client
        .business_call(Method::RuntimeGetStatus(EmptyParams {}), CALL_TIMEOUT)
        .await;
    assert!(ok.is_ok(), "sanity: call succeeds before revocation");

    resolver.revoke(DESKTOP_PRINCIPAL_ID);

    let denied = client
        .business_call(Method::RuntimeGetStatus(EmptyParams {}), CALL_TIMEOUT)
        .await;
    assert!(
        matches!(
            denied,
            Err(CallError::Rpc(RpcError::CredentialRevalidationFailed))
        ),
        "expected CredentialRevalidationFailed after revoke, got {denied:?}"
    );
}

#[tokio::test]
async fn every_one_of_the_ten_business_streams_is_denied_without_its_specific_domain_authority() {
    let pipe_name = support::unique_pipe_name("business-stream-matrix");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;
    // `agent_full_token` has ManageAgentRuns/AcknowledgeWorkIntents but no
    // read-domain authority and no Subscribe — every stream must deny it.
    let client = connect_as(&pipe_name, "agent_full_token").await;

    let streams = [
        BusinessStreamName::RuntimeStatus,
        BusinessStreamName::DeviceProjection,
        BusinessStreamName::DeviceHealth,
        BusinessStreamName::Telemetry,
        BusinessStreamName::Logs,
        BusinessStreamName::Incidents,
        BusinessStreamName::Jobs,
        BusinessStreamName::Approvals,
        BusinessStreamName::Notifications,
        BusinessStreamName::WorkIntents,
    ];
    for stream in streams {
        let params = BusinessStreamParams {
            stream,
            entity_id: FIXTURE_RUN_ID.to_string(),
            after_sequence: None,
            max_items: 10,
        };
        let mut handle = client
            .business_stream(params, CALL_TIMEOUT)
            .await
            .expect("stream call sent");
        let event = handle.recv().await.expect("stream ended, not dropped");
        assert!(
            matches!(
                event,
                loki_ipc::client::BusinessStreamEvent::Ended(Err(RpcError::PermissionDenied))
            ),
            "stream {stream:?} must deny a principal lacking its domain authority, got {event:?}"
        );
    }
}

#[tokio::test]
async fn an_authorized_stream_reaches_the_fail_closed_producer_not_a_permission_error() {
    let pipe_name = support::unique_pipe_name("business-stream-authorized");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "desktop_full_token").await;

    let params = BusinessStreamParams {
        stream: BusinessStreamName::Jobs,
        entity_id: FIXTURE_RUN_ID.to_string(),
        after_sequence: None,
        max_items: 10,
    };
    let mut handle = client
        .business_stream(params, CALL_TIMEOUT)
        .await
        .expect("stream call sent");
    let event = handle.recv().await.expect("stream ended");
    assert!(
        matches!(
            event,
            loki_ipc::client::BusinessStreamEvent::Ended(Err(RpcError::UnsupportedMethod { .. }))
        ),
        "expected authorization to pass and then fail closed at the (absent) producer, got {event:?}"
    );
}

#[tokio::test]
async fn oversized_business_string_field_is_rejected_before_dispatch() {
    let pipe_name = support::unique_pipe_name("business-oversized");
    let (_server, _resolver, _storage) = start_business_server(&pipe_name).await;
    let client = connect_as(&pipe_name, "desktop_full_token").await;

    let method = Method::AgentRunUpdate(loki_ipc::business::AgentRunUpdateParams {
        run_id: FIXTURE_RUN_ID.to_string(),
        phase: "working".to_string(),
        summary: "x".repeat(loki_ipc::business::MAX_TEXT_BYTES + 1),
        parameters_digest: "params-digest-oversize".to_string(),
        idempotency_key: "idem-oversize-1".to_string(),
    });
    let result = client.business_call(method, CALL_TIMEOUT).await;
    assert!(matches!(
        result,
        Err(CallError::Rpc(RpcError::InvalidRequest { .. }))
    ));
}
