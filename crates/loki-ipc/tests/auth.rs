//! End-to-end connection-level authentication tests over a real named pipe:
//! wrong shared secret, replayed handshake (stale nonce), and the Windows
//! ACL restricting the pipe to a specific SID.
//!
//! `Docs/graph/tasks/RUN-002.md` Deliverable 2 and Deliverable 6
//! ("a second connection attempt from a different user session is
//! rejected (or: if this environment can't test cross-user-session
//! behavior, say so plainly and test what's testable)").
//!
//! # On the cross-user-session case specifically
//!
//! This CI/dev environment has exactly one interactive Windows user
//! account and no ability to provision a second one from a test (and doing
//! so would itself be a real device/OS-changing operation requiring human
//! approval per `AGENTS.md`, not something a test should do). What *is*
//! tested here, deterministically and without a second account
//! ([`acl_denies_a_pipe_restricted_to_a_different_sid`] below): a pipe
//! whose DACL is built to allow only some *other* real SID denies the
//! current process's own connection attempt. This exercises the exact same
//! `crate::acl` code path production use restricts to the current user
//! with — the only variable changed is *which* SID is on the allow list —
//! so it proves the enforcement mechanism itself works, not merely that
//! nothing was tested.

mod support;

use loki_ipc::acl::AclScope;
use loki_ipc::auth::AuthToken;
use loki_ipc::client::{Client, ConnectError};
use loki_ipc::protocol::HandshakeRejectReason;
use loki_ipc::server::{Server, ServerConfig};

#[tokio::test]
async fn wrong_shared_secret_is_rejected_as_authentication_failed() {
    let pipe_name = support::unique_pipe_name("auth-wrong-token");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let mut config = support::client_config(&pipe_name);
    config.token = AuthToken::new("definitely-the-wrong-secret");

    let result = tokio::time::timeout(support::TEST_HANG_GUARD, Client::connect(config))
        .await
        .expect("connect must not hang");

    assert!(
        matches!(
            result,
            Err(ConnectError::Rejected {
                reason: HandshakeRejectReason::AuthenticationFailed,
                ..
            })
        ),
        "expected AuthenticationFailed, got {result:?}"
    );

    let _ = tokio::time::timeout(support::TEST_HANG_GUARD, server_task).await;
}

#[tokio::test]
async fn replaying_a_captured_hello_from_an_earlier_connection_is_rejected_as_stale_nonce() {
    let pipe_name = support::unique_pipe_name("auth-replay");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");

    let server_task = tokio::spawn(async move {
        let mut outcomes = Vec::new();
        for _ in 0..2u8 {
            let connection = server.accept().await.expect("accept");
            outcomes.push(connection.handle().await);
        }
        outcomes
    });

    // Connection 1: perform a real handshake by hand so we capture the
    // exact bytes of a *valid* Hello (nonce N1 + the correct token).
    let captured_hello = {
        use loki_ipc::frame::{DEFAULT_MAX_FRAME_BYTES, read_frame, write_frame};
        use loki_ipc::protocol::{PROTOCOL_VERSION, WireMessage, baseline_capabilities};

        let pipe = support::raw_connect(&pipe_name).await;
        let (mut reader, mut writer) = tokio::io::split(pipe);

        let challenge = read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
            .await
            .expect("read challenge")
            .expect("challenge present");
        let nonce = match challenge {
            WireMessage::Challenge { nonce, .. } => nonce,
            other => panic!("expected Challenge, got {other:?}"),
        };

        let hello = WireMessage::Hello {
            client_protocol_version: PROTOCOL_VERSION,
            client_capabilities: baseline_capabilities(),
            client_type: "replay-test-client".to_string(),
            client_version: "0.0.0".to_string(),
            nonce,
            token: support::shared_test_token().as_str().to_string(),
        };
        write_frame(&mut writer, &hello).await.expect("write hello");

        let ack = read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
            .await
            .expect("read ack")
            .expect("ack present");
        assert!(
            matches!(ack, WireMessage::HelloAck { .. }),
            "connection 1's real handshake must succeed"
        );

        // Connection 1 is now handshaken; drop it so the server's dispatch
        // loop for it ends and its accept loop moves on to connection 2.
        drop(writer);
        drop(reader);

        hello
    };

    // Connection 2: a brand-new connection gets its own fresh nonce (N2 !=
    // N1). Replay the *exact* Hello captured from connection 1 (still
    // carrying N1) onto this new connection.
    {
        use loki_ipc::frame::{DEFAULT_MAX_FRAME_BYTES, read_frame, write_frame};
        use loki_ipc::protocol::WireMessage;

        let pipe = support::raw_connect(&pipe_name).await;
        let (mut reader, mut writer) = tokio::io::split(pipe);

        let challenge = read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
            .await
            .expect("read challenge")
            .expect("challenge present");
        let fresh_nonce = match challenge {
            WireMessage::Challenge { nonce, .. } => nonce,
            other => panic!("expected Challenge, got {other:?}"),
        };
        let WireMessage::Hello {
            nonce: captured_nonce,
            ..
        } = &captured_hello
        else {
            panic!("captured_hello must be a Hello");
        };
        assert_ne!(
            *captured_nonce, fresh_nonce,
            "the test is only meaningful if the two connections really got different nonces"
        );

        write_frame(&mut writer, &captured_hello)
            .await
            .expect("replay the captured hello verbatim");

        let reject = read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
            .await
            .expect("read hello-reject")
            .expect("hello-reject present");
        match reject {
            WireMessage::HelloReject {
                reason: HandshakeRejectReason::StaleNonce,
                ..
            } => {}
            other => panic!("expected HelloReject(StaleNonce) for a replayed Hello, got {other:?}"),
        }
    }

    let outcomes = tokio::time::timeout(support::TEST_HANG_GUARD, server_task)
        .await
        .expect("server must not hang")
        .expect("server task must not panic");
    assert!(
        outcomes[0].is_ok(),
        "connection 1 (the real handshake) must have succeeded"
    );
    assert!(matches!(
        outcomes[1],
        Err(loki_ipc::server::ConnectionError::HandshakeRejected(
            HandshakeRejectReason::StaleNonce
        ))
    ));
}

