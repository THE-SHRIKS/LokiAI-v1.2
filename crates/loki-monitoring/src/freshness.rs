//! Freshness/quality-of-source tracking — `Docs/graph/tasks/MON-001.md`
//! Deliverable 2: "a stale data source is detectable, not silently treated
//! as current." Mirrors `Docs/SPEC.md` §4.3's device-level rule ("Device
//! status becomes `unknown` after its freshness window; it must not remain
//! healthy solely because the last sample was healthy") at the per-signal
//! telemetry level this crate owns.
//!
//! `Docs/SPEC.md` does not define a fixed freshness-window duration
//! anywhere — it is necessarily source/metric-specific (a heartbeat and a
//! slow environmental sensor do not share one sane window). This tracker
//! therefore never hardcodes one: every freshness query takes an explicit
//! `now` and `window`, supplied by the caller, rather than this crate
//! inventing a default (`AGENTS.md` rule 1 — do not fabricate an unspecified
//! product behavior).

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// The freshness state of one signal as of a given `now`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// A sample has been observed within `window` of `now`.
    Fresh,
    /// A sample has been observed, but not within `window` of `now`.
    Stale { elapsed: Duration },
    /// No sample has ever been observed for this signal — includes the
    /// state immediately after a process restart, before this
    /// in-process tracker has received anything again (this tracker itself
    /// is not restart-durable; see `Docs/graph/handoffs/MON-001-1.md`).
    /// Never conflated with `Stale`: a signal this tracker has never heard
    /// of is not honestly describable by an "elapsed since last sample"
    /// duration.
    Unknown,
}

impl Freshness {
    pub fn is_fresh(&self) -> bool {
        matches!(self, Freshness::Fresh)
    }
}

/// Tracks "when was this signal last observed" per signal key (see
/// [`crate::sample::TelemetrySample::signal_key`]) and answers freshness
/// queries against an explicit `now`/`window` — never a wall-clock read
/// inside this crate (same discipline `loki-storage`'s `src/timestamp.rs`
/// documents: determinism/testability, and not conflating "when this
/// tracker was asked" with an implicit clock dependency).
#[derive(Debug, Default)]
pub struct FreshnessTracker {
    last_seen: HashMap<String, SystemTime>,
}

impl FreshnessTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `signal_key` was observed at `observed_at`. Later
    /// observations only move the recorded time forward — an out-of-order
    /// (older) observation arriving after a newer one does not regress
    /// freshness, matching `Docs/SPEC.md` §3.5's "quality: late" concept
    /// (a late sample does not make the signal look fresher than its
    /// already-known newest data).
    pub fn observe(&mut self, signal_key: impl Into<String>, observed_at: SystemTime) {
        let key = signal_key.into();
        match self.last_seen.get(&key) {
            Some(existing) if *existing >= observed_at => {}
            _ => {
                self.last_seen.insert(key, observed_at);
            }
        }
    }

    /// The freshness of `signal_key` as of `now`, given `window`.
    pub fn assess(&self, signal_key: &str, now: SystemTime, window: Duration) -> Freshness {
        match self.last_seen.get(signal_key) {
            None => Freshness::Unknown,
            Some(last) => match now.duration_since(*last) {
                Ok(elapsed) if elapsed <= window => Freshness::Fresh,
                Ok(elapsed) => Freshness::Stale { elapsed },
                // `now` is before `last` (clock skew / an observation
                // timestamped in the future). Treat as fresh rather than
                // erroring: the signal is, at minimum, not stale.
                Err(_) => Freshness::Fresh,
            },
        }
    }

    /// True if this tracker has ever recorded an observation for
    /// `signal_key`, independent of how stale it now is.
    pub fn has_ever_observed(&self, signal_key: &str) -> bool {
        self.last_seen.contains_key(signal_key)
    }

    pub fn last_seen_at(&self, signal_key: &str) -> Option<SystemTime> {
        self.last_seen.get(signal_key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unobserved_signal_is_unknown_not_stale_or_fresh() {
        let tracker = FreshnessTracker::new();
        let now = SystemTime::now();
        assert_eq!(
            tracker.assess("dev-1::temp", now, Duration::from_secs(30)),
            Freshness::Unknown
        );
    }

    #[test]
    fn recently_observed_signal_is_fresh_within_window() {
        let mut tracker = FreshnessTracker::new();
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        tracker.observe("dev-1::temp", t0);

        let now = t0 + Duration::from_secs(10);
        assert!(
            tracker
                .assess("dev-1::temp", now, Duration::from_secs(30))
                .is_fresh()
        );
    }

    #[test]
    fn signal_becomes_stale_once_outside_its_window() {
        let mut tracker = FreshnessTracker::new();
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        tracker.observe("dev-1::temp", t0);

        let now = t0 + Duration::from_secs(61);
        match tracker.assess("dev-1::temp", now, Duration::from_secs(60)) {
            Freshness::Stale { elapsed } => assert_eq!(elapsed, Duration::from_secs(61)),
            other => panic!("expected Stale, got {other:?}"),
        }
    }

    #[test]
    fn a_healthy_last_sample_does_not_keep_a_signal_fresh_forever() {
        // Docs/SPEC.md §4.3: "Device status becomes unknown after its
        // freshness window; it must not remain healthy solely because the
        // last sample was healthy." — same principle at the signal level.
        let mut tracker = FreshnessTracker::new();
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        tracker.observe("dev-1::temp", t0);
        let stale = tracker.assess(
            "dev-1::temp",
            t0 + Duration::from_secs(3600),
            Duration::from_secs(60),
        );
        assert!(
            !stale.is_fresh(),
            "an hour-old sample must not still read as fresh under a 60s window"
        );
    }

    #[test]
    fn out_of_order_older_observation_does_not_regress_freshness() {
        let mut tracker = FreshnessTracker::new();
        let newer = SystemTime::UNIX_EPOCH + Duration::from_secs(2000);
        let older = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        tracker.observe("dev-1::temp", newer);
        tracker.observe("dev-1::temp", older); // arrives late, out of order
        assert_eq!(tracker.last_seen_at("dev-1::temp"), Some(newer));
    }

    #[test]
    fn has_ever_observed_distinguishes_unknown_from_never_seen() {
        let mut tracker = FreshnessTracker::new();
        assert!(!tracker.has_ever_observed("dev-1::temp"));
        tracker.observe("dev-1::temp", SystemTime::now());
        assert!(tracker.has_ever_observed("dev-1::temp"));
    }
}
