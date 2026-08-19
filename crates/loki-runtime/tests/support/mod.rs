//! Shared test-only helpers for driving the real `loki-runtime` binary as a
//! subprocess and sending it a real Windows console-control signal.
//!
//! This is test support code, not part of the `loki_runtime` library or the
//! shipped binary: `#[cfg(windows)]` throughout, `unwrap`/`expect` freely
//! used (a failing helper should fail the test loudly, which is the correct
//! behavior for test code — unlike the "no unwrap/expect on paths reachable
//! from external input" rule for production code in `src/`).

#![cfg(windows)]

use std::io::{BufRead, BufReader, Read};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// `CREATE_NEW_PROCESS_GROUP` (winbase.h): puts the child in its own console
/// process group so `GenerateConsoleCtrlEvent` can target it without also
/// signaling this test process.
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// `CTRL_BREAK_EVENT` (wincon.h).
const CTRL_BREAK_EVENT: u32 = 1;

#[link(name = "kernel32")]
unsafe extern "system" {
    /// <https://learn.microsoft.com/windows/console/generateconsolectrlevent>
    /// Renamed to snake_case on the Rust side (the real exported symbol name
    /// is preserved via `link_name`) to stay clippy/rustc naming-convention
    /// clean without a blanket `#[allow(non_snake_case)]`.
    #[link_name = "GenerateConsoleCtrlEvent"]
    fn generate_console_ctrl_event(dw_ctrl_event: u32, dw_process_group_id: u32) -> i32;
}

/// A spawned `loki-runtime` process with its stdout streamed line-by-line,
/// so a test can wait for a specific line (proof it finished starting up
/// and installed its signal handlers) before sending a signal, instead of
/// guessing with a fixed sleep — `CTRL_BREAK` delivered before the child has
/// registered its handler is not graceful shutdown, it is unhandled
/// termination, and asserting against that would be testing a race in this
/// harness, not `loki-runtime`.
pub struct RunningChild {
    child: Child,
    stdout_lines: Receiver<String>,
    stderr_thread: Option<JoinHandle<String>>,
}

impl RunningChild {
    /// Spawns the compiled `loki-runtime` binary with no arguments (the
    /// default, long-running "run" mode) in its own console process group,
    /// with `envs` set on the child in addition to its inherited
    /// environment — used by `RUN-003` tests to point `LOCALAPPDATA` at a
    /// temporary directory so a black-box test of the real binary's
    /// background-host startup never touches the real user profile. Pass an
    /// empty slice for no additional environment overrides.
    pub fn spawn_with_env(envs: &[(&str, &str)]) -> Self {
        let binary = env!("CARGO_BIN_EXE_loki-runtime");
        let mut command = Command::new(binary);
        command
            // These black-box tests assert the documented default INFO
            // lifecycle events. An ambient developer RUST_LOG override must
            // not silently change the behavior under test.
            .env_remove("RUST_LOG")
            .creation_flags(CREATE_NEW_PROCESS_GROUP)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in envs {
            command.env(key, value);
        }
        let mut child = command.spawn().expect("failed to spawn loki-runtime");

        let stdout_pipe = child.stdout.take().expect("stdout was not piped");
        let stderr_pipe = child.stderr.take().expect("stderr was not piped");

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout_pipe);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
            // The sender drops here, closing the channel; the wait loops
            // below treat a closed channel as "no more output is coming"
            // (the child exited or otherwise closed stdout).
        });

        let stderr_thread = std::thread::spawn(move || {
            let mut stderr_pipe = stderr_pipe;
            let mut buf = String::new();
            let _ = stderr_pipe.read_to_string(&mut buf);
            buf
        });

        Self {
            child,
            stdout_lines: rx,
            stderr_thread: Some(stderr_thread),
        }
    }

    fn id(&self) -> u32 {
        self.child.id()
    }

    /// Blocks until a stdout line matching `predicate` appears, or `timeout`
    /// elapses (returning `false`). Every line seen (matching or not) is
    /// appended to `seen`, so the caller retains full stdout for
    /// diagnostics either way.
    pub fn wait_for_stdout_line(
        &self,
        timeout: Duration,
        seen: &mut Vec<String>,
        predicate: impl Fn(&str) -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            match self.stdout_lines.recv_timeout(remaining) {
                Ok(line) => {
                    let is_match = predicate(&line);
                    seen.push(line);
                    if is_match {
                        return true;
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return false,
            }
        }
    }

    /// Sends `CTRL_BREAK_EVENT` to this process's group. Returns `Err` with
    /// the raw OS error (rather than panicking) if the call fails, so a
    /// caller in an environment with no attached console can skip instead of
    /// hard-failing the whole suite.
    pub fn send_ctrl_break(&self) -> Result<(), std::io::Error> {
        // SAFETY: takes two plain integers, no aliasing/lifetime/
        // initialization preconditions beyond being called from a process
        // attached to a console -- a failure of that precondition surfaces
        // as an ordinary last-error code, not undefined behavior.
        let ok = unsafe { generate_console_ctrl_event(CTRL_BREAK_EVENT, self.id()) };
        if ok == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Drains any remaining stdout lines (appending to `seen`) and waits for
    /// the process to exit, up to `timeout`. Fails the test — rather than
    /// hanging the suite forever — if the deadline passes first.
    pub fn wait_for_exit(
        mut self,
        timeout: Duration,
        seen: &mut Vec<String>,
    ) -> (ExitStatus, String) {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!("loki-runtime did not exit within {timeout:?} -- treating as a hang");
            }
            match self.stdout_lines.recv_timeout(remaining) {
                Ok(line) => seen.push(line),
                Err(RecvTimeoutError::Timeout) => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    panic!("loki-runtime did not exit within {timeout:?} -- treating as a hang");
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        let status = self.child.wait().expect("failed to wait on loki-runtime");
        let stderr = self
            .stderr_thread
            .take()
            .map(|h| h.join().expect("stderr reader thread panicked"))
            .unwrap_or_default();
        (status, stderr)
    }

    /// Kills the process immediately without waiting for a graceful exit —
    /// used when a test cannot deliver a shutdown signal at all (e.g. no
    /// console attached) and must still clean up.
    pub fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
