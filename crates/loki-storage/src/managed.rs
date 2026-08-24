//! Fail-safe SQLCipher open, migration, backup, rotation, and recovery.
//! Every destructive transition retains an independently verified encrypted
//! database/key pair until the promoted pair is verified.

use std::fs::{OpenOptions, remove_file, rename};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_SHARE_DELETE, FILE_SHARE_READ, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
    LockFileEx, UnlockFileEx,
};
use windows_sys::Win32::System::IO::OVERLAPPED;

use crate::error::StorageError;
use crate::key::DatabaseKey;
use crate::secret;
use crate::storage::{configure_connection, configure_sqlcipher, verify_cipher_integrity};

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
const MIN_DATABASE_BYTES: u64 = 512;

#[derive(Debug)]
struct OperationPaths {
    replacement: PathBuf,
    rollback: PathBuf,
    marker: PathBuf,
    pending_key: PathBuf,
    rollback_key: PathBuf,
    failed: PathBuf,
    failed_key: PathBuf,
    recovery: PathBuf,
    recovery_key: PathBuf,
    recovery_next: PathBuf,
    recovery_next_key: PathBuf,
    recovery_previous: PathBuf,
    recovery_previous_key: PathBuf,
    maintenance_lock: PathBuf,
}

impl OperationPaths {
    fn new(database: &Path, key: &Path) -> Self {
        Self {
            replacement: database.with_extension("encryption.new"),
            rollback: database.with_extension("encryption.rollback"),
            marker: database.with_extension("encryption.operation"),
            pending_key: key.with_extension("migration.pending"),
            rollback_key: key.with_extension("rotation.rollback"),
            failed: database.with_extension("encryption.failed"),
            failed_key: key.with_extension("encryption.failed"),
            recovery: database.with_extension("recovery.sqlite3"),
            recovery_key: key.with_extension("recovery.protected"),
            recovery_next: database.with_extension("recovery.next.sqlite3"),
            recovery_next_key: key.with_extension("recovery.next.protected"),
            recovery_previous: database.with_extension("recovery.previous.sqlite3"),
            recovery_previous_key: key.with_extension("recovery.previous.protected"),
            maintenance_lock: database.with_extension("maintenance.lock"),
        }
    }

    fn lifecycle_evidence_exists(&self) -> bool {
        [
            &self.replacement,
            &self.rollback,
            &self.marker,
            &self.pending_key,
            &self.rollback_key,
            &self.failed,
            &self.failed_key,
            &self.recovery,
            &self.recovery_key,
            &self.recovery_next,
            &self.recovery_next_key,
            &self.recovery_previous,
            &self.recovery_previous_key,
        ]
        .iter()
        .any(|path| path.exists())
    }
}

/// Opens an encrypted database. First use is allowed only when both database
/// and key are absent. Every other missing/truncated/key failure is explicit.
pub fn open_or_migrate_sqlcipher(
    database: &Path,
    protected_key: &Path,
) -> Result<(Connection, DatabaseKey), StorageError> {
    let paths = OperationPaths::new(database, protected_key);
    let _lease = MaintenanceLease::acquire(&paths.maintenance_lock)?;
    if paths.marker.exists() {
        recover_interrupted(database, protected_key, &paths)?;
    }

    match (database.exists(), protected_key.exists()) {
        (false, true) => Err(StorageError::MissingDatabase {
            path: database.display().to_string(),
        }),
        (false, false) if paths.lifecycle_evidence_exists() => {
            Err(StorageError::RecoveryRequired {
                path: database.display().to_string(),
                detail: "live database/key pair is absent while durable lifecycle artifacts remain"
                    .to_string(),
            })
        }
        (false, false) => create_first_database(database, protected_key),
        (true, true) => {
            reject_truncated(database)?;
            let key = load_key(protected_key)?;
            open_verified(database, key)
        }
        (true, false) => {
            reject_truncated(database)?;
            if is_plaintext_sqlite(database)? {
                migrate_plaintext(database, protected_key, &paths)
            } else {
                Err(StorageError::Secret(
                    secret::SecretError::MissingProtectedSecret(protected_key.to_path_buf()),
                ))
            }
        }
    }
}

/// Re-encrypts under a new key while retaining a verified encrypted recovery
/// database and its matching DPAPI-protected old key.
pub fn rotate_sqlcipher_key(database: &Path, protected_key: &Path) -> Result<(), StorageError> {
    rotate_with_fault(database, protected_key, FaultPoint::None)
}

