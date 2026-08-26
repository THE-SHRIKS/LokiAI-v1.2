#![cfg(windows)]

use std::fs;
use std::time::SystemTime;

use loki_connectors_sdk::ids::DeviceId;
use loki_monitoring::RuleStore;
use loki_monitoring::sample::{LogRecord, RetentionClass};
use loki_monitoring::store::SqliteTelemetryStore;
use rusqlite::Connection;

mod common;

const MARKER: &str = "LOKIAI_MONITORING_SECRET_MARKER_43f1";

fn contains(path: &std::path::Path, marker: &[u8]) -> bool {
    path.exists()
        && fs::read(path)
            .unwrap()
            .windows(marker.len())
            .any(|window| window == marker)
}

#[test]
fn telemetry_and_rules_share_one_migrated_encrypted_file() {
    common::ensure_ready();
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("monitoring.sqlite3");
    let plaintext = Connection::open(&database).unwrap();
    plaintext
        .execute_batch("CREATE TABLE legacy(value TEXT NOT NULL);")
        .unwrap();
    plaintext
        .execute("INSERT INTO legacy VALUES (?1)", [MARKER])
        .unwrap();
    drop(plaintext);

    let telemetry = SqliteTelemetryStore::open(&database).unwrap();
    let log = LogRecord::new(
        DeviceId::new("device-1").unwrap(),
        "info",
        SystemTime::UNIX_EPOCH,
        MARKER,
        "1",
        RetentionClass::new("standard").unwrap(),
    )
    .unwrap();
    telemetry.insert_log(&log).unwrap();
    let rules = RuleStore::open(&database).unwrap();
    drop(rules);
    drop(telemetry);

    assert!(!contains(&database, MARKER.as_bytes()));
    assert!(!contains(
        &database.with_extension("sqlite3-wal"),
        MARKER.as_bytes()
    ));
    let unkeyed = Connection::open(&database).unwrap();
    assert!(
        unkeyed
            .query_row("SELECT COUNT(*) FROM log_records", [], |row| row
                .get::<_, i64>(0))
            .is_err()
    );
}
