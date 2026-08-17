//! Per-invocation operation shapes — `Docs/CONNECTOR_SPEC.md` §4 "Connector
//! boundary" and §8 "Device operation contract".
//!
//! Two deliberate scope boundaries, stated up front because they are easy to
//! miss reading the individual types below:
//!
//! - **No generic transport handle.** §4 says a connector receives "a
//!   least-privilege transport handle or approved tool adapter", but
//!   `Docs/CONNECTOR_SPEC.md` never standardizes what a transport handle
//!   *is* — a serial connector's handle looks nothing like a network
//!   connector's. `AGENTS.md`'s device-family-neutral rule ("Never place
//!   ESP32-specific assumptions in shared domain or transport layers", "Keep
//!   device-family logic behind the connector contract") means this SDK
//!   must not invent one universal transport type either: doing so would
//!   bias every future connector toward whatever shape this crate guessed
//!   first. Each connector implementation owns its own transport wiring
//!   privately (e.g. as fields on its own struct, injected at construction),
//!   not through [`OperationContext`].
//! - **No identity-evidence modeling.** §5's identity-evidence fields
//!   (method, confidence, duplicate reconciliation) are discovery/identify
//!   concerns — `CON-002` scope, explicitly forbidden to this node's
//!   packet ("no discovery/registry logic (CON-002)"). [`TargetIdentity`]
//!   here is intentionally minimal: enough to name a stable target, not to
//!   model how that identity was established.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use crate::error::ValidationError;
use crate::ids::{CapabilityId, CorrelationId, DeviceId, IdempotencyKey, OperationId};

/// Which stage of the lifecycle an outcome/event belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationStage {
    Preflight,
    Execute,
    Verify,
    Recover,
}

impl fmt::Display for OperationStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            OperationStage::Preflight => "preflight",
            OperationStage::Execute => "execute",
            OperationStage::Verify => "verify",
            OperationStage::Recover => "recover",
        })
    }
}

/// A cooperative stop signal. `Docs/CONNECTOR_SPEC.md` §8 requires every
/// operation to declare "safe cancellation points" — cancellation in this
/// contract is something a connector *checks for and honors at a point it
/// has itself vetted as safe*, not something an external caller can use to
/// preemptively abort a connector mid-stage. This also covers §4's "must
/// not ... continue a device-changing stage after losing the owning job
/// lease": the runtime signals lease loss the same way it signals a user
/// cancellation request — both mean "stop at your next safe point" from the
/// connector's point of view.
///
/// Deliberately not `tokio_util::sync::CancellationToken`: that would add a
/// dependency for a handful of lines this crate can implement directly with
/// `std::sync::atomic` (`AGENTS.md` rule 11 — no dependency merely for
/// convenience).
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Requests cancellation. Idempotent.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// A bounded diagnostic/progress event, §4 "stage progress and bounded
/// diagnostics" (something a connector "may return") and §8 "progress
/// semantics".
#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticEvent {
    pub stage: OperationStage,
    pub message: String,
    pub observed_at: SystemTime,
}

/// A bounded sink a connector can emit [`DiagnosticEvent`]s to. "Bounded" is
/// enforced by whoever constructs the sink (the runtime, ultimately) — this
/// crate only provides the emission mechanism, not a queue/backpressure
/// policy, which is out of scope for a trait-definition crate with no
/// storage/IPC surface.
#[derive(Clone)]
pub struct DiagnosticsSink(Arc<dyn Fn(DiagnosticEvent) + Send + Sync>);

impl DiagnosticsSink {
    pub fn new(sink: impl Fn(DiagnosticEvent) + Send + Sync + 'static) -> Self {
        Self(Arc::new(sink))
    }

    /// A sink that discards every event — for tests and callers that do not
    /// need diagnostics.
    pub fn noop() -> Self {
        Self::new(|_event| {})
    }

    pub fn emit(&self, event: DiagnosticEvent) {
        (self.0)(event);
    }
}

impl fmt::Debug for DiagnosticsSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DiagnosticsSink(..)")
    }
}

impl Default for DiagnosticsSink {
    fn default() -> Self {
        Self::noop()
    }
}

/// A stable device target, §8 "stable target identity fields". See this
/// module's doc comment for why identity *evidence* is not modeled here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TargetIdentity {
    pub device_id: DeviceId,
}

impl TargetIdentity {
    pub fn new(device_id: DeviceId) -> Self {
        Self { device_id }
    }
}

/// Per-invocation execution context passed to every [`crate::Connector`]
/// lifecycle method — §4 "stable job context, deadline/cancellation,
/// bounded diagnostic sink".
#[derive(Debug, Clone)]
pub struct OperationContext {
    pub correlation_id: CorrelationId,
    /// `Docs/CONNECTOR_SPEC.md` does not mandate a monotonic clock for
    /// deadlines, and `AGENTS.md` rule 5 asks for UTC timestamps throughout;
    /// this crate uses `SystemTime` (wall clock) uniformly for every
    /// timestamp it carries — deadlines included — rather than mixing it
    /// with `Instant` (monotonic, not directly comparable to `SystemTime`
    /// without a conversion this crate has no documented need for at this
    /// scale). A future scheduler node may layer `Instant`-based tracking
    /// internally if clock-adjustment resilience becomes a proven need.
    pub deadline: Option<SystemTime>,
    pub cancellation: CancellationToken,
    pub diagnostics: DiagnosticsSink,
}

