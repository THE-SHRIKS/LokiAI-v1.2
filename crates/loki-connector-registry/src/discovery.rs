//! Discovery/identity shapes — `CON-002-R1`'s real candidate ingestion,
//! identify-by-candidate, and per-cycle interpretation of a connector's
//! `identify` result into a real
//! `loki_connectors_sdk::identity::IdentityProbeResult`.
//!
//! Registry-level stable-identity *reconciliation* (merging duplicate
//! candidates, recognizing a reconnect, detecting cross-candidate digest
//! conflicts, and clean-disconnect bookkeeping) lives in [`crate::registry`]
//! because it is persisted, stateful, and shared across cycles — this module
//! only does the pure, per-candidate work: build the `identify` request,
//! run it, and turn one completed lifecycle into one [`CandidateProbe`].
//!
//! ## Why this reuses `run_operation` rather than a new "discover" method
//!
//! `loki_connectors_sdk::Connector` (`CON-001`) has no `probe`/`discover`/
//! `identify` method of its own — deliberately: `operation.rs`'s module doc
//! says identity evidence (method, confidence, duplicate reconciliation) is
//! `CON-002` scope. `Docs/CONNECTOR_SPEC.md` §6 already lists `identify` as
//! a first-class capability id, and every capability — `identify` included —
//! is invoked through the same preflight -> execute -> verify contract every
//! other capability uses. So "identify a candidate" here means: build a real
//! `OperationRequest` for the `identify` capability and run it through
//! `loki_connectors_sdk::run_operation` against the actual registered
//! `Connector` trait object — the same lifecycle runner and the same success
//! rule (`LifecycleOutcome::succeeded()`) every other operation uses.
//!
//! ## Candidate sourcing (Deliverable 1)
//!
//! [`CandidateSource`] is the injectable abstraction this node's packet
//! requires in place of the mock-era "each registered connector instance is
//! itself one candidate" shortcut. It is deliberately *not* a hardware probe
//! implementation — `CON-003` (the first connector with a real transport to
//! probe) is blocked on `OQ-001`, and this crate's packet explicitly forbids
//! inventing one. A caller (test, or a future runtime embedding once a real
//! transport-probing component exists) supplies already-observed
//! [`ObservedCandidate`] values per connector, synchronously — any candidate
//! sourcing that itself needs I/O is the caller's job to have already done
//! before calling into this registry.
//!
//! ## Why [`ObservedCandidate`] carries `candidate_id` alongside the `TransportCandidate`
//!
//! `loki_connectors_sdk::identity::TransportCandidate` (read-only dependency
//! for this crate — see the packet's forbidden scope) exposes no public
//! getter for its private `candidate_id` field; the only public accessors
//! are `serialize_for_transport` (which also emits the *real, sensitive*
//! locator value — this crate has no legitimate reason to read that) and its
//! ordinary derived `Serialize` impl (which would require adding a
//! `serde_json` dependency this crate does not otherwise need, purely to
//! round-trip one already-known field back out of a value the caller itself
//! just built). Since whoever constructs a `TransportCandidate` already
//! calls `TransportCandidate::new(candidate_id, ...)` with that exact
//! string, [`CandidateSource::candidates_for`] asks the caller to hand it
//! back once, paired with the `TransportCandidate`, via [`ObservedCandidate`]
//! — never re-derived by guessing at a private field.
//!
//! ## `IdentityConfidence` handling (Section 9 open question 1 — not silently resolved)
//!
//! No minimum `IdentityConfidence` threshold policy has been decided anywhere
//! in this codebase yet. This module does not invent one: any confidence
//! level a connector reports (`Weak` included) is accepted as sufficient
//! evidence to produce a `PublicDeviceIdentity`, via
//! `IdentityProbeResult::verified`, which itself places no confidence floor
//! on its input. This is an explicit stopgap, not a considered security
//! policy — a future decision node must set a minimum-confidence policy
//! (coupled, per `CON-SDK-IDENTITY-EVIDENCE-001`'s own handoff, to whatever
//! evidence-reporting shape a connector uses) before any consequential
//! (flash/restart/enrollment) capability is gated on identity confidence.
//!
//! ## Untrusted wire-data boundary (Section 9 open question 2 — not silently resolved)
//!
//! This module never calls `IdentityProbeResult::from_json_str` (the one
//! function in `identity.rs` that parses untrusted wire/JSON data) and never
//! constructs an `IdentityProbeResult` from anything other than a real,
//! in-process `LifecycleOutcome` this crate itself obtained by calling
//! `run_operation` against an already-registered `Box<dyn Connector>` trait
//! object. Registered connectors are in-process trait objects with no
//! `loki-ipc`/wire dependency anywhere in this crate's dependency graph, so
//! there is no untrusted-wire boundary for this module to cross at this
//! milestone. This remains an architectural fact about this crate's current
//! shape, not a resolution of the wire-authenticity question itself, which
//! stays open for whatever future component (e.g. an out-of-process
//! connector host, `Docs/CONNECTOR_SPEC.md` §12) actually crosses a real wire
//! boundary for connector identity data.
//!
//! ## Busy/permission/disconnected/unsupported classification convention (Deliverable 4)
//!
//! `Docs/CONNECTOR_SPEC.md` §5 requires `identify` to be able to return "a
//! structured unsupported/permission/busy/error result" but does not
//! standardize the error-code vocabulary a connector uses to say which one —
//! `loki_connectors_sdk::error::ConnectorError.code` is documented as open,
//! shape-validated-only text, not a closed enum. This module therefore
//! defines its own, explicit registry-owned classification convention (see
//! [`DiscoveryFailureClass::classify`]) rather than inventing a silent
//! per-connector guess: a connector that wants a specific class recognized
//! reports one of the exact `code` strings documented on
//! [`DiscoveryFailureClass`]; anything else classifies as
//! [`DiscoveryFailureClass::Generic`]. This convention is this registry
//! crate's own policy, not a `loki-connectors-sdk` contract change.
//!
//! [`DiscoveryFailureClass::CandidateNotAddressable`] is a sixth, distinct
//! case this registry adds on top of the four the packet asked for
//! (`CON-002-R1` security review finding, addressed post-handoff): it is
//! never produced by [`DiscoveryFailureClass::classify`], and never means
//! the connector reported anything — it means this crate itself could not
//! build a well-formed `identify` request for the candidate at all (see
//! [`build_identify_request`]), so the connector was never invoked. A caller
//! must not treat it as Busy/PermissionDenied/Disconnected/Unsupported/
//! Generic.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use loki_connectors_sdk::error::ConnectorError;
use loki_connectors_sdk::identity::{
    DeviceIdentityEvidence, IdentityProbeResult, IdentityResolutionStatus, PublicDeviceIdentity,
    TransportCandidate,
};
use loki_connectors_sdk::ids::{CapabilityId, ConnectorId, DeviceId, IdempotencyKey, OperationId};
use loki_connectors_sdk::operation::{OperationRequest, TargetIdentity};
use loki_connectors_sdk::outcome::{LifecycleOutcome, StageOutcome};

