//! Monitoring rule model — `Docs/SPEC.md` §3.6 "Monitoring rule":
//!
//! > Fields: `rule_id`, version, workspace, scope selector, required signals,
//! > condition, window, debounce/cooldown, severity, enabled state,
//! > missing-data behavior, creator, and timestamps.
//!
//! `Docs/PRD.md` `FR-MON-002`: "Users shall configure enabled monitoring
//! rules with scope, condition, duration/window, severity, and required
//! signals."
//!
//! This module defines the field list exactly as §3.6 states it — no field
//! is invented or silently dropped (`Docs/graph/tasks/MON-002.md` Deliverable
//! 1). Two areas of this module are this node's own necessary design
//! decisions, since neither `Docs/SPEC.md` nor `Docs/PRD.md` defines a
//! syntax/vocabulary for them (the same situation `MON-001` documented for
//! its undefined freshness-window default and open `retention_class`/`level`
//! vocabularies — see `Docs/graph/handoffs/MON-001-1.md` §7):
//!
//! - **Condition syntax.** `Docs/SPEC.md` §3.6 says only "condition"; it does
//!   not define a grammar. This module implements one deterministic,
//!   auditable shape — a single-signal threshold comparison
//!   ([`Condition`]) — rather than an open-ended expression language. A
//!   richer condition grammar is a future extension, not a regression: this
//!   node's objective requires "deterministic evaluation only — no
//!   fuzzy/inferred triggering", which a fixed, exhaustively-matchable
//!   comparison satisfies without speculative scope growth.
//! - **`workspace` is an opaque, caller-supplied string, not a resolved
//!   identity.** No real workspace/device-authorization model exists yet in
//!   this codebase (`AUTH-001`/`ENR-001` are both `blocked` per
//!   `Docs/graph/TASK_GRAPH.md`). This node's own stop condition
//!   (`Docs/graph/tasks/MON-002.md` §7) requires reporting that gap rather
//!   than fabricating a workspace/device-authorization model. This module
//!   satisfies §3.6's field list by *carrying* the value (same "open,
//!   shape-validated string" precedent `crates/loki-monitoring/src/sample.rs`
//!   already establishes for `retention_class`/`level`, for the same
//!   reason: no documented closed vocabulary exists to validate against) —
//!   it does not resolve, authorize, or match against real workspace
//!   membership. See [`ScopeSelector`]'s doc for why *device* scoping does
//!   not have this same problem.

use std::fmt;
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use loki_connectors_sdk::error::ValidationError;
use loki_connectors_sdk::ids::DeviceId;

use crate::validate::non_empty_bounded;

/// Stable identifier for one monitoring rule (constant across every
/// [`MonitoringRule::version`] of that rule — see [`MonitoringRule`]'s doc
/// for the versioning contract).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleId(String);

impl RuleId {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        non_empty_bounded("rule_id", &value, 128)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which already-ingested devices a rule applies to.
///
/// This is a **mechanical, structural match against `DeviceId` values that
/// `crates/loki-monitoring`'s pipeline has already ingested samples for** —
/// it is not a device/workspace *authorization* decision. Whether a given
/// device is actually enrolled in, owned by, or permitted to report data
/// into a given workspace is a real identity/ownership question that belongs
/// to `ENR-001` (device enrollment/ownership) and `AUTH-001` (workspace
/// membership), both `blocked` today. This module deliberately does not
/// pretend to answer that question: [`ScopeSelector::matches`] only asks "is
/// this the device id (or one of the device ids) this rule's author listed",
/// exactly the same kind of plain identifier comparison
/// `crates/loki-connectors-sdk/src/ids.rs`'s `DeviceId` doc already
/// describes as in-scope for this codebase today ("distinct type at least
/// stops a raw transport-address `String` from being used ... by accident");
/// this is why this node's scope-selector tests are fully deterministic and
/// self-contained without needing a fabricated workspace/device-ownership
/// model, resolving `Docs/graph/tasks/MON-002.md` §7's stop condition rather
/// than triggering it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeSelector {
    /// Matches every device this pipeline has ingested a sample for.
    AllDevices,
    /// Matches exactly the listed device ids and no others.
    Devices(Vec<DeviceId>),
}

impl ScopeSelector {
    pub fn matches(&self, device: &DeviceId) -> bool {
        match self {
            ScopeSelector::AllDevices => true,
            ScopeSelector::Devices(ids) => ids.contains(device),
        }
    }
}

/// `Docs/SPEC.md` §3.6 "severity" — no closed vocabulary is documented
/// anywhere (unlike sample `quality`, which is closed) so this follows the
/// same "open, shape-validated string" precedent
/// `crates/loki-monitoring/src/sample.rs` already uses for `retention_class`
/// and log `level`, rather than inventing a fabricated closed enum.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Severity(String);

