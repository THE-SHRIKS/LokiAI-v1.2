//! `IncidentEngine` — turns a `loki_monitoring::evaluate::EvaluationOutcome`
//! into a correlated incident, applies the rest of `Docs/SPEC.md` §4.6's
//! state machine on top of `crate::store::IncidentStore`, and emits
//! work-intent records. This is the crate's one public orchestration surface
//! — `crate::incident::Incident`'s own methods enforce the state machine
//! itself; this module decides *when* to call them from an ingested
//! evaluation outcome or a policy sweep.
//!
//! # Correlation key and window — a documented judgment call
//!
//! `Docs/graph/tasks/INC-001.md` Deliverable 3 explicitly requires this
//! crate to "define and document your own correlation window/key since
//! `Docs/SPEC.md` does not pin one" (the same category of necessary
//! judgment call `Docs/graph/handoffs/MON-001-1.md` §7 already documented
//! for its freshness window, and `Docs/graph/handoffs/MON-002-1.md` §7
//! documented for its condition grammar).
//!
//! - **Key**: `(rule_id, device)` — the exact granularity
//!   `loki_monitoring::evaluate::RuleEvaluator::evaluate` already fires at
//!   (one call = one rule + one device); this crate does not invent a
//!   coarser or finer key.
//! - **Window**: `correlation_window`, an explicit `Duration` the caller
//!   supplies to [`IncidentEngine::new`] — no invented default, following
//!   `crates/loki-monitoring/src/freshness.rs`'s exact "no fixed default,
//!   caller-supplied" precedent. A new `Fired`/`MissingDataAlert` outcome for
//!   the same key correlates into the existing non-`Closed` incident for
//!   that key if `now - existing.updated_at <= correlation_window`;
//!   otherwise a fresh incident is created (the old one is left exactly as
//!   it was — this crate does not orphan-sweep or force-close it beyond the
//!   explicit retention-policy sweep, `Self::apply_retention_policy`).
//!
//! # What "correlates" means per existing-incident state
//!
//! - **`Closed`**: never correlates. A closed incident is done; a new
//!   firing always starts a fresh incident.
//! - **`Resolved`**: does not use `correlation_window` at all — it uses the
//!   *separate*, also caller-supplied `reopen_policy_window` against
//!   `existing.state_entered_at` (when `Resolved` was entered): within that
//!   window, `Resolved -> Open` (`Docs/SPEC.md` §4.6's named "recurrence
//!   within reopen policy" edge), immediately auto-advanced to
//!   `Investigating` (`Open`'s only outgoing edge — see below for why this
//!   auto-advance is not a diagram violation); outside it, a fresh incident
//!   is created and the old `Resolved` one is left untouched (it will
//!   eventually leave `Resolved` only via [`Self::apply_retention_policy`]).
//! - **`Open`, `Investigating`, `WaitingApproval`, `Executing`**: within
//!   `correlation_window` of `existing.updated_at`, `occurrence_count`
//!   increments and an append-only `CorrelatedOccurrence` timeline entry is
//!   recorded — `state` does not change (Deliverable 3: "not a fresh
//!   incident per firing").
//! - **`Verifying`**: within `correlation_window`, the same
//!   `occurrence_count` increment happens, **and** `Docs/SPEC.md` §4.6's
//!   named `Verifying -> Investigating: recurrence` edge fires — a new
//!   firing while an incident is being verified is exactly the "recurrence"
//!   the diagram names, not a silent no-op.
//!
//! # Why creating a new incident auto-advances `Open -> Investigating`
//!
//! `Docs/SPEC.md` §4.6's diagram gives `Open` exactly one outgoing edge
//! (`Open -> Investigating`) and no other state re-enters `Open` except via
//! the reopen edge. `Open` is therefore not a resting state a real incident
//! is meant to sit in — nothing in this crate's scope performs work while
//! `state == Open`. `Docs/graph/tasks/INC-001.md` Deliverable 9 requires a
//! work intent "a Fired/MissingDataAlert transition into `Open`/
//! `Investigating`" — treating creation-into-`Open` and its single legal
//! next edge (`BeginInvestigation`) as one atomic sequence, with exactly one
//! resulting work intent, satisfies that requirement without ever leaving an
//! incident parked in a state with no legal action pending. Both edges are
//! still real, individually-reachable, individually-tested transitions
//! (`tests/state_machine_transitions.rs` exercises `[*] --> Open` and
//! `Open --> Investigating` as the two steps they are); this module simply
//! never calls the first without immediately calling the second.
//!
//! # Work-intent emission — exactly Deliverable 9's trigger, no more
//!
//! A work intent is emitted only when this call newly lands the incident at
//! `Investigating` as a direct result of processing a firing outcome: a
//! brand-new incident, a `Resolved -> Open -> Investigating` reopen, or a
//! `Verifying -> Investigating` recurrence. A plain within-window
//! correlation onto an already-`Investigating`/`Open`/`WaitingApproval`/
//! `Executing` incident does **not** emit a second work intent — the
//! original one already covers that ongoing investigation; emitting a fresh
//! one per duplicate firing would defeat the entire purpose of correlation.

