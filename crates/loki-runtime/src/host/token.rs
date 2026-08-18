//! Interim local persistence for `loki-ipc`'s per-connection shared secret.
//!
//! `crates/loki-ipc/src/auth.rs`'s module doc explicitly hands this
//! decision to this node: "A later node (the real `loki-runtime`
//! integration, `RUN-003`) decides how the runtime process generates/
//! persists it ... and how a legitimate desktop/MCP client obtains its
//! copy."
//!
//! `Docs/ARCHITECTURE.md` §5 calls for "a per-installation secret stored in
//! OS-protected storage." Windows DPAPI-wrapped storage is the natural full
//! implementation of that, but it is not built here: it is the same
//! OS-protection boundary `DAT-002` (blocked on `OQ-007`) is chartered to
//! build for the SQLite database key, and duplicating a DPAPI integration ad
//! hoc in this node — rather than reusing whatever `DAT-002` lands — would
//! be exactly the kind of speculative/duplicated security-boundary work
//! `AGENTS.md` warns against.
//!
//! **What this module actually does instead** (an explicitly recorded gap,
//! not a silent downgrade — see the RUN-003 handoff): generate a random
//! token once and persist it as a plain file under the same per-user
//! `%LOCALAPPDATA%\LokiAI\runtime` directory the discovery file and database
//! already use (`src/host/paths.rs`), reusing it across restarts. Protection
//! today is whatever the default NTFS per-user-profile ownership already
//! provides — not yet DPAPI-wrapped. How a real desktop/MCP client obtains
//! its own copy remains unresolved (`DSK-002`'s handoff already reports this
//! honestly as "not yet available" for exactly this reason).

use std::io::{self, Read, Write};
use std::path::Path;

use uuid::Uuid;

/// Minimum accepted length for a token read back from disk — guards against
/// treating an empty/truncated/corrupted file as a valid secret and
/// silently authenticating with a weak value.
const MIN_TOKEN_LEN: usize = 32;

/// Reads the token at `path` if it looks like a real, previously generated
/// secret; otherwise generates a fresh one and writes it to `path`.
/// Returns the token's raw string value — callers wrap it in
/// `loki_ipc::auth::AuthToken` (kept out of this module so it has no
/// `loki-ipc` dependency of its own and stays portable/independently
/// testable).
pub fn load_or_create(path: &Path) -> io::Result<String> {
    if let Some(existing) = read_existing(path)? {
        return Ok(existing);
    }
    let generated = generate();
    write_new(path, &generated)?;
    Ok(generated)
}

fn read_existing(path: &Path) -> io::Result<Option<String>> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let trimmed = contents.trim();
    if trimmed.len() < MIN_TOKEN_LEN {
        return Ok(None);
    }
    Ok(Some(trimmed.to_string()))
}

fn write_new(path: &Path, token: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    file.write_all(token.as_bytes())?;
    Ok(())
}

/// Two random UUIDs concatenated: 256 bits of entropy from the already-
/// justified `uuid`/"v4" dependency, matching
/// `loki_ipc::auth::AuthToken::generate`'s own approach rather than adding a
/// second randomness source.
fn generate() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_new_token_when_no_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ipc.token");
        let token = load_or_create(&path).unwrap();
        assert!(token.len() >= MIN_TOKEN_LEN);
        assert!(path.exists());
    }

    #[test]
    fn reuses_an_existing_token_across_calls() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ipc.token");
        let first = load_or_create(&path).unwrap();
        let second = load_or_create(&path).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn a_truncated_or_short_file_is_treated_as_absent_and_regenerated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ipc.token");
        std::fs::write(&path, "too-short").unwrap();
        let token = load_or_create(&path).unwrap();
        assert!(token.len() >= MIN_TOKEN_LEN);
        assert_ne!(token, "too-short");
    }

    #[test]
    fn two_distinct_paths_get_distinct_tokens() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let a = load_or_create(&dir_a.path().join("ipc.token")).unwrap();
        let b = load_or_create(&dir_b.path().join("ipc.token")).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn creates_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("ipc.token");
        let token = load_or_create(&path).unwrap();
        assert!(token.len() >= MIN_TOKEN_LEN);
        assert!(path.exists());
    }
}
