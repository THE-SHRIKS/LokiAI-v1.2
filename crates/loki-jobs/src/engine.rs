//! [`JobEngine`]: the durable job/lease state machine
//! (`Docs/graph/tasks/JOB-001.md`).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use loki_storage::Storage;
use loki_storage::ids::{CorrelationId, DeviceId, JobId};
use loki_storage::jobs::{JobRecord, JobRepository, NewJob};
use loki_storage::states::JobState;

use crate::clock::{Clock, SystemClock};
use crate::error::JobsError;
use crate::lease::{LeaseTable, LeaseToken};
use crate::transitions;

/// Fields required to submit a new durable job
/// (`Docs/graph/tasks/JOB-001.md` Deliverable 2: idempotent submission).
#[derive(Debug, Clone)]
pub struct SubmitRequest {
    pub job_id: JobId,
    pub idempotency_key: String,
    pub correlation_id: CorrelationId,
}

/// The result of [`JobEngine::submit`] — never a bare `JobRecord`, so a
/// caller can distinguish "your request created a new job" from "this is a
/// no-op replay of an already-accepted request" without inspecting
/// timestamps.
#[derive(Debug, Clone)]
pub enum SubmitOutcome {
    /// A new job row was created.
    Created(JobRecord),
    /// `idempotency_key` was already claimed by this exact `job_id` — no new
    /// row, no duplicate side effect. `AGENTS.md` rule 5 / `Docs/SPEC.md`
    /// 4.7: "A device-changing stage cannot run twice under the same
    /// idempotency key."
    AlreadyExists(JobRecord),
}

impl SubmitOutcome {
    /// The job record either way — useful when a caller only cares about
    /// the resulting state, not whether this call created it.
    pub fn record(&self) -> &JobRecord {
        match self {
            SubmitOutcome::Created(r) | SubmitOutcome::AlreadyExists(r) => r,
        }
    }
}

/// Fields required to explicitly restart a `failed`/`outcome-unknown` job
/// as a new attempt (`Docs/graph/tasks/JOB-001.md` Deliverable 4). This
/// mints a *new* job ID/idempotency key rather than reusing the source
/// job's — `Docs/SPEC.md` 4.7's idempotency guarantee is per idempotency
/// key, and a restart is a deliberate new attempt, not a replay of the
/// failed one. `correlation_id` is carried forward from the source job for
/// traceability.
#[derive(Debug, Clone)]
pub struct RestartRequest {
    pub source_job_id: JobId,
    pub new_job_id: JobId,
    pub new_idempotency_key: String,
}

/// The result of [`JobEngine::request_cancel`].
#[derive(Debug, Clone)]
pub enum CancelOutcome {
    /// The job had not started running yet (`queued`); it was moved
    /// straight to `cancelled` — there is nothing to cooperate with.
    CancelledImmediately(JobRecord),
    /// The job is already running (`preflight`/`executing`/`testing`). The
    /// cancellation request is recorded; the running stage must observe it
    /// via [`JobEngine::is_cancel_requested`] and stop cooperatively, then
    /// call [`JobEngine::acknowledge_cancelled`] — this engine never
    /// assumes a `kill -9`-style abort (`Docs/graph/tasks/JOB-001.md`
    /// Deliverable 4).
    CancelRequested,
}

/// The durable job/lease state machine. Built directly on
/// `loki_storage::jobs::JobRepository`'s existing `jobs` table — this crate
/// adds no new SQLite table (see `crate::lease`'s doc comment for why the
/// per-device lease is in-memory instead).
pub struct JobEngine {
    storage: Arc<Storage>,
    clock: Arc<dyn Clock>,
    leases: LeaseTable,
    cancel_requests: Mutex<HashSet<JobId>>,
}

