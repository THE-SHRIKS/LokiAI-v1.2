//! Durable monitoring-rule definitions and evaluation-history storage —
//! `Docs/graph/tasks/MON-002.md` Deliverables 5–6 (rule versioning survives
//! a restart; an old evaluation record stays attributed to the rule version
//! that actually produced it).
//!
//! ## Why this is a second connection into the *same* database file, not an
//! edit to `crate::store`
//!
//! This node's task packet (`Docs/graph/tasks/MON-002.md` §2) grants write
//! access to `crates/loki-monitoring/src/**` **additions** (new modules) and
//! explicitly forbids rewriting `crate::store` (among five other `MON-001`
//! files): "do not rewrite MON-001's existing ... `store.rs` ...; extend
//! alongside them." The packet also says this node "may extend that same
//! self-contained database [`crate::store`'s], it does not need
//! `loki-storage`'s" — i.e. the rule/evaluation tables belong in the *same
//! physical SQLite file* `crate::store::SqliteTelemetryStore` opens, not a
//! third database.
//!
//! Reconciling both instructions without editing `store.rs`: SQLite
//! natively supports multiple independent connections to one file (this
//! crate already runs in `journal_mode=WAL`, which is specifically designed
//! for concurrent readers/one writer across connections). `RuleStore` is
//! therefore its own `rusqlite::Connection`, opened at the *same path* a
//! caller also passes to `SqliteTelemetryStore::open` — two connections,
//! one file, no edit to `crate::store`'s file required. Its own migration
//! runner tracks applied migrations in a table named distinctly
//! (`mon_rules_schema_migrations`) from `crate::store`'s
//! (`mon_schema_migrations`) so the two migration histories cannot collide
//! even though both currently start counting from `1`.
//!
//! This mirrors the same reasoning `Docs/graph/handoffs/MON-001-1.md` §5/§7
//! already recorded for why `crate::store` itself is a self-contained store
//! rather than a `crates/loki-storage` repository — this module is one more
//! instance of the identical "the literal instruction and the writable-scope
//! boundary are in tension; resolve by keeping the smallest in-scope
//! solution and flagging it" pattern, not a new architectural decision.
//!
//! ## What survives a restart and what does not
//!
//! Every [`MonitoringRule`] version and every recorded [`EvaluationRecord`]
//! is durable — a real SQLite table, read back unchanged across a process
//! restart (see `tests/rule_restart.rs`). `crate::evaluate::RuleEvaluator`'s
//! in-memory debounce/cooldown streak state does **not** live here and does
//! not survive a restart — see `crate::evaluate`'s module doc for why that
//! is deliberate, not an oversight.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, Row, params};

use crate::error::MonitoringError;
use crate::evaluate::EvaluationOutcome;
use crate::rule::{
    ComparisonOperator, Condition, MissingDataBehavior, MonitoringRule, RuleId, ScopeSelector,
    Severity,
};
use crate::validate::{decode_string_list, encode_string_list};
use loki_connectors_sdk::ids::DeviceId;

const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);

/// One durably recorded evaluation outcome, permanently attributed to the
/// exact `rule_id` + `rule_version` that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationRecord {
    pub rule_id: RuleId,
    pub rule_version: u32,
    pub device_id: DeviceId,
    pub evaluated_at: SystemTime,
    /// See `crate::evaluate::EvaluationOutcome::kind_wire_str`.
    pub outcome_kind: String,
    pub missing_signals: Vec<String>,
}

/// A durable store for monitoring-rule definitions and their evaluation
/// history. See this module's doc for why it owns its own connection.
pub struct RuleStore {
    conn: Mutex<Connection>,
    _key: Option<loki_storage::DatabaseKey>,
}

