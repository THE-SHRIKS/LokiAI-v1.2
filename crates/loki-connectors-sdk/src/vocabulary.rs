//! Closed enum vocabulary mirrored from `packages/contracts/src/connector.ts`
//! and `Docs/CONNECTOR_SPEC.md`.
//!
//! Per this node's task packet ("Mirror `packages/contracts`' connector
//! vocabulary ... as Rust types citing the same `Docs/CONNECTOR_SPEC.md`
//! sections, same 'no divergent copy' discipline as `DAT-001`"), every value
//! below is transcribed, not invented, and each type's wire string
//! ([`SupportTier::as_wire_str`] etc.) is identical byte-for-byte to the
//! lowercase string `connector.ts`'s `zod` schema accepts. No Rust
//! contracts/codegen crate exists yet (`Docs/graph/handoffs/CTR-001-1.md`
//! §7: "Rust codegen/crate consumption explicitly deferred to a fast-follow
//! once `RUN-001` lands"), so this is a hand-maintained mirror, the same
//! approach `DAT-001` uses for its own entity ID/state types — a future
//! shared Rust contracts crate should replace these with generated types
//! rather than this crate inventing a second, divergent generation scheme.
//!
//! A closed Rust `enum` is used only where the source gives a *definitive*
//! list. Where a source explicitly says a vocabulary "may include" more than
//! it enumerates (`Docs/CONNECTOR_SPEC.md` §6, capability ids), this crate
//! uses an open, shape-validated type instead — see [`crate::ids::CapabilityId`].
//! That is not an oversight; inventing a closed enum for an explicitly open
//! vocabulary would itself be a divergent copy.

use std::fmt;
use std::str::FromStr;

use crate::error::ValidationError;

/// `Docs/CONNECTOR_SPEC.md` §2 "Support levels" table (also used by
/// `Docs/SPEC.md` §3.3 Device "support tier"). Four values, given as a
/// closed table with no "may include" qualifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportTier {
    /// "LokiAI release tests the exact board/module, connector, toolchain,
    /// and required physical scenarios."
    Certified,
    /// "Connector recognizes a family/variant and passes contract tests, but
    /// not every physical revision is release-certified."
    Supported,
    /// "User/team supplies a connector or transport integration."
    Custom,
    /// "Candidate transport observed but no compatible connector/identity."
    Unsupported,
}

impl SupportTier {
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            SupportTier::Certified => "certified",
            SupportTier::Supported => "supported",
            SupportTier::Custom => "custom",
            SupportTier::Unsupported => "unsupported",
        }
    }
}

impl fmt::Display for SupportTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

impl FromStr for SupportTier {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "certified" => Ok(SupportTier::Certified),
            "supported" => Ok(SupportTier::Supported),
            "custom" => Ok(SupportTier::Custom),
            "unsupported" => Ok(SupportTier::Unsupported),
            other => Err(ValidationError::new(
                "support_tier",
                format!("unknown support tier {other:?} (Docs/CONNECTOR_SPEC.md §2)"),
            )),
        }
    }
}

/// `Docs/CONNECTOR_SPEC.md` §6 requires every capability to declare a
/// `risk`, but does not enumerate a fixed value set on its own. These three
/// values are lifted verbatim from `Docs/MCP_SPEC.md` §6's tool-risk
/// headings, exactly as `packages/contracts/src/connector.ts` documents
/// (`CAPABILITY_RISK_LEVELS`). A capability's own *declared* classification
/// — an input to policy, not the policy decision itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityRiskLevel {
    Low,
    Medium,
    High,
}

impl CapabilityRiskLevel {
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            CapabilityRiskLevel::Low => "low",
            CapabilityRiskLevel::Medium => "medium",
            CapabilityRiskLevel::High => "high",
        }
    }
}

impl fmt::Display for CapabilityRiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

impl FromStr for CapabilityRiskLevel {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(CapabilityRiskLevel::Low),
            "medium" => Ok(CapabilityRiskLevel::Medium),
            "high" => Ok(CapabilityRiskLevel::High),
            other => Err(ValidationError::new(
                "capability_risk_level",
                format!("unknown risk level {other:?} (Docs/CONNECTOR_SPEC.md §6)"),
            )),
        }
    }
}

/// `Docs/CONNECTOR_SPEC.md` §7: "status: passed, warning, failed,
/// unavailable, interrupted" — given as a definitive (not "may include")
/// list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HealthCheckStatus {
    Passed,
    Warning,
    Failed,
    Unavailable,
    Interrupted,
}

impl HealthCheckStatus {
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            HealthCheckStatus::Passed => "passed",
            HealthCheckStatus::Warning => "warning",
            HealthCheckStatus::Failed => "failed",
            HealthCheckStatus::Unavailable => "unavailable",
            HealthCheckStatus::Interrupted => "interrupted",
        }
    }

    /// `Docs/CONNECTOR_SPEC.md` §7: "A connector cannot infer voltage,
    /// thermal, memory, firmware, or bus health when it has no evidence
    /// source. It returns unavailable with reason." This crate extends the
    /// same honesty requirement to every non-`Passed` status, not only
    /// `Unavailable`: an unexplained warning/failure/interruption is exactly
    /// the kind of unexplained state `AGENTS.md` rule 1 forbids fabricating
    /// past. See [`crate::health::HealthCheckEntry::new`], which enforces
    /// this at construction time.
    pub const fn requires_detail(self) -> bool {
        !matches!(self, HealthCheckStatus::Passed)
    }
}

impl fmt::Display for HealthCheckStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

