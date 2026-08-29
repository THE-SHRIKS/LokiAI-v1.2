//! Local, single-use approval repository (`APR-001`'s `local_approvals`
//! table, consolidated into this crate by `P1-ACCEPTANCE-STORE-001` — see
//! `migrations/0004_approvals.sql`). `crates/loki-approvals` owns the
//! business rules (local-authority authorization, canonical digest
//! computation, single-use consumption semantics); this module owns only
//! the SQL.
//!
//! # Connection-parameterized cores (`P1-ACCEPTANCE-STORE-001`)
//!
//! Same shape as `src/jobs.rs`'s deadlock-avoidance split: every
//! [`ApprovalsRepository`] method is a `self.lock()` wrapper around a
//! private, connection-parameterized `..._core` function that
//! `crates/loki-acceptance-store` also calls directly from inside a
//! `Storage::transaction()` closure, for ADR-0006 step 4's "validate and
//! consume the exact approval" — see `src/storage.rs`'s `Storage::transaction`
//! doc comment for why calling the trait methods themselves from inside
//! that closure would deadlock.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::StorageError;
use crate::storage::Storage;

pub const CURRENT_BINDING_VERSION: i64 = 1;

const SELECT_COLUMNS: &str = "approval_id, request_digest, requester_sid, actor_id, operation, \
     target_id, artifact_digest, parameters_digest, idempotency_key, reason, requested_at_ms, \
     expires_at_ms, granted_at_ms, granted_by_sid, consumed_at_ms, binding_version, grant_generation";

/// Fields required to persist a newly requested approval.
#[derive(Debug, Clone)]
pub struct NewApproval {
    pub approval_id: String,
    pub request_digest: String,
    pub binding_version: i64,
    pub requester_sid: String,
    pub actor_id: String,
    pub grant_generation: u64,
    pub operation: String,
    pub target_id: String,
    pub artifact_digest: Option<String>,
    pub parameters_digest: String,
    pub idempotency_key: String,
    pub reason: String,
    pub requested_at_ms: i64,
    pub expires_at_ms: i64,
}

/// A persisted approval row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRow {
    pub approval_id: String,
    pub request_digest: String,
    pub binding_version: i64,
    pub requester_sid: String,
    pub actor_id: String,
    pub grant_generation: u64,
    pub operation: String,
    pub target_id: String,
    pub artifact_digest: Option<String>,
    pub parameters_digest: String,
    pub idempotency_key: String,
    pub reason: String,
    pub requested_at_ms: i64,
    pub expires_at_ms: i64,
    pub granted_at_ms: Option<i64>,
    pub granted_by_sid: Option<String>,
    pub consumed_at_ms: Option<i64>,
}

/// Outcome of a single-use consumption attempt
/// ([`ApprovalsRepository::consume_approval`]/[`consume_approval_core`]) —
/// closed, so a caller cannot mistake "no matching row" for "already
/// consumed" or silently ignore either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsumeOutcome {
    /// No row exists for the given `approval_id`.
    NotFound,
    /// The row exists but does not match the expected requester/digest
    /// binding.
    BindingMismatch,
    /// The approval has not been granted yet.
    NotGranted,
    /// `now_ms` has reached or passed the approval's `expires_at_ms`.
    Expired,
    /// The approval was already consumed by an earlier call.
    AlreadyConsumed,
    /// This call performed the single-use consumption; the returned row
    /// reflects the post-consumption state.
    Consumed(Box<ApprovalRow>),
}

/// Repository pattern over the `local_approvals` table.
pub trait ApprovalsRepository {
    fn insert_approval(&self, approval: &NewApproval) -> Result<(), StorageError>;
    fn get_approval(&self, approval_id: &str) -> Result<Option<ApprovalRow>, StorageError>;
    /// Atomically grants an ungranted approval. Returns `true` if this call
    /// performed the grant, `false` if the row does not exist or was
    /// already granted (the caller distinguishes those two cases with a
    /// preceding [`ApprovalsRepository::get_approval`] if it needs to).
    fn grant_approval(
        &self,
        approval_id: &str,
        granted_by_sid: &str,
        now_ms: i64,
    ) -> Result<bool, StorageError>;
    /// Validates and atomically consumes exactly once. See
    /// [`ConsumeOutcome`].
    fn consume_approval(
        &self,
        approval_id: &str,
        expected_request_digest: &str,
        expected_requester_sid: &str,
        now_ms: i64,
    ) -> Result<ConsumeOutcome, StorageError>;
}

impl ApprovalsRepository for Storage {
    fn insert_approval(&self, approval: &NewApproval) -> Result<(), StorageError> {
        let conn = self.lock();
        insert_approval_core(&conn, approval)
    }

    fn get_approval(&self, approval_id: &str) -> Result<Option<ApprovalRow>, StorageError> {
        let conn = self.lock();
        get_approval_core(&conn, approval_id)
    }

    fn grant_approval(
        &self,
        approval_id: &str,
        granted_by_sid: &str,
        now_ms: i64,
    ) -> Result<bool, StorageError> {
        let conn = self.lock();
        grant_approval_core(&conn, approval_id, granted_by_sid, now_ms)
    }

    fn consume_approval(
        &self,
        approval_id: &str,
        expected_request_digest: &str,
        expected_requester_sid: &str,
        now_ms: i64,
    ) -> Result<ConsumeOutcome, StorageError> {
        let conn = self.lock();
        consume_approval_core(
            &conn,
            approval_id,
            expected_request_digest,
            expected_requester_sid,
            now_ms,
        )
    }
}

