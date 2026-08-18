//! `loki-jobs` — LokiAI Main v1.2 durable job/lease engine (`JOB-001`).
//!
//! A staged job lifecycle on top of `loki_storage::jobs::JobRepository`'s
//! existing `jobs` table (`DAT-001`):
//!
//! ```text
//! queued -> preflight -> executing -> testing -> succeeded
//!                                          \-> failed
//! (preflight|executing|testing) -> cancelled   (cooperative; see JobEngine::request_cancel)
//! (preflight|executing|testing) -> outcome-unknown  (quarantine; ambiguous verify or
//!                                                     detected during JobEngine::reconcile
//!                                                     after a crash)
//! ```
//!
//! Plus: idempotent submission ([`engine::JobEngine::submit`]), a per-device
//! lease with expiry/renewal/reclaim (`crate::lease`), and explicit restart
//! of a `failed`/`outcome-unknown` job as a new attempt
//! ([`engine::JobEngine::restart`]). See `Docs/graph/tasks/JOB-001.md` for
//! exact scope.
//!
//! ## What this crate deliberately is not
//!
//! - **Not a new SQLite table/migration.** This node's writable scope
//!   (`Docs/graph/tasks/JOB-001.md` §2) is `crates/loki-jobs/**` only —
//!   `crates/loki-storage/src/**` (where any new table's repository code
//!   would have to live; `Storage`'s connection is `pub(crate)` to that
//!   crate) is out of bounds. The per-device lease is therefore in-memory,
//!   with crash recovery handled by [`engine::JobEngine::reconcile`]
//!   instead of lease durability — see `crate::lease`'s doc comment for the
//!   full rationale.
//! - **Not an IPC surface.** This is a plain Rust library. `RUN-002`
//!   provides the transport a future consumer (`MCP-003`/`OPS-001`, per the
//!   task packet) wires this engine to; this crate has no network/transport
//!   code of its own.
//! - **Not a policy/approval engine.** `policy-check`/`waiting-approval`
//!   (part of `loki_storage::states::JobState`'s shared vocabulary) belong
//!   to `APR-001`; this engine's own state machine (`crate::transitions`)
//!   does not drive them.
//! - **Not a deployment/rollout engine.** `restarting` (a device-restart
//!   sub-stage), `rollback-pending`/`rolling-back`/`rolled-back`, and
//!   `observing` are `Docs/SPEC.md` 4.8 Deployment concepts sharing the same
//!   `JobState` enum; this engine does not drive them either.
//! - **Not a policy on *what* preflight/execute/verify actually do.** Those
//!   stages' real work (talking to a connector, a device, a build executor)
//!   is entirely the caller's — this engine only enforces *when* a job may
//!   legally move between stages and persists the outcome.

pub mod clock;
pub mod engine;
pub mod error;
mod lease;
mod transitions;

pub use clock::{Clock, SystemClock};
pub use engine::{CancelOutcome, JobEngine, RestartRequest, SubmitOutcome, SubmitRequest};
pub use error::JobsError;
pub use lease::LeaseToken;
