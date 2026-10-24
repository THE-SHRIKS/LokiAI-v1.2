//! Explicit stage and lifecycle outcomes.
//!
//! This module is where `AGENTS.md` rule 6 ("Do not infer success from
//! process exit alone; use operation-specific verification") and
//! `Docs/CONNECTOR_SPEC.md` §9 ("Treat exit success as stage completion
//! only; connector verification still runs") become an enforced Rust type,
//! not just a documented expectation: [`LifecycleOutcome::succeeded`] is
//! `true` if and only if the `verify` stage independently returned
//! [`StageOutcome::Success`] — an [`StageOutcome::Success`] from `execute`
//! alone can never make it `true`, and a connector cannot construct a
//! [`LifecycleOutcome`] itself (only [`crate::lifecycle::run_operation`]
//! does, by actually calling `preflight`, `execute`, and `verify` in order).

use std::fmt;
use std::time::SystemTime;

use crate::error::{ConnectorError, ValidationError};
use crate::identity::DeviceIdentityEvidence;
use crate::operation::OperationStage;
use crate::validate::non_empty_bounded;

/// Evidence a stage produces when it succeeds — never fabricated: every
/// [`StageEvidence`] must carry a non-empty `summary` and a timestamp of
/// when it was actually observed.
#[derive(Debug, Clone, PartialEq)]
pub struct StageEvidence {
    pub summary: String,
    pub observed_at: SystemTime,
    pub evidence_ref: Option<String>,
    /// Optional structured identity evidence a connector's `identify`
    /// capability can attach to a stage's outcome, added for
    /// `Docs/graph/tasks/CON-SDK-IDENTITY-EVIDENCE-001.md` to unblock
    /// `CON-002-R1` (`Docs/graph/handoffs/CON-002-R1-1.md`): that node found
    /// no typed channel existed for a connector to report device-derived
    /// identity evidence (`evidence_type`, a fingerprint digest, and an
    /// [`crate::identity::IdentityConfidence`]) — only the free-text
    /// `summary` above, which Section 3 of that packet explicitly forbids
    /// overloading for this purpose.
    ///
    /// This carries [`DeviceIdentityEvidence`] — the already-validated,
    /// digest-only type produced by [`DeviceIdentityEvidence::new`] — and
    /// never the raw [`crate::identity::DeviceIdentityEvidenceInput`]: the
    /// input type borrows the caller's installation secret and raw
    /// fingerprint material, neither of which may be persisted into stage
    /// evidence (`raw_material_handling: "consumed_not_persisted"` in
    /// `identity.rs`), and it carries a lifetime parameter that would force
    /// a lifetime onto every `StageOutcome`/`LifecycleOutcome` in this crate
    /// — a breaking change this additive node must not make. A connector
    /// that constructs a `DeviceIdentityEvidence` from its own probe still
    /// does so through `identity.rs`'s existing validated constructor; this
    /// field is purely the payload-shape hook to attach that already-safe
    /// result to a stage's evidence. `None` by default: every existing
    /// connector and construction site is unaffected.
    pub identity_evidence: Option<DeviceIdentityEvidence>,
}

impl StageEvidence {
    pub fn new(
        summary: impl Into<String>,
        observed_at: SystemTime,
    ) -> Result<Self, ValidationError> {
        let summary = summary.into();
        non_empty_bounded("summary", &summary, 2000)?;
        Ok(Self {
            summary,
            observed_at,
            evidence_ref: None,
            identity_evidence: None,
        })
    }

    #[must_use]
    pub fn with_evidence_ref(mut self, evidence_ref: impl Into<String>) -> Self {
        self.evidence_ref = Some(evidence_ref.into());
        self
    }

    /// Attaches structured identity evidence to this stage's outcome. See
    /// the [`StageEvidence::identity_evidence`] field doc comment for why
    /// this takes the already-validated [`DeviceIdentityEvidence`] rather
    /// than raw probe input.
    #[must_use]
    pub fn with_identity_evidence(mut self, identity_evidence: DeviceIdentityEvidence) -> Self {
        self.identity_evidence = Some(identity_evidence);
        self
    }
}

/// A stage that did not complete before its deadline. Distinct from
/// [`ConnectorError`]: a timeout is not itself a connector-reported error
/// code/message, it is the outcome of [`crate::operation::OperationContext::deadline_passed`]
/// (checked by [`crate::lifecycle::run_operation`] before starting a stage)
/// or a connector's own cooperative deadline check inside a stage it has
/// already started (see `Docs/CONNECTOR_SPEC.md` §8 "safe cancellation
/// points").
#[derive(Debug, Clone, PartialEq)]
pub struct StageTimeout {
    pub stage: OperationStage,
    pub observed_at: SystemTime,
    pub deadline: SystemTime,
}

/// The explicit outcome of one lifecycle stage — exactly the three kinds
/// `Docs/graph/tasks/CON-001.md` requires: "explicit success/failure/timeout
/// outcomes".
#[derive(Debug, Clone, PartialEq)]
pub enum StageOutcome {
    Success(StageEvidence),
    Failure(ConnectorError),
    Timeout(StageTimeout),
}

