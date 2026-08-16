//! LokiAI runtime process library (`RUN-001`).
//!
//! `src/main.rs` is a thin process entry point that only parses `argv` and
//! forwards to [`run`]; every real decision lives in this library so it can
//! be unit-tested directly (see each module's colocated `tests`) without
//! spawning a real OS process. `tests/*.rs` additionally black-box-tests the
//! compiled binary itself (real process, real exit codes, and — where the
//! host environment allows it — a real OS shutdown signal).
//!
//! ## Scope
//!
//! This node is process skeleton only — see `Docs/graph/tasks/RUN-001.md`
//! §5 "Forbidden scope". It provides:
//!
//! - process bootstrap and CLI parsing ([`cli`]);
//! - structured JSON diagnostics ([`logging`]);
//! - a health mechanism: `--health-check` and a periodic heartbeat
//!   ([`health`]);
//! - graceful shutdown on a Windows console-control signal ([`shutdown`]);
//! - the run/shutdown-race orchestration that ties the above together
//!   ([`runtime`]);
//! - named, empty landing modules for the bounded contexts
//!   `Docs/ARCHITECTURE.md` §7 describes ([`domain`]), so later nodes
//!   (`RUN-002` IPC, `DAT-001` storage, `CON-001`+ connectors, `JOB-001`
//!   jobs, and others not yet chartered) have a place to land instead of
//!   growing a monolith.
//!
//! No IPC, persistence, connector, or device access is implemented here.

mod app;
pub mod cli;
pub mod domain;
pub mod health;
pub mod logging;
pub mod runtime;
pub mod shutdown;

pub use app::run;
