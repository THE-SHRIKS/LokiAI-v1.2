//! Cross-platform graceful-shutdown signal listening.
//!
//! `Docs/TECH_STACK.md` §2 scopes v1.2 to Windows 11 x64, so the real
//! implementation below is Windows-specific: it listens for Ctrl-C and the
//! Windows console-control equivalents of SIGTERM (console close, system
//! shutdown, user logoff) plus Ctrl-Break, using Tokio's own `signal`
//! feature (see the dependency note in `Cargo.toml` for why no separate
//! signal-handling crate is added). A minimal non-Windows fallback exists
//! only so `cargo check` does not hard-fail for a contributor on another OS;
//! it is not a supported/shipped target.
//!
//! `RUN-002` will extend shutdown with an authenticated-IPC shutdown
//! request; this module only owns OS-signal listening and does not log —
//! see `src/app.rs`, the only place in this crate that writes log output.

use std::fmt;

/// Which signal triggered shutdown, kept for structured shutdown logging by
/// the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownReason {
    CtrlC,
    CtrlBreak,
    CtrlClose,
    CtrlShutdown,
    CtrlLogoff,
    /// A signal listener itself failed to install. Still shuts down rather
    /// than continuing to run in a state where it can never be told to
    /// stop.
    ListenerFailed,
}

impl fmt::Display for ShutdownReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ShutdownReason::CtrlC => "ctrl_c",
            ShutdownReason::CtrlBreak => "ctrl_break",
            ShutdownReason::CtrlClose => "ctrl_close",
            ShutdownReason::CtrlShutdown => "ctrl_shutdown",
            ShutdownReason::CtrlLogoff => "ctrl_logoff",
            ShutdownReason::ListenerFailed => "listener_failed",
        };
        f.write_str(s)
    }
}

/// Waits for the first OS shutdown-equivalent signal and returns which one
/// fired.
///
/// Never panics: a failure to install one specific listener is reported
/// through the returned [`ShutdownReason::ListenerFailed`] rather than an
/// `Err`/panic, so a single console-API failure cannot leave the process in
/// a state that can never be told to stop.
pub async fn wait_for_shutdown_signal() -> ShutdownReason {
    listen().await
}

#[cfg(windows)]
async fn listen() -> ShutdownReason {
    use tokio::signal::windows;

    // Each `ctrl_*` constructor can fail if the OS refuses to install the
    // handler. Rather than unwrap/expect (this is on the direct path from an
    // OS signal, exactly what AGENTS.md's acceptance criteria calls out),
    // fall back to the `ListenerFailed` reason so a single failed listener
    // is reported instead of panicking or silently only covering the other
    // signals.
    let mut ctrl_c = match windows::ctrl_c() {
        Ok(sig) => sig,
        Err(_) => return ShutdownReason::ListenerFailed,
    };
    let mut ctrl_break = match windows::ctrl_break() {
        Ok(sig) => sig,
        Err(_) => return ShutdownReason::ListenerFailed,
    };
    let mut ctrl_close = match windows::ctrl_close() {
        Ok(sig) => sig,
        Err(_) => return ShutdownReason::ListenerFailed,
    };
    let mut ctrl_shutdown = match windows::ctrl_shutdown() {
        Ok(sig) => sig,
        Err(_) => return ShutdownReason::ListenerFailed,
    };
    let mut ctrl_logoff = match windows::ctrl_logoff() {
        Ok(sig) => sig,
        Err(_) => return ShutdownReason::ListenerFailed,
    };

    tokio::select! {
        _ = ctrl_c.recv() => ShutdownReason::CtrlC,
        _ = ctrl_break.recv() => ShutdownReason::CtrlBreak,
        _ = ctrl_close.recv() => ShutdownReason::CtrlClose,
        _ = ctrl_shutdown.recv() => ShutdownReason::CtrlShutdown,
        _ = ctrl_logoff.recv() => ShutdownReason::CtrlLogoff,
    }
}

/// Non-Windows fallback so `cargo check` does not hard-fail for a
/// contributor on another OS. `Docs/TECH_STACK.md` §2 scopes v1.2 to Windows
/// 11 x64; this path is not a supported/shipped target.
#[cfg(not(windows))]
async fn listen() -> ShutdownReason {
    match tokio::signal::ctrl_c().await {
        Ok(()) => ShutdownReason::CtrlC,
        Err(_) => ShutdownReason::ListenerFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_uses_stable_snake_case_identifiers() {
        let cases = [
            (ShutdownReason::CtrlC, "ctrl_c"),
            (ShutdownReason::CtrlBreak, "ctrl_break"),
            (ShutdownReason::CtrlClose, "ctrl_close"),
            (ShutdownReason::CtrlShutdown, "ctrl_shutdown"),
            (ShutdownReason::CtrlLogoff, "ctrl_logoff"),
            (ShutdownReason::ListenerFailed, "listener_failed"),
        ];
        for (reason, expected) in cases {
            assert_eq!(reason.to_string(), expected);
        }
    }

    // `wait_for_shutdown_signal` itself listens on real OS signal sources
    // and has no injection point by design (it is the thin edge adapter);
    // its behavior is proven by `tests/process_behavior.rs` (spawns the
    // real binary) and by `crate::runtime::run`'s unit tests, which cover
    // the race/abort logic against an injected `ShutdownReason` future.
}
