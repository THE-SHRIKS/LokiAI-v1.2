//! Crash-recoverable SQLCipher open, plaintext migration, and key rotation.
//! The source file remains in place until an encrypted replacement has passed
//! integrity verification. A durable marker lets the next startup finish or
//! roll back an interrupted swap.

use std::fs::{OpenOptions, remove_file, rename};
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::StorageError;
use crate::key::DatabaseKey;
use crate::secret::{self, SecretError};
use crate::storage::{configure_connection, configure_sqlcipher};

struct OperationPaths {
    replacement: PathBuf,
    rollback: PathBuf,
    marker: PathBuf,
    pending_key: PathBuf,
    failed: PathBuf,
}

impl OperationPaths {
    fn new(database: &Path, key: &Path) -> Self {
        Self {
            replacement: database.with_extension("encryption.new"),
            rollback: database.with_extension("encryption.rollback"),
            marker: database.with_extension("encryption.operation"),
            pending_key: key.with_extension("migration.pending"),
            failed: database.with_extension("encryption.failed"),
        }
    }
}

/// Opens an encrypted database, creating it on first use or safely migrating
/// an existing plaintext SQLite file. Existing encrypted data without its
/// protected key is refused unless a durable in-progress marker proves that
/// the matching pending key belongs to an interrupted LokiAI operation.
pub fn open_or_migrate_sqlcipher(
    database: &Path,
    protected_key: &Path,
) -> Result<(Connection, DatabaseKey), StorageError> {
    // This guard deliberately precedes every filesystem probe, DPAPI key
    // load/create, and SQLite symbol.  A missing/unaccepted/unattested arena
    // therefore cannot create a healthy-looking empty database or sidecar.
    crate::secure_memory::require_ready()?;
    let paths = OperationPaths::new(database, protected_key);
    recover_interrupted(database, protected_key, &paths)?;

    if protected_key.exists() {
        let key = load_key(protected_key)?;
        return open_verified(database, key);
    }

    if !database.exists() {
        let key = secret::load_or_create_database_key(protected_key, false)?;
        return open_verified(database, key);
    }

    migrate_plaintext(database, protected_key, &paths)
}

/// Re-encrypts a database under a fresh DPAPI-protected key with the same
/// verified replacement/rollback protocol used by plaintext migration.
pub fn rotate_sqlcipher_key(database: &Path, protected_key: &Path) -> Result<(), StorageError> {
    let paths = OperationPaths::new(database, protected_key);
    recover_interrupted(database, protected_key, &paths)?;
    if !database.exists() {
        return Err(StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "database to rotate does not exist",
        )));
    }
    let old_key = load_key(protected_key)?;
    let source = open_with_key(database, &old_key)?;
    replace_from_source(database, protected_key, &paths, source, "rotation")?;
    Ok(())
}

/// Creates an integrity-checked encrypted backup using the database's current
/// protected key. The destination must not already exist.
pub fn backup_sqlcipher_database(
    database: &Path,
    protected_key: &Path,
    destination: &Path,
) -> Result<(), StorageError> {
    if destination.exists() {
        return Err(StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "refusing to overwrite an existing backup",
        )));
    }
    let key = load_key(protected_key)?;
    let source = open_with_key(database, &key)?;
    export_to_encrypted(&source, destination, &key)?;
    let verified = open_with_key(destination, &key)?;
    drop(verified);
    Ok(())
}

fn migrate_plaintext(
    database: &Path,
    protected_key: &Path,
    paths: &OperationPaths,
) -> Result<(Connection, DatabaseKey), StorageError> {
    let source = Connection::open(database)?;
    verify_integrity(&source, database)?;
    replace_from_source(
        database,
        protected_key,
        paths,
        source,
        "plaintext-migration",
    )?;
    let key = load_key(protected_key)?;
    open_verified(database, key)
}

