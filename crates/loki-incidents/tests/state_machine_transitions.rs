//! Black-box coverage of every named edge in `Docs/SPEC.md` §4.6's diagram
//! (`Docs/graph/tasks/INC-001.md` Deliverable 10). Reopen/close-policy edges
//! live in `tests/reopen_close_policy.rs`; the `Verifying -> Investigating`
//! recurrence-via-new-firing edge lives in `tests/correlation.rs`
//! (alongside the failed-check path exercised here).

mod support;

use std::time::Duration;

use loki_connectors_sdk::ids::CapabilityId;
use loki_incidents::incident::{Assignee, AssigneeKind, IncidentState, VerificationPlan};
use loki_incidents::work_intent::DeliveryRequirements;
use support::*;

fn requirements() -> DeliveryRequirements {
    DeliveryRequirements::new("investigate and propose a fix").unwrap()
}

fn capability() -> CapabilityId {
    CapabilityId::new("read_health").unwrap()
}

#[test]
fn creation_lands_at_open_then_immediately_investigating_with_a_work_intent() {
    // [*] --> Open --> Investigating, atomically, per crate::engine's
    // documented reasoning for why Open is never a resting state.
    let engine = default_engine();
    let rule = rule("rule-1");
    let outcome = fired_outcome(epoch(100));
    let result = engine
        .ingest_fired_outcome(
            &rule,
            &device("dev-1"),
            &outcome,
            &[],
            capability(),
            requirements(),
            epoch(100),
        )
        .unwrap();

    assert_eq!(result.incident.state, IncidentState::Investigating);
    assert!(!result.correlated);
    let work_intent = result.work_intent.expect("must emit a work intent");
    assert_eq!(work_intent.incident_id, result.incident.incident_id);
    assert_eq!(work_intent.requested_capability, capability());
}

#[test]
fn investigating_to_waiting_approval() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let created = engine
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

    let incident = engine
        .request_approval(&created.incident.incident_id, epoch(10), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::WaitingApproval);
}

#[test]
fn investigating_to_verifying_no_device_change_required() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let created = engine
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

    let plan = VerificationPlan::new("watch for 5 minutes", Duration::from_secs(300)).unwrap();
    let incident = engine
        .no_device_change_required(&created.incident.incident_id, plan, epoch(10), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Verifying);
    assert!(incident.verification_plan.is_some());
}

#[test]
fn waiting_approval_denied_or_expired_returns_to_investigating() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let created = engine
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
    engine
        .request_approval(&created.incident.incident_id, epoch(10), None)
        .unwrap();

    let incident = engine
        .approval_denied_or_expired(
            &created.incident.incident_id,
            epoch(20),
            Some("expired".to_string()),
        )
        .unwrap();
    assert_eq!(incident.state, IncidentState::Investigating);
}

#[test]
fn waiting_approval_granted_moves_to_executing() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let created = engine
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
    engine
        .request_approval(&created.incident.incident_id, epoch(10), None)
        .unwrap();

    let incident = engine
        .approval_granted(&created.incident.incident_id, epoch(20), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Executing);
}

#[test]
fn executing_action_completes_moves_to_verifying() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let created = engine
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
    engine
        .request_approval(&created.incident.incident_id, epoch(10), None)
        .unwrap();
    engine
        .approval_granted(&created.incident.incident_id, epoch(20), None)
        .unwrap();

    let plan = VerificationPlan::new("watch restart", Duration::from_secs(120)).unwrap();
    let incident = engine
        .action_completed(&created.incident.incident_id, plan, epoch(30), None)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Verifying);
}

#[test]
fn executing_action_fails_safely_returns_to_investigating() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let created = engine
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
    engine
        .request_approval(&created.incident.incident_id, epoch(10), None)
        .unwrap();
    engine
        .approval_granted(&created.incident.incident_id, epoch(20), None)
        .unwrap();

    let incident = engine
        .action_failed_safely(
            &created.incident.incident_id,
            epoch(30),
            Some("device rejected the action".to_string()),
        )
        .unwrap();
    assert_eq!(incident.state, IncidentState::Investigating);
}

#[test]
fn verifying_resolves_only_after_every_check_passes_for_the_full_observation_window() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let created = engine
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
    let plan = VerificationPlan::new("watch", Duration::from_secs(300)).unwrap();
    engine
        .no_device_change_required(&created.incident.incident_id, plan, epoch(10), None)
        .unwrap();

    let mid = engine
        .record_verification_check(&created.incident.incident_id, true, epoch(60), None)
        .unwrap();
    assert_eq!(
        mid.state,
        IncidentState::Verifying,
        "must not resolve before the observation window elapses"
    );

    let resolved = engine
        .record_verification_check(&created.incident.incident_id, true, epoch(10 + 300), None)
        .unwrap();
    assert_eq!(resolved.state, IncidentState::Resolved);
}

#[test]
fn verifying_fails_check_returns_to_investigating() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let created = engine
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
    let plan = VerificationPlan::new("watch", Duration::from_secs(300)).unwrap();
    engine
        .no_device_change_required(&created.incident.incident_id, plan, epoch(10), None)
        .unwrap();

    let incident = engine
        .record_verification_check(
            &created.incident.incident_id,
            false,
            epoch(20),
            Some("regressed".to_string()),
        )
        .unwrap();
    assert_eq!(incident.state, IncidentState::Investigating);
}

#[test]
fn assignment_and_summaries_do_not_change_state() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let created = engine
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

    let assignee = Assignee::new(AssigneeKind::Agent, "agent-42").unwrap();
    let incident = engine
        .assign(&created.incident.incident_id, assignee, epoch(5))
        .unwrap();
    assert_eq!(incident.state, IncidentState::Investigating);
    assert_eq!(
        incident.assigned.as_ref().map(|a| a.identity.as_str()),
        Some("agent-42")
    );

    let incident = engine
        .set_diagnosis_summary(&created.incident.incident_id, "overheating fan", epoch(6))
        .unwrap();
    assert_eq!(
        incident.diagnosis_summary.as_deref(),
        Some("overheating fan")
    );

    let incident = engine
        .set_proposal_summary(&created.incident.incident_id, "replace fan", epoch(7))
        .unwrap();
    assert_eq!(incident.proposal_summary.as_deref(), Some("replace fan"));
}
