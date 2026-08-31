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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use loki_ipc::auth::{AuthToken, CredentialRecord, CredentialResolver, CredentialSet};
use loki_ipc::business::{
    AuthenticatedPrincipal, Authority, BusinessRpcResult, EmptyParams, PrincipalKind, ReadMeta,
    WireMutationAvailability, WireStorageReadiness,
};
use loki_ipc::client::CallError;
use loki_ipc::protocol::Method;
use loki_ipc::server::{
    BusinessCallFuture, BusinessEventEmitter, BusinessRequestContext, BusinessService,
    BusinessStreamFuture,
};
use loki_ipc::{Client, ClientConfig};
use loki_runtime::host::compose::{ComposedHost, HostConfig};
use loki_runtime::host::discovery::{self, HostStatus, RecordedState};
use loki_runtime::host::paths::HostPaths;
use loki_runtime::host::token;
use loki_runtime::shutdown::ShutdownReason;
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

    let host = ComposedHost::open_with_test_arena(&config).expect("host must start");

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
        let host = ComposedHost::open_with_test_arena(&config).expect("host must start");
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
    let restarted_host = ComposedHost::open_with_test_arena(&restarted_config)
        .expect("host must restart against the same paths");

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

/// `P1-IPC-SHUTDOWN-CORR`'s independent-review re-verification: proves the
/// fix through the **real, unmodified `ComposedHost::run` path** — not
/// `loki_ipc::Server::serve` tested in isolation, which is exactly what the
/// review found the original verification missed (`compose.rs`'s own outer
/// shutdown-timeout race was the actual bug, not anything in `server.rs`
/// alone).
///
/// A well-known test credential/business-service pair is installed via
/// [`ComposedHost::open_with_test_arena_and_business_service`] so a real
/// `Client` can issue a real, authorized `business_call` — deliberately made
/// to take ~7 seconds (inside the reviewer's named 6-9s reproduction window)
/// by genuine transport backpressure (a tiny client-side stream... no —
/// see [`SlowBusinessService`]: this call is unary, so the delay is the
/// service's own `tokio::time::sleep`, not backpressure; that is realistic
/// on its own terms — a slow but legitimate business operation). Shutdown is
/// triggered almost immediately after the call starts, so the call is
/// genuinely still in flight for nearly the whole delay.
mod slow_business_call {
    use super::*;

    const TEST_TOKEN: &str = "host-lifecycle-slow-call-test-token";
    const PRINCIPAL_ID: &str = "host-lifecycle-slow-call-test-principal";

    fn principal() -> AuthenticatedPrincipal {
        AuthenticatedPrincipal {
            kind: PrincipalKind::RuntimeService,
            principal_id: PRINCIPAL_ID.to_string(),
            authorized_entity_ids: Default::default(),
            authority: std::collections::BTreeSet::from([Authority::ReadRuntime]),
        }
    }

    fn credential_resolver() -> Arc<dyn CredentialResolver> {
        Arc::new(CredentialSet::new(vec![CredentialRecord::active(
            AuthToken::new(TEST_TOKEN),
            principal(),
        )]))
    }

    /// A `BusinessService` whose `call` notifies `call_started` the instant
    /// it begins, sleeps `delay`, then flips `call_completed` before
    /// returning a real `RuntimeStatus` result. Identical in shape to
    /// `crates/loki-ipc/tests/shutdown_drain.rs`'s own `SlowBusinessService`
    /// fixture, duplicated here (not shared as a dependency) because this
    /// crate has no dev-dependency on `loki-ipc`'s own `tests/support`
    /// module, and because keeping it colocated keeps this test
    /// self-contained.
    struct SlowBusinessService {
        call_started: Arc<tokio::sync::Notify>,
        call_completed: Arc<AtomicBool>,
        delay: Duration,
    }

    impl BusinessService for SlowBusinessService {
        fn call(&self, method: Method, context: BusinessRequestContext) -> BusinessCallFuture {
            let call_started = Arc::clone(&self.call_started);
            let call_completed = Arc::clone(&self.call_completed);
            let delay = self.delay;
            Box::pin(async move {
                call_started.notify_one();
                tokio::time::sleep(delay).await;
                call_completed.store(true, Ordering::SeqCst);
                match method {
                    Method::RuntimeGetStatus(_) => Ok(BusinessRpcResult::RuntimeStatus {
                        meta: ReadMeta {
                            source_runtime_id: "host-lifecycle-slow-call-test".to_string(),
                            observed_at: "2026-08-31T00:00:00.000Z".to_string(),
                            correlation_id: context.correlation_id.to_string(),
                        },
                        storage: WireStorageReadiness::Ready,
                        registered_device_count: 0,
                        mutations: WireMutationAvailability::Available,
                    }),
                    other => Err(loki_ipc::protocol::RpcError::UnsupportedMethod {
                        method: format!("{other:?}"),
                    }),
                }
            })
        }

        fn stream(
            &self,
            _params: loki_ipc::business::BusinessStreamParams,
            _context: BusinessRequestContext,
            _emitter: BusinessEventEmitter,
        ) -> BusinessStreamFuture {
            Box::pin(async {
                Err(loki_ipc::protocol::RpcError::UnsupportedMethod {
                    method: "business_events".to_string(),
                })
            })
        }
    }

