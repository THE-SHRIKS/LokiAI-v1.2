//! Real, file-backed integration tests for `loki-acceptance-store`
//! (`Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md` §7/§8): every test in
//! this file opens a real on-disk SQLite database via
//! `loki_storage::Storage::open` (not `:memory:`) and drives
//! `loki_acceptance_store::accept_mutation` through it directly — no
//! mocked repository, no simulated transaction, no fake clock.

use std::sync::{Arc, Barrier};

use loki_acceptance_store::{AcceptanceStoreError, accept_mutation, clock};
use loki_approvals::{ApprovalRequest, ApprovalStore};
use loki_local_authority::{LocalAuthority, LocalContext};
use loki_mutation_contract::{
    AcceptanceResult, ApprovalReference, IdempotencyKey, MutationAcceptanceRequest,
    MutationContractError, MutationMethod, MutationRejection,
};
use loki_storage::Storage;
use loki_storage::agents::AgentsRepository;
use loki_storage::approvals::{ApprovalsRepository, ConsumeOutcome, NewApproval};
use loki_storage::ids::{CorrelationId, JobId, MessageId};
use loki_storage::jobs::{JobRepository, NewJob};
use loki_storage::local_authority::{
    InstallationRow, LocalAuthorityRepository, NewAuthoritySessionRow,
};
use loki_storage::mutation_acceptance::{MutationAcceptanceRepository, NewMutationAcceptance};
use loki_storage::outbox::{NewOutboxMessage, OutboxRepository};
use loki_storage::states::JobState;

const PRINCIPAL_SID: &str = "S-1-5-21-1111111111-2222222222-3333333333-1001";
const AGENT_CONNECTION_ID: &str = "ac_test_connection_1";

/// Opens a real, on-disk, DPAPI-managed SQLCipher-encrypted database (not
/// `:memory:`, not plaintext — `Storage::open` itself refuses
/// `Cipher::None` for a real file path, per `AGENTS.md`'s "no plaintext
/// database" rule) — the same real-file path every sibling crate's own
/// `open`/`restart` tests already exercise.
fn open_storage(dir: &std::path::Path, name: &str) -> Arc<Storage> {
    loki_storage::secure_memory::test_support::ensure_test_arena_ready();
    let path = dir.join(name);
    let key_path = dir.join(format!("{name}.key.dpapi"));
    let storage = Storage::open_managed(&path, &key_path).expect("open real file-backed storage");
    storage
        .run_migrations("1970-01-01T00:00:00Z")
        .expect("run migrations");
    Arc::new(storage)
}

/// Seeds a real `local_installation_authority`/`local_authority_sessions`
/// row pair directly through `loki_storage`'s own repository (this test
/// does not need `loki-local-authority`'s IPC-session business logic —
/// only the durable rows `loki-acceptance-store`'s
/// `find_local_generation_core` reads). Returns `(generation,
/// expires_at_ms)` — `generation` is now the real, storage-assigned
/// durable session generation (`P1-PRINCIPAL-GRANT-001`), not a cast of
/// `issued_at_ms` (the old, replaced provisional shim's mapping).
fn seed_grant(
    storage: &Storage,
    principal_sid: &str,
    issued_at_ms: i64,
    lifetime_ms: i64,
    revoke_at_ms: Option<i64>,
) -> (u64, i64) {
    let _ = storage.insert_installation(&InstallationRow {
        installation_id: "li_test".to_string(),
        owner_sid: principal_sid.to_string(),
        created_at_ms: 0,
    });
    let expires_at_ms = issued_at_ms + lifetime_ms;
    let session_id = format!("ls_test_{issued_at_ms}_{lifetime_ms}");
    // `insert_authority_session_core` always inserts `revoked_at_ms = NULL`
    // (a session is never created pre-revoked — see that function's SQL);
    // simulating "already revoked" here means a real, separate revocation
    // write afterward, exactly like `LocalAuthority::revoke_session` does.
    let generation = storage
        .insert_authority_session(&NewAuthoritySessionRow {
            session_id: session_id.clone(),
            installation_id: "li_test".to_string(),
            principal_sid: principal_sid.to_string(),
            ipc_session_id: "ipc-test".to_string(),
            correlation_id: format!("corr_test_{issued_at_ms}_{lifetime_ms}"),
            issued_at_ms,
            expires_at_ms,
        })
        .expect("insert authority session");
    if let Some(revoke_at_ms) = revoke_at_ms {
        assert!(
            storage
                .revoke_authority_session(&session_id, revoke_at_ms)
                .unwrap(),
            "test setup: revocation must succeed"
        );
    }
    (generation, expires_at_ms)
}

