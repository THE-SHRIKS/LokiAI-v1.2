//! `MutationAcceptanceRequest` — the envelope ADR-0006 "Decision" step 2
//! describes as binding request identity into the acceptance transaction.
//!
//! # No `workspace` field (Part One)
//!
//! ADR-0006 step 2 reads "bind workspace, scopes, method, full canonical
//! request digest, idempotency key, target, artifact, policy, and approval
//! inputs." `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md` §14 records the
//! human product/security owner's direction (2026-08-25, chat): drop the
//! "workspace" field entirely from this contract for Part One rather than
//! invent a placeholder local workspace/tenant identifier, by analogy to how
//! `LDEV-001`/`ENR-001` already split local device identity (now) from
//! cloud ownership (later, Part Two) — `crates/loki-local-authority`
//! confirms workspace/tenant is a cloud concept
//! (`CloudCapability::Workspace`, fail-closed/`CloudUnavailable`), deferred
//! for all of Part One under ADR-0002. This struct therefore has no
//! `workspace` field, and no field stands in for one. This is a live
//! tension with ADR-0006's decision text, not a silent resolution of it —
//! see `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md` §9 item 5 and this
//! crate's handoff, both of which record it as still requiring a short
//! ADR-0006 amendment before `P1-ACCEPTANCE-STORE-001` treats it as
//! settled. This struct also has no `scopes` field: the packet's own §6
//! deliverable field list for `MutationAcceptanceRequest` (which this
//! struct implements verbatim) does not name one, unlike ADR-0006's
//! decision text — see the same §9 item 5 note in the handoff.

use crate::digest::{IdempotencyKey, OperationBinding, RequestDigest};
use crate::error::MutationContractError;
use crate::method::MutationMethod;
use crate::result::MutationRejection;

/// A single-use reference to a consumed (or consumable) approval, bound to
/// the exact canonical request digest it authorizes. Populated on
/// `MutationAcceptanceRequest` iff the method's policy tier requires it
/// (`MutationMethod::requires_approval`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalReference {
    pub approval_id: String,
    pub bound_request_digest: RequestDigest,
    pub expires_at_ms: i64,
}

/// Pure evaluation of an [`ApprovalReference`] against the exact request
/// digest it must authorize and the current evaluation time — analogous to
/// [`crate::grant::evaluate_grant`]'s pure evaluation of a
/// [`crate::grant::GrantGeneration`]. ADR-0006 step 4: "validate and consume
/// the exact approval when required." Returns
/// [`MutationRejection::ApprovalNotConsumable`] if the reference is bound to
/// a different digest than `expected_digest`, or if `now_ms` has reached or
/// passed `reference.expires_at_ms`. This function performs no I/O and does
/// not look anything up or mark anything consumed — the caller
/// (`P1-ACCEPTANCE-STORE-001`, inside its real transaction) is responsible
/// for the actual single-use consumption side effect; this crate only proves
/// the reference is structurally consumable at `now_ms`.
pub fn evaluate_approval_reference(
    reference: &ApprovalReference,
    expected_digest: &RequestDigest,
    now_ms: i64,
) -> Result<(), MutationRejection> {
    if reference.bound_request_digest != *expected_digest {
        return Err(MutationRejection::ApprovalNotConsumable);
    }
    if now_ms >= reference.expires_at_ms {
        return Err(MutationRejection::ApprovalNotConsumable);
    }
    Ok(())
}

/// An immutable operation binding plus its optional approval reference.
/// The bound fields are private and can only be observed through accessors,
/// so no caller can mutate a field after the canonical digest is derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationAcceptanceRequest {
    binding: OperationBinding,
    approval_reference: Option<ApprovalReference>,
}

