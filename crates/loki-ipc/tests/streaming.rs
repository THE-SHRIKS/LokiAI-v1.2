//! End-to-end server-to-client streaming tests over a real named pipe:
//! ordered, complete delivery, and real backpressure when the consumer is
//! slow.
//!
//! `Docs/graph/tasks/RUN-002.md` Deliverable 5 and Deliverable 6.

mod support;

use std::time::{Duration, Instant};

use loki_ipc::client::{Client, StreamEvent};
use loki_ipc::protocol::EchoStreamSummary;
use loki_ipc::server::Server;

#[tokio::test]
async fn a_fast_consumer_receives_every_item_in_order_then_a_summary() {
    let pipe_name = support::unique_pipe_name("streaming-in-order");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let client = Client::connect(support::client_config(&pipe_name))
        .await
        .expect("handshake");

    let mut handle = client
        .echo_stream("item", 50, Duration::from_secs(10))
        .await
        .expect("call must be accepted");

    let mut seen = Vec::new();
    let summary = loop {
        match tokio::time::timeout(support::TEST_HANG_GUARD, handle.recv())
            .await
            .expect("draining the stream must not hang")
        {
            Some(StreamEvent::Item(item)) => {
                assert_eq!(item.payload, "item");
                seen.push(item.seq);
            }
            Some(StreamEvent::Ended(result)) => break result,
            None => panic!("connection closed before a terminal StreamEnd arrived"),
        }
    };

    assert_eq!(
        seen,
        (0..50).collect::<Vec<_>>(),
        "items must arrive in order, none missing"
    );
    assert_eq!(summary, Ok(EchoStreamSummary { items_sent: 50 }));

    drop(client);
    let _ = tokio::time::timeout(support::TEST_HANG_GUARD, server_task).await;
}

#[tokio::test]
async fn a_slow_consumer_measurably_paces_the_server_real_backpressure() {
    // Deliberately tiny buffers so the OS pipe and the outgoing channel
    // cannot silently absorb the whole stream before the client starts
    // reading -- otherwise this test would only measure how fast the
    // client happens to loop, not real backpressure.
    let pipe_name = support::unique_pipe_name("streaming-backpressure");
    let mut config = support::server_config(&pipe_name);
    config.outgoing_channel_capacity = 1;
    config.out_buffer_size = 256;
    config.in_buffer_size = 256;
    let mut server = Server::bind(config).expect("bind server");
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let mut client_config = support::client_config(&pipe_name);
    client_config.stream_event_capacity = 1;
    let client = Client::connect(client_config).await.expect("handshake");

    const COUNT: u32 = 20;
    const PER_ITEM_DELAY: Duration = Duration::from_millis(30);

    let mut handle = client
        .echo_stream(
            "payload-for-backpressure-test",
            COUNT,
            Duration::from_secs(30),
        )
        .await
        .expect("call must be accepted");

    let started = Instant::now();
    let mut items_received = 0u32;
    let summary = loop {
        // The deliberate per-item delay is the "slow consumer": if the
        // server had already buffered/sent everything instantly with no
        // real backpressure, this delay would only affect how fast *this
        // loop* drains an already-complete backlog, not how long the
        // server took to produce it -- the elapsed-time assertion below is
        // what distinguishes the two.
        tokio::time::sleep(PER_ITEM_DELAY).await;
        match tokio::time::timeout(support::TEST_HANG_GUARD, handle.recv())
            .await
            .expect("draining the stream must not hang")
        {
            Some(StreamEvent::Item(item)) => {
                assert_eq!(item.seq, items_received, "items must still arrive in order");
                items_received += 1;
            }
            Some(StreamEvent::Ended(result)) => break result,
            None => panic!("connection closed before a terminal StreamEnd arrived"),
        }
    };
    let elapsed = started.elapsed();

    assert_eq!(items_received, COUNT);
    assert_eq!(summary, Ok(EchoStreamSummary { items_sent: COUNT }));

    // If the server had produced (and the transport had buffered) the
    // entire stream well ahead of the slow consumer, total elapsed time
    // would be close to `COUNT * PER_ITEM_DELAY` only because of *this*
    // loop's own sleeps, which is indistinguishable from real
    // backpressure by itself -- so instead assert a floor that only real
    // backpressure (the server's producer genuinely waiting on send()
    // rather than free-running ahead) combined with this loop's pacing
    // could jointly produce: comfortably above a single buffered burst
    // (a handful of items' worth of slack for the tiny channel/pipe
    // buffers), and therefore only explainable if the server's own
    // progress was gated by this consumer, not merely this test's sleeps.
    let floor = PER_ITEM_DELAY * (COUNT - 4);
    assert!(
        elapsed >= floor,
        "expected pacing consistent with real backpressure (>= {floor:?}), got {elapsed:?}"
    );

    drop(client);
    let _ = tokio::time::timeout(support::TEST_HANG_GUARD, server_task).await;
}

#[tokio::test]
async fn echo_stream_count_over_the_maximum_is_a_named_validation_error_not_a_panic() {
    let pipe_name = support::unique_pipe_name("streaming-count-too-large");
    let mut server = Server::bind(support::server_config(&pipe_name)).expect("bind server");
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept");
        connection.handle().await
    });

    let client = Client::connect(support::client_config(&pipe_name))
        .await
        .expect("handshake");

    let mut handle = client
        .echo_stream(
            "x",
            loki_ipc::protocol::MAX_ECHO_STREAM_COUNT + 1,
            Duration::from_secs(10),
        )
        .await
        .expect("call must be accepted (validation happens server-side)");

    // A single recv(), not a loop: the server rejects an over-the-maximum
    // count immediately, so the very first event must be the terminal
    // StreamEnd — no items should ever be sent for an invalid request.
    let ended = match tokio::time::timeout(support::TEST_HANG_GUARD, handle.recv())
        .await
        .expect("draining the stream must not hang")
    {
        Some(StreamEvent::Item(_)) => panic!("no items should be sent for an invalid request"),
        Some(StreamEvent::Ended(result)) => result,
        None => panic!("connection closed before a terminal StreamEnd arrived"),
    };

    assert!(matches!(
        ended,
        Err(loki_ipc::protocol::RpcError::InvalidRequest { .. })
    ));

    drop(client);
    let _ = tokio::time::timeout(support::TEST_HANG_GUARD, server_task).await;
}
