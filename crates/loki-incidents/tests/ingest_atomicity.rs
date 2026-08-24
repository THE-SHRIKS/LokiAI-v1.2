//! Proves that a database failure cannot leave orphaned evidence or a
//! successful-looking emitted work intent without its incident.

mod support;

use std::time::Duration;

use loki_connectors_sdk::ids::CapabilityId;
use loki_incidents::engine::IncidentEngine;
use loki_incidents::store::IncidentStore;
use loki_incidents::work_intent::DeliveryRequirements;
use rusqlite::Connection;
use support::*;

fn encrypted_connection(path: &std::path::Path) -> Connection {
    let key = loki_storage::secret::load_or_create_key_for_database(path).unwrap();
    let connection = Connection::open(path).unwrap();
    loki_storage::configure_sqlcipher(&connection, &key).unwrap();
    connection
}

#[test]
fn a_failed_incident_write_rolls_back_evidence_and_work_intent_together() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("incidents.sqlite3");
    let store = IncidentStore::open(&path).unwrap();

    // Force a real failure at the final incident-row stage, after the
    // transaction has attempted its evidence and work-intent inserts.
    let fault_connection = encrypted_connection(&path);
    fault_connection
        .execute_batch(
            "CREATE TRIGGER force_incident_insert_failure
             BEFORE INSERT ON incidents
             BEGIN
                 SELECT RAISE(ABORT, 'forced incident insert failure');
             END;",
        )
        .unwrap();
    drop(fault_connection);

    let engine = IncidentEngine::new(
        store,
        Duration::from_secs(300),
        Duration::from_secs(86_400),
        Duration::from_secs(604_800),
        Duration::from_secs(3_600),
    );
    let result = engine.ingest_fired_outcome(
        &rule("rule-atomic"),
        &device("device-atomic"),
        &fired_outcome(epoch(1)),
        &[],
        CapabilityId::new("read_health").unwrap(),
        DeliveryRequirements::new("investigate atomically").unwrap(),
        epoch(1),
    );
    assert!(
        result.is_err(),
        "the injected database failure must surface"
    );

    let proof_connection = encrypted_connection(&path);
    for table in [
        "incidents",
        "incident_timeline",
        "incident_evidence_snapshots",
        "incident_work_intents",
    ] {
        let count: i64 = proof_connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            count, 0,
            "{table} must contain no partial result after rollback"
        );
    }
}

#[test]
fn an_unrepresentable_expiry_is_an_error_not_a_panic_or_partial_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("expiry-overflow.sqlite3");
    let store = IncidentStore::open(&path).unwrap();
    let engine = IncidentEngine::new(
        store,
        Duration::from_secs(300),
        Duration::from_secs(86_400),
        Duration::from_secs(604_800),
        Duration::MAX,
    );

    let result = engine.ingest_fired_outcome(
        &rule("rule-overflow"),
        &device("device-overflow"),
        &fired_outcome(epoch(1)),
        &[],
        CapabilityId::new("read_health").unwrap(),
        DeliveryRequirements::new("investigate safely").unwrap(),
        epoch(1),
    );
    assert!(result.is_err(), "overflow must be returned, never panic");

    let proof_connection = encrypted_connection(&path);
    for table in [
        "incidents",
        "incident_timeline",
        "incident_evidence_snapshots",
        "incident_work_intents",
    ] {
        let count: i64 = proof_connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} must remain empty after overflow");
    }
}