impl MutationAcceptanceRequest {
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor exposes the complete closed operation binding with no mutable builder"
    )]
    pub fn new(
        principal_sid: impl Into<String>,
        actor_id: impl Into<String>,
        grant_generation: u64,
        method: MutationMethod,
        target_id: impl Into<String>,
        artifact_digest: Option<String>,
        parameters_digest: impl Into<String>,
        idempotency_key: IdempotencyKey,
        approval_reference: Option<ApprovalReference>,
    ) -> Result<Self, MutationContractError> {
        let binding = OperationBinding::new(
            principal_sid,
            actor_id,
            grant_generation,
            method,
            target_id,
            artifact_digest,
            parameters_digest,
            idempotency_key,
        )?;
        Ok(Self {
            binding,
            approval_reference,
        })
    }

    pub fn binding(&self) -> &OperationBinding {
        &self.binding
    }

    pub fn request_digest(&self) -> RequestDigest {
        self.binding.digest()
    }

    pub fn approval_reference(&self) -> Option<&ApprovalReference> {
        self.approval_reference.as_ref()
    }

    pub fn principal_sid(&self) -> &str {
        self.binding.principal_sid()
    }

    pub fn actor_id(&self) -> &str {
        self.binding.actor_id()
    }

    pub fn grant_generation(&self) -> u64 {
        self.binding.grant_generation()
    }

    pub fn method(&self) -> MutationMethod {
        self.binding.method()
    }

    pub fn target_id(&self) -> &str {
        self.binding.target_id()
    }

    pub fn artifact_digest(&self) -> Option<&str> {
        self.binding.artifact_digest()
    }

    pub fn parameters_digest(&self) -> &str {
        self.binding.parameters_digest()
    }

    pub fn idempotency_key(&self) -> &IdempotencyKey {
        self.binding.idempotency_key()
    }

    pub fn with_approval_reference(mut self, reference: ApprovalReference) -> Self {
        self.approval_reference = Some(reference);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approval_ref(digest: &RequestDigest) -> ApprovalReference {
        ApprovalReference {
            approval_id: "apr_1".to_string(),
            bound_request_digest: digest.clone(),
            expires_at_ms: 10_000,
        }
    }

    #[test]
    fn request_digest_is_always_derived_not_caller_supplied() {
        let request = MutationAcceptanceRequest::new(
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
        .unwrap();
        let expected = OperationBinding::new(
            "S-1-5-21-principal",
            "agent-1",
            1,
            MutationMethod::AgentRunRegister,
            "target-1",
            None,
            "sha256:parameters",
            IdempotencyKey::new("idem-1").unwrap(),
        )
        .unwrap()
        .digest();
        assert_eq!(request.request_digest(), expected);
    }

    #[test]
    fn approval_reference_may_be_bound_to_the_computed_digest() {
        let request = MutationAcceptanceRequest::new(
            "S-1-5-21-principal",
            "agent-1",
            1,
            MutationMethod::Flash,
            "target-1",
            Some("sha256:artifact".to_string()),
            "sha256:parameters",
            IdempotencyKey::new("idem-1").unwrap(),
            None,
        )
        .unwrap();
        let reference = approval_ref(&request.request_digest());
        assert_eq!(reference.bound_request_digest, request.request_digest());
    }

    #[test]
    fn approval_reference_with_matching_digest_and_unexpired_time_is_consumable() {
        let digest = RequestDigest::try_from_str("sha256:".to_string() + &"a".repeat(64)).unwrap();
        let reference = approval_ref(&digest);
        assert!(evaluate_approval_reference(&reference, &digest, 5_000).is_ok());
    }

    #[test]
    fn approval_reference_with_mismatched_digest_is_not_consumable() {
        let digest = RequestDigest::try_from_str("sha256:".to_string() + &"a".repeat(64)).unwrap();
        let other_digest =
            RequestDigest::try_from_str("sha256:".to_string() + &"b".repeat(64)).unwrap();
        let reference = approval_ref(&digest);
        assert!(matches!(
            evaluate_approval_reference(&reference, &other_digest, 5_000),
            Err(MutationRejection::ApprovalNotConsumable)
        ));
    }

    #[test]
    fn expired_approval_reference_is_not_consumable_even_with_matching_digest() {
        let digest = RequestDigest::try_from_str("sha256:".to_string() + &"a".repeat(64)).unwrap();
        let reference = approval_ref(&digest); // expires_at_ms: 10_000
        // now_ms == expires_at_ms: expired per "reaches or passes" semantics
        assert!(matches!(
            evaluate_approval_reference(&reference, &digest, 10_000),
            Err(MutationRejection::ApprovalNotConsumable)
        ));
        // now_ms well past expires_at_ms
        assert!(matches!(
            evaluate_approval_reference(&reference, &digest, 50_000),
            Err(MutationRejection::ApprovalNotConsumable)
        ));
    }
}