/// A monotonically increasing counter used only to make each discovery
/// cycle's generated `OperationId`/`IdempotencyKey` values unique within
/// this process — not a stable identifier of anything device-related.
/// `AGENTS.md` rule 11: a plain `AtomicU64` is sufficient here, so no
/// ID-generation crate (e.g. `uuid`) is added merely for this.
static DISCOVERY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn next_sequence() -> u64 {
    DISCOVERY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

/// The candidate-sourcing abstraction (Deliverable 1). Implemented for any
/// `Fn(&ConnectorId) -> Vec<ObservedCandidate>` closure via the blanket
/// impl below, so a caller can pass a plain closure (matching this crate's
/// existing `ctx_factory: impl FnMut(&ConnectorId) -> OperationContext`
/// pattern) or a named type when it needs to store/share the source across
/// calls (e.g. behind an `Arc`, for a future scheduler).
/// `Sync` supertrait: `run_discovery_cycle` holds a `&dyn CandidateSource`
/// across `.await` points, and this crate's own concurrency tests
/// (`registry.rs`) spawn discovery cycles onto separate tokio tasks, which
/// requires the future — and therefore anything it borrows across an await
/// — to be `Send`/`Sync`. Every closure used as a `CandidateSource` in this
/// crate only captures `Sync` data (string literals, other closures), so
/// this bound costs real callers nothing.
pub trait CandidateSource: Sync {
    /// Every currently-observed candidate for one connector, or an empty
    /// `Vec` if none are currently observed (e.g. a clean disconnect — see
    /// `crate::registry`'s reconciliation doc for what an empty result
    /// means to a cycle already tracking a live device for this connector).
    fn candidates_for(&self, connector_id: &ConnectorId) -> Vec<ObservedCandidate>;
}

impl<F> CandidateSource for F
where
    F: Fn(&ConnectorId) -> Vec<ObservedCandidate> + Sync,
{
    fn candidates_for(&self, connector_id: &ConnectorId) -> Vec<ObservedCandidate> {
        self(connector_id)
    }
}

/// One candidate a [`CandidateSource`] reports for one connector: the
/// already-validated [`TransportCandidate`] plus the exact ephemeral
/// `candidate_id` string this module uses to address the one `identify`
/// call for it. See this module's doc comment ("Why `ObservedCandidate`
/// carries `candidate_id` alongside the `TransportCandidate`") for why this
/// is not re-derived from the `TransportCandidate` itself.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedCandidate {
    pub candidate_id: String,
    pub candidate: TransportCandidate,
}

