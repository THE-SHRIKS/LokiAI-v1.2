//! `Docs/graph/tasks/MON-001.md` Deliverable 5: "buffer overflow behavior
//! (drop policy proven, not assumed)". Exercised here through
//! `loki_monitoring::Pipeline`'s public ingest API, not the internal
//! `BoundedBuffer` type directly (that has its own unit tests in
//! `src/buffer.rs`) — this is the black-box, crate-external proof.

use std::time::{Duration, SystemTime};

use loki_connectors_sdk::ids::DeviceId;
use loki_monitoring::Pipeline;
use loki_monitoring::buffer::PushOutcome;
use loki_monitoring::sample::{RetentionClass, SampleQuality, TelemetrySample};

fn sample(seq: u64, t: SystemTime) -> TelemetrySample {
    TelemetrySample::new(
        DeviceId::new("dev-1").unwrap(),
        "temperature_c",
        20.0 + seq as f64,
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
fn buffer_never_exceeds_its_configured_capacity() {
    let pipeline = Pipeline::new(10, 10, 32);
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1);

    for i in 0..1_000u64 {
        pipeline.ingest_sample(sample(i, t0 + Duration::from_secs(i)));
    }

    assert_eq!(
        pipeline.buffered_sample_count(),
        10,
        "the capacity bound must be a real, enforced ceiling, never exceeded"
    );
}

#[test]
fn overflow_evicts_the_oldest_entry_and_reports_it_explicitly() {
    let pipeline = Pipeline::new(3, 10, 32);
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1);

    pipeline.ingest_sample(sample(1, t0));
    pipeline.ingest_sample(sample(2, t0 + Duration::from_secs(1)));
    pipeline.ingest_sample(sample(3, t0 + Duration::from_secs(2)));
    // Buffer is now full with sequences [1, 2, 3]. The next push must evict
    // sequence 1 (oldest), never 3 (newest) — proving the *drop-oldest*
    // policy, not merely "some" drop policy.
    let report = pipeline.ingest_sample(sample(4, t0 + Duration::from_secs(3)));

    match report.buffer_outcome {
        PushOutcome::AcceptedEvictedOldest(evicted) => {
            assert_eq!(
                evicted.sequence,
                Some(1),
                "the oldest entry, not the newest, is dropped"
            );
        }
        other => panic!("expected an eviction outcome, got {other:?}"),
    }
    assert_eq!(pipeline.buffered_sample_count(), 3);
}

#[test]
fn dropped_count_accounts_for_every_overflow_honestly() {
    let pipeline = Pipeline::new(5, 10, 32);
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1);

    for i in 0..50u64 {
        pipeline.ingest_sample(sample(i, t0 + Duration::from_secs(i)));
    }

    // 50 pushed, capacity 5 => 45 dropped. Not approximated, not silently
    // absorbed (AGENTS.md rule 1).
    assert_eq!(pipeline.dropped_sample_count(), 45);
    assert_eq!(pipeline.buffered_sample_count(), 5);
}

#[test]
fn log_buffer_has_its_own_independent_bound_and_drop_policy() {
    use loki_monitoring::sample::LogRecord;

    let pipeline = Pipeline::new(10, 2, 32);
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1);

    for i in 0..10u64 {
        let log = LogRecord::new(
            DeviceId::new("dev-1").unwrap(),
            "info",
            t0 + Duration::from_secs(i),
            format!("boot step {i}"),
            "1",
            RetentionClass::new("standard").unwrap(),
        )
        .unwrap();
        pipeline.ingest_log(log);
    }

    assert_eq!(
        pipeline.buffered_log_count(),
        2,
        "log buffer capacity is independent of the sample buffer's"
    );
    assert_eq!(pipeline.dropped_log_count(), 8);
}