impl StageOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, StageOutcome::Success(_))
    }

    /// Converts a non-success outcome into a [`StageFailure`] for
    /// [`crate::Connector::recover`] — `None` when this outcome is
    /// [`StageOutcome::Success`], since there is nothing to recover from.
    pub fn as_failure(&self) -> Option<StageFailure> {
        match self {
            StageOutcome::Success(_) => None,
            StageOutcome::Failure(e) => Some(StageFailure::Failed(e.clone())),
            StageOutcome::Timeout(t) => Some(StageFailure::TimedOut(t.clone())),
        }
    }
}

/// The non-success subset of [`StageOutcome`], passed to
/// [`crate::Connector::recover`] so a connector's recovery logic is never
/// handed a `Success` it would have to needlessly match away.
#[derive(Debug, Clone, PartialEq)]
pub enum StageFailure {
    Failed(ConnectorError),
    TimedOut(StageTimeout),
}

/// Total (never-panicking) widening back into [`StageOutcome`] — the
/// counterpart of [`StageOutcome::as_failure`]. Used by
/// [`crate::lifecycle::run_operation`] to record a [`StageFailure`] it
/// produced internally (e.g. a deadline that had already passed) as that
/// stage's [`StageOutcome`] without an `unwrap`/`expect` at the call site:
/// unlike [`StageOutcome::as_failure`], this direction can never fail, since
/// [`StageFailure`] excludes `Success` by construction.
impl From<StageFailure> for StageOutcome {
    fn from(failure: StageFailure) -> Self {
        match failure {
            StageFailure::Failed(e) => StageOutcome::Failure(e),
            StageFailure::TimedOut(t) => StageOutcome::Timeout(t),
        }
    }
}

impl fmt::Display for StageFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StageFailure::Failed(e) => write!(f, "failed: {e}"),
            StageFailure::TimedOut(t) => write!(f, "timed out at {} stage", t.stage),
        }
    }
}

/// The full result of running a connector through
/// [`crate::lifecycle::run_operation`]. Every field records whether/how far
/// the lifecycle got — `execute`/`verify`/`recovery` are `None` exactly when
/// that stage was never reached, which is itself the proof that a preflight
/// failure short-circuits `execute` (`Docs/graph/tasks/CON-001.md`
/// acceptance criteria).
#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleOutcome {
    pub stage_reached: OperationStage,
    pub preflight: StageOutcome,
    pub execute: Option<StageOutcome>,
    pub verify: Option<StageOutcome>,
    pub recovery: Option<StageOutcome>,
}

impl LifecycleOutcome {
    /// The single source of truth for "did this operation succeed". See
    /// this module's doc comment: only a [`StageOutcome::Success`] from
    /// `verify` can make this `true`.
    pub fn succeeded(&self) -> bool {
        matches!(self.verify, Some(StageOutcome::Success(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence() -> StageEvidence {
        StageEvidence::new("ok", SystemTime::now()).unwrap()
    }

    fn error() -> ConnectorError {
        ConnectorError::new("boom", "it broke").unwrap()
    }

    #[test]
    fn as_failure_is_none_for_success() {
        assert!(StageOutcome::Success(evidence()).as_failure().is_none());
    }

    #[test]
    fn as_failure_wraps_failure_and_timeout() {
        assert!(matches!(
            StageOutcome::Failure(error()).as_failure(),
            Some(StageFailure::Failed(_))
        ));
        let timeout = StageTimeout {
            stage: OperationStage::Execute,
            observed_at: SystemTime::now(),
            deadline: SystemTime::now(),
        };
        assert!(matches!(
            StageOutcome::Timeout(timeout).as_failure(),
            Some(StageFailure::TimedOut(_))
        ));
    }

    #[test]
    fn stage_failure_round_trips_losslessly_back_into_stage_outcome() {
        let failed: StageOutcome = StageFailure::Failed(error()).into();
        assert!(matches!(failed, StageOutcome::Failure(_)));

        let timeout = StageTimeout {
            stage: OperationStage::Preflight,
            observed_at: SystemTime::now(),
            deadline: SystemTime::now(),
        };
        let timed_out: StageOutcome = StageFailure::TimedOut(timeout).into();
        assert!(matches!(timed_out, StageOutcome::Timeout(_)));
    }

    #[test]
    fn succeeded_requires_verify_success_specifically() {
        let base = LifecycleOutcome {
            stage_reached: OperationStage::Execute,
            preflight: StageOutcome::Success(evidence()),
            execute: Some(StageOutcome::Success(evidence())),
            verify: None,
            recovery: None,
        };
        assert!(
            !base.succeeded(),
            "execute succeeding alone (verify never run) must not count as success"
        );

        let verify_failed = LifecycleOutcome {
            verify: Some(StageOutcome::Failure(error())),
            ..base.clone()
        };
        assert!(!verify_failed.succeeded());

        let fully_succeeded = LifecycleOutcome {
            verify: Some(StageOutcome::Success(evidence())),
            ..base
        };
        assert!(fully_succeeded.succeeded());
    }
}
