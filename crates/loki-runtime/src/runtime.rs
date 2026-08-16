//! Composition/orchestration for the long-running "serve" loop.
//!
//! Races the shutdown signal against the heartbeat task so a panic (or any
//! unexpected return) in the heartbeat is detected and surfaced instead of
//! the process hanging forever with a dead liveness signal — Deliverable 6
//! of `Docs/graph/tasks/RUN-001.md` calls this out explicitly ("an
//! injected/simulated crash path is handled without a silent hang").
//!
//! This module performs no logging itself (see `src/app.rs`, the
//! composition root that owns all of this crate's log output) so its
//! behavior can be asserted directly in tests without capturing/parsing log
//! output.

use std::fmt;
use std::future::Future;

use crate::shutdown::ShutdownReason;

/// Why [`run`] returned.
#[derive(Debug)]
pub enum RunOutcome {
    /// A shutdown signal fired; the process should exit cleanly.
    Shutdown(ShutdownReason),
    /// The heartbeat task ended on its own — panic or an unexpected normal
    /// return. Both are abnormal for this loop (a production heartbeat never
    /// returns), so the process should exit non-zero rather than continue
    /// running with no working liveness signal.
    TaskFailure(String),
}

impl fmt::Display for RunOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunOutcome::Shutdown(reason) => write!(f, "shutdown ({reason})"),
            RunOutcome::TaskFailure(msg) => write!(f, "task failure ({msg})"),
        }
    }
}

/// Runs until `shutdown_signal` resolves (clean exit) or `heartbeat`
/// resolves/panics on its own (treated as a crash).
///
/// Generic over both futures so tests can inject a fast/deterministic
/// shutdown trigger or a deliberately panicking/returning heartbeat instead
/// of waiting on a real OS signal or a real interval timer.
pub async fn run<S, H>(shutdown_signal: S, heartbeat: H) -> RunOutcome
where
    S: Future<Output = ShutdownReason>,
    H: Future<Output = ()> + Send + 'static,
{
    let mut heartbeat_handle = tokio::spawn(heartbeat);

    tokio::select! {
        reason = shutdown_signal => {
            heartbeat_handle.abort();
            RunOutcome::Shutdown(reason)
        }
        joined = &mut heartbeat_handle => {
            match joined {
                Ok(()) => RunOutcome::TaskFailure(
                    "heartbeat task ended unexpectedly".to_string(),
                ),
                Err(join_err) => RunOutcome::TaskFailure(format!(
                    "heartbeat task panicked: {join_err}"
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn shutdown_signal_wins_and_returns_promptly() {
        let shutdown = async { ShutdownReason::CtrlC };
        let heartbeat = std::future::pending::<()>();

        let outcome = tokio::time::timeout(Duration::from_secs(2), run(shutdown, heartbeat))
            .await
            .expect("run() must not hang when the shutdown signal is already ready");

        assert!(matches!(
            outcome,
            RunOutcome::Shutdown(ShutdownReason::CtrlC)
        ));
    }

    /// Simulates a crash: an injected heartbeat future that panics instead
    /// of looping forever. `run()` must detect this and return promptly
    /// (bounded by the outer `timeout`), not hang.
    ///
    /// Note: this deliberately panics *inside* a spawned Tokio task. Tokio
    /// catches that panic and reports it via `JoinError` (which is exactly
    /// what this test asserts on) rather than crashing the process, but the
    /// default Rust panic hook still prints
    /// "thread '...' panicked at ...: simulated crash" to stderr during this
    /// test. That output is expected and does not indicate a failing
    /// assertion.
    #[tokio::test]
    async fn a_panicking_heartbeat_is_reported_not_hung() {
        let shutdown = std::future::pending::<ShutdownReason>();
        let heartbeat = async { panic!("simulated crash") };

        let outcome = tokio::time::timeout(Duration::from_secs(2), run(shutdown, heartbeat))
            .await
            .expect("run() must not hang when the heartbeat task panics");

        match outcome {
            RunOutcome::TaskFailure(msg) => assert!(msg.contains("panicked")),
            other => panic!("expected TaskFailure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_heartbeat_that_returns_normally_is_still_reported_as_a_failure() {
        // Production heartbeats never return `Ok(())` (see `health::
        // heartbeat_loop`'s own test); if one ever did, that is itself
        // abnormal and must be surfaced, not silently treated as success.
        let shutdown = std::future::pending::<ShutdownReason>();
        let heartbeat = async {};

        let outcome = tokio::time::timeout(Duration::from_secs(2), run(shutdown, heartbeat))
            .await
            .expect("run() must not hang");

        assert!(matches!(outcome, RunOutcome::TaskFailure(_)));
    }

    #[tokio::test]
    async fn shutdown_aborts_a_still_running_heartbeat() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let ran_to_completion = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran_to_completion);

        let shutdown = async { ShutdownReason::CtrlC };
        let heartbeat = async move {
            // Long enough that an (incorrectly) un-aborted task would still
            // be sleeping when `run()` returns below, and would complete
            // during this test's own post-assertion sleep -- so removing
            // `.abort()` from `run()` would make this test fail, not pass
            // vacuously.
            tokio::time::sleep(Duration::from_millis(200)).await;
            flag.store(true, Ordering::SeqCst);
        };

        let outcome = tokio::time::timeout(Duration::from_secs(2), run(shutdown, heartbeat))
            .await
            .expect("run() must not hang");
        assert!(matches!(
            outcome,
            RunOutcome::Shutdown(ShutdownReason::CtrlC)
        ));

        // Give an (incorrectly) un-aborted task time to finish its sleep and
        // flip the flag before asserting it did not.
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(
            !ran_to_completion.load(Ordering::SeqCst),
            "an aborted heartbeat task must not run to completion after shutdown"
        );
    }
}
