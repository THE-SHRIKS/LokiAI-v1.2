//! `loki-connector-registry` — connector registry plus discovery/identity/
//! support/capability/health orchestration (`CON-002`).
//!
//! Per `Docs/graph/tasks/CON-002.md`: this crate registers/deregisters
//! `loki_connectors_sdk::Connector` instances, runs discovery (identify)
//! cycles and health-check cycles against them, aggregates health status
//! honestly (an unhealthy connector is never masked), and projects each
//! registered connector's declared support-tier/capability/risk metadata
//! from its manifest. It is registry/orchestration logic only — it
//! implements no concrete device connector of its own (the ESP32-S3
//! reference connector is `CON-003`, blocked on `OQ-001`) and performs no
//! physical device I/O directly; every device-facing call happens through an
//! already-registered `Connector` trait object.
//!
//! # Module map
//!
//! | Module | Contents |
//! |---|---|
//! | [`registry`] | [`registry::ConnectorRegistry`] — register/deregister/list/get, discovery-cycle and health-cycle entry points (Deliverables 1-3) |
//! | [`discovery`] | Discovery/identity shapes and the real `identify`-capability lifecycle this crate runs (Deliverable 2) — see its module doc for why this reuses `run_operation` rather than a new SDK method |
//! | [`health`] | [`health::AggregatedHealthStatus`] — this crate's own report -> connector-level status folding rule (Deliverable 3) |
//! | [`projection`] | [`projection::RegisteredConnectorSummary`] — support-tier/capability/risk projection from a connector's manifest (Deliverable 4) |
//! | [`scheduler`] | [`scheduler::PeriodicHealthChecker`] — the periodic half of "periodic health-check invocation" |
//! | [`error`] | [`error::RegistryError`] — registry bookkeeping failures (double registration, unknown id) |
//!
//! ## Why no real transport / `loki-ipc` dependency
//!
//! The task packet names `RUN-002`'s IPC as available "for whatever
//! transport discovery needs". This node's actual scope — discovery and
//! health orchestration against `CON-001`'s conformance mock (no real
//! device connector exists yet; `CON-003` is blocked on `OQ-001`) — never
//! needs to open a transport of its own: every device-facing call this crate
//! makes goes through an already-registered, already-transport-wired
//! `Connector` trait object (`loki-connectors-sdk/src/operation.rs`'s own
//! module doc: "Each connector implementation owns its own transport wiring
//! privately"). `loki-ipc` is the desktop-shell <-> Rust-runtime IPC
//! transport (`RUN-002`'s own scope, Windows-only named pipes) — a
//! different boundary than "this registry talks to a device", and nothing
//! in this node's deliverables crosses that boundary. Adding it as an
//! unused dependency would violate `AGENTS.md` rule 11 ("no dependency
//! merely for convenience"). See `Docs/graph/handoffs/CON-002-1.md` for the
//! full reasoning and the explicit call for orchestrator review.

pub mod discovery;
pub mod error;
pub mod health;
pub mod projection;
pub mod registry;
pub mod scheduler;

pub use error::RegistryError;
pub use registry::ConnectorRegistry;

/// This crate's own `Connector` test double
/// (`tests/support/mod.rs`'s `TestConnector`), included here only under
/// `cfg(test)` so both this crate's `src/`-level unit tests
/// (`registry.rs`'s, `scheduler.rs`'s) and its `tests/*.rs` integration
/// tests exercise the exact same implementation — never two subtly
/// different doubles. Never compiled into a non-test build; not part of
/// this crate's public library surface, matching the precedent
/// `crates/loki-connectors-sdk`'s own `MockConnector` established.
#[cfg(test)]
#[path = "../tests/support/mod.rs"]
pub(crate) mod test_support;
