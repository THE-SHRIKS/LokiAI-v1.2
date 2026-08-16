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

    let outcome = tokio_runtime.block_on(
        runtime::run(
            shutdown::wait_for_shutdown_signal(),
            health::heartbeat_loop(HEARTBEAT_INTERVAL).instrument(span.clone()),
        )
        .instrument(span.clone()),
    );

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