/// Seeds a real `agent_connections` row directly through
/// `loki_storage::agents::AgentsRepository`, mirroring [`seed_grant`]'s
/// direct-repository-seeding style (`P1-PRINCIPAL-GRANT-001` Deliverable 6:
/// the test-harness correction). Returns the connection's real, durable
/// generation — always `1` for a freshly registered connection (`AGT-001`
/// has no expiry concept, so there is no `expires_at_ms` to return).
fn seed_agent_connection_grant(storage: &Storage, connection_id: &str, created_at_ms: i64) -> u64 {
    storage
        .insert_agent_connection(
            connection_id,
            "test-agent",
            "sha256:test-hash",
            "register_runs",
            created_at_ms,
        )
        .expect("insert agent connection");
    1
}

/// A grant generation valid for the real wall clock: `loki-acceptance-store`
/// always reads the real `SystemTime::now()` (see `clock`'s doc comment),
/// so a "currently valid" grant must bracket real "now," not a fixed test
/// timestamp.
fn seed_valid_grant(storage: &Storage, principal_sid: &str) -> u64 {
    let now = clock::unix_millis_now();
    seed_grant(storage, principal_sid, now, 86_400_000, None).0
}

fn agent_request(
    principal_sid: &str,
    generation: u64,
    idem: &str,
    target: &str,
) -> MutationAcceptanceRequest {
    MutationAcceptanceRequest::new(
        principal_sid,
        "agent-1",
        generation,
        MutationMethod::AgentRunRegister,
        target,
        None,
        "sha256:params",
        IdempotencyKey::new(idem).unwrap(),
        None,
    )
    .unwrap()
}

/// Builds a `Flash` request together with a real, granted, unconsumed
/// `local_approvals` row whose `request_digest` exactly matches this
/// request's own computed digest. This low-level helper deliberately seeds
/// the repository only for adversarial expiry/race/fault scenarios that
/// require otherwise-impossible persisted states. The positive integration
/// path below uses `ApprovalStore::create` and never calls this helper.
fn flash_request_with_granted_approval(
    storage: &Storage,
    principal_sid: &str,
    generation: u64,
    idem: &str,
    target: &str,
    approval_id: &str,
    approval_expires_at_ms: i64,
) -> MutationAcceptanceRequest {
    let probe = MutationAcceptanceRequest::new(
        principal_sid,
        "agent-1",
        generation,
        MutationMethod::Flash,
        target,
        Some("sha256:artifact".to_string()),
        "sha256:params",
        IdempotencyKey::new(idem).unwrap(),
        None,
    )
    .unwrap();
    storage
        .insert_approval(&NewApproval {
            approval_id: approval_id.to_string(),
            request_digest: probe.request_digest().as_str().to_string(),
            binding_version: loki_storage::approvals::CURRENT_BINDING_VERSION,
            requester_sid: principal_sid.to_string(),
            actor_id: "agent-1".to_string(),
            grant_generation: generation,
            operation: "flash".to_string(),
            target_id: target.to_string(),
            artifact_digest: Some("sha256:artifact".to_string()),
            parameters_digest: "sha256:params".to_string(),
            idempotency_key: idem.to_string(),
            reason: "test".to_string(),
            requested_at_ms: 0,
            expires_at_ms: approval_expires_at_ms,
        })
        .unwrap();
    assert!(
        storage
            .grant_approval(approval_id, principal_sid, 0)
            .unwrap(),
        "test approval must be freshly granted"
    );
    MutationAcceptanceRequest::new(
        principal_sid,
        "agent-1",
        generation,
        MutationMethod::Flash,
        target,
        Some("sha256:artifact".to_string()),
        "sha256:params",
        IdempotencyKey::new(idem).unwrap(),
        Some(ApprovalReference {
            approval_id: approval_id.to_string(),
            bound_request_digest: probe.request_digest(),
            expires_at_ms: approval_expires_at_ms,
        }),
    )
    .unwrap()
}

