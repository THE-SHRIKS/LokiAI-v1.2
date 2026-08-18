-- MON-001: this crate's own telemetry/log/gap tables.
--
-- Docs/SPEC.md §3.5 "Telemetry sample and log record" for the columns
-- below. Times are stored as INTEGER milliseconds since the Unix epoch
-- (UTC by construction — epoch time has no timezone), not the
-- loki-storage-style ISO 8601 TEXT convention: this is this crate's own
-- self-contained store (see src/store.rs's module doc for why), not a
-- table added inside crates/loki-storage, so it is free to pick the
-- representation that matches this crate's SystemTime-based domain types
-- without a lossy round trip through calendar-string parsing.
CREATE TABLE IF NOT EXISTS telemetry_samples (
    id                INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    source_id          TEXT NOT NULL,
    metric_name         TEXT NOT NULL,
    value                 REAL NOT NULL,
    unit                   TEXT NOT NULL,
    source_time_ms          INTEGER NOT NULL,
    receive_time_ms           INTEGER NOT NULL,
    quality                     TEXT NOT NULL CHECK (quality IN ('good', 'late', 'gap', 'invalid')),
    sequence                      INTEGER,
    retention_class                 TEXT NOT NULL,
    correlation_id                    TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS idx_telemetry_samples_source_metric
    ON telemetry_samples (source_id, metric_name, receive_time_ms);

CREATE TABLE IF NOT EXISTS log_records (
    id                INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    source_id          TEXT NOT NULL,
    level                TEXT NOT NULL,
    timestamp_ms           INTEGER NOT NULL,
    message                   TEXT NOT NULL,
    schema_version              TEXT NOT NULL,
    correlation_id                 TEXT,
    redaction_markers_json           TEXT NOT NULL,
    retention_class                    TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_log_records_source_time
    ON log_records (source_id, timestamp_ms);

-- Docs/graph/tasks/MON-001.md Deliverable 5: "a gap/loss scenario is
-- detectable and reported honestly" — persisted so a gap survives a
-- restart even though the in-memory src/gap.rs detector state itself does
-- not (see src/pipeline.rs's module doc).
CREATE TABLE IF NOT EXISTS telemetry_gaps (
    id                    INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    signal_key              TEXT NOT NULL,
    last_seen_sequence         INTEGER NOT NULL,
    resumed_sequence              INTEGER NOT NULL,
    missing_count                    INTEGER NOT NULL,
    detected_at_ms                      INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_telemetry_gaps_signal ON telemetry_gaps (signal_key, detected_at_ms);
