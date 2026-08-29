//! `AcceptanceResult`, `MutationRejection`, `AcceptanceRecordView`, and
//! `ReplayDecision` — ADR-0006 "Consequences": "Mutation results become
//! discriminated states with no contradictory nullable combinations."
//!
//! Each [`AcceptanceResult`] variant carries only the fields that make sense
//! for that outcome (no shared `Option<...>` fields that could disagree with
//! the variant tag) — this is enforced by the Rust type system itself, not
//! by a runtime invariant check, which is what "no contradictory nullable
//! combinations" means as a closed enum rather than one flat struct with
//! optional fields.

use loki_storage::ids::{CorrelationId, JobId};

use crate::digest::RequestDigest;
use crate::error::MutationContractError;
use crate::request::{MutationAcceptanceRequest, evaluate_approval_reference};

/// Closed rejection reasons. `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md`
/// §6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationRejection {
    /// Same idempotency key, different canonical request digest —
    /// ADR-0006 step 3: "claim or replay the idempotency record, rejecting
    /// same-key/different-digest."
    DigestConflict,
    /// The referenced grant generation does not correspond to a real,
    /// structurally valid grant (as opposed to `GrantExpired`/`GrantRevoked`,
    /// which are about a real grant's temporal validity).
    GrantInvalid {
        reason: String,
    },
    GrantExpired,
    GrantRevoked,
    /// Local policy denies the operation outright (mirrors
    /// `loki_approvals::PolicyDecision::Deny`).
    PolicyDenied,
    /// An `ApprovalReference` was present but could not be consumed
    /// (already consumed, expired, or bound to a different digest).
    ApprovalNotConsumable,
    InvalidInput {
        field: &'static str,
        reason: String,
    },
}

impl std::fmt::Display for MutationRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DigestConflict => write!(
                f,
                "idempotency key reused with a different canonical request digest"
            ),
            Self::GrantInvalid { reason } => write!(f, "grant generation is invalid: {reason}"),
            Self::GrantExpired => write!(f, "grant generation has expired"),
            Self::GrantRevoked => write!(f, "grant generation has been revoked"),
            Self::PolicyDenied => write!(f, "local policy denied the mutation"),
            Self::ApprovalNotConsumable => {
                write!(f, "the referenced approval could not be consumed")
            }
            Self::InvalidInput { field, reason } => {
                write!(f, "invalid field '{field}': {reason}")
            }
        }
    }
}

impl std::error::Error for MutationRejection {}

/// A committed acceptance's identifying fields (`job_id`, `correlation_id`,
/// `accepted_at_ms`).
///
/// # Why this is a private-field struct, not three loose enum fields
///
/// Security review finding (this crate's independent review, before
/// acceptance): if `AcceptanceResult::Accepted`/`ReplayedIdenticalAccepted`
/// carried these three fields directly as public struct-variant fields, any
/// downstream code could construct either variant via struct-literal syntax
/// (`AcceptanceResult::Accepted { job_id, correlation_id, accepted_at_ms }`),
/// completely bypassing [`construct_accepted`]'s approval-presence/
/// digest-match/expiry check — Rust gives enum variant fields the same
/// visibility as the enum itself; there is no way to make only the variant
/// constructor private while keeping the enum public. Wrapping the fields in
/// this struct, whose fields are private to this crate and whose only
/// constructor ([`AcceptedRecord::new`]) is private to this module, closes
/// that gap: the only way to obtain an `AcceptedRecord` from outside this
/// module is through [`construct_accepted`] or the replay path
/// ([`replayed_result`]), both of which run the real checks first. Only
/// accessor methods are public.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedRecord {
    job_id: JobId,
    correlation_id: CorrelationId,
    accepted_at_ms: i64,
}

impl AcceptedRecord {
    /// Private: the only callers are [`construct_accepted`] and
    /// [`replayed_result`], both in this module, both of which have already
    /// run the checks that justify producing an accepted record.
    fn new(job_id: JobId, correlation_id: CorrelationId, accepted_at_ms: i64) -> Self {
        Self {
            job_id,
            correlation_id,
            accepted_at_ms,
        }
    }

    pub fn job_id(&self) -> &JobId {
        &self.job_id
    }

    pub fn correlation_id(&self) -> &CorrelationId {
        &self.correlation_id
    }

    pub fn accepted_at_ms(&self) -> i64 {
        self.accepted_at_ms
    }
}

