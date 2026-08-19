//! `Docs/graph/tasks/INC-001.md` Deliverable 4/10: incidents reference
//! bounded evidence snapshots, never an unbounded/unlimited evidence blob.
//! Two independent bounds are proven here: the per-snapshot sample bound
//! (`crate::evidence::MAX_SAMPLES_PER_SNAPSHOT`) and the per-incident live
//! evidence-reference-list bound
//! (`crate::incident::MAX_EVIDENCE_REFS_PER_INCIDENT`).

mod support;

use std::time::Duration;

use loki_connectors_sdk::ids::CapabilityId;
use loki_incidents::incident::MAX_EVIDENCE_REFS_PER_INCIDENT;
use loki_incidents::work_intent::DeliveryRequirements;
use loki_monitoring::sample::{RetentionClass, SampleQuality, TelemetrySample};
use support::*;

fn requirements() -> DeliveryRequirements {
    DeliveryRequirements::new("investigate").unwrap()
}

fn capability() -> CapabilityId {
    CapabilityId::new("read_health").unwrap()
}

fn sample(t: std::time::SystemTime, value: f64) -> TelemetrySample {
    TelemetrySample::new(
        device("dev-1"),
        "temperature_c",
        value,
        "C",
        t,
        t,
        SampleQuality::Good,
        RetentionClass::new("standard").unwrap(),
    )
    .unwrap()
}

#[test]
fn a_snapshot_captured_from_far_more_samples_than_the_bound_still_stores_a_bounded_count() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let dev = device("dev-1");

    let samples: Vec<TelemetrySample> = (0..500).map(|i| sample(epoch(i), i as f64)).collect();

    let result = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(epoch(500)),
            &samples,
            capability(),
            requirements(),
            epoch(500),
        )
        .unwrap();

    let evidence_id = result
        .incident
        .evidence_refs
        .last()
        .expect("at least one evidence ref");
    let snapshot = engine
        .store()
        .get_evidence_snapshot(evidence_id)
        .unwrap()
        .expect("snapshot must be durably stored");

    assert_eq!(
        snapshot.samples.len(),
        loki_incidents::evidence::MAX_SAMPLES_PER_SNAPSHOT,
        "a snapshot must never store an unbounded number of samples"
    );
}

#[test]
fn an_incidents_live_evidence_reference_list_never_exceeds_its_bound() {
    let engine = engine_with_windows(
        Duration::from_secs(1_000_000), // huge — every fire correlates
        Duration::from_secs(86_400),
        Duration::from_secs(604_800),
        Duration::from_secs(3600),
    );
    let rule = rule("rule-1");
    let dev = device("dev-1");

    let mut last_incident_id = None;
    for i in 0..(MAX_EVIDENCE_REFS_PER_INCIDENT as u64 + 25) {
        let result = engine
            .ingest_fired_outcome(
                &rule,
                &dev,
                &fired_outcome(epoch(i)),
                &[sample(epoch(i), i as f64)],
                capability(),
                requirements(),
                epoch(i),
            )
            .unwrap();
        last_incident_id = Some(result.incident.incident_id.clone());
        assert!(
            result.incident.evidence_refs.len() <= MAX_EVIDENCE_REFS_PER_INCIDENT,
            "evidence_refs must never exceed the documented bound (was {})",
            result.incident.evidence_refs.len()
        );
    }

    let final_incident = engine.get(&last_incident_id.unwrap()).unwrap();
    assert_eq!(
        final_incident.evidence_refs.len(),
        MAX_EVIDENCE_REFS_PER_INCIDENT,
        "after exceeding the bound repeatedly, the live list must settle exactly at the bound"
    );
}
