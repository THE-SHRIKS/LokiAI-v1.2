mod support;

use std::collections::BTreeSet;

use loki_ipc::auth::{AuthToken, CredentialRecord, CredentialSet};
use loki_ipc::business::{ApprovalGrantParams, AuthenticatedPrincipal, Authority, PrincipalKind};
use loki_ipc::business::{
    BusinessEvent, BusinessRpcResult, BusinessStreamName, BusinessStreamParams,
    BusinessStreamSummary, EmptyParams, OperationMethod, OperationOutcome, ReadMeta, RuntimeState,
};
use loki_ipc::client::BusinessStreamEvent;
use loki_ipc::frame::{DEFAULT_MAX_FRAME_BYTES, read_frame, write_frame};
use loki_ipc::protocol::{CallOutcome, Method, MethodResult, RpcError, WireMessage};
use loki_ipc::server::{
    BusinessCallFuture, BusinessEventEmitter, BusinessRequestContext, BusinessService,
    BusinessStreamFuture, Server, ServerConfig,
};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

async fn write_unvalidated<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    message: &WireMessage,
) {
    let payload = serde_json::to_vec(message).expect("serialize raw test frame");
    writer
        .write_all(&(payload.len() as u32).to_le_bytes())
        .await
        .expect("write raw length");
    writer.write_all(&payload).await.expect("write raw payload");
}

struct RealBusinessService;

impl BusinessService for RealBusinessService {
    fn call(&self, method: Method, context: BusinessRequestContext) -> BusinessCallFuture {
        Box::pin(async move {
            match method {
                Method::RuntimeGetStatus(_) => Ok(BusinessRpcResult::RuntimeStatus {
                    meta: ReadMeta {
                        source_runtime_id: "runtime-1".to_string(),
                        observed_at: "2026-08-24T12:00:00Z".to_string(),
                        correlation_id: context.correlation_id.to_string(),
                    },
                    state: RuntimeState::Connected,
                    runtime_id: "runtime-1".to_string(),
                }),
                _ => Err(RpcError::UnsupportedMethod {
                    method: "test_service".to_string(),
                }),
            }
        })
    }

    fn stream(
        &self,
        params: BusinessStreamParams,
        context: BusinessRequestContext,
        emitter: BusinessEventEmitter,
    ) -> BusinessStreamFuture {
        Box::pin(async move {
            let event = BusinessEvent {
                stream: params.stream,
                sequence: 1,
                correlation_id: context.correlation_id.to_string(),
                occurred_at: "2026-08-24T12:00:00Z".to_string(),
                entity_id: params.scope_id,
                state: "connected".to_string(),
                source_runtime_id: "runtime-1".to_string(),
            };
            emitter.send(event).await?;
            Ok(BusinessStreamSummary {
                items_sent: 1,
                last_sequence: Some(1),
            })
        })
    }
}

fn scoped_principal(
    kind: PrincipalKind,
    principal_id: &str,
    authority: BTreeSet<Authority>,
) -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        kind,
        principal_id: principal_id.to_string(),
        workspace_id: "workspace-1".to_string(),
        scope_ids: BTreeSet::from(["runtime-1".to_string()]),
        authority,
    }
}

#[tokio::test]
async fn hello_ack_projects_only_the_server_created_principal() {
    let pipe_name = support::unique_pipe_name("business-server-principal");
    let principal = AuthenticatedPrincipal {
        kind: PrincipalKind::DesktopInstallation,
        principal_id: "desktop-installation-1".to_string(),
        workspace_id: "workspace-1".to_string(),
        scope_ids: BTreeSet::from(["device-1".to_string()]),
        authority: BTreeSet::from([Authority::ReadRuntime]),
    };
    let token = support::shared_test_token();
    let mut server = Server::bind(
        ServerConfig::new(pipe_name.clone(), token.clone())
            .with_credential_resolver(std::sync::Arc::new(CredentialSet::single(token, principal))),
    )
    .expect("bind server");
    let task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let mut config = support::client_config(&pipe_name);
    config.client_type = "agent_connection_with_fake_desktop_claim".to_string();
    let client = loki_ipc::Client::connect(config).await.expect("connect");
    let projected = client
        .authenticated_principal()
        .expect("current server projects principal");
    assert_eq!(projected.kind, PrincipalKind::DesktopInstallation);
    assert_eq!(projected.principal_id, "desktop-installation-1");
    drop(client);
    task.await.expect("server task").expect("clean disconnect");
}