impl RuleStore {
    /// Opens (creating if absent) the SQLite database at `path` and applies
    /// this module's own migrations. Pass the same `path` used for
    /// `crate::store::SqliteTelemetryStore::open` to share one physical
    /// database file, or a distinct path to keep them fully separate — both
    /// are valid; this module does not require the former.
    pub fn open(path: &Path) -> Result<Self, MonitoringError> {
        let key_path = loki_storage::secret::database_key_path(path);
        let (conn, key) = loki_storage::open_or_migrate_sqlcipher(path, &key_path)?;
        configure(&conn)?;
        migrations::apply_all(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            _key: Some(key),
        })
    }

    /// Opens a private in-memory database with migrations applied. No
    /// restart durability — used for tests that do not need a real file.
    pub fn open_in_memory() -> Result<Self, MonitoringError> {
        let conn = Connection::open_in_memory()?;
        configure(&conn)?;
        migrations::apply_all(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            _key: None,
        })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Persists `rule` as a new immutable version row.
    ///
    /// Rejects (`MonitoringError::Validation`) a `rule.version` that is not
    /// strictly greater than every version already stored for `rule.rule_id`
    /// — a rule's version history is append-only and monotonic; this store
    /// never overwrites or reinterprets a prior version
    /// (`Docs/graph/tasks/MON-002.md` Deliverable 5).
    pub fn insert_new_version(&self, rule: &MonitoringRule) -> Result<(), MonitoringError> {
        let conn = self.lock();

        let existing_max: Option<i64> = conn.query_row(
            "SELECT MAX(version) FROM monitoring_rules WHERE rule_id = ?1",
            params![rule.rule_id.as_str()],
            |r| r.get(0),
        )?;
        if let Some(existing_max) = existing_max
            && (rule.version as i64) <= existing_max
        {
            return Err(MonitoringError::Validation(
                crate::error::ValidationError::new(
                    "version",
                    format!(
                        "must be greater than the existing latest version {existing_max} for rule_id {:?} (was {})",
                        rule.rule_id.as_str(),
                        rule.version
                    ),
                ),
            ));
        }

        let (scope_kind, scope_device_ids) = encode_scope(&rule.scope);
        conn.execute(
            "INSERT INTO monitoring_rules
                (rule_id, version, workspace, scope_kind, scope_device_ids_encoded,
                 required_signals_encoded, condition_signal, condition_operator,
                 condition_threshold, window_ms, debounce_ms, cooldown_ms, severity,
                 enabled, missing_data_behavior, creator, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                rule.rule_id.as_str(),
                rule.version,
                rule.workspace,
                scope_kind,
                scope_device_ids,
                encode_string_list(&rule.required_signals),
                rule.condition.signal,
                rule.condition.operator.as_wire_str(),
                rule.condition.threshold,
                duration_to_millis(rule.window),
                duration_to_millis(rule.debounce),
                duration_to_millis(rule.cooldown),
                rule.severity.as_str(),
                rule.enabled as i64,
                rule.missing_data_behavior.as_wire_str(),
                rule.creator,
                to_millis(rule.created_at),
                to_millis(rule.updated_at),
            ],
        )?;
        Ok(())
    }

    /// The latest (highest-version) durably stored definition for `rule_id`,
    /// or `None` if no version has ever been stored.
    pub fn latest_version(
        &self,
        rule_id: &RuleId,
    ) -> Result<Option<MonitoringRule>, MonitoringError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT rule_id, version, workspace, scope_kind, scope_device_ids_encoded,
                    required_signals_encoded, condition_signal, condition_operator,
                    condition_threshold, window_ms, debounce_ms, cooldown_ms, severity,
                    enabled, missing_data_behavior, creator, created_at_ms, updated_at_ms
             FROM monitoring_rules WHERE rule_id = ?1 ORDER BY version DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![rule_id.as_str()], row_to_raw_rule)?;
        match rows.next() {
            Some(raw) => Ok(Some(raw_to_rule(raw?)?)),
            None => Ok(None),
        }
    }

    /// The exact stored definition for `rule_id` at `version`, or `None` if
    /// that specific version was never stored (either the rule does not
    /// exist, or `version` was skipped/never inserted).
    pub fn get_version(
        &self,
        rule_id: &RuleId,
        version: u32,
    ) -> Result<Option<MonitoringRule>, MonitoringError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT rule_id, version, workspace, scope_kind, scope_device_ids_encoded,
                    required_signals_encoded, condition_signal, condition_operator,
                    condition_threshold, window_ms, debounce_ms, cooldown_ms, severity,
                    enabled, missing_data_behavior, creator, created_at_ms, updated_at_ms
             FROM monitoring_rules WHERE rule_id = ?1 AND version = ?2",
        )?;
        let mut rows = stmt.query_map(params![rule_id.as_str(), version], row_to_raw_rule)?;
        match rows.next() {
            Some(raw) => Ok(Some(raw_to_rule(raw?)?)),
            None => Ok(None),
        }
    }

    /// Every stored version of `rule_id`, oldest first.
    pub fn list_versions(&self, rule_id: &RuleId) -> Result<Vec<MonitoringRule>, MonitoringError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT rule_id, version, workspace, scope_kind, scope_device_ids_encoded,
                    required_signals_encoded, condition_signal, condition_operator,
                    condition_threshold, window_ms, debounce_ms, cooldown_ms, severity,
                    enabled, missing_data_behavior, creator, created_at_ms, updated_at_ms
             FROM monitoring_rules WHERE rule_id = ?1 ORDER BY version ASC",
        )?;
        let raw_rows = stmt.query_map(params![rule_id.as_str()], row_to_raw_rule)?;
        let mut out = Vec::new();
        for raw in raw_rows {
            out.push(raw_to_rule(raw?)?);
        }
        Ok(out)
    }

    /// Durably records one evaluation outcome, permanently attributed to
    /// `rule_id`/`rule_version` — the exact version that actually produced
    /// it (`Docs/graph/tasks/MON-002.md` Deliverable 5), independent of
    /// whatever the *current* latest version of that rule is by the time
    /// this is read back.
    pub fn record_evaluation(
        &self,
        rule_id: &RuleId,
        rule_version: u32,
        device_id: &DeviceId,
        evaluated_at: SystemTime,
        outcome: &EvaluationOutcome,
    ) -> Result<(), MonitoringError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO monitoring_rule_evaluations
                (rule_id, rule_version, device_id, evaluated_at_ms, outcome_kind, missing_signals_encoded)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                rule_id.as_str(),
                rule_version,
                device_id.as_str(),
                to_millis(evaluated_at),
                outcome.kind_wire_str(),
                encode_string_list(&outcome.missing_signals()),
            ],
        )?;
        Ok(())
    }

    /// Every durably recorded evaluation for `rule_id`, across every
    /// version, oldest first — each row keeps the `rule_version` that was
    /// actually active when it was recorded, even after newer versions have
    /// since been inserted.
    pub fn list_evaluations(
        &self,
        rule_id: &RuleId,
    ) -> Result<Vec<EvaluationRecord>, MonitoringError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT rule_id, rule_version, device_id, evaluated_at_ms, outcome_kind, missing_signals_encoded
             FROM monitoring_rule_evaluations WHERE rule_id = ?1 ORDER BY id ASC",
        )?;
        let raw_rows = stmt.query_map(params![rule_id.as_str()], row_to_raw_evaluation)?;
        let mut out = Vec::new();
        for raw in raw_rows {
            out.push(raw_to_evaluation_record(raw?)?);
        }
        Ok(out)
    }
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(())
}

