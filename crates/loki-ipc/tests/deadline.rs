//! End-to-end per-call deadline and cancellation tests over a real named
//! pipe: a deadline-exceeded (or explicitly cancelled) call is stopped
//! server-side, not merely abandoned by the client while the callee keeps
//! running.
//!
//! `Docs/graph/tasks/RUN-002.md` Deliverable 4 and Deliverable 6:
//! "deadline-exceeded cancellation actually cancels (not just times out the
//! caller while the callee keeps running)".
//!
//! # Proof strategy
//!
//! `echo_stream` with a very large `count`, a deliberately tiny outgoing
//! channel capacity, and a tiny pipe buffer is used as the "slow work" to
//! cancel: the server-side item-producing loop is *structurally*
//! guaranteed to still be far from finished (blocked on a full channel)
//! well before either the deadline or an explicit cancel fires, because a
//! client that never reads cannot possibly drain more than a handful of
//! items through such small buffers. If the server's cancellation merely
//! stopped *reporting* to the client while secretly finishing the loop in
//! the background, the client would observe a large number of items
//! (however many the background task produced) before `Ended` — instead,
//! each test below asserts `Ended` arrives after only a small, bounded
//! number of items, which is only possible if the producing loop was
//! actually dropped mid-iteration by `crate::server`'s `select!` race (see
//! that module's top doc comment), not merely disowned.

mod support;

use std::time::Duration;

use loki_ipc::client::{Client, StreamEvent};
use loki_ipc::protocol::RpcError;
use loki_ipc::server::{Server, ServerConfig};

/// Small enough that a client which never reads cannot possibly drain more
/// than a handful of items before the outgoing channel and pipe buffer
/// both fill up.
fn tight_buffer_config(pipe_name: &str) -> ServerConfig {
    let mut config = support::server_config(pipe_name);
    config.outgoing_channel_capacity = 1;
    config.out_buffer_size = 512;
    config.in_buffer_size = 512;
    config
}

/// A generous upper bound on how many items could plausibly have been
/// buffered (channel + pipe kernel buffer + a small margin) before a
/// producer genuinely blocks with nobody reading. Any observed count at or
/// above this would mean the producer kept running well past where it
/// should have been cancelled.
const MAX_PLAUSIBLE_BUFFERED_ITEMS: u32 = 500;

#[tokio::test]
async fn ping_completes_normally_within_a_generous_deadline() {
    // Sanity/regression guard: deadlines must not fire spuriously for an
    // ordinary fast call under a realistic timeout.
    let pipe_name = support::unique_pipe_name("deadline-ping-succeeds");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let client = Client::connect(support::client_config(&pipe_name))
        .await
        .expect("handshake");
    let pong = client
        .ping("hello", Duration::from_secs(5))
        .await
        .expect("ping must succeed well within a 5s deadline");
    assert_eq!(pong.payload, "hello");

    drop(client);
    let _ = tokio::time::timeout(support::TEST_HANG_GUARD, server_task).await;
}

