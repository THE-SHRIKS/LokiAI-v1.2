//! Inbox repository — the cloud-to-local half of the durable outbox/inbox
//! pair (`Docs/ARCHITECTURE.md` §8 "cloud inbox checkpoints"). Table
//! structure and repository read/write API only — no delivery/transport
//! logic (`Docs/graph/tasks/DAT-001.md` §5 forbidden scope).

use rusqlite::{OptionalExtension, Row, params};

use crate::error::StorageError;
use crate::ids::{CorrelationId, MessageId};
use crate::states::InboxStatus;
use crate::storage::Storage;
use crate::timestamp::validate_utc_timestamp;

const SELECT_COLUMNS: &str = "message_id, source, message_type, correlation_id, payload_json, status, received_at, processed_at";

/// Fields required to record a received cloud-origin message.
#[derive(Debug, Clone)]
pub struct NewInboxMessage {
    pub message_id: MessageId,
    /// Open vocabulary identifying which upstream sent this (e.g. a cloud
    /// service name) — not defined by `Docs/SPEC.md`/`Docs/CLOUD_CONTRACT.md`
    /// at this layer.
    pub source: String,
    pub message_type: String,
    pub correlation_id: Option<CorrelationId>,
    /// Opaque, already-serialized JSON payload. Nullable: this table's
    /// primary purpose is the dedupe checkpoint, not payload archival (see
    /// module doc).
    pub payload_json: Option<String>,
    pub received_at: String,
}

/// A persisted inbox row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxMessageRecord {
    pub message_id: MessageId,
    pub source: String,
    pub message_type: String,
    pub correlation_id: Option<CorrelationId>,
    pub payload_json: Option<String>,
    pub status: InboxStatus,
    pub received_at: String,
    pub processed_at: Option<String>,
}

/// Repository pattern over the inbox checkpoint table.
pub trait InboxRepository {
    /// Records that `msg` was received. Returns `Ok(true)` if this is the
    /// first time this `message_id` has been seen, `Ok(false)` if it was
    /// already recorded (`Docs/SPEC.md` §9: "Consumers deduplicate by
    /// event/message ID") — idempotent, not an error and not a duplicate
    /// row.
    fn record_inbox_message(&self, msg: &NewInboxMessage) -> Result<bool, StorageError>;
    fn get_inbox_message(&self, id: &MessageId)
    -> Result<Option<InboxMessageRecord>, StorageError>;
    fn mark_inbox_message_processed(
        &self,
        id: &MessageId,
        processed_at: &str,
    ) -> Result<(), StorageError>;
    /// `Ok(false)` both when the message was never received and when it
    /// was received but not yet processed — callers that need to
    /// distinguish those should use [`InboxRepository::get_inbox_message`].
    fn was_inbox_message_processed(&self, id: &MessageId) -> Result<bool, StorageError>;
}

impl InboxRepository for Storage {
    fn record_inbox_message(&self, msg: &NewInboxMessage) -> Result<bool, StorageError> {
        validate_utc_timestamp("received_at", &msg.received_at)?;

        let conn = self.lock();
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO inbox_messages (
                message_id, source, message_type, correlation_id, payload_json, status, received_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                msg.message_id.as_str(),
                msg.source,
                msg.message_type,
                msg.correlation_id.as_ref().map(CorrelationId::as_str),
                msg.payload_json,
                InboxStatus::Received.as_str(),
                msg.received_at,
            ],
        )?;
        Ok(inserted == 1)
    }

    fn get_inbox_message(
        &self,
        id: &MessageId,
    ) -> Result<Option<InboxMessageRecord>, StorageError> {
        let conn = self.lock();
        let raw: Option<RawInboxRow> = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM inbox_messages WHERE message_id = ?1"),
                params![id.as_str()],
                row_to_raw,
            )
            .optional()?;
        raw.map(raw_to_record).transpose()
    }

    fn mark_inbox_message_processed(
        &self,
        id: &MessageId,
        processed_at: &str,
    ) -> Result<(), StorageError> {
        validate_utc_timestamp("processed_at", processed_at)?;
        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE inbox_messages SET status = ?1, processed_at = ?2 WHERE message_id = ?3",
            params![InboxStatus::Processed.as_str(), processed_at, id.as_str()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound {
                kind: "inbox message",
                id: id.as_str().to_string(),
            });
        }
        Ok(())
    }

    fn was_inbox_message_processed(&self, id: &MessageId) -> Result<bool, StorageError> {
        let conn = self.lock();
        let status: Option<String> = conn
            .query_row(
                "SELECT status FROM inbox_messages WHERE message_id = ?1",
                params![id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        match status {
            None => Ok(false),
            Some(status) => Ok(InboxStatus::parse("status", &status)? == InboxStatus::Processed),
        }
    }
}

struct RawInboxRow {
    message_id: String,
    source: String,
    message_type: String,
    correlation_id: Option<String>,
    payload_json: Option<String>,
    status: String,
    received_at: String,
    processed_at: Option<String>,
}

fn row_to_raw(row: &Row<'_>) -> rusqlite::Result<RawInboxRow> {
    Ok(RawInboxRow {
        message_id: row.get(0)?,
        source: row.get(1)?,
        message_type: row.get(2)?,
        correlation_id: row.get(3)?,
        payload_json: row.get(4)?,
        status: row.get(5)?,
        received_at: row.get(6)?,
        processed_at: row.get(7)?,
    })
}

fn raw_to_record(raw: RawInboxRow) -> Result<InboxMessageRecord, StorageError> {
    Ok(InboxMessageRecord {
        message_id: MessageId::new(raw.message_id)?,
        source: raw.source,
        message_type: raw.message_type,
        correlation_id: raw.correlation_id.map(CorrelationId::new).transpose()?,
        payload_json: raw.payload_json,
        status: InboxStatus::parse("status", &raw.status)?,
        received_at: raw.received_at,
        processed_at: raw.processed_at,
    })
}