/// Builds a granted approval through the production `ApprovalStore::create`
/// path and then submits the exact same immutable binding to mutation
/// acceptance. This prevents a storage-seeded digest from masking drift
/// between approval creation and runtime acceptance.
fn real_flash_request_with_granted_approval(
    storage: &Arc<Storage>,
    idem: &str,
    target: &str,
) -> (MutationAcceptanceRequest, String) {
    let now = clock::unix_millis_now();
    let context = LocalContext::current(format!("ipc-{idem}")).unwrap();
    let authority = LocalAuthority::new(Arc::clone(storage));
    authority.initialize_current(&context, now - 1_000).unwrap();
    let session = authority
        .issue_session(&context, now - 500, 86_400_000)
        .unwrap();
    let approvals = ApprovalStore::new(Arc::clone(storage));
    let approval_request = ApprovalRequest {
        actor_id: "agent-1".to_string(),
        operation: "flash".to_string(),
        target_id: target.to_string(),
        artifact_digest: Some("sha256:artifact".to_string()),
        parameters_digest: "sha256:params".to_string(),
        idempotency_key: idem.to_string(),
        reason: "integration verification".to_string(),
    };
    let record = approvals
        .create(
            &context,
            &session.session_id,
            &approval_request,
            now,
            3_600_000,
        )
        .unwrap();
    approvals
        .grant(&context, &session.session_id, &record.approval_id, now + 1)
        .unwrap();
    let request = MutationAcceptanceRequest::new(
        context.principal_sid(),
        approval_request.actor_id,
        record.grant_generation,
        MutationMethod::Flash,
        approval_request.target_id,
        approval_request.artifact_digest,
        approval_request.parameters_digest,
        IdempotencyKey::new(approval_request.idempotency_key).unwrap(),
        Some(ApprovalReference {
            approval_id: record.approval_id.clone(),
            bound_request_digest: loki_mutation_contract::RequestDigest::try_from_str(
                record.request_digest,
            )
            .unwrap(),
            expires_at_ms: record.expires_at_ms,
        }),
    )
    .unwrap();
    (request, record.approval_id)
}

/// Every test in this file uses fresh, isolated, per-test temp-directory
/// databases with deterministic, test-chosen identifiers — so "no orphan
/// row" is verified by checking the exact expected row is absent (or
/// present exactly once), not a broad `COUNT(*)` scan. Since
/// `mutation_acceptances.idempotency_key`/`jobs.job_id`/
/// `outbox_messages.message_id` are all primary keys, "this specific row
/// exists" and "at most once" are the same fact — nothing else could have
/// written under the same fresh, isolated database.
fn mutation_acceptance_exists(storage: &Storage, idempotency_key: &str) -> bool {
    storage
        .get_mutation_acceptance(idempotency_key)
        .unwrap()
        .is_some()
}

#[test]
fn same_key_same_digest_replays() {
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    // `P1-PRINCIPAL-GRANT-001` Deliverable 6: an `AgentRunRegister` request
    // (this test's method) is an agent-connection principal — seeded
    // through the real `AGT-001` identity model, not `LOC-001`'s
    // Windows-SID sessions (the pre-fix harness bug this deliverable
    // corrects).
    let generation = seed_agent_connection_grant(&storage, AGENT_CONNECTION_ID, 1_000);
    let request = agent_request(AGENT_CONNECTION_ID, generation, "idem-replay-1", "target-1");

    let first = accept_mutation(&storage, &request).unwrap();
    let AcceptanceResult::Accepted(first_record) = &first else {
        panic!("expected Accepted, got {first:?}");
    };

    let second = accept_mutation(&storage, &request).unwrap();
    let AcceptanceResult::ReplayedIdenticalAccepted(second_record) = &second else {
        panic!("expected ReplayedIdenticalAccepted, got {second:?}");
    };
    assert_eq!(first_record.job_id(), second_record.job_id());
    assert_eq!(
        first_record.correlation_id(),
        second_record.correlation_id()
    );
    assert_eq!(
        first_record.accepted_at_ms(),
        second_record.accepted_at_ms()
    );

    // Replay must not create a second mutation_acceptances row (the
    // primary key alone would refuse a duplicate insert; this confirms the
    // one expected row is present).
    assert!(mutation_acceptance_exists(&storage, "idem-replay-1"));
}

#[test]
fn same_key_different_digest_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    let generation = seed_agent_connection_grant(&storage, AGENT_CONNECTION_ID, 1_000);
    let first = agent_request(
        AGENT_CONNECTION_ID,
        generation,
        "idem-conflict-1",
        "target-1",
    );
    accept_mutation(&storage, &first).unwrap();

    let conflicting = agent_request(
        AGENT_CONNECTION_ID,
        generation,
        "idem-conflict-1",
        "target-2",
    );
    let result = accept_mutation(&storage, &conflicting).unwrap();
    assert_eq!(
        result,
        AcceptanceResult::Rejected(MutationRejection::DigestConflict)
    );
}

