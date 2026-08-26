#![cfg(windows)]

use loki_runtime::host::compose::{ComposedHost, HostConfig, HostStartError};
use loki_runtime::host::paths::HostPaths;
use loki_storage::{ProfileError, SecureMemoryError};

#[test]
fn missing_accepted_profile_fails_before_any_database_key_or_sidecar_exists() {
    let root = tempfile::tempdir().expect("create isolated L:-rooted test directory");
    let base = root.path().join("must-not-be-created");
    let paths = HostPaths::new(&base);
    let database = paths.database_file();
    let key = paths.database_key_file();
    let token = paths.protected_token_file();
    let discovery = paths.discovery_file();

    let error = match ComposedHost::open(&HostConfig {
        paths,
        pipe_name: format!(
            r"\\.\pipe\loki-secure-memory-fail-closed-{}",
            uuid::Uuid::new_v4()
        ),
    }) {
        Ok(_) => panic!("no unmeasured profile may start production storage"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        HostStartError::SecureMemory(SecureMemoryError::Profile(ProfileError::Unavailable))
    ));
    assert!(
        !base.exists(),
        "arena failure must precede base-dir creation"
    );
    assert!(!database.exists(), "arena failure must touch no database");
    assert!(!key.exists(), "arena failure must touch no key sidecar");
    assert!(
        !token.exists(),
        "arena failure must touch no protected token"
    );
    assert!(
        !discovery.exists(),
        "arena failure must touch no discovery sidecar"
    );
}
