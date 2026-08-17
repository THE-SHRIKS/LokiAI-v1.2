//! End-to-end malformed-input tests over a real named pipe: every case
//! rejects cleanly (closed connection, typed error) rather than panicking
//! or hanging, and — critically — a malformed connection never takes down
//! the server's ability to serve a subsequent, well-formed client.
//!
//! `Docs/graph/tasks/RUN-002.md` Deliverable 6: "malformed frame handling
//! (rejected, not a panic)".

mod support;

use std::time::Duration;

use tokio::io::AsyncWriteExt;

use loki_ipc::client::Client;
use loki_ipc::protocol::{HandshakeRejectReason, WireMessage, baseline_capabilities};
use loki_ipc::server::{ConnectionError, Server};

/// Connects raw, completes the handshake, and returns the split halves plus
/// a background task that runs `Connection::handle()`'s dispatch loop for
/// it — used by tests that need to send malformed bytes into an
/// *already-handshaken* connection specifically (as opposed to
/// `tests/support::raw_handshake`, which only proves the handshake
/// completed).
async fn serve_one_and_handshake(
    pipe_name: &str,
) -> (
    tokio::task::JoinHandle<Result<(), ConnectionError>>,
    tokio::io::WriteHalf<tokio::net::windows::named_pipe::NamedPipeClient>,
    tokio::io::ReadHalf<tokio::net::windows::named_pipe::NamedPipeClient>,
) {
    let mut server = Server::bind(support::server_config(pipe_name)).expect("bind server");
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let (writer, reader) = support::raw_handshake(pipe_name).await;
    (server_task, writer, reader)
}

#[tokio::test]
async fn oversized_length_prefix_after_handshake_closes_the_connection_not_a_panic() {
    let pipe_name = support::unique_pipe_name("malformed-oversized");
    let (server_task, mut writer, _reader) = serve_one_and_handshake(&pipe_name).await;

    // Claims a payload far past the configured maximum; never sends it.
    writer
        .write_all(&(1024u32 * 1024 * 1024).to_le_bytes())
        .await
        .expect("write oversized length prefix");
    writer.flush().await.expect("flush");

    let outcome = tokio::time::timeout(support::TEST_HANG_GUARD, server_task)
        .await
        .expect("server must reject an oversized frame promptly, not hang")
        .expect("server task must not panic");

    assert!(
        matches!(outcome, Err(ConnectionError::Frame(_))),
        "expected a Frame error, got {outcome:?}"
    );
}

#[tokio::test]
async fn invalid_json_payload_after_handshake_closes_the_connection_not_a_panic() {
    let pipe_name = support::unique_pipe_name("malformed-invalid-json");
    let (server_task, mut writer, _reader) = serve_one_and_handshake(&pipe_name).await;

    let payload = b"this is not json";
    writer
        .write_all(&(payload.len() as u32).to_le_bytes())
        .await
        .expect("write length prefix");
    writer.write_all(payload).await.expect("write payload");
    writer.flush().await.expect("flush");

    let outcome = tokio::time::timeout(support::TEST_HANG_GUARD, server_task)
        .await
        .expect("server must reject invalid JSON promptly, not hang")
        .expect("server task must not panic");

    assert!(
        matches!(outcome, Err(ConnectionError::Frame(_))),
        "expected a Frame error, got {outcome:?}"
    );
}

#[tokio::test]
async fn truncated_frame_after_handshake_closes_the_connection_not_a_panic() {
    let pipe_name = support::unique_pipe_name("malformed-truncated");
    let (server_task, mut writer, _reader) = serve_one_and_handshake(&pipe_name).await;

    // Announces 100 bytes, sends 3, then drops the connection.
    writer
        .write_all(&100u32.to_le_bytes())
        .await
        .expect("write length prefix");
    writer
        .write_all(b"abc")
        .await
        .expect("write partial payload");
    // Both halves of the client's `tokio::io::split` pair must be dropped —
    // the underlying pipe handle stays open, and the server never sees EOF,
    // as long as either half is still alive (see `Client`'s own `Drop` impl
    // doc comment in src/client.rs for the same rule on the client side).
    drop(writer);
    drop(_reader);

    let outcome = tokio::time::timeout(support::TEST_HANG_GUARD, server_task)
        .await
        .expect("server must notice the truncated frame promptly, not hang")
        .expect("server task must not panic");

    assert!(
        matches!(outcome, Err(ConnectionError::Frame(_))),
        "expected a Frame error, got {outcome:?}"
    );
}

