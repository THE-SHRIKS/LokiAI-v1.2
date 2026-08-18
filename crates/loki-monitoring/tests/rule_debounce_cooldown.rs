//! `Docs/graph/tasks/MON-002.md` Deliverable 3/7: "debounce (must stay met
//! for a duration before first firing) and cooldown (suppressed for a
//! duration after firing) are both real, tested, distinct behaviors, not
//! conflated into one." Black-box through `loki_monitoring`'s public API —
//! `crate::evaluate`'s own unit tests already cover the state-machine
//! mechanics; this proves the same contract from outside the crate.

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

fn rule(debounce: Duration, cooldown: Duration) -> MonitoringRule {
    MonitoringRule::new(
        RuleId::new("overheat").unwrap(),
        1,
        "workspace-1",
        ScopeSelector::AllDevices,
        vec!["temperature_c".to_string()],
        Condition {
            signal: "temperature_c".to_string(),
            operator: ComparisonOperator::GreaterThan,
            threshold: 80.0,
        },
        Duration::from_secs(600),
        debounce,
        cooldown,
        Severity::new("critical").unwrap(),
        true,
        MissingDataBehavior::TreatAsNotMet,
        "user-1",
        SystemTime::UNIX_EPOCH,
        SystemTime::UNIX_EPOCH,
    )
    .unwrap()
}

fn evaluate_at(
    evaluator: &mut RuleEvaluator,
    rule: &MonitoringRule,
    t: SystemTime,
    value: f64,
) -> EvaluationOutcome {
    let samples = vec![sample(t, value)];
    let mut freshness = FreshnessTracker::new();
    freshness.observe("dev-1::temperature_c", t);
    evaluator.evaluate(rule, &device("dev-1"), &samples, &freshness, t)
}

#[test]
fn a_zero_debounce_rule_never_waits_but_a_nonzero_cooldown_still_suppresses_repeats() {
    // debounce = 0s: fires on the very first met sample.
    // cooldown = 5m: subsequent met samples within 5 minutes do not re-fire.
    let rule = rule(Duration::ZERO, Duration::from_secs(300));
    let mut evaluator = RuleEvaluator::new();
    let t0 = epoch(1_000_000);

    let first = evaluate_at(&mut evaluator, &rule, t0, 95.0);
    assert!(
        first.fired(),
        "zero debounce must fire immediately: {first:?}"
    );

    for offset_secs in [1, 60, 200, 299] {
        let t = t0 + Duration::from_secs(offset_secs);
        let outcome = evaluate_at(&mut evaluator, &rule, t, 95.0);
        assert!(
            matches!(outcome, EvaluationOutcome::Suppressed { .. }),
            "expected Suppressed at +{offset_secs}s within a 300s cooldown, got {outcome:?}"
        );
    }

    let after_cooldown = evaluate_at(&mut evaluator, &rule, t0 + Duration::from_secs(301), 95.0);
    assert!(
        after_cooldown.fired(),
        "cooldown elapsed and condition still met -> must fire again: {after_cooldown:?}"
    );
}

#[test]
fn a_nonzero_debounce_rule_never_fires_early_even_with_zero_cooldown() {
    // debounce = 2m: must not fire until the condition has held 2 minutes.
    // cooldown = 0s: once satisfied, every still-met sample fires again.
    let rule = rule(Duration::from_secs(120), Duration::ZERO);
    let mut evaluator = RuleEvaluator::new();
    let t0 = epoch(2_000_000);

    for offset_secs in [0, 30, 60, 119] {
        let t = t0 + Duration::from_secs(offset_secs);
        let outcome = evaluate_at(&mut evaluator, &rule, t, 95.0);
        assert!(
            matches!(outcome, EvaluationOutcome::Pending { .. }),
            "expected Pending at +{offset_secs}s before 120s debounce elapses, got {outcome:?}"
        );
    }

    let fires_at_120 = evaluate_at(&mut evaluator, &rule, t0 + Duration::from_secs(120), 95.0);
    assert!(
        fires_at_120.fired(),
        "expected Fired exactly at debounce boundary: {fires_at_120:?}"
    );

    // cooldown = 0s: the very next still-met sample fires again too.
    let fires_again = evaluate_at(&mut evaluator, &rule, t0 + Duration::from_secs(121), 95.0);
    assert!(
        fires_again.fired(),
        "zero cooldown must not suppress a subsequent still-met sample: {fires_again:?}"
    );
}

#[test]
fn debounce_and_cooldown_both_present_compose_without_being_conflated() {
    let rule = rule(Duration::from_secs(30), Duration::from_secs(60));
    let mut evaluator = RuleEvaluator::new();
    let t0 = epoch(3_000_000);

    // Streak starts at t0 (first call establishes the streak start).
    let started = evaluate_at(&mut evaluator, &rule, t0, 95.0);
    assert!(matches!(started, EvaluationOutcome::Pending { .. }));

    // Before debounce elapses: still Pending, not Suppressed (cooldown has
    // never applied yet — there has been no fire at all).
    let pending = evaluate_at(&mut evaluator, &rule, t0 + Duration::from_secs(10), 95.0);
    assert!(matches!(pending, EvaluationOutcome::Pending { .. }));

    // Debounce elapses (30s since the streak started at t0): first fire.
    let fired = evaluate_at(&mut evaluator, &rule, t0 + Duration::from_secs(30), 95.0);
    assert!(fired.fired());

    // Within cooldown after the fire: Suppressed, not Pending (debounce
    // already satisfied for this streak; cooldown is the only remaining
    // gate).
    let suppressed = evaluate_at(&mut evaluator, &rule, t0 + Duration::from_secs(40), 95.0);
    assert!(matches!(suppressed, EvaluationOutcome::Suppressed { .. }));
}

#[test]
fn a_condition_that_briefly_drops_resets_debounce_progress() {
    let rule = rule(Duration::from_secs(30), Duration::ZERO);
    let mut evaluator = RuleEvaluator::new();
    let t0 = epoch(4_000_000);

    let pending = evaluate_at(&mut evaluator, &rule, t0 + Duration::from_secs(20), 95.0);
    assert!(matches!(pending, EvaluationOutcome::Pending { .. }));

    // Condition drops below threshold before debounce elapses.
    let not_met = evaluate_at(&mut evaluator, &rule, t0 + Duration::from_secs(25), 10.0);
    assert!(matches!(not_met, EvaluationOutcome::NotMet { .. }));

    // Condition met again: even though 30s+ have passed since the
    // *original* streak start, this is a new streak and must wait out
    // debounce again.
    let fresh_pending = evaluate_at(&mut evaluator, &rule, t0 + Duration::from_secs(40), 95.0);
    assert!(
        matches!(fresh_pending, EvaluationOutcome::Pending { .. }),
        "expected a fresh Pending after the interruption, got {fresh_pending:?}"
    );
}