impl FromStr for HealthCheckStatus {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "passed" => Ok(HealthCheckStatus::Passed),
            "warning" => Ok(HealthCheckStatus::Warning),
            "failed" => Ok(HealthCheckStatus::Failed),
            "unavailable" => Ok(HealthCheckStatus::Unavailable),
            "interrupted" => Ok(HealthCheckStatus::Interrupted),
            other => Err(ValidationError::new(
                "health_check_status",
                format!("unknown health check status {other:?} (Docs/CONNECTOR_SPEC.md §7)"),
            )),
        }
    }
}

/// `Docs/SPEC.md` §4.4 Device lifecycle states, transcribed from the same
/// ASCII diagram `packages/contracts/src/states.ts`'s
/// `DEVICE_LIFECYCLE_STATES` transcribes (this crate's required reading is
/// `Docs/CONNECTOR_SPEC.md`, not `Docs/SPEC.md`; this enum exists here only
/// because `connector.ts`'s own module doc names the capability-descriptor
/// shape — which includes `required_state: DeviceLifecycleState[]` — as
/// vocabulary "CON-001 builds on", and reusing the exact, already-accepted
/// eleven-value list is the "no divergent copy" reading of that instruction,
/// not a decision to independently own the device lifecycle state machine).
/// This crate mirrors only the closed value set, not `states.ts`'s
/// transition maps, which this crate has no need for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceLifecycleState {
    Discovered,
    Identifying,
    Identified,
    Checking,
    EnrollmentRequired,
    Unsupported,
    Degraded,
    Enrolled,
    Ready,
    Offline,
    Busy,
}

impl DeviceLifecycleState {
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            DeviceLifecycleState::Discovered => "discovered",
            DeviceLifecycleState::Identifying => "identifying",
            DeviceLifecycleState::Identified => "identified",
            DeviceLifecycleState::Checking => "checking",
            DeviceLifecycleState::EnrollmentRequired => "enrollment-required",
            DeviceLifecycleState::Unsupported => "unsupported",
            DeviceLifecycleState::Degraded => "degraded",
            DeviceLifecycleState::Enrolled => "enrolled",
            DeviceLifecycleState::Ready => "ready",
            DeviceLifecycleState::Offline => "offline",
            DeviceLifecycleState::Busy => "busy",
        }
    }
}

impl fmt::Display for DeviceLifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

impl FromStr for DeviceLifecycleState {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "discovered" => Ok(DeviceLifecycleState::Discovered),
            "identifying" => Ok(DeviceLifecycleState::Identifying),
            "identified" => Ok(DeviceLifecycleState::Identified),
            "checking" => Ok(DeviceLifecycleState::Checking),
            "enrollment-required" => Ok(DeviceLifecycleState::EnrollmentRequired),
            "unsupported" => Ok(DeviceLifecycleState::Unsupported),
            "degraded" => Ok(DeviceLifecycleState::Degraded),
            "enrolled" => Ok(DeviceLifecycleState::Enrolled),
            "ready" => Ok(DeviceLifecycleState::Ready),
            "offline" => Ok(DeviceLifecycleState::Offline),
            "busy" => Ok(DeviceLifecycleState::Busy),
            other => Err(ValidationError::new(
                "device_lifecycle_state",
                format!("unknown device lifecycle state {other:?} (Docs/SPEC.md §4.4)"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_tier_round_trips_through_wire_string() {
        for tier in [
            SupportTier::Certified,
            SupportTier::Supported,
            SupportTier::Custom,
            SupportTier::Unsupported,
        ] {
            let wire = tier.as_wire_str();
            assert_eq!(wire.parse::<SupportTier>().unwrap(), tier);
            assert_eq!(tier.to_string(), wire);
        }
    }

    #[test]
    fn capability_risk_level_round_trips() {
        for level in [
            CapabilityRiskLevel::Low,
            CapabilityRiskLevel::Medium,
            CapabilityRiskLevel::High,
        ] {
            assert_eq!(
                level.as_wire_str().parse::<CapabilityRiskLevel>().unwrap(),
                level
            );
        }
    }

    #[test]
    fn health_check_status_round_trips_and_requires_detail_rules() {
        assert!(!HealthCheckStatus::Passed.requires_detail());
        for status in [
            HealthCheckStatus::Warning,
            HealthCheckStatus::Failed,
            HealthCheckStatus::Unavailable,
            HealthCheckStatus::Interrupted,
        ] {
            assert!(status.requires_detail());
            assert_eq!(
                status.as_wire_str().parse::<HealthCheckStatus>().unwrap(),
                status
            );
        }
    }

    #[test]
    fn device_lifecycle_state_round_trips_all_eleven_values() {
        let all = [
            DeviceLifecycleState::Discovered,
            DeviceLifecycleState::Identifying,
            DeviceLifecycleState::Identified,
            DeviceLifecycleState::Checking,
            DeviceLifecycleState::EnrollmentRequired,
            DeviceLifecycleState::Unsupported,
            DeviceLifecycleState::Degraded,
            DeviceLifecycleState::Enrolled,
            DeviceLifecycleState::Ready,
            DeviceLifecycleState::Offline,
            DeviceLifecycleState::Busy,
        ];
        assert_eq!(
            all.len(),
            11,
            "Docs/SPEC.md §4.4 defines exactly eleven states"
        );
        for state in all {
            assert_eq!(
                state.as_wire_str().parse::<DeviceLifecycleState>().unwrap(),
                state
            );
        }
    }

    #[test]
    fn unknown_wire_values_are_rejected_not_guessed() {
        assert!("archived".parse::<SupportTier>().is_err());
        assert!("critical".parse::<CapabilityRiskLevel>().is_err());
        assert!("degraded-ish".parse::<HealthCheckStatus>().is_err());
        assert!("retired".parse::<DeviceLifecycleState>().is_err());
    }
}