impl JobEngine {
    /// Builds an engine backed by `storage`, using the real OS clock.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self::with_clock(storage, Arc::new(SystemClock))
    }

    /// Builds an engine with an injected [`Clock`] — production callers
    /// should prefer [`JobEngine::new`]; this exists for deterministic
    /// tests (lease expiry/reclaim, reconcile-after-crash) that must not
    /// depend on real wall-clock sleeps.
    pub fn with_clock(storage: Arc<Storage>, clock: Arc<dyn Clock>) -> Self {
        Self {
            storage,
            clock,
            leases: LeaseTable::new(),
            cancel_requests: Mutex::new(HashSet::new()),
        }
    }

    /// Looks up a job by ID. Thin passthrough convenience over the
    /// repository, returning [`JobsError`] like every other method here.
    pub fn get_job(&self, job_id: &JobId) -> Result<Option<JobRecord>, JobsError> {
        Ok(self.storage.get_job(job_id)?)
    }

    fn require_job(&self, job_id: &JobId) -> Result<JobRecord, JobsError> {
        self.storage
            .get_job(job_id)?
            .ok_or_else(|| JobsError::NotFound {
                job_id: job_id.clone(),
            })
    }

    /// Idempotent job submission (`Docs/graph/tasks/JOB-001.md`
    /// Deliverable 2). Submitting the same `idempotency_key` twice — even
    /// concurrently from two callers — returns the same underlying job
    /// record and creates no duplicate row: the second caller loses the
    /// race on the repository's own unique index, and this method turns
    /// that loss into an honest [`SubmitOutcome::AlreadyExists`] rather than
    /// an error, by re-reading the row the winner created.
    pub fn submit(&self, request: SubmitRequest) -> Result<SubmitOutcome, JobsError> {
        let now = self.clock.now_iso8601();
        let new_job = NewJob {
            job_id: request.job_id.clone(),
            state: JobState::Queued,
            idempotency_key: request.idempotency_key.clone(),
            correlation_id: request.correlation_id,
            created_at: now.clone(),
            updated_at: now,
        };

        match self.storage.insert_job(&new_job) {
            Ok(()) => {
                let record = self.require_job(&request.job_id)?;
                tracing::info!(job_id = %record.job_id, "loki-jobs: job created");
                Ok(SubmitOutcome::Created(record))
            }
            Err(insert_err) => {
                // Either this exact request was retried (the common case),
                // or two callers raced and this one lost — in both cases
                // the idempotency key already names a real row. Re-read it
                // rather than assuming *why* the insert failed.
                if let Some(existing) = self
                    .storage
                    .get_job_by_idempotency_key(&request.idempotency_key)?
                {
                    if existing.job_id == request.job_id {
                        tracing::info!(
                            job_id = %existing.job_id,
                            "loki-jobs: submit is a no-op replay of an already-accepted idempotency key"
                        );
                        return Ok(SubmitOutcome::AlreadyExists(existing));
                    }
                    return Err(JobsError::Conflict {
                        detail: format!(
                            "idempotency key '{}' is already claimed by job '{}', not '{}'",
                            request.idempotency_key, existing.job_id, request.job_id
                        ),
                    });
                }
                Err(insert_err.into())
            }
        }
    }

    fn transition_checked(&self, job_id: &JobId, to: JobState) -> Result<JobRecord, JobsError> {
        let current = self.require_job(job_id)?;
        if !transitions::is_allowed(current.state, to) {
            return Err(JobsError::InvalidTransition {
                job_id: job_id.clone(),
                from: current.state.as_str(),
                to: to.as_str(),
            });
        }
        let now = self.clock.now_iso8601();
        self.storage.transition_job_state(job_id, to, &now)?;
        tracing::info!(
            job_id = %job_id,
            from = current.state.as_str(),
            to = to.as_str(),
            "loki-jobs: job transitioned"
        );
        self.require_job(job_id)
    }

    /// `queued -> preflight`.
    pub fn begin_preflight(&self, job_id: &JobId) -> Result<JobRecord, JobsError> {
        self.transition_checked(job_id, JobState::Preflight)
    }

    /// `preflight -> executing`.
    pub fn begin_execute(&self, job_id: &JobId) -> Result<JobRecord, JobsError> {
        self.transition_checked(job_id, JobState::Executing)
    }

    /// `executing -> testing` (this engine's verification stage —
    /// `Docs/SPEC.md` 4.7: "`succeeded` requires its verification contract,
    /// not only command completion").
    pub fn begin_verify(&self, job_id: &JobId) -> Result<JobRecord, JobsError> {
        self.transition_checked(job_id, JobState::Testing)
    }

    /// `testing -> succeeded`, recording verification evidence
    /// (`Docs/SPEC.md` Section 1 `OperationResult.completed`). Refuses the
    /// move if the job is not currently `testing` — success is never
    /// inferred from anything other than this explicit, verification-gated
    /// call (`AGENTS.md` rule 6).
    pub fn complete_success(
        &self,
        job_id: &JobId,
        evidence_refs_json: Option<&str>,
        result_json: Option<&str>,
    ) -> Result<JobRecord, JobsError> {
        let current = self.require_job(job_id)?;
        if !transitions::is_allowed(current.state, JobState::Succeeded) {
            return Err(JobsError::InvalidTransition {
                job_id: job_id.clone(),
                from: current.state.as_str(),
                to: JobState::Succeeded.as_str(),
            });
        }
        let now = self.clock.now_iso8601();
        self.storage
            .complete_job(job_id, evidence_refs_json, result_json, &now)?;
        tracing::info!(job_id = %job_id, "loki-jobs: job succeeded");
        self.require_job(job_id)
    }

    /// Any in-flight state (`preflight`/`executing`/`testing`) -> `failed`,
    /// recording the error (`Docs/SPEC.md` Section 1
    /// `OperationResult.rejected.error`).
    pub fn fail(&self, job_id: &JobId, error_json: &str) -> Result<JobRecord, JobsError> {
        let current = self.require_job(job_id)?;
        if !transitions::is_allowed(current.state, JobState::Failed) {
            return Err(JobsError::InvalidTransition {
                job_id: job_id.clone(),
                from: current.state.as_str(),
                to: JobState::Failed.as_str(),
            });
        }
        let now = self.clock.now_iso8601();
        self.storage.fail_job(job_id, error_json, &now)?;
        tracing::warn!(job_id = %job_id, "loki-jobs: job failed");
        self.require_job(job_id)
    }

    /// Explicitly quarantines a job whose true outcome is unknown/ambiguous
    /// (an in-progress verify that cannot conclusively pass or fail) —
    /// `outcome-unknown`, never guessed as success or failure
    /// (`Docs/graph/tasks/JOB-001.md` Deliverable 1/5). See
    /// [`JobEngine::reconcile`] for the crash-mid-job case, which calls
    /// this same transition automatically at startup.
    pub fn quarantine(&self, job_id: &JobId) -> Result<JobRecord, JobsError> {
        self.transition_checked(job_id, JobState::OutcomeUnknown)
    }

    /// Requests cancellation of a job (`Docs/graph/tasks/JOB-001.md`
    /// Deliverable 4: "cooperative, not a kill -9 assumption"). A `queued`
    /// job (nothing running yet) is cancelled immediately. A running job
    /// only has the request recorded; the running stage must observe
    /// [`JobEngine::is_cancel_requested`] and call
    /// [`JobEngine::acknowledge_cancelled`] once it has actually stopped.
    pub fn request_cancel(&self, job_id: &JobId) -> Result<CancelOutcome, JobsError> {
        let current = self.require_job(job_id)?;
        match current.state {
            JobState::Queued => {
                let record = self.transition_checked(job_id, JobState::Cancelled)?;
                Ok(CancelOutcome::CancelledImmediately(record))
            }
            JobState::Preflight | JobState::Executing | JobState::Testing => {
                self.cancel_requests
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(job_id.clone());
                tracing::info!(job_id = %job_id, "loki-jobs: cancellation requested");
                Ok(CancelOutcome::CancelRequested)
            }
            other => Err(JobsError::Conflict {
                detail: format!(
                    "job '{job_id}' cannot be cancelled from state '{}'",
                    other.as_str()
                ),
            }),
        }
    }

    /// `true` if a still-pending cancellation request exists for `job_id`.
    /// The cooperative counterpart of [`JobEngine::request_cancel`]: a
    /// running stage should poll this (analogous to `RUN-002`'s IPC
    /// deadline/cancellation primitives, conceptually) and stop at a safe
    /// point rather than being killed.
    pub fn is_cancel_requested(&self, job_id: &JobId) -> bool {
        self.cancel_requests
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(job_id)
    }

    /// Confirms a running job has cooperatively stopped in response to a
    /// prior [`JobEngine::request_cancel`], and records the terminal
    /// `cancelled` state. Refused if there was no pending request for
    /// `job_id`.
    pub fn acknowledge_cancelled(&self, job_id: &JobId) -> Result<JobRecord, JobsError> {
        let had_request = self
            .cancel_requests
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(job_id);
        if !had_request {
            return Err(JobsError::Conflict {
                detail: format!("job '{job_id}' has no pending cancellation request"),
            });
        }
        let record = self.transition_checked(job_id, JobState::Cancelled)?;
        self.cancel_requests
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(job_id);
        Ok(record)
    }

    /// Explicitly restarts a `failed`/`outcome-unknown` job as a new attempt
    /// (`Docs/graph/tasks/JOB-001.md` Deliverable 4). Refused for a job in
    /// any other state (including one already terminal as `succeeded`/
    /// `cancelled`, or still in flight).
    pub fn restart(&self, request: RestartRequest) -> Result<SubmitOutcome, JobsError> {
        let source = self.require_job(&request.source_job_id)?;
        if !matches!(source.state, JobState::Failed | JobState::OutcomeUnknown) {
            return Err(JobsError::Conflict {
                detail: format!(
                    "job '{}' is in state '{}', not restart-eligible (must be 'failed' or 'outcome-unknown')",
                    source.job_id,
                    source.state.as_str()
                ),
            });
        }
        tracing::info!(
            source_job_id = %source.job_id,
            new_job_id = %request.new_job_id,
            "loki-jobs: restarting job as a new attempt"
        );
        self.submit(SubmitRequest {
            job_id: request.new_job_id,
            idempotency_key: request.new_idempotency_key,
            correlation_id: source.correlation_id,
        })
    }

    /// Acquires the per-device lease for `device_id` on behalf of `job_id`.
    /// See `crate::lease` for the exclusivity/reclaim guarantees.
    pub fn acquire_lease(
        &self,
        device_id: &DeviceId,
        job_id: &JobId,
        ttl_millis: u64,
    ) -> Result<LeaseToken, JobsError> {
        self.leases
            .acquire(device_id, job_id, self.clock.now_millis(), ttl_millis)
    }

    /// Extends an already-held lease.
    pub fn renew_lease(
        &self,
        device_id: &DeviceId,
        token: LeaseToken,
        ttl_millis: u64,
    ) -> Result<(), JobsError> {
        self.leases
            .renew(device_id, token, self.clock.now_millis(), ttl_millis)
    }

    /// Releases a held lease early.
    pub fn release_lease(&self, device_id: &DeviceId, token: LeaseToken) -> Result<(), JobsError> {
        self.leases.release(device_id, token)
    }

    /// Scans every job in an in-flight state (`preflight`/`executing`/
    /// `testing`) and quarantines (`outcome-unknown`) any with no
    /// currently-active lease reference in this process
    /// (`crate::lease`'s doc comment explains why "no active lease" is the
    /// correct signal for "a prior process crashed mid-job"). Intended to
    /// be called once, at startup, before this engine accepts new work —
    /// `Docs/graph/tasks/JOB-001.md` Deliverable 1: "Restart after crash
    /// resumes from a safe checkpoint or enters `outcome-unknown`," never
    /// silently marked success or silently dropped. Returns the IDs of
    /// every job it quarantined.
    pub fn reconcile(&self) -> Result<Vec<JobId>, JobsError> {
        let now_millis = self.clock.now_millis();
        let active = self.leases.active_holders(now_millis);
        let now = self.clock.now_iso8601();

        let mut quarantined = Vec::new();
        for state in [JobState::Preflight, JobState::Executing, JobState::Testing] {
            for record in self.storage.list_jobs_by_state(state)? {
                if active.contains(&record.job_id) {
                    continue;
                }
                self.storage.transition_job_state(
                    &record.job_id,
                    JobState::OutcomeUnknown,
                    &now,
                )?;
                tracing::warn!(
                    job_id = %record.job_id,
                    from_state = state.as_str(),
                    "loki-jobs: quarantined during reconcile (no active lease reference — \
                     likely a crash mid-job)"
                );
                quarantined.push(record.job_id);
            }
        }
        Ok(quarantined)
    }
}
