//! Shared black-box test helpers.

use std::time::{Duration, SystemTime};

use loki_connectors_sdk::ids::DeviceId;
use loki_incidents::engine::IncidentEngine;
use loki_incidents::store::IncidentStore;
use loki_monitoring::evaluate::{DataStatus, EvaluationOutcome};
use loki_monitoring::rule::{
    ComparisonOperator, Condition, MissingDataBehavior, MonitoringRule, RuleId, ScopeSelector,
    Severity,
};

pub fn epoch(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

pub fn device(id: &str) -> DeviceId {
    DeviceId::new(id).unwrap()
}

pub fn rule(rule_id: &str) -> MonitoringRule {
    MonitoringRule::new(
        RuleId::new(rule_id).unwrap(),
        1,
        "workspace-1",
        ScopeSelector::AllDevices,
        vec!["temperature_c".to_string()],
        Condition {
            signal: "temperature_c".to_string(),
            operator: ComparisonOperator::GreaterThan,
            threshold: 80.0,
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

pub fn fired_outcome(at: SystemTime) -> EvaluationOutcome {
    EvaluationOutcome::Fired {
        data_status: DataStatus::AllSignalsFresh,
        streak_started_at: at,
        fired_at: at,
    }
}

#[allow(dead_code)] // Shared integration-test helper; not every test binary consumes it.
pub fn missing_data_alert_outcome(at: SystemTime) -> EvaluationOutcome {
    EvaluationOutcome::MissingDataAlert {
        missing_signals: vec!["temperature_c".to_string()],
        at,
    }
}

/// A fresh in-memory `IncidentEngine` with generous, explicit windows
/// (never fabricated defaults — every `Duration` is passed by the caller,
/// matching `crates/loki-monitoring/src/freshness.rs`'s established
/// "no invented default" precedent).
pub fn engine_with_windows(
    correlation_window: Duration,
    reopen_policy_window: Duration,
    retention_close_window: Duration,
    work_intent_expiry_window: Duration,
) -> IncidentEngine {
    let store = IncidentStore::open_in_memory().unwrap();
    IncidentEngine::new(
        store,
        correlation_window,
        reopen_policy_window,
        retention_close_window,
        work_intent_expiry_window,
    )
}

#[allow(dead_code)] // Shared integration-test helper; not every test binary consumes it.
pub fn default_engine() -> IncidentEngine {
    engine_with_windows(
        Duration::from_secs(300),
        Duration::from_secs(86_400),
        Duration::from_secs(604_800),
        Duration::from_secs(3600),
    )
}

/// `IncidentStore::open` goes through `loki_storage::
/// open_or_migrate_sqlcipher`, which now requires the process-global
/// secure-memory arena to be ready first (DAT-002-MEM-CODE-001, ADR-0005).
/// Production never embeds an accepted bounds profile at this revision
/// (fail-closed by design), so this crate's own black-box tests bootstrap
/// one disposable, non-production test-only arena instead — see
/// `loki_storage::secure_memory::test_support` for why this bypass cannot
/// reach a release build.
#[allow(dead_code)] // Shared integration-test helper; not every test binary consumes it.
pub fn ensure_ready() {
    loki_storage::secure_memory::test_support::ensure_test_arena_ready();
}
