//! Durable incident/timeline/evidence-snapshot/work-intent storage — this
//! crate's own self-contained SQLite database, following the exact
//! established precedent `Docs/graph/handoffs/{MON-001,MON-002}-1.md` §5/§7
//! already recorded (and `Docs/graph/tasks/INC-001.md` §2 explicitly directs
//! this node to follow rather than re-derive): a new bounded-context crate
//! that needs its own durable tables but cannot edit
//! `crates/loki-storage/src/**` or `crates/loki-monitoring/src/**` opens its
//! own database, own connection, own idempotent migration runner — a third
//! application of an already-accepted pattern, not a new decision.
//!
//! # What is append-only versus mutable-in-place
//!
//! - `incidents`: **mutable in place.** An incident's *current* summary
//!   state (state, occurrence count, assignment, diagnosis/proposal,
//!   verification plan/state, bounded evidence-reference list, timestamps)
//!   is a single current value, not a history — `Docs/graph/tasks/INC-001.md`
//!   Deliverable 6 only requires the *timeline* to be append-only.
//! - `incident_timeline`: **append-only.** [`IncidentStore::save_incident`]
//!   only ever `INSERT`s new timeline rows (the ones the in-memory
//!   `Incident::timeline` gained since it was last loaded/saved — see
//!   `timeline_from_index`); it never updates or deletes an existing
//!   timeline row. `tests/timeline_append_only.rs` proves this mechanically
//!   by reading the raw row count before/after and confirming it only grows.
//! - `incident_evidence_snapshots`, `incident_work_intents`: **insert-only,
//!   never updated or deleted** by this crate.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

use loki_connectors_sdk::ids::{CapabilityId, DeviceId};
use loki_monitoring::rule::{RuleId, Severity};

use crate::error::IncidentError;
use crate::evidence::{EvidenceSnapshot, SampleEvidenceRef};
use crate::ids::{EvidenceId, IncidentId, WorkIntentId};
use crate::incident::{
    Assignee, AssigneeKind, Incident, IncidentState, TimelineEntry, TimelineEvent,
    VerificationCheck, VerificationPlan, VerificationState,
};
use crate::validate::{decode_string_list, encode_string_list};
use crate::work_intent::{DeliveryRequirements, WorkIntent, WorkIntentOutcome};

const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);

/// A durable store for incidents, their append-only timelines, bounded
/// evidence snapshots, and emitted work intents. See this module's doc for
/// why it owns its own connection/database.
pub struct IncidentStore {
    conn: Mutex<Connection>,
}

