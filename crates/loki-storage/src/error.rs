//! Storage error type. Every fallible operation in this crate returns
//! `Result<T, StorageError>` — no panics on external input (malformed IDs,
//! bad timestamps, corrupt files, SQL failures) per `AGENTS.md` rule 1/4.

use std::fmt;

/// Errors this crate can return. Every variant is a distinct, honest
/// failure mode — this crate never converts a failure into a fabricated
/// success value (`AGENTS.md` rule 1: "If a source is unavailable, expose
/// an honest ... error state").
#[derive(Debug)]
pub enum StorageError {
    /// Filesystem I/O failure (open/create/copy).
    Io(std::io::Error),
    /// The underlying SQLite driver reported an error (constraint
    /// violation, malformed SQL, busy timeout exceeded, ...).
    Sqlite(rusqlite::Error),
    #[cfg(windows)]
    Secret(crate::secret::SecretError),
    /// The database file exists but failed an integrity check (or SQLite
    /// could not even recognize it as a database). Never silently treated
    /// as "create a fresh empty database instead" — see
    /// `Docs/ARCHITECTURE.md` §8's key-recovery principle ("a wrong/missing
    /// key ... cannot create a replacement empty database"), applied here
    /// to file corruption.
    Corrupt { path: String, detail: String },
    /// A caller-supplied ID failed the shape validation mirrored from
    /// `packages/contracts/src/brand.ts` (`idSchema`) — see `src/ids.rs`.
    InvalidId { kind: &'static str, reason: String },
    /// A caller-supplied timestamp is not a structurally valid UTC ISO 8601
    /// string (`Docs/SPEC.md` Section 1: "All timestamps are UTC ISO
    /// 8601") — see `src/timestamp.rs`.
    InvalidTimestamp { field: &'static str, value: String },
    /// A caller-supplied (or, on read, a stored) state string is not one of
    /// the closed set mirrored from `packages/contracts/src/states.ts` /
    /// `connector.ts` — see `src/states.rs`.
    InvalidState { field: &'static str, value: String },
    /// `Storage::open`/`open_in_memory` was asked for a cipher this node
    /// does not implement yet — see [`crate::cipher::Cipher`]. This is
    /// `DAT-002`'s seam, not a bug in this crate.
    UnsupportedCipher,
    /// A lookup/update by ID found no matching row.
    NotFound { kind: &'static str, id: String },
    /// A write violated a uniqueness/consistency rule this crate enforces
    /// beyond a raw SQLite constraint (e.g. an already-terminal state).
    Conflict { detail: String },
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Io(e) => write!(f, "storage I/O error: {e}"),
            StorageError::Sqlite(e) => write!(f, "sqlite error: {e}"),
            #[cfg(windows)]
            StorageError::Secret(e) => write!(f, "{e}"),
            StorageError::Corrupt { path, detail } => {
                write!(
                    f,
                    "database at '{path}' failed integrity verification: {detail}"
                )
            }
            StorageError::InvalidId { kind, reason } => write!(f, "invalid {kind}: {reason}"),
            StorageError::InvalidTimestamp { field, value } => write!(
                f,
                "field '{field}' is not a valid UTC ISO 8601 timestamp: '{value}'"
            ),
            StorageError::InvalidState { field, value } => write!(
                f,
                "field '{field}' has an unrecognized state value: '{value}'"
            ),
            StorageError::UnsupportedCipher => write!(
                f,
                "requested cipher is not implemented by this build (see DAT-002)"
            ),
            StorageError::NotFound { kind, id } => write!(f, "{kind} '{id}' not found"),
            StorageError::Conflict { detail } => write!(f, "storage conflict: {detail}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StorageError::Io(e) => Some(e),
            StorageError::Sqlite(e) => Some(e),
            #[cfg(windows)]
            StorageError::Secret(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(e: rusqlite::Error) -> Self {
        StorageError::Sqlite(e)
    }
}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        StorageError::Io(e)
    }
}

#[cfg(windows)]
impl From<crate::secret::SecretError> for StorageError {
    fn from(e: crate::secret::SecretError) -> Self {
        StorageError::Secret(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_does_not_panic_for_every_variant() {
        let samples: Vec<StorageError> = vec![
            StorageError::Io(std::io::Error::other("x")),
            StorageError::Corrupt {
                path: "p".into(),
                detail: "d".into(),
            },
            StorageError::InvalidId {
                kind: "DeviceId",
                reason: "r".into(),
            },
            StorageError::InvalidTimestamp {
                field: "f",
                value: "v".into(),
            },
            StorageError::InvalidState {
                field: "f",
                value: "v".into(),
            },
            StorageError::UnsupportedCipher,
            StorageError::NotFound {
                kind: "device",
                id: "1".into(),
            },
            StorageError::Conflict { detail: "d".into() },
        ];
        for err in samples {
            let _ = format!("{err}");
            let _: &dyn std::error::Error = &err;
        }
    }
}