impl ObservedCandidate {
    #[must_use]
    pub fn new(candidate_id: impl Into<String>, candidate: TransportCandidate) -> Self {
        Self {
            candidate_id: candidate_id.into(),
            candidate,
        }
    }
}

/// This registry's own busy/permission/disconnect/unsupported/generic
/// classification convention — see this module's doc comment. Never
/// constructed from anything other than a connector-reported
/// [`ConnectorError`] this crate itself observed via a real lifecycle run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryFailureClass {
    /// `code` is one of: `"transport_busy"`, `"device_busy"`, `"port_busy"`.
    Busy,
    /// `code` is one of: `"permission_denied"`, `"access_denied"`.
    PermissionDenied,
    /// `code` is one of: `"device_disconnected"`, `"device_not_found"`,
    /// `"candidate_absent"`.
    Disconnected,
    /// `code` is one of: `"unsupported_candidate"`,
    /// `"unsupported_device_family"`, `"unsupported"` — `AS-004`: the
    /// candidate is not one this connector actually supports.
    Unsupported,
    /// Any other connector-reported failure `code`.
    Generic,
    /// **Not** a connector-reported failure — the connector's `identify`
    /// capability was never invoked for this candidate at all, because this
    /// registry could not build a well-formed `identify` request from it
    /// (see [`build_identify_request`]). This happens only when a
    /// legitimately-sized `candidate_id` (up to `TransportCandidate::new`'s
    /// own 128-byte bound) combined with this registry's
    /// `"discover-{candidate_id}-{sequence}"` operation-id/idempotency-key
    /// wrapper would exceed the SDK's independent 128-byte `OperationId`/
    /// `IdempotencyKey`/`DeviceId` bound. Deliberately distinct from every
    /// other variant (none of which mean "we never asked the connector"),
    /// and deliberately never produced by [`DiscoveryFailureClass::classify`]
    /// (which only ever classifies a real connector-reported
    /// [`ConnectorError`]) — a caller must not read this as the connector
    /// having reported anything.
    CandidateNotAddressable,
}

impl DiscoveryFailureClass {
    #[must_use]
    pub fn classify(error: &ConnectorError) -> Self {
        match error.code.as_str() {
            "transport_busy" | "device_busy" | "port_busy" => Self::Busy,
            "permission_denied" | "access_denied" => Self::PermissionDenied,
            "device_disconnected" | "device_not_found" | "candidate_absent" => Self::Disconnected,
            "unsupported_candidate" | "unsupported_device_family" | "unsupported" => {
                Self::Unsupported
            }
            _ => Self::Generic,
        }
    }
}

