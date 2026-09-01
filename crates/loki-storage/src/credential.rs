//! Purpose-bound, additive Windows-DPAPI credential minting/loading.
//!
//! `P1-NODE-IPC-CLIENT-001` / `ADR-0009` (Option A, accepted): the one
//! small shared DPAPI credential convention this workspace's future Node
//! consumers (Electron main, the MCP server, via `crates/loki-credential-
//! native`) read identically to what this module writes. Additive only:
//! this module never edits `crate::secret`'s existing DB-key/legacy-IPC-
//! token functions or its shared `ENTROPY` constant — it defines its own,
//! separate DPAPI entropy constant and its own protect/unprotect calls so
//! `secret.rs` has zero regression risk from this node.
//!
//! # Purpose-bound format
//!
//! [`CredentialKind`] has two variants today, `DesktopInstallation` and
//! `AgentConnection`, matching `loki_ipc::business::PrincipalKind` exactly
//! (no new kind invented here). The plaintext DPAPI-protects is not the raw
//! secret alone but a small envelope: `[format_version: u8][kind_tag:
//! u8][secret bytes]`. [`load_credential`] verifies both bytes before ever
//! returning the trailing secret bytes to a caller, so a credential
//! protected for one kind can **never** be silently accepted as the other
//! kind, even if a caller mistakenly points the wrong loader at the wrong
//! path — this is a structural property of the format (checked
//! unconditionally, before any success path returns), not a convention a
//! caller could bypass by using the "wrong" API. A single DPAPI entropy
//! constant is shared by both kinds (see [`CREDENTIAL_ENTROPY`]'s own doc
//! comment for why that, not per-kind entropy, is the right place to put
//! the "distinct from `secret.rs`" requirement) — kind separation is
//! enforced entirely by the envelope's `kind_tag` byte, which is exactly
//! what makes the mismatch case a real, observable, typed error
//! ([`CredentialError::KindMismatch`]) instead of an opaque DPAPI decrypt
//! failure indistinguishable from ordinary corruption.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom, CRYPT_INTEGER_BLOB,
    CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

/// Random credential material length, in bytes (256 bits) — generous,
/// unguessable entropy for an opaque bearer credential, matching
/// `secret.rs::generate_database_key`'s own use of `BCryptGenRandom`.
const CREDENTIAL_BYTES: usize = 32;

/// Envelope format version. Bumping this would be a breaking change to
/// every already-persisted credential file; there is exactly one version
/// today.
const FORMAT_VERSION: u8 = 1;

/// This module's own DPAPI entropy constant — deliberately a single value
/// shared by every [`CredentialKind`], not a per-kind value. Two structural
/// reasons, not merely convenience:
///
/// 1. `secret.rs`'s own `ENTROPY` constant is not `pub`, so this module
///    cannot see or reuse it (by design — no edit to `secret.rs`); this
///    constant only needs to be *distinct from that private value*, which a
///    single new value already satisfies, per `ADR-0009`'s Option A
///    consequence "must mint and protect BOTH credentials using the SAME
///    DPAPI entropy/blob convention" (singular "convention", not "one
///    convention per kind").
/// 2. Per-kind entropy would make `CryptUnprotectData` itself fail (a
///    generic Windows error, `CredentialError::Windows`) the instant a
///    caller pointed the wrong loader at the wrong file, before this
///    module's own envelope/kind-tag check ever ran — collapsing the
///    "wrong kind" case into "corrupt/unreadable", which is exactly the
///    "purpose-bound... not interchangeable/confusable at the format
///    level" property `ADR-0009` requires, but expressed as an
///    undistinguishable opaque failure instead of the specific, typed
///    [`CredentialError::KindMismatch`] `P1-NODE-IPC-CLIENT-001` Deliverable
///    1/6 requires callers be able to observe. Keeping entropy shared and
///    pushing the actual kind check into the (always-decryptable) envelope
///    is what makes that check unconditional and observable, not merely
///    theoretically present.
const CREDENTIAL_ENTROPY: &[u8] =
    b"LokiAI Main v1.2 purpose-bound credential boundary (P1-NODE-IPC-CLIENT-001, ADR-0009)";

