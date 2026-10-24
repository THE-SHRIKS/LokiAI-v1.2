#![cfg(windows)]

use loki_storage::{
    ProfileError, SecureMemoryError, SecureMemoryRuntime, Storage, StorageError,
    digest_profile_manifest, inspect_profile,
};

const VALID: &[u8] = b"version=1\narena_bytes=4096\nworking_set_min_bytes=8192\nworking_set_max_bytes=16384\nconnection_cap=1\ncache_pages_per_connection=1\nsource_sha256=0000000000000000000000000000000000000000000000000000000000000000\nbuild_sha256=1111111111111111111111111111111111111111111111111111111111111111\naccepted_by=test-reviewer\naccepted_at_utc=2026-08-24T00:00:00Z\n";

#[test]
fn missing_profile_cannot_make_production_storage_ready() {
    assert!(matches!(
        SecureMemoryRuntime::bootstrap(),
        Err(SecureMemoryError::Profile(ProfileError::Unavailable))
    ));
}

#[test]
fn tampered_profile_is_rejected_by_digest() {
    let accepted_digest = digest_profile_manifest(VALID);
    let mut tampered = VALID.to_vec();
    tampered[20] ^= 1;
    assert!(matches!(
        inspect_profile(&tampered, accepted_digest),
        Err(ProfileError::DigestMismatch)
    ));
}

#[test]
fn structurally_oversized_profile_is_rejected() {
    let oversized = vec![b'x'; 4_097];
    let digest = digest_profile_manifest(&oversized);
    assert!(matches!(
        inspect_profile(&oversized, digest),
        Err(ProfileError::Oversized)
    ));
}

#[test]
fn inspected_profile_is_not_an_operational_acceptance_token() {
    let inspection = inspect_profile(VALID, digest_profile_manifest(VALID))
        .expect("candidate profile is structurally valid");
    assert_eq!(inspection.arena_bytes, 4_096);
    assert!(matches!(
        SecureMemoryRuntime::bootstrap(),
        Err(SecureMemoryError::Profile(ProfileError::Unavailable))
    ));
}

#[test]
fn direct_managed_open_is_unavailable_before_key_or_database_touch() {
    let root = tempfile::tempdir().expect("create isolated test root");
    let database = root.path().join("must-not-exist.sqlite3");
    let key = root.path().join("must-not-exist.key.protected");
    assert!(matches!(
        Storage::open_managed(&database, &key),
        Err(StorageError::SecureMemory(SecureMemoryError::NotReady))
    ));
    assert!(!database.exists());
    assert!(!key.exists());
}