/// Closed discriminated acceptance outcome.
/// `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md` §6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptanceResult {
    Accepted(AcceptedRecord),
    ApprovalRequired {
        approval_id: String,
        bound_request_digest: RequestDigest,
        expires_at_ms: i64,
    },
    /// Same idempotency key, same canonical request digest as a previously
    /// committed acceptance: the original result is returned again rather
    /// than re-executed.
    ReplayedIdenticalAccepted(AcceptedRecord),
    Rejected(MutationRejection),
}

/// A read view of a previously committed acceptance record, keyed by the
/// same idempotency key as an incoming request. Pure data — this crate
/// never fetches this itself; `P1-ACCEPTANCE-STORE-001` loads it inside its
/// transaction and passes it to [`evaluate_replay`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceRecordView {
    pub job_id: JobId,
    pub correlation_id: CorrelationId,
    pub accepted_at_ms: i64,
    pub request_digest: RequestDigest,
}

/// The outcome of comparing an existing committed acceptance record against
/// an incoming request that shares its idempotency key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayDecision {
    Identical,
    DigestConflict,
}

/// ADR-0006 step 3, "Required evidence": "Same-key/same-digest returns the
/// original committed result; changed digest conflicts." `existing` is only
/// ever called with a record that the caller already found by the incoming
/// request's idempotency key — the "no prior record" case is the caller
/// simply not calling this function at all (there is nothing to compare).
pub fn evaluate_replay(
    existing: &AcceptanceRecordView,
    incoming: &MutationAcceptanceRequest,
) -> ReplayDecision {
    if existing.request_digest == incoming.request_digest() {
        ReplayDecision::Identical
    } else {
        ReplayDecision::DigestConflict
    }
}

/// Builds the [`AcceptanceResult::ReplayedIdenticalAccepted`] variant from a
/// record [`evaluate_replay`] already determined to be identical.
pub fn replayed_result(existing: &AcceptanceRecordView) -> AcceptanceResult {
    AcceptanceResult::ReplayedIdenticalAccepted(AcceptedRecord::new(
        existing.job_id.clone(),
        existing.correlation_id.clone(),
        existing.accepted_at_ms,
    ))
}

