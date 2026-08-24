use std::fmt;

#[derive(Debug)]
pub enum AuthorityError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    InvalidInput { field: &'static str, reason: String },
    WrongPrincipal,
    SessionNotFound,
    SessionExpired,
    SessionRevoked,
    SessionContextMismatch,
    CapabilityDenied,
    CloudUnavailable,
    Conflict(String),
}

impl fmt::Display for AuthorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "local authority I/O error: {error}"),
            Self::Sqlite(error) => write!(f, "local authority database error: {error}"),
            Self::InvalidInput { field, reason } => {
                write!(f, "invalid local authority field '{field}': {reason}")
            }
            Self::WrongPrincipal => write!(f, "Windows principal is not the installation owner"),
            Self::SessionNotFound => write!(f, "local authority session was not found"),
            Self::SessionExpired => write!(f, "local authority session has expired"),
            Self::SessionRevoked => write!(f, "local authority session has been revoked"),
            Self::SessionContextMismatch => {
                write!(f, "local authority session does not match this IPC context")
            }
            Self::CapabilityDenied => write!(f, "local capability is not allowed"),
            Self::CloudUnavailable => write!(f, "cloud authority is not configured"),
            Self::Conflict(detail) => write!(f, "local authority conflict: {detail}"),
        }
    }
}

impl std::error::Error for AuthorityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for AuthorityError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for AuthorityError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}
