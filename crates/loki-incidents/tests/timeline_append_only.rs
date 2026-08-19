//! `Docs/graph/tasks/INC-001.md` Deliverable 6/10: every state transition
//! and assignment change is recorded in an append-only timeline, not
//! overwritten. This test reads the timeline back from
//! `crate::store::IncidentStore` directly (not just the in-memory
//! `Incident` struct) after each of several mutating operations, and proves
//! two things mechanically: the persisted timeline only ever grows, and
//! every previously read entry remains byte-identical afterward (never
//! rewritten in place).

mod support;

use loki_connectors_sdk::ids::CapabilityId;
use loki_incidents::incident::{Assignee, AssigneeKind};
use loki_incidents::work_intent::DeliveryRequirements;
use support::*;

fn requirements() -> DeliveryRequirements {
    DeliveryRequirements::new("investigate").unwrap()
}

fn capability() -> CapabilityId {
    CapabilityId::new("read_health").unwrap()
}

#[test]
fn the_persisted_timeline_only_ever_grows_and_never_rewrites_a_prior_entry() {
    let engine = default_engine();
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
    let incident_id = created.incident.incident_id.clone();

    let mut previous = engine.store().timeline(&incident_id).unwrap();
    assert!(!previous.is_empty());

    // A sequence of real mutating operations, each of which must append.
    engine
        .assign(
            &incident_id,
            Assignee::new(AssigneeKind::User, "user-1").unwrap(),
            epoch(1),
        )
        .unwrap();
    let after_assign = engine.store().timeline(&incident_id).unwrap();
    assert!(after_assign.len() > previous.len());
    assert_eq!(&after_assign[..previous.len()], &previous[..]);
    previous = after_assign;

    engine
        .set_diagnosis_summary(&incident_id, "root cause found", epoch(2))
        .unwrap();
    let after_diagnosis = engine.store().timeline(&incident_id).unwrap();
    assert!(after_diagnosis.len() > previous.len());
    assert_eq!(&after_diagnosis[..previous.len()], &previous[..]);
    previous = after_diagnosis;

    engine
        .request_approval(&incident_id, epoch(3), None)
        .unwrap();
    let after_approval_request = engine.store().timeline(&incident_id).unwrap();
    assert!(after_approval_request.len() > previous.len());
    assert_eq!(&after_approval_request[..previous.len()], &previous[..]);
    previous = after_approval_request;

    engine
        .approval_denied_or_expired(&incident_id, epoch(4), Some("expired".to_string()))
        .unwrap();
    let after_denied = engine.store().timeline(&incident_id).unwrap();
    assert!(after_denied.len() > previous.len());
    assert_eq!(&after_denied[..previous.len()], &previous[..]);

    // A second firing correlates onto the same incident and must also only
    // append (a correlated-occurrence timeline entry), never touch prior
    // entries.
    let previous = after_denied;
    engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(epoch(5)),
            &[],
            capability(),
            requirements(),
            epoch(5),
        )
        .unwrap();
    let after_second_fire = engine.store().timeline(&incident_id).unwrap();
    assert!(after_second_fire.len() > previous.len());
    assert_eq!(&after_second_fire[..previous.len()], &previous[..]);
}
