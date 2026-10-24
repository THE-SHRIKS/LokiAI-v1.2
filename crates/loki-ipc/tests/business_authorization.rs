mod support;

use std::collections::BTreeSet;

use loki_ipc::auth::{
    AuthToken, CredentialRecord, CredentialResolver, CredentialSet, ResolvedCredential,
};
use loki_ipc::business::{ApprovalGrantParams, AuthenticatedPrincipal, Authority, PrincipalKind};
use loki_ipc::business::{
    BusinessEvent, BusinessRpcResult, BusinessStreamName, BusinessStreamParams,
    BusinessStreamSummary, EmptyParams, OperationMethod, OperationOutcome, ReadMeta, RuntimeState,
};
use loki_ipc::client::BusinessStreamEvent;
use loki_ipc::frame::{DEFAULT_MAX_FRAME_BYTES, read_frame, write_frame};
use loki_ipc::protocol::{CallOutcome, Method, MethodResult, RpcError, WireMessage};
use loki_ipc::protocol::{business_result_matches_method, required_method_scopes};
use loki_ipc::server::{
    BusinessCallFuture, BusinessEventEmitter, BusinessRequestContext, BusinessService,
    BusinessStreamFuture, Server, ServerConfig,
};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio::sync::Notify;
use uuid::Uuid;

#[derive(Deserialize)]
struct FixtureDocument {
    requests: Vec<Method>,
    results: Vec<BusinessRpcResult>,
    streams: Vec<BusinessStreamParams>,
}

struct FixtureBusinessService {
    results: Vec<BusinessRpcResult>,
}

impl BusinessService for FixtureBusinessService {
    fn call(&self, method: Method, context: BusinessRequestContext) -> BusinessCallFuture {
        let mut result = self
            .results
            .iter()
            .find(|result| business_result_matches_method(&method, result))
            .cloned();
        Box::pin(async move {
            let result = result.as_mut().ok_or_else(|| RpcError::UnsupportedMethod {
                method: "fixture".to_string(),
            })?;
            match result {
                BusinessRpcResult::RuntimeStatus { meta, .. }
                | BusinessRpcResult::DevicesList { meta, .. }
                | BusinessRpcResult::DeviceGet { meta, .. }
                | BusinessRpcResult::DeviceHealth { meta, .. }
                | BusinessRpcResult::DeviceLogs { meta, .. }
                | BusinessRpcResult::DeviceTelemetry { meta, .. }
                | BusinessRpcResult::IncidentsList { meta, .. }
                | BusinessRpcResult::IncidentGet { meta, .. }
                | BusinessRpcResult::JobGet { meta, .. }
                | BusinessRpcResult::AgentRun { meta, .. }
                | BusinessRpcResult::WorkIntentAcknowledged { meta, .. } => {
                    meta.correlation_id = context.correlation_id.to_string();
                }
                BusinessRpcResult::Operation {
                    correlation_id,
                    scope_ids,
                    ..
                } => {
                    *correlation_id = context.correlation_id.to_string();
                    *scope_ids = required_method_scopes(&method, &context.principal.workspace_id);
                }
            }
            if matches!(result, BusinessRpcResult::Operation { .. }) {
                context.authorize_commit()?;
            }
            Ok(result.clone())
        })
    }

    fn stream(
        &self,
        params: BusinessStreamParams,
        context: BusinessRequestContext,
        emitter: BusinessEventEmitter,
    ) -> BusinessStreamFuture {
        RealBusinessService.stream(params, context, emitter)
    }
}

#[derive(Clone)]
struct MutableCredential {
    token: String,
    principal: AuthenticatedPrincipal,
    expires_at: u64,
    revoked: bool,
}

struct MutableResolver {
    records: std::sync::RwLock<std::collections::HashMap<String, MutableCredential>>,
    now: std::sync::atomic::AtomicU64,
}

impl MutableResolver {
    fn new(records: impl IntoIterator<Item = (String, MutableCredential)>) -> Self {
        Self {
            records: std::sync::RwLock::new(records.into_iter().collect()),
            now: std::sync::atomic::AtomicU64::new(1_000),
        }
    }

    fn revoke(&self, credential_id: &str) {
        self.records
            .write()
            .unwrap()
            .get_mut(credential_id)
            .unwrap()
            .revoked = true;
    }

    fn replace_principal(&self, credential_id: &str, principal: AuthenticatedPrincipal) {
        self.records
            .write()
            .unwrap()
            .get_mut(credential_id)
            .unwrap()
            .principal = principal;
    }

    fn set_now(&self, now: u64) {
        self.now.store(now, std::sync::atomic::Ordering::SeqCst);
    }

