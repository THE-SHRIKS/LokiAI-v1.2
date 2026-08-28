//! Crash-safety proof for the agent-mutation two-call composition
//! (`Docs/graph/tasks/P1-AGENT-MUTATION-IDEMP-001.md` Deliverable 2,
//! `Docs/graph/decisions/ADR-0008-agent-mutation-idempotency-composition.md`
//! Option B).
//!
//! ADR-0008 declined to extend `loki-mutation-contract` with method-specific
//! payload/result fields (Option A) so the three agent methods
//! (`AgentRunRegister`, `AgentRunUpdate`, `WorkIntentAcknowledge`) could run
//! their real domain effect inside `loki_acceptance_store::accept_mutation`'s
//! own single transaction. Instead, this file proves the *pattern* Option B
//! chose is safe: call `accept_mutation` (the generic ADR-0006
//! grant/idempotency/outbox gate, unmodified) first, then call the real,
//! already-public `loki_agents::AgentRegistry`/
//! `loki_agent_delivery::WorkIntentDeliveryStore` method second, as two
//! independent, non-atomic calls — and a crash between them (or after the
//! second commits but before the caller observes the result) never leaves
//! the system in a state where retrying both calls from the top produces a
//! different, inconsistent, or duplicated outcome than the original attempt
//! would have.
//!
//! # No new production "composition glue" code
//!
//! This file calls both real public APIs directly, exactly as the eventual
//! real RPC/IPC caller will. It does not introduce a reusable function or
//! crate that wraps `accept_mutation` plus the real business call into one
//! call site — no such caller exists in the repository yet, and the wire
//! shape of a future request that carries both the generic
//! `MutationAcceptanceRequest` fields and the method-specific business
//! fields (`secret`, `parent_run_id`, event data, ...) is a distinct future
//! RPC/contract node's decision, not this node's to invent speculatively
//! (`P1-AGENT-MUTATION-IDEMP-001.md` §1).
//!
//! # Real crash simulation, not sequential-only assertions
//!
//! Every test below opens a real, on-disk, DPAPI-managed SQLCipher-encrypted
//! `Storage` (mirroring `acceptance_flow.rs`'s `open_storage` helper
//! exactly — not `:memory:`), performs step 1 (`accept_mutation`), then
//! drops every `Arc<Storage>`/`AgentRegistry`/`WorkIntentDeliveryStore`
//! handle held so far so no in-process state survives, then opens a **new**
//! `Storage::open_managed` over the same on-disk path/key and fresh
//! `AgentRegistry`/`WorkIntentDeliveryStore` instances before ever calling
//! the real business method — this is what makes the proof about real
//! durable persistence surviving a process restart, not merely in-process
//! object lifetime.

use std::sync::Arc;

use loki_agent_delivery::{AcknowledgeRequest, WorkIntentDeliveryStore};
use loki_agents::{AgentRegistry, AgentScope, RunState};
use loki_connectors_sdk::ids::CapabilityId;
use loki_incidents::ids::IncidentId;
use loki_incidents::work_intent::{DeliveryRequirements, WorkIntent};
use loki_local_authority::{LocalAuthority, LocalContext};
use loki_mutation_contract::{
    AcceptanceResult, IdempotencyKey, MutationAcceptanceRequest, MutationMethod,
};
use loki_storage::Storage;
use std::time::{Duration, SystemTime};

/// Opens a real, on-disk, DPAPI-managed SQLCipher-encrypted database (not
/// `:memory:`) — identical to `acceptance_flow.rs`'s `open_storage` helper
/// (duplicated here rather than shared, since this node does not edit that
/// file — `P1-AGENT-MUTATION-IDEMP-001.md` §5 ownership).
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

/// Registers a real agent connection through the real `AgentRegistry::register`
/// API (not the fabricated-secret-hash `seed_agent_connection_grant` helper
/// `acceptance_flow.rs` uses for its own, non-authenticating tests) so this
/// file's tests can both (a) have `accept_mutation`'s grant read succeed
/// against the real `agent_connections` row and (b) authenticate the real
/// business call with a real, returned secret — `P1-AGENT-MUTATION-IDEMP-001.md`
/// §3's documented reason `acceptance_flow.rs`'s own helper cannot be reused
/// here.
fn register_agent_connection(storage: Arc<Storage>) -> (AgentRegistry, String, String, u64) {
    let authority = LocalAuthority::open_in_memory().unwrap();
    let context = LocalContext::current("ipc-composition-test").unwrap();
    authority.initialize_current(&context, 100).unwrap();
    let session = authority.issue_session(&context, 200, 100_000).unwrap();
    let registry = AgentRegistry::new(storage);
    let credential = registry
        .register(
            &authority,
            &context,
            &session.session_id,
            "composition-test-agent",
            &[AgentScope::RegisterRuns, AgentScope::ReceiveWorkIntents],
            300,
        )
        .unwrap();
    let secret = credential
        .secret
        .expose_for_client_configuration()
        .to_string();
    (
        registry,
        credential.connection_id,
        secret,
        credential.generation,
    )
}

