//! Composition root: the single seam `main.rs` calls.
//!
//! This is the only module in the crate that calls `tracing::*` macros
//! directly — every other module either returns a plain typed value
//! ([`crate::cli`], [`crate::runtime`]) or is a thin OS-signal adapter
//! ([`crate::shutdown`]) so it can be tested without capturing log output.

use std::process::ExitCode;
use std::time::Duration;

use tracing::Instrument;
use uuid::Uuid;

#[cfg(windows)]
use crate::host;
use crate::{cli, health, logging, runtime, shutdown};

/// Heartbeat cadence while the service runs. Not yet operator-configurable —
/// this node is a process skeleton (`Docs/graph/tasks/RUN-001.md` §5); add a
/// CLI/config option only when a real operator need shows up.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Parses `args`, then dispatches to help/health-check/run. Kept
/// synchronous up front (no async runtime is built until [`run_service`])
/// so a malformed argument prints a clear error and returns before Tokio
/// ever starts.
pub fn run(args: Vec<String>) -> ExitCode {
    match cli::parse(&args) {
        Err(err) => {
            eprintln!("loki-runtime: {err}");
            eprintln!("Run 'loki-runtime --help' for usage.");
            ExitCode::from(2)
        }
        Ok(cli::Mode::Help) => {
            cli::print_usage();
            ExitCode::SUCCESS
        }
        Ok(cli::Mode::HealthCheck) => run_health_check(),
        Ok(cli::Mode::Run) => run_service(),
        Ok(cli::Mode::Supervised) => run_supervised(),
    }
}

fn run_health_check() -> ExitCode {
    if logging::init().is_err() {
        // Logging itself failed to initialize, so there is no subscriber to
        // report through; stderr is the only honest channel left.
        eprintln!("loki-runtime: failed to initialize logging for health check");
        return ExitCode::from(1);
    }

    let run_id = Uuid::new_v4();
    let span = tracing::info_span!("run", run_id = %run_id, mode = "health-check");
    let _enter = span.enter();
    health::run_self_check(run_id)
}

fn run_service() -> ExitCode {
    if logging::init().is_err() {
        eprintln!("loki-runtime: failed to initialize logging");
        return ExitCode::from(1);
    }

    let run_id = Uuid::new_v4();
    let span = tracing::info_span!("run", run_id = %run_id, mode = "run");

    let tokio_runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            let _enter = span.enter();
            tracing::error!(run_id = %run_id, error = %err, "failed to start async runtime");
            return ExitCode::from(1);
        }
    };

    {
        let _enter = span.enter();
        tracing::info!(
            run_id = %run_id,
            version = env!("CARGO_PKG_VERSION"),
            "loki-runtime starting"
        );
    }

    let outcome = tokio_runtime.block_on(run_composed(span.clone()));

    let exit = {
        let _enter = span.enter();
        match &outcome {
            runtime::RunOutcome::Shutdown(reason) => {
                tracing::info!(
                    run_id = %run_id,
                    reason = %reason,
                    "loki-runtime shutting down cleanly"
                );
                ExitCode::SUCCESS
            }
            runtime::RunOutcome::TaskFailure(msg) => {
                tracing::error!(
                    run_id = %run_id,
                    error = %msg,
                    "loki-runtime stopped due to an internal task failure"
                );
                ExitCode::from(1)
            }
        }
    };

    // Bound shutdown instead of trusting `Runtime`'s `Drop` to finish
    // promptly on its own — an unbounded drop is exactly the "silent hang"
    // this node must avoid.
    tokio_runtime.shutdown_timeout(Duration::from_secs(5));
    exit
}