#[tokio::test]
async fn agent_principal_cannot_grant_approval_even_if_misconfigured_with_decision_authority() {
    let pipe_name = support::unique_pipe_name("business-agent-approval-denied");
    let principal = AuthenticatedPrincipal {
        kind: PrincipalKind::AgentConnection,
        principal_id: "agent-connection-1".to_string(),
        workspace_id: "workspace-1".to_string(),
        scope_ids: BTreeSet::from(["approval-1".to_string()]),
        authority: BTreeSet::from([Authority::DecideApproval]),
    };
    let token = support::shared_test_token();
    let mut server = Server::bind(
        ServerConfig::new(pipe_name.clone(), token.clone())
            .with_credential_resolver(std::sync::Arc::new(CredentialSet::single(token, principal))),
    )
    .expect("bind server");
    let task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });
    let (mut writer, mut reader) = support::raw_handshake(&pipe_name).await;
    let correlation_id = Uuid::new_v4();
    write_frame(
        &mut writer,
        &WireMessage::Call {
            correlation_id,
            deadline_unix_millis: u64::MAX,
            method: Method::ApprovalGrant(ApprovalGrantParams {
                approval_id: "approval-1".to_string(),
                bound_request_digest: "fedcba9876543210fedcba9876543210".to_string(),
                idempotency_key: "idem-1".to_string(),
            }),
        },
    )
    .await
    .expect("write request");
    let response = read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
        .await
        .expect("read")
        .expect("response");
    assert!(
        matches!(response, WireMessage::CallResult { correlation_id: id, outcome: CallOutcome::Err { error: RpcError::PermissionDenied } } if id == correlation_id)
    );
    drop(writer);
    drop(reader);
    task.await.expect("server task").expect("clean disconnect");
}

#[tokio::test]
async fn oversized_business_request_is_rejected_during_frame_decode() {
    let oversized = "x".repeat(201);
    let message = WireMessage::Call {
        correlation_id: Uuid::new_v4(),
        deadline_unix_millis: u64::MAX,
        method: Method::DeviceGet(loki_ipc::business::DeviceParams {
            device_id: oversized,
        }),
    };
    let (mut client, mut server) = tokio::io::duplex(8_192);
    write_unvalidated(&mut client, &message).await;
    let error = read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES)
        .await
        .expect_err("semantic bound must reject");
    assert!(matches!(
        error,
        loki_ipc::frame::FrameError::InvalidContract(_)
    ));
}

#[tokio::test]
async fn invalid_identifier_characters_are_rejected_during_frame_decode() {
    let message = WireMessage::Call {
        correlation_id: Uuid::new_v4(),
        deadline_unix_millis: u64::MAX,
        method: Method::DeviceGet(loki_ipc::business::DeviceParams {
            device_id: "dev/escape".to_string(),
        }),
    };
    let (mut client, mut server) = tokio::io::duplex(4_096);
    write_unvalidated(&mut client, &message).await;
    assert!(matches!(
        read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES).await,
        Err(loki_ipc::frame::FrameError::InvalidContract(_))
    ));
}

#[tokio::test]
async fn malformed_business_output_is_rejected_before_encode_and_after_decode() {
    let message = WireMessage::CallResult {
        correlation_id: Uuid::new_v4(),
        outcome: CallOutcome::Ok {
            result: MethodResult::Business(BusinessRpcResult::Operation {
                method: OperationMethod::RestartRequest,
                outcome: OperationOutcome::Rejected,
                correlation_id: "corr-1".to_string(),
                policy_reason: "😀".repeat(501),
                approval_id: None,
                job_id: None,
                bound_request_digest: None,
            }),
        },
    };
    let (mut client, mut server) = tokio::io::duplex(16_384);
    assert!(matches!(
        write_frame(&mut client, &message).await,
        Err(loki_ipc::frame::FrameError::InvalidContract(_))
    ));
    write_unvalidated(&mut client, &message).await;
    assert!(matches!(
        read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES).await,
        Err(loki_ipc::frame::FrameError::InvalidContract(_))
    ));
}

#[tokio::test]
async fn previous_transport_client_negotiates_only_core_and_remains_usable() {
    let pipe_name = support::unique_pipe_name("business-previous-edge");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");
    let task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });
    let mut config = support::client_config(&pipe_name);
    config.protocol_version = loki_ipc::protocol::PREVIOUS_PROTOCOL_VERSION;
    config.capabilities = loki_ipc::protocol::previous_capabilities();
    let client = loki_ipc::Client::connect(config)
        .await
        .expect("previous client connects");
    assert!(
        client
            .negotiated_capabilities()
            .contains(loki_ipc::protocol::CORE_CAPABILITY)
    );
    assert!(
        !client
            .negotiated_capabilities()
            .contains(loki_ipc::business::BUSINESS_CAPABILITY)
    );
    assert_eq!(
        client
            .ping("previous-edge", std::time::Duration::from_secs(2))
            .await
            .expect("ping")
            .payload,
        "previous-edge"
    );
    drop(client);
    task.await.expect("server task").expect("clean disconnect");
}

