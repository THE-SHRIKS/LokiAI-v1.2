//! `Docs/graph/tasks/MON-002.md` Deliverable 4/7: a rule whose required
//! signal goes stale/absent behaves per its own declared
//! `MissingDataBehavior` — all three declared behaviors are exercised here,
//! not just one, and each produces an observably distinct outcome. Black-box
//! through `loki_monitoring`'s public API.

use std::time::{Duration, SystemTime};

use loki_connectors_sdk::ids::DeviceId;
use loki_monitoring::evaluate::{DataStatus, EvaluationOutcome, RuleEvaluator};
use loki_monitoring::freshness::FreshnessTracker;
use loki_monitoring::rule::{
    ComparisonOperator, Condition, MissingDataBehavior, MonitoringRule, RuleId, ScopeSelector,
    Severity,
};

fn epoch(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn device(id: &str) -> DeviceId {
    DeviceId::new(id).unwrap()
}

fn rule(missing_data_behavior: MissingDataBehavior) -> MonitoringRule {
    MonitoringRule::new(
        RuleId::new("battery-check").unwrap(),
        1,
        "workspace-1",
        ScopeSelector::AllDevices,
        vec!["battery_percent".to_string()],
        Condition {
            signal: "battery_percent".to_string(),
            operator: ComparisonOperator::LessThan,
            threshold: 20.0,
        },
        Duration::from_secs(60),
        Duration::ZERO,
        Duration::ZERO,
        Severity::new("warning").unwrap(),
        true,
        missing_data_behavior,
        "user-1",
        SystemTime::UNIX_EPOCH,
        SystemTime::UNIX_EPOCH,
    )
    .unwrap()
}

#[test]
fn treat_as_met_fires_even_though_no_real_sample_ever_arrived() {
    let rule = rule(MissingDataBehavior::TreatAsMet);
    let t0 = epoch(50_000);
    // No samples, no freshness observation at all -> the required signal
    // has never been observed (Unknown, not merely Stale).
    let freshness = FreshnessTracker::new();
    let mut evaluator = RuleEvaluator::new();

    let outcome = evaluator.evaluate(&rule, &device("dev-1"), &[], &freshness, t0);
    match outcome {
        EvaluationOutcome::Fired { data_status, .. } => match data_status {
            DataStatus::MissingTreatedAsMet { missing_signals } => {
                assert_eq!(missing_signals, vec!["battery_percent".to_string()]);
            }
            other => panic!("expected MissingTreatedAsMet, got {other:?}"),
        },
        other => panic!("TreatAsMet + missing data must fire, got {other:?}"),
    }
}

#[test]
fn treat_as_not_met_never_fires_while_data_is_missing() {
    let rule = rule(MissingDataBehavior::TreatAsNotMet);
    let t0 = epoch(50_000);
    let freshness = FreshnessTracker::new();
    let mut evaluator = RuleEvaluator::new();

    let outcome = evaluator.evaluate(&rule, &device("dev-1"), &[], &freshness, t0);
    match outcome {
        EvaluationOutcome::NotMet { data_status } => match data_status {
            DataStatus::MissingTreatedAsNotMet { missing_signals } => {
                assert_eq!(missing_signals, vec!["battery_percent".to_string()]);
            }
            other => panic!("expected MissingTreatedAsNotMet, got {other:?}"),
        },
        other => panic!("TreatAsNotMet + missing data must never fire, got {other:?}"),
    }
}

#[test]
fn fire_missing_data_alert_produces_its_own_distinct_alert_not_a_normal_fire() {
    let rule = rule(MissingDataBehavior::FireMissingDataAlert);
    let t0 = epoch(50_000);
    let freshness = FreshnessTracker::new();
    let mut evaluator = RuleEvaluator::new();

    let outcome = evaluator.evaluate(&rule, &device("dev-1"), &[], &freshness, t0);
    assert!(
        !outcome.fired(),
        "a missing-data alert is not the same thing as EvaluationOutcome::Fired"
    );
    match outcome {
        EvaluationOutcome::MissingDataAlert {
            missing_signals, ..
        } => {
            assert_eq!(missing_signals, vec!["battery_percent".to_string()]);
        }
        other => panic!("expected a dedicated MissingDataAlert, not {other:?}"),
    }
}

#[test]
fn all_three_missing_data_behaviors_are_observably_distinct_for_the_identical_missing_state() {
    let t0 = epoch(60_000);
    let freshness = FreshnessTracker::new();

    let mut treat_as_met_eval = RuleEvaluator::new();
    let treat_as_met = treat_as_met_eval.evaluate(
        &rule(MissingDataBehavior::TreatAsMet),
        &device("dev-1"),
        &[],
        &freshness,
        t0,
    );

    let mut treat_as_not_met_eval = RuleEvaluator::new();
    let treat_as_not_met = treat_as_not_met_eval.evaluate(
        &rule(MissingDataBehavior::TreatAsNotMet),
        &device("dev-1"),
        &[],
        &freshness,
        t0,
    );

    let mut fire_alert_eval = RuleEvaluator::new();
    let fire_alert = fire_alert_eval.evaluate(
        &rule(MissingDataBehavior::FireMissingDataAlert),
        &device("dev-1"),
        &[],
        &freshness,
        t0,
    );

    assert!(treat_as_met.fired());
    assert!(!treat_as_not_met.fired());
    assert!(!fire_alert.fired());
    assert!(matches!(
        fire_alert,
        EvaluationOutcome::MissingDataAlert { .. }
    ));
    assert_ne!(treat_as_met, treat_as_not_met);
    assert_ne!(treat_as_not_met, fire_alert);
}

#[test]
fn once_the_signal_becomes_fresh_again_normal_condition_evaluation_resumes() {
    use loki_monitoring::sample::{RetentionClass, SampleQuality, TelemetrySample};

    let rule = rule(MissingDataBehavior::TreatAsMet);
    let t0 = epoch(70_000);

    // First: no data at all -> forced-met fire via TreatAsMet.
    let mut evaluator = RuleEvaluator::new();
    let freshness_missing = FreshnessTracker::new();
    let missing_outcome = evaluator.evaluate(&rule, &device("dev-1"), &[], &freshness_missing, t0);
    assert!(missing_outcome.fired());

    // Then: a real, fresh sample arrives that does NOT meet the condition
    // (battery_percent 50.0 is not < 20.0) -> must now honestly report
    // NotMet with AllSignalsFresh, not keep pretending data is missing.
    let t1 = t0 + Duration::from_secs(5);
    let mut freshness_fresh = FreshnessTracker::new();
    freshness_fresh.observe("dev-1::battery_percent", t1);
    let sample = TelemetrySample::new(
        device("dev-1"),
        "battery_percent",
        50.0,
        "%",
        t1,
        t1,
        SampleQuality::Good,
        RetentionClass::new("standard").unwrap(),
    )
    .unwrap();
    let fresh_outcome =
        evaluator.evaluate(&rule, &device("dev-1"), &[sample], &freshness_fresh, t1);
    match fresh_outcome {
        EvaluationOutcome::NotMet { data_status } => {
            assert_eq!(data_status, DataStatus::AllSignalsFresh);
        }
        other => panic!("expected NotMet with AllSignalsFresh once data is fresh, got {other:?}"),
    }
}
