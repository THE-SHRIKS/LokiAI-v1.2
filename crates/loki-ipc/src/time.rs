//! UTC-millisecond deadline helpers.
//!
//! Deadlines travel on the wire as milliseconds since the Unix epoch (`u64`)
//! rather than a formatted timestamp: it is inherently UTC, trivially
//! `serde`-serializable as a plain integer, monotonic-comparable across
//! process boundaries on the same machine, and needs no date/time crate
//! (AGENTS.md rule 11 — this crate has no other use for one). This still
//! satisfies AGENTS.md rule 5's "use ... UTC timestamps" for per-call
//! deadlines: the wire value is an absolute point in time, not a
//! caller-relative "milliseconds from now" value that would need
//! re-basing if a call sat in a queue before being processed.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The current time as milliseconds since the Unix epoch (UTC).
///
/// Saturates to `0` if the system clock is somehow set before the epoch
/// rather than panicking — a clearly-wrong deadline (already-elapsed) is a
/// safe, honest degradation; a panic on a clock read is not.
pub fn unix_millis_now() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => u64::try_from(d.as_millis()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}

/// Computes `deadline_unix_millis + timeout` as an absolute deadline.
pub fn deadline_from_timeout(timeout: Duration) -> u64 {
    unix_millis_now().saturating_add(u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX))
}

/// The [`Duration`] remaining until `deadline_unix_millis`, saturating at
/// zero if the deadline has already passed (an already-elapsed deadline
/// must be treated as "expires immediately", not as an error or a negative
/// duration that would panic `Duration`'s constructor).
pub fn duration_until(deadline_unix_millis: u64) -> Duration {
    let now = unix_millis_now();
    Duration::from_millis(deadline_unix_millis.saturating_sub(now))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_millis_now_is_a_plausible_2026_timestamp() {
        // Sanity bound, not a precise check: 2026-01-01T00:00:00Z in millis.
        let jan_1_2026 = 1_767_225_600_000u64;
        assert!(unix_millis_now() > jan_1_2026);
    }

    #[test]
    fn deadline_from_timeout_is_in_the_future() {
        let now = unix_millis_now();
        let deadline = deadline_from_timeout(Duration::from_secs(5));
        assert!(deadline >= now + 4_900);
        assert!(deadline <= now + 5_100);
    }

    #[test]
    fn duration_until_future_deadline_is_positive() {
        let deadline = deadline_from_timeout(Duration::from_secs(10));
        let remaining = duration_until(deadline);
        assert!(remaining.as_millis() > 9_000);
        assert!(remaining.as_millis() <= 10_000);
    }

    #[test]
    fn duration_until_past_deadline_saturates_to_zero_not_panic() {
        let past = unix_millis_now().saturating_sub(60_000);
        assert_eq!(duration_until(past), Duration::from_millis(0));
    }

    #[test]
    fn duration_until_deadline_zero_saturates_to_zero() {
        // `0` is "the epoch", always in the past; must not underflow/panic.
        assert_eq!(duration_until(0), Duration::from_millis(0));
    }
}