impl Severity {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        non_empty_bounded("severity", &value, 32)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A rule's explicit, required-to-be-declared behavior when one of its
/// [`MonitoringRule::required_signals`] is stale or has never been observed
/// (per `crate::freshness::FreshnessTracker`). `Docs/graph/tasks/MON-002.md`
/// Deliverable 4: "never silently defaulting to one without the rule saying
/// so" — this is a mandatory field on [`MonitoringRule`] (no `Option`, no
/// `Default` impl), so every rule author must pick one explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MissingDataBehavior {
    /// Treat a stale/absent required signal as if its condition were met.
    TreatAsMet,
    /// Treat a stale/absent required signal as if its condition were not
    /// met (the safer default for most alerting use cases, but never
    /// silently assumed — see this type's doc).
    TreatAsNotMet,
    /// Do not evaluate the condition at all while data is missing; instead
    /// report the rule's own distinct missing-data alert
    /// (`crate::evaluate::EvaluationOutcome::MissingDataAlert`).
    FireMissingDataAlert,
}

impl MissingDataBehavior {
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            MissingDataBehavior::TreatAsMet => "treat_as_met",
            MissingDataBehavior::TreatAsNotMet => "treat_as_not_met",
            MissingDataBehavior::FireMissingDataAlert => "fire_missing_data_alert",
        }
    }
}

impl fmt::Display for MissingDataBehavior {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

impl FromStr for MissingDataBehavior {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "treat_as_met" => Ok(MissingDataBehavior::TreatAsMet),
            "treat_as_not_met" => Ok(MissingDataBehavior::TreatAsNotMet),
            "fire_missing_data_alert" => Ok(MissingDataBehavior::FireMissingDataAlert),
            other => Err(ValidationError::new(
                "missing_data_behavior",
                format!("unknown missing-data behavior {other:?}"),
            )),
        }
    }
}

/// A single-signal threshold comparison. See this module's doc for why this
/// is the condition grammar this node implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOperator {
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Equal,
    NotEqual,
}

impl ComparisonOperator {
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            ComparisonOperator::GreaterThan => "gt",
            ComparisonOperator::GreaterThanOrEqual => "gte",
            ComparisonOperator::LessThan => "lt",
            ComparisonOperator::LessThanOrEqual => "lte",
            ComparisonOperator::Equal => "eq",
            ComparisonOperator::NotEqual => "neq",
        }
    }

    /// Evaluates `lhs <op> rhs`. Equality/inequality on `f64` is exact
    /// (bitwise `==`/`!=`), never a fabricated epsilon tolerance this crate
    /// does not document — a condition author who needs tolerance uses
    /// `GreaterThanOrEqual`/`LessThanOrEqual` with their own threshold.
    pub fn evaluate(self, lhs: f64, rhs: f64) -> bool {
        match self {
            ComparisonOperator::GreaterThan => lhs > rhs,
            ComparisonOperator::GreaterThanOrEqual => lhs >= rhs,
            ComparisonOperator::LessThan => lhs < rhs,
            ComparisonOperator::LessThanOrEqual => lhs <= rhs,
            ComparisonOperator::Equal => lhs == rhs,
            ComparisonOperator::NotEqual => lhs != rhs,
        }
    }
}

impl fmt::Display for ComparisonOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

impl FromStr for ComparisonOperator {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gt" => Ok(ComparisonOperator::GreaterThan),
            "gte" => Ok(ComparisonOperator::GreaterThanOrEqual),
            "lt" => Ok(ComparisonOperator::LessThan),
            "lte" => Ok(ComparisonOperator::LessThanOrEqual),
            "eq" => Ok(ComparisonOperator::Equal),
            "neq" => Ok(ComparisonOperator::NotEqual),
            other => Err(ValidationError::new(
                "operator",
                format!("unknown comparison operator {other:?}"),
            )),
        }
    }
}

/// `Docs/SPEC.md` §3.6 "condition" — evaluated against the most recent
/// ingested sample for [`Condition::signal`] within a rule's `window` (see
/// `crate::evaluate`). `signal` must be one of the owning rule's
/// [`MonitoringRule::required_signals`] (`MonitoringRule::new` validates
/// this — a condition can never reference a signal the rule does not itself
/// declare as required, which would otherwise silently bypass missing-data
/// handling for that signal).
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub signal: String,
    pub operator: ComparisonOperator,
    pub threshold: f64,
}

