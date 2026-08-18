//! `Docs/graph/tasks/MON-001.md` Deliverable 5: "a gap/loss scenario is
//! detectable and reported honestly (not backfilled with fabricated data —
//! `AGENTS.md` rule 1)".

use std::time::{Duration, SystemTime};

use loki_connectors_sdk::ids::DeviceId;
use loki_monitoring::Pipeline;
use loki_monitoring::sample::{RetentionClass, SampleQuality, TelemetrySample};
use loki_monitoring::store::SqliteTelemetryStore;

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
fn a_sequence_gap_is_reported_with_the_exact_missing_count() {
    let pipeline = Pipeline::new(16, 16, 32);
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);

    pipeline.ingest_sample(sample(1, t0));
    pipeline.ingest_sample(sample(2, t0 + Duration::from_secs(1)));
    // Sequences 3, 4, 5 are lost in transit; 6 is the next one that arrives.
    let report = pipeline.ingest_sample(sample(6, t0 + Duration::from_secs(2)));

    let gap = report
        .gap
        .expect("a gap must be detected, not silently ignored");
    assert_eq!(gap.last_seen_sequence, 2);
    assert_eq!(gap.resumed_sequence, 6);
    assert_eq!(
        gap.missing_count, 3,
        "exactly sequences 3, 4, 5 are missing"
    );
}

#[test]
fn no_gap_is_reported_for_a_contiguous_sequence() {
    let pipeline = Pipeline::new(16, 16, 32);
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);

    pipeline.ingest_sample(sample(1, t0));
    let report = pipeline.ingest_sample(sample(2, t0 + Duration::from_secs(1)));
    assert!(report.gap.is_none());
}

#[test]
fn a_detected_gap_is_persisted_durably_when_a_store_is_attached() {
    let store = SqliteTelemetryStore::open_in_memory().unwrap();
    let pipeline = Pipeline::new(16, 16, 32).with_store(store);
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);

    pipeline.ingest_sample(sample(10, t0));
    pipeline.ingest_sample(sample(15, t0 + Duration::from_secs(1)));

    let gaps = pipeline
        .store()
        .unwrap()
        .list_gaps_for_signal("dev-1::temperature_c")
        .unwrap();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].missing_count, 4, "sequences 11..=14 are missing");
}

#[test]
fn a_gap_is_never_backfilled_with_a_fabricated_sample() {
    // The whole point of honest gap reporting: no synthetic sample for the
    // missing sequence numbers is ever created. Only the two real samples
    // that were actually ingested show up in the buffer/store.
    let store = SqliteTelemetryStore::open_in_memory().unwrap();
    let pipeline = Pipeline::new(16, 16, 32).with_store(store);
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);

    pipeline.ingest_sample(sample(1, t0));
    pipeline.ingest_sample(sample(100, t0 + Duration::from_secs(1)));

    assert_eq!(
        pipeline.store().unwrap().sample_count().unwrap(),
        2,
        "only the two real samples exist; nothing fabricated for sequences 2..=99"
    );
}
