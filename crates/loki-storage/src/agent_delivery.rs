//! Work-intent delivery repository (`AGT-002`'s `work_intent_deliveries`
//! table, consolidated into this crate by `P1-ACCEPTANCE-STORE-001` — see
//! `migrations/0006_agent_delivery_and_local_authority.sql`).
//! `crates/loki-agent-delivery` owns the business rules (agent scope
//! authorization, revision-checked state machine, human-fallback
//! notification ordering); this module owns only the SQL.
//!
//! # Connection-parameterized cores (`P1-ACCEPTANCE-STORE-001`)
//!
//! Same shape as `src/jobs.rs`'s deadlock-avoidance split — see that
//! module's top doc comment and `src/storage.rs`'s `Storage::transaction`
//! doc comment. `P1-ACCEPTANCE-STORE-001`'s own transaction does not use
//! this repository directly (`WorkIntentAcknowledge` is bound through
//! `crates/loki-agents`'s run/idempotency tables, not this table), but the
//! same core-function split is applied here uniformly per
//! `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md` Deliverable 2's
//! requirement that it cover "the four new modules plus the two edited
//! existing ones."

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::StorageError;
use crate::storage::Storage;

const SELECT_COLUMNS: &str = "work_intent_id, incident_id, requested_capability, evidence_refs_json, \
     delivery_requirements, state, delivery_attempts, claimed_connection_id, acknowledged_agent_run, \
     failure_reason, created_at_ms, expires_at_ms, delivered_at_ms, terminal_at_ms, revision";

/// A persisted work-intent delivery row. `evidence_refs_json` remains
/// opaque, already-serialized JSON here (as it is on disk); `state` remains
/// a raw wire string — `crates/loki-agent-delivery` owns the closed-enum
/// parsing and evidence-ref shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkIntentDeliveryRow {
    pub work_intent_id: String,
    pub incident_id: String,
    pub requested_capability: String,
    pub evidence_refs_json: String,
    pub delivery_requirements: String,
    pub state: String,
    pub delivery_attempts: i64,
    pub claimed_connection_id: Option<String>,
    pub acknowledged_agent_run: Option<String>,
    pub failure_reason: Option<String>,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub delivered_at_ms: Option<i64>,
    pub terminal_at_ms: Option<i64>,
    pub revision: i64,
}

/// Fields required to enqueue a new pristine delivery row (`revision` is
/// always `1` for a fresh insert; not modeled as a field here).
#[derive(Debug, Clone)]
pub struct NewWorkIntentDelivery {
    pub work_intent_id: String,
    pub incident_id: String,
    pub requested_capability: String,
    pub evidence_refs_json: String,
    pub delivery_requirements: String,
    pub state_wire: String,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub terminal_at_ms: Option<i64>,
}

/// Repository pattern over the `work_intent_deliveries` table.
pub trait AgentDeliveryRepository {
    fn get_work_intent_delivery(
        &self,
        work_intent_id: &str,
    ) -> Result<Option<WorkIntentDeliveryRow>, StorageError>;
    fn insert_work_intent_delivery(
        &self,
        delivery: &NewWorkIntentDelivery,
    ) -> Result<(), StorageError>;
    /// Raw revision-checked `UPDATE`, exactly as
    /// `crates/loki-agent-delivery`'s existing per-transition SQL text.
    /// Returns the number of rows changed (`0` or `1`) — the caller decides
    /// what that means for its specific transition (conflict vs. no-op).
    fn update_work_intent_delivery(
        &self,
        sql: WorkIntentDeliveryUpdate<'_>,
    ) -> Result<usize, StorageError>;
    fn expire_due_work_intent_deliveries(&self, now_ms: i64) -> Result<usize, StorageError>;
}

/// One of the small, closed set of revision-checked transitions
/// `crates/loki-agent-delivery` performs. Modeled as an enum (rather than a
/// raw SQL string parameter) so this repository module remains the only
/// place composing `UPDATE work_intent_deliveries` statements — the
/// original crate-local SQL text is preserved verbatim per transition.
pub enum WorkIntentDeliveryUpdate<'a> {
    Claim {
        work_intent_id: &'a str,
        connection_id: &'a str,
        now_ms: i64,
        expected_revision: i64,
    },
    Acknowledge {
        work_intent_id: &'a str,
        run_id: &'a str,
        now_ms: i64,
        expected_revision: i64,
    },
    RequireUserAction {
        work_intent_id: &'a str,
        now_ms: i64,
        expected_revision: i64,
    },
    RecordPermanentFailure {
        work_intent_id: &'a str,
        reason: &'a str,
        now_ms: i64,
        expected_revision: i64,
    },
    ExpireOne {
        work_intent_id: &'a str,
        now_ms: i64,
    },
}

impl AgentDeliveryRepository for Storage {
    fn get_work_intent_delivery(
        &self,
        work_intent_id: &str,
    ) -> Result<Option<WorkIntentDeliveryRow>, StorageError> {
        let conn = self.lock();
        get_work_intent_delivery_core(&conn, work_intent_id)
    }

    fn insert_work_intent_delivery(
        &self,
        delivery: &NewWorkIntentDelivery,
    ) -> Result<(), StorageError> {
        let conn = self.lock();
        insert_work_intent_delivery_core(&conn, delivery)
    }

