//! Userland crash-recovery supervisor for the `desktop-user` host mode
//! (`Docs/ARCHITECTURE.md` §3.2).
//!
//! This is **not** an OS service manager integration — per
//! `Docs/graph/tasks/RUN-003.md` §7's stop condition, Windows Service
//! Recovery Actions for the `gateway-service` host mode are a documented
//! expectation for a future packaging node (`PKG-001`+), not built here. A
//! userland process can restart its own crashed child; it cannot install
//! itself as a Windows service or configure SCM recovery actions without
//! elevated installer-time work outside this node's scope.
//!
//! What this module does: spawn `loki-runtime` (normal `Run` mode) as a
//! child process, and — if the child exits with anything other than a clean
//! shutdown (`ExitCode::SUCCESS`, `Docs/graph/tasks/RUN-001.md`'s own
//! contract) — restart it, bounded by [`RestartPolicy`] so a persistently
//! crashing host does not spin forever. The child's own state
//! (`crates/loki-storage`, `DAT-001`) survives every restart because it
//! lives on disk at a fixed path, independent of the crashed process —
//! proven by `tests/host_lifecycle.rs`'s
//! `simulated_crash_and_restart_preserves_persisted_device_state`.
//!
//! The decision logic ([`RestartHistory::decide`]) and loop orchestration
//! ([`run`]) are both generic over an injected spawn/sleep implementation so
//! they are directly unit-testable without a real process or a real
//! wall-clock delay; [`spawn_and_wait_real`] is the thin, separately-tested
//! production implementation `src/app.rs`'s `run_supervised` actually calls.

use std::path::Path;
use std::time::{Duration, Instant};

/// How many restarts within [`RestartPolicy::window`] are tolerated before
/// the supervisor gives up rather than spinning forever.
#[derive(Debug, Clone, Copy)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub window: Duration,
    pub backoff: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 5,
            window: Duration::from_secs(60),
            backoff: Duration::from_secs(2),
        }
    }
}

/// How one supervised child process run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildOutcome {
    /// Exit code 0 — the child shut down gracefully (e.g. a real OS
    /// shutdown signal, per `src/runtime.rs`'s `RunOutcome::Shutdown`). Not
    /// restarted.
    CleanExit,
    /// Any other exit code, or the process could not report one (killed/
    /// terminated abnormally).
    Crashed(Option<i32>),
}

/// What [`RestartHistory::decide`] tells the [`run`] loop to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorAction {
    Stop,
    RestartAfter(Duration),
    GiveUp,
}

/// Restart-attempt history within the sliding [`RestartPolicy::window`].
#[derive(Debug, Default)]
pub struct RestartHistory {
    attempts: Vec<Instant>,
}

impl RestartHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pure decision: given `outcome` and the current time, decides what the
    /// supervisor loop should do next. Records a restart attempt in `self`
    /// exactly when it decides to restart (never on `Stop`/`GiveUp`), and
    /// prunes attempts older than `policy.window` first so a host that
    /// crashed a few times long ago is not penalized forever.
    pub fn decide(
        &mut self,
        outcome: ChildOutcome,
        policy: &RestartPolicy,
        now: Instant,
    ) -> SupervisorAction {
        if outcome == ChildOutcome::CleanExit {
            return SupervisorAction::Stop;
        }
        self.attempts
            .retain(|&t| now.saturating_duration_since(t) <= policy.window);
        if self.attempts.len() as u32 >= policy.max_restarts {
            return SupervisorAction::GiveUp;
        }
        self.attempts.push(now);
        SupervisorAction::RestartAfter(policy.backoff)
    }
}

/// Why [`run`] stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorOutcome {
    /// The child exited cleanly; the supervisor stopped without restarting.
    Stopped,
    /// The restart budget was exhausted; the supervisor gave up rather than
    /// spinning forever on a persistently crashing child.
    GaveUp,
}