/// Which purpose-bound credential this is. Matches
/// `loki_ipc::business::PrincipalKind`'s two non-`RuntimeService` variants
/// exactly (this crate does not depend on `loki-ipc`, so the enum is
/// duplicated here rather than imported — see this module's own top doc
/// comment; `crates/loki-runtime/src/host/credential.rs` is the layer that
/// bridges the two).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CredentialKind {
    DesktopInstallation,
    AgentConnection,
}

impl CredentialKind {
    fn tag(self) -> u8 {
        match self {
            CredentialKind::DesktopInstallation => 1,
            CredentialKind::AgentConnection => 2,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(CredentialKind::DesktopInstallation),
            2 => Some(CredentialKind::AgentConnection),
            _ => None,
        }
    }

    /// Stable, lower-`snake_case` identifier — matches
    /// `loki_ipc::business::PrincipalKind`'s `#[serde(rename_all =
    /// "snake_case")]` rendering exactly, useful for diagnostics/error
    /// messages without pulling in `loki-ipc` as a dependency.
    pub fn as_str(self) -> &'static str {
        match self {
            CredentialKind::DesktopInstallation => "desktop_installation",
            CredentialKind::AgentConnection => "agent_connection",
        }
    }
}

impl std::fmt::Display for CredentialKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every way loading/minting a purpose-bound credential can fail. Every
/// variant is a specific, typed, distinguishable reason — never a bare
/// generic error — so a caller (ultimately, `crates/loki-credential-
/// native`'s Node-visible error taxonomy) can fail closed on the *specific*
/// cause instead of a generic "could not read credential".
///
/// Deliberately **not** `loki_storage::secret::SecretError`: that type has
/// no way to express [`CredentialError::KindMismatch`] without adding a
/// variant to it, which would be an edit to `secret.rs` this node's
/// ownership forbids. The `Io`/`MissingProtectedSecret`/
/// `InvalidProtectedSecret`/`Windows` variants below are semantically
/// identical to `SecretError`'s own (same meaning, same fail-closed
/// posture), so a caller already familiar with `secret.rs`'s error
/// taxonomy needs to learn exactly one new variant.
#[derive(Debug)]
pub enum CredentialError {
    Io(std::io::Error),
    MissingProtectedSecret(PathBuf),
    InvalidProtectedSecret,
    /// The file decrypted successfully (a real, validly-DPAPI-protected
    /// blob for *some* purpose-bound credential) but its envelope's
    /// `kind_tag` byte names a different [`CredentialKind`] than the one
    /// the caller asked to load. Never silently treated as
    /// `InvalidProtectedSecret` — a caller must be able to tell "you asked
    /// for the wrong kind" apart from "this file is corrupt".
    KindMismatch {
        expected: CredentialKind,
        found: Option<CredentialKind>,
    },
    Windows(std::io::Error),
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialError::Io(err) => write!(f, "credential-store I/O error: {err}"),
            CredentialError::MissingProtectedSecret(path) => {
                write!(f, "protected credential is missing: {}", path.display())
            }
            CredentialError::InvalidProtectedSecret => {
                write!(f, "protected credential is invalid or corrupted")
            }
            CredentialError::KindMismatch { expected, found } => match found {
                Some(found) => write!(
                    f,
                    "credential kind mismatch: expected {expected}, found {found}"
                ),
                None => write!(
                    f,
                    "credential kind mismatch: expected {expected}, found an unrecognized kind tag"
                ),
            },
            CredentialError::Windows(err) => {
                write!(f, "Windows credential-protection error: {err}")
            }
        }
    }
}

impl std::error::Error for CredentialError {}