impl IncidentStore {
    /// Opens (creating if absent) the SQLite database at `path` and applies
    /// this crate's own migrations. Safe to call on every process start —
    /// already-applied migrations are skipped.
    pub fn open(path: &Path) -> Result<Self, IncidentError> {
        let conn = Connection::open(path)?;
        configure(&conn)?;
        migrations::apply_all(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Opens a private in-memory database with migrations applied. No
    /// restart durability — used for tests that do not need a real file.
    pub fn open_in_memory() -> Result<Self, IncidentError> {
        let conn = Connection::open_in_memory()?;
        configure(&conn)?;
        migrations::apply_all(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Inserts (`incident.incident_id` did not exist yet) or updates
    /// (already existed) the incident's mutable summary row, then appends
    /// exactly `incident.timeline[timeline_from_index..]` as new,
    /// never-before-persisted timeline rows. Pass `0` for a brand-new
    /// incident whose entire in-memory timeline has never been persisted;
    /// pass the timeline length as it was immediately after the most recent
    /// load/save for an incident mutated since then.
    pub(crate) fn save_incident(
        &self,
        incident: &Incident,
        timeline_from_index: usize,
    ) -> Result<(), IncidentError> {
        let conn = self.lock();
        let tx = conn.unchecked_transaction()?;
        Self::save_incident_in_transaction(&tx, incident, timeline_from_index)?;
        tx.commit()?;
        Ok(())
    }

    /// Atomically persists every durable result of one firing ingest. A
    /// caller must never observe an evidence snapshot or emitted work intent
    /// without the incident/timeline mutation that produced it.
    pub(crate) fn save_ingest(
        &self,
        incident: &Incident,
        timeline_from_index: usize,
        evidence: &EvidenceSnapshot,
        work_intent: Option<&WorkIntent>,
    ) -> Result<(), IncidentError> {
        let conn = self.lock();
        let tx = conn.unchecked_transaction()?;
        insert_evidence_snapshot(&tx, evidence)?;
        if let Some(intent) = work_intent {
            insert_work_intent(&tx, intent)?;
        }
        Self::save_incident_in_transaction(&tx, incident, timeline_from_index)?;
        tx.commit()?;
        Ok(())
    }

    fn save_incident_in_transaction(
        tx: &Transaction<'_>,
        incident: &Incident,
        timeline_from_index: usize,
    ) -> Result<(), IncidentError> {
        let correlation_device_id = incident
            .affected_scope
            .first()
            .map(|d| d.as_str().to_string())
            .unwrap_or_default();
        let (assigned_kind, assigned_identity) = match &incident.assigned {
            Some(a) => (Some(a.kind.as_wire_str()), Some(a.identity.clone())),
            None => (None, None),
        };
        let (plan_description, plan_window_ms) = match &incident.verification_plan {
            Some(p) => (
                Some(p.description.clone()),
                Some(duration_to_millis(p.observation_window)),
            ),
            None => (None, None),
        };
        let verification_started_at_ms = incident.verification_state.started_at.map(to_millis);
        let verification_checks_encoded = encode_checks(&incident.verification_state.checks);
        let evidence_refs_encoded = encode_string_list(
            &incident
                .evidence_refs
                .iter()
                .map(|e| e.as_str().to_string())
                .collect::<Vec<_>>(),
        );
        let affected_scope_encoded = encode_string_list(
            &incident
                .affected_scope
                .iter()
                .map(|d| d.as_str().to_string())
                .collect::<Vec<_>>(),
        );

        let existing_row_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM incidents WHERE incident_id = ?1",
                params![incident.incident_id.as_str()],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;

        if existing_row_id.is_some() {
            tx.execute(
                "UPDATE incidents SET
                    title = ?2, severity = ?3, state = ?4, affected_scope_encoded = ?5,
                    correlation_device_id = ?6, trigger_rule_id = ?7, trigger_rule_version = ?8,
                    occurrence_count = ?9, evidence_refs_encoded = ?10, assigned_kind = ?11,
                    assigned_identity = ?12, diagnosis_summary = ?13, proposal_summary = ?14,
                    verification_plan_description = ?15, verification_plan_window_ms = ?16,
                    verification_started_at_ms = ?17, verification_checks_encoded = ?18,
                    updated_at_ms = ?19, state_entered_at_ms = ?20
                 WHERE incident_id = ?1",
                params![
                    incident.incident_id.as_str(),
                    incident.title,
                    incident.severity.as_str(),
                    incident.state.as_wire_str(),
                    affected_scope_encoded,
                    correlation_device_id,
                    incident.trigger_rule_id.as_str(),
                    incident.trigger_rule_version,
                    incident.occurrence_count,
                    evidence_refs_encoded,
                    assigned_kind,
                    assigned_identity,
                    incident.diagnosis_summary,
                    incident.proposal_summary,
                    plan_description,
                    plan_window_ms,
                    verification_started_at_ms,
                    verification_checks_encoded,
                    to_millis(incident.updated_at),
                    to_millis(incident.state_entered_at),
                ],
            )?;
        } else {
            tx.execute(
                "INSERT INTO incidents (
                    incident_id, title, severity, state, affected_scope_encoded,
                    correlation_device_id, trigger_rule_id, trigger_rule_version,
                    occurrence_count, evidence_refs_encoded, assigned_kind, assigned_identity,
                    diagnosis_summary, proposal_summary, verification_plan_description,
                    verification_plan_window_ms, verification_started_at_ms,
                    verification_checks_encoded, created_at_ms, updated_at_ms, state_entered_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                params![
                    incident.incident_id.as_str(),
                    incident.title,
                    incident.severity.as_str(),
                    incident.state.as_wire_str(),
                    affected_scope_encoded,
                    correlation_device_id,
                    incident.trigger_rule_id.as_str(),
                    incident.trigger_rule_version,
                    incident.occurrence_count,
                    evidence_refs_encoded,
                    assigned_kind,
                    assigned_identity,
                    incident.diagnosis_summary,
                    incident.proposal_summary,
                    plan_description,
                    plan_window_ms,
                    verification_started_at_ms,
                    verification_checks_encoded,
                    to_millis(incident.created_at),
                    to_millis(incident.updated_at),
                    to_millis(incident.state_entered_at),
                ],
            )?;
        }

        for entry in &incident.timeline[timeline_from_index.min(incident.timeline.len())..] {
            insert_timeline_entry(tx, incident.incident_id.as_str(), entry)?;
        }

        Ok(())
    }

    pub fn get_incident(
        &self,
        incident_id: &IncidentId,
    ) -> Result<Option<Incident>, IncidentError> {
        let conn = self.lock();
        let raw = conn
            .query_row(
                &format!("{INCIDENT_SELECT_COLUMNS} WHERE incident_id = ?1"),
                params![incident_id.as_str()],
                row_to_raw_incident,
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let timeline = self.load_timeline(&conn, incident_id.as_str())?;
        Ok(Some(raw_to_incident(raw, timeline)?))
    }

    /// The most recently created incident for `(rule_id, device)`, in any
    /// state — `crate::engine`'s correlation key. `None` if no incident has
    /// ever been created for that pair.
    pub fn latest_incident_for(
        &self,
        rule_id: &RuleId,
        device: &DeviceId,
    ) -> Result<Option<Incident>, IncidentError> {
        let conn = self.lock();
        let raw = conn
            .query_row(
                &format!(
                    "{INCIDENT_SELECT_COLUMNS} WHERE trigger_rule_id = ?1 AND correlation_device_id = ?2 ORDER BY id DESC LIMIT 1"
                ),
                params![rule_id.as_str(), device.as_str()],
                row_to_raw_incident,
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let incident_id = raw.incident_id.clone();
        let timeline = self.load_timeline(&conn, &incident_id)?;
        Ok(Some(raw_to_incident(raw, timeline)?))
    }

    pub fn list_incidents_in_state(
        &self,
        state: IncidentState,
    ) -> Result<Vec<Incident>, IncidentError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("{INCIDENT_SELECT_COLUMNS} WHERE state = ?1"))?;
        let raw_rows = stmt.query_map(params![state.as_wire_str()], row_to_raw_incident)?;
        let mut out = Vec::new();
        for raw in raw_rows {
            let raw = raw?;
            let timeline = self.load_timeline(&conn, &raw.incident_id)?;
            out.push(raw_to_incident(raw, timeline)?);
        }
        Ok(out)
    }

    pub fn timeline(&self, incident_id: &IncidentId) -> Result<Vec<TimelineEntry>, IncidentError> {
        let conn = self.lock();
        self.load_timeline(&conn, incident_id.as_str())
    }

    fn load_timeline(
        &self,
        conn: &Connection,
        incident_id: &str,
    ) -> Result<Vec<TimelineEntry>, IncidentError> {
        let mut stmt = conn.prepare(
            "SELECT at_ms, event_kind, from_state, to_state, transition, note,
                    occurrence_count, assignee_kind, assignee_identity, check_passed
             FROM incident_timeline WHERE incident_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![incident_id], row_to_timeline_entry)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row??);
        }
        Ok(out)
    }

    #[cfg(test)]
    fn insert_evidence_snapshot(&self, evidence: &EvidenceSnapshot) -> Result<(), IncidentError> {
        let conn = self.lock();
        insert_evidence_snapshot(&conn, evidence)
    }

    pub fn get_evidence_snapshot(
        &self,
        evidence_id: &EvidenceId,
    ) -> Result<Option<EvidenceSnapshot>, IncidentError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT evidence_id, rule_id, rule_version, device_id, outcome_kind,
                    missing_signals_encoded, samples_encoded, captured_at_ms
             FROM incident_evidence_snapshots WHERE evidence_id = ?1",
            params![evidence_id.as_str()],
            row_to_evidence_snapshot,
        )
        .optional()
        .map_err(IncidentError::from)?
        .transpose()
    }

