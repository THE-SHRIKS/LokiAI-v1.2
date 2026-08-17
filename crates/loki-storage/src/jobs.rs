//! Durable job repository (`Docs/SPEC.md` Section 4.7 Durable job / 3.12
//! "job ID" / Section 1 `OperationResult<T>`). One of this node's three
//! required repositories — see `Docs/graph/tasks/DAT-001.md` Deliverable 3.

use rusqlite::{OptionalExtension, Row, params};

use crate::error::StorageError;
use crate::ids::{CorrelationId, JobId};
use crate::states::JobState;
use crate::storage::Storage;
use crate::timestamp::validate_utc_timestamp;

const SELECT_COLUMNS: &str = "job_id, state, idempotency_key, correlation_id, evidence_refs_json, \
     result_json, error_json, created_at, updated_at";

/// Fields required to persist a new durable job, before it has a result.
/// `Docs/SPEC.md` 4.7 "A job persists before execution."
#[derive(Debug, Clone)]
pub struct NewJob {
    pub job_id: JobId,
    pub state: JobState,
    /// `Docs/SPEC.md` 4.7 "A device-changing stage cannot run twice under
    /// the same idempotency key"; `AGENTS.md` rule 5.
    pub idempotency_key: String,
    pub correlation_id: CorrelationId,
    pub created_at: String,
    pub updated_at: String,
}

/// A persisted job row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRecord {
    pub job_id: JobId,
    pub state: JobState,
    pub idempotency_key: String,
    pub correlation_id: CorrelationId,
    /// `Docs/SPEC.md` Section 1 `OperationResult.completed.evidence_refs[]`
    /// — opaque, already-serialized JSON array; populated by
    /// [`JobRepository::complete_job`].
    pub evidence_refs_json: Option<String>,
    /// `Docs/SPEC.md` Section 1 `OperationResult.completed.result` — opaque,
    /// already-serialized JSON; this crate does not own the result shape.
    pub result_json: Option<String>,
    /// `Docs/SPEC.md` Section 1 `OperationResult.rejected.error` (`LokiError`)
    /// — opaque, already-serialized JSON; populated by
    /// [`JobRepository::fail_job`].
    pub error_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Repository pattern over the durable job records table.
pub trait JobRepository {
    fn insert_job(&self, job: &NewJob) -> Result<(), StorageError>;
    fn get_job(&self, id: &JobId) -> Result<Option<JobRecord>, StorageError>;
    /// Looks a job up by its idempotency key — the operation a retried
    /// request uses to discover "this already has a job" instead of
    /// creating a duplicate (`Docs/SPEC.md` 4.7).
    fn get_job_by_idempotency_key(&self, key: &str) -> Result<Option<JobRecord>, StorageError>;
    /// Moves a job to a new state (`Docs/SPEC.md` Section 4.7's state
    /// list). Does not by itself populate `result_json`/`error_json` — use
    /// [`JobRepository::complete_job`]/[`JobRepository::fail_job`] for a
    /// terminal transition that also records its outcome atomically.
    fn transition_job_state(
        &self,
        id: &JobId,
        state: JobState,
        updated_at: &str,
    ) -> Result<(), StorageError>;
    /// Records a `completed` outcome (`Docs/SPEC.md` Section 1
    /// `OperationResult.completed`) and sets `state` to `succeeded` in one
    /// statement.
    fn complete_job(
        &self,
        id: &JobId,
        evidence_refs_json: Option<&str>,
        result_json: Option<&str>,
        updated_at: &str,
    ) -> Result<(), StorageError>;
    /// Records a `rejected`/failed outcome (`Docs/SPEC.md` Section 1
    /// `OperationResult.rejected.error`) and sets `state` to `failed` in one
    /// statement.
    fn fail_job(&self, id: &JobId, error_json: &str, updated_at: &str) -> Result<(), StorageError>;
    fn list_jobs_by_state(&self, state: JobState) -> Result<Vec<JobRecord>, StorageError>;
}