/// Creates a consistent encrypted backup. A source write transaction blocks
/// the exclusive snapshot instead of being silently omitted.
pub fn backup_sqlcipher_database(
    database: &Path,
    protected_key: &Path,
    destination: &Path,
) -> Result<(), StorageError> {
    let paths = OperationPaths::new(database, protected_key);
    let _lease = MaintenanceLease::acquire(&paths.maintenance_lock)?;
    if destination.exists() {
        return Err(StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "refusing to overwrite an existing backup",
        )));
    }
    reject_truncated(database)?;
    let key = load_key(protected_key)?;
    let _fence = DatabaseWriteFence::acquire(database)?;
    let source = open_read_only_with_key(database, &key)?;
    let result = export_to_encrypted(&source, destination, &key)
        .and_then(|()| verify_pair(destination, &key));
    if result.is_err() {
        remove_if_exists(destination).ok();
    } else {
        secret::harden_path(destination, false)?;
    }
    result
}

fn create_first_database(
    database: &Path,
    protected_key: &Path,
) -> Result<(Connection, DatabaseKey), StorageError> {
    let key = secret::load_or_create_database_key(protected_key, false)?;
    let result = (|| {
        let connection = Connection::open(database)?;
        configure_sqlcipher(&connection, &key)?;
        // Force an encrypted page to disk before returning. A crash after key
        // creation can therefore never leave a legitimate zero-byte database.
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS lokiai_cipher_metadata (
                format_version INTEGER NOT NULL PRIMARY KEY
             ) STRICT;
             INSERT OR IGNORE INTO lokiai_cipher_metadata(format_version) VALUES (1);",
        )?;
        verify_cipher_integrity(&connection, database)?;
        configure_connection(&connection)?;
        secret::harden_path(database, false)?;
        Ok(connection)
    })();
    match result {
        Ok(connection) => Ok((connection, key)),
        Err(error) => {
            // First use has no user data. Remove only files created by this
            // failed attempt so retry cannot observe a mismatched half-pair.
            remove_if_exists(database).ok();
            remove_if_exists(protected_key).ok();
            Err(error)
        }
    }
}

