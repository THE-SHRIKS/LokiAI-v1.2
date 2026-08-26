//! `MutationMethod` — the closed mutation-method vocabulary ADR-0006
//! "Decision"/"Consequences" names. No open/string variant: an unrecognized
//! wire string is a rejected [`MutationContractError`], never a silently
//! accepted new case (mirrors `loki_approvals::evaluate_operation`'s closed
//! policy pattern, `crates/loki-approvals/src/lib.rs`).

use crate::error::MutationContractError;

/// Whether a [`MutationMethod`] requires a consumed approval before it may
/// be accepted. Mirrors `loki_approvals::PolicyDecision`'s two consequential
/// outcomes (`ApprovalRequired`/`Allow`) — this crate does not model a
/// `Deny` tier because a caller-supplied `MutationMethod` is by construction
/// one of the two ADR-0006-named categories; local operations outside this
/// closed set (e.g. `factory_reset`) never reach this crate as a
/// `MutationMethod` at all — see `loki_approvals::evaluate_operation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyTier {
    ApprovalRequired,
    NotRequired,
}

/// The closed mutation-method vocabulary. Covers both ADR-0006-named
/// categories:
/// - approval-gated device operations, mirroring
///   `loki_approvals::evaluate_operation`'s existing closed
///   `ApprovalRequired` set (`restart`, `flash`, `apply_config`,
///   `contained_build`);
/// - approval-not-required durable mutations named in ADR-0006
///   "Consequences" (`AgentRunRegister`, `AgentRunUpdate`,
///   `WorkIntentAcknowledge`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MutationMethod {
    Restart,
    Flash,
    ApplyConfig,
    ContainedBuild,
    AgentRunRegister,
    AgentRunUpdate,
    WorkIntentAcknowledge,
}

impl MutationMethod {
    /// Every variant, in declaration order.
    pub const ALL: &'static [MutationMethod] = &[
        MutationMethod::Restart,
        MutationMethod::Flash,
        MutationMethod::ApplyConfig,
        MutationMethod::ContainedBuild,
        MutationMethod::AgentRunRegister,
        MutationMethod::AgentRunUpdate,
        MutationMethod::WorkIntentAcknowledge,
    ];

    /// The canonical wire/log string for this variant. The four
    /// approval-gated variants use the exact strings
    /// `loki_approvals::evaluate_operation` already matches on, so a caller
    /// bridging the two crates does not need a second translation table.
    /// Exhaustive `match`, no wildcard arm — a variant added later to this
    /// enum without a case here fails to compile.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Restart => "restart",
            Self::Flash => "flash",
            Self::ApplyConfig => "apply_config",
            Self::ContainedBuild => "contained_build",
            Self::AgentRunRegister => "agent_run_register",
            Self::AgentRunUpdate => "agent_run_update",
            Self::WorkIntentAcknowledge => "work_intent_acknowledge",
        }
    }

    /// Parses a wire string into this closed enum. An unrecognized string
    /// is `Err`, never silently coerced to a default variant — this is the
    /// "unknown mutation method string is not constructible" guarantee.
    pub fn parse(value: &str) -> Result<Self, MutationContractError> {
        match value {
            "restart" => Ok(Self::Restart),
            "flash" => Ok(Self::Flash),
            "apply_config" => Ok(Self::ApplyConfig),
            "contained_build" => Ok(Self::ContainedBuild),
            "agent_run_register" => Ok(Self::AgentRunRegister),
            "agent_run_update" => Ok(Self::AgentRunUpdate),
            "work_intent_acknowledge" => Ok(Self::WorkIntentAcknowledge),
            other => Err(MutationContractError::InvalidInput {
                field: "mutation_method",
                reason: format!("unrecognized mutation method '{other}'"),
            }),
        }
    }

    /// Exhaustive `match`, no wildcard arm.
    pub fn policy_tier(&self) -> PolicyTier {
        match self {
            Self::Restart | Self::Flash | Self::ApplyConfig | Self::ContainedBuild => {
                PolicyTier::ApprovalRequired
            }
            Self::AgentRunRegister | Self::AgentRunUpdate | Self::WorkIntentAcknowledge => {
                PolicyTier::NotRequired
            }
        }
    }

    pub fn requires_approval(&self) -> bool {
        matches!(self.policy_tier(), PolicyTier::ApprovalRequired)
    }
}

impl std::fmt::Display for MutationMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_round_trips_through_as_str_and_parse() {
        for method in MutationMethod::ALL {
            assert_eq!(MutationMethod::parse(method.as_str()).unwrap(), *method);
        }
    }

    #[test]
    fn unknown_mutation_method_string_is_not_constructible() {
        assert!(matches!(
            MutationMethod::parse("factory_reset"),
            Err(MutationContractError::InvalidInput {
                field: "mutation_method",
                ..
            })
        ));
        assert!(matches!(
            MutationMethod::parse(""),
            Err(MutationContractError::InvalidInput { .. })
        ));
        assert!(matches!(
            MutationMethod::parse("Restart"),
            Err(MutationContractError::InvalidInput { .. })
        ));
    }

    #[test]
    fn device_operation_methods_require_approval_and_agent_methods_do_not() {
        for method in [
            MutationMethod::Restart,
            MutationMethod::Flash,
            MutationMethod::ApplyConfig,
            MutationMethod::ContainedBuild,
        ] {
            assert!(method.requires_approval());
        }
        for method in [
            MutationMethod::AgentRunRegister,
            MutationMethod::AgentRunUpdate,
            MutationMethod::WorkIntentAcknowledge,
        ] {
            assert!(!method.requires_approval());
        }
    }
}
