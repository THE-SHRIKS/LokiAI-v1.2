//! Fail-closed consequential-mutation boundary.

/// Why mutation submission is unavailable in this runtime composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationUnavailableReason {
    /// Approval creation and mutation acceptance do not yet consume the same
    /// corrected operation-binding contract at this revision.
    OperationBindingNotIntegrated,
}

/// Current state of the mutation submission boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationAvailability {
    Unavailable(MutationUnavailableReason),
}

/// A request token with no public constructor or fields.
///
/// Keeping this type sealed prevents any caller from supplying a partial,
/// locally invented mutation contract while the corrected operation binding
/// is absent.
pub struct ClosedMutationRequest {
    _sealed: (),
}

/// No successful acceptance value exists while the boundary is closed.
#[derive(Debug)]
pub enum MutationAcceptance {}

/// Explicit rejection returned by the closed mutation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MutationUnavailable {
    pub reason: MutationUnavailableReason,
}

impl std::fmt::Display for MutationUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mutation submission unavailable: {:?}", self.reason)
    }
}

impl std::error::Error for MutationUnavailable {}

/// Bounded internal port for consequential mutation submission.
pub trait MutationSubmissionPort {
    fn availability(&self) -> MutationAvailability;

    fn submit_mutation(
        &self,
        request: ClosedMutationRequest,
    ) -> Result<MutationAcceptance, MutationUnavailable>;
}

pub(super) struct ClosedMutationSubmission;

impl MutationSubmissionPort for ClosedMutationSubmission {
    fn availability(&self) -> MutationAvailability {
        MutationAvailability::Unavailable(MutationUnavailableReason::OperationBindingNotIntegrated)
    }

    fn submit_mutation(
        &self,
        _request: ClosedMutationRequest,
    ) -> Result<MutationAcceptance, MutationUnavailable> {
        Err(MutationUnavailable {
            reason: MutationUnavailableReason::OperationBindingNotIntegrated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Only this module can construct `ClosedMutationRequest` (its field is
    // private) — external callers, including `RuntimeServices` consumers,
    // have no way to build one at all. This test exercises the port from
    // the one place that can, so the fail-closed behavior itself is
    // verified rather than merely asserted by type-level sealing.
    #[test]
    fn closed_submission_genuinely_refuses_every_request() {
        let port = ClosedMutationSubmission;
        assert_eq!(
            port.availability(),
            MutationAvailability::Unavailable(
                MutationUnavailableReason::OperationBindingNotIntegrated
            )
        );

        let request = ClosedMutationRequest { _sealed: () };
        let result = port.submit_mutation(request);

        let error = result.expect_err("the closed mutation port must never fabricate success");
        assert_eq!(
            error.reason,
            MutationUnavailableReason::OperationBindingNotIntegrated
        );
        assert_eq!(
            error.to_string(),
            "mutation submission unavailable: OperationBindingNotIntegrated"
        );
    }

    // The closed port never returns `Ok`, so `MutationAcceptance` has no
    // constructible value in this composition. This is a compile-time
    // property, not a runtime one: an uninhabited `Ok` type makes a
    // fabricated-success return value impossible to construct here, not
    // merely unlikely.
    #[allow(dead_code)]
    fn mutation_acceptance_is_uninhabited(never: MutationAcceptance) -> ! {
        match never {}
    }
}
