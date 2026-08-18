//! `Docs/graph/tasks/MON-002.md` Deliverable 2/7: deterministic window/
//! condition evaluation — a fixed rule against a fixed set of already-
//! ingested samples produces a reproducible fire/no-fire outcome, proven
//! black-box through `loki_monitoring`'s public API (not the internal
//! `crate::evaluate` unit tests, which already cover the state-machine
//! mechanics directly). Uses an explicit fake clock throughout (`SystemTime`
//! values built from `UNIX_EPOCH + Duration`), never real wall-clock time —
//! the same discipline `Docs/graph/handoffs/MON-001-1.md` established for
//! this crate's freshness/restart tests.

use std::time::{Duration, SystemTime};

use loki_connectors_sdk::ids::DeviceId;
use loki_monitoring::evaluate::{EvaluationOutcome, RuleEvaluator};
use loki_monitoring::freshness::FreshnessTracker;
use loki_monitoring::rule::{
    ComparisonOperator, Condition, MissingDataBehavior, MonitoringRule, RuleId, ScopeSelector,
    Severity,
};
use loki_monitoring::sample::{RetentionClass, SampleQuality, TelemetrySample};

fn epoch(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn device(id: &str) -> DeviceId {
    DeviceId::new(id).unwrap()
}

fn sample(device_id: &str, metric: &str, value: f64, t: SystemTime) -> TelemetrySample {
    TelemetrySample::new(
        device(device_id),
        metric,
        value,
        "C",
        t,
        t,
        SampleQuality::Good,
        RetentionClass::new("standard").unwrap(),
    )
    .unwrap()
}

fn cpu_threshold_rule() -> MonitoringRule {
    MonitoringRule::new(
        RuleId::new("high-cpu").unwrap(),
        1,
        "workspace-1",
        ScopeSelector::AllDevices,
        vec!["cpu_percent".to_string()],
        Condition {
            signal: "cpu_percent".to_string(),
            operator: ComparisonOperator::GreaterThanOrEqual,
            threshold: 90.0,
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

#[test]
fn a_fixed_rule_and_fixed_samples_reproduce_the_exact_same_outcome_every_time() {
    let rule = cpu_threshold_rule();
    let t0 = epoch(10_000);
    let samples = vec![sample("dev-1", "cpu_percent", 95.0, t0)];
    let mut freshness = FreshnessTracker::new();
    freshness.observe("dev-1::cpu_percent", t0);

    let mut outcomes = Vec::new();
    for _ in 0..5 {
        let mut evaluator = RuleEvaluator::new();
        outcomes.push(evaluator.evaluate(&rule, &device("dev-1"), &samples, &freshness, t0));
    }

    let first = &outcomes[0];
    assert!(
        outcomes.iter().all(|o| o == first),
        "identical inputs must produce an identical outcome every time: {outcomes:?}"
    );
    assert!(first.fired(), "95.0 >= 90.0 threshold must fire: {first:?}");
}

#[test]
fn a_value_below_threshold_never_fires_regardless_of_how_many_times_evaluated() {
    let rule = cpu_threshold_rule();
    let t0 = epoch(10_000);
    let samples = vec![sample("dev-1", "cpu_percent", 42.0, t0)];
    let mut freshness = FreshnessTracker::new();
    freshness.observe("dev-1::cpu_percent", t0);

    let mut evaluator = RuleEvaluator::new();
    let outcome = evaluator.evaluate(&rule, &device("dev-1"), &samples, &freshness, t0);
    assert!(matches!(outcome, EvaluationOutcome::NotMet { .. }));
    assert!(!outcome.fired());
}

#[test]
fn only_the_most_recent_in_window_sample_decides_the_outcome() {
    // Deliverable 2's "window" — an older sample outside the window must
    // not influence the outcome even if it would have met the condition.
    let rule = cpu_threshold_rule(); // window = 60s
    let t0 = epoch(10_000);
    let old_but_high = sample("dev-1", "cpu_percent", 99.0, t0 - Duration::from_secs(120));
    let recent_but_low = sample("dev-1", "cpu_percent", 10.0, t0);
    let samples = vec![old_but_high, recent_but_low];

    let mut freshness = FreshnessTracker::new();
    freshness.observe("dev-1::cpu_percent", t0);

    let mut evaluator = RuleEvaluator::new();
    let outcome = evaluator.evaluate(&rule, &device("dev-1"), &samples, &freshness, t0);
    assert!(
        matches!(outcome, EvaluationOutcome::NotMet { .. }),
        "the out-of-window high sample must not cause a fire: {outcome:?}"
    );
}

#[test]
fn a_sample_from_a_different_device_never_satisfies_another_devices_rule_scope() {
    let rule = MonitoringRule::new(
        RuleId::new("high-cpu").unwrap(),
        1,
        "workspace-1",
        ScopeSelector::Devices(vec![device("dev-1")]),
        vec!["cpu_percent".to_string()],
        Condition {
            signal: "cpu_percent".to_string(),
            operator: ComparisonOperator::GreaterThanOrEqual,
            threshold: 90.0,
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
    .unwrap();
    let t0 = epoch(10_000);
    let samples = vec![sample("dev-2", "cpu_percent", 99.0, t0)];
    let mut freshness = FreshnessTracker::new();
    freshness.observe("dev-2::cpu_percent", t0);

    let mut evaluator = RuleEvaluator::new();
    let outcome = evaluator.evaluate(&rule, &device("dev-2"), &samples, &freshness, t0);
    assert_eq!(outcome, EvaluationOutcome::OutOfScope);
}

#[test]
fn a_disabled_rule_never_fires_even_with_a_qualifying_sample() {
    let mut rule = cpu_threshold_rule();
    rule.enabled = false;
    let t0 = epoch(10_000);
    let samples = vec![sample("dev-1", "cpu_percent", 99.0, t0)];
    let mut freshness = FreshnessTracker::new();
    freshness.observe("dev-1::cpu_percent", t0);

    let mut evaluator = RuleEvaluator::new();
    let outcome = evaluator.evaluate(&rule, &device("dev-1"), &samples, &freshness, t0);
    assert_eq!(outcome, EvaluationOutcome::Disabled);
}
