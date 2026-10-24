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

use crate::digest::{IdempotencyKey, RequestDigest, compute_request_digest};
use crate::error::MutationContractError;
use crate::method::MutationMethod;
use crate::result::MutationRejection;

const MAX_FIELD: usize = 256;

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

/// The envelope binding principal, grant generation, method, target,
/// artifact digest (optional), full canonical request digest, idempotency
/// key, and an `Option<ApprovalReference>` (populated iff the method's
/// policy tier requires it) — `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md`
/// §6.
///
/// `request_digest` is never supplied independently by a caller: it is
/// always derived by [`MutationAcceptanceRequest::new`] via
/// [`compute_request_digest`] over this struct's own other fields, so a
/// constructed `MutationAcceptanceRequest` can never carry a digest that
/// does not match its own bound fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationAcceptanceRequest {
    principal_sid: String,
    grant_generation: u64,
    method: MutationMethod,
    target_id: String,
    artifact_digest: Option<String>,
    request_digest: RequestDigest,
    idempotency_key: IdempotencyKey,
    approval_reference: Option<ApprovalReference>,
}

impl MutationAcceptanceRequest {
    pub fn new(
        principal_sid: impl Into<String>,
        grant_generation: u64,
        method: MutationMethod,
        target_id: impl Into<String>,
        artifact_digest: Option<String>,
        idempotency_key: IdempotencyKey,
        approval_reference: Option<ApprovalReference>,
    ) -> Result<Self, MutationContractError> {
        let principal_sid = principal_sid.into();
        let target_id = target_id.into();
        if principal_sid.is_empty() || principal_sid.len() > MAX_FIELD {
            return Err(MutationContractError::InvalidInput {
                field: "principal_sid",
                reason: format!("must contain 1..={MAX_FIELD} bytes"),
            });
        }
        let request_digest = compute_request_digest(
            &principal_sid,
            grant_generation,
            method,
            &target_id,
            artifact_digest.as_deref(),
            &idempotency_key,
        )?;
        Ok(Self {
            principal_sid,
            grant_generation,
            method,
            target_id,
            artifact_digest,
            request_digest,
            idempotency_key,
            approval_reference,
        })
    }

    pub fn principal_sid(&self) -> &str {
        &self.principal_sid
    }
    pub fn grant_generation(&self) -> u64 {
        self.grant_generation
    }
    pub fn method(&self) -> MutationMethod {
        self.method
    }
    pub fn target_id(&self) -> &str {
        &self.target_id
    }
    pub fn artifact_digest(&self) -> Option<&str> {
        self.artifact_digest.as_deref()
    }
    pub fn request_digest(&self) -> &RequestDigest {
        &self.request_digest
    }
    pub fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }
    pub fn approval_reference(&self) -> Option<&ApprovalReference> {
        self.approval_reference.as_ref()
    }

    /// Returns an otherwise-identical request with the server-derived approval
    /// reference. Approval metadata is not part of the canonical operation
    /// digest; the operation fields remain immutable.
    pub fn with_approval_reference(&self, approval_reference: Option<ApprovalReference>) -> Self {
        Self {
            principal_sid: self.principal_sid.clone(),
            grant_generation: self.grant_generation,
            method: self.method,
            target_id: self.target_id.clone(),
            artifact_digest: self.artifact_digest.clone(),
            request_digest: self.request_digest.clone(),
            idempotency_key: self.idempotency_key.clone(),
            approval_reference,
        }
    }

    /// Defence in depth at a serialization/acceptance boundary. Safe Rust
    /// callers cannot mutate the bound fields after construction, but this
    /// check keeps any future deserialization or FFI boundary fail-closed.
    pub fn verify_digest_binding(&self) -> Result<(), MutationContractError> {
        let derived = compute_request_digest(
            &self.principal_sid,
            self.grant_generation,
            self.method,
            &self.target_id,
            self.artifact_digest.as_deref(),
            &self.idempotency_key,
        )?;
        if derived != self.request_digest {
            return Err(MutationContractError::InvalidInput {
                field: "request_digest",
                reason: "does not match immutable canonical request fields".to_string(),
            });
        }
        Ok(())
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
            1,
            MutationMethod::AgentRunRegister,
            "target-1",
            None,
            IdempotencyKey::new("idem-1").unwrap(),
            None,
        )
        .unwrap();
        let expected = compute_request_digest(
            "S-1-5-21-principal",
            1,
            MutationMethod::AgentRunRegister,
            "target-1",
            None,
            &IdempotencyKey::new("idem-1").unwrap(),
        )
        .unwrap();
        assert_eq!(request.request_digest, expected);
    }

    #[test]
    fn approval_reference_may_be_bound_to_the_computed_digest() {
        let request = MutationAcceptanceRequest::new(
            "S-1-5-21-principal",
            1,
            MutationMethod::Flash,
            "target-1",
            Some("sha256:artifact".to_string()),
            IdempotencyKey::new("idem-1").unwrap(),
            None,
        )
        .unwrap();
        let reference = approval_ref(&request.request_digest);
        assert_eq!(reference.bound_request_digest, request.request_digest);
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
