//! `Docs/graph/tasks/INC-001.md` Deliverable 9/10: a `Fired`/
//! `MissingDataAlert` transition into `Open`/`Investigating` emits a
//! work-intent record — `Docs/SPEC.md` §3.13's exact field list, emission
//! only (this test asserts the record's shape, never any delivery, since
//! this crate performs no delivery — see `crate::work_intent`'s module
//! doc).

mod support;

use loki_connectors_sdk::ids::CapabilityId;
use loki_incidents::work_intent::DeliveryRequirements;
use support::*;

fn requirements() -> DeliveryRequirements {
    DeliveryRequirements::new("investigate the temperature excursion").unwrap()
}

fn capability() -> CapabilityId {
    CapabilityId::new("read_health").unwrap()
}

#[test]
fn a_new_incident_emits_a_work_intent_with_the_full_docs_spec_field_list() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let dev = device("dev-1");

    let result = engine
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

    let intent = result
        .work_intent
        .expect("must emit on new-incident creation");

    // work_intent_id
    assert!(!intent.work_intent_id.as_str().is_empty());
    // incident
    assert_eq!(intent.incident_id, result.incident.incident_id);
    // requested capability
    assert_eq!(intent.requested_capability, capability());
    // bounded evidence references
    assert!(!intent.evidence_refs.is_empty());
    assert!(
        intent.evidence_refs.len() <= loki_incidents::work_intent::MAX_WORK_INTENT_EVIDENCE_REFS
    );
    // provider-neutral delivery requirements
    assert_eq!(
        intent.delivery_requirements.description,
        "investigate the temperature excursion"
    );
    // expiry
    assert!(intent.expiry > epoch(0));
    // delivery attempts / acknowledged agent run / terminal outcome: emission-only.
    assert_eq!(intent.delivery_attempts, 0);
    assert_eq!(intent.acknowledged_agent_run, None);
    assert_eq!(intent.terminal_outcome, None);
    // Neither an approval nor an operation credential: no field on this
    // type represents either concept - proven structurally by this being
    // the type's entire field list (see crate::work_intent's doc).

    // Durably persisted, not just returned transiently.
    let persisted = engine
        .store()
        .list_work_intents_for_incident(&result.incident.incident_id)
        .unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0], intent);
}

#[test]
fn correlating_onto_an_already_investigating_incident_does_not_emit_a_second_intent() {
    let engine = default_engine();
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
    assert!(first.work_intent.is_some());

    let second = engine
        .ingest_fired_outcome(
            &rule,
            &dev,
            &fired_outcome(epoch(10)),
            &[],
            capability(),
            requirements(),
            epoch(10),
        )
        .unwrap();
    assert!(second.correlated);
    assert!(second.work_intent.is_none());

    let persisted = engine
        .store()
        .list_work_intents_for_incident(&first.incident.incident_id)
        .unwrap();
    assert_eq!(
        persisted.len(),
        1,
        "exactly one work intent for the whole correlated episode so far"
    );
}

#[test]
fn a_non_firing_outcome_is_rejected_and_never_creates_an_incident_or_a_work_intent() {
    let engine = default_engine();
    let rule = rule("rule-1");
    let dev = device("dev-1");

    let not_met = loki_monitoring::evaluate::EvaluationOutcome::NotMet {
        data_status: loki_monitoring::evaluate::DataStatus::AllSignalsFresh,
    };

    let err = engine.ingest_fired_outcome(
        &rule,
        &dev,
        &not_met,
        &[],
        capability(),
        requirements(),
        epoch(0),
    );
    assert!(err.is_err());
    assert!(
        engine
            .store()
            .latest_incident_for(&rule.rule_id, &dev)
            .unwrap()
            .is_none()
    );
}
