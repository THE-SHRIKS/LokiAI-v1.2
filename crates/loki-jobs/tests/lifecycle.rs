//! Real end-to-end tests: a real `loki_storage::Storage` (in-memory SQLite,
//! migrated) driven through `loki_jobs::JobEngine`, exercising
//! `Docs/graph/tasks/JOB-001.md` Deliverable 5's exact scenarios.

use std::sync::Arc;

use loki_jobs::engine::{CancelOutcome, RestartRequest, SubmitOutcome};
use loki_jobs::error::JobsError;
use loki_jobs::{JobEngine, SubmitRequest};
use loki_storage::ids::{CorrelationId, DeviceId, JobId};
use loki_storage::jobs::{JobRepository, NewJob};
use loki_storage::states::JobState;
use loki_storage::{Cipher, Storage};

fn new_storage() -> Arc<Storage> {
    let storage = Storage::open_in_memory(Cipher::None).expect("open in-memory storage");
    storage
        .run_migrations("2026-01-01T00:00:00Z")
        .expect("run migrations");
    Arc::new(storage)
}

fn job_id(s: &str) -> JobId {
    JobId::new(s).unwrap()
}

fn device_id(s: &str) -> DeviceId {
    DeviceId::new(s).unwrap()
}

fn correlation_id(s: &str) -> CorrelationId {
    CorrelationId::new(s).unwrap()
}

fn submit_request(job: &str, idem: &str, corr: &str) -> SubmitRequest {
    SubmitRequest {
        job_id: job_id(job),
        idempotency_key: idem.to_string(),
        correlation_id: correlation_id(corr),
    }
}