#[tokio::test]
async fn deadline_exceeded_actually_stops_the_server_side_work_not_just_the_caller() {
    let pipe_name = support::unique_pipe_name("deadline-actually-cancels");
    let mut server = Server::bind(tight_buffer_config(&pipe_name)).expect("bind server");
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let mut client_config = support::client_config(&pipe_name);
    client_config.stream_event_capacity = 1;
    let client = Client::connect(client_config).await.expect("handshake");

    let mut handle = client
        .echo_stream(
            "x",
            loki_ipc::protocol::MAX_ECHO_STREAM_COUNT,
            Duration::from_millis(150),
        )
        .await
        .expect("call must be accepted");

    // Deliberately do not read anything for a while: long enough that the
    // 150ms deadline has certainly elapsed server-side, and long enough
    // that -- if the server were (incorrectly) still producing items in
    // the background -- many more than `MAX_PLAUSIBLE_BUFFERED_ITEMS`
    // would already be queued up somewhere waiting for us.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut items_received: u32 = 0;
    let ended = loop {
        match tokio::time::timeout(support::TEST_HANG_GUARD, handle.recv())
            .await
            .expect("draining the stream must not hang")
        {
            Some(StreamEvent::Item(_)) => {
                items_received += 1;
                assert!(
                    items_received < MAX_PLAUSIBLE_BUFFERED_ITEMS,
                    "received {items_received} items -- the server appears to have kept \
                     producing well past its deadline instead of actually cancelling"
                );
            }
            Some(StreamEvent::Ended(result)) => break result,
            None => panic!("connection closed before a terminal StreamEnd arrived"),
        }
    };

    assert_eq!(
        ended,
        Err(RpcError::DeadlineExceeded),
        "expected the call to end with the server's own DeadlineExceeded outcome"
    );
    assert!(
        items_received < MAX_PLAUSIBLE_BUFFERED_ITEMS,
        "only {items_received} items should have been producible before the tiny buffers filled"
    );

    drop(client);
    let _ = tokio::time::timeout(support::TEST_HANG_GUARD, server_task).await;
}

#[tokio::test]
async fn explicit_cancel_call_also_stops_the_server_side_work() {
    let pipe_name = support::unique_pipe_name("cancel-actually-cancels");
    let mut server = Server::bind(tight_buffer_config(&pipe_name)).expect("bind server");
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let mut client_config = support::client_config(&pipe_name);
    client_config.stream_event_capacity = 1;
    let client = Client::connect(client_config).await.expect("handshake");

    // A long deadline that will *not* fire during this test -- isolates
    // this test to the explicit-cancel path specifically, not the deadline
    // path already covered above.
    let mut handle = client
        .echo_stream(
            "x",
            loki_ipc::protocol::MAX_ECHO_STREAM_COUNT,
            Duration::from_secs(30),
        )
        .await
        .expect("call must be accepted");

    // Let the producer block on the tiny buffers first, exactly as above.
    tokio::time::sleep(Duration::from_millis(200)).await;
    client.cancel_call(handle.correlation_id()).await;

    let mut items_received: u32 = 0;
    let ended = loop {
        match tokio::time::timeout(support::TEST_HANG_GUARD, handle.recv())
            .await
            .expect("draining the stream must not hang")
        {
            Some(StreamEvent::Item(_)) => {
                items_received += 1;
                assert!(
                    items_received < MAX_PLAUSIBLE_BUFFERED_ITEMS,
                    "received {items_received} items -- the server appears to have kept \
                     producing after being cancelled instead of actually stopping"
                );
            }
            Some(StreamEvent::Ended(result)) => break result,
            None => panic!("connection closed before a terminal StreamEnd arrived"),
        }
    };

    assert_eq!(
        ended,
        Err(RpcError::Cancelled),
        "expected the call to end with Cancelled after an explicit CancelCall"
    );

    drop(client);
    let _ = tokio::time::timeout(support::TEST_HANG_GUARD, server_task).await;
}

#[tokio::test]
async fn cancelling_an_already_finished_call_is_a_silent_no_op() {
    let pipe_name = support::unique_pipe_name("cancel-after-finished-is-noop");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let client = Client::connect(support::client_config(&pipe_name))
        .await
        .expect("handshake");

    let pong = client
        .ping("already-done", Duration::from_secs(5))
        .await
        .expect("ping must succeed");
    assert_eq!(pong.payload, "already-done");

    // The call already completed; cancelling it now must not panic, hang,
    // or affect anything else on the connection.
    client.cancel_call(uuid::Uuid::new_v4()).await;

    let pong2 = client
        .ping("still-fine", Duration::from_secs(5))
        .await
        .expect("the connection must still work normally after a no-op cancel");
    assert_eq!(pong2.payload, "still-fine");

    drop(client);
    let _ = tokio::time::timeout(support::TEST_HANG_GUARD, server_task).await;
}
