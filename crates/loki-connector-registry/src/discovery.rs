//! Discovery/identity shapes — this node's Deliverable 2: "a discovery cycle
//! using CON-001's conformance mock — real discover/identify against the
//! mock, not a hand-rolled fake."
//!
//! ## Why this reuses `run_operation` rather than a new "discover" method
//!
//! `loki_connectors_sdk::Connector` (CON-001) has no `probe`/`discover`/
//! `identify` method of its own — deliberately: its own module docs (see
//! `crates/loki-connectors-sdk/src/operation.rs`) say identity evidence
//! (method, confidence, duplicate reconciliation) is explicitly
//! "`CON-002` scope", and this crate's own task packet forbids touching
//! `loki-connectors-sdk` at all. `Docs/CONNECTOR_SPEC.md` §6 already lists
//! `identify` as a first-class capability id (`ids::KNOWN_CAPABILITY_IDS`
//! includes it), and every capability — `identify` included — is invoked
//! through the exact same preflight -> execute -> verify contract every
//! other capability uses (§8's "Device operation contract" applies
//! uniformly; nothing in §5 carves `identify` out as a separately-shaped
//! call).
//!
//! So "real discover/identify against the mock" here means: this module
//! builds a real `OperationRequest` for the `identify` capability and runs
//! it through `loki_connectors_sdk::run_operation` against the actual
//! registered `Connector` trait object — the same lifecycle runner and the
//! same success rule (`LifecycleOutcome::succeeded()`, true only after an
//! independent `verify` pass) every other operation in this system uses.
//! Nothing about a discovery result is synthesized independently of what
//! the connector's own `preflight`/`execute`/`verify` methods actually
//! returned.
//!
//! ## Why there is no separate "probe" step in this node
//!
//! `Docs/CONNECTOR_SPEC.md` §5 describes two steps: `probe` (observe
//! candidate *transports*) and `identify` (bounded interrogation of one
//! candidate). Transport probing is explicitly out of `loki-connectors-sdk`'s
//! scope (see `operation.rs`'s "No generic transport handle" note: "Each
//! connector implementation owns its own transport wiring privately"), and
//! `CON-003` (the first connector with a real transport to probe) is
//! blocked on `OQ-001`. This node has only `CON-001`'s conformance mock to
//! discover against — an in-process trait object with no transport of its
//! own — so a "discovery cycle" here is, honestly: for every *already
//! registered* connector instance, invoke its `identify` capability and
//! record what it actually reports. A future connector with a real
//! transport (`CON-003`+) would still register itself with this registry
//! the same way; transport-level probing to *find* candidates worth
//! registering is a different, later concern this node does not invent a
//! design for.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use loki_connectors_sdk::ids::{CapabilityId, ConnectorId, DeviceId, IdempotencyKey, OperationId};
use loki_connectors_sdk::operation::{OperationRequest, TargetIdentity};
use loki_connectors_sdk::outcome::{LifecycleOutcome, StageOutcome};
use semver::Version;

/// A monotonically increasing counter used only to make each discovery
/// cycle's generated `OperationId`/`IdempotencyKey` values unique within
/// this process — not a stable identifier of anything device-related.
/// `AGENTS.md` rule 11: a plain `AtomicU64` is sufficient here, so no
/// ID-generation crate (e.g. `uuid`) is added merely for this.
static DISCOVERY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn next_sequence() -> u64 {
    DISCOVERY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

/// An as-yet-unconfirmed candidate this registry is asking a connector to
/// identify. `Docs/CONNECTOR_SPEC.md` §5: "COM port, IP address, BLE
/// address, or probe index is not stable identity" — a `CandidateId` is
/// exactly that kind of provisional handle, never treated as stable
/// identity by this module. Only [`DeviceIdentity::device_id`] (produced
/// after a *successful, verified* identify operation) is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CandidateId(String);

impl CandidateId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// This milestone's only source of candidates: each already-registered
    /// connector instance is itself one candidate to identify (see this
    /// module's doc comment). A future transport-probing node may replace
    /// this with real probe-derived candidates without changing
    /// [`run_identify`]'s contract.
    #[must_use]
    pub(crate) fn from_connector_id(connector_id: &ConnectorId) -> Self {
        Self(connector_id.as_str().to_string())
    }
}