fn migrate_plaintext(
    database: &Path,
    protected_key: &Path,
    paths: &OperationPaths,
) -> Result<(Connection, DatabaseKey), StorageError> {
    let _fence = DatabaseWriteFence::acquire(database)?;
    let source = open_plaintext_read_only_main(database)?;
    verify_plaintext_integrity(&source, database)?;
    begin_operation(paths, "plaintext-migration")?;

    let new_key = secret::generate_database_key()?;
    let result = (|| {
        create_plaintext_recovery_backup(&source, paths, &new_key)?;
        secret::store_database_key(&paths.pending_key, &new_key)?;
        export_plaintext_to_encrypted(&source, &paths.replacement, &new_key)?;
        verify_pair(&paths.replacement, &new_key)?;
        drop(source);

        rename(database, &paths.rollback)?;
        rename(&paths.replacement, database)?;
        secret::replace_database_key(protected_key, &new_key)?;
        verify_pair(database, &new_key)?;
        finish_operation(paths)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = recover_interrupted(database, protected_key, paths);
        return Err(error);
    }
    let key = load_key(protected_key)?;
    open_verified(database, key)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FaultPoint {
    None,
    CorruptAfterKeyPromotion,
}

fn rotate_with_fault(
    database: &Path,
    protected_key: &Path,
    fault: FaultPoint,
) -> Result<(), StorageError> {
    let paths = OperationPaths::new(database, protected_key);
    let _lease = MaintenanceLease::acquire(&paths.maintenance_lock)?;
    if paths.marker.exists() {
        recover_interrupted(database, protected_key, &paths)?;
    }
    reject_truncated(database)?;
    let old_key = load_key(protected_key)?;
    let _fence = DatabaseWriteFence::acquire(database)?;
    let source = open_read_only_with_key(database, &old_key)?;
    begin_operation(&paths, "rotation")?;

    let new_key = secret::generate_database_key()?;
    let result = (|| {
        create_recovery_backup(&source, &paths, &old_key)?;
        secret::store_database_key(&paths.rollback_key, &old_key)?;
        secret::store_database_key(&paths.pending_key, &new_key)?;
        export_to_encrypted(&source, &paths.replacement, &new_key)?;
        verify_pair(&paths.replacement, &new_key)?;
        drop(source);

        rename(database, &paths.rollback)?;
        rename(&paths.replacement, database)?;
        secret::replace_database_key(protected_key, &new_key)?;

        if fault == FaultPoint::CorruptAfterKeyPromotion {
            std::fs::write(database, b"injected post-promotion corruption")?;
        }

        verify_pair(database, &new_key)?;
        finish_operation(&paths)?;
        Ok(())
    })();
    if let Err(error) = result {
        recover_interrupted(database, protected_key, &paths)?;
        return Err(error);
    }
    Ok(())
}

fn create_recovery_backup(
    source: &Connection,
    paths: &OperationPaths,
    recovery_key: &DatabaseKey,
) -> Result<(), StorageError> {
    for path in [&paths.recovery_next, &paths.recovery_next_key] {
        remove_if_exists(path)?;
    }
    export_to_encrypted(source, &paths.recovery_next, recovery_key)?;
    secret::store_database_key(&paths.recovery_next_key, recovery_key)?;
    verify_pair_path(&paths.recovery_next, &paths.recovery_next_key)?;

    // Keep the prior recovery generation until the new generation is fully
    // promoted and verified. Recovery examines all three generations.
    remove_pair(&paths.recovery_previous, &paths.recovery_previous_key)?;
    if paths.recovery.exists() || paths.recovery_key.exists() {
        require_complete_pair(&paths.recovery, &paths.recovery_key)?;
        rename(&paths.recovery, &paths.recovery_previous)?;
        rename(&paths.recovery_key, &paths.recovery_previous_key)?;
    }
    rename(&paths.recovery_next, &paths.recovery)?;
    rename(&paths.recovery_next_key, &paths.recovery_key)?;
    verify_pair_path(&paths.recovery, &paths.recovery_key)?;
    remove_pair(&paths.recovery_previous, &paths.recovery_previous_key)?;
    Ok(())
}

fn create_plaintext_recovery_backup(
    source: &Connection,
    paths: &OperationPaths,
    recovery_key: &DatabaseKey,
) -> Result<(), StorageError> {
    for path in [&paths.recovery_next, &paths.recovery_next_key] {
        remove_if_exists(path)?;
    }
    export_plaintext_to_encrypted(source, &paths.recovery_next, recovery_key)?;
    secret::store_database_key(&paths.recovery_next_key, recovery_key)?;
    verify_pair_path(&paths.recovery_next, &paths.recovery_next_key)?;
    remove_pair(&paths.recovery_previous, &paths.recovery_previous_key)?;
    if paths.recovery.exists() || paths.recovery_key.exists() {
        require_complete_pair(&paths.recovery, &paths.recovery_key)?;
        rename(&paths.recovery, &paths.recovery_previous)?;
        rename(&paths.recovery_key, &paths.recovery_previous_key)?;
    }
    rename(&paths.recovery_next, &paths.recovery)?;
    rename(&paths.recovery_next_key, &paths.recovery_key)?;
    verify_pair_path(&paths.recovery, &paths.recovery_key)?;
    remove_pair(&paths.recovery_previous, &paths.recovery_previous_key)?;
    Ok(())
}

fn recover_interrupted(
    database: &Path,
    protected_key: &Path,
    paths: &OperationPaths,
) -> Result<(), StorageError> {
    // Prefer the promoted live pair if it is valid.
    if pair_is_valid(database, protected_key) {
        finish_operation(paths)?;
        return Ok(());
    }

    // A crash after database promotion but before key promotion.
    if pair_is_valid(database, &paths.pending_key) {
        let key = load_key(&paths.pending_key)?;
        preserve_failed_key(protected_key, &paths.failed_key)?;
        secret::replace_database_key(protected_key, &key)?;
        verify_pair(database, &key)?;
        finish_operation(paths)?;
        return Ok(());
    }

    // Rotation always retains the exact old key beside the rollback DB.
    if pair_is_valid(&paths.rollback, &paths.rollback_key) {
        restore_pair(
            &paths.rollback,
            &paths.rollback_key,
            database,
            protected_key,
            paths,
        )?;
        finish_operation(paths)?;
        return Ok(());
    }

    // Plaintext migration and rotation both create an encrypted recovery pair
    // before the live swap. Check every possible generation from an interrupted
    // recovery-pair promotion.
    for (recovery, recovery_key) in [
        (&paths.recovery, &paths.recovery_key),
        (&paths.recovery_next, &paths.recovery_next_key),
        (&paths.recovery_previous, &paths.recovery_previous_key),
    ] {
        if pair_is_valid(recovery, recovery_key) {
            restore_pair(recovery, recovery_key, database, protected_key, paths)?;
            finish_operation(paths)?;
            return Ok(());
        }
    }

    Err(StorageError::Corrupt {
        path: database.display().to_string(),
        detail: "no matching verified database/key recovery pair exists".to_string(),
    })
}

fn restore_pair(
    source_database: &Path,
    source_key: &Path,
    database: &Path,
    protected_key: &Path,
    paths: &OperationPaths,
) -> Result<(), StorageError> {
    let key = load_key(source_key)?;
    remove_if_exists(&paths.replacement)?;
    std::fs::copy(source_database, &paths.replacement)?;
    verify_pair(&paths.replacement, &key)?;
    preserve_failed_database(database, &paths.failed)?;
    rename(&paths.replacement, database)?;
    preserve_failed_key(protected_key, &paths.failed_key)?;
    secret::replace_database_key(protected_key, &key)?;
    verify_pair(database, &key)?;
    Ok(())
}

fn preserve_failed_database(path: &Path, failed: &Path) -> Result<(), StorageError> {
    remove_if_exists(failed)?;
    if path.exists() {
        rename(path, failed)?;
    }
    Ok(())
}

fn preserve_failed_key(path: &Path, failed: &Path) -> Result<(), StorageError> {
    remove_if_exists(failed)?;
    if path.exists() {
        rename(path, failed)?;
    }
    Ok(())
}

fn begin_operation(paths: &OperationPaths, operation: &str) -> Result<(), StorageError> {
    refuse_transient_targets(paths)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&paths.marker)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                StorageError::MaintenanceInProgress {
                    path: paths.marker.display().to_string(),
                }
            } else {
                StorageError::Io(error)
            }
        })?;
    writeln!(file, "version=2")?;
    writeln!(file, "operation={operation}")?;
    file.sync_all()?;
    secret::harden_path(&paths.marker, false)?;
    Ok(())
}

