//! Injected clock.
//!
//! `loki_storage`'s repositories deliberately never read a system clock —
//! every timestamp they persist is caller-supplied and validated at the
//! boundary (see `crates/loki-storage/src/timestamp.rs`'s doc comment:
//! determinism/testability, not conflating "when the DB write happened"
//! with the actual domain-event time). This crate's lease-expiry logic
//! genuinely needs "what time is it right now" as an internal decision (is
//! an existing lease still valid?), not only at a repository call boundary,
//! so it cannot get away with pure caller-supplied timestamps the way
//! `loki_storage` does. [`Clock`] is the seam that keeps that "what time is
//! it" question testable and deterministic rather than a hidden
//! `SystemTime::now()` call buried in lease/reconcile logic: production
//! code uses [`SystemClock`]; this crate's own tests (crash-mid-job races,
//! lease-expiry reclaim) inject a fake, advanceable clock instead of
//! sleeping in real wall-clock time.

use std::time::{SystemTime, UNIX_EPOCH};

/// The current instant, as milliseconds since the Unix epoch (UTC) and as a
/// UTC ISO 8601 string suitable for `loki_storage`'s
/// `created_at`/`updated_at` parameters.
pub trait Clock: Send + Sync {
    /// Milliseconds since the Unix epoch (UTC).
    fn now_millis(&self) -> u64;

    /// The same instant as [`Clock::now_millis`], rendered as a UTC ISO 8601
    /// timestamp (`Docs/SPEC.md` Section 1: "All timestamps are UTC ISO
    /// 8601") accepted by
    /// `loki_storage::timestamp::validate_utc_timestamp`.
    fn now_iso8601(&self) -> String {
        format_iso8601(self.now_millis())
    }
}

/// Reads the real OS clock. The only [`Clock`] implementation this crate
/// ships that a production caller (the future `RUN-002` consumer) should
/// use.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_millis(&self) -> u64 {
        // Saturates to `0` if the system clock is somehow set before the
        // epoch rather than panicking — matches
        // `crates/loki-ipc/src/time.rs`'s `unix_millis_now` convention: a
        // clearly-wrong "now" is a safe, honest degradation, not a panic
        // deep inside lease/reconcile logic.
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => u64::try_from(d.as_millis()).unwrap_or(u64::MAX),
            Err(_) => 0,
        }
    }
}

/// Renders `millis` (milliseconds since the Unix epoch, UTC) as
/// `YYYY-MM-DDTHH:MM:SS.mmmZ` — the shape
/// `loki_storage::timestamp::validate_utc_timestamp` accepts — without a
/// date/time crate dependency (`AGENTS.md` rule 11: no dependency merely for
/// convenience; this crate's only formatting need is this one function).
/// Uses the well-known "civil from days" integer algorithm for the
/// Gregorian calendar conversion (Howard Hinnant,
/// <http://howardhinnant.github.io/date_algorithms.html#civil_from_days>,
/// public domain).
pub fn format_iso8601(millis: u64) -> String {
    let total_seconds = (millis / 1000) as i64;
    let ms = millis % 1000;
    let days = total_seconds.div_euclid(86_400);
    let secs_of_day = total_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{ms:03}Z")
}

/// Days since the Unix epoch (1970-01-01) -> (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

#[cfg(test)]
pub(crate) mod fake {
    //! A deterministic, manually-advanceable [`Clock`] for this crate's own
    //! tests (crash/race/expiry scenarios must not depend on real
    //! wall-clock sleeps).

    use std::sync::atomic::{AtomicU64, Ordering};

    use super::Clock;

    pub(crate) struct FakeClock {
        millis: AtomicU64,
    }

    impl FakeClock {
        pub(crate) fn new(start_millis: u64) -> Self {
            Self {
                millis: AtomicU64::new(start_millis),
            }
        }

        pub(crate) fn advance(&self, delta_millis: u64) {
            self.millis.fetch_add(delta_millis, Ordering::SeqCst);
        }
    }

    impl Clock for FakeClock {
        fn now_millis(&self) -> u64 {
            self.millis.load(Ordering::SeqCst)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_unix_epoch() {
        assert_eq!(format_iso8601(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn formats_a_known_2026_instant() {
        // 2026-01-01T00:00:00.000Z, cross-checked against
        // crates/loki-ipc/src/time.rs's own "plausible 2026 timestamp"
        // constant (1_767_225_600_000).
        assert_eq!(
            format_iso8601(1_767_225_600_000),
            "2026-01-01T00:00:00.000Z"
        );
    }

    #[test]
    fn preserves_milliseconds() {
        assert_eq!(
            format_iso8601(1_767_225_600_123),
            "2026-01-01T00:00:00.123Z"
        );
    }

    #[test]
    fn output_passes_loki_storage_timestamp_validation() {
        let rendered = format_iso8601(1_767_225_661_500);
        assert!(loki_storage::timestamp::validate_utc_timestamp("t", &rendered).is_ok());
    }

    #[test]
    fn system_clock_now_millis_is_a_plausible_2026_timestamp() {
        let jan_1_2026 = 1_767_225_600_000u64;
        assert!(SystemClock.now_millis() > jan_1_2026);
    }

    #[test]
    fn system_clock_now_iso8601_round_trips_through_validation() {
        let rendered = SystemClock.now_iso8601();
        assert!(loki_storage::timestamp::validate_utc_timestamp("t", &rendered).is_ok());
    }

    #[test]
    fn fake_clock_advances_deterministically() {
        let clock = fake::FakeClock::new(1_000);
        assert_eq!(clock.now_millis(), 1_000);
        clock.advance(500);
        assert_eq!(clock.now_millis(), 1_500);
    }
}
