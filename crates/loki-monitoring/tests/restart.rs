//! `Docs/graph/tasks/MON-001.md` Deliverable 4/5: "telemetry state/
//! buffering behaves sanely across a process restart (matches `DAT-001`'s
//! restart-preserves-data discipline where storage is involved)" / "restart
//! scenario" test.
//!
//! Simulates a process restart by dropping every in-process `Pipeline`
//! value and constructing a brand new one against the same on-disk SQLite
//! file — there is no way to literally restart the OS process from within
//! a test, so this is the same "close and reopen a real file" technique
//! `crates/loki-storage/tests/restart.rs` uses for `DAT-001`.

use std::time::{Duration, SystemTime};

use loki_connectors_sdk::ids::DeviceId;
use loki_monitoring::Pipeline;
use loki_monitoring::sample::{RetentionClass, SampleQuality, TelemetrySample};
use loki_monitoring::store::SqliteTelemetryStore;

mod common;

fn sample(seq: u64, t: SystemTime) -> TelemetrySample {
    TelemetrySample::new(
        DeviceId::new("dev-1").unwrap(),
        "temperature_c",
        21.0,
        "C",
        t,
        t,
        SampleQuality::Good,
        RetentionClass::new("standard").unwrap(),
    )
    .unwrap()
    .with_sequence(seq)
}

#[test]
fn durably_persisted_samples_survive_a_simulated_process_restart() {
    common::ensure_ready();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("telemetry.sqlite3");
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(5000);

    {
        let store = SqliteTelemetryStore::open(&db_path).unwrap();
        let pipeline = Pipeline::new(8, 8, 16).with_store(store);
        pipeline.ingest_sample(sample(1, t0));
        pipeline.ingest_sample(sample(2, t0 + Duration::from_secs(1)));
        pipeline.ingest_sample(sample(3, t0 + Duration::from_secs(2)));
        // `pipeline` (and its in-memory buffer/freshness/bus/store handle)
        // is dropped here — simulating process shutdown.
    }

    // "Restart": open a brand new store/pipeline against the same file.
    let reopened_store = SqliteTelemetryStore::open(&db_path).unwrap();
    let device = DeviceId::new("dev-1").unwrap();
    let restored = reopened_store.list_samples_for_source(&device).unwrap();

    assert_eq!(
        restored.len(),
        3,
        "durably persisted samples must survive the restart"
    );
    assert_eq!(restored[0].sequence, Some(1));
    assert_eq!(restored[2].sequence, Some(3));
}

#[test]
fn a_detected_gap_also_survives_a_simulated_process_restart() {
    common::ensure_ready();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("telemetry.sqlite3");
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(5000);

    {
        let store = SqliteTelemetryStore::open(&db_path).unwrap();
        let pipeline = Pipeline::new(8, 8, 16).with_store(store);
        pipeline.ingest_sample(sample(1, t0));
        pipeline.ingest_sample(sample(5, t0 + Duration::from_secs(1))); // gap: 2,3,4 missing
    }

    let reopened_store = SqliteTelemetryStore::open(&db_path).unwrap();
    let gaps = reopened_store
        .list_gaps_for_signal("dev-1::temperature_c")
        .unwrap();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].missing_count, 3);
}

#[test]
fn in_memory_working_state_honestly_resets_after_restart_it_is_not_fabricated_as_still_fresh() {
    // Docs/SPEC.md §4.3: "Device status becomes unknown after its
    // freshness window; it must not remain healthy solely because the last
    // sample was healthy." This crate applies the same honesty to a fresh
    // `Pipeline`: it has no in-memory memory of pre-restart freshness at
    // all, and must say so (`Unknown`), never claim `Fresh` based on
    // durable history it has not actually re-observed.
    common::ensure_ready();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("telemetry.sqlite3");
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(5000);

    {
        let store = SqliteTelemetryStore::open(&db_path).unwrap();
        let pipeline = Pipeline::new(8, 8, 16).with_store(store);
        pipeline.ingest_sample(sample(1, t0));
    }

    let reopened_store = SqliteTelemetryStore::open(&db_path).unwrap();
    let fresh_pipeline = Pipeline::new(8, 8, 16).with_store(reopened_store);

    let freshness = fresh_pipeline.freshness(
        "dev-1::temperature_c",
        t0 + Duration::from_secs(1),
        Duration::from_secs(3600),
    );
    assert_eq!(
        freshness,
        loki_monitoring::freshness::Freshness::Unknown,
        "a freshly restarted pipeline must not fabricate in-memory freshness from durable history alone"
    );

    // But the durable data itself is still there, honestly retrievable.
    let device = DeviceId::new("dev-1").unwrap();
    assert_eq!(
        fresh_pipeline
            .store()
            .unwrap()
            .list_samples_for_source(&device)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn reopening_the_same_file_twice_in_a_row_is_idempotent() {
    common::ensure_ready();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("telemetry.sqlite3");

    let _first = SqliteTelemetryStore::open(&db_path).unwrap();
    let _second = SqliteTelemetryStore::open(&db_path).unwrap();
    // No panic/error on the second open — migrations do not attempt to
    // reapply and fail on already-existing tables.
}
