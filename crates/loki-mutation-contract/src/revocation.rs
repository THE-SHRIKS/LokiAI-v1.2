//! `RevocationDisposition` — ADR-0006 "Decision": "Revocation or expiry
//! before commit denies acceptance. Revocation after commit does not erase
//! the audit record or pretend the commit never happened. It prevents
//! not-yet-started stages, requests cancellation at declared safe points for
//! active work, and requires reconciliation."
//!
//! # Open extension point: "declared safe point" (§9 item 1)
//!
//! `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md` §9 item 1 records that
//! ADR-0006 does not define *who* declares a safe point or *how* it is
//! represented generically across mutation kinds. This module deliberately
//! does not invent a generic "safe point" mechanism — it exposes only the
//! coarse pre-commit/post-commit distinction as [`RevocationDisposition`].
//! Prior art for the eventual cooperative-cancellation mechanism itself
//! (not adopted here, only cited) is `loki-jobs`'s own pattern:
//! `crates/loki-jobs/src/engine.rs`'s `JobEngine::request_cancel` /
//! `JobEngine::is_cancel_requested` / `JobEngine::acknowledge_cancelled`.
//!
//! # `outcome_unknown` is a reused state, not a detection contract (§9 item 2)
//!
//! `RequiresPostCommitReconciliation` intentionally references
//! `loki_storage::states::JobState::OutcomeUnknown`/`Cancelled` by doc
//! citation rather than redefining a physical-outcome state here. This
//! module does not decide *when* a result counts as "uncertain" versus
//! "clearly failed" — `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md` §9 item
//! 2 flags that detection contract to `P1-ACCEPTANCE-STORE-001`/
//! `P1-MUTATION-REV-001`.

/// Closed disposition of a revocation/expiry event relative to the
/// acceptance transaction's commit point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationDisposition {
    /// The grant was revoked/expired before the acceptance transaction
    /// committed: ADR-0006 — "denies acceptance." No job/outbox record was
    /// created.
    DeniesAcceptance,
    /// The grant was revoked/expired after the acceptance transaction
    /// committed: not-yet-started stages are prevented, active work is
    /// asked to cooperatively cancel at its own declared safe point (see
    /// this module's top doc comment), and the physical outcome may need to
    /// be recorded as `JobState::OutcomeUnknown` rather than guessed
    /// cancelled or succeeded — see `loki_storage::states::JobState`.
    RequiresPostCommitReconciliation,
}

/// Pure classification of a revocation/expiry event given only whether the
/// acceptance transaction had already committed when it occurred. This
/// crate performs no I/O and observes no clock; the caller
/// (`P1-ACCEPTANCE-STORE-001`) determines `committed_before_revocation`
/// from its own transaction state.
pub fn evaluate_revocation_disposition(committed_before_revocation: bool) -> RevocationDisposition {
    if committed_before_revocation {
        RevocationDisposition::RequiresPostCommitReconciliation
    } else {
        RevocationDisposition::DeniesAcceptance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_commit_revocation_denies_acceptance() {
        assert_eq!(
            evaluate_revocation_disposition(false),
            RevocationDisposition::DeniesAcceptance
        );
    }

    #[test]
    fn post_commit_revocation_requires_reconciliation() {
        assert_eq!(
            evaluate_revocation_disposition(true),
            RevocationDisposition::RequiresPostCommitReconciliation
        );
    }

    #[test]
    fn revocation_disposition_is_closed_with_exhaustive_match() {
        for sample in [
            RevocationDisposition::DeniesAcceptance,
            RevocationDisposition::RequiresPostCommitReconciliation,
        ] {
            match sample {
                RevocationDisposition::DeniesAcceptance => {}
                RevocationDisposition::RequiresPostCommitReconciliation => {}
            }
        }
    }
}