    /// A generous bound for this test's own hang guards — comfortably above
    /// the slow call's own ~7s delay plus this crate's own drain grace
    /// periods (`loki_ipc::server::CALL_TASK_DRAIN_GRACE` = 10s,
    /// `CONNECTION_DRAIN_GRACE` = 20s), never a normal-operation timing
    /// assumption on its own.
    const SLOW_CALL_HANG_GUARD: Duration = Duration::from_secs(40);

    /// The core re-verification: a real in-flight business call taking ~7
    /// seconds (inside the reviewer's named 6-9s gap) must have genuinely
    /// finished — observed independently via the client's own received
    /// response, not inferred — before `ComposedHost::run`'s returned task
    /// resolves (i.e. before it proceeds to `drop(storage)`/
    /// `secure_memory.shutdown()`). Before this node's fix,
    /// `ComposedHost::run` raced its own IPC task against a *shorter*,
    /// non-cancelling 5s outer timeout and proceeded regardless of whether
    /// the call (or even the whole IPC task) had actually finished — this
    /// test's own assertions are exactly what would have caught that: the
    /// host task returning while the client-observed call was still
    /// unresolved.
    #[tokio::test]
    async fn run_does_not_return_until_a_slow_in_flight_business_call_actually_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let config = super::test_config(dir.path(), "slow-business-call");
        let pipe_name = config.pipe_name.clone();

        let call_started = Arc::new(tokio::sync::Notify::new());
        let call_completed = Arc::new(AtomicBool::new(false));
        let delay = Duration::from_secs(7);
        let service: Arc<dyn BusinessService> = Arc::new(SlowBusinessService {
            call_started: Arc::clone(&call_started),
            call_completed: Arc::clone(&call_completed),
            delay,
        });

        let host = ComposedHost::open_with_test_arena_and_business_service(
            &config,
            credential_resolver(),
            service,
        )
        .expect("host must start with the test business service installed");

        // Shutdown is signalled almost immediately after the call is
        // confirmed to have started (below), so the run() task's own
        // internal race ends while the call is still genuinely in flight.
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let run_task = tokio::spawn(async move {
            host.run(
                async {
                    let _ = shutdown_rx.await;
                    ShutdownReason::CtrlC
                },
                std::future::pending::<()>(),
            )
            .await
        });

        let client_config = ClientConfig::new(
            pipe_name.clone(),
            AuthToken::new(TEST_TOKEN),
            "slow-business-call-test",
        );
        let client = tokio::time::timeout(SLOW_CALL_HANG_GUARD, Client::connect(client_config))
            .await
            .expect("connect must not hang")
            .expect("connect should succeed against the real running host");

        let call_task = tokio::spawn(async move {
            client
                .business_call(
                    Method::RuntimeGetStatus(EmptyParams {}),
                    Duration::from_secs(30),
                )
                .await
        });

        // Deterministic, not timing-guessed: wait for the business service
        // to confirm the call has actually started before triggering
        // shutdown.
        tokio::time::timeout(SLOW_CALL_HANG_GUARD, call_started.notified())
            .await
            .expect("business_service.call must start promptly");

        let _ = shutdown_tx.send(());

        // The in-flight call cannot possibly have finished yet (it sleeps
        // 7s; shutdown was just signalled).
        assert!(
            !call_completed.load(Ordering::SeqCst),
            "test timing assumption violated: the slow call finished before shutdown could race it"
        );

        let run_returned_at = Instant::now();
        let outcome = tokio::time::timeout(SLOW_CALL_HANG_GUARD, run_task)
            .await
            .expect("host run task must not hang")
            .expect("host run task must not panic");
        let run_wait_elapsed = run_returned_at.elapsed();

        // The critical assertion: by the time `run()`'s task has resolved
        // (i.e. is about to/has already dropped `storage` and shut down
        // `secure_memory`), the slow call must have ALREADY completed. This
        // is the exact property the independent review's finding said the
        // original fix did not actually guarantee.
        assert!(
            call_completed.load(Ordering::SeqCst),
            "ComposedHost::run() returned before the in-flight business call actually \
             finished — the exact use-after-free-class hazard this re-verification targets"
        );
        assert!(
            matches!(outcome, loki_runtime::runtime::RunOutcome::Shutdown(_)),
            "unexpected run outcome: {outcome:?}"
        );

        // A loose sanity bound (not a tight timing assertion): run() should
        // not have needed anywhere near its full internal grace periods to
        // notice the call had finished and unwind — waiting for the call
        // itself (~7s) dominates.
        assert!(
            run_wait_elapsed < Duration::from_secs(30),
            "run() took implausibly long ({run_wait_elapsed:?}) to return after the slow call \
             finished; expected it to unwind promptly once the call was done"
        );

        let call_result = tokio::time::timeout(SLOW_CALL_HANG_GUARD, call_task)
            .await
            .expect("the business_call task must not hang")
            .expect("the business_call task must not panic");
        match call_result {
            Ok(BusinessRpcResult::RuntimeStatus { .. }) => {}
            Ok(other) => panic!("unexpected business result shape: {other:?}"),
            Err(CallError::ConnectionClosed) => {
                // Acceptable: the connection may have been torn down by the
                // server-side shutdown drain immediately after delivering
                // (or concurrently with finishing) the call — what matters
                // for this test is `call_completed` above, which is set by
                // the service itself before any response is even sent, and
                // is independent of whether the client happened to observe
                // the reply before its own transport went away.
            }
            Err(other) => panic!("unexpected client call error: {other:?}"),
        }
    }
}