/// Builds the `identify`-capability [`OperationRequest`] this module sends
/// through `run_operation` for one candidate. The resulting `DeviceId` is
/// used ONLY as the SDK's call-addressing handle for this one request — per
/// this node's task packet Section 3, it is never again treated as the
/// candidate's/device's final stable identity; only a
/// [`loki_connectors_sdk::identity::PublicDeviceIdentity`] produced by
/// [`crate::registry::ConnectorRegistry`]'s reconciliation logic is.
///
/// ## Why this returns `Result` instead of panicking
///
/// `TransportCandidate::new` (and therefore any `candidate_id` a
/// [`CandidateSource`] may legitimately hand this crate) only bounds
/// `candidate_id` to 128 bytes — the *same* 128-byte bound `OperationId`,
/// `IdempotencyKey`, and `DeviceId` independently enforce on their own raw
/// input. This function does not address the connector with the raw
/// `candidate_id`; it wraps it as `"discover-{candidate_id}-{sequence}"` for
/// the operation id/idempotency key. That wrapper adds roughly 30 bytes
/// (`"discover-"` + `"-"` + up to 20 `u64` digits), so any `candidate_id`
/// longer than that headroom — entirely valid per `TransportCandidate::new`'s
/// own bound — pushes the wrapped string over the SDK's 128-byte bound. A
/// panic on a value this crate's own dependency contract says is legitimate
/// would be a reachable, security-relevant availability bug (a discovery
/// cycle for one candidate could take the whole process down), so this
/// returns an honest [`DiscoveryFailureClass::CandidateNotAddressable`]
/// instead. The raw (un-wrapped) `candidate_id` passed to `DeviceId::new`
/// below can never itself exceed `DeviceId`'s bound given a `candidate_id`
/// that round-trips through `TransportCandidate::new`'s identical bound, but
/// is still guarded the same way rather than re-asserted with `.expect`, so
/// a caller that hands `ObservedCandidate` a `candidate_id` that does not
/// actually match what went into its `TransportCandidate` cannot panic this
/// crate either.
pub(crate) fn build_identify_request(
    candidate: &ObservedCandidate,
) -> Result<OperationRequest, DiscoveryFailureClass> {
    let sequence = next_sequence();
    let ephemeral_id = &candidate.candidate_id;
    let not_addressable = |_: loki_connectors_sdk::error::ValidationError| {
        DiscoveryFailureClass::CandidateNotAddressable
    };
    let operation_id =
        OperationId::new(format!("discover-{ephemeral_id}-{sequence}")).map_err(not_addressable)?;
    let idempotency_key = IdempotencyKey::new(format!("discover-{ephemeral_id}-{sequence}"))
        .map_err(not_addressable)?;
    let device_id = DeviceId::new(ephemeral_id).map_err(not_addressable)?;
    OperationRequest::new(
        operation_id,
        CapabilityId::new("identify").expect("'identify' is a valid capability-id slug"),
        TargetIdentity::new(device_id),
        idempotency_key,
    )
    .map_err(not_addressable)
}

/// One candidate this crate could not even address for an `identify` call —
/// see [`build_identify_request`] and [`DiscoveryFailureClass::CandidateNotAddressable`].
/// `lifecycle: None` because the connector was never invoked.
pub(crate) fn unaddressable_probe(
    observed: ObservedCandidate,
    failure_class: DiscoveryFailureClass,
    observed_at: SystemTime,
) -> CandidateProbe {
    CandidateProbe {
        candidate: observed.candidate,
        lifecycle: None,
        evidence: None,
        probe_result: None,
        failure_class: Some(failure_class),
        observed_at,
    }
}

/// One candidate's fully-interpreted `identify` result — Deliverable 2. Only
/// [`LifecycleOutcome::succeeded`] (an independently-verified `verify`
/// success) can ever produce `evidence: Some(_)`/a `Verified`
/// [`IdentityProbeResult`]; every other outcome (preflight/execute/verify
/// failure or timeout) is preserved verbatim in `lifecycle`, with `evidence:
/// None` and, for a real connector-reported failure, a classified
/// [`DiscoveryFailureClass`].
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateProbe {
    pub candidate: TransportCandidate,
    /// `None` only when this candidate could never be addressed for an
    /// `identify` call at all (`failure_class ==
    /// Some(DiscoveryFailureClass::CandidateNotAddressable)`) — the
    /// connector was never invoked. Every other outcome, including every
    /// connector-reported failure, always carries `Some(_)` here.
    pub lifecycle: Option<LifecycleOutcome>,
    /// The raw, already-validated identity evidence this candidate's
    /// `identify` call reported, when it reported any — kept alongside
    /// `probe_result` (rather than only inside it) because
    /// `IdentityProbeResult` exposes no public getter for its own internal
    /// evidence list, and `crate::registry`'s cross-candidate reconciliation
    /// needs to compare fingerprint digests across every candidate a
    /// connector reported this cycle.
    pub evidence: Option<DeviceIdentityEvidence>,
    /// This candidate's own, individually-computed
    /// [`IdentityProbeResult`] — `Some` whenever the lifecycle reached and
    /// passed `verify` (whether or not the connector attached identity
    /// evidence: an honest `Unknown` result when it did not), `None` only
    /// when identify did not succeed at all (see `failure_class` for why).
    pub probe_result: Option<IdentityProbeResult>,
    /// Set only when the lifecycle did not succeed and the first
    /// connector-reported failure classifies under this registry's
    /// busy/permission/disconnect/unsupported/generic convention.
    pub failure_class: Option<DiscoveryFailureClass>,
    pub observed_at: SystemTime,
}

