//! Sequence-gap/loss detection — `Docs/graph/tasks/MON-001.md` Deliverable
//! 5: "a gap/loss scenario is detectable and reported honestly (not
//! backfilled with fabricated data — `AGENTS.md` rule 1)". `Docs/SPEC.md`
//! §9: "Gaps and dropped samples are first-class events or quality states."
//!
//! This is a mechanical computation over `Docs/SPEC.md` §3.5's "sequence
//! where available" field, not an invented business rule: if a
//! monotonically increasing per-signal sequence number skips values, the
//! skipped range is missing data, full stop. No value is synthesized for
//! the missing range — a [`GapNotice`] only ever records that a gap
//! happened and how large it was.

use std::collections::HashMap;
use std::time::SystemTime;

/// One detected gap in a per-signal sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapNotice {
    pub signal_key: String,
    /// Last sequence number actually observed before the gap.
    pub last_seen_sequence: u64,
    /// The sequence number that resumed after the gap (the one that
    /// triggered detection).
    pub resumed_sequence: u64,
    /// Exact count of missing sequence numbers in
    /// `(last_seen_sequence, resumed_sequence)` — never estimated.
    pub missing_count: u64,
    pub detected_at: SystemTime,
}

/// Tracks the last-seen sequence number per signal and reports a
/// [`GapNotice`] whenever a new sequence number is not exactly one more
/// than the last one seen for that signal. Sequence numbers only ever move
/// forward for gap-detection purposes: an out-of-order/duplicate/older
/// sequence number is reported distinctly (never silently accepted as if it
/// were the next expected value, and never allowed to regress the detector
/// into re-reporting an already-closed gap).
#[derive(Debug, Default)]
pub struct SequenceGapDetector {
    last_seen: HashMap<String, u64>,
}

/// What [`SequenceGapDetector::observe`] concluded about one incoming
/// sequence number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceOutcome {
    /// First sequence number ever seen for this signal — nothing to compare
    /// against yet.
    FirstObservation,
    /// Exactly the next expected value. No gap.
    InOrder,
    /// A gap was detected and is reported in full.
    Gap(GapNotice),
    /// The incoming sequence number is not greater than the last one seen
    /// (duplicate or out-of-order/late arrival). Not itself a gap — but not
    /// silently treated as "in order" either, since it is neither.
    NotForward {
        last_seen_sequence: u64,
        received_sequence: u64,
    },
}

impl SequenceGapDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Observes `sequence` for `signal_key` at `now` (used only to
    /// timestamp any [`GapNotice`] produced — this crate does not read a
    /// system clock itself, per `AGENTS.md` rule 5/`loki-storage`'s
    /// established caller-supplied-timestamp discipline).
    pub fn observe(&mut self, signal_key: &str, sequence: u64, now: SystemTime) -> SequenceOutcome {
        match self.last_seen.get(signal_key).copied() {
            None => {
                self.last_seen.insert(signal_key.to_string(), sequence);
                SequenceOutcome::FirstObservation
            }
            Some(last) if sequence == last + 1 => {
                self.last_seen.insert(signal_key.to_string(), sequence);
                SequenceOutcome::InOrder
            }
            Some(last) if sequence > last + 1 => {
                let missing_count = sequence - last - 1;
                self.last_seen.insert(signal_key.to_string(), sequence);
                SequenceOutcome::Gap(GapNotice {
                    signal_key: signal_key.to_string(),
                    last_seen_sequence: last,
                    resumed_sequence: sequence,
                    missing_count,
                    detected_at: now,
                })
            }
            Some(last) => SequenceOutcome::NotForward {
                last_seen_sequence: last,
                received_sequence: sequence,
            },
        }
    }

    pub fn last_seen_sequence(&self, signal_key: &str) -> Option<u64> {
        self.last_seen.get(signal_key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_observation_reports_no_gap() {
        let mut d = SequenceGapDetector::new();
        assert_eq!(
            d.observe("dev-1::temp", 1, SystemTime::now()),
            SequenceOutcome::FirstObservation
        );
    }

    #[test]
    fn consecutive_sequences_report_in_order() {
        let mut d = SequenceGapDetector::new();
        d.observe("dev-1::temp", 1, SystemTime::now());
        assert_eq!(
            d.observe("dev-1::temp", 2, SystemTime::now()),
            SequenceOutcome::InOrder
        );
        assert_eq!(
            d.observe("dev-1::temp", 3, SystemTime::now()),
            SequenceOutcome::InOrder
        );
    }

    #[test]
    fn a_skip_reports_the_exact_missing_count_never_backfilled() {
        let mut d = SequenceGapDetector::new();
        let t0 = SystemTime::now();
        d.observe("dev-1::temp", 1, t0);
        d.observe("dev-1::temp", 2, t0);
        // Sequences 3 and 4 never arrive; 5 does.
        let outcome = d.observe("dev-1::temp", 5, t0);
        match outcome {
            SequenceOutcome::Gap(notice) => {
                assert_eq!(notice.last_seen_sequence, 2);
                assert_eq!(notice.resumed_sequence, 5);
                assert_eq!(notice.missing_count, 2, "exactly 3 and 4 are missing");
                assert_eq!(notice.signal_key, "dev-1::temp");
            }
            other => panic!("expected Gap, got {other:?}"),
        }
        // The detector must not have fabricated entries for 3/4 — its
        // notion of "last seen" moves straight to the resumed value.
        assert_eq!(d.last_seen_sequence("dev-1::temp"), Some(5));
    }

    #[test]
    fn duplicate_or_out_of_order_sequence_is_reported_distinctly_not_as_in_order() {
        let mut d = SequenceGapDetector::new();
        let t0 = SystemTime::now();
        d.observe("dev-1::temp", 5, t0);
        let outcome = d.observe("dev-1::temp", 3, t0); // late/out of order
        assert_eq!(
            outcome,
            SequenceOutcome::NotForward {
                last_seen_sequence: 5,
                received_sequence: 3,
            }
        );
        // Must not have regressed the tracked "last seen" value.
        assert_eq!(d.last_seen_sequence("dev-1::temp"), Some(5));

        let dup = d.observe("dev-1::temp", 5, t0);
        assert_eq!(
            dup,
            SequenceOutcome::NotForward {
                last_seen_sequence: 5,
                received_sequence: 5,
            }
        );
    }

    #[test]
    fn distinct_signals_track_independent_sequences() {
        let mut d = SequenceGapDetector::new();
        let t0 = SystemTime::now();
        d.observe("dev-1::temp", 1, t0);
        d.observe("dev-2::temp", 1, t0);
        assert_eq!(d.observe("dev-1::temp", 2, t0), SequenceOutcome::InOrder);
        // dev-2 restarting its own count at 2 is still in order for it,
        // independent of dev-1's state.
        assert_eq!(d.observe("dev-2::temp", 2, t0), SequenceOutcome::InOrder);
    }
}