    pub fn list_work_intents_for_incident(
        &self,
        incident_id: &IncidentId,
    ) -> Result<Vec<WorkIntent>, IncidentError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT work_intent_id, incident_id, requested_capability, evidence_refs_encoded,
                    delivery_requirements_description, expiry_ms, delivery_attempts,
                    acknowledged_agent_run, terminal_outcome_kind, terminal_outcome_reason,
                    created_at_ms
             FROM incident_work_intents WHERE incident_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![incident_id.as_str()], row_to_work_intent)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row??);
        }
        Ok(out)
    }
}

fn insert_evidence_snapshot(
    conn: &Connection,
    evidence: &EvidenceSnapshot,
) -> Result<(), IncidentError> {
    conn.execute(
        "INSERT INTO incident_evidence_snapshots
            (evidence_id, rule_id, rule_version, device_id, outcome_kind,
             missing_signals_encoded, samples_encoded, captured_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            evidence.evidence_id.as_str(),
            evidence.rule_id.as_str(),
            evidence.rule_version,
            evidence.device_id.as_str(),
            evidence.outcome_kind,
            encode_string_list(&evidence.missing_signals),
            encode_samples(&evidence.samples),
            to_millis(evidence.captured_at),
        ],
    )?;
    Ok(())
}

fn insert_work_intent(conn: &Connection, intent: &WorkIntent) -> Result<(), IncidentError> {
    let (terminal_kind, terminal_reason) = encode_terminal_outcome(&intent.terminal_outcome);
    conn.execute(
        "INSERT INTO incident_work_intents
            (work_intent_id, incident_id, requested_capability, evidence_refs_encoded,
             delivery_requirements_description, expiry_ms, delivery_attempts,
             acknowledged_agent_run, terminal_outcome_kind, terminal_outcome_reason,
             created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            intent.work_intent_id.as_str(),
            intent.incident_id.as_str(),
            intent.requested_capability.as_str(),
            encode_string_list(
                &intent
                    .evidence_refs
                    .iter()
                    .map(|e| e.as_str().to_string())
                    .collect::<Vec<_>>()
            ),
            intent.delivery_requirements.description,
            to_millis(intent.expiry),
            intent.delivery_attempts,
            intent.acknowledged_agent_run,
            terminal_kind,
            terminal_reason,
            to_millis(intent.created_at),
        ],
    )?;
    Ok(())
}