/// Connection-parameterized core of [`ApprovalsRepository::insert_approval`].
pub fn insert_approval_core(conn: &Connection, approval: &NewApproval) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO local_approvals
         (approval_id, request_digest, requester_sid, actor_id, operation,
          target_id, artifact_digest, parameters_digest, idempotency_key,
          reason, requested_at_ms, expires_at_ms, binding_version, grant_generation)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        params![
            approval.approval_id,
            approval.request_digest,
            approval.requester_sid,
            approval.actor_id,
            approval.operation,
            approval.target_id,
            approval.artifact_digest,
            approval.parameters_digest,
            approval.idempotency_key,
            approval.reason,
            approval.requested_at_ms,
            approval.expires_at_ms,
            approval.binding_version,
            i64::try_from(approval.grant_generation).map_err(|_| StorageError::InvalidId {
                kind: "GrantGeneration",
                reason: "generation exceeds representable range".to_string(),
            })?,
        ],
    )?;
    Ok(())
}

pub fn get_approval_core(
    conn: &Connection,
    approval_id: &str,
) -> Result<Option<ApprovalRow>, StorageError> {
    conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM local_approvals WHERE approval_id = ?1"),
        params![approval_id],
        row_to_approval,
    )
    .optional()
    .map_err(Into::into)
}

pub fn grant_approval_core(
    conn: &Connection,
    approval_id: &str,
    granted_by_sid: &str,
    now_ms: i64,
) -> Result<bool, StorageError> {
    let changed = conn.execute(
        "UPDATE local_approvals SET granted_at_ms=?1, granted_by_sid=?2
         WHERE approval_id=?3 AND granted_at_ms IS NULL AND binding_version=?4",
        params![now_ms, granted_by_sid, approval_id, CURRENT_BINDING_VERSION],
    )?;
    Ok(changed == 1)
}

/// Connection-parameterized core of
/// [`ApprovalsRepository::consume_approval`] — ADR-0006 step 4's "validate
/// and consume the exact approval when required," applied atomically. This
/// is the exact function `crates/loki-acceptance-store` calls from inside
/// its `Storage::transaction()` closure after
/// `loki_mutation_contract::construct_accepted` has already independently
/// confirmed digest/expiry match via `evaluate_approval_reference`; this
/// function re-derives and re-checks binding/grant/expiry/consumption state
/// directly against the row (defense in depth — never trusts the caller's
/// claim that a reference is still valid) before performing the one
/// permitted write.
pub fn consume_approval_core(
    conn: &Connection,
    approval_id: &str,
    expected_request_digest: &str,
    expected_requester_sid: &str,
    now_ms: i64,
) -> Result<ConsumeOutcome, StorageError> {
    let Some(row) = get_approval_core(conn, approval_id)? else {
        return Ok(ConsumeOutcome::NotFound);
    };
    if row.binding_version != CURRENT_BINDING_VERSION
        || row.request_digest != expected_request_digest
        || row.requester_sid != expected_requester_sid
    {
        return Ok(ConsumeOutcome::BindingMismatch);
    }
    if row.granted_at_ms.is_none() {
        return Ok(ConsumeOutcome::NotGranted);
    }
    if now_ms >= row.expires_at_ms {
        return Ok(ConsumeOutcome::Expired);
    }
    if row.consumed_at_ms.is_some() {
        return Ok(ConsumeOutcome::AlreadyConsumed);
    }
    let changed = conn.execute(
        "UPDATE local_approvals SET consumed_at_ms=?1
         WHERE approval_id=?2 AND consumed_at_ms IS NULL",
        params![now_ms, approval_id],
    )?;
    if changed != 1 {
        // Lost a race with another consumer between the read above and this
        // write. Under `Storage`'s single `Mutex<Connection>` this is only
        // reachable if `conn` is a bare `&Connection` obtained without
        // holding that mutex for this whole call (never true for the two
        // real callers: `ApprovalsRepository::consume_approval`'s
        // `self.lock()` wrapper, and `Storage::transaction`'s closure,
        // which holds the mutex for the closure's entire duration) — kept
        // as a defended outcome rather than an `unreachable!()` panic
        // (`AGENTS.md` rule 1).
        return Ok(ConsumeOutcome::AlreadyConsumed);
    }
    let Some(updated) = get_approval_core(conn, approval_id)? else {
        return Ok(ConsumeOutcome::NotFound);
    };
    Ok(ConsumeOutcome::Consumed(Box::new(updated)))
}

fn row_to_approval(row: &Row<'_>) -> rusqlite::Result<ApprovalRow> {
    Ok(ApprovalRow {
        approval_id: row.get(0)?,
        request_digest: row.get(1)?,
        requester_sid: row.get(2)?,
        actor_id: row.get(3)?,
        operation: row.get(4)?,
        target_id: row.get(5)?,
        artifact_digest: row.get(6)?,
        parameters_digest: row.get(7)?,
        idempotency_key: row.get(8)?,
        reason: row.get(9)?,
        requested_at_ms: row.get(10)?,
        expires_at_ms: row.get(11)?,
        granted_at_ms: row.get(12)?,
        granted_by_sid: row.get(13)?,
        consumed_at_ms: row.get(14)?,
        binding_version: row.get(15)?,
        grant_generation: u64::try_from(row.get::<_, i64>(16)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                16,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
    })
}
