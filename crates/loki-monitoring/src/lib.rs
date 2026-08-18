//! `loki-monitoring` — bounded telemetry/log ingestion (`MON-001`).
//!
//! Per `Docs/graph/tasks/MON-001.md`: quality/freshness tracking, buffering
//! with an explicit, tested upper bound, storage, and a subscription
//! mechanism later nodes (`MON-002`, UI) will consume. Device-neutral — no
//! ESP32-specific assumptions; this crate ingests from
//! `loki-connectors-sdk`'s connector-neutral vocabulary (`DeviceId`,
//! `CorrelationId`, the health-check shape), not from any specific
//! connector.
//!
//! # Module map
//!
//! | Module | Deliverable | Contents |
//! |---|---|---|
//! | [`sample`] | — | `Docs/SPEC.md` §3.5 telemetry sample / log record shapes |
//! | [`buffer`] | 1. Bounded ingestion buffer | [`buffer::BoundedBuffer`], drop-oldest policy |
//! | [`freshness`] | 2. Freshness/quality tracking | [`freshness::FreshnessTracker`] |
//! | [`gap`] | 5. Honest gap/loss reporting | [`gap::SequenceGapDetector`] |
//! | [`subscription`] | 3. Backpressure | [`subscription::EventBus`], bounded multi-subscriber fan-out |
//! | [`store`] | 4. Restart/durability | [`store::SqliteTelemetryStore`] — see its doc comment for why this is a self-contained store, not a `crates/loki-storage` repository |
//! | [`pipeline`] | Ties the above together | [`pipeline::Pipeline`] |
//! | [`error`] | — | [`error::MonitoringError`] |
//! | [`rule`] | `MON-002`: rule model | [`rule::MonitoringRule`] — `Docs/SPEC.md` §3.6 field list |
//! | [`evaluate`] | `MON-002`: deterministic evaluation | [`evaluate::RuleEvaluator`] — window/condition/debounce/cooldown/missing-data |
//! | [`rule_store`] | `MON-002`: rule/evaluation durability | [`rule_store::RuleStore`] — versioned rule definitions and evaluation history |
//!
//! # What this crate deliberately is not
//!
//! - **Not an IPC surface.** No transport/network code; a later node wires
//!   this crate's `Pipeline`/`RuleEvaluator` into `RUN-002`'s IPC layer.
//! - **Not a connector.** This crate ingests connector-neutral shapes; it
//!   contains no device-family-specific logic (`AGENTS.md`'s
//!   device-family-neutral rule).
//! - **Not a `crates/loki-storage` repository.** See [`store`]'s module doc
//!   for the ownership-boundary reasoning (also true of [`rule_store`]).
//! - **Not an incident engine.** A [`evaluate::EvaluationOutcome::Fired`] or
//!   [`evaluate::EvaluationOutcome::MissingDataAlert`] is this crate's
//!   honest, deterministic answer to "did this rule's condition actually
//!   fire" — turning that into a durable, deduplicated, workflow-tracked
//!   incident (`Docs/SPEC.md` §3.7) is `INC-001`'s scope, not this crate's.
//! - **Not a real device/workspace authorization model.** [`rule::ScopeSelector`]
//!   matches already-ingested `DeviceId`s mechanically; it does not resolve
//!   device ownership or workspace membership (`AUTH-001`/`ENR-001`, both
//!   `blocked`). See [`rule::ScopeSelector`]'s doc comment.

pub mod buffer;
pub mod error;
pub mod evaluate;
pub mod freshness;
pub mod gap;
pub mod pipeline;
pub mod rule;
pub mod rule_store;
pub mod sample;
pub mod store;
pub mod subscription;

mod validate;

pub use error::MonitoringError;
pub use evaluate::{EvaluationOutcome, RuleEvaluator};
pub use pipeline::{LogIngestReport, Pipeline, SampleIngestReport};
pub use rule::MonitoringRule;
pub use rule_store::RuleStore;
