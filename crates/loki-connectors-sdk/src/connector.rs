//! The device-neutral connector trait, `Docs/CONNECTOR_SPEC.md` §4
//! "Connector boundary" and this node's central deliverable.
//!
//! ## Why `#[async_trait]`
//!
//! Every lifecycle method is `async`. Real connectors do real I/O (serial
//! reads, HTTP calls, spawned toolchain processes) that must not block the
//! runtime's async executor (`Docs/ARCHITECTURE.md` §3.2: "Rust with an
//! async runtime"), so a synchronous trait would force every future
//! connector to either block a worker thread or wrap itself in
//! `spawn_blocking` boilerplate this SDK should not impose.
//!
//! Native `async fn` in traits does not produce a dyn-compatible trait on
//! its own (the anonymous per-call future type cannot be named in a vtable
//! without help), and `Docs/CONNECTOR_SPEC.md` §12 ("Third-party/custom
//! connectors") describes connectors that are not known at compile time —
//! which needs *some* form of dynamic dispatch (`Box<dyn Connector>` /
//! `Arc<dyn Connector>`, or an equivalent boundary for an out-of-process
//! connector host). Getting this wrong now is exactly what this node's stop
//! condition warns against ("do not guess a design later connectors would
//! have to rework") — reworking a trait's method signatures after `CON-002`/
//! `CON-003` have implementations of it would be a breaking change to this
//! crate. `async-trait` (dtolnay, MIT OR Apache-2.0, proc-macro only, no
//! runtime footprint, the de facto standard solution to this exact problem
//! across the Rust ecosystem) is added for this reason — see `Cargo.toml`
//! for the full dependency justification.
//!
//! **Every `impl Connector for ...` must also carry `#[async_trait]`** —
//! this is a macro-desugaring requirement of the `async-trait` crate on both
//! sides (trait definition and impl), not optional boilerplate. See
//! `tests/support/mod.rs`'s `MockConnector` for a worked example.
//!
//! ## What each method is for
//!
//! - [`Connector::manifest`] / [`Connector::capabilities`]: static,
//!   `&self`-only descriptive data (§3, §6) — no I/O.
//! - [`Connector::health_check`]: a lightweight, frequently-pollable health
//!   probe (§7), independent of any specific operation.
//! - [`Connector::preflight`], [`Connector::execute`], [`Connector::verify`]:
//!   the operation lifecycle (§8) — call these directly only if you are
//!   implementing your own orchestration; most callers should use
//!   [`crate::lifecycle::run_operation`], which sequences these three
//!   correctly (short-circuiting `execute` on preflight failure, refusing to
//!   report success without an independent `verify` pass) and is what
//!   [`crate::conformance`]'s suite exercises.
//! - [`Connector::recover`]: §8 "rollback/recovery plan" / §11
//!   "interruption/reconciliation/recovery contract" — invoked by
//!   [`crate::lifecycle::run_operation`] whenever `execute` was attempted but
//!   the operation did not end in verified success.

use async_trait::async_trait;

use crate::capability::CapabilityDescriptor;
use crate::health::HealthReport;
use crate::manifest::ConnectorManifest;
use crate::operation::{OperationContext, OperationRequest};
use crate::outcome::{StageFailure, StageOutcome};

/// The device-neutral connector contract. See the module doc comment above.
///
/// `Send + Sync` so a connector can be held behind `Arc<dyn Connector>` and
/// invoked from any task in a multi-threaded async runtime — the shape a
/// connector registry (`CON-002`+) will need.
#[async_trait]
pub trait Connector: Send + Sync {
    /// The static package manifest, `Docs/CONNECTOR_SPEC.md` §3.
    fn manifest(&self) -> &ConnectorManifest;

    /// Declared capabilities, `Docs/CONNECTOR_SPEC.md` §6. Implementations
    /// typically return `&self.manifest().capabilities`, but the method is
    /// independent of [`Connector::manifest`] so a connector may serve a
    /// filtered/environment-specific subset without mutating its own
    /// manifest.
    fn capabilities(&self) -> &[CapabilityDescriptor];

    /// A live health probe, `Docs/CONNECTOR_SPEC.md` §7. Must not fabricate
    /// a status for a metric the connector has no evidence source for — see
    /// `Docs/CONNECTOR_SPEC.md` §7 and [`crate::health::HealthCheckEntry::new`],
    /// which enforces that any non-`Passed` status carries a reason.
    async fn health_check(&mut self, ctx: &OperationContext) -> HealthReport;

    /// Preflight checks, `Docs/CONNECTOR_SPEC.md` §8. Must not mutate device
    /// state — preflight happens strictly before "execution stages" in the
    /// declared operation contract.
    async fn preflight(&mut self, op: &OperationRequest, ctx: &OperationContext) -> StageOutcome;

    /// Executes the operation. `Docs/CONNECTOR_SPEC.md` §9: "Treat exit
    /// success as stage completion only; connector verification still
    /// runs." — a [`StageOutcome::Success`] returned here describes only
    /// that this stage completed as far as the connector can tell; it is
    /// never sufficient on its own to consider the operation successful
    /// (see [`crate::outcome::LifecycleOutcome::succeeded`]).
    async fn execute(&mut self, op: &OperationRequest, ctx: &OperationContext) -> StageOutcome;

    /// Independently confirms the target actually reached the declared
    /// state, `Docs/CONNECTOR_SPEC.md` §8 "verification plan" / §11 "target
    /// re-identification before mutation". This is the only stage whose
    /// success actually completes the operation.
    async fn verify(&mut self, op: &OperationRequest, ctx: &OperationContext) -> StageOutcome;

    /// Attempts recovery/rollback evidence capture after `execute` was
    /// attempted but the operation did not end in verified success,
    /// `Docs/CONNECTOR_SPEC.md` §8 "rollback/recovery plan" and §11
    /// "interruption/reconciliation/recovery contract". A connector with no
    /// real recovery mechanism for this capability must say so honestly —
    /// e.g. by returning `StageOutcome::Failure` describing that recovery is
    /// unsupported — rather than fabricating a success it did not verify
    /// (`AGENTS.md` rule 1).
    async fn recover(
        &mut self,
        op: &OperationRequest,
        ctx: &OperationContext,
        failure: &StageFailure,
    ) -> StageOutcome;
}
