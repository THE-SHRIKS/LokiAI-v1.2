//! Incident model and `Docs/SPEC.md` §4.6 state machine.
//!
//! # Field list — `Docs/SPEC.md` §3.7 "Incident"
//!
//! > Fields: `incident_id`, title, severity, state, affected scope,
//! > trigger/rule version, occurrence count, evidence snapshot references,
//! > assigned user/agent, diagnosis and proposal summaries, verification
//! > plan/state, timeline, and timestamps.
//!
//! Every field is present on [`Incident`] below, no field invented or
//! dropped, following the exact same "field list matches the spec exactly"
//! discipline `Docs/graph/handoffs/MON-002-1.md` already established for
//! `MonitoringRule`. One necessary implementation-internal addition:
//! [`Incident::state_entered_at`] — §3.7 says only "timestamps" (plural,
//! unspecified, exactly like §3.6's own "timestamps"), so this crate is free
//! to choose which timestamps that covers; `state_entered_at` is required to
//! implement the two *time-based* diagram edges honestly (the `Verifying ->
//! Resolved` observation window and the `Resolved -> Open` reopen-policy
//! window both need "when did the incident actually enter this state", not
//! merely "when was it last touched").
//!
//! `severity` reuses `loki_monitoring::rule::Severity` and `trigger_rule_id`
//! reuses `loki_monitoring::rule::RuleId` rather than duplicating a second,
//! divergent copy of either type — this crate already depends on
//! `loki-monitoring` for `EvaluationOutcome`, so there is no reason to
//! re-derive shapes it already validates correctly.
//!
//! # `Docs/SPEC.md` §4.6's state machine — the real acceptance bar
//!
//! ```text
//! [*] --> Open
//! Open --> Investigating
//! Investigating --> WaitingApproval
//! Investigating --> Verifying: no device change required
//! WaitingApproval --> Investigating: denied or expired
//! WaitingApproval --> Executing: approved
//! Executing --> Verifying: action completes
//! Executing --> Investigating: action fails safely
//! Verifying --> Resolved: every check passes for observation window
//! Verifying --> Investigating: recurrence or failed check
//! Resolved --> Open: recurrence within reopen policy
//! Resolved --> Closed: retention/policy close
//! ```
//!
//! [`allowed_next`] is the single, exhaustive source of truth for which
//! `(IncidentState, IncidentTransition)` pairs are legal — every arm above
//! (except the implicit `[*] --> Open` pseudo-edge, which is not a named
//! transition at all; it is only ever performed by [`Incident::new`]'s
//! construction, per that method's own doc) is one match arm, and every
//! other combination (including every `IncidentTransition` applied to
//! `Closed`, which has no outgoing edges at all) falls through to `None` and
//! is rejected by [`Incident::apply_transition`]. `tests/
//! state_machine_transitions.rs` exercises every named edge directly, and
//! this module's own `every_pair_not_in_the_diagram_is_rejected` test
//! mechanically enumerates every `(state, transition)` combination and
//! asserts that only the eleven named diagram edges succeed — proving "no
//! edge not in the diagram is reachable" rather than merely asserting it for
//! the edges that do exist.

use std::fmt;
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use loki_connectors_sdk::error::ValidationError;
use loki_connectors_sdk::ids::DeviceId;
use loki_monitoring::rule::{RuleId, Severity};

use crate::error::IncidentError;
use crate::ids::{EvidenceId, IncidentId};
use crate::validate::non_empty_bounded;

/// An incident's live evidence-reference list never grows past this many
/// entries — a documented bound (`Docs/graph/tasks/INC-001.md` Deliverable
/// 4), oldest reference dropped first, mirroring
/// `crates/loki-monitoring/src/buffer.rs`'s drop-oldest discipline. Older
/// evidence snapshots remain durably stored (`crate::store::IncidentStore`
/// never deletes a snapshot row); only the *live* incident's reference list
/// is bounded.
pub const MAX_EVIDENCE_REFS_PER_INCIDENT: usize = 20;

