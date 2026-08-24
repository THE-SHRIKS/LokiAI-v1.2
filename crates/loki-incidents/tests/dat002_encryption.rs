#![cfg(windows)]

mod support;

use std::fs;
use std::time::Duration;

use loki_connectors_sdk::ids::CapabilityId;
use loki_incidents::engine::IncidentEngine;
use loki_incidents::store::IncidentStore;
use loki_incidents::work_intent::DeliveryRequirements;
use rusqlite::Connection;
use support::*;

const MARKER: &str = "LOKIAI_INCIDENT_SECRET_MARKER_a91e";

#[test]
fn incident_evidence_file_migrates_and_remains_encrypted() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("incidents.sqlite3");
    let plaintext = Connection::open(&database).unwrap();
    plaintext
        .execute_batch("CREATE TABLE legacy(value TEXT NOT NULL);")
        .unwrap();
    plaintext
        .execute("INSERT INTO legacy VALUES (?1)", [MARKER])
        .unwrap();
    drop(plaintext);

    let store = IncidentStore::open(&database).unwrap();
    let engine = IncidentEngine::new(
        store,
        Duration::from_secs(300),
        Duration::from_secs(86_400),
        Duration::from_secs(604_800),
        Duration::from_secs(3_600),
    );
    engine
        .ingest_fired_outcome(
            &rule("encrypted-rule"),
            &device("encrypted-device"),
            &fired_outcome(epoch(1)),
            &[],
            CapabilityId::new("read_health").unwrap(),
            DeliveryRequirements::new(MARKER).unwrap(),
            epoch(1),
        )
        .unwrap();
    drop(engine);

    let bytes = fs::read(&database).unwrap();
    assert!(
        !bytes
            .windows(MARKER.len())
            .any(|window| window == MARKER.as_bytes())
    );
    let unkeyed = Connection::open(&database).unwrap();
    assert!(
        unkeyed
            .query_row("SELECT COUNT(*) FROM incidents", [], |row| row
                .get::<_, i64>(0))
            .is_err()
    );
}