fn finish_operation(paths: &OperationPaths) -> Result<(), StorageError> {
    for path in [
        &paths.replacement,
        &paths.rollback,
        &paths.pending_key,
        &paths.rollback_key,
        &paths.marker,
        &paths.recovery_next,
        &paths.recovery_next_key,
        &paths.recovery_previous,
        &paths.recovery_previous_key,
    ] {
        remove_if_exists(path)?;
    }
    Ok(())
}

fn refuse_transient_targets(paths: &OperationPaths) -> Result<(), StorageError> {
    for path in [
        &paths.replacement,
        &paths.rollback,
        &paths.pending_key,
        &paths.rollback_key,
        &paths.recovery_next,
        &paths.recovery_next_key,
        &paths.recovery_previous,
        &paths.recovery_previous_key,
    ] {
        if path.exists() {
            return Err(StorageError::MaintenanceInProgress {
                path: path.display().to_string(),
            });
        }
    }
    Ok(())
}

fn export_to_encrypted(
    source: &Connection,
    destination: &Path,
    key: &DatabaseKey,
) -> Result<(), StorageError> {
    if destination.exists() {
        return Err(StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("refusing to overwrite {}", destination.display()),
        )));
    }
    // Create and harden the destination through the reviewed filesystem
    // boundary before SQLCipher opens it. This also prevents a directory ACL
    // race where SQLite would otherwise create a file before its DACL is set.
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?
        .sync_all()?;
    secret::harden_path(destination, false)?;
    let mut encrypted = Connection::open(destination)?;
    configure_sqlcipher(&encrypted, key)?;
    {
        let backup = rusqlite::backup::Backup::new(source, &mut encrypted)?;
        backup.run_to_completion(64, Duration::from_millis(25), None)?;
    }
    verify_cipher_integrity(&encrypted, destination)?;
    drop(encrypted);
    secret::harden_path(destination, false)?;
    Ok(())
}