/// One versioned monitoring rule (`Docs/SPEC.md` §3.6).
///
/// # Versioning contract
///
/// [`RuleId`] is stable across a rule's lifetime; [`MonitoringRule::version`]
/// increments every time the rule's definition changes. A `MonitoringRule`
/// value is otherwise immutable once constructed — there is no in-place
/// mutation API on this type. `crate::rule_store::RuleStore` persists each
/// version as its own immutable row and rejects a non-increasing version for
/// an existing `rule_id` (`Docs/graph/tasks/MON-002.md` Deliverable 5: "an
/// old evaluation record stays attributed to the rule version that actually
/// produced it").
#[derive(Debug, Clone, PartialEq)]
pub struct MonitoringRule {
    pub rule_id: RuleId,
    pub version: u32,
    pub workspace: String,
    pub scope: ScopeSelector,
    pub required_signals: Vec<String>,
    pub condition: Condition,
    pub window: Duration,
    pub debounce: Duration,
    pub cooldown: Duration,
    pub severity: Severity,
    pub enabled: bool,
    pub missing_data_behavior: MissingDataBehavior,
    pub creator: String,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

impl MonitoringRule {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rule_id: RuleId,
        version: u32,
        workspace: impl Into<String>,
        scope: ScopeSelector,
        required_signals: Vec<String>,
        condition: Condition,
        window: Duration,
        debounce: Duration,
        cooldown: Duration,
        severity: Severity,
        enabled: bool,
        missing_data_behavior: MissingDataBehavior,
        creator: impl Into<String>,
        created_at: SystemTime,
        updated_at: SystemTime,
    ) -> Result<Self, ValidationError> {
        if version == 0 {
            return Err(ValidationError::new(
                "version",
                "must be >= 1 (versions start at 1, never 0)",
            ));
        }
        let workspace = workspace.into();
        non_empty_bounded("workspace", &workspace, 128)?;
        let creator = creator.into();
        non_empty_bounded("creator", &creator, 128)?;

        if required_signals.is_empty() {
            return Err(ValidationError::new(
                "required_signals",
                "must declare at least one required signal",
            ));
        }
        for signal in &required_signals {
            non_empty_bounded("required_signals[]", signal, 128)?;
        }
        if !required_signals.contains(&condition.signal) {
            return Err(ValidationError::new(
                "condition.signal",
                "must be one of required_signals — a condition cannot reference an undeclared signal",
            ));
        }
        non_empty_bounded("condition.signal", &condition.signal, 128)?;

        if window.is_zero() {
            return Err(ValidationError::new("window", "must be greater than zero"));
        }

        if let ScopeSelector::Devices(ids) = &scope
            && ids.is_empty()
        {
            return Err(ValidationError::new(
                "scope",
                "Devices(..) selector must list at least one device id — use AllDevices to match every device",
            ));
        }

        Ok(Self {
            rule_id,
            version,
            workspace,
            scope,
            required_signals,
            condition,
            window,
            debounce,
            cooldown,
            severity,
            enabled,
            missing_data_behavior,
            creator,
            created_at,
            updated_at,
        })
    }

