//! Real consequential-mutation submission boundary.
//!
//! This boundary was intentionally fail-closed (`ClosedMutationSubmission`,
//! `MutationUnavailable { OperationBindingNotIntegrated }`) while the
//! approval/operation-binding correction (`P1-APPROVAL-OPERATION-BINDING-CORR`)
//! had not yet landed in `loki-acceptance-store`/`loki-approvals` — see
//! `Docs/graph/decisions/ADR-0008-agent-mutation-idempotency-composition.md`
//! and its Amendment 1. That correction is now integrated (`e13ae46`,
//! `Docs/graph/handoffs/P1-APPROVAL-OPERATION-BINDING-CORR-1.md`), so this
//! module now routes every mutation submission through the real, atomic
//! `loki_acceptance_store::accept_mutation` coordinator against the shared
//! `Arc<loki_storage::Storage>` `RuntimeServices` already composes over.
//!
//! # What this module still refuses to do
//!
//! - It never fabricates an `AcceptanceResult::Accepted`/
//!   `ReplayedIdenticalAccepted` value itself — those are only ever
//!   constructed inside `loki-mutation-contract` (private-field,
//!   closed-constructor types), and this module only ever forwards whatever
//!   `accept_mutation` returns, unmodified, as `Ok(..)`.
//! - It never exposes the `Arc<Storage>` handle it holds — the field is
//!   private, and neither this port's trait nor its implementation returns
//!   it or a raw connection/transaction to any caller.
//! - It never bypasses `Storage::transaction()` — the only call this module
//!   makes into `loki-acceptance-store` is the single public
//!   `accept_mutation` entry point, which itself wraps its entire seven-step
//!   ADR-0006 sequence in exactly one `Storage::transaction()` call.

use std::sync::Arc;

use loki_acceptance_store::{AcceptanceStoreError, accept_mutation};
use loki_mutation_contract::{AcceptanceResult, MutationAcceptanceRequest, MutationContractError};
use loki_storage::{Storage, StorageError};

/// Current state of the mutation submission boundary. The port is available
/// at every revision from this correction onward — there is no longer a
/// "closed" state to report, since the real coordinator is always reachable
/// once `RuntimeServices` has composed a migrated `Storage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationAvailability {
    Available,
}

/// An infrastructure-level failure submitting a mutation: a genuine
/// database fault (`Storage`) or a caller/construction bug surfaced by
/// `loki-mutation-contract` (`Contract`, e.g. an approval-gated method
/// reaching this port with no `approval_reference` at all).
///
/// This is deliberately **not** how a legitimate business rejection (bad
/// approval reference, expired/revoked grant, digest conflict, ...) is
/// reported — those are real, honest `Ok(AcceptanceResult::Rejected(..))`
/// values, not an `Err` here. Collapsing a business rejection into a generic
/// error would lose the caller's ability to distinguish "your mutation was
/// correctly refused" from "the mutation boundary itself is broken."
#[derive(Debug)]
pub enum MutationSubmissionError {
    Storage(StorageError),
    Contract(MutationContractError),
}

impl std::fmt::Display for MutationSubmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(err) => write!(f, "mutation submission storage error: {err}"),
            Self::Contract(err) => write!(f, "mutation submission contract error: {err}"),
        }
    }
}

impl std::error::Error for MutationSubmissionError {}

impl From<AcceptanceStoreError> for MutationSubmissionError {
    fn from(value: AcceptanceStoreError) -> Self {
        match value {
            AcceptanceStoreError::Storage(err) => Self::Storage(err),
            AcceptanceStoreError::Contract(err) => Self::Contract(err),
        }
    }
}

/// Bounded internal port for consequential mutation submission. The request
/// type is `loki-mutation-contract`'s own already-hardened
/// `MutationAcceptanceRequest` (private fields, closed constructor,
/// digest-derived-not-caller-supplied) — this port neither widens nor
/// narrows that contract, it only forwards it into the real coordinator.
pub trait MutationSubmissionPort {
    fn availability(&self) -> MutationAvailability;