fn agent_mutation_request(
    connection_id: &str,
    generation: u64,
    method: MutationMethod,
    target_id: &str,
    idem: &str,
) -> MutationAcceptanceRequest {
    MutationAcceptanceRequest::new(
        connection_id,
        generation,
        method,
        target_id,
        None,
        IdempotencyKey::new(idem).unwrap(),
        None,
    )
    .unwrap()
}

fn assert_same_accepted(first: &AcceptanceResult, retry: &AcceptanceResult) {
    let AcceptanceResult::Accepted(first_record) = first else {
        panic!("expected first attempt Accepted, got {first:?}");
    };
    let AcceptanceResult::ReplayedIdenticalAccepted(retry_record) = retry else {
        panic!("expected retry ReplayedIdenticalAccepted, got {retry:?}");
    };
    assert_eq!(first_record.job_id(), retry_record.job_id());
    assert_eq!(first_record.correlation_id(), retry_record.correlation_id());
    assert_eq!(first_record.accepted_at_ms(), retry_record.accepted_at_ms());
}

/// `MutationMethod::AgentRunRegister` + `AgentRegistry::start_run`.
#[test]
fn agent_run_register_composition_survives_restart_between_accept_and_apply() {
    let dir = tempfile::tempdir().unwrap();
    let db_name = "agent_run_register.sqlite3";

    let (connection_id, secret, generation, first_accept) = {
        let storage = open_storage(dir.path(), db_name);
        let (_, connection_id, secret, generation) = register_agent_connection(storage.clone());

        // Step 1: the generic ADR-0006 acceptance gate.
        let request = agent_mutation_request(
            &connection_id,
            generation,
            MutationMethod::AgentRunRegister,
            "run-target-1",
            "idem-run-register-1",
        );
        let accepted = loki_acceptance_store::accept_mutation(&storage, &request).unwrap();
        assert!(
            matches!(accepted, AcceptanceResult::Accepted(_)),
            "expected Accepted, got {accepted:?}"
        );

        // Simulate the crash: every handle held so far goes out of scope
        // here (storage, registry are dropped at the end of this block).
        (connection_id, secret, generation, accepted)
    };

    // Simulate the restart: fresh Storage handle, fresh AgentRegistry, over
    // the same on-disk file.
    let storage2 = open_storage(dir.path(), db_name);
    let registry2 = AgentRegistry::new(storage2.clone());

    // Step 2, first real attempt (post-restart): the real domain effect.
    let run_idem_key = "run-register-business-idem-1";
    let run1 = registry2
        .start_run(&connection_id, &secret, run_idem_key, None, 5_000)
        .unwrap();

    // Full retry of both calls, simulating "the caller never learned step 2
    // succeeded and retries everything from the top": the retried
    // accept_mutation call must replay the exact pre-crash Accepted result.
    let request = agent_mutation_request(
        &connection_id,
        generation,
        MutationMethod::AgentRunRegister,
        "run-target-1",
        "idem-run-register-1",
    );
    let retry_accept = loki_acceptance_store::accept_mutation(&storage2, &request).unwrap();
    assert_same_accepted(&first_accept, &retry_accept);

    let run2 = registry2
        .start_run(&connection_id, &secret, run_idem_key, None, 6_000)
        .unwrap();
    assert_eq!(
        run1, run2,
        "retrying start_run with the same idempotency_key must replay the identical run, not create a second one"
    );

    // Direct repository read: exactly one durable agent_runs row for this
    // connection/idempotency key.
    let count: i64 = storage2
        .transaction(|tx| {
            tx.query_row(
                "SELECT COUNT(*) FROM agent_runs WHERE connection_id = ?1 AND idempotency_key = ?2",
                rusqlite::params![connection_id, run_idem_key],
                |row| row.get(0),
            )
            .map_err(loki_storage::StorageError::from)
        })
        .unwrap();
    assert_eq!(
        count, 1,
        "exactly one agent_runs row must exist, no duplication across both attempts"
    );
}

