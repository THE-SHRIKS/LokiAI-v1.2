//! `Docs/graph/tasks/INC-001.md` Deliverable 8/10:
//! - `Resolved -> Open` only within a documented reopen-policy window
//!   (triggered by a new firing recurring — `Resolved -> Closed` is a
//!   separate, time-based retention sweep, not triggered by a firing).
//! - `Resolved -> Closed` via `crate::engine::IncidentEngine::
//!   apply_retention_policy`, a real, fake-clock-tested policy sweep.

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

/// Drives a freshly created incident all the way to `Resolved` at
/// `resolved_at`, returning its id.
fn resolved_incident(
    engine: &loki_incidents::IncidentEngine,
    rule: &loki_monitoring::rule::MonitoringRule,
    dev: &loki_connectors_sdk::ids::DeviceId,
    resolved_at: std::time::SystemTime,
) -> loki_incidents::ids::IncidentId {
    let created = engine
        .ingest_fired_outcome(
            rule,
            dev,
            &fired_outcome(epoch(0)),
            &[],
            capability(),
            requirements(),
            epoch(0),
        )
        .unwrap();
    let window = Duration::from_secs(60);
    let plan = VerificationPlan::new("watch", window).unwrap();
    engine
        .no_device_change_required(&created.incident.incident_id, plan, epoch(10), None)
        .unwrap();
    let resolved = engine
        .record_verification_check(&created.incident.incident_id, true, resolved_at, None)
        .unwrap();
    assert_eq!(resolved.state, IncidentState::Resolved);
    created.incident.incident_id
}

#[test]
fn a_recurrence_within_the_reopen_policy_window_reopens_the_same_incident() {
    let reopen_window = Duration::from_secs(3600);
    let engine = engine_with_windows(
        Duration::from_secs(300),
        reopen_window,
        Duration::from_secs(604_800),
        Duration::from_secs(3600),
    );
    let rule = rule("rule-1");
    let dev = device("dev-1");

    // Verifying entered at 10, plan window 60s -> resolves at 70.
    let incident_id = resolved_incident(&engine, &rule, &dev, epoch(70));

    // A new firing 100s after resolution - well within the 3600s reopen
    // window.
    let recurrence_at = epoch(70 + 100);
    let recurred = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(recurrence_at),
            &[],
            capability(),
            requirements(),
            recurrence_at,
        )
        .unwrap();

    assert_eq!(recurred.incident.incident_id, incident_id);
    assert_eq!(
        recurred.incident.state,
        IncidentState::Investigating,
        "Resolved -> Open -> Investigating (Open has no other outgoing edge)"
    );
    assert!(
        recurred.work_intent.is_some(),
        "a reopen due to recurrence emits a fresh work intent"
    );
}

#[test]
fn a_recurrence_after_the_reopen_policy_window_creates_a_fresh_incident_instead() {
    let reopen_window = Duration::from_secs(3600);
    let engine = engine_with_windows(
        Duration::from_secs(300),
        reopen_window,
        Duration::from_secs(604_800),
        Duration::from_secs(3600),
    );
    let rule = rule("rule-1");
    let dev = device("dev-1");

    let incident_id = resolved_incident(&engine, &rule, &dev, epoch(70));

    // A new firing 10,000s after resolution - well outside the 3600s reopen
    // window.
    let recurrence_at = epoch(70 + 10_000);
    let recurred = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(recurrence_at),
            &[],
            capability(),
            requirements(),
            recurrence_at,
        )
        .unwrap();

    assert_ne!(
        recurred.incident.incident_id, incident_id,
        "outside the reopen policy window, the old Resolved incident must not be reopened"
    );
    assert_eq!(recurred.incident.state, IncidentState::Investigating);
    assert_eq!(recurred.incident.occurrence_count, 1);

    // The original incident is untouched — still Resolved, not silently
    // mutated.
    let original = engine.get(&incident_id).unwrap();
    assert_eq!(original.state, IncidentState::Resolved);
}

#[test]
fn retention_policy_closes_a_resolved_incident_once_the_retention_window_elapses() {
    let retention_window = Duration::from_secs(604_800); // 7 days
    let engine = engine_with_windows(
        Duration::from_secs(300),
        Duration::from_secs(3600),
        retention_window,
        Duration::from_secs(3600),
    );
    let rule = rule("rule-1");
    let dev = device("dev-1");

    let incident_id = resolved_incident(&engine, &rule, &dev, epoch(70));

    // Sweep well before the retention window elapses - must not close yet.
    let too_early = epoch(70 + 1000);
    let closed = engine.apply_retention_policy(too_early).unwrap();
    assert!(closed.is_empty());
    assert_eq!(
        engine.get(&incident_id).unwrap().state,
        IncidentState::Resolved
    );

    // Sweep after the retention window has fully elapsed.
    let after_retention = epoch(70) + retention_window + Duration::from_secs(1);
    let closed = engine.apply_retention_policy(after_retention).unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].incident_id, incident_id);
    assert_eq!(
        engine.get(&incident_id).unwrap().state,
        IncidentState::Closed
    );
}

#[test]
fn a_closed_incident_never_correlates_or_reopens_a_new_firing_always_starts_fresh() {
    let engine = engine_with_windows(
        Duration::from_secs(300),
        Duration::from_secs(3600),
        Duration::from_secs(1), // retention window of 1s so it closes almost immediately
        Duration::from_secs(3600),
    );
    let rule = rule("rule-1");
    let dev = device("dev-1");

    let incident_id = resolved_incident(&engine, &rule, &dev, epoch(70));
    let closed = engine.apply_retention_policy(epoch(72)).unwrap();
    assert_eq!(closed.len(), 1);

    // A brand-new firing right after closing (well within what would have
    // been every other window) must never touch the closed incident.
    let after = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(epoch(73)),
            &[],
            capability(),
            requirements(),
            epoch(73),
        )
        .unwrap();

    assert_ne!(after.incident.incident_id, incident_id);
    assert_eq!(after.incident.occurrence_count, 1);
    assert_eq!(
        engine.get(&incident_id).unwrap().state,
        IncidentState::Closed,
        "the closed incident itself must remain untouched"
    );
}