impl From<std::io::Error> for CredentialError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Idempotent load-or-create: if `path` already holds a protected
/// credential, loads and returns it (identical to [`load_credential`]); if
/// not, generates a fresh, random 256-bit credential, persists it
/// (atomically — write-temp-then-rename, matching `secret.rs::
/// store_new_secret`'s own convention), and returns the plaintext. This is
/// the **only** function in this module that may create a fresh secret —
/// [`load_credential`] never falls back to generating one.
pub fn mint_or_load_credential(
    kind: CredentialKind,
    path: &Path,
) -> Result<Vec<u8>, CredentialError> {
    if path.exists() {
        return load_credential(kind, path);
    }

    let secret = generate_credential_bytes()?;
    let mut envelope = Vec::with_capacity(2 + secret.len());
    envelope.push(FORMAT_VERSION);
    envelope.push(kind.tag());
    envelope.extend_from_slice(&secret);

    let protect_result = protect(&envelope);
    wipe(&mut envelope);
    let protected = protect_result?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("protected.new");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&protected)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary, path)?;

    Ok(secret)
}

/// Read-only load: fails closed (`MissingProtectedSecret`/
/// `InvalidProtectedSecret`/`KindMismatch`) — never creates a fresh
/// credential, never falls back to a default/empty value.
pub fn load_credential(kind: CredentialKind, path: &Path) -> Result<Vec<u8>, CredentialError> {
    if !path.exists() {
        return Err(CredentialError::MissingProtectedSecret(path.to_path_buf()));
    }
    let mut protected = Vec::new();
    OpenOptions::new()
        .read(true)
        .open(path)?
        .read_to_end(&mut protected)?;
    if protected.is_empty() {
        return Err(CredentialError::InvalidProtectedSecret);
    }

    let mut envelope = unprotect(&mut protected)?;
    if envelope.len() < 2 {
        wipe(&mut envelope);
        return Err(CredentialError::InvalidProtectedSecret);
    }
    let format_version = envelope[0];
    let found_tag = envelope[1];
    if format_version != FORMAT_VERSION {
        wipe(&mut envelope);
        return Err(CredentialError::InvalidProtectedSecret);
    }
    if found_tag != kind.tag() {
        wipe(&mut envelope);
        return Err(CredentialError::KindMismatch {
            expected: kind,
            found: CredentialKind::from_tag(found_tag),
        });
    }

    let secret = envelope[2..].to_vec();
    wipe(&mut envelope);
    Ok(secret)
}

fn generate_credential_bytes() -> Result<Vec<u8>, CredentialError> {
    let mut bytes = vec![0u8; CREDENTIAL_BYTES];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        return Err(CredentialError::Windows(std::io::Error::from_raw_os_error(
            status,
        )));
    }
    Ok(bytes)
}

fn blob(bytes: &mut [u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_mut_ptr(),
    }
}

fn protect(plaintext: &[u8]) -> Result<Vec<u8>, CredentialError> {
    let mut input = plaintext.to_vec();
    let mut entropy = CREDENTIAL_ENTROPY.to_vec();
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptProtectData(
            &blob(&mut input),
            std::ptr::null(),
            &blob(&mut entropy),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    wipe(&mut input);
    if ok == 0 {
        return Err(CredentialError::Windows(std::io::Error::last_os_error()));
    }
    copy_and_free(output)
}

fn unprotect(protected: &mut [u8]) -> Result<Vec<u8>, CredentialError> {
    let mut entropy = CREDENTIAL_ENTROPY.to_vec();
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptUnprotectData(
            &blob(protected),
            std::ptr::null_mut(),
            &blob(&mut entropy),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(CredentialError::Windows(std::io::Error::last_os_error()));
    }
    copy_and_free(output)
}

fn copy_and_free(output: CRYPT_INTEGER_BLOB) -> Result<Vec<u8>, CredentialError> {
    if output.pbData.is_null() {
        return Err(CredentialError::InvalidProtectedSecret);
    }
    let native = unsafe { std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize) };
    let result = native.to_vec();
    wipe(native);
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(result)
}

