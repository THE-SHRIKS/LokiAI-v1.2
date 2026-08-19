//! Work-intent record — `Docs/SPEC.md` §3.13 "Notification and work intent":
//!
//! > Work-intent fields: `work_intent_id`, incident, requested capability,
//! > bounded evidence references, provider-neutral delivery requirements,
//! > expiry, delivery attempts, acknowledged agent run when present, and
//! > terminal outcome. A work intent is neither an approval nor an operation
//! > credential.
//!
//! This module defines the record shape exactly and `crate::engine`
//! constructs one every time a `Fired`/`MissingDataAlert` outcome drives an
//! incident into `Open`/`Investigating` (`Docs/graph/tasks/INC-001.md`
//! Deliverable 9). **This crate only emits the record** — delivering it to
//! any provider/agent is `AGT-002`'s scope (blocked on `AGT-001`/`NOT-001`
//! per `Docs/graph/TASK_GRAPH.md`), and this module's own fields make that
//! boundary mechanically obvious: `delivery_attempts` starts at (and never
//! leaves, in this crate) `0`, `acknowledged_agent_run` starts at (and never
//! leaves) `None`, `terminal_outcome` starts at (and never leaves) `None` —
//! nothing in this crate has the authority to advance any of the three, and
//! this crate exposes no method that tries.
//!
//! # `requested_capability`: reusing `CON-001`'s existing vocabulary
//!
//! `Docs/graph/tasks/INC-001.md` §7's stop condition requires `NEEDS_REPLAN`
//! "if the work-intent record's 'requested capability' field requires a real
//! capability/policy vocabulary that does not exist yet beyond what
//! `CON-001`'s manifest already defines." It does not: `CON-001` already
//! defines exactly this — `loki_connectors_sdk::ids::CapabilityId`, an open,
//! shape-validated capability slug (`Docs/CONNECTOR_SPEC.md` §6), reused here
//! unchanged rather than re-derived. This resolves the stop condition rather
//! than triggering it.
//!
//! # `delivery_requirements`: an open, documented field, not a fabricated one
//!
//! No real notification/agent-dispatch contract exists yet (`AGT-002`/
//! `NOT-001` both blocked), so — following the exact same "open,
//! shape-validated string, not a fabricated closed vocabulary" precedent
//! `Docs/graph/handoffs/MON-002-1.md` §7 already established for `Severity`/
//! `workspace` — [`DeliveryRequirements`] carries a free-form, bounded
//! description rather than a structured schema this node has no contract to
//! define.

use std::time::SystemTime;

use loki_connectors_sdk::ids::CapabilityId;

use crate::evidence::EvidenceSnapshot;
use crate::ids::{EvidenceId, IncidentId, WorkIntentId};
use crate::validate::non_empty_bounded;

/// The maximum number of evidence references one work intent carries — the
/// same bounded-evidence discipline `crate::evidence` documents, applied
/// here too so a work intent can never smuggle an unbounded evidence blob
/// even if a caller supplied more evidence snapshots than that bound.
pub const MAX_WORK_INTENT_EVIDENCE_REFS: usize = 20;

/// "Provider-neutral delivery requirements" — see this module's doc for why
/// this is an open, bounded string rather than a structured schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryRequirements {
    pub description: String,
}

impl DeliveryRequirements {
    pub fn new(description: impl Into<String>) -> Result<Self, crate::error::IncidentError> {
        let description = description.into();
        non_empty_bounded("delivery_requirements.description", &description, 2048)?;
        Ok(Self { description })
    }
}

/// A terminal outcome for a work intent's eventual delivery/acknowledgement
/// lifecycle. Defined for shape-completeness per `Docs/SPEC.md` §3.13 ("...
/// and terminal outcome"); this crate never itself produces a
/// [`WorkIntent`] whose `terminal_outcome` is anything but `None` — see this
/// module's doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkIntentOutcome {
    Delivered,
    Expired,
    Cancelled,
    Failed { reason: String },
}