const INCIDENT_SELECT_COLUMNS: &str = "SELECT
    incident_id, title, severity, state, affected_scope_encoded, trigger_rule_id,
    trigger_rule_version, occurrence_count, evidence_refs_encoded, assigned_kind,
    assigned_identity, diagnosis_summary, proposal_summary, verification_plan_description,
    verification_plan_window_ms, verification_started_at_ms, verification_checks_encoded,
    created_at_ms, updated_at_ms, state_entered_at_ms
 FROM incidents";

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

/// Encodes one [`VerificationCheck`] as `at_ms:passed:note` via the same
/// nested length-prefixed scheme `crate::validate::encode_string_list` uses,
/// then joins the whole list the same way. An absent note and an empty-string
/// note are not distinguished on round-trip (both decode to `None`) — a
/// documented, deliberate simplification (a check's note is a free-form
/// human aid, not a field this crate's own logic branches on).
fn encode_checks(checks: &[VerificationCheck]) -> String {
    let items: Vec<String> = checks
        .iter()
        .map(|c| {
            encode_string_list(&[
                to_millis(c.at).to_string(),
                if c.passed {
                    "1".to_string()
                } else {
                    "0".to_string()
                },
                c.note.clone().unwrap_or_default(),
            ])
        })
        .collect();
    encode_string_list(&items)
}

fn decode_checks(encoded: &str) -> Vec<VerificationCheck> {
    decode_string_list(encoded)
        .iter()
        .filter_map(|item| {
            let fields = decode_string_list(item);
            if fields.len() != 3 {
                return None;
            }
            let at_ms: i64 = fields[0].parse().ok()?;
            let passed = fields[1] == "1";
            let note = if fields[2].is_empty() {
                None
            } else {
                Some(fields[2].clone())
            };
            Some(VerificationCheck {
                at: from_millis(at_ms),
                passed,
                note,
            })
        })
        .collect()
}

fn encode_samples(samples: &[SampleEvidenceRef]) -> String {
    let items: Vec<String> = samples
        .iter()
        .map(|s| {
            encode_string_list(&[
                s.metric_name.clone(),
                s.value.to_string(),
                s.unit.clone(),
                to_millis(s.source_time).to_string(),
            ])
        })
        .collect();
    encode_string_list(&items)
}