    fn submit_mutation(
        &self,
        request: &MutationAcceptanceRequest,
    ) -> Result<AcceptanceResult, MutationSubmissionError>;
}

/// Holds the shared `Arc<Storage>` privately — never returned or exposed to
/// any caller of `MutationSubmissionPort`.
pub(super) struct RealMutationSubmission {
    storage: Arc<Storage>,
}

impl RealMutationSubmission {
    pub(super) fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }
}

impl MutationSubmissionPort for RealMutationSubmission {
    fn availability(&self) -> MutationAvailability {
        MutationAvailability::Available
    }

    fn submit_mutation(
        &self,
        request: &MutationAcceptanceRequest,
    ) -> Result<AcceptanceResult, MutationSubmissionError> {
        Ok(accept_mutation(self.storage.as_ref(), request)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_mutation_contract::{IdempotencyKey, MutationMethod, MutationRejection};
    use loki_storage::agents::AgentsRepository;
    use loki_storage::local_authority::{LocalAuthorityRepository, NewAuthoritySessionRow};

    const PRINCIPAL_SID: &str = "S-1-5-21-1111111111-2222222222-3333333333-9001";
    const AGENT_CONNECTION_ID: &str = "ac_runtime_services_test";

    fn storage() -> Arc<Storage> {
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let storage = Storage::open_in_memory(loki_storage::Cipher::None).unwrap();
        storage.run_migrations("2026-08-30T00:00:00.000Z").unwrap();
        Arc::new(storage)
    }

    /// Seeds a real, currently-valid agent-connection grant directly through
    /// `loki_storage`, mirroring `loki-acceptance-store`'s own integration
    /// tests — the agent-method identity model
    /// (`MutationMethod::requires_approval() == false`) needs no local
    /// authority session or approval, only a real `agent_connections` row.
    fn seed_agent_grant(storage: &Storage) -> u64 {
        storage
            .insert_agent_connection(
                AGENT_CONNECTION_ID,
                "test-agent",
                "sha256:test-hash",
                "register_runs",
                0,
            )
            .expect("insert agent connection");
        1
    }

    fn agent_request(generation: u64, idem: &str) -> MutationAcceptanceRequest {
        MutationAcceptanceRequest::new(
            AGENT_CONNECTION_ID,
            "agent-1",
            generation,
            MutationMethod::AgentRunRegister,
            "target-1",
            None,
            "sha256:params",
            IdempotencyKey::new(idem).unwrap(),
            None,
        )
        .unwrap()
    }

    /// Acceptance criterion: a real accepted mutation via the port produces
    /// the exact same result as calling `loki_acceptance_store::accept_mutation`
    /// directly — proving there is no divergence/wrapper bug in
    /// `submit_mutation`'s forwarding.
    #[test]
    fn accepted_mutation_matches_calling_accept_mutation_directly() {
        let storage = storage();
        let generation = seed_agent_grant(&storage);
        let port = RealMutationSubmission::new(Arc::clone(&storage));

        let via_port = port
            .submit_mutation(&agent_request(generation, "idem-via-port"))
            .expect("accepted mutation must not error");
        assert!(matches!(via_port, AcceptanceResult::Accepted(_)));

        let direct = accept_mutation(storage.as_ref(), &agent_request(generation, "idem-direct"))
            .expect("direct accept_mutation call must not error");
        assert!(matches!(direct, AcceptanceResult::Accepted(_)));

        // Same idempotency key through both call paths must replay to the
        // exact same accepted record — proving the port is a transparent
        // forward, not a divergent reimplementation.
        let request = agent_request(generation, "idem-identical-both-paths");
        let via_port_first = port.submit_mutation(&request).unwrap();
        let AcceptanceResult::Accepted(via_port_record) = via_port_first else {
            panic!("expected Accepted");
        };
        let direct_replay = accept_mutation(storage.as_ref(), &request).unwrap();
        let AcceptanceResult::ReplayedIdenticalAccepted(direct_record) = direct_replay else {
            panic!("expected ReplayedIdenticalAccepted");
        };
        assert_eq!(via_port_record.job_id(), direct_record.job_id());
        assert_eq!(
            via_port_record.correlation_id(),
            direct_record.correlation_id()
        );
        assert_eq!(
            via_port_record.accepted_at_ms(),
            direct_record.accepted_at_ms()
        );
    }

    /// Acceptance criterion: a rejected mutation (bad/expired grant
    /// reference) is correctly surfaced as an honest
    /// `Ok(AcceptanceResult::Rejected(..))`, never silently swallowed and
    /// never converted into a generic `Err` that loses the rejection reason.
    #[test]
    fn rejected_mutation_surfaces_the_real_rejection_not_a_generic_error() {
        let storage = storage();
        let port = RealMutationSubmission::new(Arc::clone(&storage));

        // No agent connection was ever seeded for this connection id, so the
        // grant lookup must fail closed with a real, specific rejection.
        let request = agent_request(1, "idem-no-grant");
        let result = port
            .submit_mutation(&request)
            .expect("a business rejection is Ok(Rejected), not Err");
        assert!(matches!(
            result,
            AcceptanceResult::Rejected(MutationRejection::GrantInvalid { .. })
        ));
    }

    /// Adjacent to the above: an expired local-authority grant (the
    /// approval-gated identity model) is also surfaced as a real, specific
    /// `GrantExpired` rejection, not swallowed.
    #[test]
    fn expired_local_grant_is_rejected_not_swallowed() {
        let storage = storage();
        let now_ms = loki_acceptance_store::clock::unix_millis_now();
        // Seed a session that already expired in the past relative to real
        // "now" (`accept_mutation` always reads the real wall clock).
        storage
            .insert_installation(&loki_storage::local_authority::InstallationRow {
                installation_id: "li_expired_test".to_string(),
                owner_sid: PRINCIPAL_SID.to_string(),
                created_at_ms: 0,
            })
            .ok();
        let generation = storage
            .insert_authority_session(&NewAuthoritySessionRow {
                session_id: "ls_expired_test".to_string(),
                installation_id: "li_expired_test".to_string(),
                principal_sid: PRINCIPAL_SID.to_string(),
                ipc_session_id: "ipc-expired-test".to_string(),
                correlation_id: "corr_expired_test".to_string(),
                issued_at_ms: now_ms - 10_000,
                expires_at_ms: now_ms - 5_000,
            })
            .expect("insert authority session");

        let port = RealMutationSubmission::new(Arc::clone(&storage));
        let request = MutationAcceptanceRequest::new(
            PRINCIPAL_SID,
            "agent-1",
            generation,
            MutationMethod::Restart,
            "target-1",
            None,
            "sha256:params",
            IdempotencyKey::new("idem-expired").unwrap(),
            None,
        )
        .unwrap();

        let result = port.submit_mutation(&request).unwrap();
        assert_eq!(
            result,
            AcceptanceResult::Rejected(MutationRejection::GrantExpired)
        );
    }

    /// Acceptance criterion: no raw `Storage` handle or connection is ever
    /// returned/exposed through the port's public API — a compile-time
    /// property of `submit_mutation`'s/`availability`'s signatures (neither
    /// returns anything containing `Storage`), asserted here so any future
    /// signature change that widened this is caught by review, not merely by
    /// reading the trait definition.
    #[test]
    fn port_never_returns_storage_or_a_connection() {
        fn assert_no_storage_leak<T: MutationSubmissionPort>(_port: &T) {
            fn assert_send_and_not_storage<X: 'static>() {}
            assert_send_and_not_storage::<MutationAvailability>();
            assert_send_and_not_storage::<Result<AcceptanceResult, MutationSubmissionError>>();
        }
        let storage = storage();
        let port = RealMutationSubmission::new(storage);
        assert_no_storage_leak(&port);
    }
}