fn to_millis(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(_) => 0,
    }
}

fn from_millis(ms: i64) -> SystemTime {
    if ms >= 0 {
        UNIX_EPOCH + Duration::from_millis(ms as u64)
    } else {
        UNIX_EPOCH - Duration::from_millis((-ms) as u64)
    }
}

fn duration_to_millis(d: Duration) -> i64 {
    d.as_millis() as i64
}

fn millis_to_duration(ms: i64) -> Duration {
    Duration::from_millis(ms.max(0) as u64)
}

fn encode_scope(scope: &ScopeSelector) -> (&'static str, String) {
    match scope {
        ScopeSelector::AllDevices => ("all_devices", String::new()),
        ScopeSelector::Devices(ids) => (
            "devices",
            encode_string_list(
                &ids.iter()
                    .map(|d| d.as_str().to_string())
                    .collect::<Vec<_>>(),
            ),
        ),
    }
}

fn decode_scope(kind: &str, encoded: &str) -> Result<ScopeSelector, MonitoringError> {
    match kind {
        "all_devices" => Ok(ScopeSelector::AllDevices),
        "devices" => {
            let ids = decode_string_list(encoded)
                .into_iter()
                .map(DeviceId::new)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ScopeSelector::Devices(ids))
        }
        other => Err(MonitoringError::Storage(format!(
            "unknown stored scope_kind {other:?}"
        ))),
    }
}

