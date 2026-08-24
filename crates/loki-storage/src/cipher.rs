//! The database encryption key/cipher boundary `DAT-002` will implement.
//!
//! `Docs/graph/tasks/DAT-001.md` §1: "Plaintext SQLite for now —
//! full-database encryption is `DAT-002` (blocked on `OQ-007`); this node
//! must not invent an encryption scheme to fill that gap, just leave the
//! key/cipher boundary as an explicit, swappable seam." `Docs/ARCHITECTURE.md`
//! §8 requires the eventual key to come from OS-protected secret storage and
//! never be persisted beside the database — this crate does not implement
//! that retrieval (out of scope, `DAT-002`); it only defines where it plugs
//! in.

/// How [`crate::Storage::open`] (or [`crate::Storage::open_in_memory`])
/// should unlock the database file.
///
/// `DAT-002` is expected to add a variant such as `Sqlcipher(SqlCipherKey)`
/// here and thread the key through [`crate::Storage::open`]. That is a
/// feature-flag-level change on the `rusqlite` dependency, confirmed
/// available on the exact pinned version this crate uses today:
/// `bundled-sqlcipher` and `sqlcipher` are both real feature names on
/// `rusqlite = "=0.40.2"` (this crate's `Cargo.toml`) — swapping the
/// `bundled` feature for `bundled-sqlcipher` is the expected path, not a
/// rewrite of this module or of `src/storage.rs`'s `Storage::open` shape.
///
/// `#[non_exhaustive]` because this type is expected to grow a variant in a
/// later node; a caller must not assume today's single variant is final.
use crate::key::DatabaseKey;

#[derive(Default)]
#[non_exhaustive]
pub enum Cipher {
    /// No encryption. The only variant this node (`DAT-001`) implements.
    /// [`crate::Storage::open`] returns [`crate::StorageError::UnsupportedCipher`]
    /// for anything else, rather than silently ignoring an encryption
    /// request or inventing a scheme (`AGENTS.md` rule 1).
    #[default]
    None,
    /// SQLCipher Community Edition with a key obtained from the Windows
    /// DPAPI-backed secret store. A raw passphrase is never accepted here.
    SqlCipher(DatabaseKey),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_none() {
        assert!(matches!(Cipher::default(), Cipher::None));
    }
}