use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use loki_connectors_sdk::ids::{CapabilityId, DeviceId};
use loki_monitoring::evaluate::EvaluationOutcome;
use loki_monitoring::rule::MonitoringRule;
use loki_monitoring::sample::TelemetrySample;

use crate::error::IncidentError;
use crate::evidence::EvidenceSnapshot;
use crate::ids::IncidentId;
use crate::incident::{Assignee, Incident, IncidentState, IncidentTransition, VerificationPlan};
use crate::store::IncidentStore;
use crate::work_intent::{DeliveryRequirements, WorkIntent};

/// The result of one [`IncidentEngine::ingest_fired_outcome`] call.
pub struct IngestResult {
    pub incident: Incident,
    /// `Some` exactly when this call newly landed the incident at
    /// `Investigating` — see this module's doc.
    pub work_intent: Option<WorkIntent>,
    /// `true` if this call correlated into an already-existing incident
    /// (`occurrence_count` incremented on a pre-existing row) rather than
    /// creating a brand-new one.
    pub correlated: bool,
}

/// Orchestrates `crate::incident::Incident`'s state machine, evidence
/// capture, correlation, and work-intent emission on top of a durable
/// [`IncidentStore`]. See this module's doc for the correlation/reopen/
/// retention window semantics — all four `Duration`s below are explicit,
/// caller-supplied policy, never an invented default.
pub struct IncidentEngine {
    store: IncidentStore,
    /// Serializes every read-mutate-write sequence inside the single owning
    /// runtime process. Without this guard, simultaneous firings could create
    /// duplicate incidents and concurrent transitions could overwrite each
    /// other's state or append-only timeline additions.
    mutation_lock: Mutex<()>,
    correlation_window: Duration,
    reopen_policy_window: Duration,
    retention_close_window: Duration,
    work_intent_expiry_window: Duration,
}

impl IncidentEngine {
    pub fn new(
        store: IncidentStore,
        correlation_window: Duration,
        reopen_policy_window: Duration,
        retention_close_window: Duration,
        work_intent_expiry_window: Duration,
    ) -> Self {
        Self {
            store,
            mutation_lock: Mutex::new(()),
            correlation_window,
            reopen_policy_window,
            retention_close_window,
            work_intent_expiry_window,
        }
    }

    pub fn store(&self) -> &IncidentStore {
        &self.store
    }