fn decode_samples(encoded: &str) -> Vec<SampleEvidenceRef> {
    decode_string_list(encoded)
        .iter()
        .filter_map(|item| {
            let fields = decode_string_list(item);
            if fields.len() != 4 {
                return None;
            }
            let value: f64 = fields[1].parse().ok()?;
            let source_time_ms: i64 = fields[3].parse().ok()?;
            Some(SampleEvidenceRef {
                metric_name: fields[0].clone(),
                value,
                unit: fields[2].clone(),
                source_time: from_millis(source_time_ms),
            })
        })
        .collect()
}

fn encode_terminal_outcome(
    outcome: &Option<WorkIntentOutcome>,
) -> (Option<&'static str>, Option<String>) {
    match outcome {
        None => (None, None),
        Some(WorkIntentOutcome::Delivered) => (Some("delivered"), None),
        Some(WorkIntentOutcome::Expired) => (Some("expired"), None),
        Some(WorkIntentOutcome::Cancelled) => (Some("cancelled"), None),
        Some(WorkIntentOutcome::Failed { reason }) => (Some("failed"), Some(reason.clone())),
    }
}

fn decode_terminal_outcome(
    kind: Option<String>,
    reason: Option<String>,
) -> Result<Option<WorkIntentOutcome>, IncidentError> {
    match kind.as_deref() {
        None => Ok(None),
        Some("delivered") => Ok(Some(WorkIntentOutcome::Delivered)),
        Some("expired") => Ok(Some(WorkIntentOutcome::Expired)),
        Some("cancelled") => Ok(Some(WorkIntentOutcome::Cancelled)),
        Some("failed") => Ok(Some(WorkIntentOutcome::Failed {
            reason: reason.unwrap_or_default(),
        })),
        Some(other) => Err(IncidentError::Storage(format!(
            "unknown stored terminal_outcome_kind {other:?}"
        ))),
    }
}

struct RawIncidentRow {
    incident_id: String,
    title: String,
    severity: String,
    state: String,
    affected_scope_encoded: String,
    trigger_rule_id: String,
    trigger_rule_version: i64,
    occurrence_count: i64,
    evidence_refs_encoded: String,
    assigned_kind: Option<String>,
    assigned_identity: Option<String>,
    diagnosis_summary: Option<String>,
    proposal_summary: Option<String>,
    verification_plan_description: Option<String>,
    verification_plan_window_ms: Option<i64>,
    verification_started_at_ms: Option<i64>,
    verification_checks_encoded: String,
    created_at_ms: i64,
    updated_at_ms: i64,
    state_entered_at_ms: i64,
}

fn row_to_raw_incident(row: &Row<'_>) -> rusqlite::Result<RawIncidentRow> {
    Ok(RawIncidentRow {
        incident_id: row.get(0)?,
        title: row.get(1)?,
        severity: row.get(2)?,
        state: row.get(3)?,
        affected_scope_encoded: row.get(4)?,
        trigger_rule_id: row.get(5)?,
        trigger_rule_version: row.get(6)?,
        occurrence_count: row.get(7)?,
        evidence_refs_encoded: row.get(8)?,
        assigned_kind: row.get(9)?,
        assigned_identity: row.get(10)?,
        diagnosis_summary: row.get(11)?,
        proposal_summary: row.get(12)?,
        verification_plan_description: row.get(13)?,
        verification_plan_window_ms: row.get(14)?,
        verification_started_at_ms: row.get(15)?,
        verification_checks_encoded: row.get(16)?,
        created_at_ms: row.get(17)?,
        updated_at_ms: row.get(18)?,
        state_entered_at_ms: row.get(19)?,
    })
}

