//! End-to-end handshake tests over a real named pipe: successful handshake
//! with capability negotiation, and version-mismatch rejection (a clear,
//! typed error rather than a silent accept or a hang).
//!
//! `Docs/graph/tasks/RUN-002.md` Deliverable 3 and Deliverable 6.

mod support;

use std::collections::BTreeSet;

use loki_ipc::client::{Client, ConnectError};
use loki_ipc::protocol::{CORE_CAPABILITY, HandshakeRejectReason, ProtocolVersion};
use loki_ipc::server::Server;

#[tokio::test]
async fn successful_handshake_negotiates_the_core_capability_and_a_session_id() {
    let pipe_name = support::unique_pipe_name("handshake-ok");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");

    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let client = tokio::time::timeout(
        support::TEST_HANG_GUARD,
        Client::connect(support::client_config(&pipe_name)),
    )
    .await
    .expect("connect must not hang")
    .expect("handshake must succeed");

    assert!(client.negotiated_capabilities().contains(CORE_CAPABILITY));
    assert_ne!(client.session_id(), uuid::Uuid::nil());

    drop(client);
    let outcome = tokio::time::timeout(support::TEST_HANG_GUARD, server_task)
        .await
        .expect("server task must not hang")
        .expect("server task must not panic");
    assert!(
        outcome.is_ok(),
        "a clean client disconnect must not be reported as a connection error: {outcome:?}"
    );
}

#[tokio::test]
async fn version_major_mismatch_is_rejected_with_a_clear_typed_error_not_a_hang() {
    let pipe_name = support::unique_pipe_name("handshake-version-mismatch");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");

    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let mut config = support::client_config(&pipe_name);
    config.protocol_version = ProtocolVersion::new(9999, 0);

    let result = tokio::time::timeout(support::TEST_HANG_GUARD, Client::connect(config))
        .await
        .expect("connect must not hang even when the server will reject it");

    match result {
        Err(ConnectError::Rejected {
            reason: HandshakeRejectReason::VersionMismatch { client_version, .. },
            ..
        }) => {
            assert_eq!(client_version, ProtocolVersion::new(9999, 0));
        }
        other => panic!("expected a VersionMismatch rejection, got {other:?}"),
    }

    let outcome = tokio::time::timeout(support::TEST_HANG_GUARD, server_task)
        .await
        .expect("server task must not hang")
        .expect("server task must not panic");
    assert!(matches!(
        outcome,
        Err(loki_ipc::server::ConnectionError::HandshakeRejected(
            HandshakeRejectReason::VersionMismatch { .. }
        ))
    ));
}

#[tokio::test]
async fn missing_required_capability_is_rejected_not_silently_accepted() {
    let pipe_name = support::unique_pipe_name("handshake-missing-capability");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");

    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let mut config = support::client_config(&pipe_name);
    config.capabilities = BTreeSet::new(); // deliberately omits CORE_CAPABILITY

    let result = tokio::time::timeout(support::TEST_HANG_GUARD, Client::connect(config))
        .await
        .expect("connect must not hang");

    assert!(matches!(
        result,
        Err(ConnectError::Rejected {
            reason: HandshakeRejectReason::MissingCapability { .. },
            ..
        })
    ));

    let _ = tokio::time::timeout(support::TEST_HANG_GUARD, server_task)
        .await
        .expect("server task must not hang");
}

#[tokio::test]
async fn a_second_client_can_connect_after_the_first_disconnects() {
    // Proves the server's "always keep a spare instance ready" bookkeeping
    // (crate::server::Server::accept) actually works, not just once.
    let pipe_name = support::unique_pipe_name("handshake-sequential-clients");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");

    let server_task = tokio::spawn(async move {
        for _ in 0..2u8 {
            let connection = server.accept().await.expect("accept");
            connection.handle().await.expect("handle");
        }
    });

    for _ in 0..2u8 {
        let client = tokio::time::timeout(
            support::TEST_HANG_GUARD,
            Client::connect(support::client_config(&pipe_name)),
        )
        .await
        .expect("connect must not hang")
        .expect("handshake must succeed");
        drop(client);
    }

    tokio::time::timeout(support::TEST_HANG_GUARD, server_task)
        .await
        .expect("server task must not hang")
        .expect("server task must not panic");
}