/// Identity evidence for a candidate that was successfully identified —
/// `Docs/CONNECTOR_SPEC.md` §5: "Identity evidence records connector/
/// version, method, time, and confidence." `confidence` is free text
/// (bounded only by carrying whatever the connector's own verify-stage
/// evidence summary said) rather than an invented closed enum: the spec
/// requires the concept to exist but never enumerates fixed confidence
/// levels, unlike, say, `Docs/CONNECTOR_SPEC.md` §7's closed health-status
/// list — the same "no divergent copy of an undefined vocabulary"
/// discipline `loki-connectors-sdk` already applies to `DeviceFamilyMatch::match_rule`.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceIdentity {
    pub device_id: DeviceId,
    pub connector_id: ConnectorId,
    pub connector_version: Version,
    pub method: String,
    pub confidence: String,
    pub established_at: SystemTime,
    pub evidence_ref: Option<String>,
}

/// One candidate's outcome for one discovery cycle — always produced,
/// whether or not identification actually succeeded, so a caller can see
/// exactly what happened to *every* candidate this cycle touched, not only
/// the ones that resolved to a stable identity.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveryOutcome {
    pub connector_id: ConnectorId,
    pub candidate: CandidateId,
    /// The full lifecycle detail (`preflight`/`execute`/`verify`/`recovery`)
    /// from the real `identify` operation this cycle ran — never discarded,
    /// so a caller can see *why* identification failed, not just that it did.
    pub lifecycle: LifecycleOutcome,
    /// `Some` only when `lifecycle.succeeded()` is true. Never fabricated
    /// from a partial/failed lifecycle (`AGENTS.md` rule 1).
    pub identity: Option<DeviceIdentity>,
    pub observed_at: SystemTime,
}

/// Builds the `identify`-capability [`OperationRequest`] this module sends
/// through `run_operation` for one candidate.
pub(crate) fn build_identify_request(candidate: &CandidateId) -> OperationRequest {
    let sequence = next_sequence();
    OperationRequest::new(
        OperationId::new(format!("discover-{}-{sequence}", candidate.as_str()))
            .expect("candidate-derived operation id stays within the SDK's length bound"),
        CapabilityId::new("identify").expect("'identify' is a valid capability-id slug"),
        TargetIdentity::new(
            DeviceId::new(candidate.as_str())
                .expect("candidate id reused as a provisional DeviceId stays within bound"),
        ),
        IdempotencyKey::new(format!("discover-{}-{sequence}", candidate.as_str()))
            .expect("candidate-derived idempotency key stays within the SDK's length bound"),
    )
    .expect("well-formed identify request")
}

