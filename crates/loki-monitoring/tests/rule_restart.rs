//! `Docs/graph/tasks/MON-002.md` Deliverable 6/7: "enabled rules and their
//! debounce/cooldown state behave sanely across a process restart —
//! durable data survives via the store; in-memory working state honestly
//! resets rather than being fabricated." Simulates a process restart the
//! same way `crates/loki-monitoring/tests/restart.rs` does for `MON-001`:
//! drop every in-process value and construct brand new ones against the
//! same on-disk SQLite file (there is no way to literally restart the OS
//! process from within a test).

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

fn rule() -> MonitoringRule {
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
        Duration::from_secs(30),
        Duration::ZERO,
        Severity::new("critical").unwrap(),
        true,
        MissingDataBehavior::TreatAsNotMet,
        "user-1",
        epoch(5000),
        epoch(5000),
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
fn a_rule_definition_durably_survives_a_simulated_process_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("rules.sqlite3");
    let rule_id = RuleId::new("overheat").unwrap();

    {
        let store = RuleStore::open(&db_path).unwrap();
        store.insert_new_version(&rule()).unwrap();
    } // store (and its connection) dropped here — simulated process exit.

    let reopened = RuleStore::open(&db_path).unwrap();
    let read_back = reopened.latest_version(&rule_id).unwrap().unwrap();
    assert_eq!(read_back, rule());
}

#[test]
fn evaluation_history_durably_survives_a_simulated_process_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("rules.sqlite3");
    let rule_id = RuleId::new("overheat").unwrap();
    let t0 = epoch(100_000);

    {
        let store = RuleStore::open(&db_path).unwrap();
        store.insert_new_version(&rule()).unwrap();
        let outcome = EvaluationOutcome::Fired {
            data_status: loki_monitoring::evaluate::DataStatus::AllSignalsFresh,
            streak_started_at: t0,
            fired_at: t0,
        };
        store
            .record_evaluation(&rule_id, 1, &device("dev-1"), t0, &outcome)
            .unwrap();
    }

    let reopened = RuleStore::open(&db_path).unwrap();
    let history = reopened.list_evaluations(&rule_id).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].outcome_kind, "fired");
    assert_eq!(history[0].rule_version, 1);
}

#[test]
fn in_memory_debounce_state_honestly_resets_after_a_restart_it_is_not_fabricated() {
    let r = rule(); // debounce = 30s
    let t0 = epoch(200_000);

    let mut evaluator = RuleEvaluator::new();
    let samples = vec![sample(t0, 95.0)];
    let mut freshness = FreshnessTracker::new();
    freshness.observe("dev-1::temperature_c", t0);
    let before_restart = evaluator.evaluate(&r, &device("dev-1"), &samples, &freshness, t0);
    assert!(
        matches!(before_restart, EvaluationOutcome::Pending { .. }),
        "expected Pending before the debounce window elapses, got {before_restart:?}"
    );

    // Simulated restart: brand new RuleEvaluator, even though "real" time
    // has moved forward past the point the pre-restart streak would have
    // satisfied its 30s debounce.
    let mut restarted_evaluator = RuleEvaluator::new();
    let t1 = t0 + Duration::from_secs(35);
    let samples1 = vec![sample(t1, 95.0)];
    let mut freshness1 = FreshnessTracker::new();
    freshness1.observe("dev-1::temperature_c", t1);
    let after_restart =
        restarted_evaluator.evaluate(&r, &device("dev-1"), &samples1, &freshness1, t1);

    assert!(
        matches!(after_restart, EvaluationOutcome::Pending { .. }),
        "a restarted evaluator must not fabricate pre-restart debounce progress \
         (it would otherwise dishonestly fire immediately using a streak start \
         time this process never actually observed); got {after_restart:?}"
    );
    assert!(
        !after_restart.fired(),
        "must not fire on restart using fabricated pre-restart state"
    );
}

#[test]
fn a_disabled_rules_definition_still_persists_and_reloads_as_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("rules.sqlite3");
    let rule_id = RuleId::new("overheat").unwrap();

    let mut disabled_rule = rule();
    disabled_rule.enabled = false;

    {
        let store = RuleStore::open(&db_path).unwrap();
        store.insert_new_version(&disabled_rule).unwrap();
    }

    let reopened = RuleStore::open(&db_path).unwrap();
    let read_back = reopened.latest_version(&rule_id).unwrap().unwrap();
    assert!(
        !read_back.enabled,
        "enabled=false must survive a restart honestly"
    );
}