/// Windows-SID (approval-required-method) grant expiry — regression-proof
/// for the unmodified `LOC-001` path (`P1-PRINCIPAL-GRANT-001` acceptance
/// criterion): a real session's real `expires_at_ms` still denies
/// acceptance exactly as before this node's change. Uses `Flash`
/// (approval-required) rather than `agent_request` so this genuinely
/// exercises the `find_local_generation_core` branch, not the agent one —
/// no approval reference is needed because the grant check (step 1) runs
/// and rejects before the approval check (step 4) is ever reached.
#[test]
fn local_grant_expiry_denies_acceptance_for_approval_required_method() {
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    // Issued and expired far in the past relative to the real wall clock —
    // deterministic without needing to fake `SystemTime`.
    let (generation, _expires) = seed_grant(&storage, PRINCIPAL_SID, 1_000, 60_000, None);
    let request = MutationAcceptanceRequest::new(
        PRINCIPAL_SID,
        "agent-1",
        generation,
        MutationMethod::Flash,
        "target-1",
        Some("sha256:artifact".to_string()),
        "sha256:params",
        IdempotencyKey::new("idem-expired-1").unwrap(),
        None,
    )
    .unwrap();

    let result = accept_mutation(&storage, &request).unwrap();
    assert_eq!(
        result,
        AcceptanceResult::Rejected(MutationRejection::GrantExpired)
    );
    assert!(!mutation_acceptance_exists(&storage, "idem-expired-1"));
}

/// `P1-PRINCIPAL-GRANT-001` acceptance criterion: an agent-connection grant
/// that would have been "expired" under the old shim's timestamp semantics
/// (`generation == issued_at_ms`, seeded far in the "past" here) is still
/// accepted — `AGT-001` has no expiry concept at all — and is denied only
/// once the connection is actually revoked.
#[test]
fn agent_grant_is_revocable_but_never_expires() {
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    // Seeded far in the "past" by every clock measure the old shim would
    // have used — irrelevant here, since this identity model has no
    // expiry field at all to compare against `now_ms`.
    let generation = seed_agent_connection_grant(&storage, AGENT_CONNECTION_ID, 1_000);
    let request = agent_request(
        AGENT_CONNECTION_ID,
        generation,
        "idem-agent-never-expires-1",
        "target-1",
    );

    let accepted = accept_mutation(&storage, &request).unwrap();
    assert!(
        matches!(accepted, AcceptanceResult::Accepted(_)),
        "an agent-connection grant with no expiry concept must still be accepted long after \
         the old shim's timestamp-based semantics would have called it expired, got {accepted:?}"
    );

    assert!(
        storage
            .revoke_agent_connection(AGENT_CONNECTION_ID, 2_000)
            .unwrap(),
        "test setup: revocation must succeed"
    );
    let denied_request = agent_request(
        AGENT_CONNECTION_ID,
        generation,
        "idem-agent-never-expires-2",
        "target-1",
    );
    let result = accept_mutation(&storage, &denied_request).unwrap();
    assert!(
        matches!(result, AcceptanceResult::Rejected(_)),
        "a revoked agent connection must deny acceptance, got {result:?}"
    );
}

/// Windows-SID (approval-required-method) grant revocation — regression-proof
/// for the unmodified `LOC-001` path: revoking the exact session/generation
/// a request names must still be visible and denied, exactly as before this
/// node's change (`LOC-001`'s per-session, immutable-generation row shape
/// keeps the revoked row queryable at that same generation).
#[test]
fn local_grant_revocation_denies_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    let now = clock::unix_millis_now();
    let (generation, _expires) =
        seed_grant(&storage, PRINCIPAL_SID, now, 86_400_000, Some(now + 1));
    let request = MutationAcceptanceRequest::new(
        PRINCIPAL_SID,
        "agent-1",
        generation,
        MutationMethod::Flash,
        "target-1",
        Some("sha256:artifact".to_string()),
        "sha256:params",
        IdempotencyKey::new("idem-revoked-1").unwrap(),
        None,
    )
    .unwrap();

    let result = accept_mutation(&storage, &request).unwrap();
    assert_eq!(
        result,
        AcceptanceResult::Rejected(MutationRejection::GrantRevoked)
    );
}

/// Agent-connection version of the revocation proof above — both identity
/// models must each independently prove their own revocation path is read
/// and enforced (`P1-PRINCIPAL-GRANT-001` Deliverable 6), since that is the
/// exact property this node's per-identity-model read boundary exists to
/// guarantee.
#[test]
fn agent_connection_revocation_denies_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    let generation = seed_agent_connection_grant(&storage, AGENT_CONNECTION_ID, 1_000);
    assert!(
        storage
            .revoke_agent_connection(AGENT_CONNECTION_ID, 2_000)
            .unwrap(),
        "test setup: revocation must succeed"
    );
    let request = agent_request(
        AGENT_CONNECTION_ID,
        generation,
        "idem-agent-revoked-1",
        "target-1",
    );

    let result = accept_mutation(&storage, &request).unwrap();
    assert!(
        matches!(result, AcceptanceResult::Rejected(_)),
        "a revoked agent connection must deny acceptance, got {result:?}"
    );
    assert!(!mutation_acceptance_exists(
        &storage,
        "idem-agent-revoked-1"
    ));
}

