//! Integration tests for the composed background host (`RUN-003`):
//! `src/host/compose.rs` and `src/host/discovery.rs` exercised together
//! against real OS resources — a real named pipe, a real `DAT-001` SQLite
//! file, and a real filesystem discovery file. Complements each module's own
//! colocated unit tests (`src/host/*.rs`, `crate::host::supervisor`'s tests
//! in particular), which exercise decision logic in isolation, and
//! `tests/process_behavior.rs`, which black-box-tests the compiled binary.
//!
//! Every test here uses a unique per-test pipe name and a temporary
//! directory for `HostPaths`, never the well-known per-user production pipe
//! or the real `%LOCALAPPDATA%` — so these tests never collide with each
//! other, with a real running LokiAI host, or with
//! `tests/process_behavior.rs`'s black-box tests of the real binary.

#![cfg(windows)]

use std::time::Duration;

use loki_ipc::auth::AuthToken;
use loki_ipc::{Client, ClientConfig};
use loki_runtime::host::compose::{ComposedHost, HostConfig};
use loki_runtime::host::discovery::{self, HostStatus, RecordedState};
use loki_runtime::host::paths::HostPaths;
use loki_runtime::host::token;
use loki_storage::devices::{DeviceRepository, NewDevice};
use loki_storage::ids::{ConnectorId, DeviceId, RuntimeId};
use loki_storage::states::{DeviceLifecycleState, SupportTier};

/// A bound for assertions that must not hang the suite if the implementation
/// regresses into a real deadlock — not a normal-operation timing
/// assumption.
const HANG_GUARD: Duration = Duration::from_secs(10);

fn unique_pipe_name(label: &str) -> String {
    format!(
        r"\\.\pipe\loki-runtime-host-test-{label}-{}",
        uuid::Uuid::new_v4()
    )
}

fn test_config(dir: &std::path::Path, label: &str) -> HostConfig {
    HostConfig {
        paths: HostPaths::new(dir),
        pipe_name: unique_pipe_name(label),
    }
}

fn sample_device(device_id: &str) -> NewDevice {
    NewDevice {
        device_id: DeviceId::new(device_id).unwrap(),
        connector_id: ConnectorId::new("connector-test").unwrap(),
        connector_version: "0.0.0".to_string(),
        hardware_identity: "hw-1".to_string(),
        family: "esp32-s3".to_string(),
        board_identity: None,
        transport_reference: None,
        runtime_id: RuntimeId::new("runtime-test").unwrap(),
        support_tier: SupportTier::Custom,
        lifecycle_state: DeviceLifecycleState::Discovered,
        capabilities_json: None,
        firmware_artifact_ref: None,
        config_artifact_ref: None,
        model_artifact_ref: None,
        health_json: None,
        last_seen_at: "2026-01-01T00:00:00Z".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

/// Deliverable 1 / 5: the composed host binds a real IPC listener alongside
/// its storage/discovery bookkeeping, and a real client can connect and
/// complete a real `ping` round trip against it — and, using the very same
/// live host, `discovery::locate` independently confirms
/// [`HostStatus::Healthy`] (never from the discovery file's claim alone).
#[tokio::test]
async fn startup_accepts_a_real_ipc_connection_and_is_located_as_healthy() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path(), "startup");
    let pipe_name = config.pipe_name.clone();
    let paths = config.paths.clone();

    let host = ComposedHost::open(&config).expect("host must start");

    let run_task = tokio::spawn(async move {
        host.run(
            async {
                tokio::time::sleep(Duration::from_millis(600)).await;
                loki_runtime::shutdown::ShutdownReason::CtrlC
            },
            std::future::pending::<()>(),
        )
        .await
    });

    let raw_token = token::load_or_create(&paths.token_file()).expect("token must be readable");
    let client_config = ClientConfig::new(
        pipe_name.clone(),
        AuthToken::new(raw_token),
        "host-lifecycle-test",
    );

    let client = tokio::time::timeout(HANG_GUARD, Client::connect(client_config))
        .await
        .expect("connect must not hang")
        .expect("connect should succeed against the real running host");

    let pong = tokio::time::timeout(
        HANG_GUARD,
        client.ping("host-lifecycle-test", Duration::from_secs(3)),
    )
    .await
    .expect("ping must not hang")
    .expect("ping should succeed");
    assert_eq!(pong.payload, "host-lifecycle-test");

    let raw_token_for_discovery = token::load_or_create(&paths.token_file()).unwrap();
    let discovery_status = tokio::time::timeout(
        HANG_GUARD,
        discovery::locate(
            &paths,
            &pipe_name,
            &AuthToken::new(raw_token_for_discovery),
            Duration::from_secs(2),
            Duration::from_secs(2),
        ),
    )
    .await
    .expect("locate must not hang against a real running host");
    assert_eq!(discovery_status, HostStatus::Healthy);

    let outcome = tokio::time::timeout(HANG_GUARD, run_task)
        .await
        .expect("host run task must not hang")
        .expect("host run task must not panic");
    assert!(matches!(
        outcome,
        loki_runtime::runtime::RunOutcome::Shutdown(_)
    ));

    // A clean shutdown must mark the discovery file "stopped" so a
    // subsequent `locate` call reports `NotRunning` without needing another
    // live connect attempt.
    let after_shutdown = tokio::time::timeout(
        HANG_GUARD,
        discovery::locate(
            &paths,
            &pipe_name,
            &AuthToken::new("irrelevant-after-clean-shutdown-long-enough"),
            Duration::from_millis(200),
            Duration::from_millis(200),
        ),
    )
    .await
    .expect("locate must not hang after shutdown");
    assert_eq!(after_shutdown, HostStatus::NotRunning);
}

