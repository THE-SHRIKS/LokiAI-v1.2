//! `P1-NODE-IPC-CLIENT-001` / `ADR-0009` (Option A, accepted): the one
//! shared native Node addon that reads (never mints) the two purpose-bound
//! DPAPI credentials `loki_storage::credential` writes.
//!
//! # Least-privilege boundary
//!
//! Both exported functions are **read-only from Node's perspective** — this
//! crate does not expose `mint_or_load_credential` at all, only
//! `load_credential`. Minting stays a Rust-runtime-owned operation
//! (`crates/loki-runtime/src/host/credential.rs`). A compromised or buggy
//! Node-side consumer of this addon can at most read an existing credential
//! it is pointed at; it can never forge/mint a new one the runtime would
//! accept.
//!
//! # No plaintext outside an in-process `Buffer`
//!
//! Each function returns the raw decrypted secret bytes directly into the
//! caller's own N-API `Buffer`, entirely in-process — never via a temp
//! file, stdout, or environment variable (`ADR-0009` forbids exactly this
//! class of leak). Errors are specific and typed (see [`map_error`]) —
//! never a bare generic `Error` — with the plaintext-relevant detail
//! limited to path/kind names, never secret bytes.

use std::path::PathBuf;

use loki_storage::credential::{self, CredentialError, CredentialKind};
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// Maps [`CredentialError`] to a specific, machine-distinguishable N-API
/// error. napi-rs surfaces a thrown `Error`'s message to JS as-is; this
/// crate encodes the specific reason as a stable, parseable prefix
/// (`CREDENTIAL_MISSING:`/`CREDENTIAL_CORRUPT:`/
/// `CREDENTIAL_KIND_MISMATCH:`/`CREDENTIAL_WINDOWS_ERROR:`) so
/// `packages/native-credential-store`'s TypeScript wrapper can re-throw the
/// matching typed JS error class
/// (`CredentialMissingError`/`CredentialCorruptError`/
/// `CredentialKindMismatchError`/`CredentialWindowsError`) instead of a bare
/// generic `Error` — the "fail closed on the *specific* reason" property
/// the task packet requires. Never includes the decrypted secret itself
/// (there is none to include at an error path).
fn map_error(kind: CredentialKind, err: CredentialError) -> Error {
    let (code, detail): (&str, String) = match &err {
        CredentialError::MissingProtectedSecret(path) => (
            "CREDENTIAL_MISSING",
            format!("protected {kind} credential is missing: {}", path.display()),
        ),
        CredentialError::InvalidProtectedSecret => (
            "CREDENTIAL_CORRUPT",
            format!("protected {kind} credential is invalid or corrupted"),
        ),
        CredentialError::KindMismatch { expected, found } => (
            "CREDENTIAL_KIND_MISMATCH",
            format!(
                "expected a {expected} credential, found {}",
                found
                    .map(|found| found.to_string())
                    .unwrap_or_else(|| "an unrecognized credential kind".to_string())
            ),
        ),
        CredentialError::Windows(inner) => (
            "CREDENTIAL_WINDOWS_ERROR",
            format!("Windows credential-protection error loading {kind} credential: {inner}"),
        ),
        CredentialError::Io(inner) => (
            "CREDENTIAL_WINDOWS_ERROR",
            format!("I/O error loading {kind} credential: {inner}"),
        ),
    };
    Error::new(Status::GenericFailure, format!("{code}: {detail}"))
}

fn load(kind: CredentialKind, protected_path: String) -> Result<Buffer> {
    let path = PathBuf::from(protected_path);
    let secret = credential::load_credential(kind, &path).map_err(|err| map_error(kind, err))?;
    Ok(Buffer::from(secret))
}

/// Reads and decrypts the `DesktopInstallation` credential at
/// `protected_path`. Fails closed (a specific, typed, distinguishable
/// error — see [`map_error`]) on a missing, corrupt, or wrong-kind file;
/// never returns a default/empty credential.
#[napi]
pub fn load_desktop_installation_credential(protected_path: String) -> Result<Buffer> {
    load(CredentialKind::DesktopInstallation, protected_path)
}

/// Reads and decrypts the `AgentConnection` credential at `protected_path`.
/// Same fail-closed contract as
/// [`load_desktop_installation_credential`].
#[napi]
pub fn load_agent_connection_credential(protected_path: String) -> Result<Buffer> {
    load(CredentialKind::AgentConnection, protected_path)
}