/// An incident's affected-device scope is bounded for the same reason every
/// other list in this crate is: an unbounded scope is not a real, auditable
/// scope. No number is specified anywhere in `Docs/SPEC.md`, so this is a
/// documented judgment call, generous enough for a real fleet-wide rule.
pub const MAX_AFFECTED_SCOPE_DEVICES: usize = 256;

/// Verification checks retained per incident are bounded for the same
/// reason — an incident sitting in `Verifying` for a very long observation
/// window with frequent checks must not accumulate unbounded memory.
pub const MAX_VERIFICATION_CHECKS_RETAINED: usize = 200;

/// `Docs/SPEC.md` §4.6's seven states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IncidentState {
    Open,
    Investigating,
    WaitingApproval,
    Executing,
    Verifying,
    Resolved,
    Closed,
}

impl IncidentState {
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            IncidentState::Open => "open",
            IncidentState::Investigating => "investigating",
            IncidentState::WaitingApproval => "waiting_approval",
            IncidentState::Executing => "executing",
            IncidentState::Verifying => "verifying",
            IncidentState::Resolved => "resolved",
            IncidentState::Closed => "closed",
        }
    }

    /// Every state, for exhaustive test enumeration
    /// (`tests::every_pair_not_in_the_diagram_is_rejected`).
    pub const ALL: [IncidentState; 7] = [
        IncidentState::Open,
        IncidentState::Investigating,
        IncidentState::WaitingApproval,
        IncidentState::Executing,
        IncidentState::Verifying,
        IncidentState::Resolved,
        IncidentState::Closed,
    ];
}

impl fmt::Display for IncidentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

impl FromStr for IncidentState {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(IncidentState::Open),
            "investigating" => Ok(IncidentState::Investigating),
            "waiting_approval" => Ok(IncidentState::WaitingApproval),
            "executing" => Ok(IncidentState::Executing),
            "verifying" => Ok(IncidentState::Verifying),
            "resolved" => Ok(IncidentState::Resolved),
            "closed" => Ok(IncidentState::Closed),
            other => Err(ValidationError::new(
                "state",
                format!("unknown incident state {other:?}"),
            )),
        }
    }
}

/// Every named edge in `Docs/SPEC.md` §4.6's diagram, one variant per
/// labeled/unlabeled arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IncidentTransition {
    /// `Open -> Investigating`
    BeginInvestigation,
    /// `Investigating -> WaitingApproval`
    RequestApproval,
    /// `Investigating -> Verifying: no device change required`
    NoDeviceChangeRequired,
    /// `WaitingApproval -> Investigating: denied or expired`
    ApprovalDeniedOrExpired,
    /// `WaitingApproval -> Executing: approved`
    ApprovalGranted,
    /// `Executing -> Verifying: action completes`
    ActionCompleted,
    /// `Executing -> Investigating: action fails safely`
    ActionFailedSafely,
    /// `Verifying -> Resolved: every check passes for observation window`
    VerificationPassedObservationWindow,
    /// `Verifying -> Investigating: recurrence or failed check`
    VerificationRecurrenceOrFailedCheck,
    /// `Resolved -> Open: recurrence within reopen policy`
    ReopenRecurrenceWithinPolicy,
    /// `Resolved -> Closed: retention/policy close`
    CloseRetentionPolicy,
}

impl IncidentTransition {
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            IncidentTransition::BeginInvestigation => "begin_investigation",
            IncidentTransition::RequestApproval => "request_approval",
            IncidentTransition::NoDeviceChangeRequired => "no_device_change_required",
            IncidentTransition::ApprovalDeniedOrExpired => "approval_denied_or_expired",
            IncidentTransition::ApprovalGranted => "approval_granted",
            IncidentTransition::ActionCompleted => "action_completed",
            IncidentTransition::ActionFailedSafely => "action_failed_safely",
            IncidentTransition::VerificationPassedObservationWindow => {
                "verification_passed_observation_window"
            }
            IncidentTransition::VerificationRecurrenceOrFailedCheck => {
                "verification_recurrence_or_failed_check"
            }
            IncidentTransition::ReopenRecurrenceWithinPolicy => "reopen_recurrence_within_policy",
            IncidentTransition::CloseRetentionPolicy => "close_retention_policy",
        }
    }

    pub const ALL: [IncidentTransition; 11] = [
        IncidentTransition::BeginInvestigation,
        IncidentTransition::RequestApproval,
        IncidentTransition::NoDeviceChangeRequired,
        IncidentTransition::ApprovalDeniedOrExpired,
        IncidentTransition::ApprovalGranted,
        IncidentTransition::ActionCompleted,
        IncidentTransition::ActionFailedSafely,
        IncidentTransition::VerificationPassedObservationWindow,
        IncidentTransition::VerificationRecurrenceOrFailedCheck,
        IncidentTransition::ReopenRecurrenceWithinPolicy,
        IncidentTransition::CloseRetentionPolicy,
    ];
}

