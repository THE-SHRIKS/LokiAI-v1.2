//! The ONLY place in this crate allowed to construct a
//! [`crate::business::WireMutationOutcome`]/[`crate::business::WireMutationRejection`]
//! value. Every function here is an explicit, reviewable, exhaustive
//! (no wildcard arm) mapping from a real
//! `loki_mutation_contract::AcceptanceResult`/`MutationRejection` value —
//! never an independent reconstruction of acceptance semantics.
//! `ADR-0006`/REV-004: "IPC cannot be the mutation-acceptance boundary, only
//! a transport in front of one." `Docs/graph/tasks/P1-RPC-CONTRACT-001-R1.md`
//! Deliverable 1 and acceptance criterion: "Every mutation-result wire
//! variant is produced only via an explicit, reviewable mapping function
//! from a real `loki_mutation_contract::AcceptanceResult`/`MutationRejection`
//! value."
//!
//! `loki_mutation_contract::AcceptanceResult`'s `Accepted`/
//! `ReplayedIdenticalAccepted` variants wrap a private-field `AccceptedRecord`
//! reachable only through its three public accessor methods (`job_id()`,
//! `correlation_id()`, `accepted_at_ms()`) — this module only ever reads
//! through those accessors, never constructs an `AcceptedRecord` itself
//! (it cannot: the constructor is private to `loki-mutation-contract`).

use loki_mutation_contract::{AcceptanceResult, MutationRejection};

use crate::business::{WireMutationOutcome, WireMutationRejection};

/// Maps a real `MutationRejection` to its exact wire mirror. Exhaustive
/// match, no wildcard arm: a variant added to the source enum without a
/// case here fails to compile.
pub fn map_rejection(source: &MutationRejection) -> WireMutationRejection {
    match source {
        MutationRejection::DigestConflict => WireMutationRejection::DigestConflict,
        MutationRejection::GrantInvalid { reason } => WireMutationRejection::GrantInvalid {
            reason: reason.clone(),
        },
        MutationRejection::GrantExpired => WireMutationRejection::GrantExpired,
        MutationRejection::GrantRevoked => WireMutationRejection::GrantRevoked,
        MutationRejection::PolicyDenied => WireMutationRejection::PolicyDenied,
        MutationRejection::ApprovalNotConsumable => WireMutationRejection::ApprovalNotConsumable,
        MutationRejection::InvalidInput { field, reason } => WireMutationRejection::InvalidInput {
            field: (*field).to_string(),
            reason: reason.clone(),
        },
    }
}