/// SQLCipher's Online Backup API rejects a plaintext source paired with an
/// encrypted destination, so legacy migration uses SQLCipher's purpose-built
/// export function. The caller holds `DatabaseWriteFence`, making the source
/// immutable for the entire export and promotion sequence.
fn export_plaintext_to_encrypted(
    source: &Connection,
    destination: &Path,
    key: &DatabaseKey,
) -> Result<(), StorageError> {
    if destination.exists() {
        return Err(StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("refusing to overwrite {}", destination.display()),
        )));
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?
        .sync_all()?;
    secret::harden_path(destination, false)?;
    let destination_text = destination.to_str().ok_or_else(|| {
        StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "database path is not valid Unicode",
        ))
    })?;
    let mut attach = format!(
        "ATTACH DATABASE ?1 AS lokiai_encrypted KEY \"x'{}'\"",
        key.sqlcipher_hex()
    );
    let attach_result = source.execute(&attach, [destination_text]);
    crate::storage::wipe_string(&mut attach);
    if let Err(error) = attach_result {
        remove_if_exists(destination).ok();
        return Err(StorageError::Sqlite(error));
    }
    let export_result =
        source.query_row(
            "SELECT sqlcipher_export('lokiai_encrypted')",
            [],
            |_| Ok(()),
        );
    let detach_result = source.execute_batch("DETACH DATABASE lokiai_encrypted");
    export_result?;
    detach_result?;
    secret::harden_path(destination, false)?;
    Ok(())
}

fn open_verified(
    database: &Path,
    key: DatabaseKey,
) -> Result<(Connection, DatabaseKey), StorageError> {
    let connection = open_with_key(database, &key)?;
    configure_connection(&connection)?;
    secret::harden_path(database, false)?;
    Ok((connection, key))
}

fn open_with_key(database: &Path, key: &DatabaseKey) -> Result<Connection, StorageError> {
    let connection =
        Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    configure_sqlcipher(&connection, key)?;
    match verify_cipher_integrity(&connection, database) {
        Ok(()) => Ok(connection),
        Err(StorageError::Sqlite(_)) | Err(StorageError::Corrupt { .. }) => {
            Err(StorageError::WrongDatabaseKey {
                path: database.display().to_string(),
            })
        }
        Err(error) => Err(error),
    }
}

fn open_read_only_with_key(database: &Path, key: &DatabaseKey) -> Result<Connection, StorageError> {
    let connection =
        Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    configure_sqlcipher(&connection, key)?;
    verify_cipher_integrity(&connection, database)?;
    Ok(connection)
}

fn open_plaintext_read_only_main(database: &Path) -> Result<Connection, StorageError> {
    let path = database.to_str().ok_or_else(|| {
        StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "database path is not valid Unicode",
        ))
    })?;
    let mut uri = String::from("file:///");
    for byte in path.replace('\\', "/").bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b':' | b'-' | b'_' | b'.') {
            uri.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            write!(&mut uri, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    uri.push_str("?mode=ro");
    Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(StorageError::Sqlite)
}

fn verify_pair(database: &Path, key: &DatabaseKey) -> Result<(), StorageError> {
    let connection = open_with_key(database, key)?;
    drop(connection);
    Ok(())
}

fn verify_pair_path(database: &Path, protected_key: &Path) -> Result<(), StorageError> {
    let key = load_key(protected_key)?;
    verify_pair(database, &key)
}

fn pair_is_valid(database: &Path, protected_key: &Path) -> bool {
    database.exists()
        && protected_key.exists()
        && reject_truncated(database).is_ok()
        && verify_pair_path(database, protected_key).is_ok()
}

fn require_complete_pair(database: &Path, key: &Path) -> Result<(), StorageError> {
    if database.exists() != key.exists() || !pair_is_valid(database, key) {
        return Err(StorageError::Corrupt {
            path: database.display().to_string(),
            detail: "recovery database/key pair is incomplete or invalid".to_string(),
        });
    }
    Ok(())
}

fn remove_pair(database: &Path, key: &Path) -> Result<(), StorageError> {
    remove_if_exists(database)?;
    remove_if_exists(key)?;
    Ok(())
}

fn verify_plaintext_integrity(connection: &Connection, path: &Path) -> Result<(), StorageError> {
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

fn reject_truncated(path: &Path) -> Result<(), StorageError> {
    let bytes = std::fs::metadata(path)?.len();
    if bytes < MIN_DATABASE_BYTES {
        return Err(StorageError::TruncatedDatabase {
            path: path.display().to_string(),
            bytes,
        });
    }
    Ok(())
}

fn is_plaintext_sqlite(path: &Path) -> Result<bool, StorageError> {
    use std::io::Read as _;
    let mut file = OpenOptions::new().read(true).open(path)?;
    let mut header = [0u8; SQLITE_HEADER.len()];
    file.read_exact(&mut header)?;
    Ok(&header == SQLITE_HEADER)
}

fn load_key(path: &Path) -> Result<DatabaseKey, StorageError> {
    secret::load_database_key(path).map_err(StorageError::Secret)
}

fn remove_if_exists(path: &Path) -> Result<(), StorageError> {
    match remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StorageError::Io(error)),
    }
}