fn replace_from_source(
    database: &Path,
    protected_key: &Path,
    paths: &OperationPaths,
    source: Connection,
    operation: &str,
) -> Result<(), StorageError> {
    refuse_stale_targets(paths)?;
    write_marker(&paths.marker, operation)?;

    let new_key = secret::generate_database_key()?;
    secret::store_database_key(&paths.pending_key, &new_key)?;
    let result = (|| {
        export_to_encrypted(&source, &paths.replacement, &new_key)?;
        drop(source);
        let destination = open_with_key(&paths.replacement, &new_key)?;
        verify_integrity(&destination, &paths.replacement)?;
        drop(destination);

        rename(database, &paths.rollback)?;
        if let Err(error) = rename(&paths.replacement, database) {
            let _ = rename(&paths.rollback, database);
            return Err(StorageError::Io(error));
        }
        if let Err(error) = rename(&paths.pending_key, protected_key) {
            let _ = rename(database, &paths.failed);
            let _ = rename(&paths.rollback, database);
            return Err(StorageError::Io(error));
        }

        let verification_key = load_key(protected_key)?;
        let verified = open_with_key(database, &verification_key)?;
        drop(verified);
        remove_if_exists(&paths.rollback)?;
        remove_if_exists(&paths.marker)?;
        Ok(())
    })();

    if result.is_err() {
        // Preserve the original whenever the swap did not complete. Recovery
        // on the next open handles the narrow crash windows between renames.
        remove_if_exists(&paths.replacement).ok();
        remove_if_exists(&paths.pending_key).ok();
        if !database.exists() && paths.rollback.exists() {
            rename(&paths.rollback, database).ok();
        }
        remove_if_exists(&paths.marker).ok();
    }
    result
}

fn export_to_encrypted(
    source: &Connection,
    destination: &Path,
    key: &DatabaseKey,
) -> Result<(), StorageError> {
    let destination_text = destination.to_str().ok_or_else(|| {
        StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "database path is not valid Unicode",
        ))
    })?;
    // The filename is bound, not interpolated. Only the in-memory raw key is
    // placed in the SQLCipher-specific KEY literal and the statement is never
    // logged. SQLite's Online Backup API rejects cross-codec copies.
    let hex = key.sqlcipher_hex();
    let mut attach = format!(
        "ATTACH DATABASE ?1 AS lokiai_encrypted KEY \"x'{}'\"",
        hex.as_str()
    );
    let attach_result = source.execute(&attach, [destination_text]);
    crate::storage::wipe_string(&mut attach);
    drop(hex);
    attach_result?;
    let export_result =
        source.query_row(
            "SELECT sqlcipher_export('lokiai_encrypted')",
            [],
            |_| Ok(()),
        );
    let detach_result = source.execute_batch("DETACH DATABASE lokiai_encrypted");
    export_result?;
    detach_result?;
    Ok(())
}

fn recover_interrupted(
    database: &Path,
    protected_key: &Path,
    paths: &OperationPaths,
) -> Result<(), StorageError> {
    if !paths.marker.exists() {
        return Ok(());
    }

    if protected_key.exists() && database.exists() {
        let key = load_key(protected_key)?;
        if open_with_key(database, &key).is_ok() {
            cleanup_operation(paths)?;
            return Ok(());
        }
    }

    if paths.pending_key.exists() && database.exists() {
        let key = load_key(&paths.pending_key)?;
        if open_with_key(database, &key).is_ok() {
            rename(&paths.pending_key, protected_key)?;
            cleanup_operation(paths)?;
            return Ok(());
        }
    }

    if paths.rollback.exists() {
        if database.exists() {
            remove_if_exists(&paths.failed)?;
            rename(database, &paths.failed)?;
        }
        rename(&paths.rollback, database)?;
    }
    remove_if_exists(&paths.replacement)?;
    remove_if_exists(&paths.pending_key)?;
    remove_if_exists(&paths.marker)?;
    Ok(())
}

fn cleanup_operation(paths: &OperationPaths) -> Result<(), StorageError> {
    remove_if_exists(&paths.rollback)?;
    remove_if_exists(&paths.replacement)?;
    remove_if_exists(&paths.pending_key)?;
    remove_if_exists(&paths.marker)?;
    Ok(())
}

fn refuse_stale_targets(paths: &OperationPaths) -> Result<(), StorageError> {
    for path in [&paths.replacement, &paths.rollback, &paths.pending_key] {
        if path.exists() {
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("refusing to overwrite recovery artifact {}", path.display()),
            )));
        }
    }
    Ok(())
}