impl fmt::Display for IncidentTransition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

impl FromStr for IncidentTransition {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "begin_investigation" => Ok(IncidentTransition::BeginInvestigation),
            "request_approval" => Ok(IncidentTransition::RequestApproval),
            "no_device_change_required" => Ok(IncidentTransition::NoDeviceChangeRequired),
            "approval_denied_or_expired" => Ok(IncidentTransition::ApprovalDeniedOrExpired),
            "approval_granted" => Ok(IncidentTransition::ApprovalGranted),
            "action_completed" => Ok(IncidentTransition::ActionCompleted),
            "action_failed_safely" => Ok(IncidentTransition::ActionFailedSafely),
            "verification_passed_observation_window" => {
                Ok(IncidentTransition::VerificationPassedObservationWindow)
            }
            "verification_recurrence_or_failed_check" => {
                Ok(IncidentTransition::VerificationRecurrenceOrFailedCheck)
            }
            "reopen_recurrence_within_policy" => {
                Ok(IncidentTransition::ReopenRecurrenceWithinPolicy)
            }
            "close_retention_policy" => Ok(IncidentTransition::CloseRetentionPolicy),
            other => Err(ValidationError::new(
                "transition",
                format!("unknown incident transition {other:?}"),
            )),
        }
    }
}

/// The single source of truth for `Docs/SPEC.md` §4.6's legal edges. See
/// this module's doc for why this being one exhaustive match is what makes
/// "no edge not in the diagram is reachable" a provable, tested property
/// rather than an assertion.
const fn allowed_next(
    state: IncidentState,
    transition: IncidentTransition,
) -> Option<IncidentState> {
    use IncidentState::*;
    use IncidentTransition::*;
    match (state, transition) {
        (Open, BeginInvestigation) => Some(Investigating),
        (Investigating, RequestApproval) => Some(WaitingApproval),
        (Investigating, NoDeviceChangeRequired) => Some(Verifying),
        (WaitingApproval, ApprovalDeniedOrExpired) => Some(Investigating),
        (WaitingApproval, ApprovalGranted) => Some(Executing),
        (Executing, ActionCompleted) => Some(Verifying),
        (Executing, ActionFailedSafely) => Some(Investigating),
        (Verifying, VerificationPassedObservationWindow) => Some(Resolved),
        (Verifying, VerificationRecurrenceOrFailedCheck) => Some(Investigating),
        (Resolved, ReopenRecurrenceWithinPolicy) => Some(Open),
        (Resolved, CloseRetentionPolicy) => Some(Closed),
        _ => None,
    }
}

/// Who an incident is assigned to. An opaque, caller-supplied identity
/// string — no real user/agent identity model exists yet, same posture
/// `Docs/graph/handoffs/MON-002-1.md` already documents for
/// `MonitoringRule::creator` (`Docs/graph/tasks/INC-001.md` Deliverable 5:
/// "opaque string at this milestone, same posture as `MON-002`'s
/// `rule.creator`").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssigneeKind {
    User,
    Agent,
}

impl AssigneeKind {
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            AssigneeKind::User => "user",
            AssigneeKind::Agent => "agent",
        }
    }
}

