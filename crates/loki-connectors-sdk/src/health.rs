//! Health report shapes — `Docs/CONNECTOR_SPEC.md` §7 "Health checks":
//! "Health report entries include: check ID/version and category; status
//! ...; observed value/unit and threshold/expectation where meaningful;
//! observed and received timestamp; evidence/reference and redaction class;
//! suggested next safe action."

use std::time::SystemTime;

use crate::error::ValidationError;
use crate::validate::non_empty_bounded;
use crate::vocabulary::HealthCheckStatus;

/// §7 "observed value/unit".
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedValue {
    pub value: f64,
    pub unit: String,
}

/// One health-report entry, §7.
#[derive(Debug, Clone, PartialEq)]
pub struct HealthCheckEntry {
    pub check_id: String,
    pub check_version: String,
    pub category: String,
    pub status: HealthCheckStatus,
    /// The "reason" §7 requires whenever a check does not simply pass:
    /// "[a connector] returns unavailable with reason." See
    /// [`HealthCheckStatus::requires_detail`].
    pub detail: Option<String>,
    pub observed_value: Option<ObservedValue>,
    pub threshold: Option<String>,
    pub observed_at: SystemTime,
    pub received_at: SystemTime,
    pub evidence_ref: Option<String>,
    pub suggested_next_action: Option<String>,
}

impl HealthCheckEntry {
    /// Enforces §7's honesty requirement at construction time: a
    /// `status` other than `Passed` must carry a non-empty `detail`. A
    /// connector that cannot explain *why* a check is not passing must
    /// report `Unavailable` with that explicit reason rather than silently
    /// omitting one — `AGENTS.md` rule 1 forbids exactly this kind of
    /// unexplained fabricated-looking state.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        check_id: impl Into<String>,
        check_version: impl Into<String>,
        category: impl Into<String>,
        status: HealthCheckStatus,
        detail: Option<String>,
        observed_at: SystemTime,
        received_at: SystemTime,
    ) -> Result<Self, ValidationError> {
        let check_id = check_id.into();
        non_empty_bounded("check_id", &check_id, 128)?;
        let check_version = check_version.into();
        non_empty_bounded("check_version", &check_version, 50)?;
        let category = category.into();
        non_empty_bounded("category", &category, 64)?;

        if status.requires_detail() {
            let has_detail = detail
                .as_deref()
                .map(|d| !d.trim().is_empty())
                .unwrap_or(false);
            if !has_detail {
                return Err(ValidationError::new(
                    "detail",
                    format!(
                        "must explain why when status is {status} (Docs/CONNECTOR_SPEC.md §7: \
                         a connector \"returns unavailable with reason\")"
                    ),
                ));
            }
        }

        Ok(Self {
            check_id,
            check_version,
            category,
            status,
            detail,
            observed_value: None,
            threshold: None,
            observed_at,
            received_at,
            evidence_ref: None,
            suggested_next_action: None,
        })
    }

    #[must_use]
    pub fn with_observed_value(mut self, value: f64, unit: impl Into<String>) -> Self {
        self.observed_value = Some(ObservedValue {
            value,
            unit: unit.into(),
        });
        self
    }

    #[must_use]
    pub fn with_threshold(mut self, threshold: impl Into<String>) -> Self {
        self.threshold = Some(threshold.into());
        self
    }

    #[must_use]
    pub fn with_evidence_ref(mut self, evidence_ref: impl Into<String>) -> Self {
        self.evidence_ref = Some(evidence_ref.into());
        self
    }

    #[must_use]
    pub fn with_suggested_next_action(mut self, action: impl Into<String>) -> Self {
        self.suggested_next_action = Some(action.into());
        self
    }
}

/// A connector's full health report for one `health_check` invocation.
#[derive(Debug, Clone, PartialEq)]
pub struct HealthReport {
    pub entries: Vec<HealthCheckEntry>,
    pub generated_at: SystemTime,
}

impl HealthReport {
    pub fn new(entries: Vec<HealthCheckEntry>, generated_at: SystemTime) -> Self {
        Self {
            entries,
            generated_at,
        }
    }

    pub fn empty(generated_at: SystemTime) -> Self {
        Self::new(Vec::new(), generated_at)
    }

    pub fn all_passed(&self) -> bool {
        self.entries
            .iter()
            .all(|e| e.status == HealthCheckStatus::Passed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passed_status_does_not_require_detail() {
        let entry = HealthCheckEntry::new(
            "power.voltage",
            "1",
            "power",
            HealthCheckStatus::Passed,
            None,
            SystemTime::now(),
            SystemTime::now(),
        );
        assert!(entry.is_ok());
    }

    #[test]
    fn unavailable_without_reason_is_rejected() {
        let entry = HealthCheckEntry::new(
            "power.voltage",
            "1",
            "power",
            HealthCheckStatus::Unavailable,
            None,
            SystemTime::now(),
            SystemTime::now(),
        );
        assert!(
            entry.is_err(),
            "Docs/CONNECTOR_SPEC.md §7 requires unavailable to carry a reason"
        );
    }

    #[test]
    fn unavailable_with_blank_reason_is_still_rejected() {
        let entry = HealthCheckEntry::new(
            "power.voltage",
            "1",
            "power",
            HealthCheckStatus::Unavailable,
            Some("   ".to_string()),
            SystemTime::now(),
            SystemTime::now(),
        );
        assert!(entry.is_err());
    }

    #[test]
    fn unavailable_with_real_reason_is_accepted() {
        let entry = HealthCheckEntry::new(
            "power.voltage",
            "1",
            "power",
            HealthCheckStatus::Unavailable,
            Some("no voltage sensor exposed by this board revision".to_string()),
            SystemTime::now(),
            SystemTime::now(),
        );
        assert!(entry.is_ok());
    }

    #[test]
    fn all_passed_is_true_only_when_every_entry_passed() {
        let now = SystemTime::now();
        let passed =
            HealthCheckEntry::new("a", "1", "cat", HealthCheckStatus::Passed, None, now, now)
                .unwrap();
        let failed = HealthCheckEntry::new(
            "b",
            "1",
            "cat",
            HealthCheckStatus::Failed,
            Some("boom".to_string()),
            now,
            now,
        )
        .unwrap();

        assert!(HealthReport::new(vec![passed.clone()], now).all_passed());
        assert!(!HealthReport::new(vec![passed, failed], now).all_passed());
        assert!(HealthReport::empty(now).all_passed());
    }
}
