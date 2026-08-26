use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Authentication,
    Replay,
    Expired,
    Malformed,
    Oversized,
    IdentityMismatch,
    InvalidTransition,
    Journal,
    NativeUnavailable,
    NativeFailure,
    Quarantined,
    Validation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildError {
    pub code: ErrorCode,
    pub detail: String,
}

impl BuildError {
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl Display for BuildError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.detail)
    }
}

impl std::error::Error for BuildError {}
