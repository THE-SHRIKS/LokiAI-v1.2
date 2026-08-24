//! On-disk layout for one background-host instance's local state.
//!
//! Everything lives under one base directory: the `DAT-001` SQLite database,
//! the desktop-discovery file (`src/host/discovery.rs`), and the interim IPC
//! token file (`src/host/token.rs`). [`HostPaths::default_for_current_user`]
//! resolves the production location; tests construct [`HostPaths::new`]
//! directly against a temporary directory so no test ever touches a real
//! user profile.

use std::io;
use std::path::{Path, PathBuf};

/// A single background host instance's on-disk file layout, rooted at
/// `base_dir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPaths {
    base_dir: PathBuf,
}

impl HostPaths {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// `%LOCALAPPDATA%\LokiAI\runtime` — per-user by construction (every
    /// Windows user account has its own `LOCALAPPDATA`), matching the
    /// `desktop-user` host mode `Docs/ARCHITECTURE.md` §3.2 describes ("a
    /// separate per-user background process ... may survive window closure
    /// and start at login").
    pub fn default_for_current_user() -> io::Result<Self> {
        let local_app_data = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "LOCALAPPDATA is not set; cannot resolve the per-user runtime data directory",
            )
        })?;
        Ok(Self::new(
            PathBuf::from(local_app_data).join("LokiAI").join("runtime"),
        ))
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// The desktop-discovery file — see `src/host/discovery.rs`.
    pub fn discovery_file(&self) -> PathBuf {
        self.base_dir.join("host.json")
    }

    /// The interim local IPC shared-secret file — see `src/host/token.rs`.
    pub fn token_file(&self) -> PathBuf {
        self.base_dir.join("ipc.token")
    }

    pub fn protected_token_file(&self) -> PathBuf {
        self.base_dir.join("ipc.token.protected")
    }

    pub fn database_key_file(&self) -> PathBuf {
        self.base_dir.join("database.key.protected")
    }

    /// The `DAT-001` SQLite database this host instance owns.
    pub fn database_file(&self) -> PathBuf {
        self.base_dir.join("loki-runtime.sqlite3")
    }

    /// Creates `base_dir` (and any missing parents) if it does not already
    /// exist. Idempotent.
    pub fn ensure_base_dir(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.base_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_paths_are_nested_under_base_dir() {
        let paths = HostPaths::new(r"C:\tmp\lokiai-test");
        assert_eq!(
            paths.discovery_file(),
            PathBuf::from(r"C:\tmp\lokiai-test\host.json")
        );
        assert_eq!(
            paths.token_file(),
            PathBuf::from(r"C:\tmp\lokiai-test\ipc.token")
        );
        assert_eq!(
            paths.protected_token_file(),
            PathBuf::from(r"C:\tmp\lokiai-test\ipc.token.protected")
        );
        assert_eq!(
            paths.database_key_file(),
            PathBuf::from(r"C:\tmp\lokiai-test\database.key.protected")
        );
        assert_eq!(
            paths.database_file(),
            PathBuf::from(r"C:\tmp\lokiai-test\loki-runtime.sqlite3")
        );
    }

    #[test]
    fn ensure_base_dir_creates_missing_nested_directories() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("a").join("b").join("c");
        let paths = HostPaths::new(&nested);
        assert!(!nested.exists());
        paths.ensure_base_dir().unwrap();
        assert!(nested.is_dir());
    }

    #[test]
    fn ensure_base_dir_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let paths = HostPaths::new(root.path());
        paths.ensure_base_dir().unwrap();
        paths.ensure_base_dir().unwrap();
        assert!(root.path().is_dir());
    }

    #[test]
    fn default_for_current_user_is_rooted_under_local_app_data() {
        // This test relies on LOCALAPPDATA being set, which every real
        // Windows session (including CI runners) sets; if it is ever
        // genuinely absent, the honest `io::Error` path is exercised
        // instead and this assertion is skipped rather than failing for an
        // unrelated environment reason.
        let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let paths = HostPaths::default_for_current_user().unwrap();
        assert!(paths.base_dir().starts_with(PathBuf::from(local_app_data)));
        assert!(
            paths
                .base_dir()
                .ends_with(PathBuf::from("LokiAI").join("runtime"))
        );
    }
}
