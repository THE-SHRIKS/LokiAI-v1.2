//! Durable telemetry/log/gap storage — `Docs/graph/tasks/MON-001.md`
//! Deliverable 4: "Restart: telemetry state/buffering behaves sanely across
//! a process restart (matches `DAT-001`'s restart-preserves-data discipline
//! where storage is involved)."
//!
//! ## Why this is a self-contained store, not a `crates/loki-storage`
//! repository
//!
//! This node's task packet (`Docs/graph/tasks/MON-001.md` §2) grants write
//! access to `crates/loki-monitoring/**` plus one `Cargo.toml` members line,
//! and explicitly forbids touching `crates/loki-storage/**` directly except
//! for adding a new migration `.sql` file. In practice that carve-out alone
//! cannot wire a working repository into `loki-storage`'s database:
//! `crates/loki-storage/src/migrations.rs`'s `MIGRATIONS` constant is the
//! only thing that actually embeds/runs a migration file, and registering a
//! new entry there means editing `crates/loki-storage/src/migrations.rs` —
//! a source file, not the `migrations/` directory, and therefore outside
//! this node's writable scope. Likewise, `loki_storage::Storage`'s
//! underlying `rusqlite::Connection` is `pub(crate)`-only
//! (`crates/loki-storage/src/storage.rs`'s `lock`/`transaction`), so no
//! other crate can execute SQL against it without `loki-storage` itself
//! exposing a new public seam — again, an edit to `loki-storage/src` this
//! node is not scoped to make.
//!
//! Given that, this module implements telemetry/log/gap persistence as this
//! crate's *own* SQLite database — own connection, own tiny idempotent
//! migration runner (`self::migrations`, deliberately mirroring
//! `loki-storage`'s versioned-migration pattern rather than inventing a
//! different one), own file. This is recorded as an explicit decision in
//! `Docs/graph/handoffs/MON-001-1.md`, not a silent workaround: whether the
//! long-term architecture should instead be one shared database file (which
//! would need a small `DAT-001`-owning follow-up to register this table and
//! expose a connection seam) is exactly the kind of persistence-boundary
//! question `AGENTS.md`'s source-of-truth order reserves for the
//! orchestrator/an ADR, not something this node resolves unilaterally by
//! reaching into `loki-storage/src` anyway.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, Row, params};

use crate::error::MonitoringError;
use crate::gap::GapNotice;
use crate::sample::{LogRecord, RetentionClass, SampleQuality, TelemetrySample, TypedValue};
use loki_connectors_sdk::ids::{CorrelationId, DeviceId};

const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);

/// A durable SQLite-backed telemetry/log/gap store. See this module's doc
/// comment for why it owns its own connection rather than sharing
/// `loki_storage::Storage`'s.
pub struct SqliteTelemetryStore {
    conn: Mutex<Connection>,
    _key: Option<loki_storage::DatabaseKey>,
}

