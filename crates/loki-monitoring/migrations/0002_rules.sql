-- MON-002: monitoring-rule definitions and their evaluation history.
--
-- Docs/SPEC.md §3.6 "Monitoring rule" for the rule columns. Applied by
-- crate::rule_store's own migration runner against the *same physical
-- database file* MON-001's crate::store::SqliteTelemetryStore opens (both
-- are separate rusqlite::Connection handles into one self-contained
-- loki-monitoring SQLite database — see crate::rule_store's module doc for
-- why this is a second connection rather than an edit to crate::store,
-- which is outside this node's writable scope).
--
-- Times are stored as INTEGER milliseconds since the Unix epoch, matching
-- 0001_telemetry.sql's convention in this same database.

-- One row per *version* of a rule — never updated in place. A rule's
-- version history is the durable record that Docs/graph/tasks/MON-002.md
-- Deliverable 5 requires: "changing a rule's definition does not
-- retroactively reinterpret already-evaluated history."
CREATE TABLE IF NOT EXISTS monitoring_rules (
    id                    INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    rule_id                  TEXT NOT NULL,
    version                     INTEGER NOT NULL,
    workspace                     TEXT NOT NULL,
    scope_kind                       TEXT NOT NULL CHECK (scope_kind IN ('all_devices', 'devices')),
    scope_device_ids_encoded            TEXT NOT NULL,
    required_signals_encoded               TEXT NOT NULL,
    condition_signal                          TEXT NOT NULL,
    condition_operator                           TEXT NOT NULL,
    condition_threshold                             REAL NOT NULL,
    window_ms                                          INTEGER NOT NULL,
    debounce_ms                                           INTEGER NOT NULL,
    cooldown_ms                                              INTEGER NOT NULL,
    severity                                                    TEXT NOT NULL,
    enabled                                                        INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    missing_data_behavior                                             TEXT NOT NULL,
    creator                                                              TEXT NOT NULL,
    created_at_ms                                                          INTEGER NOT NULL,
    updated_at_ms                                                            INTEGER NOT NULL,
    UNIQUE (rule_id, version)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_monitoring_rules_rule_id
    ON monitoring_rules (rule_id, version);

-- Every evaluation outcome, permanently attributed to the exact rule_id +
-- version that produced it (Deliverable 5). Append-only.
CREATE TABLE IF NOT EXISTS monitoring_rule_evaluations (
    id                    INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    rule_id                  TEXT NOT NULL,
    rule_version                 INTEGER NOT NULL,
    device_id                        TEXT NOT NULL,
    evaluated_at_ms                     INTEGER NOT NULL,
    outcome_kind                           TEXT NOT NULL CHECK (
        outcome_kind IN (
            'disabled', 'out_of_scope', 'missing_data_alert',
            'not_met', 'pending', 'fired', 'suppressed'
        )
    ),
    missing_signals_encoded                    TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_monitoring_rule_evaluations_rule
    ON monitoring_rule_evaluations (rule_id, rule_version, evaluated_at_ms);