impl JobRepository for Storage {
    fn insert_job(&self, job: &NewJob) -> Result<(), StorageError> {
        validate_utc_timestamp("created_at", &job.created_at)?;
        validate_utc_timestamp("updated_at", &job.updated_at)?;

        let conn = self.lock();
        conn.execute(
            "INSERT INTO jobs (job_id, state, idempotency_key, correlation_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                job.job_id.as_str(),
                job.state.as_str(),
                job.idempotency_key,
                job.correlation_id.as_str(),
                job.created_at,
                job.updated_at,
            ],
        )?;
        Ok(())
    }

    fn get_job(&self, id: &JobId) -> Result<Option<JobRecord>, StorageError> {
        let conn = self.lock();
        let raw: Option<RawJobRow> = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM jobs WHERE job_id = ?1"),
                params![id.as_str()],
                row_to_raw,
            )
            .optional()?;
        raw.map(raw_to_record).transpose()
    }

    fn get_job_by_idempotency_key(&self, key: &str) -> Result<Option<JobRecord>, StorageError> {
        let conn = self.lock();
        let raw: Option<RawJobRow> = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM jobs WHERE idempotency_key = ?1"),
                params![key],
                row_to_raw,
            )
            .optional()?;
        raw.map(raw_to_record).transpose()
    }

    fn transition_job_state(
        &self,
        id: &JobId,
        state: JobState,
        updated_at: &str,
    ) -> Result<(), StorageError> {
        validate_utc_timestamp("updated_at", updated_at)?;
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE jobs SET state = ?1, updated_at = ?2 WHERE job_id = ?3",
            params![state.as_str(), updated_at, id.as_str()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound {
                kind: "job",
                id: id.as_str().to_string(),
            });
        }
        Ok(())
    }

    fn complete_job(
        &self,
        id: &JobId,
        evidence_refs_json: Option<&str>,
        result_json: Option<&str>,
        updated_at: &str,
    ) -> Result<(), StorageError> {
        validate_utc_timestamp("updated_at", updated_at)?;
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE jobs SET state = ?1, evidence_refs_json = ?2, result_json = ?3, updated_at = ?4
             WHERE job_id = ?5",
            params![
                JobState::Succeeded.as_str(),
                evidence_refs_json,
                result_json,
                updated_at,
                id.as_str()
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound {
                kind: "job",
                id: id.as_str().to_string(),
            });
        }
        Ok(())
    }

    fn fail_job(&self, id: &JobId, error_json: &str, updated_at: &str) -> Result<(), StorageError> {
        validate_utc_timestamp("updated_at", updated_at)?;
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE jobs SET state = ?1, error_json = ?2, updated_at = ?3 WHERE job_id = ?4",
            params![
                JobState::Failed.as_str(),
                error_json,
                updated_at,
                id.as_str()
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound {
                kind: "job",
                id: id.as_str().to_string(),
            });
        }
        Ok(())
    }

    fn list_jobs_by_state(&self, state: JobState) -> Result<Vec<JobRecord>, StorageError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SELECT_COLUMNS} FROM jobs WHERE state = ?1 ORDER BY created_at"
        ))?;
        let rows = stmt.query_map(params![state.as_str()], row_to_raw)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(raw_to_record(row?)?);
        }
        Ok(out)
    }
}

struct RawJobRow {
    job_id: String,
    state: String,
    idempotency_key: String,
    correlation_id: String,
    evidence_refs_json: Option<String>,
    result_json: Option<String>,
    error_json: Option<String>,
    created_at: String,
    updated_at: String,
}

fn row_to_raw(row: &Row<'_>) -> rusqlite::Result<RawJobRow> {
    Ok(RawJobRow {
        job_id: row.get(0)?,
        state: row.get(1)?,
        idempotency_key: row.get(2)?,
        correlation_id: row.get(3)?,
        evidence_refs_json: row.get(4)?,
        result_json: row.get(5)?,
        error_json: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn raw_to_record(raw: RawJobRow) -> Result<JobRecord, StorageError> {
    Ok(JobRecord {
        job_id: JobId::new(raw.job_id)?,
        state: JobState::parse("state", &raw.state)?,
        idempotency_key: raw.idempotency_key,
        correlation_id: CorrelationId::new(raw.correlation_id)?,
        evidence_refs_json: raw.evidence_refs_json,
        result_json: raw.result_json,
        error_json: raw.error_json,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
    })
}
