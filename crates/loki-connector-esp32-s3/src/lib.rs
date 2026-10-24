//! `loki-connector-esp32-s3` — the first concrete
//! `loki_connectors_sdk::Connector` implementation, for the confirmed
//! ESP32-S3 profile (`Docs/CONTEXT.md` OQ-001: QFN56 rev v0.2, 8MB embedded
//! PSRAM AP_3v3, 16MB external quad flash, 3.3V, USB-Serial/JTAG native).
//!
//! Per `Docs/graph/tasks/CON-003-R1.md`: this crate implements manifest,
//! capability declarations, health-check logic, and the
//! identify/preflight/execute/verify/recover STRUCTURE for this device
//! family, fully testable via an injected fake [`transport::Esp32Transport`]
//! — **zero real device I/O is reachable from this crate's own production or
//! test code**: no serial-port crate dependency, no `std::process::Command`,
//! no open port/USB handle anywhere in `src/`. A real transport
//! implementation is explicitly out of scope for this node — see
//! `transport`'s module doc for the injection boundary a later,
//! separately-gated node (`CON-003-PHYS-001`) implements against.
//!
//! `SupportTier::Custom`, never `Certified` — see `manifest`'s module doc
//! for why, and the worker handoff for the explicit acceptance-criteria
//! record.
//!
//! # Module map
//!
//! | Module | Contents |
//! |---|---|
//! | [`transport`] | [`transport::Esp32Transport`] — the injectable ROM/bootloader boundary — and its response/error types |
//! | [`capabilities`] | Builds this connector's six declared `CapabilityDescriptor`s |
//! | [`manifest`] | Builds this connector's `ConnectorManifest` |
//! | [`connector`] | [`connector::Esp32S3Connector`] — the `Connector` implementation itself |
//! | [`clock`] | UTC ISO 8601 timestamp formatting (no date/time crate dependency — see its own doc) |
//!
//! The fake `Esp32Transport` test double (Deliverable 3) is intentionally
//! **not** part of this crate's public library surface — mirroring
//! `loki-connectors-sdk`'s own `MockConnector` pattern exactly, it lives in
//! `tests/support/mod.rs`, "used only by this crate's own tests."

pub mod capabilities;
pub mod clock;
pub mod connector;
pub mod manifest;
pub mod transport;

pub use connector::Esp32S3Connector;
pub use transport::Esp32Transport;