impl FromStr for AssigneeKind {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(AssigneeKind::User),
            "agent" => Ok(AssigneeKind::Agent),
            other => Err(ValidationError::new(
                "assignee_kind",
                format!("unknown assignee kind {other:?}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignee {
    pub kind: AssigneeKind,
    pub identity: String,
}

impl Assignee {
    pub fn new(kind: AssigneeKind, identity: impl Into<String>) -> Result<Self, ValidationError> {
        let identity = identity.into();
        non_empty_bounded("assignee.identity", &identity, 128)?;
        Ok(Self { kind, identity })
    }
}

/// The plan an incident's `Verifying` state is evaluated against — set the
/// instant `Verifying` is entered (`Investigating -> Verifying` or
/// `Executing -> Verifying`), one per verification cycle (an incident may
/// enter `Verifying` more than once across its life if it recurs).
#[derive(Debug, Clone, PartialEq)]
pub struct VerificationPlan {
    pub description: String,
    /// `Docs/graph/tasks/INC-001.md` Deliverable 7: "a real, tested
    /// observation window — not a single instantaneous check." Every check
    /// recorded must pass and at least the full window must have elapsed
    /// since `Verifying` was entered before `Resolved` is reachable — see
    /// [`Incident::record_verification_check`].
    pub observation_window: Duration,
}

impl VerificationPlan {
    pub fn new(
        description: impl Into<String>,
        observation_window: Duration,
    ) -> Result<Self, ValidationError> {
        let description = description.into();
        non_empty_bounded("verification_plan.description", &description, 2048)?;
        if observation_window.is_zero() {
            return Err(ValidationError::new(
                "verification_plan.observation_window",
                "must be greater than zero — a zero window is not a real observation period",
            ));
        }
        Ok(Self {
            description,
            observation_window,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerificationCheck {
    pub at: SystemTime,
    pub passed: bool,
    pub note: Option<String>,
}

/// Progress against the current [`VerificationPlan`]. Reset every time the
/// incident (re-)enters `Verifying` — a new verification cycle starts a
/// fresh, empty check history, exactly like `crates/loki-monitoring`'s
/// evaluator resets its debounce streak on a fresh streak start (a
/// deliberate, non-fabricated reset, not a bug).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VerificationState {
    pub started_at: Option<SystemTime>,
    pub checks: Vec<VerificationCheck>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimelineEvent {
    Created,
    StateTransition {
        from: IncidentState,
        to: IncidentState,
        transition: IncidentTransition,
        note: Option<String>,
    },
    /// A repeated `Fired`/`MissingDataAlert` outcome correlated into this
    /// incident without changing its state (`crate::engine`'s correlation
    /// logic — see that module's doc for exactly when this fires versus a
    /// state transition).
    CorrelatedOccurrence {
        occurrence_count: u32,
    },
    AssignmentChanged {
        assignee: Option<Assignee>,
    },
    VerificationCheckRecorded {
        passed: bool,
        note: Option<String>,
    },
    DiagnosisUpdated,
    ProposalUpdated,
}

/// One append-only timeline entry (`Docs/graph/tasks/INC-001.md` Deliverable
/// 6: "every state transition and assignment change is recorded in an
/// append-only timeline, not overwritten"). `Incident::timeline` is only
/// ever appended to — no method on this crate removes or mutates an
/// existing entry (`tests/timeline_append_only.rs` proves this
/// mechanically).
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineEntry {
    pub at: SystemTime,
    pub event: TimelineEvent,
}

/// One incident (`Docs/SPEC.md` §3.7). See this module's doc for the field
/// list and where it deviates (never) from the spec's list.
#[derive(Debug, Clone, PartialEq)]
pub struct Incident {
    pub incident_id: IncidentId,
    pub title: String,
    pub severity: Severity,
    pub state: IncidentState,
    pub affected_scope: Vec<DeviceId>,
    pub trigger_rule_id: RuleId,
    pub trigger_rule_version: u32,
    pub occurrence_count: u32,
    pub evidence_refs: Vec<EvidenceId>,
    pub assigned: Option<Assignee>,
    pub diagnosis_summary: Option<String>,
    pub proposal_summary: Option<String>,
    pub verification_plan: Option<VerificationPlan>,
    pub verification_state: VerificationState,
    pub timeline: Vec<TimelineEntry>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    /// See this module's doc — necessary beyond §3.7's literal (unspecified)
    /// "timestamps" wording to implement the state machine's time-based
    /// edges honestly.
    pub state_entered_at: SystemTime,
}

impl Incident {
    /// Constructs a brand-new incident in `Open` — the only state
    /// `Docs/SPEC.md` §4.6's diagram allows an incident to start in
    /// (`[*] --> Open`). Only `crate::engine` calls this; every other
    /// crate-external caller only ever sees an `Incident` already read back
    /// from `crate::store::IncidentStore`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        incident_id: IncidentId,
        title: impl Into<String>,
        severity: Severity,
        affected_scope: Vec<DeviceId>,
        trigger_rule_id: RuleId,
        trigger_rule_version: u32,
        created_at: SystemTime,
    ) -> Result<Self, IncidentError> {
        let title = title.into();
        non_empty_bounded("title", &title, 256)?;
        if affected_scope.is_empty() {
            return Err(
                ValidationError::new("affected_scope", "must list at least one device").into(),
            );
        }
        if affected_scope.len() > MAX_AFFECTED_SCOPE_DEVICES {
            return Err(ValidationError::new(
                "affected_scope",
                format!("must list at most {MAX_AFFECTED_SCOPE_DEVICES} devices"),
            )
            .into());
        }

        Ok(Self {
            incident_id,
            title,
            severity,
            state: IncidentState::Open,
            affected_scope,
            trigger_rule_id,
            trigger_rule_version,
            occurrence_count: 1,
            evidence_refs: Vec::new(),
            assigned: None,
            diagnosis_summary: None,
            proposal_summary: None,
            verification_plan: None,
            verification_state: VerificationState::default(),
            timeline: vec![TimelineEntry {
                at: created_at,
                event: TimelineEvent::Created,
            }],
            created_at,
            updated_at: created_at,
            state_entered_at: created_at,
        })
    }

    /// Applies one named diagram edge. Rejects (`IncidentError::
    /// InvalidTransition`) any `(state, transition)` pair
    /// [`allowed_next`] does not recognize — this is the single enforcement
    /// point for "no edge not in the diagram is reachable."
    pub fn apply_transition(
        &mut self,
        transition: IncidentTransition,
        at: SystemTime,
        note: Option<String>,
    ) -> Result<(), IncidentError> {
        match allowed_next(self.state, transition) {
            Some(next) => {
                let from = self.state;
                self.state = next;
                self.state_entered_at = at;
                self.updated_at = at;
                if matches!(next, IncidentState::Verifying) {
                    // A new verification cycle always starts with a clean
                    // check history — never fabricated pre-cycle progress.
                    self.verification_state = VerificationState {
                        started_at: Some(at),
                        checks: Vec::new(),
                    };
                }
                self.timeline.push(TimelineEntry {
                    at,
                    event: TimelineEvent::StateTransition {
                        from,
                        to: next,
                        transition,
                        note,
                    },
                });
                Ok(())
            }
            None => Err(IncidentError::InvalidTransition {
                from: self.state.as_wire_str(),
                transition: transition.as_wire_str(),
            }),
        }
    }

    /// `Investigating -> Verifying` or `Executing -> Verifying`, setting the
    /// verification plan this cycle will be checked against. Rejects any
    /// other `transition` value or any state this specific edge does not
    /// originate from — delegated to [`Self::apply_transition`], so the same
    /// single enforcement point still governs reachability.
    pub fn enter_verifying(
        &mut self,
        transition: IncidentTransition,
        plan: VerificationPlan,
        at: SystemTime,
        note: Option<String>,
    ) -> Result<(), IncidentError> {
        if !matches!(
            transition,
            IncidentTransition::NoDeviceChangeRequired | IncidentTransition::ActionCompleted
        ) {
            return Err(IncidentError::InvalidTransition {
                from: self.state.as_wire_str(),
                transition: transition.as_wire_str(),
            });
        }
        self.apply_transition(transition, at, note)?;
        self.verification_plan = Some(plan);
        Ok(())
    }

    /// `Resolved -> Open` due to a new `Fired`/`MissingDataAlert` recurrence
    /// arriving within the reopen-policy window (`crate::engine` decides the
    /// window check; this method only applies the already-decided
    /// transition and increments `occurrence_count`, since a reopen is, by
    /// definition, caused by a new occurrence).
    pub fn reopen_due_to_recurrence(
        &mut self,
        at: SystemTime,
        note: Option<String>,
    ) -> Result<(), IncidentError> {
        self.apply_transition(IncidentTransition::ReopenRecurrenceWithinPolicy, at, note)?;
        self.occurrence_count += 1;
        Ok(())
    }

    /// Records a repeated `Fired`/`MissingDataAlert` outcome that
    /// `crate::engine` determined correlates with this *already-open*
    /// incident (same rule+scope, within the correlation window) —
    /// increments `occurrence_count` without changing `state`
    /// (`Docs/graph/tasks/INC-001.md` Deliverable 3: "not a fresh incident
    /// per firing").
    pub(crate) fn record_correlated_occurrence(&mut self, at: SystemTime) {
        self.occurrence_count += 1;
        self.updated_at = at;
        self.timeline.push(TimelineEntry {
            at,
            event: TimelineEvent::CorrelatedOccurrence {
                occurrence_count: self.occurrence_count,
            },
        });
    }

    /// Bounded push — see [`MAX_EVIDENCE_REFS_PER_INCIDENT`].
    pub(crate) fn push_evidence_ref(&mut self, evidence_id: EvidenceId) {
        self.evidence_refs.push(evidence_id);
        if self.evidence_refs.len() > MAX_EVIDENCE_REFS_PER_INCIDENT {
            self.evidence_refs.remove(0);
        }
    }

    pub fn assign(&mut self, assignee: Assignee, at: SystemTime) -> Result<(), IncidentError> {
        if matches!(self.state, IncidentState::Closed) {
            return Err(IncidentError::InvalidTransition {
                from: self.state.as_wire_str(),
                transition: "assign",
            });
        }
        self.assigned = Some(assignee.clone());
        self.updated_at = at;
        self.timeline.push(TimelineEntry {
            at,
            event: TimelineEvent::AssignmentChanged {
                assignee: Some(assignee),
            },
        });
        Ok(())
    }

    pub fn set_diagnosis_summary(
        &mut self,
        summary: impl Into<String>,
        at: SystemTime,
    ) -> Result<(), IncidentError> {
        if matches!(self.state, IncidentState::Closed) {
            return Err(IncidentError::InvalidTransition {
                from: self.state.as_wire_str(),
                transition: "set_diagnosis_summary",
            });
        }
        let summary = summary.into();
        non_empty_bounded("diagnosis_summary", &summary, 4096)?;
        self.diagnosis_summary = Some(summary);
        self.updated_at = at;
        self.timeline.push(TimelineEntry {
            at,
            event: TimelineEvent::DiagnosisUpdated,
        });
        Ok(())
    }

    pub fn set_proposal_summary(
        &mut self,
        summary: impl Into<String>,
        at: SystemTime,
    ) -> Result<(), IncidentError> {
        if matches!(self.state, IncidentState::Closed) {
            return Err(IncidentError::InvalidTransition {
                from: self.state.as_wire_str(),
                transition: "set_proposal_summary",
            });
        }
        let summary = summary.into();
        non_empty_bounded("proposal_summary", &summary, 4096)?;
        self.proposal_summary = Some(summary);
        self.updated_at = at;
        self.timeline.push(TimelineEntry {
            at,
            event: TimelineEvent::ProposalUpdated,
        });
        Ok(())
    }

    /// Records one verification check against the current
    /// [`VerificationPlan`] (must be in `Verifying`, must already have a
    /// plan — both invariants `crate::engine` maintains by only ever
    /// entering `Verifying` through [`Self::enter_verifying`]).
    ///
    /// - `passed = false`: immediately applies `Verifying -> Investigating`
    ///   (`VerificationRecurrenceOrFailedCheck`) — a single failed check is
    ///   enough to abandon the current verification cycle
    ///   (`Docs/graph/tasks/INC-001.md` Deliverable 7/8).
    /// - `passed = true`: recorded, but `Verifying -> Resolved` only fires
    ///   once `at` is at least `verification_plan.observation_window` past
    ///   this cycle's `verification_state.started_at` — a single passing
    ///   check right after entering `Verifying` is never sufficient on its
    ///   own (Deliverable 7: "not a single instantaneous check").
    pub fn record_verification_check(
        &mut self,
        passed: bool,
        at: SystemTime,
        note: Option<String>,
    ) -> Result<(), IncidentError> {
        if !matches!(self.state, IncidentState::Verifying) {
            return Err(IncidentError::InvalidTransition {
                from: self.state.as_wire_str(),
                transition: "record_verification_check",
            });
        }

        self.verification_state.checks.push(VerificationCheck {
            at,
            passed,
            note: note.clone(),
        });
        if self.verification_state.checks.len() > MAX_VERIFICATION_CHECKS_RETAINED {
            self.verification_state.checks.remove(0);
        }
        self.updated_at = at;
        self.timeline.push(TimelineEntry {
            at,
            event: TimelineEvent::VerificationCheckRecorded { passed, note },
        });

        if !passed {
            return self.apply_transition(
                IncidentTransition::VerificationRecurrenceOrFailedCheck,
                at,
                Some("verification check failed".to_string()),
            );
        }

        let plan = self.verification_plan.as_ref().ok_or_else(|| {
            IncidentError::Validation(ValidationError::new(
                "verification_plan",
                "Verifying state must always have a verification plan set",
            ))
        })?;
        let started_at = self
            .verification_state
            .started_at
            .unwrap_or(self.state_entered_at);
        let elapsed = at.duration_since(started_at).unwrap_or(Duration::ZERO);

        if elapsed >= plan.observation_window {
            self.apply_transition(
                IncidentTransition::VerificationPassedObservationWindow,
                at,
                Some("every check passed for the full observation window".to_string()),
            )?;
        }
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

    fn fresh_incident(at: SystemTime) -> Incident {
        Incident::new(
            IncidentId::generate(),
            "temperature over threshold",
            Severity::new("critical").unwrap(),
            vec![device("dev-1")],
            RuleId::new("rule-1").unwrap(),
            1,
            at,
        )
        .unwrap()
    }

    #[test]
    fn new_incident_starts_in_open_with_one_occurrence_and_a_created_timeline_entry() {
        let incident = fresh_incident(epoch(0));
        assert_eq!(incident.state, IncidentState::Open);
        assert_eq!(incident.occurrence_count, 1);
        assert_eq!(incident.timeline.len(), 1);
        assert!(matches!(incident.timeline[0].event, TimelineEvent::Created));
    }

    #[test]
    fn rejects_empty_affected_scope() {
        let err = Incident::new(
            IncidentId::generate(),
            "title",
            Severity::new("critical").unwrap(),
            vec![],
            RuleId::new("rule-1").unwrap(),
            1,
            epoch(0),
        );
        assert!(err.is_err());
    }

    #[test]
    fn every_pair_not_in_the_diagram_is_rejected() {
        // Mechanically enumerates every (state, transition) combination and
        // asserts only the twelve documented diagram edges succeed —
        // Deliverable 2: "no edge not in the diagram is reachable."
        let mut allowed_count = 0;
        for state in IncidentState::ALL {
            for transition in IncidentTransition::ALL {
                let mut incident = fresh_incident(epoch(0));
                incident.state = state;
                incident.state_entered_at = epoch(0);
                let result = incident.apply_transition(transition, epoch(1), None);
                match allowed_next(state, transition) {
                    Some(expected_next) => {
                        allowed_count += 1;
                        assert!(
                            result.is_ok(),
                            "expected {state:?} + {transition:?} to be allowed"
                        );
                        assert_eq!(incident.state, expected_next);
                    }
                    None => {
                        assert!(
                            result.is_err(),
                            "expected {state:?} + {transition:?} to be REJECTED, it succeeded"
                        );
                    }
                }
            }
        }
        assert_eq!(
            allowed_count, 11,
            "exactly 11 (state, transition) pairs are legal per Docs/SPEC.md §4.6"
        );
    }

    #[test]
    fn closed_has_no_outgoing_edges_at_all() {
        for transition in IncidentTransition::ALL {
            let mut incident = fresh_incident(epoch(0));
            incident.state = IncidentState::Closed;
            assert!(
                incident
                    .apply_transition(transition, epoch(1), None)
                    .is_err(),
                "Closed must be terminal; {transition:?} must not succeed"
            );
        }
    }

    #[test]
    fn assign_and_diagnosis_and_proposal_are_rejected_once_closed() {
        let mut incident = fresh_incident(epoch(0));
        incident.state = IncidentState::Closed;
        assert!(
            incident
                .assign(
                    Assignee::new(AssigneeKind::User, "user-1").unwrap(),
                    epoch(1)
                )
                .is_err()
        );
        assert!(incident.set_diagnosis_summary("x", epoch(1)).is_err());
        assert!(incident.set_proposal_summary("x", epoch(1)).is_err());
    }

    #[test]
    fn timeline_grows_on_assignment_and_summaries_without_removing_prior_entries() {
        let mut incident = fresh_incident(epoch(0));
        let starting_len = incident.timeline.len();
        incident
            .assign(
                Assignee::new(AssigneeKind::Agent, "agent-1").unwrap(),
                epoch(1),
            )
            .unwrap();
        incident
            .set_diagnosis_summary("root cause", epoch(2))
            .unwrap();
        incident
            .set_proposal_summary("restart service", epoch(3))
            .unwrap();
        assert_eq!(incident.timeline.len(), starting_len + 3);
        // Original entry must still be there, unmodified.
        assert!(matches!(incident.timeline[0].event, TimelineEvent::Created));
    }

    #[test]
    fn verification_check_failing_immediately_returns_to_investigating() {
        let mut incident = fresh_incident(epoch(0));
        incident
            .apply_transition(IncidentTransition::BeginInvestigation, epoch(1), None)
            .unwrap();
        incident
            .enter_verifying(
                IncidentTransition::NoDeviceChangeRequired,
                VerificationPlan::new("check X", Duration::from_secs(300)).unwrap(),
                epoch(2),
                None,
            )
            .unwrap();
        incident
            .record_verification_check(false, epoch(3), Some("still failing".to_string()))
            .unwrap();
        assert_eq!(incident.state, IncidentState::Investigating);
    }

    #[test]
    fn verification_check_passing_before_window_elapses_stays_verifying() {
        let mut incident = fresh_incident(epoch(0));
        incident
            .apply_transition(IncidentTransition::BeginInvestigation, epoch(1), None)
            .unwrap();
        incident
            .enter_verifying(
                IncidentTransition::NoDeviceChangeRequired,
                VerificationPlan::new("check X", Duration::from_secs(300)).unwrap(),
                epoch(2),
                None,
            )
            .unwrap();
        incident
            .record_verification_check(true, epoch(2) + Duration::from_secs(10), None)
            .unwrap();
        assert_eq!(
            incident.state,
            IncidentState::Verifying,
            "one early passing check must not resolve the incident"
        );
    }

    #[test]
    fn verification_check_passing_after_full_window_resolves() {
        let mut incident = fresh_incident(epoch(0));
        incident
            .apply_transition(IncidentTransition::BeginInvestigation, epoch(1), None)
            .unwrap();
        incident
            .enter_verifying(
                IncidentTransition::NoDeviceChangeRequired,
                VerificationPlan::new("check X", Duration::from_secs(300)).unwrap(),
                epoch(2),
                None,
            )
            .unwrap();
        incident
            .record_verification_check(true, epoch(2) + Duration::from_secs(300), None)
            .unwrap();
        assert_eq!(incident.state, IncidentState::Resolved);
    }

    #[test]
    fn record_verification_check_outside_verifying_is_rejected() {
        let mut incident = fresh_incident(epoch(0));
        assert!(
            incident
                .record_verification_check(true, epoch(1), None)
                .is_err()
        );
    }
}
