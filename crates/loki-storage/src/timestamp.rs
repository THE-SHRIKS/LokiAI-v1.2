//! UTC ISO 8601 timestamp validation mirroring
//! `packages/contracts/src/timestamp.ts`'s `IsoUtcTimestampSchema` —
//! `Docs/SPEC.md` Section 1: "All timestamps are UTC ISO 8601."
//!
//! This crate does not generate timestamps itself (no wall-clock
//! dependency): every timestamp value stored by a repository method is
//! supplied by the caller and validated here at the trust boundary
//! (`AGENTS.md` rule 4). This is a deliberate design choice, not an
//! oversight — see `Docs/graph/handoffs/DAT-001-1.md` §7 for the full
//! rationale: determinism/testability, not conflating "when the DB write
//! happened" with the actual domain-event time, and avoiding a date/time
//! formatting dependency purely for convenience (`AGENTS.md` rule 11).

use crate::error::StorageError;

/// Validates that `value` is a structurally well-formed UTC ISO 8601
/// timestamp (`YYYY-MM-DDTHH:MM:SS[.fraction]Z`) and returns a
/// [`StorageError::InvalidTimestamp`] naming `field` if not.
pub fn validate_utc_timestamp(field: &'static str, value: &str) -> Result<(), StorageError> {
    if is_valid_utc_timestamp(value) {
        Ok(())
    } else {
        Err(StorageError::InvalidTimestamp {
            field,
            value: value.to_string(),
        })
    }
}

/// Structural (not full-calendar) validity check. Bounds each component to
/// a sane numeric range (month 01-12, day 01-31, hour 00-23, minute 00-59,
/// second 00-60 to allow a leap second) without validating e.g.
/// day-of-month-for-that-specific-month — the same conservative "shape, not
/// full semantics" bound `packages/contracts/src/brand.ts` documents for
/// IDs, applied here to timestamps. This is a trust-boundary shape check,
/// not a general-purpose calendar library; adding a date/time crate solely
/// to validate a caller-supplied string would be exactly the kind of
/// convenience dependency `AGENTS.md` rule 11 warns against.
fn is_valid_utc_timestamp(value: &str) -> bool {
    // Minimum: "YYYY-MM-DDTHH:MM:SSZ" = 20 bytes.
    if value.len() < 20 || !value.is_ascii() {
        return false;
    }
    let Some(without_z) = value.strip_suffix('Z') else {
        // Only the 'Z' (UTC/Zulu) suffix is accepted — a numeric offset
        // such as "+05:30" is a valid ISO 8601 timestamp but not UTC.
        return false;
    };

    let Some((date, time)) = without_z.split_once('T') else {
        return false;
    };

    if !is_valid_date(date) {
        return false;
    }
    is_valid_time(time)
}

fn all_ascii_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn parses_in_range(s: &str, lo: u32, hi: u32) -> bool {
    all_ascii_digits(s) && s.parse::<u32>().is_ok_and(|n| (lo..=hi).contains(&n))
}

fn is_valid_date(date: &str) -> bool {
    let parts: Vec<&str> = date.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return false;
    };
    year.len() == 4
        && all_ascii_digits(year)
        && parses_in_range(month, 1, 12)
        && parses_in_range(day, 1, 31)
}

fn is_valid_time(time: &str) -> bool {
    let (main, fraction) = match time.split_once('.') {
        Some((main, fraction)) => (main, Some(fraction)),
        None => (time, None),
    };
    if let Some(fraction) = fraction
        && !all_ascii_digits(fraction)
    {
        return false;
    }

    let parts: Vec<&str> = main.split(':').collect();
    let [hour, minute, second] = parts.as_slice() else {
        return false;
    };
    // Second allows 60 for a leap second.
    parses_in_range(hour, 0, 23) && parses_in_range(minute, 0, 59) && parses_in_range(second, 0, 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_examples() {
        for v in [
            "2026-08-16T00:00:00Z",
            "2026-01-01T23:59:60Z",
            "2026-08-16T12:34:56.789Z",
            "2026-08-16T12:34:56.123456Z",
        ] {
            assert!(is_valid_utc_timestamp(v), "expected valid: {v}");
        }
    }

    #[test]
    fn rejects_missing_z() {
        assert!(!is_valid_utc_timestamp("2026-08-16T00:00:00"));
    }

    #[test]
    fn rejects_numeric_offset_form() {
        assert!(!is_valid_utc_timestamp("2026-08-16T00:00:00+05:30"));
    }

    #[test]
    fn rejects_garbage_and_out_of_range() {
        for v in [
            "",
            "not-a-timestamp",
            "2026-13-01T00:00:00Z",
            "2026-08-32T00:00:00Z",
            "2026-08-16T25:00:00Z",
            "2026-08-16T00:60:00Z",
            "2026/08/16T00:00:00Z",
            "2026-08-16 00:00:00Z",
        ] {
            assert!(!is_valid_utc_timestamp(v), "expected invalid: {v}");
        }
    }

    #[test]
    fn validate_utc_timestamp_reports_the_field_name() {
        match validate_utc_timestamp("last_seen_at", "garbage") {
            Err(StorageError::InvalidTimestamp { field, value }) => {
                assert_eq!(field, "last_seen_at");
                assert_eq!(value, "garbage");
            }
            other => panic!("expected InvalidTimestamp, got {other:?}"),
        }
    }
}
