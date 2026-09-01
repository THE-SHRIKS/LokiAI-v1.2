//! Mints/loads the `DesktopInstallation`/`AgentConnection` credentials
//! (`P1-NODE-IPC-CLIENT-001`, `ADR-0009`) alongside the existing legacy IPC
//! token (`crate::host::token`), via `loki_storage::credential`'s
//! purpose-bound DPAPI convention.
//!
//! **Additive only.** These two credentials are persisted to disk here but
//! are **not** wired into the live `Server`'s `CredentialResolver` in this
//! node — see `crate::host::compose`'s call site and
//! `Docs/graph/tasks/P1-NODE-IPC-CLIENT-001.md` §1/§9 for exactly why
//! (production wiring needs a real per-installation/per-agent-connection
//! `Authority`/`authorized_entity_ids` provisioning policy this node's
//! packet explicitly does not decide).

use std::io;

use loki_storage::credential::{self, CredentialError, CredentialKind};

use crate::host::paths::HostPaths;

/// Either credential's minting/loading failed. Distinguishes which kind so
/// a startup failure log names the actual offending file, not a generic
/// "credential error".
#[derive(Debug)]
pub enum HostCredentialError {
    DesktopInstallation(CredentialError),
    AgentConnection(CredentialError),
}

impl std::fmt::Display for HostCredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostCredentialError::DesktopInstallation(err) => {
                write!(f, "desktop-installation credential error: {err}")
            }
            HostCredentialError::AgentConnection(err) => {
                write!(f, "agent-connection credential error: {err}")
            }
        }
    }
}

impl std::error::Error for HostCredentialError {}

impl From<HostCredentialError> for io::Error {
    fn from(value: HostCredentialError) -> Self {
        io::Error::other(value)
    }
}

/// Mints (idempotent) or loads both new purpose-bound credentials against
/// `paths`' new file helpers, then immediately wipes each plaintext copy
/// this function itself holds before returning — the protected file is
/// already durably written by `loki_storage::credential::
/// mint_or_load_credential` before this function ever sees the plaintext,
/// so nothing here holds either secret any longer than the moment needed to
/// zero it. Never logs, never returns, and never otherwise exposes the
/// plaintext to any caller.
pub fn mint_or_load_both(paths: &HostPaths) -> Result<(), HostCredentialError> {
    let mut desktop_installation = credential::mint_or_load_credential(
        CredentialKind::DesktopInstallation,
        &paths.desktop_installation_credential_file(),
    )
    .map_err(HostCredentialError::DesktopInstallation)?;
    wipe(&mut desktop_installation);

    let mut agent_connection = credential::mint_or_load_credential(
        CredentialKind::AgentConnection,
        &paths.agent_connection_credential_file(),
    )
    .map_err(HostCredentialError::AgentConnection)?;
    wipe(&mut agent_connection);

    Ok(())
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
    fn mint_or_load_both_persists_two_distinct_independently_loadable_credential_files() {
        let dir = tempfile::tempdir().unwrap();
        let paths = HostPaths::new(dir.path());

        mint_or_load_both(&paths).expect("first mint must succeed");
        assert!(paths.desktop_installation_credential_file().exists());
        assert!(paths.agent_connection_credential_file().exists());

        let desktop = credential::load_credential(
            CredentialKind::DesktopInstallation,
            &paths.desktop_installation_credential_file(),
        )
        .expect("desktop-installation credential must load back");
        let agent = credential::load_credential(
            CredentialKind::AgentConnection,
            &paths.agent_connection_credential_file(),
        )
        .expect("agent-connection credential must load back");
        assert_ne!(desktop, agent);

        // Idempotent: calling again must not change either file's content.
        mint_or_load_both(&paths).expect("second call must succeed and be a no-op mint");
        let desktop_again = credential::load_credential(
            CredentialKind::DesktopInstallation,
            &paths.desktop_installation_credential_file(),
        )
        .unwrap();
        assert_eq!(desktop, desktop_again);
    }
}