/// OS-owned byte-range lock. Windows releases it on process termination, so a
/// durable marker denotes recovery state while this lock denotes a live owner.
struct MaintenanceLease {
    file: std::fs::File,
    overlapped: Box<OVERLAPPED>,
}

/// Non-cooperative Windows write exclusion for the live database. Opening
/// with read/delete sharing rejects every existing writable handle and blocks
/// new writable handles, while permitting this process to rename the guarded
/// file during promotion.
struct DatabaseWriteFence {
    _file: std::fs::File,
}

impl DatabaseWriteFence {
    fn acquire(path: &Path) -> Result<Self, StorageError> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
            .open(path)
            .map_err(|error| StorageError::MaintenanceInProgress {
                path: format!("{} ({error})", path.display()),
            })?;
        Ok(Self { _file: file })
    }
}

impl MaintenanceLease {
    fn acquire(path: &Path) -> Result<Self, StorageError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        secret::harden_path(path, false)?;
        let mut overlapped = Box::new(unsafe { std::mem::zeroed::<OVERLAPPED>() });
        let locked = unsafe {
            LockFileEx(
                file.as_raw_handle(),
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                1,
                0,
                &mut *overlapped,
            )
        };
        if locked == 0 {
            return Err(StorageError::MaintenanceInProgress {
                path: path.display().to_string(),
            });
        }
        Ok(Self { file, overlapped })
    }
}

impl Drop for MaintenanceLease {
    fn drop(&mut self) {
        unsafe {
            UnlockFileEx(self.file.as_raw_handle(), 0, 1, 0, &mut *self.overlapped);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_database_with_existing_key_is_never_recreated() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("missing.sqlite3");
        let key = dir.path().join("missing.key.protected");
        let (connection, _) = open_or_migrate_sqlcipher(&database, &key).unwrap();
        drop(connection);
        std::fs::remove_file(&database).unwrap();
        let result = open_or_migrate_sqlcipher(&database, &key);
        assert!(matches!(result, Err(StorageError::MissingDatabase { .. })));
        assert!(!database.exists());
    }

    #[test]
    fn zero_byte_database_with_key_is_a_truncated_recovery_state() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("truncated.sqlite3");
        let key = dir.path().join("truncated.key.protected");
        let (connection, _) = open_or_migrate_sqlcipher(&database, &key).unwrap();
        drop(connection);
        std::fs::write(&database, []).unwrap();
        assert!(matches!(
            open_or_migrate_sqlcipher(&database, &key),
            Err(StorageError::TruncatedDatabase { bytes: 0, .. })
        ));
    }

