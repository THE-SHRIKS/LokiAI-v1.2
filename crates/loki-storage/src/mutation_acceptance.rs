//! The `mutation_acceptances` repository — ADR-0006 step 2/3's durable
//! acceptance record (`P1-ACCEPTANCE-STORE-001` Deliverable 5; see
//! `migrations/0007_mutation_acceptances.sql` for the schema and why
//! `accepted_at_ms` is `INTEGER` epoch milliseconds rather than this
//! crate's usual `TEXT` UTC-ISO-8601 timestamp convention). This table is
//! brand new and independent of `jobs.idempotency_key`
//! (`crates/loki-storage/migrations/0002_jobs.sql`) — see this module's
//! parent migration file's doc comment.
//!
//! # Connection-parameterized core (`P1-ACCEPTANCE-STORE-001`)
//!
//! Same shape as `src/jobs.rs`'s deadlock-avoidance split — see that
//! module's top doc comment and `src/storage.rs`'s `Storage::transaction`
//! doc comment.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::StorageError;
use crate::storage::Storage;

const SELECT_COLUMNS: &str = "idempotency_key, principal_sid, grant_generation, method, target_id, \
     artifact_digest, request_digest, job_id, correlation_id, accepted_at_ms";

/// Fields required to persist a newly committed mutation acceptance.
#[derive(Debug, Clone)]
pub struct NewMutationAcceptance {
    pub idempotency_key: String,
    pub principal_sid: String,
    pub grant_generation: u64,
    /// `loki_mutation_contract::MutationMethod::as_str()`'s wire string —
    /// this crate does not depend on `loki-mutation-contract` (that crate
    /// depends on this one, not the reverse), so the method is stored and
    /// returned as a plain `String` here.
    pub method: String,
    pub target_id: String,
    pub artifact_digest: Option<String>,
    pub request_digest: String,
    /// `None` for agent methods that never create a `jobs` row — see the
    /// migration file's doc comment.
    pub job_id: Option<String>,
    pub correlation_id: String,
    pub accepted_at_ms: i64,
}

/// A persisted mutation acceptance row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationAcceptanceRow {
    pub idempotency_key: String,
    pub principal_sid: String,
    pub grant_generation: u64,
    pub method: String,
    pub target_id: String,
    pub artifact_digest: Option<String>,
    pub request_digest: String,
    pub job_id: Option<String>,
    pub correlation_id: String,
    pub accepted_at_ms: i64,
}

/// Repository pattern over the `mutation_acceptances` table.
pub trait MutationAcceptanceRepository {
    /// Inserts a new row. `idempotency_key` is the table's primary key, so
    /// a duplicate insert surfaces as a real `StorageError::Sqlite`
    /// constraint-violation error, never a silently accepted second row —
    /// callers must always check
    /// [`MutationAcceptanceRepository::get_mutation_acceptance`] first
    /// (ADR-0006 step 3: "claim or replay the idempotency record").
    fn insert_mutation_acceptance(&self, row: &NewMutationAcceptance) -> Result<(), StorageError>;
    fn get_mutation_acceptance(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<MutationAcceptanceRow>, StorageError>;
}

impl MutationAcceptanceRepository for Storage {
    fn insert_mutation_acceptance(&self, row: &NewMutationAcceptance) -> Result<(), StorageError> {
        let conn = self.lock();
        insert_mutation_acceptance_core(&conn, row)
    }

    fn get_mutation_acceptance(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<MutationAcceptanceRow>, StorageError> {
        let conn = self.lock();
        get_mutation_acceptance_core(&conn, idempotency_key)
    }
}

/// Connection-parameterized core of
/// [`MutationAcceptanceRepository::insert_mutation_acceptance`] — this is
/// the exact function `crates/loki-acceptance-store` calls from inside its
/// `Storage::transaction()` closure for ADR-0006 step 3's "claim ... the
/// idempotency record."
pub fn insert_mutation_acceptance_core(
    conn: &Connection,
    row: &NewMutationAcceptance,
) -> Result<(), StorageError> {
    let generation_i64 =
        i64::try_from(row.grant_generation).map_err(|_| StorageError::InvalidId {
            kind: "GrantGeneration",
            reason: "generation exceeds representable range".to_string(),
        })?;
    conn.execute(
        "INSERT INTO mutation_acceptances
         (idempotency_key, principal_sid, grant_generation, method, target_id,
          artifact_digest, request_digest, job_id, correlation_id, accepted_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            row.idempotency_key,
            row.principal_sid,
            generation_i64,
            row.method,
            row.target_id,
            row.artifact_digest,
            row.request_digest,
            row.job_id,
            row.correlation_id,
            row.accepted_at_ms,
        ],
    )?;
    Ok(())
}

pub fn get_mutation_acceptance_core(
    conn: &Connection,
    idempotency_key: &str,
) -> Result<Option<MutationAcceptanceRow>, StorageError> {
    conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM mutation_acceptances WHERE idempotency_key = ?1"),
        params![idempotency_key],
        row_to_mutation_acceptance,
    )
    .optional()
    .map_err(Into::into)
}

fn row_to_mutation_acceptance(row: &Row<'_>) -> rusqlite::Result<MutationAcceptanceRow> {
    let generation_i64: i64 = row.get(2)?;
    Ok(MutationAcceptanceRow {
        idempotency_key: row.get(0)?,
        principal_sid: row.get(1)?,
        grant_generation: u64::try_from(generation_i64).unwrap_or(0),
        method: row.get(3)?,
        target_id: row.get(4)?,
        artifact_digest: row.get(5)?,
        request_digest: row.get(6)?,
        job_id: row.get(7)?,
        correlation_id: row.get(8)?,
        accepted_at_ms: row.get(9)?,
    })
}
