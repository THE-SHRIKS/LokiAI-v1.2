//! Shared helpers for loki-ipc's black-box integration tests: each test
//! spins up a real `Server` and a real `Client` over a real, uniquely-named
//! Windows named pipe (never a shared/fixed name — parallel `cargo test`
//! execution must not collide).

#![allow(dead_code)] // not every helper is used by every test file

pub mod business;

use std::sync::Arc;
use std::time::Duration;

use loki_ipc::auth::AuthToken;
use loki_ipc::client::ClientConfig;
use loki_ipc::server::ServerConfig;

/// A fresh, collision-resistant pipe name for one test.
pub fn unique_pipe_name(label: &str) -> String {
    format!(r"\\.\pipe\loki-ipc-test-{label}-{}", uuid::Uuid::new_v4())
}

/// The shared secret used by tests that do not specifically exercise wrong-
/// token rejection.
pub fn shared_test_token() -> AuthToken {
    AuthToken::new("integration-test-shared-secret")
}

pub fn server_config(pipe_name: &str) -> ServerConfig {
    ServerConfig::new(pipe_name.to_string(), shared_test_token())
}

pub fn client_config(pipe_name: &str) -> ClientConfig {
    ClientConfig::new(
        pipe_name.to_string(),
        shared_test_token(),
        "loki-ipc-test-client",
    )
}

/// A server config wired to [`business::TestBusinessService`] and the
/// standard business credential set, over a real seeded `Storage`. Returns
/// the config, the `SharedCredentialSet` handle (for post-handshake revoke
/// tests), and the same `Arc<Storage>` the service uses (so a test can seed
/// a real, granted `local_approvals` row before exercising the accepted
/// approval-gated path).
pub fn business_server_config(
    pipe_name: &str,
) -> (
    ServerConfig,
    business::SharedCredentialSet,
    Arc<loki_storage::Storage>,
) {
    let storage = business::seeded_storage();
    let resolver = business::business_credential_set();
    let config = ServerConfig::new(pipe_name.to_string(), shared_test_token())
        .with_credential_resolver(Arc::new(resolver.clone()))
        .with_business_service(Arc::new(business::TestBusinessService::new(Arc::clone(
            &storage,
        ))));
    (config, resolver, storage)
}

/// Like [`business_server_config`], but wires
/// [`business::TestBusinessService::new_with_live_stream_gate`] instead —
/// used only by the mid-subscription-revocation coverage test, which needs
/// a real, live `BusinessEvents` producer to revoke a credential against
/// mid-delivery rather than only around a unary call. Returns the config,
/// the `SharedCredentialSet` handle, and the producer's gate (notify it to
/// release the second of the two real events the producer emits).
pub fn business_server_config_with_live_stream(
    pipe_name: &str,
) -> (
    ServerConfig,
    business::SharedCredentialSet,
    Arc<tokio::sync::Notify>,
) {
    let storage = business::seeded_storage();
    let resolver = business::business_credential_set();
    let (service, gate) = business::TestBusinessService::new_with_live_stream_gate(storage);
    let config = ServerConfig::new(pipe_name.to_string(), shared_test_token())
        .with_credential_resolver(Arc::new(resolver.clone()))
        .with_business_service(Arc::new(service));
    (config, resolver, gate)
}

/// A client config that authenticates with `token` instead of
/// [`shared_test_token`] — used to select which fixture principal
/// (`business::business_credential_set`) a business test connects as.
pub fn client_config_with_token(pipe_name: &str, token: &str) -> ClientConfig {
    ClientConfig::new(
        pipe_name.to_string(),
        AuthToken::new(token),
        "loki-ipc-test-client",
    )
}

/// A generous bound for assertions that must not hang the suite if the
/// implementation regresses into a real deadlock — not a normal-operation
/// timing assumption.
pub const TEST_HANG_GUARD: Duration = Duration::from_secs(10);

/// `ERROR_FILE_NOT_FOUND` / `ERROR_PIPE_BUSY`: the only transient connect
/// failures worth a short retry in test helpers below (the server side is
/// always bound synchronously before these helpers are called in this
/// crate's tests, so in practice this loop runs once, but a retry keeps the
/// helper honest under real OS scheduling jitter rather than assuming zero
/// delay).
const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_PIPE_BUSY: i32 = 231;

/// Opens a raw, unauthenticated-at-the-frame-level connection to `pipe_name`
/// — used only by tests that deliberately bypass [`loki_ipc::client::Client`]
/// to inject malformed bytes or a non-`Hello` first message.
pub async fn raw_connect(pipe_name: &str) -> tokio::net::windows::named_pipe::NamedPipeClient {
    let options = tokio::net::windows::named_pipe::ClientOptions::new();
    let deadline = tokio::time::Instant::now() + TEST_HANG_GUARD;
    loop {
        match options.open(pipe_name) {
            Ok(client) => return client,
            Err(err)
                if matches!(
                    err.raw_os_error(),
                    Some(ERROR_FILE_NOT_FOUND) | Some(ERROR_PIPE_BUSY)
                ) && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(err) => panic!("raw_connect to {pipe_name} failed: {err}"),
        }
    }
}

/// Connects raw and drives the wire-level handshake by hand (using
/// `loki_ipc`'s own frame codec and wire types, but not its `Client`), so
/// the caller retains the raw split halves afterward to send hand-crafted
/// bytes.
pub async fn raw_handshake(
    pipe_name: &str,
) -> (
    tokio::io::WriteHalf<tokio::net::windows::named_pipe::NamedPipeClient>,
    tokio::io::ReadHalf<tokio::net::windows::named_pipe::NamedPipeClient>,
) {
    use loki_ipc::frame::{DEFAULT_MAX_FRAME_BYTES, read_frame, write_frame};
    use loki_ipc::protocol::{PROTOCOL_VERSION, WireMessage, baseline_capabilities};

    let pipe = raw_connect(pipe_name).await;
    let (mut reader, mut writer) = tokio::io::split(pipe);

    let challenge = read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
        .await
        .expect("read challenge frame")
        .expect("challenge frame present, not a clean close");
    let nonce = match challenge {
        WireMessage::Challenge { nonce, .. } => nonce,
        other => panic!("expected Challenge as the first server message, got {other:?}"),
    };

    let hello = WireMessage::Hello {
        client_protocol_version: PROTOCOL_VERSION,
        client_capabilities: baseline_capabilities(),
        client_type: "raw-test-client".to_string(),
        client_version: "0.0.0".to_string(),
        nonce,
        token: shared_test_token().as_str().to_string(),
    };
    write_frame(&mut writer, &hello).await.expect("write hello");

    let ack = read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
        .await
        .expect("read hello-ack frame")
        .expect("hello-ack frame present, not a clean close");
    match ack {
        WireMessage::HelloAck { .. } => {}
        other => panic!("expected HelloAck, got {other:?}"),
    }

    (writer, reader)
}
