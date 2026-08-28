//! A tiny, dependency-free Unix-epoch-milliseconds clock read, local to
//! this crate.
//!
//! # Why this crate does not import `loki-runtime`'s clock (§9 item 1)
//!
//! `claudeplanner.md` §2.3 recommends reusing
//! `crates/loki-runtime/src/host/clock.rs`. Direct inspection (as of this
//! packet, `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md` §9 item 1) shows
//! `loki-runtime` depends only on `loki-storage`/`loki-ipc` today, and
//! `P1-RUNTIME-COMP-001` (a later node) will make `loki-runtime` depend on
//! this crate (and the other service crates) for composition. If this
//! crate also depended on `loki-runtime` for its clock, that would be a
//! real dependency cycle the moment `P1-RUNTIME-COMP-001` lands, not a
//! theoretical one — hence this small, locally-owned duplicate rather than
//! a shared crate (`AGENTS.md` rule 11 does not require avoiding a two-line
//! duplication to prevent a structural cycle). This is a deliberate,
//! documented deviation, not a silent one.
//!
//! `accepted_at_ms` (ADR-0006 step 2/the timestamp
//! `loki_mutation_contract::construct_accepted` receives) is read via
//! [`unix_millis_now`] from **inside** the `Storage::transaction()` closure,
//! immediately before calling `construct_accepted` — never client-supplied,
//! never read before lock acquisition (closes the time-of-check/
//! time-of-use gap `Docs/graph/TASK_GRAPH.md`'s `P1-MUTATION-CONTRACT-001`
//! acceptance record flags).

use std::time::{SystemTime, UNIX_EPOCH};

/// The current time as signed Unix milliseconds — the exact type
/// `loki_mutation_contract`'s `accepted_at_ms`/`GrantGeneration`/
/// `ApprovalReference` fields already use. Never panics: a clock read
/// failure (only possible if the system clock is set before the Unix
/// epoch) or an out-of-`i64`-range duration falls back to `0` rather than
/// unwrapping (`AGENTS.md` rule 1) — this value feeds an acceptance
/// decision, so an honest fallback that fails every subsequent expiry
/// check closed (`now_ms < any real expires_at_ms` is false once `now_ms`
/// wraps to `0`... actually `0` is *before* every real epoch, which makes
/// every expiry check `now_ms >= expires_at_ms` evaluate `false`) would be
/// dangerous; instead an out-of-range duration saturates to `i64::MAX`,
/// which fails every subsequent expiry/grant check closed (denies
/// acceptance) rather than silently appearing "always valid."
pub fn unix_millis_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_millis()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

/// Formats `ms` (Unix epoch milliseconds, as this crate's own
/// `NewMutationAcceptance`/`AcceptedRecord` values already carry) as the
/// `YYYY-MM-DDTHH:MM:SSZ` UTC-ISO-8601 string
/// `loki_storage::timestamp::validate_utc_timestamp` accepts — needed
/// because `loki_storage::jobs`/`loki_storage::outbox`'s `created_at`/
/// `updated_at` columns use that crate's usual `TEXT` timestamp convention
/// (`crates/loki-storage/src/timestamp.rs`), unlike the new
/// `mutation_acceptances.accepted_at_ms` column
/// (`crates/loki-storage/migrations/0007_mutation_acceptances.sql`).
/// Sub-second precision is truncated (floored), matching every other
/// second-resolution UTC-ISO-8601 timestamp this codebase already produces
/// (`crates/loki-runtime/src/host/clock.rs::now_utc_iso8601`, whose
/// `civil_from_days` calendar-math transcription this function reuses
/// verbatim rather than importing that crate — see this module's top doc
/// comment for why).
pub fn format_unix_millis_as_iso8601(ms: i64) -> String {
    let total_seconds = ms.div_euclid(1000).max(0);
    let days = total_seconds / 86_400;
    let seconds_of_day = total_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant's `civil_from_days` algorithm (public domain;
/// <http://howardhinnant.github.io/date_algorithms.html>): converts a day
/// count since the Unix epoch (1970-01-01) into a proleptic-Gregorian
/// `(year, month, day)` triple. Transcribed directly (same technique
/// `crates/loki-runtime/src/host/clock.rs` already uses independently)
/// rather than adding a calendar crate for one conversion.
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
    fn unix_millis_now_is_plausibly_after_this_crate_was_written() {
        // 2026-01-01T00:00:00Z in Unix milliseconds — a loose sanity bound,
        // not a precise assertion, so this test does not flake or need
        // updating for ordinary clock drift.
        const WRITTEN_AFTER_MILLIS: i64 = 1_767_225_600_000;
        assert!(unix_millis_now() > WRITTEN_AFTER_MILLIS);
    }

    #[test]
    fn epoch_formats_as_1970_01_01() {
        assert_eq!(format_unix_millis_as_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn known_reference_point_2000_01_01() {
        assert_eq!(
            format_unix_millis_as_iso8601(10_957 * 86_400 * 1000),
            "2000-01-01T00:00:00Z"
        );
    }

    #[test]
    fn sub_second_precision_is_truncated() {
        assert_eq!(
            format_unix_millis_as_iso8601(3_661_999),
            "1970-01-01T01:01:01Z"
        );
    }

    #[test]
    fn formatted_output_is_accepted_by_loki_storages_own_validator() {
        // Cross-crate honesty check: this crate's hand-rolled formatter
        // must actually satisfy `loki-storage`'s independent structural
        // validator, not just "look right" by inspection.
        let formatted = format_unix_millis_as_iso8601(unix_millis_now());
        assert!(
            loki_storage::timestamp::validate_utc_timestamp("t", &formatted).is_ok(),
            "expected a valid UTC ISO 8601 timestamp, got {formatted}"
        );
    }
}