/// Deliverable 2: simulates a crash (the host process disappears without a
/// clean shutdown — modeled here by dropping [`ComposedHost`] directly,
/// never calling `.run()`'s graceful-stop path) and confirms `DAT-001` state
/// written before the crash is intact after the host is reopened against the
/// same on-disk paths, exactly as the real supervisor
/// (`src/host/supervisor.rs`) restarting a crashed child would observe.
#[tokio::test]
async fn simulated_crash_and_restart_preserves_persisted_device_state() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path(), "crash-restart");

    let device = sample_device("dev-crash-restart-1");
    {
        let host = ComposedHost::open(&config).expect("host must start");
        host.storage()
            .insert_device(&device)
            .expect("device insert must succeed before the simulated crash");
        // No `.run()` call, no graceful shutdown marker — `host` is simply
        // dropped here, modeling an unclean process death. The discovery
        // file is left claiming `"serving"`, exactly as a real crash would
        // leave it.
    }

    // A fresh pipe name for the restarted instance -- a real restart binds a
    // brand new `Server`, it does not reuse the crashed process's OS handle.
    let restarted_config = HostConfig {
        paths: config.paths.clone(),
        pipe_name: unique_pipe_name("crash-restart-restarted"),
    };
    let restarted_host =
        ComposedHost::open(&restarted_config).expect("host must restart against the same paths");

    let restarted_device = restarted_host
        .storage()
        .get_device(&device.device_id)
        .expect("device lookup must succeed after restart")
        .expect("the device inserted before the simulated crash must still be present");

    assert_eq!(restarted_device.device_id, device.device_id);
    assert_eq!(restarted_device.hardware_identity, device.hardware_identity);
    assert_eq!(restarted_device.lifecycle_state, device.lifecycle_state);
}

/// Deliverable 5's negative case, exercised end to end through `HostPaths`
/// exactly as a desktop client would use them: no host was ever started at
/// these paths, so discovery must report `NotRunning` promptly — not a hang,
/// not a false-positive `Healthy`.
#[tokio::test]
async fn discovery_reports_not_running_for_a_host_that_was_never_started() {
    let dir = tempfile::tempdir().unwrap();
    let paths = HostPaths::new(dir.path());
    let never_bound_pipe = unique_pipe_name("never-started");

    let status = tokio::time::timeout(
        HANG_GUARD,
        discovery::locate(
            &paths,
            &never_bound_pipe,
            &AuthToken::new("irrelevant-token-long-enough-to-be-plausible"),
            Duration::from_millis(300),
            Duration::from_millis(300),
        ),
    )
    .await
    .expect("locate must not hang when no host was ever started");
    assert_eq!(status, HostStatus::NotRunning);
}

/// A crashed host (unclean death, discovery file left claiming `"serving"`)
/// must be reported as `Unreachable`, never silently `Healthy` and never a
/// hang — `AGENTS.md` rule 1.
#[tokio::test]
async fn a_crashed_host_with_a_stale_serving_file_is_reported_unreachable_not_healthy() {
    let dir = tempfile::tempdir().unwrap();
    let paths = HostPaths::new(dir.path());
    let pipe_name = unique_pipe_name("crashed");

    discovery::write(
        &paths.discovery_file(),
        RecordedState::Serving,
        &pipe_name,
        0,
    )
    .expect("writing the discovery file must succeed");

    let status = tokio::time::timeout(
        HANG_GUARD,
        discovery::locate(
            &paths,
            &pipe_name,
            &AuthToken::new("irrelevant-token-long-enough-to-be-plausible"),
            Duration::from_millis(300),
            Duration::from_millis(300),
        ),
    )
    .await
    .expect("locate must not hang against a crashed host's stale pipe");
    assert!(matches!(status, HostStatus::Unreachable { .. }));
}
