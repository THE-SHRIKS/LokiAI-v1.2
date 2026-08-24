//! Black-box tests that spawn the real, compiled `loki-runtime` binary.
//!
//! These complement (not duplicate) the colocated unit tests in `src/`,
//! which exercise the same decision logic through injected fakes. Only
//! tests here observe a real OS process: real exit codes, real stdout/
//! stderr, and — on Windows, where a console is attached — a real shutdown
//! signal.

mod support;

use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_loki-runtime")
}

/// `RUN-003`: the real `Run`-mode background host always binds the same
/// well-known, per-user named pipe (`loki_runtime::host::discovery::
/// well_known_pipe_name`) regardless of `LOCALAPPDATA` — that determinism is
/// the whole point of the "well-known" pipe (a desktop client does not need
/// to read any file to know where to connect). Two real `Run`-mode
/// processes spawned by this file's tests would therefore race for the same
/// OS pipe name if run concurrently (`cargo test` runs `#[test]` functions
/// within one binary in parallel by default). Every test in this file that
/// spawns a real `Run`-mode child holds this lock for its whole lifetime so
/// at most one such child ever exists at a time.
static RUN_MODE_PIPE_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn health_check_mode_starts_up_logs_json_and_exits_zero() {
    let output = Command::new(binary_path())
        .arg("--health-check")
        // The parent developer/CI environment may set a stricter filter
        // such as `RUST_LOG=warn`. This test verifies the runtime's
        // documented default health-check contract, so isolate it from
        // that ambient operator override.
        .env_remove("RUST_LOG")
        .output()
        .expect("failed to spawn loki-runtime --health-check");

    assert!(
        output.status.success(),
        "expected exit 0, got {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let last_line = stdout
        .lines()
        .next_back()
        .filter(|line| !line.trim().is_empty())
        .unwrap_or_else(|| panic!("--health-check produced no stdout output"));

    let value: serde_json::Value = serde_json::from_str(last_line)
        .unwrap_or_else(|err| panic!("last stdout line was not valid JSON ({err}): {last_line}"));

    assert_eq!(
        value.get("status").and_then(serde_json::Value::as_str),
        Some("ok"),
        "unexpected health-check JSON: {value}"
    );
    assert!(
        value
            .get("run_id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| !s.is_empty()),
        "expected a non-empty run_id correlation field: {value}"
    );
    assert_eq!(
        value.get("level").and_then(serde_json::Value::as_str),
        Some("INFO")
    );
}

#[test]
fn malformed_argument_is_a_clear_error_not_a_panic() {
    let output = Command::new(binary_path())
        .arg("--this-flag-does-not-exist")
        .output()
        .expect("failed to spawn loki-runtime with a bad flag");

    assert!(
        !output.status.success(),
        "a malformed argument must not exit 0"
    );
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--this-flag-does-not-exist"),
        "stderr should name the bad argument: {stderr}"
    );
    assert!(
        !stderr.contains("panicked at") && !stderr.contains("RUST_BACKTRACE"),
        "a malformed argument must not panic/backtrace-spam: {stderr}"
    );
}

#[test]
fn help_flag_prints_usage_and_exits_zero() {
    let output = Command::new(binary_path())
        .arg("--help")
        .output()
        .expect("failed to spawn loki-runtime --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.to_lowercase().contains("usage"));
}

#[test]
fn unexpected_value_on_health_check_is_a_clear_error() {
    let output = Command::new(binary_path())
        .arg("--health-check=now")
        .output()
        .expect("failed to spawn loki-runtime with --health-check=now");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--health-check"));
    assert!(!stderr.contains("panicked at"));
}

/// Real end-to-end proof of graceful shutdown, per
/// `Docs/graph/tasks/RUN-001.md` §8's "Runtime" verification row: spawns the
/// long-running process, sends it a real `CTRL_BREAK_EVENT`, and asserts a
/// clean shutdown log line plus exit code 0 — not code inspection.
///
/// Sending a Windows console-control event requires the *sending* process to
/// be attached to a console. Some automated/sandboxed hosts run `cargo test`
/// with no console attached; when that is what `GenerateConsoleCtrlEvent`
/// itself reports, this test skips (with an explanation) rather than failing
/// the suite on an environment limitation unrelated to `loki-runtime`'s
/// correctness. It does not skip on any other failure, including a hang.
#[cfg(windows)]
#[test]
fn ctrl_break_triggers_graceful_shutdown() {
    let _guard = RUN_MODE_PIPE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // `RUN-003`: `Run` mode now also opens a real `DAT-001` database and
    // discovery file under `LOCALAPPDATA` (`src/host/compose.rs`) — point it
    // at a fresh temp directory so this test never touches the real user
    // profile.
    let local_app_data = tempfile::tempdir().expect("failed to create a temp LOCALAPPDATA");
    let child = support::RunningChild::spawn_with_env(&[(
        "LOCALAPPDATA",
        local_app_data
            .path()
            .to_str()
            .expect("temp dir path must be valid UTF-8"),
    )]);
    let mut stdout_lines = Vec::new();

    // Wait for proof the child has actually finished starting up — not just
    // logged its very first "starting" line, but also finished the
    // synchronous background-host composition (`RUN-003`: real storage
    // migration + real named-pipe bind, see `src/host/compose.rs`) that now
    // runs before the async shutdown-signal listener is ever polled/
    // installed. Signaling any earlier races the child's own startup: an
    // unhandled CTRL_BREAK terminates the process outright (no graceful
    // shutdown, no log line), which would be a bug in this test harness, not
    // evidence of a bug in `loki-runtime`.
    let bound = child.wait_for_stdout_line(Duration::from_secs(10), &mut stdout_lines, |line| {
        line.contains("ipc listener bound and serving")
    });
    assert!(
        bound,
        "loki-runtime did not log the ipc-listener-bound line within 10s; stdout so far: {stdout_lines:?}"
    );
    // Small headroom past that log line for the shutdown-signal future's
    // first poll (where the actual OS console-control handlers are
    // installed) to run — see this test's comment above.
    std::thread::sleep(Duration::from_millis(200));

    if let Err(err) = child.send_ctrl_break() {
        // No shutdown signal was actually delivered, so the child is still
        // running and will not exit on its own -- clean it up directly.
        child.kill();
        eprintln!(
            "skipping ctrl_break_triggers_graceful_shutdown: \
             GenerateConsoleCtrlEvent failed ({err}); this host is not \
             attached to a console."
        );
        return;
    }

    let (status, stderr) = child.wait_for_exit(Duration::from_secs(10), &mut stdout_lines);

    assert!(
        status.success(),
        "expected exit 0 after CTRL_BREAK, got {:?}\nstdout: {}\nstderr: {stderr}",
        status.code(),
        stdout_lines.join("\n"),
    );

    let shutdown_line = stdout_lines
        .iter()
        .find(|line| line.contains("shutting down cleanly"))
        .unwrap_or_else(|| {
            panic!(
                "no clean-shutdown log line found\nstdout: {}\nstderr: {stderr}",
                stdout_lines.join("\n")
            )
        });

    let value: serde_json::Value = serde_json::from_str(shutdown_line).unwrap_or_else(|err| {
        panic!("shutdown log line was not valid JSON ({err}): {shutdown_line}")
    });
    assert_eq!(
        value.get("reason").and_then(serde_json::Value::as_str),
        Some("ctrl_break")
    );
}

/// `RUN-003` Deliverable 1/5, black-box at the real compiled-binary level:
/// the default (`Run`-mode) process composes the real background host — a
/// real `RUN-002` IPC listener a client can actually connect to and `ping`,
/// alongside the pre-existing `RUN-001` health/logging — and a clean
/// shutdown leaves the desktop-discovery file honestly marked `"stopped"`.
///
/// `LOCALAPPDATA` is pointed at a fresh temporary directory for the spawned
/// process so this test never touches the real user profile and never
/// collides with a real LokiAI host that might already be running (or with
/// another concurrent run of this same test) — see `tests/host_lifecycle.rs`
/// for why this crate's other IPC/discovery tests use per-test paths and
/// pipe names for the same reason. The well-known pipe name itself is
/// per-user (not per-`LOCALAPPDATA`), so this test's own process resolves
/// the exact same pipe name the spawned child does by calling the same
/// production function.
///
/// Skips (rather than failing the suite) under the same no-console
/// environment condition `ctrl_break_triggers_graceful_shutdown` documents.
#[cfg(windows)]
#[test]
fn run_mode_composes_a_real_ipc_listener_that_accepts_a_connection_then_shuts_down_cleanly() {
    let _guard = RUN_MODE_PIPE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let local_app_data = tempfile::tempdir().expect("failed to create a temp LOCALAPPDATA");
    let child = support::RunningChild::spawn_with_env(&[(
        "LOCALAPPDATA",
        local_app_data
            .path()
            .to_str()
            .expect("temp dir path must be valid UTF-8"),
    )]);
    let mut stdout_lines = Vec::new();

    let bound = child.wait_for_stdout_line(Duration::from_secs(10), &mut stdout_lines, |line| {
        line.contains("ipc listener bound and serving")
    });
    assert!(
        bound,
        "loki-runtime did not log the ipc-listener-bound line within 10s; stdout so far: {stdout_lines:?}"
    );

    let runtime_dir = local_app_data.path().join("LokiAI").join("runtime");
    let legacy_token_path = runtime_dir.join("ipc.token");
    let raw_token = loki_runtime::host::token::load_or_create(&legacy_token_path)
        .expect("DPAPI-protected IPC token must be readable once serving was logged");
    assert!(
        !legacy_token_path.exists(),
        "the runtime must not recreate the legacy plaintext token"
    );
    let pipe_name = loki_runtime::host::discovery::well_known_pipe_name().expect(
        "failed to resolve the well-known pipe name (same-user, same function the child used)",
    );

    let connect_rt =
        tokio::runtime::Runtime::new().expect("failed to build a client-side tokio runtime");
    connect_rt.block_on(async {
        let client_config = loki_ipc::ClientConfig::new(
            pipe_name,
            loki_ipc::auth::AuthToken::new(raw_token),
            "process-behavior-test-client",
        );
        let client = tokio::time::timeout(
            Duration::from_secs(5),
            loki_ipc::Client::connect(client_config),
        )
        .await
        .expect("connect must not hang")
        .expect("connect should succeed against the real running background host");
        let pong = tokio::time::timeout(
            Duration::from_secs(5),
            client.ping("process-behavior-test", Duration::from_secs(3)),
        )
        .await
        .expect("ping must not hang")
        .expect("ping should succeed");
        assert_eq!(pong.payload, "process-behavior-test");
    });

    if let Err(err) = child.send_ctrl_break() {
        child.kill();
        eprintln!(
            "skipping shutdown assertions in \
             run_mode_composes_a_real_ipc_listener_that_accepts_a_connection_then_shuts_down_cleanly: \
             GenerateConsoleCtrlEvent failed ({err}); this host is not attached to a console."
        );
        return;
    }

    let (status, stderr) = child.wait_for_exit(Duration::from_secs(10), &mut stdout_lines);
    assert!(
        status.success(),
        "expected exit 0 after CTRL_BREAK, got {:?}\nstdout: {}\nstderr: {stderr}",
        status.code(),
        stdout_lines.join("\n"),
    );

    let discovery_contents = std::fs::read_to_string(runtime_dir.join("host.json"))
        .expect("discovery file must exist after a host that successfully started");
    assert!(
        discovery_contents.contains("\"stopped\""),
        "expected the discovery file to record the stopped state after a clean shutdown: {discovery_contents}"
    );
}