#[tokio::test]
async fn typed_business_result_and_stream_cross_a_real_named_pipe() {
    let pipe_name = support::unique_pipe_name("business-positive-wire");
    let token = support::shared_test_token();
    let principal = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-1",
        BTreeSet::from([Authority::ReadRuntime, Authority::Subscribe]),
    );
    let config = ServerConfig::new(pipe_name.clone(), token.clone())
        .with_credential_resolver(std::sync::Arc::new(CredentialSet::single(token, principal)))
        .with_business_service(std::sync::Arc::new(RealBusinessService));
    let mut server = Server::bind(config).expect("bind server");
    let task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });
    let client = loki_ipc::Client::connect(support::client_config(&pipe_name))
        .await
        .expect("connect");
    let result = client
        .business_call(
            Method::RuntimeGetStatus(EmptyParams {}),
            std::time::Duration::from_secs(2),
        )
        .await
        .expect("typed unary result");
    assert!(matches!(
        result,
        BusinessRpcResult::RuntimeStatus {
            state: RuntimeState::Connected,
            ..
        }
    ));
    let mut stream = client
        .business_stream(
            BusinessStreamParams {
                stream: BusinessStreamName::RuntimeStatus,
                workspace_id: "workspace-1".to_string(),
                scope_id: "runtime-1".to_string(),
                after_sequence: None,
                max_items: 2,
            },
            std::time::Duration::from_secs(2),
        )
        .await
        .expect("typed stream");
    assert!(matches!(
        stream.recv().await,
        Some(BusinessStreamEvent::Item(BusinessEvent { sequence: 1, .. }))
    ));
    assert!(matches!(
        stream.recv().await,
        Some(BusinessStreamEvent::Ended(Ok(BusinessStreamSummary {
            items_sent: 1,
            last_sequence: Some(1)
        })))
    ));
    drop(client);
    task.await.expect("server task").expect("clean disconnect");
}

#[tokio::test]
async fn distinct_credentials_resolve_distinct_scoped_principals_on_one_server() {
    let pipe_name = support::unique_pipe_name("business-multi-credential");
    let desktop_token = AuthToken::new("desktop-secret");
    let agent_token = AuthToken::new("agent-secret");
    let desktop = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-1",
        BTreeSet::from([Authority::ReadRuntime]),
    );
    let mut agent = scoped_principal(
        PrincipalKind::AgentConnection,
        "agent-1",
        BTreeSet::from([Authority::ReadJobs]),
    );
    agent.workspace_id = "workspace-2".to_string();
    agent.scope_ids = BTreeSet::from(["job-2".to_string()]);
    let resolver = CredentialSet::new(vec![
        CredentialRecord::active(desktop_token.clone(), desktop),
        CredentialRecord::active(agent_token.clone(), agent),
    ]);
    let config = ServerConfig::new(pipe_name.clone(), AuthToken::new("unused"))
        .with_credential_resolver(std::sync::Arc::new(resolver));
    let mut server = Server::bind(config).expect("bind server");
    let task = tokio::spawn(async move {
        let mut handles = Vec::new();
        for _ in 0..2 {
            let connection = server.accept().await.expect("accept");
            handles.push(tokio::spawn(connection.handle()));
        }
        for handle in handles {
            handle.await.expect("connection task").expect("disconnect");
        }
    });
    let desktop_client = loki_ipc::Client::connect(loki_ipc::ClientConfig::new(
        pipe_name.clone(),
        desktop_token,
        "desktop",
    ))
    .await
    .expect("desktop connect");
    let agent_client =
        loki_ipc::Client::connect(loki_ipc::ClientConfig::new(pipe_name, agent_token, "agent"))
            .await
            .expect("agent connect");
    let desktop_projection = desktop_client
        .authenticated_principal()
        .expect("desktop principal");
    let agent_projection = agent_client
        .authenticated_principal()
        .expect("agent principal");
    assert_eq!(desktop_projection.kind, PrincipalKind::DesktopInstallation);
    assert_eq!(desktop_projection.workspace_id, "workspace-1");
    assert_eq!(agent_projection.kind, PrincipalKind::AgentConnection);
    assert_eq!(agent_projection.workspace_id, "workspace-2");
    assert!(!agent_projection.authorizes(Authority::ReadRuntime));
    drop(desktop_client);
    drop(agent_client);
    task.await.expect("server task");
}

#[tokio::test]
async fn hostile_v1_0_business_advertisement_is_downgraded_and_denied() {
    let pipe_name = support::unique_pipe_name("business-hostile-downgrade");
    let token = support::shared_test_token();
    let principal = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-1",
        BTreeSet::from([Authority::ReadRuntime]),
    );
    let config = ServerConfig::new(pipe_name.clone(), token.clone())
        .with_credential_resolver(std::sync::Arc::new(CredentialSet::single(token, principal)));
    let mut server = Server::bind(config).expect("bind server");
    let task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });
    let mut client_config = support::client_config(&pipe_name);
    client_config.protocol_version = loki_ipc::protocol::PREVIOUS_PROTOCOL_VERSION;
    client_config.capabilities = loki_ipc::protocol::baseline_capabilities();
    let client = loki_ipc::Client::connect(client_config)
        .await
        .expect("connect");
    assert!(
        !client
            .negotiated_capabilities()
            .contains(loki_ipc::business::BUSINESS_CAPABILITY)
    );
    assert!(matches!(
        client
            .business_call(
                Method::RuntimeGetStatus(EmptyParams {}),
                std::time::Duration::from_secs(2)
            )
            .await,
        Err(loki_ipc::client::CallError::Rpc(
            RpcError::MissingCapability { .. }
        ))
    ));
    drop(client);
    task.await.expect("server task").expect("disconnect");
}
