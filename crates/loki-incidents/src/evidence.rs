//! Bounded evidence snapshots — `Docs/graph/tasks/INC-001.md` Deliverable 4:
//! "incidents reference bounded evidence snapshots (from `MON-002`'s
//! evaluation outcome and the underlying samples), never an unbounded/
//! unlimited evidence blob."
//!
//! An [`EvidenceSnapshot`] is captured once, at the moment a
//! `Fired`/`MissingDataAlert` outcome is ingested (`crate::engine`), from
//! whatever slice of already-ingested `loki_monitoring::sample::
//! TelemetrySample`s the caller supplies (the same slice the caller passed
//! to `RuleEvaluator::evaluate` to produce the outcome this snapshot is
//! evidence for). Two independent bounds apply, both documented here since
//! neither `Docs/SPEC.md` §3.7 nor §4.6 pins a number:
//!
//! - **Per-snapshot sample bound** ([`MAX_SAMPLES_PER_SNAPSHOT`]): a single
//!   snapshot never stores more than this many sample references, keeping
//!   the *most recent* ones by `source_time` if the caller supplies more —
//!   mirrors `crates/loki-monitoring/src/buffer.rs`'s drop-oldest discipline,
//!   applied once at construction time rather than as a live ring buffer.
//! - **Per-incident snapshot-reference bound** (`crate::incident::
//!   MAX_EVIDENCE_REFS_PER_INCIDENT`): an incident's *live* reference list
//!   never grows past that many entries either — see `crate::incident`'s
//!   doc for why that is a separate bound from this one.

use std::time::SystemTime;

use loki_connectors_sdk::ids::DeviceId;
use loki_monitoring::rule::RuleId;
use loki_monitoring::sample::TelemetrySample;

use crate::ids::EvidenceId;

/// The maximum number of sample references one [`EvidenceSnapshot`] retains,
/// regardless of how many samples the caller supplies to
/// [`EvidenceSnapshot::capture`]. A necessary, documented judgment call (no
/// number is specified anywhere in `Docs/SPEC.md`) chosen to be generous
/// enough for a human/agent to see real recent readings while remaining a
/// real, enforced, small bound rather than an unlimited blob.
pub const MAX_SAMPLES_PER_SNAPSHOT: usize = 20;

/// One bounded reference to an already-ingested sample's value, captured at
/// evidence-snapshot time (not a live pointer back into
/// `crates/loki-monitoring`'s own storage, which this crate does not have
/// write/read access to beyond what a caller passes in).
#[derive(Debug, Clone, PartialEq)]
pub struct SampleEvidenceRef {
    pub metric_name: String,
    pub value: f64,
    pub unit: String,
    pub source_time: SystemTime,
}

impl SampleEvidenceRef {
    fn from_sample(sample: &TelemetrySample) -> Self {
        Self {
            metric_name: sample.metric_name.clone(),
            value: sample.value.value,
            unit: sample.value.unit.clone(),
            source_time: sample.source_time,
        }
    }
}

/// A bounded, immutable evidence snapshot referencing the exact rule
/// version/device/outcome that produced it plus a bounded set of the
/// underlying samples. Never mutated after [`EvidenceSnapshot::capture`] —
/// evidence is a point-in-time record, not a live view.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceSnapshot {
    pub evidence_id: EvidenceId,
    pub rule_id: RuleId,
    pub rule_version: u32,
    pub device_id: DeviceId,
    /// `loki_monitoring::evaluate::EvaluationOutcome::kind_wire_str()` of the
    /// outcome this snapshot documents — `"fired"` or `"missing_data_alert"`
    /// for every snapshot this crate itself constructs (`crate::engine`
    /// rejects any other outcome kind before evidence is ever captured).
    pub outcome_kind: String,
    pub missing_signals: Vec<String>,
    /// Bounded to at most [`MAX_SAMPLES_PER_SNAPSHOT`] entries, most recent
    /// (`source_time`) first-dropped-oldest.
    pub samples: Vec<SampleEvidenceRef>,
    pub captured_at: SystemTime,
}

impl EvidenceSnapshot {
    /// Builds a bounded snapshot. `samples` may be any length — only the
    /// most recent [`MAX_SAMPLES_PER_SNAPSHOT`] (by `source_time`) are kept;
    /// this is proven, not merely typed, by
    /// `tests::capturing_more_than_the_bound_keeps_only_the_most_recent_samples`.
    pub fn capture(
        rule_id: RuleId,
        rule_version: u32,
        device_id: DeviceId,
        outcome_kind: impl Into<String>,
        missing_signals: Vec<String>,
        samples: &[TelemetrySample],
        captured_at: SystemTime,
    ) -> Self {
        let mut sorted: Vec<&TelemetrySample> = samples.iter().collect();
        sorted.sort_by_key(|s| s.source_time);
        let start = sorted.len().saturating_sub(MAX_SAMPLES_PER_SNAPSHOT);
        let bounded_samples = sorted[start..]
            .iter()
            .map(|s| SampleEvidenceRef::from_sample(s))
            .collect();

        Self {
            evidence_id: EvidenceId::generate(),
            rule_id,
            rule_version,
            device_id,
            outcome_kind: outcome_kind.into(),
            missing_signals,
            samples: bounded_samples,
            captured_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_monitoring::sample::{RetentionClass, SampleQuality};
    use std::time::Duration;

    fn device(id: &str) -> DeviceId {
        DeviceId::new(id).unwrap()
    }

    fn rule_id() -> RuleId {
        RuleId::new("rule-1").unwrap()
    }

    fn epoch(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
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
    fn capturing_fewer_than_the_bound_keeps_them_all() {
        let samples = vec![sample(epoch(1), 1.0), sample(epoch(2), 2.0)];
        let snap = EvidenceSnapshot::capture(
            rule_id(),
            1,
            device("dev-1"),
            "fired",
            vec![],
            &samples,
            epoch(10),
        );
        assert_eq!(snap.samples.len(), 2);
    }

    #[test]
    fn capturing_more_than_the_bound_keeps_only_the_most_recent_samples() {
        // 50 samples, well over MAX_SAMPLES_PER_SNAPSHOT, inserted out of
        // order to also prove sorting happens before bounding.
        let mut samples: Vec<TelemetrySample> =
            (0..50).map(|i| sample(epoch(i), i as f64)).collect();
        samples.reverse();

        let snap = EvidenceSnapshot::capture(
            rule_id(),
            1,
            device("dev-1"),
            "fired",
            vec![],
            &samples,
            epoch(1000),
        );

        assert_eq!(snap.samples.len(), MAX_SAMPLES_PER_SNAPSHOT);
        // Must be the 20 *most recent* (highest source_time / value), not an
        // arbitrary slice.
        let values: Vec<f64> = snap.samples.iter().map(|s| s.value).collect();
        let expected: Vec<f64> = (30..50).map(|i| i as f64).collect();
        assert_eq!(
            values, expected,
            "must keep the most recent samples, not drop them"
        );
    }

    #[test]
    fn evidence_id_is_freshly_generated_and_unique_per_snapshot() {
        let samples = vec![sample(epoch(1), 1.0)];
        let a = EvidenceSnapshot::capture(
            rule_id(),
            1,
            device("dev-1"),
            "fired",
            vec![],
            &samples,
            epoch(10),
        );
        let b = EvidenceSnapshot::capture(
            rule_id(),
            1,
            device("dev-1"),
            "fired",
            vec![],
            &samples,
            epoch(10),
        );
        assert_ne!(a.evidence_id, b.evidence_id);
    }
}
