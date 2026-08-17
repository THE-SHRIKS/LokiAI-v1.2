//! `loki-connectors-sdk` — the device-neutral connector trait and
//! supporting contracts (`CON-001`).
//!
//! Per `Docs/graph/tasks/CON-001.md`: manifest shape, capability
//! declaration, health-check interface, operation lifecycle
//! (preflight/execute/verify), and recovery hooks — all per
//! `Docs/CONNECTOR_SPEC.md`. This crate defines the *contract*; it contains
//! no concrete device connector, no discovery/registry logic (`CON-002`),
//! and no physical device I/O of any kind. The ESP32-S3 reference connector
//! (`CON-003`) is blocked on hardware confirmation (`OQ-001`) and, per
//! `AGENTS.md`'s device-family-neutral rule, nothing in this crate is
//! shaped around ESP32-only concepts.
//!
//! # Module map
//!
//! | Module | `Docs/CONNECTOR_SPEC.md` section | Contents |
//! |---|---|---|
//! | [`vocabulary`] | §2, §6, §7 (+ `Docs/SPEC.md` §4.4) | Closed enums: support tier, capability risk, health status, device lifecycle state |
//! | [`ids`] | §3, §4, §6, §8 | Validated identifier newtypes and the open capability-id slug |
//! | [`capability`] | §6 | [`capability::CapabilityDescriptor`] |
//! | [`manifest`] | §3 | [`manifest::ConnectorManifest`] and its constituent types |
//! | [`health`] | §7 | [`health::HealthCheckEntry`], [`health::HealthReport`] |
//! | [`operation`] | §4, §8 | Per-invocation request/context types, cancellation, diagnostics |
//! | [`outcome`] | §8, §9 | Explicit stage/lifecycle outcomes — see [`outcome`]'s doc comment |
//! | [`connector`] | §4 | The [`Connector`] trait itself |
//! | [`lifecycle`] | §8, §9 | [`lifecycle::run_operation`], the canonical stage sequencing |
//! | [`conformance`] | §11 | The reusable conformance harness (Deliverable 5) |
//! | [`error`] | §4, §8, §11 | [`error::ValidationError`], [`error::ConnectorError`] |
//!
//! `MockConnector`, the in-memory fake proving this trait is actually
//! implementable and testable (Deliverable 4), is intentionally **not**
//! part of this crate's public library surface — per the task packet, it is
//! "used only by this crate's own tests" and lives in
//! `tests/support/mod.rs`.

pub mod capability;
pub mod conformance;
pub mod connector;
pub mod error;
pub mod health;
pub mod ids;
pub mod lifecycle;
pub mod manifest;
pub mod operation;
pub mod outcome;
pub mod vocabulary;

mod validate;

pub use connector::Connector;
pub use lifecycle::run_operation;