    #[test]
    fn post_key_promotion_corruption_restores_matching_old_pair() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("rotate.sqlite3");
        let key = dir.path().join("rotate.key.protected");
        let (connection, _) = open_or_migrate_sqlcipher(&database, &key).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE preserved(value TEXT); INSERT INTO preserved VALUES ('yes');",
            )
            .unwrap();
        drop(connection);

        assert!(rotate_with_fault(&database, &key, FaultPoint::CorruptAfterKeyPromotion).is_err());
        let (reopened, _) = open_or_migrate_sqlcipher(&database, &key).unwrap();
        let value: String = reopened
            .query_row("SELECT value FROM preserved", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "yes");
    }

    #[test]
    fn retained_recovery_pair_is_encrypted_and_readable_after_rotation() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("rotate.sqlite3");
        let key = dir.path().join("rotate.key.protected");
        let (connection, _) = open_or_migrate_sqlcipher(&database, &key).unwrap();
        connection
            .execute_batch("CREATE TABLE t(v); INSERT INTO t VALUES ('secret');")
            .unwrap();
        drop(connection);
        rotate_sqlcipher_key(&database, &key).unwrap();
        let paths = OperationPaths::new(&database, &key);
        assert!(pair_is_valid(&paths.recovery, &paths.recovery_key));
        assert!(
            !std::fs::read(&paths.recovery)
                .unwrap()
                .windows(6)
                .any(|w| w == b"secret")
        );
    }

    #[test]
    fn concurrent_open_connection_prevents_silent_plaintext_swap() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("plain.sqlite3");
        let key = dir.path().join("plain.key.protected");
        let first = Connection::open(&database).unwrap();
        first
            .execute_batch("CREATE TABLE t(v); INSERT INTO t VALUES ('one');")
            .unwrap();
        let second = Connection::open(&database).unwrap();
        second
            .execute_batch("BEGIN IMMEDIATE; INSERT INTO t VALUES ('two');")
            .unwrap();
        let result = open_or_migrate_sqlcipher(&database, &key);
        assert!(matches!(
            result,
            Err(StorageError::MaintenanceInProgress { .. })
        ));
        second.execute_batch("COMMIT").unwrap();
        drop(second);
        drop(first);
        let plain = Connection::open(&database).unwrap();
        let count: i64 = plain
            .query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        assert!(!key.exists());
    }

    #[test]
    fn live_maintenance_owner_prevents_recovery_of_its_marker() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("leased.sqlite3");
        let key = dir.path().join("leased.key.protected");
        let paths = OperationPaths::new(&database, &key);
        let lease = MaintenanceLease::acquire(&paths.maintenance_lock).unwrap();
        std::fs::write(&paths.marker, b"version=2\noperation=rotation\n").unwrap();

        let result = open_or_migrate_sqlcipher(&database, &key);
        assert!(matches!(
            result,
            Err(StorageError::MaintenanceInProgress { .. })
        ));
        assert!(
            paths.marker.exists(),
            "a live owner's journal must remain untouched"
        );
        drop(lease);
    }

    #[test]
    fn retained_lifecycle_pair_never_becomes_an_empty_first_use() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("retained.sqlite3");
        let key = dir.path().join("retained.key.protected");
        let (connection, _) = open_or_migrate_sqlcipher(&database, &key).unwrap();
        connection
            .execute_batch("CREATE TABLE retained(v); INSERT INTO retained VALUES ('real');")
            .unwrap();
        drop(connection);
        rotate_sqlcipher_key(&database, &key).unwrap();
        let paths = OperationPaths::new(&database, &key);
        assert!(pair_is_valid(&paths.recovery, &paths.recovery_key));
        std::fs::remove_file(&database).unwrap();
        std::fs::remove_file(&key).unwrap();

        let result = open_or_migrate_sqlcipher(&database, &key);
        assert!(matches!(result, Err(StorageError::RecoveryRequired { .. })));
        assert!(
            !database.exists(),
            "must not synthesize an empty live database"
        );
        assert!(!key.exists(), "must not synthesize a replacement key");
        assert!(pair_is_valid(&paths.recovery, &paths.recovery_key));
    }

    #[test]
    fn maintenance_lock_is_exclusive_across_processes() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("process.sqlite3");
        let key = dir.path().join("process.key.protected");
        let paths = OperationPaths::new(&database, &key);
        let lease = MaintenanceLease::acquire(&paths.maintenance_lock).unwrap();
        std::fs::write(&paths.marker, b"version=2\noperation=rotation\n").unwrap();

        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("managed::tests::maintenance_lock_child_probe")
            .arg("--nocapture")
            .env("LOKIAI_MAINTENANCE_LOCK_CHILD", &paths.maintenance_lock)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "child process must observe the live owner"
        );
        assert!(
            paths.marker.exists(),
            "child must not consume the owner's journal"
        );
        drop(lease);
    }

    #[test]
    fn maintenance_lock_child_probe() {
        let Some(path) = std::env::var_os("LOKIAI_MAINTENANCE_LOCK_CHILD") else {
            return;
        };
        assert!(matches!(
            MaintenanceLease::acquire(Path::new(&path)),
            Err(StorageError::MaintenanceInProgress { .. })
        ));
    }
}