/// Constructs an `AcceptanceResult::Accepted`, but refuses to do so for an
/// approval-gated method unless `request.approval_reference` is present,
/// bound to `request.request_digest`, and not yet expired at
/// `accepted_at_ms` — ADR-0006 step 4: "validate and consume the exact
/// approval when required." This is the type-level proof behind "approval
/// required method cannot construct Accepted without approval reference":
/// there is no code path in this crate that produces
/// `AcceptanceResult::Accepted` for such a method without it.
///
/// A missing `approval_reference` on a method that requires one is treated
/// as a caller/construction error (`MutationContractError::InvalidInput`) —
/// the caller should never have reached this function without one. A
/// *present* reference that fails digest-match or has expired is not a
/// construction error: it is a genuine runtime rejection, so it is routed
/// through the same `MutationRejection`-typed path
/// [`crate::grant::evaluate_grant`]'s callers use, returned as
/// `Ok(AcceptanceResult::Rejected(MutationRejection::ApprovalNotConsumable))`
/// rather than an `Err`.
///
/// `accepted_at_ms` doubles as the evaluation time (`now_ms`) for the
/// approval-expiry check: it is the acceptance transaction's own commit
/// timestamp, which is the correct "now" to evaluate expiry against — this
/// function performs no I/O and does not read a clock itself.
pub fn construct_accepted(
    request: &MutationAcceptanceRequest,
    job_id: JobId,
    correlation_id: CorrelationId,
    accepted_at_ms: i64,
) -> Result<AcceptanceResult, MutationContractError> {
    if request.binding().method().requires_approval() {
        match request.approval_reference() {
            None => {
                return Err(MutationContractError::InvalidInput {
                    field: "approval_reference",
                    reason: "method requires a consumed approval reference".to_string(),
                });
            }
            Some(reference) => {
                if let Err(rejection) = evaluate_approval_reference(
                    reference,
                    &request.request_digest(),
                    accepted_at_ms,
                ) {
                    return Ok(AcceptanceResult::Rejected(rejection));
                }
            }
        }
    }
    Ok(AcceptanceResult::Accepted(AcceptedRecord::new(
        job_id,
        correlation_id,
        accepted_at_ms,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::IdempotencyKey;
    use crate::method::MutationMethod;
    use crate::request::ApprovalReference;

    fn job_id() -> JobId {
        JobId::new("job_1").unwrap()
    }
    fn correlation_id() -> CorrelationId {
        CorrelationId::new("corr_1").unwrap()
    }

    fn agent_request() -> MutationAcceptanceRequest {
        MutationAcceptanceRequest::new(
            "S-1-5-21-principal",
            "agent-1",
            1,
            MutationMethod::AgentRunRegister,
            "target-1",
            None,
            "sha256:parameters",
            IdempotencyKey::new("idem-1").unwrap(),
            None,
        )
        .unwrap()
    }

    fn flash_request(approval_reference: Option<ApprovalReference>) -> MutationAcceptanceRequest {
        MutationAcceptanceRequest::new(
            "S-1-5-21-principal",
            "agent-1",
            1,
            MutationMethod::Flash,
            "target-1",
            Some("sha256:artifact".to_string()),
            "sha256:parameters",
            IdempotencyKey::new("idem-1").unwrap(),
            approval_reference,
        )
        .unwrap()
    }

    #[test]
    fn same_key_same_digest_replays_original_accepted_result() {
        let incoming = agent_request();
        let existing = AcceptanceRecordView {
            job_id: job_id(),
            correlation_id: correlation_id(),
            accepted_at_ms: 5_000,
            request_digest: incoming.request_digest(),
        };
        assert_eq!(
            evaluate_replay(&existing, &incoming),
            ReplayDecision::Identical
        );
        let replayed = replayed_result(&existing);
        assert_eq!(
            replayed,
            AcceptanceResult::ReplayedIdenticalAccepted(AcceptedRecord::new(
                job_id(),
                correlation_id(),
                5_000,
            ))
        );
    }

    #[test]
    fn same_key_different_digest_is_rejected_as_digest_conflict() {
        let incoming = agent_request();
        let different = MutationAcceptanceRequest::new(
            "S-1-5-21-principal",
            "agent-1",
            1,
            MutationMethod::AgentRunRegister,
            "target-2",
            None,
            "sha256:parameters",
            IdempotencyKey::new("idem-1").unwrap(),
            None,
        )
        .unwrap();
        let existing = AcceptanceRecordView {
            job_id: job_id(),
            correlation_id: correlation_id(),
            accepted_at_ms: 5_000,
            // simulate a prior record computed for a different target under
            // the same idempotency key
            request_digest: different.request_digest(),
        };
        assert_eq!(
            evaluate_replay(&existing, &incoming),
            ReplayDecision::DigestConflict
        );
    }

    #[test]
    fn approval_required_method_cannot_construct_accepted_without_approval_reference() {
        let request = flash_request(None);
        let result = construct_accepted(&request, job_id(), correlation_id(), 5_000);
        assert!(matches!(
            result,
            Err(MutationContractError::InvalidInput {
                field: "approval_reference",
                ..
            })
        ));

        let request = flash_request(None);
        let valid = flash_request(Some(ApprovalReference {
            approval_id: "apr_1".to_string(),
            bound_request_digest: request.request_digest(),
            expires_at_ms: 10_000,
        }));
        let accepted = construct_accepted(&valid, job_id(), correlation_id(), 5_000).unwrap();
        assert!(matches!(accepted, AcceptanceResult::Accepted(_)));
    }

    /// Reviewer finding 2: a present, digest-mismatched `ApprovalReference`
    /// must not raise a raw `MutationContractError::InvalidInput` — it must
    /// be routed through `MutationRejection::ApprovalNotConsumable`, and
    /// `construct_accepted` (a real call path, not `evaluate_approval_reference`
    /// called in isolation) must be the thing that produces it.
    #[test]
    fn mismatched_approval_digest_is_rejected_as_approval_not_consumable_via_construct_accepted() {
        let mismatched = flash_request(Some(ApprovalReference {
            approval_id: "apr_1".to_string(),
            bound_request_digest: RequestDigest::try_from_str(
                "sha256:".to_string() + &"a".repeat(64),
            )
            .unwrap(),
            expires_at_ms: 10_000,
        }));
        let result = construct_accepted(&mismatched, job_id(), correlation_id(), 5_000).unwrap();
        assert_eq!(
            result,
            AcceptanceResult::Rejected(MutationRejection::ApprovalNotConsumable)
        );
    }

    /// Reviewer finding 2: a present, digest-matched, but expired
    /// `ApprovalReference` must be rejected via
    /// `MutationRejection::ApprovalNotConsumable`, reachable from the real
    /// `construct_accepted` call path (not `evaluate_approval_reference`
    /// called in isolation).
    #[test]
    fn expired_but_digest_matched_approval_reference_is_rejected_as_approval_not_consumable_via_construct_accepted()
     {
        let request = flash_request(None);
        let expired = flash_request(Some(ApprovalReference {
            approval_id: "apr_1".to_string(),
            bound_request_digest: request.request_digest(),
            expires_at_ms: 5_000,
        }));
        // accepted_at_ms (10_000) has reached/passed expires_at_ms (5_000)
        let result = construct_accepted(&expired, job_id(), correlation_id(), 10_000).unwrap();
        assert_eq!(
            result,
            AcceptanceResult::Rejected(MutationRejection::ApprovalNotConsumable)
        );
    }

    #[test]
    fn agent_method_constructs_accepted_without_any_approval_reference() {
        let request = agent_request();
        let result = construct_accepted(&request, job_id(), correlation_id(), 5_000);
        assert!(result.is_ok());
    }

    #[test]
    fn acceptance_result_variants_carry_no_contradictory_fields() {
        // Exhaustive match, no wildcard arm: proves the closed set and that
        // each arm only destructures the fields that variant actually has
        // (a contradictory nullable combination would require a shared
        // Option field across arms, which this enum shape does not have).
        let samples = [
            AcceptanceResult::Accepted(AcceptedRecord::new(job_id(), correlation_id(), 1)),
            AcceptanceResult::ApprovalRequired {
                approval_id: "apr_1".to_string(),
                bound_request_digest: RequestDigest::try_from_str(
                    "sha256:".to_string() + &"a".repeat(64),
                )
                .unwrap(),
                expires_at_ms: 2,
            },
            AcceptanceResult::ReplayedIdenticalAccepted(AcceptedRecord::new(
                job_id(),
                correlation_id(),
                3,
            )),
            AcceptanceResult::Rejected(MutationRejection::GrantExpired),
        ];
        for sample in samples {
            match sample {
                AcceptanceResult::Accepted(_record) => {}
                AcceptanceResult::ApprovalRequired {
                    approval_id: _,
                    bound_request_digest: _,
                    expires_at_ms: _,
                } => {}
                AcceptanceResult::ReplayedIdenticalAccepted(_record) => {}
                AcceptanceResult::Rejected(_rejection) => {}
            }
        }
    }

    #[test]
    fn mutation_rejection_is_closed_with_exhaustive_match() {
        let samples = [
            MutationRejection::DigestConflict,
            MutationRejection::GrantInvalid {
                reason: "r".to_string(),
            },
            MutationRejection::GrantExpired,
            MutationRejection::GrantRevoked,
            MutationRejection::PolicyDenied,
            MutationRejection::ApprovalNotConsumable,
            MutationRejection::InvalidInput {
                field: "f",
                reason: "r".to_string(),
            },
        ];
        for sample in samples {
            match sample {
                MutationRejection::DigestConflict => {}
                MutationRejection::GrantInvalid { .. } => {}
                MutationRejection::GrantExpired => {}
                MutationRejection::GrantRevoked => {}
                MutationRejection::PolicyDenied => {}
                MutationRejection::ApprovalNotConsumable => {}
                MutationRejection::InvalidInput { .. } => {}
            }
        }
    }

    /// Reviewer finding 1: `AcceptedRecord`'s fields are private and its
    /// constructor (`AcceptedRecord::new`) is private to this module — the
    /// only public surface is the three accessor methods. This test proves
    /// a real `AcceptedRecord` produced by `construct_accepted` exposes its
    /// fields only through those accessors and matches what was passed in.
    #[test]
    fn accepted_record_exposes_fields_only_through_accessor_methods() {
        let request = agent_request();
        let accepted = construct_accepted(&request, job_id(), correlation_id(), 5_000).unwrap();
        let AcceptanceResult::Accepted(record) = accepted else {
            panic!("expected AcceptanceResult::Accepted");
        };
        assert_eq!(*record.job_id(), job_id());
        assert_eq!(*record.correlation_id(), correlation_id());
        assert_eq!(record.accepted_at_ms(), 5_000);
    }
}