/// The first connector-reported [`ConnectorError`] across
/// preflight/execute/verify, in stage order — the failure this module
/// classifies. A [`StageOutcome::Timeout`] is deliberately not classified
/// into any of the named failure classes (it carries no connector-reported
/// `code` to classify from); it remains visible only via `lifecycle` itself.
fn first_error(lifecycle: &LifecycleOutcome) -> Option<&ConnectorError> {
    if let StageOutcome::Failure(e) = &lifecycle.preflight {
        return Some(e);
    }
    if let Some(StageOutcome::Failure(e)) = &lifecycle.execute {
        return Some(e);
    }
    if let Some(StageOutcome::Failure(e)) = &lifecycle.verify {
        return Some(e);
    }
    None
}

/// Turns a completed identify [`LifecycleOutcome`] into a [`CandidateProbe`]
/// — the sole place this crate decides whether a lifecycle's real result
/// counts as resolved identity evidence for one candidate. See this
/// module's doc comment for the `IdentityConfidence` and untrusted-wire-data
/// policy this function embodies.
pub(crate) fn interpret_identify_outcome(
    observed: ObservedCandidate,
    lifecycle: LifecycleOutcome,
    observed_at: SystemTime,
) -> CandidateProbe {
    let candidate = observed.candidate;
    if lifecycle.succeeded() {
        let stage_evidence = match &lifecycle.verify {
            Some(StageOutcome::Success(stage_evidence)) => stage_evidence,
            // `succeeded()` guarantees `verify` is `Some(Success(_))`; any
            // other shape here would mean this crate's own invariant broke,
            // not that the connector misbehaved.
            _ => unreachable!(
                "LifecycleOutcome::succeeded() guarantees Some(StageOutcome::Success(_)) in verify"
            ),
        };
        match stage_evidence.identity_evidence.clone() {
            Some(evidence) => {
                let probe_result = IdentityProbeResult::verified(
                    candidate.clone(),
                    vec![evidence.clone()],
                    "identify_capability_verified",
                )
                .expect("a single-item evidence vec is always internally digest-consistent");
                CandidateProbe {
                    candidate,
                    lifecycle: Some(lifecycle),
                    evidence: Some(evidence),
                    probe_result: Some(probe_result),
                    failure_class: None,
                    observed_at,
                }
            }
            None => {
                // Verify succeeded, but the connector's identify capability
                // never attached typed identity evidence
                // (`StageEvidence.identity_evidence`) — an honest "no
                // device-derived evidence to promote to a stable identity"
                // result, never fabricated (`AGENTS.md` rule 1).
                let probe_result = IdentityProbeResult::unresolved(
                    candidate.clone(),
                    IdentityResolutionStatus::Unknown,
                    Vec::new(),
                    "identify_succeeded_without_identity_evidence",
                )
                .expect("Unknown status with empty evidence is always a valid construction");
                CandidateProbe {
                    candidate,
                    lifecycle: Some(lifecycle),
                    evidence: None,
                    probe_result: Some(probe_result),
                    failure_class: None,
                    observed_at,
                }
            }
        }
    } else {
        let failure_class = first_error(&lifecycle).map(DiscoveryFailureClass::classify);
        CandidateProbe {
            candidate,
            lifecycle: Some(lifecycle),
            evidence: None,
            probe_result: None,
            failure_class,
            observed_at,
        }
    }
}

