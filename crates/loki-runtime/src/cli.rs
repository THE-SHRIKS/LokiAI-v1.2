//! Command-line argument parsing.
//!
//! Intentionally hand-rolled rather than pulling in an argument-parsing
//! crate: this node ships a process skeleton with exactly two flags, and
//! AGENTS.md rule 11 forbids a dependency added merely for convenience. If a
//! later node needs a materially larger CLI surface, that is the point to
//! reconsider this decision, not before.

use std::fmt;

/// What the process should do after parsing `argv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Run the long-lived service (default with no recognized-mode flags).
    /// `RUN-003`: this composes the real background host (IPC listener +
    /// persisted storage) alongside `RUN-001`'s health/logging — see
    /// `src/host/compose.rs`.
    Run,
    /// Perform a one-shot self-check and exit; see [`crate::health`].
    HealthCheck,
    /// `RUN-003`: supervise `Run` mode as a child process, restarting it on
    /// an unexpected (non-zero/killed) exit, bounded by a restart budget —
    /// see `src/host/supervisor.rs`. Userland crash recovery only, not an
    /// OS-service-manager integration (that is a future packaging node's
    /// scope, per `Docs/graph/tasks/RUN-003.md` §7).
    Supervised,
    /// Print usage and exit successfully.
    Help,
}

/// A command line that could not be parsed. `Display` is the exact message
/// written to stderr, so it stays operator-readable rather than a debug
/// dump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// An argument that is not one of the recognized flags.
    UnknownArgument(String),
    /// A recognized boolean flag was given a `--flag=value` value it does
    /// not accept.
    UnexpectedValue { flag: &'static str, value: String },
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::UnknownArgument(arg) => write!(f, "unrecognized argument '{arg}'"),
            CliError::UnexpectedValue { flag, value } => {
                write!(f, "'{flag}' does not take a value (got '{value}')")
            }
        }
    }
}

impl std::error::Error for CliError {}

/// Parses `argv`, including `argv[0]` (the program name, mirroring
/// `std::env::args()`), which is ignored. Never panics on malformed input —
/// every rejection path returns `Err` instead.
///
/// If more than one mode-selecting flag is given, the last one wins; this is
/// simple, predictable, and easy to explain in an error/help message.
pub fn parse<S: AsRef<str>>(argv: &[S]) -> Result<Mode, CliError> {
    let mut mode = Mode::Run;

    for raw in argv.iter().skip(1) {
        let arg = raw.as_ref();
        let (flag, value) = match arg.split_once('=') {
            Some((f, v)) => (f, Some(v)),
            None => (arg, None),
        };

        match flag {
            "--help" | "-h" => mode = Mode::Help,
            "--health-check" => {
                if let Some(value) = value {
                    return Err(CliError::UnexpectedValue {
                        flag: "--health-check",
                        value: value.to_string(),
                    });
                }
                mode = Mode::HealthCheck;
            }
            "--supervised" => {
                if let Some(value) = value {
                    return Err(CliError::UnexpectedValue {
                        flag: "--supervised",
                        value: value.to_string(),
                    });
                }
                mode = Mode::Supervised;
            }
            _ => return Err(CliError::UnknownArgument(arg.to_string())),
        }
    }

    Ok(mode)
}

/// Prints usage to stdout (conventional destination for `--help`; parse
/// *errors* are reported separately, on stderr, by the caller).
pub fn print_usage() {
    println!(
        "loki-runtime {version}\n\
         LokiAI Main v1.2 runtime process.\n\
         \n\
         Usage: loki-runtime [OPTIONS]\n\
         \n\
         Options:\n\
         \x20\x20--health-check   Run a one-shot self-check and exit\n\
         \x20\x20--supervised     Supervise the background host as a child\n\
         \x20\x20                 process, restarting it after an\n\
         \x20\x20                 unexpected crash (bounded restart budget)\n\
         \x20\x20-h, --help       Print this message and exit\n\
         \n\
         With no options, runs the long-lived background host (IPC listener\n\
         + persisted storage + heartbeat/health logging) until a shutdown\n\
         signal (Ctrl-C, Ctrl-Break, console close, logoff, or shutdown) is\n\
         received.",
        version = env!("CARGO_PKG_VERSION"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_defaults_to_run_mode() {
        let argv = ["loki-runtime"];
        assert_eq!(parse(&argv), Ok(Mode::Run));
    }

    #[test]
    fn health_check_flag_selects_health_check_mode() {
        let argv = ["loki-runtime", "--health-check"];
        assert_eq!(parse(&argv), Ok(Mode::HealthCheck));
    }

    #[test]
    fn supervised_flag_selects_supervised_mode() {
        let argv = ["loki-runtime", "--supervised"];
        assert_eq!(parse(&argv), Ok(Mode::Supervised));
    }

    #[test]
    fn supervised_rejects_a_value() {
        let argv = ["loki-runtime", "--supervised=now"];
        let err = parse(&argv).expect_err("--supervised does not take a value");
        assert_eq!(
            err,
            CliError::UnexpectedValue {
                flag: "--supervised",
                value: "now".to_string(),
            }
        );
        assert!(err.to_string().contains("--supervised"));
        assert!(err.to_string().contains("now"));
    }

    #[test]
    fn help_flags_select_help_mode() {
        for flag in ["--help", "-h"] {
            let argv = ["loki-runtime", flag];
            assert_eq!(parse(&argv), Ok(Mode::Help), "flag {flag}");
        }
    }

    #[test]
    fn unknown_flag_is_a_clear_error_not_a_panic() {
        let argv = ["loki-runtime", "--definitely-not-a-real-flag"];
        let err = parse(&argv).expect_err("unknown flag must be rejected");
        assert_eq!(
            err,
            CliError::UnknownArgument("--definitely-not-a-real-flag".to_string())
        );
        assert!(err.to_string().contains("--definitely-not-a-real-flag"));
    }

    #[test]
    fn health_check_rejects_a_value() {
        let argv = ["loki-runtime", "--health-check=now"];
        let err = parse(&argv).expect_err("--health-check does not take a value");
        assert_eq!(
            err,
            CliError::UnexpectedValue {
                flag: "--health-check",
                value: "now".to_string(),
            }
        );
        assert!(err.to_string().contains("--health-check"));
        assert!(err.to_string().contains("now"));
    }

    #[test]
    fn last_mode_flag_wins_when_flags_conflict() {
        let argv = ["loki-runtime", "--health-check", "--help"];
        assert_eq!(parse(&argv), Ok(Mode::Help));
    }

    #[test]
    fn empty_argv_does_not_panic() {
        let argv: [&str; 0] = [];
        assert_eq!(parse(&argv), Ok(Mode::Run));
    }
}
