//! Health orchestration — this node's Deliverable 3: periodic health-check
//! invocation, health-status aggregation, and honest reflection of an
//! unhealthy connector (never silently masked as healthy, per `AGENTS.md`
//! rule 1).
//!
//! [`AggregatedHealthStatus`] is this crate's own type, not
//! `loki_connectors_sdk::vocabulary::HealthCheckStatus`: that SDK type
//! describes one *entry* in a `HealthReport` (§7's "status: passed, warning,
//! failed, unavailable, interrupted"); this module's job is to fold a whole
//! report's entries into one registry-level status per connector, which
//! `Docs/CONNECTOR_SPEC.md` does not itself define an aggregation rule for.
//! The rule below is this crate's own documented, conservative choice — see
//! [`AggregatedHealthStatus::from_report`] — not a divergent copy of
//! anything the spec already pins.

use std::time::SystemTime;

use loki_connectors_sdk::health::HealthReport;
use loki_connectors_sdk::vocabulary::HealthCheckStatus;

/// A whole connector's aggregated health, folded from its latest
/// [`HealthReport`]. Ordered worst-to-best is not implied by declaration
/// order here; see [`AggregatedHealthStatus::from_report`] for the actual
/// precedence rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AggregatedHealthStatus {
    /// At least one entry reported `Failed` — `Docs/CONNECTOR_SPEC.md` §7's
    /// clearest possible negative signal, so it takes precedence over every
    /// other entry status.
    Unhealthy,
    /// No entry `Failed`, but at least one reported `Warning` or
    /// `Interrupted` — a real problem was observed, short of an outright
    /// failure.
    Degraded,
    /// The report had at least one entry and every entry reported `Passed`.
    Healthy,
    /// The report was empty, or every entry reported `Unavailable` (no
    /// worse status observed) — `Docs/CONNECTOR_SPEC.md` §7: "[a connector]
    /// returns unavailable with reason" when it has no evidence source.
    /// This crate refuses to call that `Healthy`: an absence of evidence is
    /// not evidence of health (`AGENTS.md` rule 1 — never infer a state the
    /// data does not actually support).
    Unavailable,
}

impl AggregatedHealthStatus {
    /// The sole place this crate's report -> status folding rule is
    /// implemented, so `run_health_cycle`'s callers and this module's own
    /// tests can never see two different aggregations disagree about the
    /// same report.
    #[must_use]
    pub fn from_report(report: &HealthReport) -> Self {
        if report.entries.is_empty() {
            return AggregatedHealthStatus::Unavailable;
        }
        let mut saw_non_unavailable = false;
        for entry in &report.entries {
            match entry.status {
                HealthCheckStatus::Failed => return AggregatedHealthStatus::Unhealthy,
                HealthCheckStatus::Warning | HealthCheckStatus::Interrupted => {
                    saw_non_unavailable = true;
                }
                HealthCheckStatus::Passed => saw_non_unavailable = true,
                HealthCheckStatus::Unavailable => {}
            }
        }
        if !saw_non_unavailable {
            return AggregatedHealthStatus::Unavailable;
        }
        // No Failed entry. Degraded if anything other than a clean Passed
        // was observed alongside real evidence; otherwise every real entry
        // passed.
        let any_degraded = report.entries.iter().any(|e| {
            matches!(
                e.status,
                HealthCheckStatus::Warning | HealthCheckStatus::Interrupted
            )
        });
        if any_degraded {
            AggregatedHealthStatus::Degraded
        } else {
            AggregatedHealthStatus::Healthy
        }
    }
}

/// The registry's stored record of a connector's most recent health check —
/// both the folded [`AggregatedHealthStatus`] and the full underlying
/// [`HealthReport`], so a caller that needs the per-check detail (evidence,
/// suggested next action, ...) never has to re-derive it from the aggregate
/// alone.
#[derive(Debug, Clone, PartialEq)]
pub struct HealthStatusRecord {
    pub aggregated: AggregatedHealthStatus,
    pub report: HealthReport,
    pub checked_at: SystemTime,
}

impl HealthStatusRecord {
    #[must_use]
    pub fn from_report(report: HealthReport, checked_at: SystemTime) -> Self {
        let aggregated = AggregatedHealthStatus::from_report(&report);
        Self {
            aggregated,
            report,
            checked_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_connectors_sdk::health::HealthCheckEntry;

    fn entry(status: HealthCheckStatus, detail: Option<&str>) -> HealthCheckEntry {
        HealthCheckEntry::new(
            "check.x",
            "1",
            "category",
            status,
            detail.map(str::to_string),
            SystemTime::now(),
            SystemTime::now(),
        )
        .unwrap()
    }

    #[test]
    fn empty_report_is_unavailable_not_healthy() {
        let report = HealthReport::empty(SystemTime::now());
        assert_eq!(
            AggregatedHealthStatus::from_report(&report),
            AggregatedHealthStatus::Unavailable
        );
    }

    #[test]
    fn all_unavailable_entries_stay_unavailable() {
        let report = HealthReport::new(
            vec![
                entry(HealthCheckStatus::Unavailable, Some("no sensor")),
                entry(HealthCheckStatus::Unavailable, Some("no sensor 2")),
            ],
            SystemTime::now(),
        );
        assert_eq!(
            AggregatedHealthStatus::from_report(&report),
            AggregatedHealthStatus::Unavailable
        );
    }

    #[test]
    fn all_passed_is_healthy() {
        let report = HealthReport::new(
            vec![entry(HealthCheckStatus::Passed, None)],
            SystemTime::now(),
        );
        assert_eq!(
            AggregatedHealthStatus::from_report(&report),
            AggregatedHealthStatus::Healthy
        );
    }

    #[test]
    fn any_failed_entry_makes_the_whole_connector_unhealthy_even_with_passes() {
        let report = HealthReport::new(
            vec![
                entry(HealthCheckStatus::Passed, None),
                entry(HealthCheckStatus::Failed, Some("boom")),
            ],
            SystemTime::now(),
        );
        assert_eq!(
            AggregatedHealthStatus::from_report(&report),
            AggregatedHealthStatus::Unhealthy,
            "an unhealthy connector must never be masked by unrelated passing checks"
        );
    }

    #[test]
    fn warning_without_failure_is_degraded_not_healthy_or_unhealthy() {
        let report = HealthReport::new(
            vec![
                entry(HealthCheckStatus::Passed, None),
                entry(HealthCheckStatus::Warning, Some("borderline")),
            ],
            SystemTime::now(),
        );
        assert_eq!(
            AggregatedHealthStatus::from_report(&report),
            AggregatedHealthStatus::Degraded
        );
    }

    #[test]
    fn interrupted_entry_is_degraded() {
        let report = HealthReport::new(
            vec![entry(HealthCheckStatus::Interrupted, Some("cancelled"))],
            SystemTime::now(),
        );
        assert_eq!(
            AggregatedHealthStatus::from_report(&report),
            AggregatedHealthStatus::Degraded
        );
    }

    #[test]
    fn record_carries_the_full_report_alongside_the_aggregate() {
        let report = HealthReport::new(
            vec![entry(HealthCheckStatus::Failed, Some("boom"))],
            SystemTime::now(),
        );
        let record = HealthStatusRecord::from_report(report.clone(), SystemTime::now());
        assert_eq!(record.aggregated, AggregatedHealthStatus::Unhealthy);
        assert_eq!(record.report, report);
    }
}