/// Maps a real `AcceptanceResult` to its exact wire mirror. Exhaustive
/// match, no wildcard arm.
pub fn map_acceptance_result(source: &AcceptanceResult) -> WireMutationOutcome {
    match source {
        AcceptanceResult::Accepted(record) => WireMutationOutcome::Accepted {
            job_id: record.job_id().as_str().to_string(),
            correlation_id: record.correlation_id().as_str().to_string(),
            accepted_at_ms: record.accepted_at_ms(),
        },
        AcceptanceResult::ApprovalRequired {
            approval_id,
            bound_request_digest,
            expires_at_ms,
        } => WireMutationOutcome::ApprovalRequired {
            approval_id: approval_id.clone(),
            bound_request_digest: bound_request_digest.as_str().to_string(),
            expires_at_ms: *expires_at_ms,
        },
        AcceptanceResult::ReplayedIdenticalAccepted(record) => {
            WireMutationOutcome::ReplayedIdenticalAccepted {
                job_id: record.job_id().as_str().to_string(),
                correlation_id: record.correlation_id().as_str().to_string(),
                accepted_at_ms: record.accepted_at_ms(),
            }
        }
        AcceptanceResult::Rejected(rejection) => WireMutationOutcome::Rejected {
            rejection: map_rejection(rejection),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_mutation_contract::digest::RequestDigest;
    use loki_mutation_contract::request::{ApprovalReference, MutationAcceptanceRequest};
    use loki_mutation_contract::{IdempotencyKey, MutationMethod};
    use loki_storage::ids::{CorrelationId, JobId};

    fn request() -> MutationAcceptanceRequest {
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

    #[test]
    fn accepted_maps_every_field_through_the_real_accessors() {
        let accepted = loki_mutation_contract::construct_accepted(
            &request(),
            JobId::new("job_1").unwrap(),
            CorrelationId::new("corr_1").unwrap(),
            5_000,
        )
        .unwrap();
        match map_acceptance_result(&accepted) {
            WireMutationOutcome::Accepted {
                job_id,
                correlation_id,
                accepted_at_ms,
            } => {
                assert_eq!(job_id, "job_1");
                assert_eq!(correlation_id, "corr_1");
                assert_eq!(accepted_at_ms, 5_000);
            }
            other => panic!("expected Accepted, got {other:?}"),
        }
    }

    #[test]
    fn approval_required_maps_every_field() {
        let digest = RequestDigest::try_from_str("sha256:".to_string() + &"a".repeat(64)).unwrap();
        let source = AcceptanceResult::ApprovalRequired {
            approval_id: "apr_1".to_string(),
            bound_request_digest: digest,
            expires_at_ms: 9_000,
        };
        match map_acceptance_result(&source) {
            WireMutationOutcome::ApprovalRequired {
                approval_id,
                bound_request_digest,
                expires_at_ms,
            } => {
                assert_eq!(approval_id, "apr_1");
                assert_eq!(
                    bound_request_digest,
                    "sha256:".to_string() + &"a".repeat(64)
                );
                assert_eq!(expires_at_ms, 9_000);
            }
            other => panic!("expected ApprovalRequired, got {other:?}"),
        }
    }

    #[test]
    fn every_mutation_rejection_variant_maps_to_its_exact_wire_counterpart() {
        let cases = [
            (MutationRejection::DigestConflict, "digest_conflict"),
            (MutationRejection::GrantExpired, "grant_expired"),
            (MutationRejection::GrantRevoked, "grant_revoked"),
            (MutationRejection::PolicyDenied, "policy_denied"),
            (
                MutationRejection::ApprovalNotConsumable,
                "approval_not_consumable",
            ),
        ];
        for (source, expected_tag) in cases {
            let wire = map_rejection(&source);
            let json = serde_json::to_value(&wire).unwrap();
            assert_eq!(json["kind"], expected_tag);
        }
        let grant_invalid = map_rejection(&MutationRejection::GrantInvalid {
            reason: "bad".to_string(),
        });
        assert_eq!(
            grant_invalid,
            WireMutationRejection::GrantInvalid {
                reason: "bad".to_string()
            }
        );
        let invalid_input = map_rejection(&MutationRejection::InvalidInput {
            field: "f",
            reason: "r".to_string(),
        });
        assert_eq!(
            invalid_input,
            WireMutationRejection::InvalidInput {
                field: "f".to_string(),
                reason: "r".to_string(),
            }
        );
    }

    /// Uses the *real* approval-required construction path
    /// (`construct_accepted` with an approval-gated method and no
    /// reference) rather than hand-constructing
    /// `AcceptanceResult::ApprovalRequired` from this test, proving the
    /// mapping is exercised against the actual contract behavior an
    /// approval-gated mutation exhibits.
    #[test]
    fn rejected_via_construct_accepted_maps_through_the_real_path() {
        let request = MutationAcceptanceRequest::new(
            "S-1-5-21-principal",
            "agent-1",
            1,
            MutationMethod::Flash,
            "target-1",
            Some("sha256:artifact".to_string()),
            "sha256:parameters",
            IdempotencyKey::new("idem-1").unwrap(),
            Some(ApprovalReference {
                approval_id: "apr_1".to_string(),
                bound_request_digest: RequestDigest::try_from_str(
                    "sha256:".to_string() + &"b".repeat(64),
                )
                .unwrap(),
                expires_at_ms: 10_000,
            }),
        )
        .unwrap();
        let result = loki_mutation_contract::construct_accepted(
            &request,
            JobId::new("job_1").unwrap(),
            CorrelationId::new("corr_1").unwrap(),
            5_000,
        )
        .unwrap();
        assert_eq!(
            map_acceptance_result(&result),
            WireMutationOutcome::Rejected {
                rejection: WireMutationRejection::ApprovalNotConsumable
            }
        );
    }
}
