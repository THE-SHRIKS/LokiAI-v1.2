//! `Docs/graph/tasks/INC-001.md` Deliverable 7/10: `Verifying -> Resolved`
//! requires every check to pass for a real, tested observation window
//! (fake-clock-driven — every timestamp below is an explicit
//! `SystemTime`, never real wall-clock time), not a single instantaneous
//! check.

mod support;

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

fn incident_in_verifying(
    engine: &loki_incidents::IncidentEngine,
    window: Duration,
    verifying_entered_at: std::time::SystemTime,
) -> loki_incidents::ids::IncidentId {
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
    let plan = VerificationPlan::new("watch", window).unwrap();
    engine
        .no_device_change_required(
            &created.incident.incident_id,
            plan,
            verifying_entered_at,
            None,
        )
        .unwrap();
    created.incident.incident_id
}

#[test]
fn a_single_instantaneous_passing_check_never_resolves_on_its_own() {
    let engine = default_engine();
    let window = Duration::from_secs(300);
    let incident_id = incident_in_verifying(&engine, window, epoch(0));

    // Same instant Verifying was entered - zero elapsed.
    let incident = engine
        .record_verification_check(&incident_id, true, epoch(0), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Verifying);
}

#[test]
fn a_passing_check_one_second_before_the_window_elapses_does_not_resolve() {
    let engine = default_engine();
    let window = Duration::from_secs(300);
    let incident_id = incident_in_verifying(&engine, window, epoch(1000));

    let incident = engine
        .record_verification_check(&incident_id, true, epoch(1000 + 299), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Verifying);
}

#[test]
fn a_passing_check_exactly_at_the_window_boundary_resolves() {
    let engine = default_engine();
    let window = Duration::from_secs(300);
    let incident_id = incident_in_verifying(&engine, window, epoch(1000));

    let incident = engine
        .record_verification_check(&incident_id, true, epoch(1000 + 300), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Resolved);
}

#[test]
fn a_passing_check_well_past_the_window_also_resolves() {
    let engine = default_engine();
    let window = Duration::from_secs(300);
    let incident_id = incident_in_verifying(&engine, window, epoch(1000));

    let incident = engine
        .record_verification_check(&incident_id, true, epoch(1000 + 9_999), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Resolved);
}

#[test]
fn multiple_passing_checks_across_the_window_all_must_pass_a_single_failure_anywhere_prevents_resolution()
 {
    let engine = default_engine();
    let window = Duration::from_secs(300);
    let incident_id = incident_in_verifying(&engine, window, epoch(0));

    // Passes at 50s, 100s, 150s...
    for t in [50, 100, 150, 200] {
        let incident = engine
            .record_verification_check(&incident_id, true, epoch(t), None)
            .unwrap();
        assert_eq!(incident.state, IncidentState::Verifying);
    }

    // A failure at 250s must immediately abandon the cycle, even though
    // every prior check in this same window passed.
    let incident = engine
        .record_verification_check(&incident_id, false, epoch(250), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Investigating);

    // Time continuing to pass after the abandoned cycle must not somehow
    // resolve it - it is no longer even Verifying.
    let err = engine.record_verification_check(&incident_id, true, epoch(400), None);
    assert!(
        err.is_err(),
        "cannot record a verification check once the incident left Verifying"
    );
}

#[test]
fn a_fresh_verification_cycle_after_recurrence_requires_the_full_window_again() {
    let engine = default_engine();
    let window = Duration::from_secs(300);
    let incident_id = incident_in_verifying(&engine, window, epoch(0));

    // Fail immediately -> Investigating.
    engine
        .record_verification_check(&incident_id, false, epoch(10), None)
        .unwrap();

    // Re-enter Verifying with a fresh cycle.
    let plan = VerificationPlan::new("watch again", window).unwrap();
    let incident = engine
        .no_device_change_required(&incident_id, plan, epoch(20), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Verifying);

    // A pass immediately after re-entering must not resolve — it must wait
    // out the fresh cycle's own full window, not reuse any elapsed time
    // from the abandoned first cycle.
    let incident = engine
        .record_verification_check(&incident_id, true, epoch(20 + 299), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Verifying);

    let incident = engine
        .record_verification_check(&incident_id, true, epoch(20 + 300), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Resolved);
}
