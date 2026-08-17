//! Outbox repository — the local-to-cloud half of the durable outbox/inbox
//! pair (`Docs/ARCHITECTURE.md` §8 "activity outbox"; `CLD-002` owns
//! delivery). Table structure and repository read/write API only — no
//! delivery/transport logic (`Docs/graph/tasks/DAT-001.md` §5 forbidden
//! scope).

use rusqlite::{OptionalExtension, Row, params};

use crate::error::StorageError;
use crate::ids::{CausationId, CorrelationId, MessageId};
use crate::states::OutboxStatus;
use crate::storage::Storage;
use crate::timestamp::validate_utc_timestamp;

const SELECT_COLUMNS: &str = "message_id, stream_key, message_type, correlation_id, causation_id, \
     payload_json, status, attempts, last_error, created_at, updated_at, dispatched_at";

/// Fields required to enqueue a new outbound message.
#[derive(Debug, Clone)]
pub struct NewOutboxMessage {
    pub message_id: MessageId,
    /// Groups messages that must preserve relative order on delivery
    /// (`Docs/SPEC.md` §9: "Ordering is guaranteed only within a declared
    /// stream key such as one job or one device lease.").
    pub stream_key: String,
    /// Open vocabulary — no closed set is defined in `Docs/SPEC.md`/
    /// `Docs/ARCHITECTURE.md` for outbox message types.
    pub message_type: String,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<CausationId>,
    /// Opaque, already-serialized JSON payload.
    pub payload_json: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A persisted outbox row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxMessageRecord {
    pub message_id: MessageId,
    pub stream_key: String,
    pub message_type: String,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<CausationId>,
    pub payload_json: String,
    pub status: OutboxStatus,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub dispatched_at: Option<String>,
}

/// Repository pattern over the outbox table.
pub trait OutboxRepository {
    /// Enqueues `msg`. Idempotent by `message_id`
    /// (`Docs/SPEC.md` §9: "Consumers deduplicate by event/message ID" —
    /// applied here on the write side too, for a caller retrying an enqueue
    /// after an uncertain prior attempt): returns `Ok(true)` if this is a
    /// new row, `Ok(false)` if `message_id` was already enqueued (no
    /// duplicate row, no error).
    fn enqueue_outbox_message(&self, msg: &NewOutboxMessage) -> Result<bool, StorageError>;
    fn get_outbox_message(
        &self,
        id: &MessageId,
    ) -> Result<Option<OutboxMessageRecord>, StorageError>;
    fn list_outbox_messages_by_status(
        &self,
        status: OutboxStatus,
        limit: i64,
    ) -> Result<Vec<OutboxMessageRecord>, StorageError>;
    fn mark_outbox_message_sent(
        &self,
        id: &MessageId,
        dispatched_at: &str,
        updated_at: &str,
    ) -> Result<(), StorageError>;
    fn mark_outbox_message_failed(
        &self,
        id: &MessageId,
        error: &str,
        updated_at: &str,
    ) -> Result<(), StorageError>;
}

impl OutboxRepository for Storage {
    fn enqueue_outbox_message(&self, msg: &NewOutboxMessage) -> Result<bool, StorageError> {
        validate_utc_timestamp("created_at", &msg.created_at)?;
        validate_utc_timestamp("updated_at", &msg.updated_at)?;

        let conn = self.lock();
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO outbox_messages (
                message_id, stream_key, message_type, correlation_id, causation_id,
                payload_json, status, attempts, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9)",
            params![
                msg.message_id.as_str(),
                msg.stream_key,
                msg.message_type,
                msg.correlation_id.as_str(),
                msg.causation_id.as_ref().map(CausationId::as_str),
                msg.payload_json,
                OutboxStatus::Pending.as_str(),
                msg.created_at,
                msg.updated_at,
            ],
        )?;
        Ok(inserted == 1)
    }

    fn get_outbox_message(
        &self,
        id: &MessageId,
    ) -> Result<Option<OutboxMessageRecord>, StorageError> {
        let conn = self.lock();
        let raw: Option<RawOutboxRow> = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM outbox_messages WHERE message_id = ?1"),
                params![id.as_str()],
                row_to_raw,
            )
            .optional()?;
        raw.map(raw_to_record).transpose()
    }

    fn list_outbox_messages_by_status(
        &self,
        status: OutboxStatus,
        limit: i64,
    ) -> Result<Vec<OutboxMessageRecord>, StorageError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SELECT_COLUMNS} FROM outbox_messages WHERE status = ?1 ORDER BY created_at LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![status.as_str(), limit], row_to_raw)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(raw_to_record(row?)?);
        }
        Ok(out)
    }

    fn mark_outbox_message_sent(
        &self,
        id: &MessageId,
        dispatched_at: &str,
        updated_at: &str,
    ) -> Result<(), StorageError> {
        validate_utc_timestamp("dispatched_at", dispatched_at)?;
        validate_utc_timestamp("updated_at", updated_at)?;
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE outbox_messages SET status = ?1, dispatched_at = ?2, updated_at = ?3 WHERE message_id = ?4",
            params![OutboxStatus::Sent.as_str(), dispatched_at, updated_at, id.as_str()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound {
                kind: "outbox message",
                id: id.as_str().to_string(),
            });
        }
        Ok(())
    }

    fn mark_outbox_message_failed(
        &self,
        id: &MessageId,
        error: &str,
        updated_at: &str,
    ) -> Result<(), StorageError> {
        validate_utc_timestamp("updated_at", updated_at)?;
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE outbox_messages SET status = ?1, last_error = ?2, attempts = attempts + 1, updated_at = ?3
             WHERE message_id = ?4",
            params![OutboxStatus::Failed.as_str(), error, updated_at, id.as_str()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound {
                kind: "outbox message",
                id: id.as_str().to_string(),
            });
        }
        Ok(())
    }
}

struct RawOutboxRow {
    message_id: String,
    stream_key: String,
    message_type: String,
    correlation_id: String,
    causation_id: Option<String>,
    payload_json: String,
    status: String,
    attempts: i64,
    last_error: Option<String>,
    created_at: String,
    updated_at: String,
    dispatched_at: Option<String>,
}

fn row_to_raw(row: &Row<'_>) -> rusqlite::Result<RawOutboxRow> {
    Ok(RawOutboxRow {
        message_id: row.get(0)?,
        stream_key: row.get(1)?,
        message_type: row.get(2)?,
        correlation_id: row.get(3)?,
        causation_id: row.get(4)?,
        payload_json: row.get(5)?,
        status: row.get(6)?,
        attempts: row.get(7)?,
        last_error: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        dispatched_at: row.get(11)?,
    })
}

fn raw_to_record(raw: RawOutboxRow) -> Result<OutboxMessageRecord, StorageError> {
    Ok(OutboxMessageRecord {
        message_id: MessageId::new(raw.message_id)?,
        stream_key: raw.stream_key,
        message_type: raw.message_type,
        correlation_id: CorrelationId::new(raw.correlation_id)?,
        causation_id: raw.causation_id.map(CausationId::new).transpose()?,
        payload_json: raw.payload_json,
        status: OutboxStatus::parse("status", &raw.status)?,
        attempts: raw.attempts,
        last_error: raw.last_error,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
        dispatched_at: raw.dispatched_at,
    })
}