    /// Consumes one `Fired`/`MissingDataAlert` [`EvaluationOutcome`],
    /// captures bounded evidence from `evidence_samples` (the same slice the
    /// caller used to produce `outcome`), correlates or creates an incident
    /// per this module's documented policy, and emits a work intent when
    /// (and only when) this call newly drives the incident into
    /// `Investigating`. Returns [`IncidentError::NotAFiringOutcome`] for any
    /// other `outcome` variant.
    #[allow(clippy::too_many_arguments)]
    pub fn ingest_fired_outcome(
        &self,
        rule: &MonitoringRule,
        device: &DeviceId,
        outcome: &EvaluationOutcome,
        evidence_samples: &[TelemetrySample],
        requested_capability: CapabilityId,
        delivery_requirements: DeliveryRequirements,
        at: SystemTime,
    ) -> Result<IngestResult, IncidentError> {
        // The runtime is the single database owner, but evaluations may
        // arrive concurrently on different worker threads. Keep the full
        // evidence -> correlate/create -> work-intent -> incident sequence
        // serialized so the correlation contract remains true under race.
        let _mutation_guard = self
            .mutation_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if !matches!(
            outcome,
            EvaluationOutcome::Fired { .. } | EvaluationOutcome::MissingDataAlert { .. }
        ) {
            return Err(IncidentError::NotAFiringOutcome);
        }

        let evidence = EvidenceSnapshot::capture(
            rule.rule_id.clone(),
            rule.version,
            device.clone(),
            &rule.required_signals,
            outcome.kind_wire_str(),
            outcome.missing_signals(),
            evidence_samples,
            at,
        );
        let existing = self.store.latest_incident_for(&rule.rule_id, device)?;

        // The fourth element is `timeline_from_index`: how much of the
        // returned incident's in-memory timeline was already persisted
        // before this call, so `IncidentStore::save_incident` only ever
        // appends the genuinely new entries (append-only, per that module's
        // doc) rather than re-inserting ones that already exist.
        let (mut incident, correlated, emits_work_intent, timeline_from_index) = match existing {
            None => {
                let incident = self.create_incident(rule, device, at)?;
                (incident, false, true, 0)
            }
            Some(existing) => match existing.state {
                IncidentState::Closed => {
                    let incident = self.create_incident(rule, device, at)?;
                    (incident, false, true, 0)
                }
                IncidentState::Resolved => {
                    let within_reopen =
                        elapsed_since(at, existing.state_entered_at) <= self.reopen_policy_window;
                    if within_reopen {
                        let mut existing = existing;
                        let original_len = existing.timeline.len();
                        existing.reopen_due_to_recurrence(
                            at,
                            Some("recurrence within reopen policy window".to_string()),
                        )?;
                        existing.apply_transition(
                            IncidentTransition::BeginInvestigation,
                            at,
                            Some("auto-advance out of Open into Investigating".to_string()),
                        )?;
                        (existing, false, true, original_len)
                    } else {
                        let incident = self.create_incident(rule, device, at)?;
                        (incident, false, true, 0)
                    }
                }
                _ => {
                    // Open, Investigating, WaitingApproval, Executing, Verifying.
                    let within_window =
                        elapsed_since(at, existing.updated_at) <= self.correlation_window;
                    if !within_window {
                        let incident = self.create_incident(rule, device, at)?;
                        (incident, false, true, 0)
                    } else {
                        let mut existing = existing;
                        let original_len = existing.timeline.len();
                        existing.record_correlated_occurrence(at);
                        if existing.state == IncidentState::Verifying {
                            existing.apply_transition(
                                IncidentTransition::VerificationRecurrenceOrFailedCheck,
                                at,
                                Some("new firing recurred during verification".to_string()),
                            )?;
                            (existing, true, true, original_len)
                        } else {
                            (existing, true, false, original_len)
                        }
                    }
                }
            },
        };

        incident.push_evidence_ref(evidence.evidence_id.clone());

        let work_intent = if emits_work_intent {
            let expiry = at.checked_add(self.work_intent_expiry_window).ok_or(
                IncidentError::TimeOverflow {
                    field: "work_intent.expiry",
                },
            )?;
            let intent = WorkIntent::emit(
                incident.incident_id.clone(),
                requested_capability,
                std::slice::from_ref(&evidence),
                delivery_requirements,
                expiry,
                at,
            );
            Some(intent)
        } else {
            None
        };

        self.store.save_ingest(
            &incident,
            timeline_from_index,
            &evidence,
            work_intent.as_ref(),
        )?;

        Ok(IngestResult {
            incident,
            work_intent,
            correlated,
        })
    }

    fn create_incident(
        &self,
        rule: &MonitoringRule,
        device: &DeviceId,
        at: SystemTime,
    ) -> Result<Incident, IncidentError> {
        let mut incident = Incident::new(
            IncidentId::generate(),
            format!("{} on {}", rule.rule_id.as_str(), device.as_str()),
            rule.severity.clone(),
            vec![device.clone()],
            rule.rule_id.clone(),
            rule.version,
            at,
        )?;
        incident.apply_transition(
            IncidentTransition::BeginInvestigation,
            at,
            Some("new incident opened from a firing outcome".to_string()),
        )?;
        Ok(incident)
    }

    /// Sweeps every durably stored `Resolved` incident and closes any whose
    /// `state_entered_at` (when it entered `Resolved`) is at least
    /// `retention_close_window` in the past as of `now`
    /// (`Docs/SPEC.md` §4.6's `Resolved -> Closed: retention/policy close`
    /// edge). Returns every incident this call closed.
    pub fn apply_retention_policy(&self, now: SystemTime) -> Result<Vec<Incident>, IncidentError> {
        let _mutation_guard = self
            .mutation_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let resolved = self
            .store
            .list_incidents_in_state(IncidentState::Resolved)?;
        let mut closed = Vec::new();
        for mut incident in resolved {
            if elapsed_since(now, incident.state_entered_at) >= self.retention_close_window {
                let original_len = incident.timeline.len();
                incident.apply_transition(
                    IncidentTransition::CloseRetentionPolicy,
                    now,
                    Some("retention policy close".to_string()),
                )?;
                self.store.save_incident(&incident, original_len)?;
                closed.push(incident);
            }
        }
        Ok(closed)
    }

