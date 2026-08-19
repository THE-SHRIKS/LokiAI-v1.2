//! Process-level diagnostics bootstrap: installs a single structured JSON
//! `tracing` subscriber (Deliverable 3 of `Docs/graph/tasks/RUN-001.md`).
//!
//! This module is infrastructure, not a bounded context — see
//! [`crate::domain`] for where actual business logic lands in later nodes.

use std::fmt;

use tracing_subscriber::EnvFilter;

/// Failure while installing the global `tracing` subscriber.
///
/// The only realistic cause is a second call to [`init`] within one process
/// (for example a future test harness, or a future embedding of this crate
/// as a library). It is a distinct, typed error rather than a panic so a
/// caller can decide what an already-initialized subscriber means for it.
#[derive(Debug)]
pub struct LoggingInitError(String);

impl fmt::Display for LoggingInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to install global tracing subscriber: {}", self.0)
    }
}

impl std::error::Error for LoggingInitError {}

/// Installs a JSON `tracing` subscriber writing to stdout.
///
/// Level filtering honors the `RUST_LOG` environment variable (default
/// `info`) so operators can raise verbosity without a rebuild. Span context
/// is included in every event (`with_current_span`/`with_span_list`) so a
/// correlation field such as `run_id`, entered once on the top-level run
/// span, is attached to every subsequent log line for that run without every
/// call site having to repeat it.
///
/// Returns `Err` instead of panicking if a global subscriber is already
/// installed.
pub fn init() -> Result<(), LoggingInitError> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .json()
        // `tracing-subscriber` defaults to stderr. LokiAI's process
        // contract and black-box health-check test require structured
        // diagnostics on stdout, so make the destination explicit rather
        // than relying on a library default.
        .with_writer(std::io::stdout)
        .flatten_event(true)
        .with_target(true)
        .with_current_span(true)
        .with_span_list(true)
        .with_env_filter(filter)
        .try_init()
        .map_err(|err| LoggingInitError(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_is_operator_readable() {
        let err =
            LoggingInitError("a global default trace dispatcher has already been set".to_string());
        let rendered = err.to_string();
        assert!(rendered.contains("failed to install global tracing subscriber"));
        assert!(rendered.contains("already been set"));
    }

    /// `try_init()` is documented to error (not panic) when a global
    /// subscriber already exists. This test only asserts the *second* call
    /// here, so it is correct regardless of whether this test or an earlier
    /// one in this binary happened to win the first, process-global
    /// installation: by the time this second call runs, one is guaranteed
    /// to already be installed.
    #[test]
    fn second_init_in_process_reports_an_error_not_a_panic() {
        let _ = init();
        let second = init();
        assert!(
            second.is_err(),
            "a second init() call in one process must not silently succeed or panic"
        );
    }
}