#[test]
fn approval_required_without_reference_never_reaches_construct_accepted_bypassed() {
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    let generation = seed_valid_grant(&storage, PRINCIPAL_SID);
    let request = MutationAcceptanceRequest::new(
        PRINCIPAL_SID,
        "agent-1",
        generation,
        MutationMethod::Flash,
        "target-1",
        Some("sha256:artifact".to_string()),
        "sha256:params",
        IdempotencyKey::new("idem-no-ref-1").unwrap(),
        None,
    )
    .unwrap();

    let result = accept_mutation(&storage, &request);
    match result {
        Err(AcceptanceStoreError::Contract(MutationContractError::InvalidInput {
            field: "approval_reference",
            ..
        })) => {}
        other => panic!("expected a construction error naming approval_reference, got {other:?}"),
    }
    // No code path reached far enough to construct an `Accepted`/
    // `ReplayedIdenticalAccepted` result, and nothing was written.
    assert!(!mutation_acceptance_exists(&storage, "idem-no-ref-1"));
}

#[test]
fn agent_method_accepts_without_jobs_row() {
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    let generation = seed_agent_connection_grant(&storage, AGENT_CONNECTION_ID, 1_000);
    let request = agent_request(AGENT_CONNECTION_ID, generation, "idem-agent-1", "target-1");

    let result = accept_mutation(&storage, &request).unwrap();
    let AcceptanceResult::Accepted(record) = &result else {
        panic!("expected Accepted, got {result:?}");
    };
    assert!(
        record.job_id().as_str().starts_with("mjob_"),
        "agent-method job_id must use the synthesized mjob_ prefix, got {}",
        record.job_id().as_str()
    );
    assert!(
        storage.get_job(record.job_id()).unwrap().is_none(),
        "agent methods must never create a real jobs row"
    );
    let acceptance = storage
        .get_mutation_acceptance("idem-agent-1")
        .unwrap()
        .expect("mutation_acceptances row must exist");
    assert_eq!(acceptance.job_id.as_deref(), Some(record.job_id().as_str()));
}

#[test]
fn approval_gated_accept_consumes_approval_and_creates_real_job() {
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    let (request, approval_id) =
        real_flash_request_with_granted_approval(&storage, "idem-flash-1", "target-1");

    let result = accept_mutation(&storage, &request).unwrap();
    let AcceptanceResult::Accepted(record) = &result else {
        panic!("expected Accepted, got {result:?}");
    };
    assert!(record.job_id().as_str().starts_with("job_"));
    assert!(storage.get_job(record.job_id()).unwrap().is_some());
    let approval = storage.get_approval(&approval_id).unwrap().unwrap();
    assert!(
        approval.consumed_at_ms.is_some(),
        "approval must be consumed"
    );
}

#[test]
fn approval_already_expired_by_the_time_the_transaction_runs_is_rejected() {
    // "An approval expiring between request-receipt and lock-acquisition"
    // (Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md §7): simulated here by
    // granting an approval whose `expires_at_ms` is already in the past
    // relative to the real wall clock by the time `accept_mutation` runs
    // (real delay is unnecessary — the approval is simply expired already,
    // which is the exact condition this scenario needs to reproduce).
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    let generation = seed_valid_grant(&storage, PRINCIPAL_SID);
    let request = flash_request_with_granted_approval(
        &storage,
        PRINCIPAL_SID,
        generation,
        "idem-flash-expired-1",
        "target-1",
        "apr_test_expired_1",
        1_000, // already long expired relative to real "now"
    );

    let result = accept_mutation(&storage, &request).unwrap();
    assert_eq!(
        result,
        AcceptanceResult::Rejected(MutationRejection::ApprovalNotConsumable)
    );
    let approval = storage.get_approval("apr_test_expired_1").unwrap().unwrap();
    assert!(
        approval.consumed_at_ms.is_none(),
        "an expired approval must never be consumed"
    );
}

