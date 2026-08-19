//! `Docs/graph/tasks/INC-001.md` Deliverable 3/10: repeated `Fired`/
//! `MissingDataAlert` outcomes for the same rule+scope correlate into one
//! incident with an incrementing `occurrence_count` within the correlation
//! window; outside the window, a fresh incident is created. Also covers the
//! `Verifying -> Investigating: recurrence` diagram edge, since a firing
//! that arrives while an incident is being verified is exactly that named
//! recurrence (see `crate::engine`'s module doc).

mod support;

use std::sync::{Arc, Barrier};
use std::time::Duration;

use loki_connectors_sdk::ids::CapabilityId;
use loki_incidents::incident::{IncidentState, VerificationPlan};
use loki_incidents::work_intent::DeliveryRequirements;
use support::*;

fn requirements() -> DeliveryRequirements {
    DeliveryRequirements::new("investigate").unwrap()
}

fn capability() -> CapabilityId {
    CapabilityId::new("read_health").unwrap()
}

#[test]
fn repeated_fires_within_the_correlation_window_increment_occurrence_count_not_a_fresh_incident() {
    let engine = engine_with_windows(
        Duration::from_secs(300),
        Duration::from_secs(86_400),
        Duration::from_secs(604_800),
        Duration::from_secs(3600),
    );
    let rule = rule("rule-1");
    let dev = device("dev-1");

    let first = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(epoch(0)),
            &[],
            capability(),
            requirements(),
            epoch(0),
        )
        .unwrap();
    assert!(!first.correlated);
    assert_eq!(first.incident.occurrence_count, 1);
    assert!(first.work_intent.is_some());

    // 60s later, well within the 300s correlation window.
    let second = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(epoch(60)),
            &[],
            capability(),
            requirements(),
            epoch(60),
        )
        .unwrap();
    assert!(second.correlated);
    assert_eq!(second.incident.incident_id, first.incident.incident_id);
    assert_eq!(second.incident.occurrence_count, 2);
    assert!(
        second.work_intent.is_none(),
        "a plain correlated recurrence onto an already-Investigating incident must not emit a second work intent"
    );
    assert_eq!(
        second.incident.state,
        IncidentState::Investigating,
        "correlation must not silently change state"
    );

    // A third fire, still within the window measured from the *second*
    // fire's updated_at.
    let third = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(epoch(120)),
            &[],
            capability(),
            requirements(),
            epoch(120),
        )
        .unwrap();
    assert!(third.correlated);
    assert_eq!(third.incident.occurrence_count, 3);
}

#[test]
fn a_fire_outside_the_correlation_window_creates_a_fresh_incident() {
    let engine = engine_with_windows(
        Duration::from_secs(300),
        Duration::from_secs(86_400),
        Duration::from_secs(604_800),
        Duration::from_secs(3600),
    );
    let rule = rule("rule-1");
    let dev = device("dev-1");

    let first = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(epoch(0)),
            &[],
            capability(),
            requirements(),
            epoch(0),
        )
        .unwrap();

    // 10,000s later - far outside the 300s correlation window.
    let second = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(epoch(10_000)),
            &[],
            capability(),
            requirements(),
            epoch(10_000),
        )
        .unwrap();

    assert!(!second.correlated);
    assert_ne!(second.incident.incident_id, first.incident.incident_id);
    assert_eq!(second.incident.occurrence_count, 1);
    assert!(
        second.work_intent.is_some(),
        "a brand-new incident must emit its own work intent"
    );
}

#[test]
fn different_devices_never_correlate_with_each_others_incidents() {
    let engine = default_engine();
    let rule = rule("rule-1");

    let a = engine
        .ingest_fired_outcome(
            &rule,
            &device("dev-1"),
            &fired_outcome(epoch(0)),
            &[],
            capability(),
            requirements(),
            epoch(0),
        )
        .unwrap();
    let b = engine
        .ingest_fired_outcome(
            &rule,
            &device("dev-2"),
            &fired_outcome(epoch(1)),
            &[],
            capability(),
            requirements(),
            epoch(1),
        )
        .unwrap();

    assert_ne!(a.incident.incident_id, b.incident.incident_id);
    assert!(!b.correlated);
}

#[test]
fn a_recurrence_while_verifying_forces_the_named_recurrence_edge_back_to_investigating() {
    let engine = engine_with_windows(
        Duration::from_secs(300),
        Duration::from_secs(86_400),
        Duration::from_secs(604_800),
        Duration::from_secs(3600),
    );
    let rule = rule("rule-1");
    let dev = device("dev-1");

    let created = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(epoch(0)),
            &[],
            capability(),
            requirements(),
            epoch(0),
        )
        .unwrap();
    let plan = VerificationPlan::new("watch", Duration::from_secs(300)).unwrap();
    let verifying = engine
        .no_device_change_required(&created.incident.incident_id, plan, epoch(10), None)
        .unwrap();
    assert_eq!(verifying.state, IncidentState::Verifying);

    // A new firing arrives 20s later, well within the correlation window,
    // while the incident is still being verified.
    let recurred = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(epoch(30)),
            &[],
            capability(),
            requirements(),
            epoch(30),
        )
        .unwrap();

    assert!(recurred.correlated);
    assert_eq!(recurred.incident.incident_id, created.incident.incident_id);
    assert_eq!(
        recurred.incident.state,
        IncidentState::Investigating,
        "Docs/SPEC.md §4.6's Verifying -> Investigating recurrence edge must fire"
    );
    assert_eq!(recurred.incident.occurrence_count, 2);
    assert!(
        recurred.work_intent.is_some(),
        "re-entering Investigating due to a verification recurrence emits a fresh work intent"
    );
}

#[test]
fn missing_data_alert_outcomes_correlate_exactly_like_fired_outcomes() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let dev = device("dev-1");

    let first = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &missing_data_alert_outcome(epoch(0)),
            &[],
            capability(),
            requirements(),
            epoch(0),
        )
        .unwrap();
    assert_eq!(first.incident.state, IncidentState::Investigating);

    let second = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &missing_data_alert_outcome(epoch(10)),
            &[],
            capability(),
            requirements(),
            epoch(10),
        )
        .unwrap();
    assert!(second.correlated);
    assert_eq!(second.incident.occurrence_count, 2);
}

#[test]
fn simultaneous_fires_for_the_same_rule_and_device_cannot_create_duplicate_incidents() {
    const FIRINGS: usize = 8;
    let engine = Arc::new(default_engine());
    let start = Arc::new(Barrier::new(FIRINGS));

    let handles: Vec<_> = (0..FIRINGS)
        .map(|_| {
            let engine = Arc::clone(&engine);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                let rule = rule("rule-race");
                let dev = device("dev-race");
                start.wait();
                engine
                    .ingest_fired_outcome(
                        &rule,
                        &dev,
                        &fired_outcome(epoch(10)),
                        &[],
                        capability(),
                        requirements(),
                        epoch(10),
                    )
                    .unwrap()
            })
        })
        .collect();

    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("ingest thread must not panic"))
        .collect();
    let incident_id = results[0].incident.incident_id.clone();

    assert!(
        results
            .iter()
            .all(|result| result.incident.incident_id == incident_id),
        "every simultaneous firing must correlate into one incident"
    );
    assert_eq!(
        engine.get(&incident_id).unwrap().occurrence_count,
        FIRINGS as u32
    );
    assert_eq!(
        results.iter().filter(|result| !result.correlated).count(),
        1,
        "exactly one firing creates the incident; all others correlate"
    );
}