#[tokio::test]
async fn a_non_call_message_sent_as_the_first_client_message_is_rejected_as_malformed_hello() {
    let pipe_name = support::unique_pipe_name("malformed-not-a-hello");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let pipe = support::raw_connect(&pipe_name).await;
    let (mut reader, mut writer) = tokio::io::split(pipe);

    // Consume the Challenge, then send something that is a well-formed
    // frame but not a `Hello` -- exactly a client that skips the handshake.
    let _ = loki_ipc::frame::read_frame(&mut reader, loki_ipc::frame::DEFAULT_MAX_FRAME_BYTES)
        .await
        .expect("read challenge")
        .expect("challenge present");
    loki_ipc::frame::write_frame(
        &mut writer,
        &WireMessage::CancelCall {
            correlation_id: uuid::Uuid::nil(),
        },
    )
    .await
    .expect("write a non-Hello message");

    let reject = loki_ipc::frame::read_frame(&mut reader, loki_ipc::frame::DEFAULT_MAX_FRAME_BYTES)
        .await
        .expect("read hello-reject")
        .expect("hello-reject present");
    match reject {
        WireMessage::HelloReject {
            reason: HandshakeRejectReason::MalformedHello { message },
            ..
        } => {
            assert!(message.to_lowercase().contains("hello"));
        }
        other => panic!("expected HelloReject(MalformedHello), got {other:?}"),
    }

    let outcome = tokio::time::timeout(support::TEST_HANG_GUARD, server_task)
        .await
        .expect("server must not hang")
        .expect("server task must not panic");
    assert!(matches!(
        outcome,
        Err(ConnectionError::HandshakeRejected(
            HandshakeRejectReason::MalformedHello { .. }
        ))
    ));
}

#[tokio::test]
async fn a_malformed_connection_does_not_stop_the_server_from_serving_the_next_client() {
    let pipe_name = support::unique_pipe_name("malformed-then-good-client");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");

    let server_task = tokio::spawn(async move {
        let mut outcomes = Vec::new();
        for _ in 0..2u8 {
            let connection = server.accept().await.expect("accept");
            outcomes.push(connection.handle().await);
        }
        outcomes
    });

    // First client: connects, then immediately sends garbage bytes with no
    // handshake at all.
    let bad_pipe = support::raw_connect(&pipe_name).await;
    let (_bad_reader, mut bad_writer) = tokio::io::split(bad_pipe);
    bad_writer
        .write_all(b"\xff\xff\xff\xffnotalengthprefixatall")
        .await
        .expect("write garbage");
    bad_writer.flush().await.expect("flush");
    drop(bad_writer);

    // Give the server a bounded moment to notice and move on to preparing
    // its next instance before the second (well-behaved) client connects.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Second client: a completely normal, well-behaved handshake.
    let good_client = tokio::time::timeout(
        support::TEST_HANG_GUARD,
        Client::connect(support::client_config(&pipe_name)),
    )
    .await
    .expect("connect must not hang")
    .expect("a well-behaved second client must still be able to connect and handshake");
    assert!(
        good_client
            .negotiated_capabilities()
            .contains(loki_ipc::protocol::CORE_CAPABILITY)
    );
    drop(good_client);

    let outcomes = tokio::time::timeout(support::TEST_HANG_GUARD, server_task)
        .await
        .expect("server must not hang")
        .expect("server task must not panic");
    assert_eq!(outcomes.len(), 2);
    assert!(
        outcomes[0].is_err(),
        "the first (bad) connection must be reported as an error"
    );
    assert!(
        outcomes[1].is_ok(),
        "the second (good) connection must succeed: {:?}",
        outcomes[1]
    );
}

#[tokio::test]
async fn hello_with_capabilities_declared_out_of_order_still_round_trips() {
    // Not a malformed case -- a sanity check that capability *sets* really
    // are treated as sets (order-independent) at the wire level, so a
    // false "malformed" rejection due to accidental order-sensitivity
    // would be caught here.
    let pipe_name = support::unique_pipe_name("capabilities-are-a-set");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let mut config = support::client_config(&pipe_name);
    let mut caps = baseline_capabilities();
    caps.insert("zzz.last.alphabetically".to_string());
    caps.insert("aaa.first.alphabetically".to_string());
    config.capabilities = caps;

    let client = Client::connect(config)
        .await
        .expect("handshake must succeed");
    drop(client);

    tokio::time::timeout(support::TEST_HANG_GUARD, server_task)
        .await
        .expect("server must not hang")
        .expect("server task must not panic")
        .expect("connection must end cleanly");
}