struct RawRuleRow {
    rule_id: String,
    version: i64,
    workspace: String,
    scope_kind: String,
    scope_device_ids_encoded: String,
    required_signals_encoded: String,
    condition_signal: String,
    condition_operator: String,
    condition_threshold: f64,
    window_ms: i64,
    debounce_ms: i64,
    cooldown_ms: i64,
    severity: String,
    enabled: i64,
    missing_data_behavior: String,
    creator: String,
    created_at_ms: i64,
    updated_at_ms: i64,
}

fn row_to_raw_rule(row: &Row<'_>) -> rusqlite::Result<RawRuleRow> {
    Ok(RawRuleRow {
        rule_id: row.get(0)?,
        version: row.get(1)?,
        workspace: row.get(2)?,
        scope_kind: row.get(3)?,
        scope_device_ids_encoded: row.get(4)?,
        required_signals_encoded: row.get(5)?,
        condition_signal: row.get(6)?,
        condition_operator: row.get(7)?,
        condition_threshold: row.get(8)?,
        window_ms: row.get(9)?,
        debounce_ms: row.get(10)?,
        cooldown_ms: row.get(11)?,
        severity: row.get(12)?,
        enabled: row.get(13)?,
        missing_data_behavior: row.get(14)?,
        creator: row.get(15)?,
        created_at_ms: row.get(16)?,
        updated_at_ms: row.get(17)?,
    })
}

fn raw_to_rule(raw: RawRuleRow) -> Result<MonitoringRule, MonitoringError> {
    let scope = decode_scope(&raw.scope_kind, &raw.scope_device_ids_encoded)?;
    let operator: ComparisonOperator = raw.condition_operator.parse()?;
    let missing_data_behavior: MissingDataBehavior = raw.missing_data_behavior.parse()?;

    Ok(MonitoringRule::new(
        RuleId::new(raw.rule_id)?,
        raw.version as u32,
        raw.workspace,
        scope,
        decode_string_list(&raw.required_signals_encoded),
        Condition {
            signal: raw.condition_signal,
            operator,
            threshold: raw.condition_threshold,
        },
        millis_to_duration(raw.window_ms),
        millis_to_duration(raw.debounce_ms),
        millis_to_duration(raw.cooldown_ms),
        Severity::new(raw.severity)?,
        raw.enabled != 0,
        missing_data_behavior,
        raw.creator,
        from_millis(raw.created_at_ms),
        from_millis(raw.updated_at_ms),
    )?)
}

struct RawEvaluationRow {
    rule_id: String,
    rule_version: i64,
    device_id: String,
    evaluated_at_ms: i64,
    outcome_kind: String,
    missing_signals_encoded: String,
}

fn row_to_raw_evaluation(row: &Row<'_>) -> rusqlite::Result<RawEvaluationRow> {
    Ok(RawEvaluationRow {
        rule_id: row.get(0)?,
        rule_version: row.get(1)?,
        device_id: row.get(2)?,
        evaluated_at_ms: row.get(3)?,
        outcome_kind: row.get(4)?,
        missing_signals_encoded: row.get(5)?,
    })
}

fn raw_to_evaluation_record(raw: RawEvaluationRow) -> Result<EvaluationRecord, MonitoringError> {
    Ok(EvaluationRecord {
        rule_id: RuleId::new(raw.rule_id)?,
        rule_version: raw.rule_version as u32,
        device_id: DeviceId::new(raw.device_id)?,
        evaluated_at: from_millis(raw.evaluated_at_ms),
        outcome_kind: raw.outcome_kind,
        missing_signals: decode_string_list(&raw.missing_signals_encoded),
    })
}

mod migrations {
    use rusqlite::Connection;

    use crate::error::MonitoringError;

    const RULES_MIGRATION_SQL: &str = include_str!("../migrations/0002_rules.sql");