    /// The per-device signal key `crate::freshness::FreshnessTracker` and
    /// `crate::gap::SequenceGapDetector` track state under for one of this
    /// rule's `required_signals`, for a specific `device` — matches
    /// `crate::sample::TelemetrySample::signal_key`'s exact format
    /// (`"{source_id}::{metric_name}"`), which is what `crate::pipeline`
    /// actually feeds those trackers.
    pub fn signal_key_for(device: &DeviceId, metric_name: &str) -> String {
        format!("{}::{}", device.as_str(), metric_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str) -> DeviceId {
        DeviceId::new(id).unwrap()
    }

    fn base_condition() -> Condition {
        Condition {
            signal: "temperature_c".to_string(),
            operator: ComparisonOperator::GreaterThan,
            threshold: 80.0,
        }
    }

    fn make_rule(version: u32) -> MonitoringRule {
        MonitoringRule::new(
            RuleId::new("rule-1").unwrap(),
            version,
            "workspace-1",
            ScopeSelector::AllDevices,
            vec!["temperature_c".to_string()],
            base_condition(),
            Duration::from_secs(60),
            Duration::from_secs(30),
            Duration::from_secs(300),
            Severity::new("critical").unwrap(),
            true,
            MissingDataBehavior::TreatAsNotMet,
            "user-1",
            SystemTime::UNIX_EPOCH,
            SystemTime::UNIX_EPOCH,
        )
        .unwrap()
    }

    #[test]
    fn rule_id_rejects_empty() {
        assert!(RuleId::new("").is_err());
        assert!(RuleId::new("rule-1").is_ok());
    }

    #[test]
    fn version_zero_is_rejected() {
        let err = MonitoringRule::new(
            RuleId::new("rule-1").unwrap(),
            0,
            "workspace-1",
            ScopeSelector::AllDevices,
            vec!["temperature_c".to_string()],
            base_condition(),
            Duration::from_secs(60),
            Duration::from_secs(30),
            Duration::from_secs(300),
            Severity::new("critical").unwrap(),
            true,
            MissingDataBehavior::TreatAsNotMet,
            "user-1",
            SystemTime::UNIX_EPOCH,
            SystemTime::UNIX_EPOCH,
        );
        assert!(err.is_err());
    }

    #[test]
    fn condition_signal_must_be_a_required_signal() {
        let mut condition = base_condition();
        condition.signal = "humidity".to_string();
        let err = MonitoringRule::new(
            RuleId::new("rule-1").unwrap(),
            1,
            "workspace-1",
            ScopeSelector::AllDevices,
            vec!["temperature_c".to_string()],
            condition,
            Duration::from_secs(60),
            Duration::from_secs(30),
            Duration::from_secs(300),
            Severity::new("critical").unwrap(),
            true,
            MissingDataBehavior::TreatAsNotMet,
            "user-1",
            SystemTime::UNIX_EPOCH,
            SystemTime::UNIX_EPOCH,
        );
        assert!(err.is_err());
    }

    #[test]
    fn zero_window_is_rejected() {
        let err = MonitoringRule::new(
            RuleId::new("rule-1").unwrap(),
            1,
            "workspace-1",
            ScopeSelector::AllDevices,
            vec!["temperature_c".to_string()],
            base_condition(),
            Duration::ZERO,
            Duration::from_secs(30),
            Duration::from_secs(300),
            Severity::new("critical").unwrap(),
            true,
            MissingDataBehavior::TreatAsNotMet,
            "user-1",
            SystemTime::UNIX_EPOCH,
            SystemTime::UNIX_EPOCH,
        );
        assert!(err.is_err());
    }

    #[test]
    fn empty_devices_selector_is_rejected_in_favor_of_all_devices() {
        let err = MonitoringRule::new(
            RuleId::new("rule-1").unwrap(),
            1,
            "workspace-1",
            ScopeSelector::Devices(vec![]),
            vec!["temperature_c".to_string()],
            base_condition(),
            Duration::from_secs(60),
            Duration::from_secs(30),
            Duration::from_secs(300),
            Severity::new("critical").unwrap(),
            true,
            MissingDataBehavior::TreatAsNotMet,
            "user-1",
            SystemTime::UNIX_EPOCH,
            SystemTime::UNIX_EPOCH,
        );
        assert!(err.is_err());
    }

    #[test]
    fn scope_selector_all_devices_matches_anything() {
        let scope = ScopeSelector::AllDevices;
        assert!(scope.matches(&device("dev-1")));
        assert!(scope.matches(&device("dev-anything")));
    }

    #[test]
    fn scope_selector_devices_matches_only_listed_ids() {
        let scope = ScopeSelector::Devices(vec![device("dev-1"), device("dev-2")]);
        assert!(scope.matches(&device("dev-1")));
        assert!(scope.matches(&device("dev-2")));
        assert!(!scope.matches(&device("dev-3")));
    }

    #[test]
    fn missing_data_behavior_round_trips() {
        for behavior in [
            MissingDataBehavior::TreatAsMet,
            MissingDataBehavior::TreatAsNotMet,
            MissingDataBehavior::FireMissingDataAlert,
        ] {
            assert_eq!(
                behavior
                    .as_wire_str()
                    .parse::<MissingDataBehavior>()
                    .unwrap(),
                behavior
            );
        }
        assert!("unknown".parse::<MissingDataBehavior>().is_err());
    }

    #[test]
    fn comparison_operator_round_trips_and_evaluates() {
        for op in [
            ComparisonOperator::GreaterThan,
            ComparisonOperator::GreaterThanOrEqual,
            ComparisonOperator::LessThan,
            ComparisonOperator::LessThanOrEqual,
            ComparisonOperator::Equal,
            ComparisonOperator::NotEqual,
        ] {
            assert_eq!(op.as_wire_str().parse::<ComparisonOperator>().unwrap(), op);
        }
        assert!(ComparisonOperator::GreaterThan.evaluate(10.0, 5.0));
        assert!(!ComparisonOperator::GreaterThan.evaluate(5.0, 10.0));
        assert!(ComparisonOperator::Equal.evaluate(5.0, 5.0));
        assert!(ComparisonOperator::NotEqual.evaluate(5.0, 6.0));
    }

    #[test]
    fn signal_key_for_matches_telemetry_sample_convention() {
        assert_eq!(
            MonitoringRule::signal_key_for(&device("dev-1"), "temperature_c"),
            "dev-1::temperature_c"
        );
    }

    #[test]
    fn a_rule_constructs_successfully_with_valid_fields() {
        let rule = make_rule(1);
        assert_eq!(rule.rule_id.as_str(), "rule-1");
        assert_eq!(rule.version, 1);
        assert!(rule.enabled);
    }
}
