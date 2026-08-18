//! `Docs/graph/tasks/MON-002.md` Deliverable 5/7: "changing a rule's
//! definition does not retroactively reinterpret already-evaluated history;
//! an old evaluation record stays attributed to the rule version that
//! actually produced it." Black-box through `loki_monitoring`'s public
//! `RuleStore`/`RuleEvaluator` API.

use std::time::{Duration, SystemTime};

use loki_connectors_sdk::ids::DeviceId;
use loki_monitoring::evaluate::{EvaluationOutcome, RuleEvaluator};
use loki_monitoring::freshness::FreshnessTracker;
use loki_monitoring::rule::{
    ComparisonOperator, Condition, MissingDataBehavior, MonitoringRule, RuleId, ScopeSelector,
    Severity,
};
use loki_monitoring::rule_store::RuleStore;
use loki_monitoring::sample::{RetentionClass, SampleQuality, TelemetrySample};

fn epoch(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn device(id: &str) -> DeviceId {
    DeviceId::new(id).unwrap()
}

fn rule_with(version: u32, threshold: f64) -> MonitoringRule {
    MonitoringRule::new(
        RuleId::new("overheat").unwrap(),
        version,
        "workspace-1",
        ScopeSelector::AllDevices,
        vec!["temperature_c".to_string()],
        Condition {
            signal: "temperature_c".to_string(),
            operator: ComparisonOperator::GreaterThan,
            threshold,
        },
        Duration::from_secs(60),
        Duration::ZERO,
        Duration::ZERO,
        Severity::new("critical").unwrap(),
        true,
        MissingDataBehavior::TreatAsNotMet,
        "user-1",
        SystemTime::UNIX_EPOCH,
        SystemTime::UNIX_EPOCH,
    )
    .unwrap()
}

fn sample(t: SystemTime, value: f64) -> TelemetrySample {
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
fn an_old_evaluation_record_keeps_its_original_rule_version_after_the_rule_changes() {
    let store = RuleStore::open_in_memory().unwrap();
    let rule_id = RuleId::new("overheat").unwrap();

    // Version 1: threshold 80.0.
    let v1 = rule_with(1, 80.0);
    store.insert_new_version(&v1).unwrap();

    let t0 = epoch(100_000);
    let mut evaluator = RuleEvaluator::new();
    let samples_v1 = vec![sample(t0, 95.0)];
    let mut freshness_v1 = FreshnessTracker::new();
    freshness_v1.observe("dev-1::temperature_c", t0);
    let outcome_v1 = evaluator.evaluate(&v1, &device("dev-1"), &samples_v1, &freshness_v1, t0);
    assert!(outcome_v1.fired());
    store
        .record_evaluation(&rule_id, v1.version, &device("dev-1"), t0, &outcome_v1)
        .unwrap();

    // Rule changes: version 2, a much higher threshold that the same 95.0
    // sample would NOT have met.
    let v2 = rule_with(2, 200.0);
    store.insert_new_version(&v2).unwrap();

    let t1 = t0 + Duration::from_secs(10);
    let samples_v2 = vec![sample(t1, 95.0)];
    let mut freshness_v2 = FreshnessTracker::new();
    freshness_v2.observe("dev-1::temperature_c", t1);
    let outcome_v2 = evaluator.evaluate(&v2, &device("dev-1"), &samples_v2, &freshness_v2, t1);
    assert!(matches!(outcome_v2, EvaluationOutcome::NotMet { .. }));
    store
        .record_evaluation(&rule_id, v2.version, &device("dev-1"), t1, &outcome_v2)
        .unwrap();

    // The durable evaluation history must show the FIRST record still
    // attributed to version 1 — not retroactively reinterpreted as if
    // version 2's (much higher, unmet) threshold had always applied.
    let history = store.list_evaluations(&rule_id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].rule_version, 1);
    assert_eq!(history[0].outcome_kind, "fired");
    assert_eq!(history[1].rule_version, 2);
    assert_eq!(history[1].outcome_kind, "not_met");

    // And the store's own definition of version 1 is unchanged/still
    // readable, independent of version 2 existing.
    let stored_v1 = store.get_version(&rule_id, 1).unwrap().unwrap();
    assert_eq!(stored_v1.condition.threshold, 80.0);
    let stored_v2 = store.get_version(&rule_id, 2).unwrap().unwrap();
    assert_eq!(stored_v2.condition.threshold, 200.0);
    let latest = store.latest_version(&rule_id).unwrap().unwrap();
    assert_eq!(latest.version, 2);
}

#[test]
fn inserting_a_non_increasing_version_is_rejected_and_history_is_untouched() {
    let store = RuleStore::open_in_memory().unwrap();
    let rule_id = RuleId::new("overheat").unwrap();

    store.insert_new_version(&rule_with(1, 80.0)).unwrap();
    store.insert_new_version(&rule_with(2, 90.0)).unwrap();

    // Attempting to insert version 2 again (or version 1) must fail rather
    // than silently overwrite an already-evaluated version's definition.
    assert!(store.insert_new_version(&rule_with(2, 999.0)).is_err());
    assert!(store.insert_new_version(&rule_with(1, 999.0)).is_err());

    let versions = store.list_versions(&rule_id).unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].condition.threshold, 80.0);
    assert_eq!(versions[1].condition.threshold, 90.0);
}
