//! Black-box tests that spawn the real, compiled `loki-runtime` binary.
//!
//! These complement (not duplicate) the colocated unit tests in `src/`,
//! which exercise the same decision logic through injected fakes. Only
//! tests here observe a real OS process: real exit codes, real stdout/
//! stderr, and — on Windows, where a console is attached — a real shutdown
//! signal.

mod support;

use std::process::Command;
use std::time::Duration;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_loki-runtime")
}

#[test]
fn health_check_mode_starts_up_logs_json_and_exits_zero() {
    let output = Command::new(binary_path())
        .arg("--health-check")
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
    let child = support::RunningChild::spawn();
    let mut stdout_lines = Vec::new();

    // Wait for proof the child has actually finished starting up (built its
    // Tokio runtime and registered its console-control handlers) before
    // signaling it. Sending CTRL_BREAK any earlier races the child's own
    // startup: an unhandled CTRL_BREAK terminates the process outright
    // (no graceful shutdown, no log line), which would be a bug in this test
    // harness, not evidence of a bug in `loki-runtime`.
    let started = child.wait_for_stdout_line(Duration::from_secs(5), &mut stdout_lines, |line| {
        line.contains("loki-runtime starting")
    });
    assert!(
        started,
        "loki-runtime did not log a startup line within 5s; stdout so far: {stdout_lines:?}"
    );

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