/// `MutationMethod::AgentRunUpdate` + `AgentRegistry::start_run` (to create
/// the run first, outside the measured composition) then `finish_run` as the
/// measured second call.
///
/// This test is confirmed to fail red if `finish_run`'s
/// `P1-AGENT-MUTATION-IDEMP-001` Deliverable 1 fix is reverted: without the
/// fix, the retry's `finish_run` call below would return
/// `Err(AgentError::TerminalRun)` instead of `Ok(())`, since the run is
/// already terminal from the first post-restart attempt.
#[test]
fn agent_run_update_composition_survives_restart_between_accept_and_apply() {
    let dir = tempfile::tempdir().unwrap();
    let db_name = "agent_run_update.sqlite3";

    let (connection_id, secret, generation, run_id, first_accept) = {
        let storage = open_storage(dir.path(), db_name);
        let (registry, connection_id, secret, generation) =
            register_agent_connection(storage.clone());

        // Outside the measured composition: create the run to be finished.
        let run = registry
            .start_run(&connection_id, &secret, "run-to-finish-1", None, 1_000)
            .unwrap();

        // Step 1: the generic ADR-0006 acceptance gate, targeting the real
        // run_id this composition will transition.
        let request = agent_mutation_request(
            &connection_id,
            generation,
            MutationMethod::AgentRunUpdate,
            &run.run_id,
            "idem-run-update-1",
        );
        let accepted = loki_acceptance_store::accept_mutation(&storage, &request).unwrap();
        assert!(
            matches!(accepted, AcceptanceResult::Accepted(_)),
            "expected Accepted, got {accepted:?}"
        );

        (connection_id, secret, generation, run.run_id, accepted)
    };

    // Simulate the restart.
    let storage2 = open_storage(dir.path(), db_name);
    let registry2 = AgentRegistry::new(storage2.clone());

    // Step 2, first real attempt (post-restart).
    registry2
        .finish_run(&connection_id, &secret, &run_id, RunState::Failed, 7_000)
        .unwrap();

    // Full retry of both calls: the retried accept_mutation call must
    // replay the exact pre-crash Accepted result.
    let request = agent_mutation_request(
        &connection_id,
        generation,
        MutationMethod::AgentRunUpdate,
        &run_id,
        "idem-run-update-1",
    );
    let retry_accept = loki_acceptance_store::accept_mutation(&storage2, &request).unwrap();
    assert_same_accepted(&first_accept, &retry_accept);

    // This is the call that fails red without Deliverable 1's fix.
    registry2
        .finish_run(&connection_id, &secret, &run_id, RunState::Failed, 8_000)
        .unwrap();

    let reloaded = registry2
        .require_run_for_connection(&connection_id, &run_id)
        .unwrap();
    assert_eq!(reloaded.state, RunState::Failed);
    assert_eq!(
        reloaded.completed_at_ms,
        Some(7_000),
        "replay must not advance completed_at_ms to the retry's now_ms"
    );

    // Direct repository read: exactly one row, in the terminal state, for
    // this run_id (a PRIMARY KEY, so "this row exists" and "at most once"
    // are the same fact).
    let count: i64 = storage2
        .transaction(|tx| {
            tx.query_row(
                "SELECT COUNT(*) FROM agent_runs WHERE run_id = ?1 AND state = 'failed'",
                rusqlite::params![run_id],
                |row| row.get(0),
            )
            .map_err(loki_storage::StorageError::from)
        })
        .unwrap();
    assert_eq!(count, 1);
}