/// One candidate's final, registry-reconciled result for one discovery
/// cycle — Deliverable 3/4's public-facing shape, assembled by
/// [`crate::registry::ConnectorRegistry`] from a [`CandidateProbe`] plus
/// this cycle's cross-candidate reconciliation outcome for the owning
/// connector. See `crate::registry`'s module doc for the reconciliation
/// rules that decide `public_identity`/`reconciliation_conflict`.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveryOutcome {
    pub connector_id: ConnectorId,
    pub candidate: TransportCandidate,
    /// The full lifecycle detail (`preflight`/`execute`/`verify`/`recovery`)
    /// from the real `identify` operation this cycle ran — never discarded,
    /// so a caller can see *why* identification failed, not just that it
    /// did. `None` only when this candidate could never be addressed for an
    /// `identify` call at all — see [`CandidateProbe::lifecycle`].
    pub lifecycle: Option<LifecycleOutcome>,
    /// This candidate's own, individually-computed
    /// [`IdentityProbeResult`] — see [`CandidateProbe::probe_result`].
    pub probe_result: Option<IdentityProbeResult>,
    /// The registry-accepted stable public identity for this candidate this
    /// cycle, after reconciliation across every candidate this connector
    /// reported this cycle. `None` whenever this candidate's own
    /// `probe_result` was not `Verified`, or whenever `reconciliation_conflict`
    /// is `true` (fail-closed — no identity is accepted from a connector
    /// whose candidates disagreed this cycle, even for the candidates that
    /// individually resolved cleanly).
    pub public_identity: Option<PublicDeviceIdentity>,
    /// `true` exactly when this candidate's connector reported multiple
    /// candidates this cycle that independently verified to *different*
    /// fingerprint digests — a fail-closed conflict (`AS-003`/`AS-004`,
    /// Deliverable 3's "malformed/conflicting evidence... fails closed"
    /// acceptance criterion), regardless of what this one candidate's own
    /// `probe_result` said.
    pub reconciliation_conflict: bool,
    /// Set when `lifecycle` did not succeed and the first connector-reported
    /// failure classifies under this crate's
    /// busy/permission/disconnect/unsupported/generic convention (see this
    /// module's doc comment), OR when `lifecycle` is `None` because this
    /// candidate could never be addressed for an `identify` call at all
    /// (`Some(DiscoveryFailureClass::CandidateNotAddressable)`).
    pub failure_class: Option<DiscoveryFailureClass>,
    pub observed_at: SystemTime,
}