    pub fn get(&self, incident_id: &IncidentId) -> Result<Incident, IncidentError> {
        self.store
            .get_incident(incident_id)?
            .ok_or(IncidentError::NotFound)
    }

    fn with_incident<F>(&self, incident_id: &IncidentId, f: F) -> Result<Incident, IncidentError>
    where
        F: FnOnce(&mut Incident) -> Result<(), IncidentError>,
    {
        let _mutation_guard = self
            .mutation_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut incident = self.get(incident_id)?;
        let original_len = incident.timeline.len();
        f(&mut incident)?;
        self.store.save_incident(&incident, original_len)?;
        Ok(incident)
    }

    pub fn request_approval(
        &self,
        incident_id: &IncidentId,
        at: SystemTime,
        note: Option<String>,
    ) -> Result<Incident, IncidentError> {
        self.with_incident(incident_id, |i| {
            i.apply_transition(IncidentTransition::RequestApproval, at, note)
        })
    }

    pub fn no_device_change_required(
        &self,
        incident_id: &IncidentId,
        plan: VerificationPlan,
        at: SystemTime,
        note: Option<String>,
    ) -> Result<Incident, IncidentError> {
        self.with_incident(incident_id, |i| {
            i.enter_verifying(IncidentTransition::NoDeviceChangeRequired, plan, at, note)
        })
    }

    pub fn approval_denied_or_expired(
        &self,
        incident_id: &IncidentId,
        at: SystemTime,
        note: Option<String>,
    ) -> Result<Incident, IncidentError> {
        self.with_incident(incident_id, |i| {
            i.apply_transition(IncidentTransition::ApprovalDeniedOrExpired, at, note)
        })
    }

    pub fn approval_granted(
        &self,
        incident_id: &IncidentId,
        at: SystemTime,
        note: Option<String>,
    ) -> Result<Incident, IncidentError> {
        self.with_incident(incident_id, |i| {
            i.apply_transition(IncidentTransition::ApprovalGranted, at, note)
        })
    }

    pub fn action_completed(
        &self,
        incident_id: &IncidentId,
        plan: VerificationPlan,
        at: SystemTime,
        note: Option<String>,
    ) -> Result<Incident, IncidentError> {
        self.with_incident(incident_id, |i| {
            i.enter_verifying(IncidentTransition::ActionCompleted, plan, at, note)
        })
    }

    pub fn action_failed_safely(
        &self,
        incident_id: &IncidentId,
        at: SystemTime,
        note: Option<String>,
    ) -> Result<Incident, IncidentError> {
        self.with_incident(incident_id, |i| {
            i.apply_transition(IncidentTransition::ActionFailedSafely, at, note)
        })
    }

    pub fn record_verification_check(
        &self,
        incident_id: &IncidentId,
        passed: bool,
        at: SystemTime,
        note: Option<String>,
    ) -> Result<Incident, IncidentError> {
        self.with_incident(incident_id, |i| {
            i.record_verification_check(passed, at, note)
        })
    }

    pub fn assign(
        &self,
        incident_id: &IncidentId,
        assignee: Assignee,
        at: SystemTime,
    ) -> Result<Incident, IncidentError> {
        self.with_incident(incident_id, |i| i.assign(assignee, at))
    }

    pub fn set_diagnosis_summary(
        &self,
        incident_id: &IncidentId,
        summary: impl Into<String>,
        at: SystemTime,
    ) -> Result<Incident, IncidentError> {
        let summary = summary.into();
        self.with_incident(incident_id, |i| i.set_diagnosis_summary(summary, at))
    }

    pub fn set_proposal_summary(
        &self,
        incident_id: &IncidentId,
        summary: impl Into<String>,
        at: SystemTime,
    ) -> Result<Incident, IncidentError> {
        let summary = summary.into();
        self.with_incident(incident_id, |i| i.set_proposal_summary(summary, at))
    }
}

/// `now - earlier`, treating `now` before `earlier` (clock skew) as zero
/// elapsed rather than an error — the same clock-skew handling
/// `crates/loki-monitoring/src/evaluate.rs`'s `elapsed_at_least`/
/// `remaining_cooldown` already document and use.
fn elapsed_since(now: SystemTime, earlier: SystemTime) -> Duration {
    now.duration_since(earlier).unwrap_or(Duration::ZERO)
}
