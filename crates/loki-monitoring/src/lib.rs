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
//!
//! # What this crate deliberately is not
//!
//! - **Not a monitoring-rule engine.** Rule evaluation, windows, debounce/
//!   cooldown, and missing-data semantics are `MON-002`'s scope
//!   (`Docs/graph/tasks/MON-001.md`'s objective names `MON-002` as the next
//!   consumer of this crate's subscription mechanism).
//! - **Not an IPC surface.** No transport/network code; a later node wires
//!   this crate's `Pipeline` into `RUN-002`'s IPC layer.
//! - **Not a connector.** This crate ingests connector-neutral shapes; it
//!   contains no device-family-specific logic (`AGENTS.md`'s
//!   device-family-neutral rule).
//! - **Not a `crates/loki-storage` repository.** See [`store`]'s module doc
//!   for the ownership-boundary reasoning.

pub mod buffer;
pub mod error;
pub mod freshness;
pub mod gap;
pub mod pipeline;
pub mod sample;
pub mod store;
pub mod subscription;

mod validate;

pub use error::MonitoringError;
pub use pipeline::{LogIngestReport, Pipeline, SampleIngestReport};