    pub(super) fn apply_all(conn: &Connection) -> Result<(), MonitoringError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS mon_rules_schema_migrations (
                version    INTEGER NOT NULL PRIMARY KEY,
                name        TEXT NOT NULL
            ) STRICT;",
        )?;

        let already: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mon_rules_schema_migrations WHERE version = 1",
            [],
            |r| r.get(0),
        )?;
        if already > 0 {
            return Ok(());
        }

        conn.execute_batch(RULES_MIGRATION_SQL)?;
        conn.execute(
            "INSERT INTO mon_rules_schema_migrations (version, name) VALUES (1, 'rules')",
            [],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::ComparisonOperator;
    use std::time::Duration;

    fn device(id: &str) -> DeviceId {
        DeviceId::new(id).unwrap()
    }

    fn epoch(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn rule(version: u32) -> MonitoringRule {
        MonitoringRule::new(
            RuleId::new("rule-1").unwrap(),
            version,
            "workspace-1",
            ScopeSelector::Devices(vec![device("dev-1")]),
            vec!["temperature_c".to_string()],
            Condition {
                signal: "temperature_c".to_string(),
                operator: ComparisonOperator::GreaterThan,
                threshold: 80.0,
            },
            Duration::from_secs(60),
            Duration::from_secs(30),
            Duration::from_secs(300),
            Severity::new("critical").unwrap(),
            true,
            MissingDataBehavior::TreatAsNotMet,
            "user-1",
            epoch(1000),
            epoch(1000),
        )
        .unwrap()
    }

    #[test]
    fn open_in_memory_applies_migrations_and_starts_empty() {
        let store = RuleStore::open_in_memory().unwrap();
        assert!(
            store
                .latest_version(&RuleId::new("rule-1").unwrap())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn insert_and_read_back_a_rule_round_trips() {
        let store = RuleStore::open_in_memory().unwrap();
        let r = rule(1);
        store.insert_new_version(&r).unwrap();

        let read = store
            .latest_version(&RuleId::new("rule-1").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(read, r);
    }

    #[test]
    fn a_non_increasing_version_is_rejected() {
        let store = RuleStore::open_in_memory().unwrap();
        store.insert_new_version(&rule(2)).unwrap();
        let err = store.insert_new_version(&rule(2));
        assert!(err.is_err(), "duplicate version must be rejected");
        let err = store.insert_new_version(&rule(1));
        assert!(err.is_err(), "lower version must be rejected");
    }

    #[test]
    fn list_versions_returns_every_version_oldest_first() {
        let store = RuleStore::open_in_memory().unwrap();
        store.insert_new_version(&rule(1)).unwrap();
        store.insert_new_version(&rule(2)).unwrap();
        store.insert_new_version(&rule(3)).unwrap();

        let versions = store
            .list_versions(&RuleId::new("rule-1").unwrap())
            .unwrap();
        assert_eq!(
            versions.iter().map(|r| r.version).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn migrations_are_idempotent_across_repeated_opens_on_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rules.sqlite3");

        {
            let store = RuleStore::open(&path).unwrap();
            store.insert_new_version(&rule(1)).unwrap();
        }
        let store = RuleStore::open(&path).unwrap();
        assert!(
            store
                .latest_version(&RuleId::new("rule-1").unwrap())
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn evaluation_records_round_trip() {
        let store = RuleStore::open_in_memory().unwrap();
        store.insert_new_version(&rule(1)).unwrap();

        let outcome = EvaluationOutcome::Fired {
            data_status: crate::evaluate::DataStatus::AllSignalsFresh,
            streak_started_at: epoch(1000),
            fired_at: epoch(1030),
        };
        store
            .record_evaluation(
                &RuleId::new("rule-1").unwrap(),
                1,
                &device("dev-1"),
                epoch(1030),
                &outcome,
            )
            .unwrap();

        let records = store
            .list_evaluations(&RuleId::new("rule-1").unwrap())
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].rule_version, 1);
        assert_eq!(records[0].outcome_kind, "fired");
        assert_eq!(records[0].device_id.as_str(), "dev-1");
    }

    #[test]
    fn shares_one_physical_database_file_with_a_second_connection() {
        // Proves the "same self-contained database, second connection"
        // design this module's doc describes actually works.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared.sqlite3");

        let telemetry_store = crate::store::SqliteTelemetryStore::open(&path).unwrap();
        let rule_store = RuleStore::open(&path).unwrap();

        rule_store.insert_new_version(&rule(1)).unwrap();
        assert_eq!(telemetry_store.sample_count().unwrap(), 0);
        assert!(
            rule_store
                .latest_version(&RuleId::new("rule-1").unwrap())
                .unwrap()
                .is_some()
        );
    }
}