#[test]
fn concurrent_submission_of_the_same_idempotency_key_yields_exactly_one_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    let generation = seed_agent_connection_grant(&storage, AGENT_CONNECTION_ID, 1_000);
    let request = Arc::new(agent_request(
        AGENT_CONNECTION_ID,
        generation,
        "idem-concurrent-1",
        "target-1",
    ));
    let barrier = Arc::new(Barrier::new(2));

    let handles: Vec<_> = (0..2)
        .map(|_| {
            let storage = Arc::clone(&storage);
            let request = Arc::clone(&request);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                accept_mutation(&storage, &request)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    let accepted_count = results
        .iter()
        .filter(|r| matches!(r, Ok(AcceptanceResult::Accepted(_))))
        .count();
    let replayed_count = results
        .iter()
        .filter(|r| matches!(r, Ok(AcceptanceResult::ReplayedIdenticalAccepted(_))))
        .count();
    assert_eq!(accepted_count, 1, "exactly one thread must win Accepted");
    assert_eq!(
        replayed_count, 1,
        "the other thread must observe ReplayedIdenticalAccepted, never a second Accepted"
    );
    assert!(
        mutation_acceptance_exists(&storage, "idem-concurrent-1"),
        "exactly one committed mutation_acceptances row"
    );
}

#[test]
fn revocation_racing_acceptance_yields_exactly_one_legal_outcome() {
    // Real thread interleaving: one thread revokes the grant while another
    // submits a mutation under it. Whichever operation's `Storage::transaction`
    // (or, for revocation, `LocalAuthorityRepository::revoke_authority_session`)
    // wins the single shared `Mutex<Connection>` first determines the
    // outcome deterministically — `Storage`'s single-writer design makes
    // "exactly one legal outcome" structural, not probabilistic.
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    let now = clock::unix_millis_now();
    let (generation, _expires) = seed_grant(&storage, PRINCIPAL_SID, now, 86_400_000, None);
    let session_id = format!("ls_test_{now}_86400000");
    // `Flash` always requires a consumed approval reference (step 4) — a
    // real, granted, unconsumed approval is seeded here so the race's
    // winning outcome, if `accept` wins, can actually reach `Accepted`
    // rather than failing at the unrelated approval-reference check.
    let request = Arc::new(flash_request_with_granted_approval(
        &storage,
        PRINCIPAL_SID,
        generation,
        "idem-revoke-race-1",
        "target-1",
        "apr_revoke_race_1",
        now + 3_600_000,
    ));

    let barrier = Arc::new(Barrier::new(2));
    let accept_handle = {
        let storage = Arc::clone(&storage);
        let request = Arc::clone(&request);
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            accept_mutation(&storage, &request)
        })
    };
    let revoke_handle = {
        let storage = Arc::clone(&storage);
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            storage.revoke_authority_session(&session_id, now + 1)
        })
    };

    let accept_result = accept_handle.join().unwrap().unwrap();
    let _revoke_result = revoke_handle.join().unwrap().unwrap();

    // Whichever ran first, the outcome is legal: either the mutation was
    // accepted before the revocation committed (ADR-0006: pre-commit
    // acceptance is not erased by a later revocation), or the revocation
    // won the race and the acceptance was denied. Never both an `Accepted`
    // result *and* zero evidence of it, and never a panic/deadlock (this
    // line running at all proves no deadlock occurred).
    match accept_result {
        AcceptanceResult::Accepted(_) => {
            assert!(mutation_acceptance_exists(&storage, "idem-revoke-race-1"));
        }
        AcceptanceResult::Rejected(MutationRejection::GrantRevoked) => {
            assert!(!mutation_acceptance_exists(&storage, "idem-revoke-race-1"));
        }
        other => panic!("unexpected outcome under revocation race: {other:?}"),
    }
}

/// Agent-connection version of the race above (`P1-PRINCIPAL-GRANT-001`
/// Deliverable 6/7): one thread revokes an agent connection (which also
/// atomically bumps its generation — Section 5) while another submits an
/// `AgentRunRegister` mutation naming that connection's pre-revoke
/// generation. Unlike `LOC-001`'s immutable-per-session generation, a
/// revoke here mutates the *same* row's generation in place — so the
/// legal outcomes for the losing thread are `Accepted` (raced and won) or
/// `Rejected` for any reason (the pre-revoke generation may no longer
/// match the row at all once revoked, which is still an honest denial —
/// see `crates/loki-storage/src/agents.rs`'s module doc comment on why
/// this differs from `LOC-001`'s `GrantRevoked`-specific outcome). Never a
/// panic/deadlock (this test returning at all proves that).
#[test]
fn agent_connection_revocation_racing_acceptance_yields_exactly_one_legal_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    let generation = seed_agent_connection_grant(&storage, AGENT_CONNECTION_ID, 1_000);
    let request = Arc::new(agent_request(
        AGENT_CONNECTION_ID,
        generation,
        "idem-agent-revoke-race-1",
        "target-1",
    ));

    let barrier = Arc::new(Barrier::new(2));
    let accept_handle = {
        let storage = Arc::clone(&storage);
        let request = Arc::clone(&request);
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            accept_mutation(&storage, &request)
        })
    };
    let revoke_handle = {
        let storage = Arc::clone(&storage);
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            storage.revoke_agent_connection(AGENT_CONNECTION_ID, 2_000)
        })
    };

    let accept_result = accept_handle.join().unwrap().unwrap();
    let _revoke_result = revoke_handle.join().unwrap().unwrap();

    match accept_result {
        AcceptanceResult::Accepted(_) => {
            assert!(mutation_acceptance_exists(
                &storage,
                "idem-agent-revoke-race-1"
            ));
        }
        AcceptanceResult::Rejected(_) => {
            assert!(!mutation_acceptance_exists(
                &storage,
                "idem-agent-revoke-race-1"
            ));
        }
        other => panic!("unexpected outcome under agent revocation race: {other:?}"),
    }
}