    fn update_work_intent_delivery(
        &self,
        update: WorkIntentDeliveryUpdate<'_>,
    ) -> Result<usize, StorageError> {
        let conn = self.lock();
        update_work_intent_delivery_core(&conn, update)
    }

    fn expire_due_work_intent_deliveries(&self, now_ms: i64) -> Result<usize, StorageError> {
        let conn = self.lock();
        expire_due_work_intent_deliveries_core(&conn, now_ms)
    }
}

pub fn get_work_intent_delivery_core(
    conn: &Connection,
    work_intent_id: &str,
) -> Result<Option<WorkIntentDeliveryRow>, StorageError> {
    conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM work_intent_deliveries WHERE work_intent_id=?1"),
        params![work_intent_id],
        row_to_delivery,
    )
    .optional()
    .map_err(Into::into)
}

pub fn insert_work_intent_delivery_core(
    conn: &Connection,
    delivery: &NewWorkIntentDelivery,
) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO work_intent_deliveries(work_intent_id,incident_id,requested_capability,evidence_refs_json,delivery_requirements,state,delivery_attempts,created_at_ms,expires_at_ms,terminal_at_ms,revision) VALUES(?1,?2,?3,?4,?5,?6,0,?7,?8,?9,1)",
        params![
            delivery.work_intent_id,
            delivery.incident_id,
            delivery.requested_capability,
            delivery.evidence_refs_json,
            delivery.delivery_requirements,
            delivery.state_wire,
            delivery.created_at_ms,
            delivery.expires_at_ms,
            delivery.terminal_at_ms,
        ],
    )?;
    Ok(())
}

pub fn update_work_intent_delivery_core(
    conn: &Connection,
    update: WorkIntentDeliveryUpdate<'_>,
) -> Result<usize, StorageError> {
    let changed = match update {
        WorkIntentDeliveryUpdate::Claim {
            work_intent_id,
            connection_id,
            now_ms,
            expected_revision,
        } => conn.execute(
            "UPDATE work_intent_deliveries SET state='delivered',delivery_attempts=delivery_attempts+1,claimed_connection_id=?1,delivered_at_ms=?2,revision=revision+1 WHERE work_intent_id=?3 AND state='queued' AND revision=?4",
            params![connection_id, now_ms, work_intent_id, expected_revision],
        )?,
        WorkIntentDeliveryUpdate::Acknowledge {
            work_intent_id,
            run_id,
            now_ms,
            expected_revision,
        } => conn.execute(
            "UPDATE work_intent_deliveries SET state='acknowledged',acknowledged_agent_run=?1,terminal_at_ms=?2,revision=revision+1 WHERE work_intent_id=?3 AND revision=?4",
            params![run_id, now_ms, work_intent_id, expected_revision],
        )?,
        WorkIntentDeliveryUpdate::RequireUserAction {
            work_intent_id,
            now_ms,
            expected_revision,
        } => conn.execute(
            "UPDATE work_intent_deliveries SET state='user_action_required',terminal_at_ms=?1,revision=revision+1 WHERE work_intent_id=?2 AND state='queued' AND revision=?3",
            params![now_ms, work_intent_id, expected_revision],
        )?,
        WorkIntentDeliveryUpdate::RecordPermanentFailure {
            work_intent_id,
            reason,
            now_ms,
            expected_revision,
        } => conn.execute(
            "UPDATE work_intent_deliveries SET state='failed',failure_reason=?1,terminal_at_ms=?2,revision=revision+1 WHERE work_intent_id=?3 AND revision=?4",
            params![reason, now_ms, work_intent_id, expected_revision],
        )?,
        WorkIntentDeliveryUpdate::ExpireOne {
            work_intent_id,
            now_ms,
        } => conn.execute(
            "UPDATE work_intent_deliveries SET state='expired',terminal_at_ms=?1,revision=revision+1 WHERE work_intent_id=?2 AND state IN ('queued','delivered')",
            params![now_ms, work_intent_id],
        )?,
    };
    Ok(changed)
}

pub fn expire_due_work_intent_deliveries_core(
    conn: &Connection,
    now_ms: i64,
) -> Result<usize, StorageError> {
    Ok(conn.execute(
        "UPDATE work_intent_deliveries SET state='expired',terminal_at_ms=?1,revision=revision+1 WHERE expires_at_ms<=?1 AND state IN ('queued','delivered')",
        params![now_ms],
    )?)
}

fn row_to_delivery(row: &Row<'_>) -> rusqlite::Result<WorkIntentDeliveryRow> {
    Ok(WorkIntentDeliveryRow {
        work_intent_id: row.get(0)?,
        incident_id: row.get(1)?,
        requested_capability: row.get(2)?,
        evidence_refs_json: row.get(3)?,
        delivery_requirements: row.get(4)?,
        state: row.get(5)?,
        delivery_attempts: row.get(6)?,
        claimed_connection_id: row.get(7)?,
        acknowledged_agent_run: row.get(8)?,
        failure_reason: row.get(9)?,
        created_at_ms: row.get(10)?,
        expires_at_ms: row.get(11)?,
        delivered_at_ms: row.get(12)?,
        terminal_at_ms: row.get(13)?,
        revision: row.get(14)?,
    })
}