fn raw_to_incident(
    raw: RawIncidentRow,
    timeline: Vec<TimelineEntry>,
) -> Result<Incident, IncidentError> {
    let state: IncidentState = raw.state.parse()?;
    let severity = Severity::new(raw.severity)?;
    let trigger_rule_id = RuleId::new(raw.trigger_rule_id)?;
    let affected_scope = decode_string_list(&raw.affected_scope_encoded)
        .into_iter()
        .map(DeviceId::new)
        .collect::<Result<Vec<_>, _>>()?;
    let evidence_refs = decode_string_list(&raw.evidence_refs_encoded)
        .into_iter()
        .map(EvidenceId::new)
        .collect::<Result<Vec<_>, _>>()?;

    let assigned = match (raw.assigned_kind, raw.assigned_identity) {
        (Some(kind), Some(identity)) => {
            let kind: AssigneeKind = kind.parse()?;
            Some(Assignee::new(kind, identity)?)
        }
        _ => None,
    };

    let verification_plan = match (
        raw.verification_plan_description,
        raw.verification_plan_window_ms,
    ) {
        (Some(description), Some(window_ms)) => Some(VerificationPlan::new(
            description,
            millis_to_duration(window_ms),
        )?),
        _ => None,
    };

    let verification_state = VerificationState {
        started_at: raw.verification_started_at_ms.map(from_millis),
        checks: decode_checks(&raw.verification_checks_encoded),
    };

    Ok(Incident {
        incident_id: IncidentId::new(raw.incident_id)?,
        title: raw.title,
        severity,
        state,
        affected_scope,
        trigger_rule_id,
        trigger_rule_version: raw.trigger_rule_version as u32,
        occurrence_count: raw.occurrence_count as u32,
        evidence_refs,
        assigned,
        diagnosis_summary: raw.diagnosis_summary,
        proposal_summary: raw.proposal_summary,
        verification_plan,
        verification_state,
        timeline,
        created_at: from_millis(raw.created_at_ms),
        updated_at: from_millis(raw.updated_at_ms),
        state_entered_at: from_millis(raw.state_entered_at_ms),
    })
}

fn insert_timeline_entry(
    tx: &rusqlite::Transaction<'_>,
    incident_id: &str,
    entry: &TimelineEntry,
) -> Result<(), IncidentError> {
    let at_ms = to_millis(entry.at);
    match &entry.event {
        TimelineEvent::Created => {
            tx.execute(
                "INSERT INTO incident_timeline (incident_id, at_ms, event_kind) VALUES (?1, ?2, 'created')",
                params![incident_id, at_ms],
            )?;
        }
        TimelineEvent::StateTransition {
            from,
            to,
            transition,
            note,
        } => {
            tx.execute(
                "INSERT INTO incident_timeline
                    (incident_id, at_ms, event_kind, from_state, to_state, transition, note)
                 VALUES (?1, ?2, 'state_transition', ?3, ?4, ?5, ?6)",
                params![
                    incident_id,
                    at_ms,
                    from.as_wire_str(),
                    to.as_wire_str(),
                    transition.as_wire_str(),
                    note
                ],
            )?;
        }
        TimelineEvent::CorrelatedOccurrence { occurrence_count } => {
            tx.execute(
                "INSERT INTO incident_timeline
                    (incident_id, at_ms, event_kind, occurrence_count)
                 VALUES (?1, ?2, 'correlated_occurrence', ?3)",
                params![incident_id, at_ms, occurrence_count],
            )?;
        }
        TimelineEvent::AssignmentChanged { assignee } => {
            let (kind, identity) = match assignee {
                Some(a) => (Some(a.kind.as_wire_str()), Some(a.identity.clone())),
                None => (None, None),
            };
            tx.execute(
                "INSERT INTO incident_timeline
                    (incident_id, at_ms, event_kind, assignee_kind, assignee_identity)
                 VALUES (?1, ?2, 'assignment_changed', ?3, ?4)",
                params![incident_id, at_ms, kind, identity],
            )?;
        }
        TimelineEvent::VerificationCheckRecorded { passed, note } => {
            tx.execute(
                "INSERT INTO incident_timeline
                    (incident_id, at_ms, event_kind, check_passed, note)
                 VALUES (?1, ?2, 'verification_check_recorded', ?3, ?4)",
                params![incident_id, at_ms, *passed as i64, note],
            )?;
        }
        TimelineEvent::DiagnosisUpdated => {
            tx.execute(
                "INSERT INTO incident_timeline (incident_id, at_ms, event_kind) VALUES (?1, ?2, 'diagnosis_updated')",
                params![incident_id, at_ms],
            )?;
        }
        TimelineEvent::ProposalUpdated => {
            tx.execute(
                "INSERT INTO incident_timeline (incident_id, at_ms, event_kind) VALUES (?1, ?2, 'proposal_updated')",
                params![incident_id, at_ms],
            )?;
        }
    }
    Ok(())
}

