#![cfg(windows)]

use std::fs;

use loki_storage::{backup_sqlcipher_database, open_or_migrate_sqlcipher, rotate_sqlcipher_key};
use rusqlite::Connection;

const MARKER: &str = "LOKIAI_DAT002_CONFIDENTIAL_MARKER_7d9b";

/// `open_or_migrate_sqlcipher` requires the process-global secure-memory
/// arena to be ready first (DAT-002-MEM-CODE-001, ADR-0005 deliverable 6).
/// Production never embeds an accepted bounds profile at this revision
/// (fail-closed by design), so this binary bootstraps one disposable,
/// non-production test-only arena instead — see
/// `loki_storage::secure_memory::test_support` for why this bypass cannot
/// reach a release build.
fn ensure_ready() {
    loki_storage::secure_memory::test_support::ensure_test_arena_ready();
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn plaintext_migration_preserves_rows_and_removes_plaintext_artifacts() {
    ensure_ready();
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("core.sqlite3");
    let key_path = dir.path().join("core.key.protected");

    let plaintext = Connection::open(&database).unwrap();
    plaintext
        .execute_batch("CREATE TABLE secrets(value TEXT NOT NULL);")
        .unwrap();
    plaintext
        .execute("INSERT INTO secrets(value) VALUES (?1)", [MARKER])
        .unwrap();
    drop(plaintext);
    assert!(contains(&fs::read(&database).unwrap(), MARKER.as_bytes()));

    let (encrypted, _key) = open_or_migrate_sqlcipher(&database, &key_path).unwrap();
    let value: String = encrypted
        .query_row("SELECT value FROM secrets", [], |row| row.get(0))
        .unwrap();
    assert_eq!(value, MARKER);
    encrypted
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    drop(encrypted);

    assert!(!contains(&fs::read(&database).unwrap(), MARKER.as_bytes()));
    assert!(!database.with_extension("encryption.rollback").exists());
    assert!(!database.with_extension("encryption.operation").exists());
    let unkeyed = Connection::open(&database).unwrap();
    assert!(
        unkeyed
            .query_row("SELECT value FROM secrets", [], |row| row
                .get::<_, String>(0))
            .is_err()
    );
}

#[test]
fn missing_or_wrong_key_never_replaces_the_existing_database() {
    ensure_ready();
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("core.sqlite3");
    let key_path = dir.path().join("core.key.protected");
    let (connection, _key) = open_or_migrate_sqlcipher(&database, &key_path).unwrap();
    connection
        .execute_batch("CREATE TABLE protected(value TEXT NOT NULL);")
        .unwrap();
    connection
        .execute("INSERT INTO protected(value) VALUES (?1)", [MARKER])
        .unwrap();
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    drop(connection);
    let original = fs::read(&database).unwrap();

    let saved_key = dir.path().join("saved.key");
    fs::copy(&key_path, &saved_key).unwrap();
    fs::remove_file(&key_path).unwrap();
    assert!(open_or_migrate_sqlcipher(&database, &key_path).is_err());
    assert_eq!(fs::read(&database).unwrap(), original);

    fs::copy(&saved_key, &key_path).unwrap();
    let other_database = dir.path().join("other.sqlite3");
    let other_key = dir.path().join("other.key.protected");
    drop(open_or_migrate_sqlcipher(&other_database, &other_key).unwrap());
    fs::copy(&other_key, &key_path).unwrap();
    assert!(open_or_migrate_sqlcipher(&database, &key_path).is_err());
    assert_eq!(fs::read(&database).unwrap(), original);
}

#[test]
fn key_rotation_preserves_data_and_changes_the_protected_key() {
    ensure_ready();
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("core.sqlite3");
    let key_path = dir.path().join("core.key.protected");
    let (connection, _key) = open_or_migrate_sqlcipher(&database, &key_path).unwrap();
    connection
        .execute_batch("CREATE TABLE protected(value TEXT NOT NULL);")
        .unwrap();
    connection
        .execute("INSERT INTO protected(value) VALUES (?1)", [MARKER])
        .unwrap();
    drop(connection);
    let old_protected_key = fs::read(&key_path).unwrap();

    rotate_sqlcipher_key(&database, &key_path).unwrap();
    assert_ne!(fs::read(&key_path).unwrap(), old_protected_key);
    let (reopened, _key) = open_or_migrate_sqlcipher(&database, &key_path).unwrap();
    let value: String = reopened
        .query_row("SELECT value FROM protected", [], |row| row.get(0))
        .unwrap();
    assert_eq!(value, MARKER);
    drop(reopened);
    assert!(!contains(&fs::read(&database).unwrap(), MARKER.as_bytes()));
}

#[test]
fn dpapi_blob_does_not_contain_the_plaintext_secret() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token.protected");
    loki_storage::secret::store_new_secret(&path, MARKER.as_bytes()).unwrap();
    assert!(!contains(&fs::read(&path).unwrap(), MARKER.as_bytes()));
    assert_eq!(
        loki_storage::secret::load_secret(&path).unwrap(),
        MARKER.as_bytes()
    );
}

/// CODE-REV-001 CRITICAL-2: `cipher_memory_security` must actually be
/// verified ON after a real key configuration and a forced allocation, not
/// merely requested. `open_or_migrate_sqlcipher` is the real production
/// entrypoint (unlike `storage.rs`'s in-crate unit-test coverage of
/// `configure_sqlcipher` directly), so this proves the guarantee holds
/// end-to-end through the path production actually uses.
#[test]
fn cipher_memory_security_is_verified_on_end_to_end_after_forced_allocation() {
    ensure_ready();
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("core.sqlite3");
    let key_path = dir.path().join("core.key.protected");
    let (connection, _key) = open_or_migrate_sqlcipher(&database, &key_path).unwrap();

    connection
        .execute_batch("CREATE TABLE forced(value TEXT); INSERT INTO forced VALUES ('x');")
        .unwrap();
    let _: String = connection
        .query_row("SELECT value FROM forced", [], |row| row.get(0))
        .unwrap();

    let reported: String = connection
        .query_row("PRAGMA cipher_memory_security;", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        reported, "1",
        "cipher_memory_security must read back ON (1) after forced allocation"
    );
}

#[test]
fn managed_backup_is_encrypted_integrity_checked_and_readable() {
    ensure_ready();
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("core.sqlite3");
    let backup = dir.path().join("core.backup.sqlite3");
    let key_path = dir.path().join("core.key.protected");
    let (connection, _key) = open_or_migrate_sqlcipher(&database, &key_path).unwrap();
    connection
        .execute_batch("CREATE TABLE protected(value TEXT NOT NULL);")
        .unwrap();
    connection
        .execute("INSERT INTO protected(value) VALUES (?1)", [MARKER])
        .unwrap();
    drop(connection);

    backup_sqlcipher_database(&database, &key_path, &backup).unwrap();
    assert!(!contains(&fs::read(&backup).unwrap(), MARKER.as_bytes()));
    let (reopened, _key) = open_or_migrate_sqlcipher(&backup, &key_path).unwrap();
    let value: String = reopened
        .query_row("SELECT value FROM protected", [], |row| row.get(0))
        .unwrap();
    assert_eq!(value, MARKER);
}
