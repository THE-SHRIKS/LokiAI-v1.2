-- INC-001: incident, timeline, evidence-snapshot, and work-intent durable
-- storage — this crate's own self-contained SQLite database, applied by
-- crate::store's own idempotent migration runner. Follows the exact
-- precedent Docs/graph/handoffs/{MON-001,MON-002}-1.md §5/§7 already
-- recorded: a new bounded-context crate that needs its own durable tables
-- but cannot edit crates/loki-storage/src/** opens its own database with its
-- own migration, rather than a workaround or silently skipped persistence.
--
-- Times are stored as INTEGER milliseconds since the Unix epoch, matching
-- crates/loki-monitoring's own migration convention in this same workspace.

-- One row per incident, updated in place as its mutable summary state
-- changes (current state, occurrence count, assignment, diagnosis/proposal,
-- verification plan/state, bounded evidence-reference list). This is
-- deliberately NOT append-only — an incident's *current* state is a single
-- value, not a history; the append-only audit trail lives in
-- incident_timeline below. `Docs/graph/tasks/INC-001.md` Deliverable 6 only
-- requires the *timeline* to be append-only, not this summary row.
CREATE TABLE IF NOT EXISTS incidents (
    id                                INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    incident_id                          TEXT NOT NULL UNIQUE,
    title                                    TEXT NOT NULL,
    severity                                     TEXT NOT NULL,
    state                                            TEXT NOT NULL CHECK (
        state IN (
            'open', 'investigating', 'waiting_approval', 'executing',
            'verifying', 'resolved', 'closed'
        )
    ),
    affected_scope_encoded                               TEXT NOT NULL,
    -- Denormalized single-device correlation key (this crate always
    -- constructs single-device incidents at its current ingest granularity —
    -- see crate::engine's module doc) so crate::store::IncidentStore::
    -- latest_incident_for can use an index rather than scanning and decoding
    -- affected_scope_encoded for every row.
    correlation_device_id                                    TEXT NOT NULL,
    trigger_rule_id                                              TEXT NOT NULL,
    trigger_rule_version                                             INTEGER NOT NULL,
    occurrence_count                                                     INTEGER NOT NULL,
    -- Bounded live reference list (crate::incident::MAX_EVIDENCE_REFS_PER_INCIDENT).
    evidence_refs_encoded                                                    TEXT NOT NULL,
    assigned_kind                                                                TEXT,
    assigned_identity                                                                TEXT,
    diagnosis_summary                                                                    TEXT,
    proposal_summary                                                                         TEXT,
    verification_plan_description                                                               TEXT,
    verification_plan_window_ms                                                                     INTEGER,
    verification_started_at_ms                                                                          INTEGER,
    -- Bounded current-cycle check history (crate::incident::MAX_VERIFICATION_CHECKS_RETAINED).
    verification_checks_encoded                                                                            TEXT NOT NULL,
    created_at_ms                                                                                              INTEGER NOT NULL,
    updated_at_ms                                                                                                  INTEGER NOT NULL,
    state_entered_at_ms                                                                                              INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_incidents_rule_device
    ON incidents (trigger_rule_id, correlation_device_id, id);
CREATE INDEX IF NOT EXISTS idx_incidents_state
    ON incidents (state);

-- Append-only audit trail (Deliverable 6). Every state transition and
-- assignment change is a new row here; no code path in this crate updates or
-- deletes an existing row of this table.
CREATE TABLE IF NOT EXISTS incident_timeline (
    id                    INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    incident_id               TEXT NOT NULL,
    at_ms                         INTEGER NOT NULL,
    event_kind                       TEXT NOT NULL CHECK (
        event_kind IN (
            'created', 'state_transition', 'correlated_occurrence',
            'assignment_changed', 'verification_check_recorded',
            'diagnosis_updated', 'proposal_updated'
        )
    ),
    from_state                           TEXT,
    to_state                                 TEXT,
    transition                                   TEXT,
    note                                             TEXT,
    occurrence_count                                     INTEGER,
    assignee_kind                                            TEXT,
    assignee_identity                                            TEXT,
    check_passed                                                     INTEGER
) STRICT;

CREATE INDEX IF NOT EXISTS idx_incident_timeline_incident
    ON incident_timeline (incident_id, id);

-- Bounded evidence snapshots (Deliverable 4). Never deleted by this crate —
-- an incident's *live* reference list (incidents.evidence_refs_encoded) is
-- bounded and drop-oldest, but the underlying snapshot rows remain as a
-- durable audit trail even once no longer referenced live.
CREATE TABLE IF NOT EXISTS incident_evidence_snapshots (
    id                    INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    evidence_id               TEXT NOT NULL UNIQUE,
    rule_id                       TEXT NOT NULL,
    rule_version                     INTEGER NOT NULL,
    device_id                            TEXT NOT NULL,
    outcome_kind                             TEXT NOT NULL,
    missing_signals_encoded                      TEXT NOT NULL,
    -- Bounded sample references (crate::evidence::MAX_SAMPLES_PER_SNAPSHOT).
    samples_encoded                                  TEXT NOT NULL,
    captured_at_ms                                       INTEGER NOT NULL
) STRICT;

-- Work-intent records (Deliverable 9). Emission-only — see
-- crate::work_intent's module doc for why delivery_attempts/
-- acknowledged_agent_run/terminal_outcome never leave their emission-time
-- default in any row this crate itself writes.
CREATE TABLE IF NOT EXISTS incident_work_intents (
    id                        INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    work_intent_id                TEXT NOT NULL UNIQUE,
    incident_id                       TEXT NOT NULL,
    requested_capability                 TEXT NOT NULL,
    evidence_refs_encoded                    TEXT NOT NULL,
    delivery_requirements_description            TEXT NOT NULL,
    expiry_ms                                        INTEGER NOT NULL,
    delivery_attempts                                    INTEGER NOT NULL,
    acknowledged_agent_run                                   TEXT,
    terminal_outcome_kind                                        TEXT,
    terminal_outcome_reason                                          TEXT,
    created_at_ms                                                        INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_incident_work_intents_incident
    ON incident_work_intents (incident_id);