fn write_marker(path: &Path, operation: &str) -> Result<(), StorageError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    writeln!(file, "version=1")?;
    writeln!(file, "operation={operation}")?;
    file.sync_all()?;
    Ok(())
}

fn open_verified(
    database: &Path,
    key: DatabaseKey,
) -> Result<(Connection, DatabaseKey), StorageError> {
    let connection = open_with_key(database, &key)?;
    configure_connection(&connection)?;
    Ok((connection, key))
}

fn open_with_key(database: &Path, key: &DatabaseKey) -> Result<Connection, StorageError> {
    let connection = Connection::open(database)?;
    configure_sqlcipher(&connection, key)?;
    verify_integrity(&connection, database)?;
    Ok(connection)
}

fn verify_integrity(connection: &Connection, path: &Path) -> Result<(), StorageError> {
    let result = connection.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0));
    match result {
        Ok(value) if value == "ok" => Ok(()),
        Ok(value) => Err(StorageError::Corrupt {
            path: path.display().to_string(),
            detail: value,
        }),
        Err(error) => Err(StorageError::Corrupt {
            path: path.display().to_string(),
            detail: error.to_string(),
        }),
    }
}

fn load_key(path: &Path) -> Result<DatabaseKey, StorageError> {
    let plaintext = secret::load_secret(path)?;
    // `DatabaseKey::from_plaintext_vec` wipes `plaintext`'s backing
    // allocation before it is dropped, unlike a raw `.try_into::<[u8;
    // 32]>()`, which would leave the DPAPI-unprotected plaintext key in
    // freed, unwiped heap memory.
    DatabaseKey::from_plaintext_vec(plaintext)
        .ok_or(StorageError::Secret(SecretError::InvalidProtectedSecret))
}

fn remove_if_exists(path: &Path) -> Result<(), StorageError> {
    match remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StorageError::Io(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_pre_swap_restores_source_then_retries_migration() {
        // `open_or_migrate_sqlcipher` requires arena readiness (deliverable
        // 6/acceptance criterion 1). Production never bootstraps this
        // process-global arena more than once, so this crate's own tests
        // share one disposable, non-production test-only arena instead of
        // each asserting a fresh process — see `secure_memory::test_support`.
        crate::secure_memory::test_support::ensure_test_arena_ready();
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("recover.sqlite3");
        let key = dir.path().join("recover.key.protected");
        let paths = OperationPaths::new(&database, &key);
        let plaintext = Connection::open(&database).unwrap();
        plaintext
            .execute_batch(
                "CREATE TABLE preserved(value TEXT); INSERT INTO preserved VALUES ('yes');",
            )
            .unwrap();
        drop(plaintext);
        write_marker(&paths.marker, "plaintext-migration").unwrap();
        rename(&database, &paths.rollback).unwrap();

        let (connection, _key) = open_or_migrate_sqlcipher(&database, &key).unwrap();
        let value: String = connection
            .query_row("SELECT value FROM preserved", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "yes");
        assert!(!paths.marker.exists());
        assert!(!paths.rollback.exists());
    }

    #[test]
    fn interrupted_key_promotion_is_completed_only_after_key_verification() {
        crate::secure_memory::test_support::ensure_test_arena_ready();
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("recover.sqlite3");
        let key = dir.path().join("recover.key.protected");
        let paths = OperationPaths::new(&database, &key);
        let (connection, _database_key) = open_or_migrate_sqlcipher(&database, &key).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE preserved(value TEXT); INSERT INTO preserved VALUES ('yes');",
            )
            .unwrap();
        drop(connection);
        rename(&key, &paths.pending_key).unwrap();
        write_marker(&paths.marker, "rotation").unwrap();

        let (reopened, _key) = open_or_migrate_sqlcipher(&database, &key).unwrap();
        let value: String = reopened
            .query_row("SELECT value FROM preserved", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "yes");
        assert!(key.exists());
        assert!(!paths.pending_key.exists());
        assert!(!paths.marker.exists());
    }
}
