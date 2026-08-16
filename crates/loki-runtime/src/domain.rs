//! Landing modules for the bounded contexts `Docs/ARCHITECTURE.md` §7
//! names.
//!
//! `RUN-001` is process skeleton only (see `Docs/graph/tasks/RUN-001.md` §5
//! "Forbidden scope": no IPC, no storage, no connector execution, no
//! device/hardware access). Every submodule here is therefore an empty,
//! documented landing spot — no types, no logic — so later nodes add real
//! domain code to a named place instead of bolting it onto `app`/`main`.
//! Node IDs are named only where `Docs/graph/tasks/RUN-001.md` itself
//! already names them (`RUN-002`, `DAT-001`, `CON-001`+, `JOB-001`); the
//! rest are left as "a later node" rather than guessing an ID that has not
//! been chartered.

/// Bounded context 1/10: Identity and Workspace — account/workspace/session
/// projections the runtime needs locally.
pub mod identity;

/// Bounded context 2/10: Runtime and Connectivity — runtime registration,
/// desktop/MCP/cloud handshake, and connection health. Likely first touched
/// by `RUN-002` (IPC).
pub mod connectivity;

/// Bounded context 3/10: Device Registry and Connectors — device identity,
/// capability, and connector metadata. Owned by `CON-001` and later
/// connector nodes.
pub mod devices;

/// Bounded context 4/10: Telemetry and Monitoring — device telemetry
/// ingestion and monitoring-rule evaluation.
pub mod telemetry;

/// Bounded context 5/10: Incidents and Evidence — incident detection and
/// evidence capture.
pub mod incidents;

/// Bounded context 6/10: Jobs, Approvals, and Policy — durable job state
/// machines and policy/approval evaluation. Owned by `JOB-001` and later
/// policy nodes.
pub mod jobs;

/// Bounded context 7/10: Artifacts and Deployments — build/artifact
/// validation, staging, and deployment tracking.
pub mod artifacts;

/// Bounded context 8/10: Agent Connections and Runs — MCP client connection
/// and agent run tracking.
pub mod agents;

/// Bounded context 9/10: Activity and Audit — append-only activity/audit
/// event log.
pub mod activity;

/// Bounded context 10/10: Diagnostics and Updates — device-facing
/// diagnostics and update orchestration. Not to be confused with this
/// crate's own process logging, which lives in [`crate::logging`].
pub mod updates_and_diagnostics;