fn row_to_timeline_entry(row: &Row<'_>) -> rusqlite::Result<Result<TimelineEntry, IncidentError>> {
    let at_ms: i64 = row.get(0)?;
    let event_kind: String = row.get(1)?;
    let from_state: Option<String> = row.get(2)?;
    let to_state: Option<String> = row.get(3)?;
    let transition: Option<String> = row.get(4)?;
    let note: Option<String> = row.get(5)?;
    let occurrence_count: Option<i64> = row.get(6)?;
    let assignee_kind: Option<String> = row.get(7)?;
    let assignee_identity: Option<String> = row.get(8)?;
    let check_passed: Option<i64> = row.get(9)?;

    let event = (|| -> Result<TimelineEvent, IncidentError> {
        Ok(match event_kind.as_str() {
            "created" => TimelineEvent::Created,
            "state_transition" => TimelineEvent::StateTransition {
                from: from_state.unwrap_or_default().parse()?,
                to: to_state.unwrap_or_default().parse()?,
                transition: transition.unwrap_or_default().parse()?,
                note,
            },
            "correlated_occurrence" => TimelineEvent::CorrelatedOccurrence {
                occurrence_count: occurrence_count.unwrap_or_default() as u32,
            },
            "assignment_changed" => TimelineEvent::AssignmentChanged {
                assignee: match (assignee_kind, assignee_identity) {
                    (Some(kind), Some(identity)) => Some(Assignee::new(kind.parse()?, identity)?),
                    _ => None,
                },
            },
            "verification_check_recorded" => TimelineEvent::VerificationCheckRecorded {
                passed: check_passed.unwrap_or_default() != 0,
                note,
            },
            "diagnosis_updated" => TimelineEvent::DiagnosisUpdated,
            "proposal_updated" => TimelineEvent::ProposalUpdated,
            other => {
                return Err(IncidentError::Storage(format!(
                    "unknown stored timeline event_kind {other:?}"
                )));
            }
        })
    })();

    Ok(event.map(|event| TimelineEntry {
        at: from_millis(at_ms),
        event,
    }))
}

fn row_to_evidence_snapshot(
    row: &Row<'_>,
) -> rusqlite::Result<Result<EvidenceSnapshot, IncidentError>> {
    let evidence_id: String = row.get(0)?;
    let rule_id: String = row.get(1)?;
    let rule_version: i64 = row.get(2)?;
    let device_id: String = row.get(3)?;
    let outcome_kind: String = row.get(4)?;
    let missing_signals_encoded: String = row.get(5)?;
    let samples_encoded: String = row.get(6)?;
    let captured_at_ms: i64 = row.get(7)?;

    let built = (|| -> Result<EvidenceSnapshot, IncidentError> {
        Ok(EvidenceSnapshot {
            evidence_id: EvidenceId::new(evidence_id)?,
            rule_id: RuleId::new(rule_id)?,
            rule_version: rule_version as u32,
            device_id: DeviceId::new(device_id)?,
            outcome_kind,
            missing_signals: decode_string_list(&missing_signals_encoded),
            samples: decode_samples(&samples_encoded),
            captured_at: from_millis(captured_at_ms),
        })
    })();
    Ok(built)
}

fn row_to_work_intent(row: &Row<'_>) -> rusqlite::Result<Result<WorkIntent, IncidentError>> {
    let work_intent_id: String = row.get(0)?;
    let incident_id: String = row.get(1)?;
    let requested_capability: String = row.get(2)?;
    let evidence_refs_encoded: String = row.get(3)?;
    let delivery_requirements_description: String = row.get(4)?;
    let expiry_ms: i64 = row.get(5)?;
    let delivery_attempts: i64 = row.get(6)?;
    let acknowledged_agent_run: Option<String> = row.get(7)?;
    let terminal_outcome_kind: Option<String> = row.get(8)?;
    let terminal_outcome_reason: Option<String> = row.get(9)?;
    let created_at_ms: i64 = row.get(10)?;

    let built = (|| -> Result<WorkIntent, IncidentError> {
        Ok(WorkIntent {
            work_intent_id: WorkIntentId::new(work_intent_id)?,
            incident_id: IncidentId::new(incident_id)?,
            requested_capability: CapabilityId::new(requested_capability)?,
            evidence_refs: decode_string_list(&evidence_refs_encoded)
                .into_iter()
                .map(EvidenceId::new)
                .collect::<Result<Vec<_>, _>>()?,
            delivery_requirements: DeliveryRequirements::new(delivery_requirements_description)?,
            expiry: from_millis(expiry_ms),
            delivery_attempts: delivery_attempts as u32,
            acknowledged_agent_run,
            terminal_outcome: decode_terminal_outcome(
                terminal_outcome_kind,
                terminal_outcome_reason,
            )?,
            created_at: from_millis(created_at_ms),
        })
    })();
    Ok(built)
}