impl OperationContext {
    pub fn new(correlation_id: CorrelationId) -> Self {
        Self {
            correlation_id,
            deadline: None,
            cancellation: CancellationToken::new(),
            diagnostics: DiagnosticsSink::noop(),
        }
    }

    #[must_use]
    pub fn with_deadline(mut self, deadline: SystemTime) -> Self {
        self.deadline = Some(deadline);
        self
    }

    #[must_use]
    pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    #[must_use]
    pub fn with_diagnostics(mut self, diagnostics: DiagnosticsSink) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    /// True once `deadline` has already passed, evaluated against the
    /// current wall-clock time. Used by [`crate::lifecycle::run_operation`]
    /// to refuse to *start* a new stage once the deadline has passed —
    /// distinct from a connector's own in-progress cooperative check (see
    /// [`CancellationToken`]'s doc comment on why this crate does not
    /// preemptively abort an in-flight connector call).
    pub fn deadline_passed(&self) -> bool {
        match self.deadline {
            Some(deadline) => SystemTime::now() >= deadline,
            None => false,
        }
    }
}

/// The approved, already-validated operation request a connector receives
/// — §4 "validated device/operation/artifact inputs; approved capability
/// and operation plan". `canonical_inputs` is carried as an opaque,
/// already-validated byte payload: each capability's own `schema_ref`/
/// `schema_version` (see [`crate::capability::CapabilityDescriptor`]) names
/// what that payload means; validating *against* that schema is the
/// runtime's job before this type is ever constructed (§4: connector
/// receives already-"validated ... inputs"), not this SDK's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRequest {
    pub operation_id: OperationId,
    pub capability_id: CapabilityId,
    pub target: TargetIdentity,
    pub canonical_inputs: Vec<u8>,
    pub idempotency_key: IdempotencyKey,
    /// §8 "exclusive lease mode".
    pub exclusive_lease: bool,
    /// §8's declared "expected downtime"/interruption for this specific
    /// invocation, mirroring
    /// [`crate::capability::CapabilityDescriptor::expected_interruption`]'s
    /// concept at the per-request level.
    pub expected_interruption: bool,
}

impl OperationRequest {
    pub fn new(
        operation_id: OperationId,
        capability_id: CapabilityId,
        target: TargetIdentity,
        idempotency_key: IdempotencyKey,
    ) -> Result<Self, ValidationError> {
        Ok(Self {
            operation_id,
            capability_id,
            target,
            canonical_inputs: Vec::new(),
            idempotency_key,
            exclusive_lease: false,
            expected_interruption: false,
        })
    }

    #[must_use]
    pub fn with_canonical_inputs(mut self, canonical_inputs: Vec<u8>) -> Self {
        self.canonical_inputs = canonical_inputs;
        self
    }

    #[must_use]
    pub fn with_exclusive_lease(mut self, exclusive_lease: bool) -> Self {
        self.exclusive_lease = exclusive_lease;
        self
    }

    #[must_use]
    pub fn with_expected_interruption(mut self, expected_interruption: bool) -> Self {
        self.expected_interruption = expected_interruption;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn cancellation_token_starts_uncancelled_and_is_shared_across_clones() {
        let token = CancellationToken::new();
        let clone = token.clone();
        assert!(!token.is_cancelled());
        clone.cancel();
        assert!(
            token.is_cancelled(),
            "clones share the same underlying flag"
        );
    }

    #[test]
    fn diagnostics_sink_invokes_the_provided_callback() {
        use std::sync::atomic::AtomicUsize;
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = Arc::clone(&count);
        let sink = DiagnosticsSink::new(move |_event| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });
        sink.emit(DiagnosticEvent {
            stage: OperationStage::Execute,
            message: "halfway".to_string(),
            observed_at: SystemTime::now(),
        });
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn noop_sink_does_not_panic() {
        let sink = DiagnosticsSink::noop();
        sink.emit(DiagnosticEvent {
            stage: OperationStage::Verify,
            message: "ok".to_string(),
            observed_at: SystemTime::now(),
        });
    }

    #[test]
    fn deadline_passed_is_false_with_no_deadline() {
        let ctx = OperationContext::new(CorrelationId::new("corr-1").unwrap());
        assert!(!ctx.deadline_passed());
    }

    #[test]
    fn deadline_passed_reflects_real_wall_clock_comparison() {
        let past = OperationContext::new(CorrelationId::new("corr-1").unwrap())
            .with_deadline(SystemTime::now() - Duration::from_millis(1));
        assert!(past.deadline_passed());

        let future = OperationContext::new(CorrelationId::new("corr-2").unwrap())
            .with_deadline(SystemTime::now() + Duration::from_secs(60));
        assert!(!future.deadline_passed());
    }

    #[test]
    fn operation_request_builders_set_fields() {
        let req = OperationRequest::new(
            OperationId::new("op-1").unwrap(),
            CapabilityId::new("restart").unwrap(),
            TargetIdentity::new(DeviceId::new("dev-1").unwrap()),
            IdempotencyKey::new("idem-1").unwrap(),
        )
        .unwrap()
        .with_canonical_inputs(vec![1, 2, 3])
        .with_exclusive_lease(true);

        assert_eq!(req.canonical_inputs, vec![1, 2, 3]);
        assert!(req.exclusive_lease);
        assert!(!req.expected_interruption);
    }
}