impl SqliteTelemetryStore {
    /// Opens (creating if absent) the SQLite database at `path` and applies
    /// this crate's own migrations. Safe to call on every process start —
    /// already-applied migrations are skipped (mirrors
    /// `loki_storage::Storage::run_migrations`'s idempotency).
    pub fn open(path: &Path) -> Result<Self, MonitoringError> {
        let key_path = loki_storage::secret::database_key_path(path);
        let (conn, key) = loki_storage::open_or_migrate_sqlcipher(path, &key_path)?;
        configure(&conn)?;
        migrations::apply_all(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            _key: Some(key),
        })
    }

    /// Opens a private in-memory database with migrations applied. No
    /// restart durability — used for tests that do not need a real file.
    pub fn open_in_memory() -> Result<Self, MonitoringError> {
        let conn = Connection::open_in_memory()?;
        configure(&conn)?;
        migrations::apply_all(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            _key: None,
        })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn insert_sample(&self, sample: &TelemetrySample) -> Result<(), MonitoringError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO telemetry_samples
                (source_id, metric_name, value, unit, source_time_ms, receive_time_ms,
                 quality, sequence, retention_class, correlation_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                sample.source_id.as_str(),
                sample.metric_name,
                sample.value.value,
                sample.value.unit,
                to_millis(sample.source_time),
                to_millis(sample.receive_time),
                sample.quality.as_wire_str(),
                sample.sequence.map(u64_to_i64),
                sample.retention_class.as_str(),
                sample.correlation_id.as_ref().map(CorrelationId::as_str),
            ],
        )?;
        Ok(())
    }

    pub fn insert_log(&self, log: &LogRecord) -> Result<(), MonitoringError> {
        let conn = self.lock();
        let redaction_markers_json = encode_string_list(&log.redaction_markers);
        conn.execute(
            "INSERT INTO log_records
                (source_id, level, timestamp_ms, message, schema_version, correlation_id,
                 redaction_markers_json, retention_class)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                log.source_id.as_str(),
                log.level,
                to_millis(log.timestamp),
                log.message,
                log.schema_version,
                log.correlation_id.as_ref().map(CorrelationId::as_str),
                redaction_markers_json,
                log.retention_class.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn insert_gap(&self, gap: &GapNotice) -> Result<(), MonitoringError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO telemetry_gaps
                (signal_key, last_seen_sequence, resumed_sequence, missing_count, detected_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                gap.signal_key,
                u64_to_i64(gap.last_seen_sequence),
                u64_to_i64(gap.resumed_sequence),
                u64_to_i64(gap.missing_count),
                to_millis(gap.detected_at),
            ],
        )?;
        Ok(())
    }

    /// All samples for `source_id`, oldest first. Used by restart tests and
    /// future read-side consumers.
    pub fn list_samples_for_source(
        &self,
        source_id: &DeviceId,
    ) -> Result<Vec<TelemetrySample>, MonitoringError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT source_id, metric_name, value, unit, source_time_ms, receive_time_ms,
                    quality, sequence, retention_class, correlation_id
             FROM telemetry_samples WHERE source_id = ?1 ORDER BY id ASC",
        )?;
        let raw_rows = stmt.query_map(params![source_id.as_str()], row_to_raw_sample)?;
        let mut out = Vec::new();
        for raw in raw_rows {
            out.push(raw_to_sample(raw?)?);
        }
        Ok(out)
    }

    /// Every recorded gap for `signal_key`, oldest first.
    pub fn list_gaps_for_signal(
        &self,
        signal_key: &str,
    ) -> Result<Vec<GapNotice>, MonitoringError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT signal_key, last_seen_sequence, resumed_sequence, missing_count, detected_at_ms
             FROM telemetry_gaps WHERE signal_key = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![signal_key], |row| {
            let last_seen_sequence: i64 = row.get(1)?;
            let resumed_sequence: i64 = row.get(2)?;
            let missing_count: i64 = row.get(3)?;
            let detected_at_ms: i64 = row.get(4)?;
            Ok(GapNotice {
                signal_key: row.get(0)?,
                last_seen_sequence: i64_to_u64(last_seen_sequence),
                resumed_sequence: i64_to_u64(resumed_sequence),
                missing_count: i64_to_u64(missing_count),
                detected_at: from_millis(detected_at_ms),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn sample_count(&self) -> Result<u64, MonitoringError> {
        let conn = self.lock();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM telemetry_samples", [], |r| r.get(0))?;
        Ok(count as u64)
    }

    pub fn gap_count(&self) -> Result<u64, MonitoringError> {
        let conn = self.lock();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM telemetry_gaps", [], |r| r.get(0))?;
        Ok(count as u64)
    }
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(())
}

fn to_millis(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        // A timestamp before the Unix epoch is not a realistic
        // telemetry/log time in this product's lifetime; clamp rather than
        // panic so a pathological input cannot crash the ingest path.
        Err(_) => 0,
    }
}

fn from_millis(ms: i64) -> SystemTime {
    if ms >= 0 {
        UNIX_EPOCH + Duration::from_millis(ms as u64)
    } else {
        UNIX_EPOCH - Duration::from_millis((-ms) as u64)
    }
}

/// `Docs/SPEC.md` §3.5 sequence numbers/counts are `u64` in this crate's
/// domain types, but `rusqlite` only implements `ToSql`/`FromSql` for
/// signed integer widths (SQLite integers are always signed 64-bit). A
/// sequence number/count large enough to overflow `i64` is not a realistic
/// value in this product's lifetime, so this is a lossless cast in
/// practice; `as` is used deliberately rather than a fallible conversion
/// that would otherwise have to invent an error variant for a case that
/// cannot occur.
fn u64_to_i64(value: u64) -> i64 {
    value as i64
}

fn i64_to_u64(value: i64) -> u64 {
    value as u64
}

/// Minimal dependency-free encoding for `Vec<String>` — a length-prefixed
/// scheme, avoiding a JSON dependency purely for one column's
/// list-of-strings shape (`AGENTS.md` rule 11).
fn encode_string_list(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("{}:{}", s.len(), s))
        .collect::<Vec<_>>()
        .join("")
}

#[allow(dead_code)] // round-trip counterpart of encode_string_list; exercised by this module's own tests, not yet by a public read path.
fn decode_string_list(encoded: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = encoded;
    while let Some(colon) = rest.find(':') {
        let (len_str, after_colon) = rest.split_at(colon);
        let after_colon = &after_colon[1..];
        let Ok(len) = len_str.parse::<usize>() else {
            break;
        };
        if after_colon.len() < len {
            break;
        }
        out.push(after_colon[..len].to_string());
        rest = &after_colon[len..];
    }
    out
}

struct RawSampleRow {
    source_id: String,
    metric_name: String,
    value: f64,
    unit: String,
    source_time_ms: i64,
    receive_time_ms: i64,
    quality: String,
    sequence: Option<i64>,
    retention_class: String,
    correlation_id: Option<String>,
}

