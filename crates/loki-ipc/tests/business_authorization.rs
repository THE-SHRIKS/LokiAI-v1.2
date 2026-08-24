mod support;

use std::collections::BTreeSet;

use loki_ipc::business::{ApprovalGrantParams, AuthenticatedPrincipal, Authority, PrincipalKind};
use loki_ipc::frame::{DEFAULT_MAX_FRAME_BYTES, read_frame, write_frame};
use loki_ipc::protocol::{CallOutcome, Method, RpcError, WireMessage};
use loki_ipc::server::{Server, ServerConfig};
use uuid::Uuid;

#[tokio::test]
async fn hello_ack_projects_only_the_server_created_principal() {
    let pipe_name = support::unique_pipe_name("business-server-principal");
    let principal = AuthenticatedPrincipal {
        kind: PrincipalKind::DesktopInstallation,
        principal_id: "desktop-installation-1".to_string(),
        authority: BTreeSet::from([Authority::ReadRuntime]),
    };
    let mut server = Server::bind(
        ServerConfig::new(pipe_name.clone(), support::shared_test_token())
            .with_principal(principal),
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
        authority: BTreeSet::from([Authority::DecideApproval]),
    };
    let mut server = Server::bind(
        ServerConfig::new(pipe_name.clone(), support::shared_test_token())
            .with_principal(principal),
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
    write_frame(&mut client, &message)
        .await
        .expect("write syntactically valid frame");
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
    write_frame(&mut client, &message)
        .await
        .expect("write syntactically valid frame");
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