#[test]
fn full_happy_path_reaches_succeeded_with_verification() {
    let storage = new_storage();
    let engine = JobEngine::new(storage);

    let created = engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();
    let SubmitOutcome::Created(record) = created else {
        panic!("expected a newly created job");
    };
    assert_eq!(record.state, JobState::Queued);

    engine.begin_preflight(&job_id("job-1")).unwrap();
    engine.begin_execute(&job_id("job-1")).unwrap();
    engine.begin_verify(&job_id("job-1")).unwrap();
    let done = engine
        .complete_success(&job_id("job-1"), Some("[]"), Some(r#"{"ok":true}"#))
        .unwrap();

    assert_eq!(done.state, JobState::Succeeded);
    assert_eq!(done.result_json.as_deref(), Some(r#"{"ok":true}"#));
}

#[test]
fn success_is_never_inferred_without_going_through_verify() {
    let storage = new_storage();
    let engine = JobEngine::new(storage);

    engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();
    engine.begin_preflight(&job_id("job-1")).unwrap();
    engine.begin_execute(&job_id("job-1")).unwrap();

    // Still "executing", not "testing" — a direct jump to "succeeded" must
    // be refused, not silently accepted (AGENTS.md rule 6: no state
    // transition without operation-specific verification).
    let err = engine
        .complete_success(&job_id("job-1"), None, None)
        .unwrap_err();
    assert!(matches!(err, JobsError::InvalidTransition { .. }));
}

#[test]
fn submitting_the_same_idempotency_key_twice_is_a_no_op() {
    let storage = new_storage();
    let engine = JobEngine::new(storage);

    let first = engine
        .submit(submit_request("job-1", "idem-shared", "corr-1"))
        .unwrap();
    assert!(matches!(first, SubmitOutcome::Created(_)));

    // Same job_id/idempotency_key resubmitted (a retried request).
    let second = engine
        .submit(submit_request("job-1", "idem-shared", "corr-1"))
        .unwrap();
    assert!(matches!(second, SubmitOutcome::AlreadyExists(_)));
    assert_eq!(first.record().job_id, second.record().job_id);
}

#[test]
fn reusing_an_idempotency_key_for_a_different_job_id_is_a_conflict_not_silently_accepted() {
    let storage = new_storage();
    let engine = JobEngine::new(storage);

    engine
        .submit(submit_request("job-1", "idem-shared", "corr-1"))
        .unwrap();

    let err = engine
        .submit(submit_request("job-2", "idem-shared", "corr-1"))
        .unwrap_err();
    assert!(matches!(err, JobsError::Conflict { .. }));
}

#[test]
fn concurrent_submits_with_the_same_idempotency_key_create_exactly_one_job() {
    use std::thread;

    let storage = new_storage();
    let engine = Arc::new(JobEngine::new(storage.clone()));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let engine = Arc::clone(&engine);
        handles.push(thread::spawn(move || {
            engine.submit(submit_request("job-race", "idem-race", "corr-1"))
        }));
    }
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    for r in &results {
        assert!(
            r.is_ok(),
            "every racer must get an Ok outcome, not an error: {r:?}"
        );
    }
    let created_count = results
        .iter()
        .filter(|r| matches!(r, Ok(SubmitOutcome::Created(_))))
        .count();
    assert_eq!(created_count, 1, "exactly one racer creates the job");

    let matching_job = storage.get_job_by_idempotency_key("idem-race").unwrap();
    assert!(matching_job.is_some(), "exactly one durable row must exist");
}

#[test]
fn crash_mid_job_leaves_a_quarantined_state_not_a_falsely_completed_one() {
    let storage = new_storage();

    // Simulate a job that was "executing" when the previous process
    // crashed: insert it directly via the repository (bypassing the
    // engine, which is exactly what a real crash does — no engine method
    // ever gets to run its own transition-out-of-executing logic).
    storage
        .insert_job(&NewJob {
            job_id: job_id("crashed-job"),
            state: JobState::Queued,
            idempotency_key: "idem-crashed".to_string(),
            correlation_id: correlation_id("corr-1"),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .unwrap();
    storage
        .transition_job_state(
            &job_id("crashed-job"),
            JobState::Executing,
            "2026-01-01T00:00:01Z",
        )
        .unwrap();

    // A brand-new engine (simulating the restarted process) has no memory
    // of ever having leased anything for this job.
    let engine = JobEngine::new(storage.clone());
    let quarantined = engine.reconcile().unwrap();

    assert_eq!(quarantined, vec![job_id("crashed-job")]);
    let record = storage.get_job(&job_id("crashed-job")).unwrap().unwrap();
    assert_eq!(
        record.state,
        JobState::OutcomeUnknown,
        "a crash-mid-job outcome must never be guessed as succeeded or failed"
    );
}

#[test]
fn reconcile_does_not_quarantine_a_job_whose_lease_is_still_active() {
    let storage = new_storage();
    let engine = JobEngine::new(storage.clone());

    engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();
    engine.begin_preflight(&job_id("job-1")).unwrap();
    engine.begin_execute(&job_id("job-1")).unwrap();

    // This job legitimately holds a still-valid lease in this same
    // process — reconcile must not treat it as crashed.
    engine
        .acquire_lease(&device_id("dev-1"), &job_id("job-1"), 60_000)
        .unwrap();

    let quarantined = engine.reconcile().unwrap();
    assert!(quarantined.is_empty());
    let record = storage.get_job(&job_id("job-1")).unwrap().unwrap();
    assert_eq!(record.state, JobState::Executing);
}

#[test]
fn ambiguous_verify_is_explicitly_quarantined_not_guessed() {
    let storage = new_storage();
    let engine = JobEngine::new(storage.clone());

    engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();
    engine.begin_preflight(&job_id("job-1")).unwrap();
    engine.begin_execute(&job_id("job-1")).unwrap();
    engine.begin_verify(&job_id("job-1")).unwrap();

    // The verify stage itself could not conclusively determine pass/fail.
    let record = engine.quarantine(&job_id("job-1")).unwrap();
    assert_eq!(record.state, JobState::OutcomeUnknown);
}

#[test]
fn a_race_for_the_same_device_lease_has_exactly_one_winner() {
    use std::thread;

    let storage = new_storage();
    let engine = Arc::new(JobEngine::new(storage));
    for i in 0..8 {
        engine
            .submit(submit_request(
                &format!("job-{i}"),
                &format!("idem-{i}"),
                "corr-1",
            ))
            .unwrap();
    }

    let mut handles = Vec::new();
    for i in 0..8 {
        let engine = Arc::clone(&engine);
        handles.push(thread::spawn(move || {
            engine.acquire_lease(
                &device_id("contested"),
                &job_id(&format!("job-{i}")),
                60_000,
            )
        }));
    }
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let winners = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        winners, 1,
        "exactly one concurrent attempt must win the device lease"
    );
}

#[test]
fn an_unrenewed_lease_is_reclaimable_by_another_attempt() {
    use loki_jobs::clock::Clock;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct FakeClock(AtomicU64);
    impl Clock for FakeClock {
        fn now_millis(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    let storage = new_storage();
    let clock = Arc::new(FakeClock(AtomicU64::new(0)));
    let engine = JobEngine::with_clock(storage, clock.clone());

    engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();
    engine
        .submit(submit_request("job-2", "idem-2", "corr-1"))
        .unwrap();

    engine
        .acquire_lease(&device_id("dev-1"), &job_id("job-1"), 1_000)
        .unwrap();

    // job-2 tries immediately: must lose.
    let immediate_attempt = engine.acquire_lease(&device_id("dev-1"), &job_id("job-2"), 1_000);
    assert!(matches!(
        immediate_attempt,
        Err(JobsError::LeaseHeld { .. })
    ));

    // job-1 never renews; time passes beyond the lease TTL.
    clock.0.store(2_000, Ordering::SeqCst);

    // job-2 tries again after expiry: must win.
    let reclaim_attempt = engine.acquire_lease(&device_id("dev-1"), &job_id("job-2"), 1_000);
    assert!(reclaim_attempt.is_ok());
}

#[test]
fn cancel_of_a_queued_job_is_immediate() {
    let storage = new_storage();
    let engine = JobEngine::new(storage);
    engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();

    let outcome = engine.request_cancel(&job_id("job-1")).unwrap();
    assert!(matches!(outcome, CancelOutcome::CancelledImmediately(_)));
}

#[test]
fn cancel_of_a_running_job_is_cooperative_not_a_kill_assumption() {
    let storage = new_storage();
    let engine = JobEngine::new(storage.clone());
    engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();
    engine.begin_preflight(&job_id("job-1")).unwrap();
    engine.begin_execute(&job_id("job-1")).unwrap();

    let outcome = engine.request_cancel(&job_id("job-1")).unwrap();
    assert!(matches!(outcome, CancelOutcome::CancelRequested));

    // The job is still "executing" — cancellation is not assumed just
    // because it was requested.
    let record = storage.get_job(&job_id("job-1")).unwrap().unwrap();
    assert_eq!(record.state, JobState::Executing);
    assert!(engine.is_cancel_requested(&job_id("job-1")));

    // Only once the running stage cooperatively observes the request and
    // acknowledges does the job actually move to "cancelled".
    let record = engine.acknowledge_cancelled(&job_id("job-1")).unwrap();
    assert_eq!(record.state, JobState::Cancelled);
    assert!(!engine.is_cancel_requested(&job_id("job-1")));
}

#[test]
fn acknowledging_a_cancel_with_no_pending_request_is_refused() {
    let storage = new_storage();
    let engine = JobEngine::new(storage);
    engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();
    engine.begin_preflight(&job_id("job-1")).unwrap();

    let err = engine.acknowledge_cancelled(&job_id("job-1")).unwrap_err();
    assert!(matches!(err, JobsError::Conflict { .. }));
}

#[test]
fn a_failed_job_can_be_explicitly_restarted_as_a_new_attempt() {
    let storage = new_storage();
    let engine = JobEngine::new(storage.clone());
    engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();
    engine.begin_preflight(&job_id("job-1")).unwrap();
    engine
        .fail(&job_id("job-1"), r#"{"message":"boom"}"#)
        .unwrap();

    let restarted = engine
        .restart(RestartRequest {
            source_job_id: job_id("job-1"),
            new_job_id: job_id("job-1-retry"),
            new_idempotency_key: "idem-1-retry".to_string(),
        })
        .unwrap();
    let SubmitOutcome::Created(record) = restarted else {
        panic!("expected a new job to be created for the restart attempt");
    };
    assert_eq!(record.job_id, job_id("job-1-retry"));
    assert_eq!(record.state, JobState::Queued);
    // Traceability back to the original attempt.
    assert_eq!(record.correlation_id, correlation_id("corr-1"));
}

#[test]
fn a_quarantined_job_can_be_explicitly_restarted() {
    let storage = new_storage();
    let engine = JobEngine::new(storage);
    engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();
    engine.begin_preflight(&job_id("job-1")).unwrap();
    engine.quarantine(&job_id("job-1")).unwrap();

    let restarted = engine
        .restart(RestartRequest {
            source_job_id: job_id("job-1"),
            new_job_id: job_id("job-1-retry"),
            new_idempotency_key: "idem-1-retry".to_string(),
        })
        .unwrap();
    assert!(matches!(restarted, SubmitOutcome::Created(_)));
}

#[test]
fn restarting_a_job_that_is_not_terminal_is_refused() {
    let storage = new_storage();
    let engine = JobEngine::new(storage);
    engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();
    engine.begin_preflight(&job_id("job-1")).unwrap();

    let err = engine
        .restart(RestartRequest {
            source_job_id: job_id("job-1"),
            new_job_id: job_id("job-1-retry"),
            new_idempotency_key: "idem-1-retry".to_string(),
        })
        .unwrap_err();
    assert!(matches!(err, JobsError::Conflict { .. }));
}

#[test]
fn restarting_a_succeeded_job_is_refused() {
    let storage = new_storage();
    let engine = JobEngine::new(storage);
    engine
        .submit(submit_request("job-1", "idem-1", "corr-1"))
        .unwrap();
    engine.begin_preflight(&job_id("job-1")).unwrap();
    engine.begin_execute(&job_id("job-1")).unwrap();
    engine.begin_verify(&job_id("job-1")).unwrap();
    engine
        .complete_success(&job_id("job-1"), None, None)
        .unwrap();

    let err = engine
        .restart(RestartRequest {
            source_job_id: job_id("job-1"),
            new_job_id: job_id("job-1-retry"),
            new_idempotency_key: "idem-1-retry".to_string(),
        })
        .unwrap_err();
    assert!(matches!(err, JobsError::Conflict { .. }));
}

#[test]
fn get_job_on_an_unknown_id_is_an_honest_none_not_a_fabricated_record() {
    let storage = new_storage();
    let engine = JobEngine::new(storage);
    let result = engine.get_job(&job_id("does-not-exist")).unwrap();
    assert!(result.is_none());
}

#[test]
fn transitioning_an_unknown_job_is_not_found() {
    let storage = new_storage();
    let engine = JobEngine::new(storage);
    let err = engine.begin_preflight(&job_id("ghost")).unwrap_err();
    assert!(matches!(err, JobsError::NotFound { .. }));
}
