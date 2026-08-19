//! `loki-incidents` — incident correlation and evidence engine (`INC-001`).
//!
//! Turns a `loki-monitoring` (`MON-002`) `RuleEvaluator`
//! `Fired`/`MissingDataAlert` [`evaluate::EvaluationOutcome`] outcome into a
//! correlated, bounded-evidence [`incident::Incident`] with the state
//! machine `Docs/SPEC.md` §4.6 defines exactly, plus assignment, an
//! append-only timeline, verification-window resolution, reopen/close
//! policy, and [`work_intent::WorkIntent`] emission per `Docs/SPEC.md` §3.13's
//! work-intent fields.
//!
//! # Module map
//!
//! | Module | Deliverable | Contents |
//! |---|---|---|
//! | [`incident`] | 1, 2, 6, 7, 8 | [`incident::Incident`], `Docs/SPEC.md` §3.7 field list, the §4.6 state machine |
//! | [`evidence`] | 4 | [`evidence::EvidenceSnapshot`] — bounded evidence, never unbounded |
//! | [`work_intent`] | 9 | [`work_intent::WorkIntent`] — `Docs/SPEC.md` §3.13 field list, emission-only |
//! | [`engine`] | 3, 5, 9, 10 | [`engine::IncidentEngine`] — correlation, ingestion, reopen/retention policy sweeps |
//! | [`store`] | — | [`store::IncidentStore`] — this crate's own self-contained SQLite database |
//! | [`ids`] | — | Stable identifier newtypes ([`ids::IncidentId`], [`ids::EvidenceId`], [`ids::WorkIntentId`]) |
//! | [`error`] | — | [`error::IncidentError`] |
//!
//! # What this crate deliberately is not
//!
//! - **Not a delivery mechanism.** [`engine::IncidentEngine::ingest_fired_outcome`]
//!   *emits* a [`work_intent::WorkIntent`] record; nothing in this crate
//!   delivers it to any provider/agent (`AGT-002`, blocked on `AGT-001`/
//!   `NOT-001`) or acknowledges/expires/cancels it — see
//!   [`work_intent`]'s module doc.
//! - **Not a device-changing action.** This crate never itself executes,
//!   flashes, restarts, or otherwise changes a device — that is
//!   `OPS-001`/`APR-001`'s scope. `IncidentTransition::ApprovalGranted` and
//!   `IncidentTransition::ActionCompleted`/`ActionFailedSafely` only ever
//!   record that *some other subsystem* made that decision/took that
//!   action; this crate has no execution authority of its own.
//! - **Not an evaluation engine.** This crate consumes
//!   `loki_monitoring::evaluate::EvaluationOutcome` as-is; it does not
//!   evaluate monitoring-rule conditions itself (`MON-002`'s scope).
//! - **Not an IPC surface.** No transport/network code; a later node wires
//!   this crate's `IncidentEngine` into `RUN-002`'s IPC layer and a real
//!   work-intent delivery mechanism.
//! - **Not a real device/workspace authorization model.** `affected_scope`
//!   carries `loki_connectors_sdk::ids::DeviceId` values mechanically, the
//!   same posture `Docs/graph/handoffs/MON-002-1.md` already documents for
//!   `loki_monitoring::rule::ScopeSelector` (`AUTH-001`/`ENR-001` both
//!   `blocked`).

pub mod engine;
pub mod error;
pub mod evidence;
pub mod ids;
pub mod incident;
pub mod store;
pub mod work_intent;

mod validate;

pub use engine::{IncidentEngine, IngestResult};
pub use error::IncidentError;
pub use incident::{Incident, IncidentState, IncidentTransition};
pub use store::IncidentStore;