/// Fault injection (`Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md` §7/§8):
/// real file-backed `Storage`, a real panic injected inside a real
/// `Storage::transaction()` closure immediately after each of the seven
/// ADR-0006 steps, using the exact same connection-parameterized core
/// functions `loki_acceptance_store::accept_mutation_in_transaction` itself
/// calls (`loki_storage::{local_authority, mutation_acceptance, approvals,
/// jobs, outbox}::*_core`) — not a mock, not a simulated substitute.
/// Reproduced twice (`RUNS`) against the same storage and the same
/// still-unconsumed approval row, proving `Storage::transaction`'s
/// rollback-via-`Drop` leaves the connection fully usable for the next
/// caller every time, not just once.
#[test]
fn fault_injection_after_each_step_leaves_zero_orphan_rows() {
    const RUNS: usize = 2;
    let dir = tempfile::tempdir().unwrap();
    let storage = open_storage(dir.path(), "db1.sqlite3");
    let generation = seed_valid_grant(&storage, PRINCIPAL_SID);

    // The approval's stored `request_digest` must equal the real digest
    // the shared immutable `OperationBinding` derives for the
    // exact request this test eventually submits (`Flash`/"target-1"/no
    // artifact/"idem-fault-1") — not an arbitrary string — so the final
    // successful `accept_mutation` call at the end of this test genuinely
    // exercises `evaluate_approval_reference`'s real digest-match check,
    // not a coincidental pass.
    let probe_digest = MutationAcceptanceRequest::new(
        PRINCIPAL_SID,
        "agent-1",
        generation,
        MutationMethod::Flash,
        "target-1",
        None,
        "sha256:params",
        IdempotencyKey::new("idem-fault-1").unwrap(),
        None,
    )
    .unwrap()
    .request_digest()
    .as_str()
    .to_string();
    storage
        .insert_approval(&NewApproval {
            approval_id: "apr_fault_1".to_string(),
            request_digest: probe_digest.clone(),
            binding_version: loki_storage::approvals::CURRENT_BINDING_VERSION,
            requester_sid: PRINCIPAL_SID.to_string(),
            actor_id: "agent-1".to_string(),
            grant_generation: generation,
            operation: "flash".to_string(),
            target_id: "target-1".to_string(),
            artifact_digest: None,
            parameters_digest: "sha256:params".to_string(),
            idempotency_key: "idem-fault-1".to_string(),
            reason: "test".to_string(),
            requested_at_ms: 0,
            expires_at_ms: clock::unix_millis_now() + 3_600_000,
        })
        .unwrap();
    assert!(
        storage
            .grant_approval("apr_fault_1", PRINCIPAL_SID, 0)
            .unwrap()
    );

    for run in 0..RUNS {
        for step in 1u8..=7 {
            let job_id = JobId::new(format!("job_fault_{run}_{step}")).unwrap();
            let correlation_id = CorrelationId::new(format!("corr_fault_{run}_{step}")).unwrap();
            let message_id = MessageId::new(format!("msg_fault_{run}_{step}")).unwrap();

            let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                storage
                    .transaction(|tx| {
                        // Step 1: revalidate GrantGeneration (read only).
                        // This test's request is `Flash` (approval-required),
                        // i.e. a Windows-SID (`LocalSession`) principal — the
                        // real engine's step 1 for this method reads
                        // `find_local_generation_core`.
                        let _ = loki_storage::local_authority::find_local_generation_core(
                            tx,
                            PRINCIPAL_SID,
                            generation,
                        )?;
                        if step == 1 {
                            panic!("injected fault after step 1");
                        }
                        // Step 2: bind method/digest/idempotency key/target/
                        // artifact/approval inputs — no I/O (already done by
                        // `MutationAcceptanceRequest::new` in the real
                        // engine); this checkpoint proves a fault at this
                        // point also leaves nothing written, trivially or
                        // not.
                        if step == 2 {
                            panic!("injected fault after step 2");
                        }
                        // Step 3: claim or replay the idempotency record
                        // (read only for this "claim" path).
                        let _ = loki_storage::mutation_acceptance::get_mutation_acceptance_core(
                            tx,
                            "idem-fault-1",
                        )?;
                        if step == 3 {
                            panic!("injected fault after step 3");
                        }
                        // Step 4: validate and consume the exact approval.
                        let outcome = loki_storage::approvals::consume_approval_core(
                            tx,
                            "apr_fault_1",
                            &probe_digest,
                            PRINCIPAL_SID,
                            0,
                        )?;
                        assert!(matches!(outcome, ConsumeOutcome::Consumed(_)));
                        if step == 4 {
                            panic!("injected fault after step 4");
                        }
                        // Step 5: create the durable job record.
                        loki_storage::jobs::insert_job_core(
                            tx,
                            &NewJob {
                                job_id: job_id.clone(),
                                state: JobState::Queued,
                                idempotency_key: "idem-fault-1".to_string(),
                                correlation_id: correlation_id.clone(),
                                created_at: "2026-01-01T00:00:00Z".to_string(),
                                updated_at: "2026-01-01T00:00:00Z".to_string(),
                            },
                        )?;
                        if step == 5 {
                            panic!("injected fault after step 5");
                        }
                        // Step 6: append the transactional outbox record.
                        loki_storage::outbox::enqueue_outbox_message_core(
                            tx,
                            &NewOutboxMessage {
                                message_id: message_id.clone(),
                                stream_key: format!("job:{}", job_id.as_str()),
                                message_type: "test".to_string(),
                                correlation_id: correlation_id.clone(),
                                causation_id: None,
                                payload_json: "{}".to_string(),
                                created_at: "2026-01-01T00:00:00Z".to_string(),
                                updated_at: "2026-01-01T00:00:00Z".to_string(),
                            },
                        )?;
                        if step == 6 {
                            panic!("injected fault after step 6");
                        }
                        // Step 7: insert the mutation_acceptances row
                        // (commit boundary is `Storage::transaction`'s own
                        // `tx.commit()`, immediately after this closure
                        // returns `Ok`).
                        loki_storage::mutation_acceptance::insert_mutation_acceptance_core(
                            tx,
                            &NewMutationAcceptance {
                                idempotency_key: "idem-fault-1".to_string(),
                                principal_sid: PRINCIPAL_SID.to_string(),
                                grant_generation: generation,
                                method: "flash".to_string(),
                                target_id: "target-1".to_string(),
                                artifact_digest: None,
                                request_digest: probe_digest.clone(),
                                job_id: Some(job_id.as_str().to_string()),
                                correlation_id: correlation_id.as_str().to_string(),
                                accepted_at_ms: 0,
                            },
                        )?;
                        if step == 7 {
                            panic!("injected fault after step 7 (before commit)");
                        }
                        Ok::<(), loki_storage::StorageError>(())
                    })
                    .ok();
            }));

            assert!(
                panicked.is_err(),
                "run {run} step {step}: injected panic must actually unwind"
            );

            assert!(
                !mutation_acceptance_exists(&storage, "idem-fault-1"),
                "run {run} step {step}: rollback must leave zero orphan mutation_acceptances rows"
            );
            assert!(
                storage.get_job(&job_id).unwrap().is_none(),
                "run {run} step {step}: rollback must leave zero orphan jobs rows"
            );
            assert!(
                storage.get_outbox_message(&message_id).unwrap().is_none(),
                "run {run} step {step}: rollback must leave zero orphan outbox rows"
            );
            let approval = storage.get_approval("apr_fault_1").unwrap().unwrap();
            assert!(
                approval.consumed_at_ms.is_none(),
                "run {run} step {step}: the approval must still be unconsumed after rollback"
            );

            // The connection must remain usable for the very next caller —
            // proven by successfully running an ordinary read here, on the
            // very next statement after an unwound panic inside a
            // transaction on this same `Storage`.
            assert!(storage.get_job(&job_id).unwrap().is_none());
        }
    }

    // After every injected fault across both runs, the storage is still
    // fully functional end to end: a real, successful acceptance still
    // commits normally, consuming the very approval every injected fault
    // above left untouched.
    let final_request = MutationAcceptanceRequest::new(
        PRINCIPAL_SID,
        "agent-1",
        generation,
        MutationMethod::Flash,
        "target-1",
        None,
        "sha256:params",
        IdempotencyKey::new("idem-fault-1").unwrap(),
        Some(ApprovalReference {
            approval_id: "apr_fault_1".to_string(),
            bound_request_digest: loki_mutation_contract::RequestDigest::try_from_str(
                probe_digest.clone(),
            )
            .unwrap(),
            expires_at_ms: clock::unix_millis_now() + 3_600_000,
        }),
    )
    .unwrap();
    let result = accept_mutation(&storage, &final_request).unwrap();
    assert!(matches!(result, AcceptanceResult::Accepted(_)));
    let approval = storage.get_approval("apr_fault_1").unwrap().unwrap();
    assert!(approval.consumed_at_ms.is_some());
}