/// One work-intent record (`Docs/SPEC.md` §3.13). See this module's doc for
/// the emission-only scope boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkIntent {
    pub work_intent_id: WorkIntentId,
    pub incident_id: IncidentId,
    pub requested_capability: CapabilityId,
    /// Bounded to at most [`MAX_WORK_INTENT_EVIDENCE_REFS`] entries.
    pub evidence_refs: Vec<EvidenceId>,
    pub delivery_requirements: DeliveryRequirements,
    pub expiry: SystemTime,
    /// Always `0` for a work intent this crate constructs — see this
    /// module's doc.
    pub delivery_attempts: u32,
    /// Always `None` for a work intent this crate constructs.
    pub acknowledged_agent_run: Option<String>,
    /// Always `None` for a work intent this crate constructs.
    pub terminal_outcome: Option<WorkIntentOutcome>,
    pub created_at: SystemTime,
}

impl WorkIntent {
    /// Constructs a fresh work-intent record at emission time.
    /// `evidence_snapshots` may be any length — only the most recently
    /// captured [`MAX_WORK_INTENT_EVIDENCE_REFS`] snapshot ids are kept
    /// (bounded evidence references, mirroring `crate::evidence`'s own
    /// per-snapshot bound).
    pub fn emit(
        incident_id: IncidentId,
        requested_capability: CapabilityId,
        evidence_snapshots: &[EvidenceSnapshot],
        delivery_requirements: DeliveryRequirements,
        expiry: SystemTime,
        created_at: SystemTime,
    ) -> Self {
        let mut sorted: Vec<&EvidenceSnapshot> = evidence_snapshots.iter().collect();
        sorted.sort_by_key(|e| e.captured_at);
        let start = sorted.len().saturating_sub(MAX_WORK_INTENT_EVIDENCE_REFS);
        let evidence_refs = sorted[start..]
            .iter()
            .map(|e| e.evidence_id.clone())
            .collect();

        Self {
            work_intent_id: WorkIntentId::generate(),
            incident_id,
            requested_capability,
            evidence_refs,
            delivery_requirements,
            expiry,
            delivery_attempts: 0,
            acknowledged_agent_run: None,
            terminal_outcome: None,
            created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_connectors_sdk::ids::DeviceId;
    use loki_monitoring::rule::RuleId;
    use std::time::Duration;

    fn epoch(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn evidence(at: SystemTime) -> EvidenceSnapshot {
        EvidenceSnapshot::capture(
            RuleId::new("rule-1").unwrap(),
            1,
            DeviceId::new("dev-1").unwrap(),
            "fired",
            vec![],
            &[],
            at,
        )
    }

    #[test]
    fn emitted_work_intent_has_untouched_delivery_fields() {
        let wi = WorkIntent::emit(
            IncidentId::generate(),
            CapabilityId::new("restart").unwrap(),
            &[evidence(epoch(1))],
            DeliveryRequirements::new("investigate and propose a fix").unwrap(),
            epoch(1000),
            epoch(1),
        );
        assert_eq!(wi.delivery_attempts, 0);
        assert_eq!(wi.acknowledged_agent_run, None);
        assert_eq!(wi.terminal_outcome, None);
        assert_eq!(wi.evidence_refs.len(), 1);
    }

    #[test]
    fn evidence_refs_are_bounded_to_the_most_recent() {
        let snapshots: Vec<EvidenceSnapshot> = (0..(MAX_WORK_INTENT_EVIDENCE_REFS + 10) as u64)
            .map(|i| evidence(epoch(i)))
            .collect();
        let wi = WorkIntent::emit(
            IncidentId::generate(),
            CapabilityId::new("read_health").unwrap(),
            &snapshots,
            DeliveryRequirements::new("investigate").unwrap(),
            epoch(10_000),
            epoch(1),
        );
        assert_eq!(wi.evidence_refs.len(), MAX_WORK_INTENT_EVIDENCE_REFS);
    }
}