/// The full result of one [`crate::registry::ConnectorRegistry::run_discovery_cycle`]
/// call.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DiscoveryCycleReport {
    /// One entry per candidate actually identified this cycle.
    pub outcomes: Vec<DiscoveryOutcome>,
    /// Devices that were reconciled `live` before this cycle, but whose
    /// owning connector reported zero candidates this cycle — a clean
    /// disconnect, not an error. `AGENTS.md` rule 1: no identity is
    /// fabricated as still-live without new evidence; this list is exactly
    /// the set of previously-live identities this cycle can no longer
    /// confirm.
    pub newly_absent: Vec<PublicDeviceIdentity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_connectors_sdk::error::ConnectorError;
    use loki_connectors_sdk::identity::{
        DeviceIdentityEvidence, DeviceIdentityEvidenceInput, IdentityConfidence,
        SensitiveUnstableLocator,
    };
    use loki_connectors_sdk::operation::OperationStage;
    use loki_connectors_sdk::outcome::StageEvidence;

    const FIXTURE_INSTALLATION_KEY: &[u8; 32] = b"fixture-installation-key-32bytes";

    fn candidate(candidate_id: &str) -> TransportCandidate {
        TransportCandidate::new(
            candidate_id,
            "loki-mock-connector",
            "1.0.0",
            "fixture_transport",
            SensitiveUnstableLocator::new("COM-fixture").unwrap(),
            "2026-08-26T12:00:00.000Z",
        )
        .unwrap()
    }

    fn identity_evidence(material: &str, confidence: IdentityConfidence) -> DeviceIdentityEvidence {
        DeviceIdentityEvidence::new(DeviceIdentityEvidenceInput {
            evidence_type: "hardware_uid".into(),
            fingerprint_material: material.into(),
            installation_key: FIXTURE_INSTALLATION_KEY,
            confidence,
            connector_id: "loki-mock-connector".into(),
            connector_version: "1.0.0".into(),
            observed_at: "2026-08-26T12:00:01.000Z".into(),
        })
        .unwrap()
    }

    fn evidence_stage(summary: &str) -> StageEvidence {
        StageEvidence::new(summary, SystemTime::now()).unwrap()
    }

    #[test]
    fn two_requests_for_the_same_candidate_get_distinct_operation_ids() {
        let c = ObservedCandidate::new("cand-1", candidate("cand-1"));
        let a = build_identify_request(&c).expect("short candidate id is addressable");
        let b = build_identify_request(&c).expect("short candidate id is addressable");
        assert_ne!(a.operation_id, b.operation_id);
        assert_ne!(a.idempotency_key, b.idempotency_key);
    }

    #[test]
    fn a_candidate_id_exactly_at_transportcandidate_s_128_byte_bound_does_not_panic() {
        // `TransportCandidate::new` accepts a `candidate_id` up to 128 bytes.
        // This is the boundary the reachable panic this test guards against
        // was reported against: a legitimately-sized candidate id that the
        // "discover-{id}-{sequence}" wrapper pushes over `OperationId`'s own
        // 128-byte bound.
        let id_128 = "c".repeat(128);
        let c = ObservedCandidate::new(id_128.clone(), candidate(&id_128));
        // Does not panic. Because the wrapper adds well over the ~0 bytes of
        // headroom a 128-byte input leaves, this must fail closed with the
        // dedicated non-connector-reported failure class, never panic and
        // never silently fall back to Busy/PermissionDenied/Disconnected.
        let result = build_identify_request(&c);
        assert_eq!(result, Err(DiscoveryFailureClass::CandidateNotAddressable));
    }

    #[test]
    fn a_candidate_id_far_beyond_the_old_panic_threshold_does_not_panic() {
        // The old code panicked (`.expect(...)` on `OperationId::new`'s
        // `Err`) for any candidate id longer than roughly 117 bytes once
        // wrapped. This uses the full 128-byte bound `TransportCandidate::new`
        // itself allows, which is well past that old threshold.
        let id_128 = "d".repeat(128);
        let c = ObservedCandidate::new(id_128.clone(), candidate(&id_128));
        let result = build_identify_request(&c);
        assert_eq!(result, Err(DiscoveryFailureClass::CandidateNotAddressable));
    }

    #[test]
    fn a_short_candidate_id_still_builds_a_well_formed_request() {
        // Confirms the fix does not regress the ordinary, well-inside-bound
        // case: a ~20-byte candidate id, comfortably inside the wrapper's
        // headroom, still produces `Ok`.
        let id = "cand-well-within-bound";
        let c = ObservedCandidate::new(id, candidate(id));
        let request = build_identify_request(&c).expect("well within bound, must not fail");
        assert!(request.operation_id.as_str().starts_with("discover-"));
    }

    #[test]
    fn successful_lifecycle_with_identity_evidence_yields_verified_probe_result() {
        let evidence = identity_evidence("device-material-1", IdentityConfidence::Weak);
        let lifecycle = LifecycleOutcome {
            stage_reached: OperationStage::Verify,
            preflight: StageOutcome::Success(evidence_stage("preflight ok")),
            execute: Some(StageOutcome::Success(evidence_stage("execute ok"))),
            verify: Some(StageOutcome::Success(
                evidence_stage("chip fingerprint matched").with_identity_evidence(evidence.clone()),
            )),
            recovery: None,
        };
        let probe = interpret_identify_outcome(
            ObservedCandidate::new("cand-1", candidate("cand-1")),
            lifecycle,
            SystemTime::now(),
        );

        assert_eq!(probe.evidence.as_ref(), Some(&evidence));
        let result = probe.probe_result.expect("verified probe result");
        assert!(result.public_identity().is_some());
        assert_eq!(
            result.public_identity().unwrap().device_id(),
            format!("dev1_{}", evidence.fingerprint_digest())
        );
    }

    #[test]
    fn successful_lifecycle_without_identity_evidence_yields_unknown_not_fabricated() {
        let lifecycle = LifecycleOutcome {
            stage_reached: OperationStage::Verify,
            preflight: StageOutcome::Success(evidence_stage("preflight ok")),
            execute: Some(StageOutcome::Success(evidence_stage("execute ok"))),
            verify: Some(StageOutcome::Success(evidence_stage(
                "verify ok (no typed evidence)",
            ))),
            recovery: None,
        };
        let probe = interpret_identify_outcome(
            ObservedCandidate::new("cand-1", candidate("cand-1")),
            lifecycle,
            SystemTime::now(),
        );

        assert!(probe.evidence.is_none());
        let result = probe.probe_result.expect("still produces a probe result");
        assert!(result.public_identity().is_none());
    }

    #[test]
    fn preflight_failure_is_classified_busy() {
        let lifecycle = LifecycleOutcome {
            stage_reached: OperationStage::Preflight,
            preflight: StageOutcome::Failure(
                ConnectorError::new("transport_busy", "port in use").unwrap(),
            ),
            execute: None,
            verify: None,
            recovery: None,
        };
        let probe = interpret_identify_outcome(
            ObservedCandidate::new("cand-1", candidate("cand-1")),
            lifecycle,
            SystemTime::now(),
        );
        assert!(probe.probe_result.is_none());
        assert!(probe.evidence.is_none());
        assert_eq!(probe.failure_class, Some(DiscoveryFailureClass::Busy));
    }

    #[test]
    fn permission_denied_is_distinguished_from_busy_and_generic() {
        let lifecycle = LifecycleOutcome {
            stage_reached: OperationStage::Preflight,
            preflight: StageOutcome::Failure(
                ConnectorError::new("permission_denied", "no access to serial port").unwrap(),
            ),
            execute: None,
            verify: None,
            recovery: None,
        };
        let probe = interpret_identify_outcome(
            ObservedCandidate::new("cand-1", candidate("cand-1")),
            lifecycle,
            SystemTime::now(),
        );
        assert_eq!(
            probe.failure_class,
            Some(DiscoveryFailureClass::PermissionDenied)
        );
    }

    #[test]
    fn unsupported_candidate_is_distinguished() {
        let lifecycle = LifecycleOutcome {
            stage_reached: OperationStage::Verify,
            preflight: StageOutcome::Success(evidence_stage("preflight ok")),
            execute: Some(StageOutcome::Success(evidence_stage("execute ok"))),
            verify: Some(StageOutcome::Failure(
                ConnectorError::new("unsupported_candidate", "device family mismatch").unwrap(),
            )),
            recovery: None,
        };
        let probe = interpret_identify_outcome(
            ObservedCandidate::new("cand-1", candidate("cand-1")),
            lifecycle,
            SystemTime::now(),
        );
        assert_eq!(
            probe.failure_class,
            Some(DiscoveryFailureClass::Unsupported)
        );
        assert!(probe.probe_result.is_none());
    }

    #[test]
    fn generic_failure_is_its_own_class() {
        let lifecycle = LifecycleOutcome {
            stage_reached: OperationStage::Verify,
            preflight: StageOutcome::Success(evidence_stage("preflight ok")),
            execute: Some(StageOutcome::Success(evidence_stage("execute ok"))),
            verify: Some(StageOutcome::Failure(
                ConnectorError::new("identity_mismatch", "reported chip id changed mid-cycle")
                    .unwrap(),
            )),
            recovery: Some(StageOutcome::Failure(
                ConnectorError::new("no_recovery", "identify has no recovery path").unwrap(),
            )),
        };
        let probe = interpret_identify_outcome(
            ObservedCandidate::new("cand-1", candidate("cand-1")),
            lifecycle,
            SystemTime::now(),
        );
        assert_eq!(probe.failure_class, Some(DiscoveryFailureClass::Generic));
        assert!(probe.probe_result.is_none());
    }

    #[test]
    fn timeout_is_not_classified_into_a_named_failure_class() {
        let lifecycle = LifecycleOutcome {
            stage_reached: OperationStage::Preflight,
            preflight: StageOutcome::Timeout(loki_connectors_sdk::outcome::StageTimeout {
                stage: OperationStage::Preflight,
                observed_at: SystemTime::now(),
                deadline: SystemTime::now(),
            }),
            execute: None,
            verify: None,
            recovery: None,
        };
        let probe = interpret_identify_outcome(
            ObservedCandidate::new("cand-1", candidate("cand-1")),
            lifecycle,
            SystemTime::now(),
        );
        assert_eq!(probe.failure_class, None);
    }
}