mod migrations {
    use rusqlite::Connection;

    use crate::error::IncidentError;

    const INCIDENTS_MIGRATION_SQL: &str = include_str!("../migrations/0001_incidents.sql");

    pub(super) fn apply_all(conn: &Connection) -> Result<(), IncidentError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS inc_schema_migrations (
                version    INTEGER NOT NULL PRIMARY KEY,
                name        TEXT NOT NULL
            ) STRICT;",
        )?;

        let already: i64 = conn.query_row(
            "SELECT COUNT(*) FROM inc_schema_migrations WHERE version = 1",
            [],
            |r| r.get::<_, i64>(0),
        )?;
        if already > 0 {
            return Ok(());
        }

        conn.execute_batch(INCIDENTS_MIGRATION_SQL)?;
        conn.execute(
            "INSERT INTO inc_schema_migrations (version, name) VALUES (1, 'incidents')",
            [],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_monitoring::rule::Severity;

    fn device(id: &str) -> DeviceId {
        DeviceId::new(id).unwrap()
    }

    fn epoch(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn fresh_incident() -> Incident {
        Incident::new(
            IncidentId::generate(),
            "title",
            Severity::new("critical").unwrap(),
            vec![device("dev-1")],
            RuleId::new("rule-1").unwrap(),
            1,
            epoch(0),
        )
        .unwrap()
    }

    #[test]
    fn open_in_memory_starts_empty() {
        let store = IncidentStore::open_in_memory().unwrap();
        assert!(
            store
                .get_incident(&IncidentId::new("does-not-exist").unwrap())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn save_and_read_back_round_trips_exactly() {
        let store = IncidentStore::open_in_memory().unwrap();
        let incident = fresh_incident();
        store.save_incident(&incident, 0).unwrap();

        let read = store.get_incident(&incident.incident_id).unwrap().unwrap();
        assert_eq!(read, incident);
    }

    #[test]
    fn latest_incident_for_finds_by_rule_and_device() {
        let store = IncidentStore::open_in_memory().unwrap();
        let incident = fresh_incident();
        store.save_incident(&incident, 0).unwrap();

        let found = store
            .latest_incident_for(&RuleId::new("rule-1").unwrap(), &device("dev-1"))
            .unwrap()
            .unwrap();
        assert_eq!(found.incident_id, incident.incident_id);

        assert!(
            store
                .latest_incident_for(&RuleId::new("rule-1").unwrap(), &device("dev-2"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn migrations_are_idempotent_across_repeated_opens_on_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("incidents.sqlite3");
        {
            let store = IncidentStore::open(&path).unwrap();
            store.save_incident(&fresh_incident(), 0).unwrap();
        }
        let store = IncidentStore::open(&path).unwrap();
        assert!(
            store
                .latest_incident_for(&RuleId::new("rule-1").unwrap(), &device("dev-1"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn evidence_snapshot_round_trips() {
        let store = IncidentStore::open_in_memory().unwrap();
        let evidence = EvidenceSnapshot::capture(
            RuleId::new("rule-1").unwrap(),
            1,
            device("dev-1"),
            &["temperature_c".to_string()],
            "fired",
            vec!["temperature_c".to_string()],
            &[],
            epoch(5),
        );
        store.insert_evidence_snapshot(&evidence).unwrap();
        let read = store
            .get_evidence_snapshot(&evidence.evidence_id)
            .unwrap()
            .unwrap();
        assert_eq!(read, evidence);
    }
}
