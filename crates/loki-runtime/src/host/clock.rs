//! Minimal, dependency-free UTC clock helpers for `src/host/*`.
//!
//! `crates/loki-storage` deliberately never reads a system clock itself
//! (`Docs/graph/handoffs/DAT-001-1.md` §7 / `loki-storage/src/timestamp.rs`)
//! — every timestamp it persists is caller-supplied. This node's host
//! composition (`src/host/compose.rs`) is exactly that caller, so it needs
//! one: a real UTC-ISO-8601 "now" for `Storage::run_migrations`, and a
//! plain Unix-milliseconds "now" for the desktop-discovery file
//! (`src/host/discovery.rs`), which is this crate's own JSON format, not a
//! `loki-storage`-validated field.
//!
//! Hand-rolled rather than adding a date/time crate (`chrono`, `time`, ...)
//! for two small conversions — the same `AGENTS.md` rule 11 stance
//! `loki-storage/src/timestamp.rs` documents for its own (structural-only)
//! timestamp validation.

use std::time::{SystemTime, UNIX_EPOCH};

/// The current time as Unix milliseconds. Never panics: a clock read
/// failure (only possible if the system clock is set before the Unix
/// epoch) falls back to `0` rather than unwrapping, since this value is
/// diagnostic (discovery-file bookkeeping), never a correctness-critical
/// input.
pub fn unix_millis_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// The current time as a UTC ISO 8601 string (`YYYY-MM-DDTHH:MM:SSZ`),
/// accepted by `loki_storage::Storage::run_migrations` and the repository
/// `*_at` fields.
pub fn now_utc_iso8601() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format_unix_seconds(elapsed.as_secs())
}

/// Formats a Unix-seconds timestamp as `YYYY-MM-DDTHH:MM:SSZ`. Split out of
/// [`now_utc_iso8601`] so the actual calendar math is testable against known
/// reference points without depending on the real wall clock.
fn format_unix_seconds(total_seconds: u64) -> String {
    let days = total_seconds / 86_400;
    let seconds_of_day = total_seconds % 86_400;
    let (year, month, day) = civil_from_days(i64::try_from(days).unwrap_or(i64::MAX));
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant's `civil_from_days` algorithm (public domain;
/// <http://howardhinnant.github.io/date_algorithms.html>): converts a day
/// count since the Unix epoch (1970-01-01) into a proleptic-Gregorian
/// `(year, month, day)` triple. Transcribed directly rather than adding a
/// calendar crate for one conversion.
fn civil_from_days(z_epoch: i64) -> (i64, u32, u32) {
    let z = z_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or(0); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = i64::try_from(yoe).unwrap_or(0) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1); // [1, 31]
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1); // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_formats_as_1970_01_01() {
        assert_eq!(format_unix_seconds(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn known_reference_point_2000_01_01() {
        // 10957 days from the epoch: 30 years including 7 leap years
        // (1972, 76, 80, 84, 88, 92, 96) = 30*365 + 7.
        assert_eq!(format_unix_seconds(10_957 * 86_400), "2000-01-01T00:00:00Z");
    }

    #[test]
    fn time_of_day_components_are_zero_padded() {
        assert_eq!(format_unix_seconds(3_661), "1970-01-01T01:01:01Z");
    }

    #[test]
    fn leap_day_2024_02_29_round_trips() {
        // Independently known: 2024-02-29 is 19782 days after the epoch.
        assert_eq!(format_unix_seconds(19_782 * 86_400), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn now_utc_iso8601_is_accepted_by_loki_storages_own_validator() {
        // Cross-crate honesty check: this crate's hand-rolled formatter must
        // actually satisfy `loki-storage`'s independent structural
        // validator, not just "look right" by inspection.
        let now = now_utc_iso8601();
        assert!(
            loki_storage::timestamp::validate_utc_timestamp("t", &now).is_ok(),
            "expected a valid UTC ISO 8601 timestamp, got {now}"
        );
    }

    #[test]
    fn unix_millis_now_is_plausibly_after_this_crate_was_written() {
        // 2026-01-01T00:00:00Z in Unix milliseconds — a loose sanity bound,
        // not a precise assertion, so this test does not flake or need
        // updating for ordinary clock drift.
        const WRITTEN_AFTER_MILLIS: u64 = 1_767_225_600_000;
        assert!(unix_millis_now() > WRITTEN_AFTER_MILLIS);
    }
}