/// `RUN-003`: the actual long-running future `run_service` drives.
///
/// On Windows, this composes the real background host — `RUN-002`'s IPC
/// listener and `DAT-001`'s persisted storage — via
/// [`host::compose::ComposedHost`], then reuses [`runtime::run`] (`RUN-001`,
/// unmodified) internally for the same shutdown-vs-heartbeat race as before.
/// A failure to even start the host (bad configuration, storage error, the
/// well-known pipe already bound by another running instance, ...) is
/// reported as [`runtime::RunOutcome::TaskFailure`] — an honest failure, not
/// a silently-downgraded "healthy" process (`AGENTS.md` rule 1).
///
/// On any other target this crate can still be `cargo check`ed on (see
/// `src/host.rs`'s platform-gate doc comment), this falls back to
/// `RUN-001`'s original bare shutdown/heartbeat race with no host — that
/// path is not a supported/shipped v1.2 target (`Docs/TECH_STACK.md` §2).
#[cfg(windows)]
async fn run_composed(span: tracing::Span) -> runtime::RunOutcome {
    let config = match host::compose::HostConfig::for_current_user() {
        Ok(config) => config,
        Err(err) => {
            let _enter = span.enter();
            tracing::error!(error = %err, "failed to resolve background host configuration");
            return runtime::RunOutcome::TaskFailure(format!(
                "failed to resolve background host configuration: {err}"
            ));
        }
    };

    let composed = match host::compose::ComposedHost::open(&config) {
        Ok(composed) => composed,
        Err(err) => {
            let _enter = span.enter();
            tracing::error!(error = %err, "failed to start background host");
            return runtime::RunOutcome::TaskFailure(format!(
                "failed to start background host: {err}"
            ));
        }
    };

    composed
        .run(
            shutdown::wait_for_shutdown_signal(),
            health::heartbeat_loop(HEARTBEAT_INTERVAL).instrument(span.clone()),
        )
        .instrument(span)
        .await
}

#[cfg(not(windows))]
async fn run_composed(span: tracing::Span) -> runtime::RunOutcome {
    runtime::run(
        shutdown::wait_for_shutdown_signal(),
        health::heartbeat_loop(HEARTBEAT_INTERVAL).instrument(span.clone()),
    )
    .instrument(span)
    .await
}

/// `RUN-003` Deliverable 2: userland crash recovery. Spawns `loki-runtime`
/// in its normal `Run` mode (see [`run_composed`]) as a child process and
/// restarts it after an unexpected (non-clean) exit, bounded by
/// [`host::supervisor::RestartPolicy::default`] so a persistently crashing
/// host does not spin forever. The child's `DAT-001` state survives every
/// restart because it lives on disk at a fixed path
/// (`host::paths::HostPaths`), independent of the crashed process — see
/// `crates/loki-runtime/src/host/supervisor.rs`'s doc comment and
/// `tests/host_lifecycle.rs`.
///
/// This is explicitly **not** an OS-service-manager integration — see
/// `host::supervisor`'s doc comment and `Docs/graph/tasks/RUN-003.md` §7's
/// stop condition. A real OS console-control signal (Ctrl-C etc.) delivered
/// to this process's console process group is, by default, also delivered
/// to the child (no `CREATE_NEW_PROCESS_GROUP` is used when spawning it), so
/// an operator stopping the supervisor also stops the currently-supervised
/// child; a clean child exit (0) then makes the supervisor stop rather than
/// restart.
#[cfg(windows)]
fn run_supervised() -> ExitCode {
    if logging::init().is_err() {
        eprintln!("loki-runtime: failed to initialize logging");
        return ExitCode::from(1);
    }

    let run_id = Uuid::new_v4();
    let span = tracing::info_span!("run", run_id = %run_id, mode = "supervised");
    let _enter = span.enter();

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            tracing::error!(run_id = %run_id, error = %err, "supervisor: failed to resolve own executable path");
            return ExitCode::from(1);
        }
    };

    tracing::info!(run_id = %run_id, exe = %exe.display(), "supervisor: starting to supervise loki-runtime");

    let outcome = host::supervisor::run(
        || match host::supervisor::spawn_and_wait_real(&exe, &[]) {
            Ok(outcome) => outcome,
            Err(err) => {
                tracing::error!(run_id = %run_id, error = %err, "supervisor: failed to spawn child process");
                host::supervisor::ChildOutcome::Crashed(None)
            }
        },
        host::supervisor::RestartPolicy::default(),
        std::thread::sleep,
    );

    match outcome {
        host::supervisor::SupervisorOutcome::Stopped => {
            tracing::info!(run_id = %run_id, "supervisor: child exited cleanly; stopping");
            ExitCode::SUCCESS
        }
        host::supervisor::SupervisorOutcome::GaveUp => {
            tracing::error!(run_id = %run_id, "supervisor: restart budget exhausted; giving up");
            ExitCode::from(1)
        }
    }
}

#[cfg(not(windows))]
fn run_supervised() -> ExitCode {
    eprintln!(
        "loki-runtime: --supervised requires the Windows-only background host \
         (crates/loki-ipc targets Windows named pipes only); this platform is \
         not a supported v1.2 target (Docs/TECH_STACK.md \u{a7}2)."
    );
    ExitCode::from(1)
}