fn wipe(bytes: &mut [u8]) {
    for byte in bytes {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_then_load_round_trips_the_same_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("desktop.credential.protected");

        let minted = mint_or_load_credential(CredentialKind::DesktopInstallation, &path).unwrap();
        assert_eq!(minted.len(), CREDENTIAL_BYTES);

        let loaded = load_credential(CredentialKind::DesktopInstallation, &path).unwrap();
        assert_eq!(minted, loaded);

        // Never plaintext on disk.
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(minted.len()).any(|w| w == minted.as_slice()));
    }

    #[test]
    fn mint_is_idempotent_across_repeated_calls() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.credential.protected");

        let first = mint_or_load_credential(CredentialKind::AgentConnection, &path).unwrap();
        let second = mint_or_load_credential(CredentialKind::AgentConnection, &path).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn the_two_kinds_mint_distinct_secrets_and_never_collide() {
        let dir = tempfile::tempdir().unwrap();
        let desktop_path = dir.path().join("desktop.credential.protected");
        let agent_path = dir.path().join("agent.credential.protected");

        let desktop =
            mint_or_load_credential(CredentialKind::DesktopInstallation, &desktop_path).unwrap();
        let agent = mint_or_load_credential(CredentialKind::AgentConnection, &agent_path).unwrap();
        assert_ne!(desktop, agent);
    }

    #[test]
    fn loading_the_wrong_kind_against_a_real_protected_file_fails_closed_as_kind_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("desktop.credential.protected");
        mint_or_load_credential(CredentialKind::DesktopInstallation, &path).unwrap();

        let result = load_credential(CredentialKind::AgentConnection, &path);
        match result {
            Err(CredentialError::KindMismatch { expected, found }) => {
                assert_eq!(expected, CredentialKind::AgentConnection);
                assert_eq!(found, Some(CredentialKind::DesktopInstallation));
            }
            other => panic!("expected KindMismatch, got {other:?}"),
        }
    }

    #[test]
    fn loading_a_missing_file_fails_closed_as_missing_not_a_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.protected");
        let result = load_credential(CredentialKind::DesktopInstallation, &path);
        assert!(matches!(
            result,
            Err(CredentialError::MissingProtectedSecret(_))
        ));
    }

    #[test]
    fn loading_a_corrupted_file_fails_closed_as_invalid_or_windows_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("desktop.credential.protected");
        mint_or_load_credential(CredentialKind::DesktopInstallation, &path).unwrap();
        // Overwrite the real protected blob with random bytes of the same
        // rough shape — DPAPI's own integrity check must reject this.
        std::fs::write(&path, [7u8; 64]).unwrap();

        let result = load_credential(CredentialKind::DesktopInstallation, &path);
        assert!(matches!(
            result,
            Err(CredentialError::Windows(_)) | Err(CredentialError::InvalidProtectedSecret)
        ));
    }

    #[test]
    fn loading_an_empty_file_fails_closed_as_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.protected");
        std::fs::write(&path, []).unwrap();
        let result = load_credential(CredentialKind::DesktopInstallation, &path);
        assert!(matches!(
            result,
            Err(CredentialError::InvalidProtectedSecret)
        ));
    }

    #[test]
    fn rotating_one_kind_does_not_disturb_the_other() {
        let dir = tempfile::tempdir().unwrap();
        let desktop_path = dir.path().join("desktop.credential.protected");
        let agent_path = dir.path().join("agent.credential.protected");

        let desktop_original =
            mint_or_load_credential(CredentialKind::DesktopInstallation, &desktop_path).unwrap();
        mint_or_load_credential(CredentialKind::AgentConnection, &agent_path).unwrap();

        // Rotate (delete + re-mint) only the agent credential.
        std::fs::remove_file(&agent_path).unwrap();
        let agent_rotated =
            mint_or_load_credential(CredentialKind::AgentConnection, &agent_path).unwrap();
        assert!(!agent_rotated.is_empty());

        // The untouched desktop credential still loads, byte-for-byte
        // identical to before the agent rotation.
        let desktop_after =
            load_credential(CredentialKind::DesktopInstallation, &desktop_path).unwrap();
        assert_eq!(desktop_original, desktop_after);
    }

    #[test]
    fn credential_kind_as_str_matches_the_ipc_principal_kind_snake_case_rendering() {
        assert_eq!(
            CredentialKind::DesktopInstallation.as_str(),
            "desktop_installation"
        );
        assert_eq!(CredentialKind::AgentConnection.as_str(), "agent_connection");
    }
}
