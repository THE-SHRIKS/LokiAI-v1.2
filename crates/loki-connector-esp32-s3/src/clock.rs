//! Renders the current instant as a UTC ISO 8601 string matching
//! `loki_connectors_sdk::identity`'s own `validate_utc` shape
//! (`YYYY-MM-DDTHH:MM:SS.mmmZ`), needed for
//! [`loki_connectors_sdk::identity::DeviceIdentityEvidenceInput::observed_at`].
//!
//! Deliberately no date/time crate dependency (`AGENTS.md` rule 11 — no
//! dependency merely for convenience; this crate's only formatting need is
//! this one function) — mirrors `crates/loki-jobs/src/clock.rs`'s and
//! `crates/loki-runtime/src/host/clock.rs`'s own identical, independently
//! hand-maintained copy of the same well-known "civil from days" integer
//! algorithm (Howard Hinnant,
//! <http://howardhinnant.github.io/date_algorithms.html#civil_from_days>,
//! public domain). No shared Rust time-formatting crate exists yet in this
//! workspace, so each crate that needs this keeps its own small, tested
//! copy rather than inventing a shared dependency for one function.

use std::time::{SystemTime, UNIX_EPOCH};

/// The current instant, rendered as a UTC ISO 8601 timestamp.
pub fn now_iso8601() -> String {
    format_iso8601(now_millis())
}

fn now_millis() -> u64 {
    // Saturates to `0` if the system clock is somehow set before the epoch
    // rather than panicking — a clearly-wrong "now" is a safe, honest
    // degradation, not a panic deep inside connector dispatch logic.
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => u64::try_from(d.as_millis()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}

fn format_iso8601(millis: u64) -> String {
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
mod tests {
    use super::*;

    #[test]
    fn formats_the_unix_epoch() {
        assert_eq!(format_iso8601(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn formats_a_known_2026_instant() {
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
    fn now_iso8601_produces_a_plausible_length_string() {
        let rendered = now_iso8601();
        assert!(rendered.ends_with('Z'));
        assert_eq!(rendered.len(), 24);
    }
}
