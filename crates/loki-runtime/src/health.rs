//! Health surface for a process with no IPC listener yet (`RUN-002` adds
//! that). Two honest mechanisms, per Deliverable 4 of
//! `Docs/graph/tasks/RUN-001.md`: a one-shot `--health-check` self-check,
//! and a periodic heartbeat log while the service runs. Neither claims to
//! check device, storage, or IPC health — those subsystems do not exist
//! yet, and inventing a status for them would violate AGENTS.md rule 1.

use std::process::ExitCode;
use std::time::Duration;

use uuid::Uuid;

/// Runs the one-shot `--health-check` self-check and returns the process
/// exit code (`SUCCESS` = healthy).
///
/// Scope is intentionally limited to what this binary can honestly claim
/// right now: that it started and that structured logging is working (by
/// the time this runs, the caller has already confirmed `logging::init()`
/// succeeded). There is no device, storage, or IPC subsystem yet to check.
pub fn run_self_check(run_id: Uuid) -> ExitCode {
    tracing::info!(
        run_id = %run_id,
        component = "process",
        status = "ok",
        version = env!("CARGO_PKG_VERSION"),
        "health check passed: process started and logging is initialized"
    );
    ExitCode::SUCCESS
}

/// Logs a heartbeat event every `interval`.
///
/// Runs until cancelled/aborted by the caller (see [`crate::runtime::run`]):
/// this loop never returns on its own. That is deliberate — the caller
/// treats this future ending (return *or* panic) as a crash signal, so a
/// stuck or dead heartbeat is reported instead of the process silently
/// hanging with no working liveness signal.
pub async fn heartbeat_loop(interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    // The first `tick()` fires immediately; skip it so a heartbeat is not
    // logged in the very same instant as the "starting" log line.
    ticker.tick().await;

    let mut beat: u64 = 0;
    loop {
        ticker.tick().await;
        beat += 1;
        tracing::info!(beat, "heartbeat");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn heartbeat_loop_never_returns_on_its_own() {
        let result = tokio::time::timeout(
            Duration::from_millis(150),
            heartbeat_loop(Duration::from_millis(10)),
        )
        .await;

        assert!(
            result.is_err(),
            "heartbeat_loop must run indefinitely; crate::runtime::run's crash \
             detector depends on it never returning Ok on its own"
        );
    }

    #[test]
    fn run_self_check_reports_success() {
        // `ExitCode` is intentionally opaque in stable Rust (no equality/
        // introspection API), so the exact process exit code this produces
        // is verified end-to-end in tests/process_behavior.rs, which spawns
        // the real binary and inspects `ExitStatus`. Here we only assert
        // that a call with a fresh run id does not panic.
        let _ = run_self_check(Uuid::new_v4());
    }
}