#[tokio::test]
async fn acl_allows_a_pipe_restricted_to_the_current_user() {
    // Positive control for the ACL mechanism: `AclScope::CurrentUser` (the
    // production default) must not lock out the very user it names.
    let pipe_name = support::unique_pipe_name("acl-allows-current-user");
    let mut config = ServerConfig::new(pipe_name.clone(), support::shared_test_token());
    config.acl_scope = AclScope::CurrentUser;
    let mut server = Server::bind(config).expect("bind server with CurrentUser ACL scope");

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
    .expect("the current user must be able to connect to a pipe scoped to the current user");
    drop(client);

    let _ = tokio::time::timeout(support::TEST_HANG_GUARD, server_task).await;
}

#[tokio::test]
async fn acl_denies_a_pipe_restricted_to_a_different_sid() {
    // See this file's module doc comment for why this is the real,
    // deterministic substitute for a second-Windows-user-account test in
    // this environment: the pipe's DACL allows only the well-known Null
    // SID (`S-1-0-0`), which can never be the SID of whatever real account
    // is running this test process. If the ACL wiring were broken (e.g.
    // silently ignored, falling back to the default "Everyone can read"
    // pipe descriptor), this test would fail by *succeeding* at connecting
    // -- it is not a test that can pass by accident.
    let pipe_name = support::unique_pipe_name("acl-denies-other-sid");
    let mut config = ServerConfig::new(pipe_name.clone(), support::shared_test_token());
    config.acl_scope = AclScope::OnlySid("S-1-0-0".to_string());
    let mut server = Server::bind(config).expect("bind server with a restricted ACL scope");

    // Keep the server's accept loop alive across the (failed) connection
    // attempt so a real ACL rejection (which happens at `CreateFile` time,
    // before the server's `accept()` even completes) can be distinguished
    // from "the server wasn't listening at all".
    let server_task = tokio::spawn(async move {
        tokio::time::timeout(support::TEST_HANG_GUARD, server.accept()).await
    });

    let config = support::client_config(&pipe_name);
    let open_result = tokio::time::timeout(support::TEST_HANG_GUARD, Client::connect(config))
        .await
        .expect("the OS must deny this connection promptly, not hang");
    match open_result {
        Err(ConnectError::Io(err)) => {
            assert_eq!(
                err.kind(),
                std::io::ErrorKind::PermissionDenied,
                "expected the OS to deny the connection outright (ACCESS_DENIED), got: {err:?}"
            );
        }
        other => panic!(
            "expected the ACL to deny this connection at the OS level with a PermissionDenied \
             I/O error, got {other:?}"
        ),
    }

    // The server's `accept()` never completes because no client ever
    // successfully opened a handle to hand off -- confirm it is still
    // sitting there waiting (timed out), not that it crashed.
    let accept_outcome = server_task.await.expect("server task must not panic");
    assert!(
        accept_outcome.is_err(),
        "server accept() should still be waiting (timed out) since the ACL denied the only \
         connection attempt: {accept_outcome:?}"
    );
}
