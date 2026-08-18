//! This engine's allowed job-state transitions.
//!
//! `loki_storage::states::JobState` is a shared 15-value vocabulary mirrored
//! from `Docs/SPEC.md` 4.7/4.8 — `policy-check`/`waiting-approval` belong to
//! `APR-001`'s approval gate, `rollback-pending`/`rolling-back`/
//! `rolled-back` are a deployment-rollback concept, and `restarting` is a
//! device-restart sub-stage of a deployment (`Docs/SPEC.md` 4.8). None of
//! those are this node's bounded scope
//! (`Docs/graph/tasks/JOB-001.md` §1: "created → leased → running →
//! (verify) → completed/failed, with explicit `quarantine`"). This module
//! only recognizes the subset of `JobState` this engine actually drives —
//! `queued -> preflight -> executing -> testing -> succeeded`, with
//! `failed`/`cancelled`/`outcome-unknown` reachable from any non-terminal
//! in-flight state — and rejects every other `(from, to)` pair, including
//! any move into/out of an out-of-scope state, as an
//! [`crate::error::JobsError::InvalidTransition`].

use loki_storage::states::JobState;

/// `true` if moving a job from `from` to `to` is part of this engine's
/// supported lifecycle.
pub(crate) fn is_allowed(from: JobState, to: JobState) -> bool {
    use JobState::{
        Cancelled, Executing, Failed, OutcomeUnknown, Preflight, Queued, Succeeded, Testing,
    };

    matches!(
        (from, to),
        (Queued, Preflight)
            | (Preflight, Executing)
            | (Executing, Testing)
            | (Testing, Succeeded)
            | (Preflight, Failed)
            | (Executing, Failed)
            | (Testing, Failed)
            | (Queued, Cancelled)
            | (Preflight, Cancelled)
            | (Executing, Cancelled)
            | (Testing, Cancelled)
            | (Preflight, OutcomeUnknown)
            | (Executing, OutcomeUnknown)
            | (Testing, OutcomeUnknown)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_is_allowed() {
        assert!(is_allowed(JobState::Queued, JobState::Preflight));
        assert!(is_allowed(JobState::Preflight, JobState::Executing));
        assert!(is_allowed(JobState::Executing, JobState::Testing));
        assert!(is_allowed(JobState::Testing, JobState::Succeeded));
    }

    #[test]
    fn terminal_states_have_no_outgoing_transitions() {
        for terminal in [
            JobState::Succeeded,
            JobState::Failed,
            JobState::Cancelled,
            JobState::OutcomeUnknown,
        ] {
            for to in JobState::ALL {
                assert!(
                    !is_allowed(terminal, *to),
                    "terminal state {terminal:?} must not transition to {to:?}"
                );
            }
        }
    }

    #[test]
    fn queued_can_only_move_to_preflight_or_cancelled() {
        for to in JobState::ALL {
            let allowed = is_allowed(JobState::Queued, *to);
            let expected = matches!(to, JobState::Preflight | JobState::Cancelled);
            assert_eq!(allowed, expected, "queued -> {to:?}");
        }
    }

    #[test]
    fn out_of_scope_states_have_no_allowed_transitions_in_either_direction() {
        let out_of_scope = [
            JobState::PolicyCheck,
            JobState::WaitingApproval,
            JobState::Restarting,
            JobState::Observing,
            JobState::RollbackPending,
            JobState::RollingBack,
            JobState::RolledBack,
        ];
        for state in out_of_scope {
            for other in JobState::ALL {
                assert!(!is_allowed(state, *other), "{state:?} -> {other:?}");
                assert!(!is_allowed(*other, state), "{other:?} -> {state:?}");
            }
        }
    }

    #[test]
    fn quarantine_is_reachable_only_from_in_flight_states() {
        for from in JobState::ALL {
            let allowed = is_allowed(*from, JobState::OutcomeUnknown);
            let expected = matches!(
                from,
                JobState::Preflight | JobState::Executing | JobState::Testing
            );
            assert_eq!(allowed, expected, "{from:?} -> outcome-unknown");
        }
    }
}