/// Drives the supervise loop: spawn a child, decide what to do with its
/// outcome, repeat. Generic over `spawn_and_wait` (spawns one child and
/// blocks until it exits) and `sleep` (the backoff delay) so this is fully
/// unit-testable without a real process or a real wall-clock wait — see this
/// module's tests. `src/app.rs`'s `run_supervised` is the real caller,
/// passing [`spawn_and_wait_real`] and `std::thread::sleep`.
pub fn run<F, S>(mut spawn_and_wait: F, policy: RestartPolicy, mut sleep: S) -> SupervisorOutcome
where
    F: FnMut() -> ChildOutcome,
    S: FnMut(Duration),
{
    let mut history = RestartHistory::new();
    loop {
        let outcome = spawn_and_wait();
        match history.decide(outcome, &policy, Instant::now()) {
            SupervisorAction::Stop => return SupervisorOutcome::Stopped,
            SupervisorAction::GiveUp => return SupervisorOutcome::GaveUp,
            SupervisorAction::RestartAfter(delay) => sleep(delay),
        }
    }
}

/// Spawns `exe args` as a child process and blocks until it exits,
/// classifying the result as [`ChildOutcome`]. The real, production
/// process-spawning primitive [`run`] is driven with; kept separate from
/// `run` itself so the decision/loop logic above stays testable without a
/// real OS process (see this module's top doc comment).
pub fn spawn_and_wait_real(exe: &Path, args: &[String]) -> std::io::Result<ChildOutcome> {
    let status = std::process::Command::new(exe).args(args).status()?;
    Ok(match status.code() {
        Some(0) => ChildOutcome::CleanExit,
        Some(code) => ChildOutcome::Crashed(Some(code)),
        None => ChildOutcome::Crashed(None),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(max_restarts: u32) -> RestartPolicy {
        RestartPolicy {
            max_restarts,
            window: Duration::from_secs(60),
            backoff: Duration::from_millis(1),
        }
    }

    #[test]
    fn a_clean_exit_stops_without_restarting() {
        let mut history = RestartHistory::new();
        let action = history.decide(ChildOutcome::CleanExit, &policy(5), Instant::now());
        assert_eq!(action, SupervisorAction::Stop);
    }

    #[test]
    fn a_crash_within_budget_restarts_after_the_configured_backoff() {
        let mut history = RestartHistory::new();
        let policy = RestartPolicy {
            max_restarts: 5,
            window: Duration::from_secs(60),
            backoff: Duration::from_secs(3),
        };
        let action = history.decide(ChildOutcome::Crashed(Some(1)), &policy, Instant::now());
        assert_eq!(
            action,
            SupervisorAction::RestartAfter(Duration::from_secs(3))
        );
    }

    #[test]
    fn repeated_crashes_within_the_window_eventually_give_up() {
        let mut history = RestartHistory::new();
        let policy = policy(3);
        let now = Instant::now();
        assert_eq!(
            history.decide(ChildOutcome::Crashed(None), &policy, now),
            SupervisorAction::RestartAfter(policy.backoff)
        );
        assert_eq!(
            history.decide(ChildOutcome::Crashed(None), &policy, now),
            SupervisorAction::RestartAfter(policy.backoff)
        );
        assert_eq!(
            history.decide(ChildOutcome::Crashed(None), &policy, now),
            SupervisorAction::RestartAfter(policy.backoff)
        );
        // Fourth crash within the same window exceeds max_restarts (3).
        assert_eq!(
            history.decide(ChildOutcome::Crashed(None), &policy, now),
            SupervisorAction::GiveUp
        );
    }

    #[test]
    fn crashes_outside_the_window_are_not_counted_against_the_budget() {
        let mut history = RestartHistory::new();
        let policy = policy(2);
        let t0 = Instant::now();
        assert_eq!(
            history.decide(ChildOutcome::Crashed(None), &policy, t0),
            SupervisorAction::RestartAfter(policy.backoff)
        );
        assert_eq!(
            history.decide(ChildOutcome::Crashed(None), &policy, t0),
            SupervisorAction::RestartAfter(policy.backoff)
        );
        // Would give up if evaluated at t0 again, but a full window has
        // passed, so both earlier attempts are pruned first.
        let much_later = t0 + policy.window + Duration::from_secs(1);
        assert_eq!(
            history.decide(ChildOutcome::Crashed(None), &policy, much_later),
            SupervisorAction::RestartAfter(policy.backoff)
        );
    }

    #[test]
    fn run_stops_immediately_on_a_clean_exit_without_sleeping() {
        let mut sleeps = Vec::new();
        let outcome = run(
            || ChildOutcome::CleanExit,
            RestartPolicy::default(),
            |d| sleeps.push(d),
        );
        assert_eq!(outcome, SupervisorOutcome::Stopped);
        assert!(sleeps.is_empty());
    }

    #[test]
    fn run_restarts_after_a_crash_then_stops_on_a_later_clean_exit() {
        let mut calls = 0u32;
        let mut sleeps = Vec::new();
        let outcome = run(
            || {
                calls += 1;
                if calls < 3 {
                    ChildOutcome::Crashed(Some(1))
                } else {
                    ChildOutcome::CleanExit
                }
            },
            RestartPolicy {
                max_restarts: 10,
                window: Duration::from_secs(60),
                backoff: Duration::from_millis(5),
            },
            |d| sleeps.push(d),
        );
        assert_eq!(outcome, SupervisorOutcome::Stopped);
        assert_eq!(
            calls, 3,
            "expected exactly 2 restarts before the clean exit"
        );
        assert_eq!(
            sleeps,
            vec![Duration::from_millis(5), Duration::from_millis(5)]
        );
    }

    #[test]
    fn run_gives_up_after_the_restart_budget_is_exhausted() {
        let mut calls = 0u32;
        let outcome = run(
            || {
                calls += 1;
                ChildOutcome::Crashed(Some(1))
            },
            RestartPolicy {
                max_restarts: 3,
                window: Duration::from_secs(60),
                backoff: Duration::from_millis(1),
            },
            |_| {},
        );
        assert_eq!(outcome, SupervisorOutcome::GaveUp);
        assert_eq!(
            calls, 4,
            "3 restarts allowed => 4 total spawn attempts before giving up"
        );
    }

    #[test]
    fn spawn_and_wait_real_propagates_a_spawn_failure_for_a_nonexistent_executable() {
        let result = spawn_and_wait_real(Path::new("this-executable-does-not-exist.exe"), &[]);
        assert!(result.is_err());
    }

    #[test]
    fn spawn_and_wait_real_classifies_a_real_clean_exit() {
        // `cmd.exe /c exit 0` is a real, short-lived Windows process — not
        // an injected closure — proving `spawn_and_wait_real` classifies a
        // real OS process's exit status correctly, not just the `run()`
        // decision logic against a fake.
        let outcome = spawn_and_wait_real(
            Path::new("cmd.exe"),
            &["/c".to_string(), "exit".to_string(), "0".to_string()],
        )
        .expect("cmd.exe must be spawnable on Windows");
        assert_eq!(outcome, ChildOutcome::CleanExit);
    }

    #[test]
    fn spawn_and_wait_real_classifies_a_real_nonzero_exit_as_crashed() {
        let outcome = spawn_and_wait_real(
            Path::new("cmd.exe"),
            &["/c".to_string(), "exit".to_string(), "7".to_string()],
        )
        .expect("cmd.exe must be spawnable on Windows");
        assert_eq!(outcome, ChildOutcome::Crashed(Some(7)));
    }

    #[test]
    fn run_drives_real_os_processes_end_to_end_via_spawn_and_wait_real() {
        // Not injected: this drives the real supervise loop (`run`) with
        // the real, production `spawn_and_wait_real` primitive against
        // `cmd.exe /c exit 7`, a real short-lived Windows process that
        // always exits non-zero — proving the supervising mechanism
        // restarts a *real* crashed process (Deliverable 2 of
        // `Docs/graph/tasks/RUN-003.md`), not just a closure standing in
        // for one.
        let mut attempts = 0u32;
        let policy = RestartPolicy {
            max_restarts: 2,
            window: Duration::from_secs(60),
            backoff: Duration::from_millis(1),
        };
        let outcome = run(
            || {
                attempts += 1;
                spawn_and_wait_real(
                    Path::new("cmd.exe"),
                    &["/c".to_string(), "exit".to_string(), "7".to_string()],
                )
                .expect("cmd.exe must be spawnable on Windows")
            },
            policy,
            |_| {},
        );
        assert_eq!(outcome, SupervisorOutcome::GaveUp);
        assert_eq!(
            attempts, 3,
            "expected the initial attempt plus 2 restarts before giving up"
        );
    }
}