fn row_to_raw_sample(row: &Row<'_>) -> rusqlite::Result<RawSampleRow> {
    Ok(RawSampleRow {
        source_id: row.get(0)?,
        metric_name: row.get(1)?,
        value: row.get(2)?,
        unit: row.get(3)?,
        source_time_ms: row.get(4)?,
        receive_time_ms: row.get(5)?,
        quality: row.get(6)?,
        sequence: row.get(7)?,
        retention_class: row.get(8)?,
        correlation_id: row.get(9)?,
    })
}

fn raw_to_sample(raw: RawSampleRow) -> Result<TelemetrySample, MonitoringError> {
    Ok(TelemetrySample {
        source_id: DeviceId::new(raw.source_id)?,
        metric_name: raw.metric_name,
        value: TypedValue {
            value: raw.value,
            unit: raw.unit,
        },
        source_time: from_millis(raw.source_time_ms),
        receive_time: from_millis(raw.receive_time_ms),
        quality: raw.quality.parse::<SampleQuality>()?,
        sequence: raw.sequence.map(i64_to_u64),
        retention_class: RetentionClass::new(raw.retention_class)?,
        correlation_id: raw.correlation_id.map(CorrelationId::new).transpose()?,
    })
}

mod migrations {
    use rusqlite::Connection;

    use crate::error::MonitoringError;

    const TELEMETRY_MIGRATION_SQL: &str = include_str!("../migrations/0001_telemetry.sql");

    pub(super) fn apply_all(conn: &Connection) -> Result<(), MonitoringError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS mon_schema_migrations (
                version    INTEGER NOT NULL PRIMARY KEY,
                name        TEXT NOT NULL
            ) STRICT;",
        )?;

        let already: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mon_schema_migrations WHERE version = 1",
            [],
            |r| r.get(0),
        )?;
        if already > 0 {
            return Ok(());
        }

        conn.execute_batch(TELEMETRY_MIGRATION_SQL)?;
        conn.execute(
            "INSERT INTO mon_schema_migrations (version, name) VALUES (1, 'telemetry')",
            [],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::SampleQuality;

    fn sample(seq: Option<u64>) -> TelemetrySample {
        let mut s = TelemetrySample::new(
            DeviceId::new("dev-1").unwrap(),
            "temperature_c",
            21.5,
            "C",
            SystemTime::UNIX_EPOCH + Duration::from_secs(1000),
            SystemTime::UNIX_EPOCH + Duration::from_secs(1001),
            SampleQuality::Good,
            RetentionClass::new("standard").unwrap(),
        )
        .unwrap();
        s.sequence = seq;
        s
    }

    #[test]
    fn open_in_memory_applies_migrations_and_starts_empty() {
        let store = SqliteTelemetryStore::open_in_memory().unwrap();
        assert_eq!(store.sample_count().unwrap(), 0);
        assert_eq!(store.gap_count().unwrap(), 0);
    }

    #[test]
    fn insert_and_read_back_a_sample_round_trips() {
        let store = SqliteTelemetryStore::open_in_memory().unwrap();
        let s = sample(Some(7));
        store.insert_sample(&s).unwrap();

        let read = store
            .list_samples_for_source(&DeviceId::new("dev-1").unwrap())
            .unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].metric_name, "temperature_c");
        assert_eq!(read[0].value.value, 21.5);
        assert_eq!(read[0].value.unit, "C");
        assert_eq!(read[0].sequence, Some(7));
        assert_eq!(read[0].quality, SampleQuality::Good);
    }

    #[test]
    fn insert_and_read_back_a_gap_round_trips() {
        let store = SqliteTelemetryStore::open_in_memory().unwrap();
        let gap = GapNotice {
            signal_key: "dev-1::temperature_c".to_string(),
            last_seen_sequence: 2,
            resumed_sequence: 5,
            missing_count: 2,
            detected_at: SystemTime::UNIX_EPOCH + Duration::from_secs(2000),
        };
        store.insert_gap(&gap).unwrap();
        let read = store.list_gaps_for_signal("dev-1::temperature_c").unwrap();
        assert_eq!(read, vec![gap]);
    }

    #[test]
    fn migrations_are_idempotent_across_repeated_opens_on_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("telemetry.sqlite3");

        {
            let store = SqliteTelemetryStore::open(&path).unwrap();
            store.insert_sample(&sample(Some(1))).unwrap();
        }
        // Reopening must not fail or duplicate schema application.
        let store = SqliteTelemetryStore::open(&path).unwrap();
        assert_eq!(store.sample_count().unwrap(), 1);
    }

    #[test]
    fn string_list_encoding_round_trips_including_empty_and_colon_containing_values() {
        let items = vec![
            "a".to_string(),
            "field:with:colons".to_string(),
            String::new(),
        ];
        let encoded = encode_string_list(&items);
        assert_eq!(decode_string_list(&encoded), items);
    }
}
