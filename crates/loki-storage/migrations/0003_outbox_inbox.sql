-- DAT-001: outbox/inbox table pair for future cloud sync (CLD-002).
--
-- Docs/ARCHITECTURE.md §8 Persistence: runtime-owned SQLite owns "activity
-- outbox and cloud inbox checkpoints." Docs/ARCHITECTURE.md §9 / Docs/SPEC.md
-- §9: "Cloud sync is at-least-once. IDs and versions make it idempotent." /
-- "Consumers deduplicate by event/message ID."
--
-- This migration is table structure and a repository read/write API only —
-- no delivery/transport logic lives in this crate
-- (Docs/graph/tasks/DAT-001.md §5 forbidden scope: "no outbox delivery/
-- transport logic, just the durable table structure and repository
-- read/write API"). status/attempts exist so a future delivery worker
-- (CLD-002) has somewhere to record its own progress; this migration does
-- not implement that worker, and the status vocabulary below is this
-- node's own minimal scaffold (see src/states.rs), not a Docs/SPEC.md-
-- defined entity.
CREATE TABLE IF NOT EXISTS outbox_messages (
    message_id      TEXT NOT NULL PRIMARY KEY,
    stream_key       TEXT NOT NULL,
    message_type      TEXT NOT NULL,
    correlation_id    TEXT NOT NULL,
    causation_id      TEXT,
    payload_json      TEXT NOT NULL,
    status            TEXT NOT NULL CHECK (status IN ('pending', 'sending', 'sent', 'failed')),
    attempts          INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    dispatched_at      TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS idx_outbox_status_created ON outbox_messages (status, created_at);
CREATE INDEX IF NOT EXISTS idx_outbox_stream_key ON outbox_messages (stream_key);

-- Docs/ARCHITECTURE.md §8 "cloud inbox checkpoints" — records which
-- cloud-origin message IDs this runtime has already seen/processed, for
-- idempotent at-least-once consumption (Docs/SPEC.md §9: "Consumers
-- deduplicate by event/message ID"). payload_json is nullable: this table's
-- primary purpose is the dedupe checkpoint ("checkpoints", not "archive"),
-- not guaranteed payload retention.
CREATE TABLE IF NOT EXISTS inbox_messages (
    message_id       TEXT NOT NULL PRIMARY KEY,
    source            TEXT NOT NULL,
    message_type       TEXT NOT NULL,
    correlation_id     TEXT,
    payload_json       TEXT,
    status             TEXT NOT NULL CHECK (status IN ('received', 'processed', 'failed')),
    received_at        TEXT NOT NULL,
    processed_at       TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS idx_inbox_status_received ON inbox_messages (status, received_at);