    fn current_time(&self) -> u64 {
        self.now.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl CredentialResolver for MutableResolver {
    fn resolve(&self, token: &str, _now_unix_millis: u64) -> Option<ResolvedCredential> {
        let now = self.current_time();
        self.records
            .read()
            .unwrap()
            .iter()
            .find_map(|(id, record)| {
                (record.token == token && !record.revoked && now < record.expires_at).then(|| {
                    ResolvedCredential {
                        credential_id: id.clone(),
                        principal: record.principal.clone(),
                    }
                })
            })
    }

    fn revalidate(
        &self,
        credential_id: &str,
        _now_unix_millis: u64,
    ) -> Option<AuthenticatedPrincipal> {
        let now = self.current_time();
        self.records
            .read()
            .unwrap()
            .get(credential_id)
            .and_then(|record| {
                (!record.revoked && now < record.expires_at).then(|| record.principal.clone())
            })
    }
}

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

struct GatedStreamService {
    first_sent: std::sync::Arc<Notify>,
    resume: std::sync::Arc<Notify>,
}

struct RejectProbeService {
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl BusinessService for RejectProbeService {
    fn call(&self, _method: Method, _context: BusinessRequestContext) -> BusinessCallFuture {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async {
            Err(RpcError::Internal {
                message: "scope authorization was bypassed".to_string(),
            })
        })
    }

    fn stream(
        &self,
        _params: BusinessStreamParams,
        _context: BusinessRequestContext,
        _emitter: BusinessEventEmitter,
    ) -> BusinessStreamFuture {
        Box::pin(async {
            Err(RpcError::Internal {
                message: "unexpected stream".to_string(),
            })
        })
    }
}

struct BlockingCallService {
    started: std::sync::Arc<Notify>,
    dropped: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

struct DropSignal(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl BusinessService for BlockingCallService {
    fn call(&self, _method: Method, _context: BusinessRequestContext) -> BusinessCallFuture {
        let started = self.started.clone();
        let dropped = self.dropped.clone();
        Box::pin(async move {
            let _drop_signal = DropSignal(dropped);
            started.notify_one();
            std::future::pending::<Result<BusinessRpcResult, RpcError>>().await
        })
    }

    fn stream(
        &self,
        _params: BusinessStreamParams,
        _context: BusinessRequestContext,
        _emitter: BusinessEventEmitter,
    ) -> BusinessStreamFuture {
        Box::pin(async {
            Err(RpcError::Internal {
                message: "unexpected stream".to_string(),
            })
        })
    }
}

struct MidFlightCommitService {
    started: std::sync::Arc<Notify>,
    resume: std::sync::Arc<Notify>,
    committed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl BusinessService for MidFlightCommitService {
    fn call(&self, method: Method, context: BusinessRequestContext) -> BusinessCallFuture {
        let started = self.started.clone();
        let resume = self.resume.clone();
        let committed = self.committed.clone();
        Box::pin(async move {
            let Method::RestartRequest(params) = method else {
                return Err(RpcError::Internal {
                    message: "unexpected method".to_string(),
                });
            };
            started.notify_one();
            resume.notified().await;
            context.authorize_commit()?;
            committed.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(BusinessRpcResult::Operation {
                method: OperationMethod::RestartRequest,
                outcome: OperationOutcome::Accepted,
                correlation_id: context.correlation_id.to_string(),
                policy_reason: "accepted".to_string(),
                idempotency_key: params.idempotency_key,
                artifact_digest: None,
                request_reason: None,
                approval_id: None,
                job_id: Some("job-1".to_string()),
                bound_request_digest: None,
                scope_ids: BTreeSet::from([format!("device:{}", params.device_id)]),
            })
        })
    }

    fn stream(
        &self,
        _params: BusinessStreamParams,
        _context: BusinessRequestContext,
        _emitter: BusinessEventEmitter,
    ) -> BusinessStreamFuture {
        Box::pin(async {
            Err(RpcError::Internal {
                message: "unexpected stream".to_string(),
            })
        })
    }
}

struct PacingBusinessService {
    progress: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl BusinessService for PacingBusinessService {
    fn call(&self, method: Method, context: BusinessRequestContext) -> BusinessCallFuture {
        RealBusinessService.call(method, context)
    }

    fn stream(
        &self,
        params: BusinessStreamParams,
        context: BusinessRequestContext,
        emitter: BusinessEventEmitter,
    ) -> BusinessStreamFuture {
        let progress = self.progress.clone();
        Box::pin(async move {
            for sequence in 1..=params.max_items {
                emitter
                    .send(BusinessEvent {
                        stream: params.stream,
                        sequence: u64::from(sequence),
                        correlation_id: context.correlation_id.to_string(),
                        occurred_at: "2026-08-24T12:00:00Z".to_string(),
                        entity_id: params.scope_id.clone(),
                        state: "x".repeat(100),
                        source_runtime_id: "runtime-1".to_string(),
                    })
                    .await?;
                progress.store(usize::from(sequence), std::sync::atomic::Ordering::SeqCst);
            }
            Ok(BusinessStreamSummary {
                items_sent: params.max_items,
                last_sequence: Some(u64::from(params.max_items)),
            })
        })
    }
}

impl BusinessService for GatedStreamService {
    fn call(&self, method: Method, context: BusinessRequestContext) -> BusinessCallFuture {
        RealBusinessService.call(method, context)
    }

    fn stream(
        &self,
        params: BusinessStreamParams,
        context: BusinessRequestContext,
        emitter: BusinessEventEmitter,
    ) -> BusinessStreamFuture {
        let first_sent = std::sync::Arc::clone(&self.first_sent);
        let resume = std::sync::Arc::clone(&self.resume);
        Box::pin(async move {
            for sequence in 1..=2 {
                emitter
                    .send(BusinessEvent {
                        stream: params.stream,
                        sequence,
                        correlation_id: context.correlation_id.to_string(),
                        occurred_at: "2026-08-24T12:00:00Z".to_string(),
                        entity_id: params.scope_id.clone(),
                        state: "connected".to_string(),
                        source_runtime_id: "runtime-1".to_string(),
                    })
                    .await?;
                if sequence == 1 {
                    first_sent.notify_one();
                    resume.notified().await;
                }
            }
            Ok(BusinessStreamSummary {
                items_sent: 2,
                last_sequence: Some(2),
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
        scope_ids: BTreeSet::from([
            "runtime:runtime-1".to_string(),
            "workspace:workspace-1".to_string(),
        ]),
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
        scope_ids: BTreeSet::from(["device:device-1".to_string()]),
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
        scope_ids: BTreeSet::from(["approval:approval-1".to_string()]),
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
                idempotency_key: "idem-1".to_string(),
                artifact_digest: None,
                request_reason: None,
                approval_id: None,
                job_id: None,
                bound_request_digest: None,
                scope_ids: BTreeSet::from(["device:dev-1".to_string()]),
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
    agent.scope_ids = BTreeSet::from(["job:job-2".to_string()]);
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

#[tokio::test]
async fn established_sessions_revalidate_revocation_and_principal_replacement_independently() {
    let pipe_name = support::unique_pipe_name("business-live-revalidation");
    let token_a = AuthToken::new("credential-a-token");
    let token_b = AuthToken::new("credential-b-token");
    let principal_a = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-a",
        BTreeSet::from([Authority::ReadRuntime, Authority::Subscribe]),
    );
    let principal_b = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-b",
        BTreeSet::from([Authority::ReadRuntime]),
    );
    let resolver = std::sync::Arc::new(MutableResolver::new([
        (
            "credential-a".to_string(),
            MutableCredential {
                token: token_a.as_str().to_string(),
                principal: principal_a,
                expires_at: 10_000,
                revoked: false,
            },
        ),
        (
            "credential-b".to_string(),
            MutableCredential {
                token: token_b.as_str().to_string(),
                principal: principal_b.clone(),
                expires_at: 10_000,
                revoked: false,
            },
        ),
    ]));
    let config = ServerConfig::new(pipe_name.clone(), AuthToken::new("unused"))
        .with_credential_resolver(resolver.clone())
        .with_business_service(std::sync::Arc::new(RealBusinessService));
    let mut server = Server::bind(config).expect("bind");
    let task = tokio::spawn(async move {
        let mut handles = Vec::new();
        for _ in 0..3 {
            handles.push(tokio::spawn(
                server.accept().await.expect("accept").handle(),
            ));
        }
        for handle in handles {
            match handle.await.expect("task") {
                Ok(())
                | Err(loki_ipc::server::ConnectionError::HandshakeRejected(
                    loki_ipc::protocol::HandshakeRejectReason::AuthenticationFailed,
                )) => {}
                Err(error) => panic!("unexpected server connection result: {error}"),
            }
        }
    });
    let client_a = loki_ipc::Client::connect(loki_ipc::ClientConfig::new(
        pipe_name.clone(),
        token_a,
        "desktop-a",
    ))
    .await
    .expect("connect a");
    let client_b = loki_ipc::Client::connect(loki_ipc::ClientConfig::new(
        pipe_name.clone(),
        token_b,
        "desktop-b",
    ))
    .await
    .expect("connect b");
    resolver.revoke("credential-a");
    assert!(matches!(
        client_a
            .business_call(
                Method::RuntimeGetStatus(EmptyParams {}),
                std::time::Duration::from_secs(2)
            )
            .await,
        Err(loki_ipc::client::CallError::Rpc(
            RpcError::CredentialInvalidated
        ))
    ));
    let mut denied_stream = client_a
        .business_stream(
            BusinessStreamParams {
                stream: BusinessStreamName::RuntimeStatus,
                workspace_id: "workspace-1".to_string(),
                scope_id: "runtime-1".to_string(),
                after_sequence: None,
                max_items: 1,
            },
            std::time::Duration::from_secs(2),
        )
        .await
        .expect("stream request is queued before server-side rejection");
    assert!(matches!(
        denied_stream.recv().await,
        Some(BusinessStreamEvent::Ended(Err(
            RpcError::CredentialInvalidated
        )))
    ));
    assert!(matches!(
        loki_ipc::Client::connect(loki_ipc::ClientConfig::new(
            pipe_name,
            AuthToken::new("credential-a-token"),
            "desktop-a-reconnect",
        ))
        .await,
        Err(loki_ipc::client::ConnectError::Rejected {
            reason: loki_ipc::protocol::HandshakeRejectReason::AuthenticationFailed,
            ..
        })
    ));
    client_b
        .business_call(
            Method::RuntimeGetStatus(EmptyParams {}),
            std::time::Duration::from_secs(2),
        )
        .await
        .expect("replacement credential remains unaffected");
    let mut changed = principal_b;
    changed.authority.clear();
    resolver.replace_principal("credential-b", changed);
    assert!(matches!(
        client_b
            .business_call(
                Method::RuntimeGetStatus(EmptyParams {}),
                std::time::Duration::from_secs(2)
            )
            .await,
        Err(loki_ipc::client::CallError::Rpc(
            RpcError::CredentialInvalidated
        ))
    ));
    drop(client_a);
    drop(client_b);
    task.await.expect("server");
}

#[tokio::test]
async fn post_handshake_expiry_denies_calls_and_reconnect() {
    let pipe_name = support::unique_pipe_name("business-live-expiry");
    let token = AuthToken::new("expiring-token");
    let principal = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-expiring",
        BTreeSet::from([Authority::ReadRuntime]),
    );
    let resolver = std::sync::Arc::new(MutableResolver::new([(
        "expiring".to_string(),
        MutableCredential {
            token: token.as_str().to_string(),
            principal,
            expires_at: 1_500,
            revoked: false,
        },
    )]));
    let config = ServerConfig::new(pipe_name.clone(), AuthToken::new("unused"))
        .with_credential_resolver(resolver.clone())
        .with_business_service(std::sync::Arc::new(RealBusinessService));
    let mut server = Server::bind(config).expect("bind");
    let task = tokio::spawn(async move {
        let first = tokio::spawn(server.accept().await.expect("accept").handle());
        let second = tokio::spawn(server.accept().await.expect("accept").handle());
        let _ = first.await;
        let _ = second.await;
    });
    let config = || loki_ipc::ClientConfig::new(pipe_name.clone(), token.clone(), "desktop");
    let client = loki_ipc::Client::connect(config())
        .await
        .expect("connect before expiry");
    resolver.set_now(2_000);
    assert!(matches!(
        client
            .business_call(
                Method::RuntimeGetStatus(EmptyParams {}),
                std::time::Duration::from_secs(2)
            )
            .await,
        Err(loki_ipc::client::CallError::Rpc(
            RpcError::CredentialInvalidated
        ))
    ));
    assert!(matches!(
        loki_ipc::Client::connect(config()).await,
        Err(loki_ipc::client::ConnectError::Rejected {
            reason: loki_ipc::protocol::HandshakeRejectReason::AuthenticationFailed,
            ..
        })
    ));
    drop(client);
    task.await.expect("server");
}

#[tokio::test]
async fn active_stream_terminates_when_credential_is_revoked() {
    let pipe_name = support::unique_pipe_name("business-active-revoke");
    let token = AuthToken::new("stream-token");
    let principal = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-stream",
        BTreeSet::from([Authority::ReadRuntime, Authority::Subscribe]),
    );
    let resolver = std::sync::Arc::new(MutableResolver::new([(
        "stream-credential".to_string(),
        MutableCredential {
            token: token.as_str().to_string(),
            principal,
            expires_at: 10_000,
            revoked: false,
        },
    )]));
    let first_sent = std::sync::Arc::new(Notify::new());
    let resume = std::sync::Arc::new(Notify::new());
    let service = GatedStreamService {
        first_sent: first_sent.clone(),
        resume: resume.clone(),
    };
    let config = ServerConfig::new(pipe_name.clone(), AuthToken::new("unused"))
        .with_credential_resolver(resolver.clone())
        .with_business_service(std::sync::Arc::new(service));
    let mut server = Server::bind(config).expect("bind");
    let task = tokio::spawn(async move { server.accept().await.expect("accept").handle().await });
    let client =
        loki_ipc::Client::connect(loki_ipc::ClientConfig::new(pipe_name, token, "desktop"))
            .await
            .expect("connect");
    let mut stream = client
        .business_stream(
            BusinessStreamParams {
                stream: BusinessStreamName::RuntimeStatus,
                workspace_id: "workspace-1".to_string(),
                scope_id: "runtime-1".to_string(),
                after_sequence: None,
                max_items: 2,
            },
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("stream");
    first_sent.notified().await;
    assert!(matches!(
        stream.recv().await,
        Some(BusinessStreamEvent::Item(_))
    ));
    resolver.revoke("stream-credential");
    resume.notify_one();
    assert!(matches!(
        stream.recv().await,
        Some(BusinessStreamEvent::Ended(Err(
            RpcError::CredentialInvalidated
        )))
    ));
    drop(client);
    task.await.expect("task").expect("disconnect");
}

#[derive(Clone, Copy)]
enum MidFlightAuthorityLoss {
    Revocation,
    Expiry,
    PrincipalReplacement,
}

impl MidFlightAuthorityLoss {
    fn pipe_suffix(self) -> &'static str {
        match self {
            Self::Revocation => "revoke",
            Self::Expiry => "expiry",
            Self::PrincipalReplacement => "principal",
        }
    }
}

async fn assert_in_flight_consequential_call_cannot_commit(loss: MidFlightAuthorityLoss) {
    let pipe_name =
        support::unique_pipe_name(&format!("business-mid-flight-{}", loss.pipe_suffix()));
    let token = AuthToken::new("mid-flight-token");
    let principal = AuthenticatedPrincipal {
        kind: PrincipalKind::DesktopInstallation,
        principal_id: "desktop-mid-flight".to_string(),
        workspace_id: "workspace-1".to_string(),
        scope_ids: BTreeSet::from(["device:dev-1".to_string()]),
        authority: BTreeSet::from([Authority::RequestDeviceOperation]),
    };
    let resolver = std::sync::Arc::new(MutableResolver::new([(
        "mid-flight-credential".to_string(),
        MutableCredential {
            token: token.as_str().to_string(),
            principal,
            expires_at: 10_000,
            revoked: false,
        },
    )]));
    let started = std::sync::Arc::new(Notify::new());
    let resume = std::sync::Arc::new(Notify::new());
    let committed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let service = MidFlightCommitService {
        started: started.clone(),
        resume: resume.clone(),
        committed: committed.clone(),
    };
    let config = ServerConfig::new(pipe_name.clone(), AuthToken::new("unused"))
        .with_credential_resolver(resolver.clone())
        .with_business_service(std::sync::Arc::new(service));
    let mut server = Server::bind(config).expect("bind");
    let task = tokio::spawn(async move { server.accept().await.expect("accept").handle().await });
    let client = loki_ipc::Client::connect(loki_ipc::ClientConfig::new(
        pipe_name,
        token,
        "desktop-mid-flight",
    ))
    .await
    .expect("connect");
    {
        let call = client.business_call(
            Method::RestartRequest(loki_ipc::business::IdempotentDeviceParams {
                device_id: "dev-1".to_string(),
                idempotency_key: "idem-mid-flight".to_string(),
            }),
            std::time::Duration::from_secs(5),
        );
        tokio::pin!(call);
        tokio::select! {
            () = started.notified() => {}
            result = &mut call => panic!("call completed before commit boundary: {result:?}"),
        }
        match loss {
            MidFlightAuthorityLoss::Revocation => resolver.revoke("mid-flight-credential"),
            MidFlightAuthorityLoss::Expiry => resolver.set_now(10_000),
            MidFlightAuthorityLoss::PrincipalReplacement => resolver.replace_principal(
                "mid-flight-credential",
                AuthenticatedPrincipal {
                    kind: PrincipalKind::DesktopInstallation,
                    principal_id: "desktop-mid-flight-replaced".to_string(),
                    workspace_id: "workspace-1".to_string(),
                    scope_ids: BTreeSet::from(["device:dev-1".to_string()]),
                    authority: BTreeSet::from([Authority::RequestDeviceOperation]),
                },
            ),
        }
        resume.notify_one();
        assert!(matches!(
            call.await,
            Err(loki_ipc::client::CallError::Rpc(
                RpcError::CredentialInvalidated
            ))
        ));
    }
    assert!(!committed.load(std::sync::atomic::Ordering::SeqCst));
    drop(client);
    task.await.expect("task").expect("disconnect");
}

#[tokio::test]
async fn in_flight_consequential_call_cannot_commit_after_revocation() {
    assert_in_flight_consequential_call_cannot_commit(MidFlightAuthorityLoss::Revocation).await;
}

#[tokio::test]
async fn in_flight_consequential_call_cannot_commit_after_expiry() {
    assert_in_flight_consequential_call_cannot_commit(MidFlightAuthorityLoss::Expiry).await;
}

#[tokio::test]
async fn in_flight_consequential_call_cannot_commit_after_principal_replacement() {
    assert_in_flight_consequential_call_cannot_commit(MidFlightAuthorityLoss::PrincipalReplacement)
        .await;
}

#[tokio::test]
async fn all_23_unary_methods_and_all_10_streams_cross_a_real_named_pipe() {
    let fixture: FixtureDocument = serde_json::from_str(include_str!(
        "../../../packages/contracts/fixtures/business-rpc.json"
    ))
    .expect("fixture");
    let mut scopes = fixture
        .requests
        .iter()
        .flat_map(|method| required_method_scopes(method, "workspace-1"))
        .collect::<BTreeSet<_>>();
    for params in &fixture.streams {
        let kind = match params.stream {
            BusinessStreamName::RuntimeStatus => "runtime",
            BusinessStreamName::DeviceProjection
            | BusinessStreamName::DeviceHealth
            | BusinessStreamName::Telemetry
            | BusinessStreamName::Logs => "device",
            BusinessStreamName::Incidents => "incident",
            BusinessStreamName::Jobs => "job",
            BusinessStreamName::Approvals => "approval",
            BusinessStreamName::Notifications => "notification",
            BusinessStreamName::WorkIntents => "work_intent",
        };
        scopes.insert(format!("{kind}:{}", params.scope_id));
    }
    let principal = AuthenticatedPrincipal {
        kind: PrincipalKind::DesktopInstallation,
        principal_id: "desktop-complete".to_string(),
        workspace_id: "workspace-1".to_string(),
        scope_ids: scopes,
        authority: BTreeSet::from([
            Authority::ReadRuntime,
            Authority::ReadDevices,
            Authority::ReadTelemetry,
            Authority::ReadIncidents,
            Authority::ReadJobs,
            Authority::ManageAgentRuns,
            Authority::AcknowledgeWorkIntents,
            Authority::RequestDiagnostics,
            Authority::CancelJobs,
            Authority::RequestBuild,
            Authority::RequestDeviceOperation,
            Authority::DecideApproval,
            Authority::ReadApprovals,
            Authority::ReadNotifications,
            Authority::ReadWorkIntents,
            Authority::Subscribe,
        ]),
    };
    let pipe_name = support::unique_pipe_name("business-complete-wire");
    let token = AuthToken::new("complete-token");
    let service = FixtureBusinessService {
        results: fixture.results,
    };
    let config = ServerConfig::new(pipe_name.clone(), token.clone())
        .with_credential_resolver(std::sync::Arc::new(CredentialSet::single(
            token.clone(),
            principal,
        )))
        .with_business_service(std::sync::Arc::new(service));
    let mut server = Server::bind(config).expect("bind");
    let task = tokio::spawn(async move { server.accept().await.expect("accept").handle().await });
    let client = loki_ipc::Client::connect(loki_ipc::ClientConfig::new(
        pipe_name,
        token,
        "complete-client",
    ))
    .await
    .expect("connect");
    for method in fixture.requests {
        client
            .business_call(method, std::time::Duration::from_secs(2))
            .await
            .expect("each unary method returns its coupled type");
    }
    for params in fixture.streams {
        let mut stream = client
            .business_stream(params, std::time::Duration::from_secs(2))
            .await
            .expect("stream starts");
        assert!(matches!(
            stream.recv().await,
            Some(BusinessStreamEvent::Item(_))
        ));
        assert!(matches!(
            stream.recv().await,
            Some(BusinessStreamEvent::Ended(Ok(_)))
        ));
    }
    drop(client);
    task.await.expect("task").expect("disconnect");
}

#[tokio::test]
async fn all_23_unary_methods_are_denied_cross_scope_before_the_producer() {
    let fixture: FixtureDocument = serde_json::from_str(include_str!(
        "../../../packages/contracts/fixtures/business-rpc.json"
    ))
    .expect("fixture");
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for (index, method) in fixture.requests.into_iter().enumerate() {
        let required_scopes = required_method_scopes(&method, "workspace-1");
        let missing_scope = required_scopes
            .iter()
            .next()
            .expect("every business method declares at least one scope")
            .clone();
        let mut incomplete_scopes = required_scopes;
        incomplete_scopes.remove(&missing_scope);
        if incomplete_scopes.is_empty() {
            incomplete_scopes.insert("workspace:unrelated".to_string());
        }
        let principal = AuthenticatedPrincipal {
            kind: PrincipalKind::DesktopInstallation,
            principal_id: format!("desktop-wrong-scope-{index}"),
            workspace_id: "workspace-1".to_string(),
            scope_ids: incomplete_scopes,
            authority: BTreeSet::from([
                Authority::ReadRuntime,
                Authority::ReadDevices,
                Authority::ReadTelemetry,
                Authority::ReadIncidents,
                Authority::ReadJobs,
                Authority::ManageAgentRuns,
                Authority::AcknowledgeWorkIntents,
                Authority::RequestDiagnostics,
                Authority::CancelJobs,
                Authority::RequestBuild,
                Authority::RequestDeviceOperation,
                Authority::DecideApproval,
                Authority::ReadApprovals,
                Authority::ReadNotifications,
                Authority::ReadWorkIntents,
                Authority::Subscribe,
            ]),
        };
        let pipe_name = support::unique_pipe_name(&format!("business-cross-scope-{index}"));
        let token = AuthToken::new(format!("cross-scope-token-{index}"));
        let mut server = Server::bind(
            ServerConfig::new(pipe_name.clone(), token.clone())
                .with_credential_resolver(std::sync::Arc::new(CredentialSet::single(
                    token.clone(),
                    principal,
                )))
                .with_business_service(std::sync::Arc::new(RejectProbeService {
                    calls: calls.clone(),
                })),
        )
        .expect("bind");
        let task =
            tokio::spawn(async move { server.accept().await.expect("accept").handle().await });
        let client = loki_ipc::Client::connect(loki_ipc::ClientConfig::new(
            pipe_name,
            token,
            format!("cross-scope-client-{index}"),
        ))
        .await
        .expect("connect");
        assert!(
            matches!(
                client
                    .business_call(method, std::time::Duration::from_secs(2))
                    .await,
                Err(loki_ipc::client::CallError::Rpc(RpcError::PermissionDenied))
            ),
            "method {index} must be denied when required scope {missing_scope} is absent"
        );
        drop(client);
        task.await.expect("task").expect("disconnect");
    }
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn business_deadline_drops_blocking_producer_work() {
    let pipe_name = support::unique_pipe_name("business-deadline-drop");
    let token = support::shared_test_token();
    let principal = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-deadline",
        BTreeSet::from([Authority::ReadRuntime]),
    );
    let started = std::sync::Arc::new(Notify::new());
    let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut server = Server::bind(
        ServerConfig::new(pipe_name.clone(), token.clone())
            .with_credential_resolver(std::sync::Arc::new(CredentialSet::single(token, principal)))
            .with_business_service(std::sync::Arc::new(BlockingCallService {
                started: started.clone(),
                dropped: dropped.clone(),
            })),
    )
    .expect("bind");
    let task = tokio::spawn(async move { server.accept().await.expect("accept").handle().await });
    let client = loki_ipc::Client::connect(support::client_config(&pipe_name))
        .await
        .expect("connect");
    let started_wait = started.notified();
    let call = client.business_call(
        Method::RuntimeGetStatus(EmptyParams {}),
        std::time::Duration::from_millis(30),
    );
    let (result, ()) = tokio::join!(call, started_wait);
    assert!(matches!(
        result,
        Err(loki_ipc::client::CallError::Rpc(RpcError::DeadlineExceeded))
    ));
    assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
    drop(client);
    task.await.expect("task").expect("disconnect");
}

#[tokio::test]
async fn explicit_cancel_drops_blocking_business_producer_work() {
    let pipe_name = support::unique_pipe_name("business-cancel-drop");
    let token = support::shared_test_token();
    let principal = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-cancel",
        BTreeSet::from([Authority::ReadRuntime]),
    );
    let started = std::sync::Arc::new(Notify::new());
    let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut server = Server::bind(
        ServerConfig::new(pipe_name.clone(), token.clone())
            .with_credential_resolver(std::sync::Arc::new(CredentialSet::single(token, principal)))
            .with_business_service(std::sync::Arc::new(BlockingCallService {
                started: started.clone(),
                dropped: dropped.clone(),
            })),
    )
    .expect("bind");
    let task = tokio::spawn(async move { server.accept().await.expect("accept").handle().await });
    let (mut writer, mut reader) = support::raw_handshake(&pipe_name).await;
    let correlation_id = Uuid::new_v4();
    let started_wait = started.notified();
    write_frame(
        &mut writer,
        &WireMessage::Call {
            correlation_id,
            deadline_unix_millis: u64::MAX,
            method: Method::RuntimeGetStatus(EmptyParams {}),
        },
    )
    .await
    .expect("send blocking call");
    started_wait.await;
    write_frame(&mut writer, &WireMessage::CancelCall { correlation_id })
        .await
        .expect("send cancellation");
    assert!(matches!(
        read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
            .await
            .expect("read")
            .expect("result"),
        WireMessage::CallResult {
            correlation_id: id,
            outcome: CallOutcome::Err { error: RpcError::Cancelled },
        } if id == correlation_id
    ));
    assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
    drop(writer);
    drop(reader);
    task.await.expect("task").expect("disconnect");
}

#[tokio::test]
async fn duplicate_active_correlation_is_rejected_without_replacing_original_canceller() {
    let pipe_name = support::unique_pipe_name("business-duplicate-correlation");
    let token = support::shared_test_token();
    let principal = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-duplicate",
        BTreeSet::from([Authority::ReadRuntime]),
    );
    let started = std::sync::Arc::new(Notify::new());
    let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut config = ServerConfig::new(pipe_name.clone(), token.clone())
        .with_credential_resolver(std::sync::Arc::new(CredentialSet::single(token, principal)))
        .with_business_service(std::sync::Arc::new(BlockingCallService {
            started: started.clone(),
            dropped: dropped.clone(),
        }));
    config.max_in_flight_requests = 1;
    let mut server = Server::bind(config).expect("bind");
    let task = tokio::spawn(async move { server.accept().await.expect("accept").handle().await });
    let (mut writer, mut reader) = support::raw_handshake(&pipe_name).await;
    let correlation_id = Uuid::new_v4();
    write_frame(
        &mut writer,
        &WireMessage::Call {
            correlation_id,
            deadline_unix_millis: u64::MAX,
            method: Method::RuntimeGetStatus(EmptyParams {}),
        },
    )
    .await
    .expect("start original call");
    started.notified().await;
    write_frame(
        &mut writer,
        &WireMessage::CallStream {
            correlation_id,
            deadline_unix_millis: u64::MAX,
            method: loki_ipc::protocol::StreamMethod::EchoStream(
                loki_ipc::protocol::EchoStreamParams {
                    payload: "duplicate".to_string(),
                    count: 1,
                },
            ),
        },
    )
    .await
    .expect("send duplicate correlation");
    assert!(matches!(
        read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
            .await
            .expect("read duplicate rejection")
            .expect("duplicate rejection frame"),
        WireMessage::StreamEnd {
            correlation_id: id,
            outcome: loki_ipc::protocol::StreamOutcome::Err {
                error: RpcError::DuplicateCorrelationId
            },
        } if id == correlation_id
    ));
    write_frame(&mut writer, &WireMessage::CancelCall { correlation_id })
        .await
        .expect("cancel original");
    assert!(matches!(
        read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
            .await
            .expect("read original cancellation")
            .expect("original cancellation frame"),
        WireMessage::CallResult {
            correlation_id: id,
            outcome: CallOutcome::Err { error: RpcError::Cancelled },
        } if id == correlation_id
    ));
    assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
    drop(writer);
    drop(reader);
    task.await.expect("task").expect("disconnect");
}

#[tokio::test]
async fn in_flight_capacity_rejects_calls_and_streams_then_recovers() {
    let pipe_name = support::unique_pipe_name("business-dispatch-capacity");
    let token = support::shared_test_token();
    let principal = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-capacity",
        BTreeSet::from([Authority::ReadRuntime]),
    );
    let started = std::sync::Arc::new(Notify::new());
    let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut config = ServerConfig::new(pipe_name.clone(), token.clone())
        .with_credential_resolver(std::sync::Arc::new(CredentialSet::single(token, principal)))
        .with_business_service(std::sync::Arc::new(BlockingCallService {
            started: started.clone(),
            dropped: dropped.clone(),
        }));
    config.max_in_flight_requests = 1;
    let mut server = Server::bind(config).expect("bind");
    let task = tokio::spawn(async move { server.accept().await.expect("accept").handle().await });
    let (mut writer, mut reader) = support::raw_handshake(&pipe_name).await;
    let active_id = Uuid::new_v4();
    write_frame(
        &mut writer,
        &WireMessage::Call {
            correlation_id: active_id,
            deadline_unix_millis: u64::MAX,
            method: Method::RuntimeGetStatus(EmptyParams {}),
        },
    )
    .await
    .expect("start capacity holder");
    started.notified().await;

    let rejected_call_id = Uuid::new_v4();
    write_frame(
        &mut writer,
        &WireMessage::Call {
            correlation_id: rejected_call_id,
            deadline_unix_millis: u64::MAX,
            method: Method::Ping(loki_ipc::protocol::PingParams {
                payload: "over-capacity".to_string(),
            }),
        },
    )
    .await
    .expect("send over-capacity call");
    assert!(matches!(
        read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
            .await
            .expect("read capacity error")
            .expect("capacity frame"),
        WireMessage::CallResult {
            correlation_id: id,
            outcome: CallOutcome::Err {
                error: RpcError::TooManyInFlight { limit: 1 }
            },
        } if id == rejected_call_id
    ));

    let rejected_stream_id = Uuid::new_v4();
    write_frame(
        &mut writer,
        &WireMessage::CallStream {
            correlation_id: rejected_stream_id,
            deadline_unix_millis: u64::MAX,
            method: loki_ipc::protocol::StreamMethod::EchoStream(
                loki_ipc::protocol::EchoStreamParams {
                    payload: "over-capacity".to_string(),
                    count: 1,
                },
            ),
        },
    )
    .await
    .expect("send over-capacity stream");
    assert!(matches!(
        read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
            .await
            .expect("read stream capacity error")
            .expect("stream capacity frame"),
        WireMessage::StreamEnd {
            correlation_id: id,
            outcome: loki_ipc::protocol::StreamOutcome::Err {
                error: RpcError::TooManyInFlight { limit: 1 }
            },
        } if id == rejected_stream_id
    ));

    write_frame(
        &mut writer,
        &WireMessage::CancelCall {
            correlation_id: active_id,
        },
    )
    .await
    .expect("cancel capacity holder");
    assert!(matches!(
        read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
            .await
            .expect("read holder cancellation")
            .expect("holder cancellation frame"),
        WireMessage::CallResult {
            correlation_id: id,
            outcome: CallOutcome::Err { error: RpcError::Cancelled },
        } if id == active_id
    ));
    assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));

    let recovered_id = Uuid::new_v4();
    write_frame(
        &mut writer,
        &WireMessage::Call {
            correlation_id: recovered_id,
            deadline_unix_millis: u64::MAX,
            method: Method::Ping(loki_ipc::protocol::PingParams {
                payload: "recovered".to_string(),
            }),
        },
    )
    .await
    .expect("send recovered call");
    assert!(matches!(
        read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
            .await
            .expect("read recovered call")
            .expect("recovered frame"),
        WireMessage::CallResult {
            correlation_id: id,
            outcome: CallOutcome::Ok {
                result: MethodResult::Ping(loki_ipc::protocol::PongResult { payload, .. })
            },
        } if id == recovered_id && payload == "recovered"
    ));
    drop(writer);
    drop(reader);
    task.await.expect("task").expect("disconnect");
}

#[tokio::test]
async fn slow_business_consumer_paces_the_real_named_pipe_producer() {
    const COUNT: u16 = 1_024;
    let pipe_name = support::unique_pipe_name("business-backpressure");
    let token = support::shared_test_token();
    let principal = scoped_principal(
        PrincipalKind::DesktopInstallation,
        "desktop-pacing",
        BTreeSet::from([Authority::ReadRuntime, Authority::Subscribe]),
    );
    let progress = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut config = ServerConfig::new(pipe_name.clone(), token.clone())
        .with_credential_resolver(std::sync::Arc::new(CredentialSet::single(token, principal)))
        .with_business_service(std::sync::Arc::new(PacingBusinessService {
            progress: progress.clone(),
        }));
    config.outgoing_channel_capacity = 1;
    config.out_buffer_size = 256;
    config.in_buffer_size = 256;
    let mut server = Server::bind(config).expect("bind");
    let task = tokio::spawn(async move { server.accept().await.expect("accept").handle().await });
    let mut client_config = support::client_config(&pipe_name);
    client_config.stream_event_capacity = 1;
    let client = loki_ipc::Client::connect(client_config)
        .await
        .expect("connect");
    let mut stream = client
        .business_stream(
            BusinessStreamParams {
                stream: BusinessStreamName::RuntimeStatus,
                workspace_id: "workspace-1".to_string(),
                scope_id: "runtime-1".to_string(),
                after_sequence: None,
                max_items: COUNT,
            },
            std::time::Duration::from_secs(30),
        )
        .await
        .expect("stream");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        progress.load(std::sync::atomic::Ordering::SeqCst) < usize::from(COUNT),
        "bounded channels and pipe buffers must prevent free-running completion"
    );
    let mut received = 0_u16;
    loop {
        match tokio::time::timeout(support::TEST_HANG_GUARD, stream.recv())
            .await
            .expect("stream must progress")
            .expect("stream stays connected")
        {
            BusinessStreamEvent::Item(_) => received += 1,
            BusinessStreamEvent::Ended(result) => {
                assert_eq!(
                    result,
                    Ok(BusinessStreamSummary {
                        items_sent: COUNT,
                        last_sequence: Some(u64::from(COUNT)),
                    })
                );
                break;
            }
        }
    }
    assert_eq!(received, COUNT);
    assert_eq!(
        progress.load(std::sync::atomic::Ordering::SeqCst),
        usize::from(COUNT)
    );
    drop(client);
    task.await.expect("task").expect("disconnect");
}