/// `MutationMethod::WorkIntentAcknowledge` + a real enqueue-claim-acknowledge
/// flow + `WorkIntentDeliveryStore::acknowledge` as the measured second call.
#[test]
fn work_intent_acknowledge_composition_survives_restart_between_accept_and_apply() {
    let dir = tempfile::tempdir().unwrap();
    let db_name = "work_intent_ack.sqlite3";

    let (connection_id, secret, generation, work_intent_id, run_id, revision, first_accept) = {
        let storage = open_storage(dir.path(), db_name);
        let (registry, connection_id, secret, generation) =
            register_agent_connection(storage.clone());
        let delivery = WorkIntentDeliveryStore::new(storage.clone());

        let intent = WorkIntent::emit(
            IncidentId::generate(),
            CapabilityId::new("read_health").unwrap(),
            &[],
            DeliveryRequirements::new("investigate locally").unwrap(),
            SystemTime::UNIX_EPOCH + Duration::from_millis(600_000),
            SystemTime::UNIX_EPOCH + Duration::from_millis(1_000),
        );
        let work_intent_id = intent.work_intent_id.as_str().to_string();
        let queued = delivery.enqueue(&intent, 1_100).unwrap();
        let delivered = delivery
            .claim(
                &registry,
                &connection_id,
                &secret,
                &work_intent_id,
                queued.revision,
                2_000,
            )
            .unwrap();

        // Outside the measured composition: create the run the intent will
        // be acknowledged under.
        let run = registry
            .start_run(&connection_id, &secret, "run-for-ack-1", None, 2_100)
            .unwrap();

        // Step 1: the generic ADR-0006 acceptance gate.
        let request = agent_mutation_request(
            &connection_id,
            generation,
            MutationMethod::WorkIntentAcknowledge,
            &work_intent_id,
            "idem-wi-ack-1",
        );
        let accepted = loki_acceptance_store::accept_mutation(&storage, &request).unwrap();
        assert!(
            matches!(accepted, AcceptanceResult::Accepted(_)),
            "expected Accepted, got {accepted:?}"
        );

        (
            connection_id,
            secret,
            generation,
            work_intent_id,
            run.run_id,
            delivered.revision,
            accepted,
        )
    };

    // Simulate the restart.
    let storage2 = open_storage(dir.path(), db_name);
    let registry2 = AgentRegistry::new(storage2.clone());
    let delivery2 = WorkIntentDeliveryStore::new(storage2.clone());

    // Step 2, first real attempt (post-restart).
    let ack1 = delivery2
        .acknowledge(
            &registry2,
            AcknowledgeRequest {
                connection_id: &connection_id,
                secret: &secret,
                work_intent_id: &work_intent_id,
                run_id: &run_id,
                expected_revision: revision,
                now_ms: 9_000,
            },
        )
        .unwrap();
    assert_eq!(
        ack1.acknowledged_agent_run.as_deref(),
        Some(run_id.as_str())
    );

    // Full retry of both calls: the retried accept_mutation call must
    // replay the exact pre-crash Accepted result.
    let request = agent_mutation_request(
        &connection_id,
        generation,
        MutationMethod::WorkIntentAcknowledge,
        &work_intent_id,
        "idem-wi-ack-1",
    );
    let retry_accept = loki_acceptance_store::accept_mutation(&storage2, &request).unwrap();
    assert_same_accepted(&first_accept, &retry_accept);

    let ack2 = delivery2
        .acknowledge(
            &registry2,
            AcknowledgeRequest {
                connection_id: &connection_id,
                secret: &secret,
                work_intent_id: &work_intent_id,
                run_id: &run_id,
                expected_revision: revision,
                now_ms: 9_100,
            },
        )
        .unwrap();
    assert_eq!(
        ack1, ack2,
        "retry must observe the identical, already-acknowledged record"
    );

    // Direct repository read: exactly one acknowledged state for this work
    // intent, bound to exactly the run that acknowledged it.
    let final_record = delivery2.get(&work_intent_id).unwrap();
    assert_eq!(
        final_record.acknowledged_agent_run.as_deref(),
        Some(run_id.as_str())
    );
    assert_eq!(
        final_record.state,
        loki_agent_delivery::DeliveryState::Acknowledged
    );
}

/// After a successful `AgentRunUpdate` composition (run finished as
/// `Failed`), a **different** target state (`Succeeded`) passed to
/// `finish_run` for the same run is still rejected with
/// `Err(AgentError::TerminalRun)` — the composed-context regression proof
/// that Deliverable 1's fix narrows correctly to "identical retry only,"
/// mirroring `loki-agents`'s own `invalid_transition_is_denied` unit test
/// but in this cross-crate composed context.
#[test]
fn finish_run_conflicting_retry_after_composition_is_still_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let db_name = "agent_run_update_conflict.sqlite3";

    let (connection_id, secret, run_id) = {
        let storage = open_storage(dir.path(), db_name);
        let (registry, connection_id, secret, generation) =
            register_agent_connection(storage.clone());
        let run = registry
            .start_run(&connection_id, &secret, "run-to-conflict-1", None, 1_000)
            .unwrap();

        let request = agent_mutation_request(
            &connection_id,
            generation,
            MutationMethod::AgentRunUpdate,
            &run.run_id,
            "idem-run-update-conflict-1",
        );
        let accepted = loki_acceptance_store::accept_mutation(&storage, &request).unwrap();
        assert!(matches!(accepted, AcceptanceResult::Accepted(_)));

        (connection_id, secret, run.run_id)
    };

    let storage2 = open_storage(dir.path(), db_name);
    let registry2 = AgentRegistry::new(storage2.clone());

    registry2
        .finish_run(&connection_id, &secret, &run_id, RunState::Failed, 7_000)
        .unwrap();

    let conflicting =
        registry2.finish_run(&connection_id, &secret, &run_id, RunState::Succeeded, 8_000);
    assert!(
        matches!(conflicting, Err(loki_agents::AgentError::TerminalRun)),
        "a conflicting terminal-state retry must still be denied, got {conflicting:?}"
    );
}