/// Turns a completed identify [`LifecycleOutcome`] into a
/// [`DiscoveryOutcome`] — the sole place this crate decides whether a
/// lifecycle's real result counts as a resolved identity. Only
/// [`LifecycleOutcome::succeeded`] (an independently-verified `verify`
/// success) ever produces `Some(DeviceIdentity)`; every other outcome
/// (preflight/execute/verify failure or timeout) is preserved verbatim in
/// `lifecycle` with `identity: None`.
pub(crate) fn interpret_identify_outcome(
    connector_id: ConnectorId,
    connector_version: Version,
    candidate: CandidateId,
    lifecycle: LifecycleOutcome,
    observed_at: SystemTime,
) -> DiscoveryOutcome {
    let identity = if lifecycle.succeeded() {
        match &lifecycle.verify {
            Some(StageOutcome::Success(evidence)) => Some(DeviceIdentity {
                device_id: DeviceId::new(candidate.as_str())
                    .expect("already validated when the request was built"),
                connector_id: connector_id.clone(),
                connector_version,
                method: "identify_capability_lifecycle".to_string(),
                confidence: evidence.summary.clone(),
                established_at: evidence.observed_at,
                evidence_ref: evidence.evidence_ref.clone(),
            }),
            // `succeeded()` guarantees `verify` is `Some(Success(_))`; any
            // other shape here would mean this crate's own invariant broke,
            // not that the connector misbehaved.
            _ => unreachable!(
                "LifecycleOutcome::succeeded() guarantees Some(StageOutcome::Success(_)) in verify"
            ),
        }
    } else {
        None
    };

    DiscoveryOutcome {
        connector_id,
        candidate,
        lifecycle,
        identity,
        observed_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_connectors_sdk::error::ConnectorError;
    use loki_connectors_sdk::operation::OperationStage;
    use loki_connectors_sdk::outcome::StageEvidence;

    fn evidence(summary: &str) -> StageEvidence {
        StageEvidence::new(summary, SystemTime::now()).unwrap()
    }

    #[test]
    fn candidate_from_connector_id_round_trips_the_string() {
        let id = ConnectorId::new("loki-mock-connector").unwrap();
        assert_eq!(
            CandidateId::from_connector_id(&id).as_str(),
            "loki-mock-connector"
        );
    }

    #[test]
    fn two_requests_for_the_same_candidate_get_distinct_operation_ids() {
        let candidate = CandidateId::new("cand-1");
        let a = build_identify_request(&candidate);
        let b = build_identify_request(&candidate);
        assert_ne!(a.operation_id, b.operation_id);
        assert_ne!(a.idempotency_key, b.idempotency_key);
    }

    #[test]
    fn successful_lifecycle_yields_identity_evidence_from_real_verify_output() {
        let lifecycle = LifecycleOutcome {
            stage_reached: OperationStage::Verify,
            preflight: StageOutcome::Success(evidence("preflight ok")),
            execute: Some(StageOutcome::Success(evidence("execute ok"))),
            verify: Some(StageOutcome::Success(
                evidence("chip fingerprint matched").with_evidence_ref("evidence://chip-1"),
            )),
            recovery: None,
        };
        let outcome = interpret_identify_outcome(
            ConnectorId::new("loki-mock-connector").unwrap(),
            Version::parse("1.0.0").unwrap(),
            CandidateId::new("cand-1"),
            lifecycle,
            SystemTime::now(),
        );

        let identity = outcome
            .identity
            .expect("succeeded lifecycle yields identity");
        assert_eq!(identity.device_id.as_str(), "cand-1");
        assert_eq!(identity.confidence, "chip fingerprint matched");
        assert_eq!(identity.evidence_ref.as_deref(), Some("evidence://chip-1"));
    }

    #[test]
    fn failed_verify_after_apparent_execute_success_never_fabricates_identity() {
        let lifecycle = LifecycleOutcome {
            stage_reached: OperationStage::Verify,
            preflight: StageOutcome::Success(evidence("preflight ok")),
            execute: Some(StageOutcome::Success(evidence("execute ok (exit 0)"))),
            verify: Some(StageOutcome::Failure(
                ConnectorError::new("identity_mismatch", "reported chip id changed mid-cycle")
                    .unwrap(),
            )),
            recovery: Some(StageOutcome::Failure(
                ConnectorError::new("no_recovery", "identify has no recovery path").unwrap(),
            )),
        };
        let outcome = interpret_identify_outcome(
            ConnectorId::new("loki-mock-connector").unwrap(),
            Version::parse("1.0.0").unwrap(),
            CandidateId::new("cand-1"),
            lifecycle.clone(),
            SystemTime::now(),
        );

        assert!(outcome.identity.is_none());
        assert_eq!(outcome.lifecycle, lifecycle);
    }

    #[test]
    fn preflight_failure_never_fabricates_identity() {
        let lifecycle = LifecycleOutcome {
            stage_reached: OperationStage::Preflight,
            preflight: StageOutcome::Failure(
                ConnectorError::new("transport_busy", "port in use").unwrap(),
            ),
            execute: None,
            verify: None,
            recovery: None,
        };
        let outcome = interpret_identify_outcome(
            ConnectorId::new("loki-mock-connector").unwrap(),
            Version::parse("1.0.0").unwrap(),
            CandidateId::new("cand-1"),
            lifecycle,
            SystemTime::now(),
        );
        assert!(outcome.identity.is_none());
    }
}
