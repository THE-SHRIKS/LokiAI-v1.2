use serde::{Deserialize, Serialize};

use crate::{BuildError, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapsuleState {
    Requested,
    Creating,
    Created,
    Attaching,
    Attached,
    Mounting,
    Mounted,
    PreparingAcl,
    Prepared,
    Executing,
    OutputQuarantined,
    Detaching,
    Detached,
    Destroying,
    Destroyed,
    Quarantined,
}

impl CapsuleState {
    pub fn can_transition(self, next: Self) -> bool {
        use CapsuleState::*;
        matches!(
            (self, next),
            (Requested, Creating)
                | (Creating, Created)
                | (Created, Attaching)
                | (Attaching, Attached)
                | (Attached, Mounting)
                | (Mounting, Mounted)
                | (Mounted, PreparingAcl)
                | (PreparingAcl, Prepared)
                | (Prepared, Executing)
                | (Executing, OutputQuarantined)
                | (OutputQuarantined, Detaching)
                | (Detaching, Detached)
                | (Detached, Destroying)
                | (Destroying, Destroyed)
                | (_, Quarantined)
        )
    }

    pub fn transition(self, next: Self) -> Result<Self, BuildError> {
        if self == next && matches!(self, Self::Destroyed | Self::Quarantined) {
            return Ok(self);
        }
        self.can_transition(next).then_some(next).ok_or_else(|| {
            BuildError::new(
                ErrorCode::InvalidTransition,
                format!("transition from {self:?} to {next:?} is not permitted"),
            )
        })
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Destroyed | Self::Quarantined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_is_explicit() {
        let states = [
            CapsuleState::Requested,
            CapsuleState::Creating,
            CapsuleState::Created,
            CapsuleState::Attaching,
            CapsuleState::Attached,
            CapsuleState::Mounting,
            CapsuleState::Mounted,
            CapsuleState::PreparingAcl,
            CapsuleState::Prepared,
            CapsuleState::Executing,
            CapsuleState::OutputQuarantined,
            CapsuleState::Detaching,
            CapsuleState::Detached,
            CapsuleState::Destroying,
            CapsuleState::Destroyed,
        ];
        for pair in states.windows(2) {
            assert_eq!(pair[0].transition(pair[1]).unwrap(), pair[1]);
        }
    }

    #[test]
    fn skips_and_reuse_fail_closed() {
        assert!(
            CapsuleState::Requested
                .transition(CapsuleState::Mounted)
                .is_err()
        );
        assert!(
            CapsuleState::Destroyed
                .transition(CapsuleState::Creating)
                .is_err()
        );
        assert_eq!(
            CapsuleState::Attaching
                .transition(CapsuleState::Quarantined)
                .unwrap(),
            CapsuleState::Quarantined
        );
    }
}
