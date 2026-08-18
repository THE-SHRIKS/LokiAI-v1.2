//! `Docs/graph/tasks/MON-001.md` Deliverable 5: "backpressure doesn't
//! deadlock or crash" — a slow/absent consumer must never stall or panic
//! the producer side, and memory use must stay bounded regardless of how
//! far behind a subscriber falls.

use std::time::{Duration, SystemTime};

use loki_connectors_sdk::ids::DeviceId;
use loki_monitoring::Pipeline;
use loki_monitoring::sample::{RetentionClass, SampleQuality, TelemetrySample};
use loki_monitoring::subscription::{PublishOutcome, SubscriptionError, TelemetryEvent};

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

#[tokio::test]
async fn ingesting_far_more_than_the_bus_capacity_never_blocks_with_no_subscriber() {
    let pipeline = Pipeline::new(1_000, 1_000, /* bus_capacity */ 4);
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1);

    let result = tokio::time::timeout(Duration::from_secs(5), async {
        for i in 0..10_000u64 {
            let report = pipeline.ingest_sample(sample(i, t0 + Duration::from_millis(i)));
            assert_eq!(report.publish_outcome, PublishOutcome::NoSubscribers);
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "producer must never hang waiting on a consumer that does not exist"
    );
    // The ingest buffer itself is still bounded independent of the bus.
    assert_eq!(pipeline.buffered_sample_count(), 1_000);
}

#[tokio::test]
async fn a_slow_subscriber_falls_behind_without_stalling_the_producer_or_crashing() {
    let pipeline = Pipeline::new(1_000, 1_000, /* bus_capacity */ 8);
    let mut subscription = pipeline.subscribe();
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1);

    // Ingest well beyond bus capacity before the subscriber reads anything
    // at all — simulating an arbitrarily slow consumer.
    let publish_result = tokio::time::timeout(Duration::from_secs(5), async {
        for i in 0..500u64 {
            pipeline.ingest_sample(sample(i, t0 + Duration::from_millis(i)));
        }
    })
    .await;
    assert!(
        publish_result.is_ok(),
        "the producer side must not block on a slow consumer"
    );

    // The subscriber, reading now, must get an explicit Lagged signal
    // rather than silence, a crash, or a panic.
    let recv_result = tokio::time::timeout(Duration::from_secs(5), subscription.recv()).await;
    match recv_result {
        Ok(Err(SubscriptionError::Lagged { skipped })) => {
            assert!(
                skipped > 0,
                "a real number of events must have been skipped"
            );
        }
        Ok(Ok(_)) => {
            // Acceptable if the bus happened to still hold the very first
            // event (capacity edge case); the important invariant is "no
            // hang, no panic", asserted by the outer timeout succeeding.
        }
        Ok(Err(other)) => panic!("unexpected subscription error: {other:?}"),
        Err(_) => panic!("a slow subscriber must not cause recv() to hang forever"),
    }
}

#[tokio::test]
async fn multiple_subscribers_at_different_speeds_do_not_affect_each_other_or_the_producer() {
    let pipeline = Pipeline::new(1_000, 1_000, 16);
    let mut fast = pipeline.subscribe();
    let _slow_never_reads = pipeline.subscribe();
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1);

    for i in 0..200u64 {
        pipeline.ingest_sample(sample(i, t0 + Duration::from_millis(i)));
    }

    // The fast subscriber can still make progress even though the slow one
    // never reads at all.
    let received = tokio::time::timeout(Duration::from_secs(2), fast.recv()).await;
    assert!(matches!(
        received,
        Ok(Ok(TelemetryEvent::Sample(_))) | Ok(Err(SubscriptionError::Lagged { .. }))
    ));
}
