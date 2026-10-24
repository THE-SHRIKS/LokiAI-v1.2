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
const BACKUP_MANIFEST_MAGIC: &[u8; 16] = b"LOKIAI.BACKUP.V1";
const BACKUP_MANIFEST_BYTES: usize = BACKUP_MANIFEST_MAGIC.len() + 8 + 32;

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
    recovery_reconciled: PathBuf,
    recovery_reconciled_key: PathBuf,
    maintenance_lock: PathBuf,
    wal: PathBuf,
    shm: PathBuf,
    journal: PathBuf,
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
            recovery_reconciled: database.with_extension("recovery.reconciled.sqlite3"),
            recovery_reconciled_key: key.with_extension("recovery.reconciled.protected"),
            maintenance_lock: database.with_extension("maintenance.lock"),
            wal: sqlite_sidecar(database, "-wal"),
            shm: sqlite_sidecar(database, "-shm"),
            journal: sqlite_sidecar(database, "-journal"),
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
            &self.recovery_reconciled,
            &self.recovery_reconciled_key,
            &self.wal,
            &self.shm,
            &self.journal,
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
    let destination_key = secret::backup_key_path(destination);
    let destination_manifest = secret::backup_manifest_path(destination);
    if destination.exists() || destination_key.exists() || destination_manifest.exists() {
        return Err(StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "refusing to overwrite an existing backup database/key pair",
        )));
    }
    reject_truncated(database)?;
    let key = load_key(protected_key)?;
    let _fence = DatabaseWriteFence::acquire(database)?;
    let source = open_read_only_with_key(database, &key)?;
    let result = secret::store_backup_key(&destination_key, &key)
        .map_err(StorageError::Secret)
        .and_then(|()| export_to_encrypted(&source, destination, &key))
        .and_then(|()| verify_pair(destination, &key))
        .and_then(|()| create_backup_manifest(destination, &destination_key))
        .and_then(|manifest| {
            secret::store_backup_manifest(&destination_manifest, &manifest)
                .map_err(StorageError::Secret)
        });
    if result.is_err() {
        remove_if_exists(destination).ok();
        remove_if_exists(&destination_key).ok();
        remove_if_exists(&destination_manifest).ok();
    } else {
        secret::harden_path(destination, false)?;
    }
    result
}

/// Opens only a complete, purpose-bound backup database/key/manifest bundle.
/// Normal managed-open deliberately cannot consume a backup-purpose key.
pub fn open_backup_sqlcipher(database: &Path) -> Result<(Connection, DatabaseKey), StorageError> {
    let key_path = secret::backup_key_path(database);
    let manifest_path = secret::backup_manifest_path(database);
    if !database.exists() || !key_path.exists() || !manifest_path.exists() {
        return Err(StorageError::RecoveryRequired {
            path: database.display().to_string(),
            detail: "backup database/key/manifest bundle is incomplete".to_string(),
        });
    }
    let expected = create_backup_manifest(database, &key_path)?;
    let actual = secret::load_backup_manifest(&manifest_path).map_err(StorageError::Secret)?;
    if actual != expected {
        return Err(StorageError::Corrupt {
            path: database.display().to_string(),
            detail: "backup manifest does not match the database/key bundle".to_string(),
        });
    }
    let key = secret::load_backup_key(&key_path).map_err(StorageError::Secret)?;
    open_verified(database, key)
}

fn create_backup_manifest(database: &Path, key_path: &Path) -> Result<Vec<u8>, StorageError> {
    let database_len = std::fs::metadata(database)?.len();
    let protected_key = std::fs::read(key_path)?;
    let key_digest = secret::sha256(&protected_key).map_err(StorageError::Secret)?;
    let mut manifest = Vec::with_capacity(BACKUP_MANIFEST_BYTES);
    manifest.extend_from_slice(BACKUP_MANIFEST_MAGIC);
    manifest.extend_from_slice(&database_len.to_le_bytes());
    manifest.extend_from_slice(&key_digest);
    Ok(manifest)
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
            remove_if_exists(&sqlite_sidecar(database, "-wal")).ok();
            remove_if_exists(&sqlite_sidecar(database, "-shm")).ok();
            remove_if_exists(&sqlite_sidecar(database, "-journal")).ok();
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
    AfterRecoveryPair,
    AfterPendingKeys,
    AfterReplacementVerified,
    AfterDatabaseDemotion,
    AfterDatabasePromotion,
    AfterKeyPromotion,
    CorruptAfterKeyPromotion,
    AfterFinalVerification,
}

fn inject_rotation_fault(requested: FaultPoint, current: FaultPoint) -> Result<(), StorageError> {
    if requested == current {
        return Err(StorageError::Io(std::io::Error::other(format!(
            "injected rotation crash at {current:?}"
        ))));
    }
    Ok(())
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
        inject_rotation_fault(fault, FaultPoint::AfterRecoveryPair)?;
        secret::store_database_key(&paths.rollback_key, &old_key)?;
        secret::store_database_key(&paths.pending_key, &new_key)?;
        inject_rotation_fault(fault, FaultPoint::AfterPendingKeys)?;
        export_to_encrypted(&source, &paths.replacement, &new_key)?;
        verify_pair(&paths.replacement, &new_key)?;
        inject_rotation_fault(fault, FaultPoint::AfterReplacementVerified)?;
        drop(source);

        rename(database, &paths.rollback)?;
        inject_rotation_fault(fault, FaultPoint::AfterDatabaseDemotion)?;
        rename(&paths.replacement, database)?;
        inject_rotation_fault(fault, FaultPoint::AfterDatabasePromotion)?;
        secret::replace_database_key(protected_key, &new_key)?;
        inject_rotation_fault(fault, FaultPoint::AfterKeyPromotion)?;

        if fault == FaultPoint::CorruptAfterKeyPromotion {
            std::fs::write(database, b"injected post-promotion corruption")?;
        }

        verify_pair(database, &new_key)?;
        inject_rotation_fault(fault, FaultPoint::AfterFinalVerification)?;
        finish_operation(&paths)?;
        Ok(())
    })();
    if let Err(error) = result {
        if fault == FaultPoint::None {
            recover_interrupted(database, protected_key, &paths)?;
        }
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
    let _fence = if database.exists() {
        Some(DatabaseWriteFence::acquire(database)?)
    } else {
        None
    };
    reconcile_recovery_generations(paths)?;
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

fn reconcile_recovery_generations(paths: &OperationPaths) -> Result<(), StorageError> {
    let has_split_generation = [
        &paths.recovery_next,
        &paths.recovery_next_key,
        &paths.recovery_previous,
        &paths.recovery_previous_key,
        &paths.recovery_reconciled,
        &paths.recovery_reconciled_key,
    ]
    .iter()
    .any(|path| path.exists());
    if !has_split_generation {
        return Ok(());
    }

    let databases = [
        &paths.recovery_reconciled,
        &paths.recovery_next,
        &paths.recovery,
        &paths.recovery_previous,
    ];
    let keys = [
        &paths.recovery_reconciled_key,
        &paths.recovery_next_key,
        &paths.recovery_key,
        &paths.recovery_previous_key,
    ];
    let matching = databases.iter().find_map(|database| {
        keys.iter()
            .find(|key| pair_is_valid(database, key))
            .map(|key| (*database, *key))
    });
    let Some((database, key_path)) = matching else {
        return Err(StorageError::Corrupt {
            path: paths.recovery.display().to_string(),
            detail: "split recovery generations contain no matching verified database/key pair"
                .to_string(),
        });
    };

    if database != &paths.recovery_reconciled {
        remove_pair(&paths.recovery_reconciled, &paths.recovery_reconciled_key)?;
        std::fs::copy(database, &paths.recovery_reconciled)?;
        let key = load_key(key_path)?;
        secret::store_database_key(&paths.recovery_reconciled_key, &key)?;
        verify_pair_path(&paths.recovery_reconciled, &paths.recovery_reconciled_key)?;
    }

    // The reconciled staging pair remains intact until both canonical files
    // are copied and verified, so every crash point retains a matching pair.
    std::fs::copy(&paths.recovery_reconciled, &paths.recovery)?;
    std::fs::copy(&paths.recovery_reconciled_key, &paths.recovery_key)?;
    verify_pair_path(&paths.recovery, &paths.recovery_key)?;
    remove_pair(&paths.recovery_reconciled, &paths.recovery_reconciled_key)?;
    Ok(())
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
        &paths.recovery_next,
        &paths.recovery_next_key,
        &paths.recovery_previous,
        &paths.recovery_previous_key,
        &paths.recovery_reconciled,
        &paths.recovery_reconciled_key,
    ] {
        remove_if_exists(path)?;
    }
    remove_if_exists(&paths.marker)
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
    secret::create_secure_new_file(destination)?.sync_all()?;
    let mut encrypted = Connection::open(destination)?;
    configure_sqlcipher(&encrypted, key)?;
    {
        let backup = rusqlite::backup::Backup::new(source, &mut encrypted)?;
        backup.run_to_completion(64, Duration::from_millis(25), None)?;
    }
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
    secret::create_secure_new_file(destination)?.sync_all()?;
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

fn sqlite_sidecar(database: &Path, suffix: &str) -> PathBuf {
    let mut path = database.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
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
    fn every_rotation_promotion_boundary_recovers_on_the_next_open() {
        for fault in [
            FaultPoint::AfterRecoveryPair,
            FaultPoint::AfterPendingKeys,
            FaultPoint::AfterReplacementVerified,
            FaultPoint::AfterDatabaseDemotion,
            FaultPoint::AfterDatabasePromotion,
            FaultPoint::AfterKeyPromotion,
            FaultPoint::AfterFinalVerification,
        ] {
            eprintln!("running rotation crash matrix point {fault:?}");
            let dir = tempfile::tempdir().unwrap();
            let database = dir.path().join(format!("rotate-{fault:?}.sqlite3"));
            let key = dir.path().join(format!("rotate-{fault:?}.key.protected"));
            let (connection, _) = open_or_migrate_sqlcipher(&database, &key).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE preserved(value TEXT); INSERT INTO preserved VALUES ('yes');",
                )
                .unwrap();
            drop(connection);

            assert!(rotate_with_fault(&database, &key, fault).is_err());
            let paths = OperationPaths::new(&database, &key);
            assert!(
                paths.marker.exists(),
                "{fault:?} must leave a recovery journal"
            );
            let (reopened, _) = open_or_migrate_sqlcipher(&database, &key).unwrap();
            let value: String = reopened
                .query_row("SELECT value FROM preserved", [], |row| row.get(0))
                .unwrap();
            assert_eq!(value, "yes", "data lost after {fault:?}");
            assert!(
                !paths.marker.exists(),
                "recovery did not finish for {fault:?}"
            );
            drop(reopened);
            rotate_sqlcipher_key(&database, &key).unwrap_or_else(|error| {
                panic!("next rotation failed after recovery from {fault:?}: {error}")
            });
        }
    }

    #[test]
    fn split_recovery_generations_are_reconciled_before_live_pair_short_circuit() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("split.sqlite3");
        let key = dir.path().join("split.key.protected");
        drop(open_or_migrate_sqlcipher(&database, &key).unwrap());
        rotate_sqlcipher_key(&database, &key).unwrap();
        let paths = OperationPaths::new(&database, &key);
        std::fs::copy(&paths.recovery, &paths.recovery_next).unwrap();
        std::fs::copy(&paths.recovery_key, &paths.recovery_next_key).unwrap();
        std::fs::rename(&paths.recovery, &paths.recovery_previous).unwrap();
        std::fs::write(&paths.marker, b"version=2\noperation=rotation\n").unwrap();
        assert!(pair_is_valid(&database, &key));

        drop(open_or_migrate_sqlcipher(&database, &key).unwrap());
        assert!(pair_is_valid(&paths.recovery, &paths.recovery_key));
        assert!(!paths.recovery_next.exists());
        assert!(!paths.recovery_previous.exists());
        rotate_sqlcipher_key(&database, &key).unwrap();
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
    fn orphan_sqlite_sidecars_block_empty_first_use() {
        for suffix in ["-wal", "-shm", "-journal"] {
            let dir = tempfile::tempdir().unwrap();
            let database = dir.path().join("sidecar.sqlite3");
            let key = dir.path().join("sidecar.key.protected");
            let sidecar = sqlite_sidecar(&database, suffix);
            std::fs::write(&sidecar, b"durable lifecycle evidence").unwrap();

            assert!(matches!(
                open_or_migrate_sqlcipher(&database, &key),
                Err(StorageError::RecoveryRequired { .. })
            ));
            assert!(!database.exists());
            assert!(!key.exists());
            assert!(sidecar.exists());
        }
    }

    #[test]
    fn real_sqlite_wal_shm_and_journal_are_encrypted_lifecycle_evidence() {
        const MARKER: &[u8] = b"LOKIAI_REAL_SIDECAR_SECRET_74c13b";

        let wal_dir = tempfile::tempdir().unwrap();
        let wal_database = wal_dir.path().join("wal.sqlite3");
        let wal_key = wal_dir.path().join("wal.key.protected");
        let (wal_connection, _) = open_or_migrate_sqlcipher(&wal_database, &wal_key).unwrap();
        wal_connection
            .execute_batch(
                "PRAGMA wal_autocheckpoint=0;
                 CREATE TABLE sidecar_secret(v BLOB);
                 INSERT INTO sidecar_secret VALUES (x'4c4f4b4941495f5245414c5f534944454341525f5345435245545f373463313362');",
            )
            .unwrap();
        let wal_path = sqlite_sidecar(&wal_database, "-wal");
        let shm_path = sqlite_sidecar(&wal_database, "-shm");
        let wal_bytes = std::fs::read(&wal_path).unwrap();
        let shm_bytes = std::fs::read(&shm_path).unwrap();
        assert!(!wal_bytes.windows(MARKER.len()).any(|bytes| bytes == MARKER));
        assert!(!shm_bytes.windows(MARKER.len()).any(|bytes| bytes == MARKER));
        drop(wal_connection);
        std::fs::remove_file(&wal_database).unwrap();
        std::fs::remove_file(&wal_key).unwrap();
        std::fs::write(&wal_path, wal_bytes).unwrap();
        std::fs::write(&shm_path, shm_bytes).unwrap();
        assert!(matches!(
            open_or_migrate_sqlcipher(&wal_database, &wal_key),
            Err(StorageError::RecoveryRequired { .. })
        ));

        let journal_dir = tempfile::tempdir().unwrap();
        let journal_database = journal_dir.path().join("journal.sqlite3");
        let journal_key = journal_dir.path().join("journal.key.protected");
        let (journal_connection, _) =
            open_or_migrate_sqlcipher(&journal_database, &journal_key).unwrap();
        journal_connection
            .execute_batch(
                "PRAGMA journal_mode=DELETE;
                 CREATE TABLE sidecar_secret(v BLOB);
                 INSERT INTO sidecar_secret VALUES (x'4c4f4b4941495f5245414c5f534944454341525f5345435245545f373463313362');
                 BEGIN IMMEDIATE;
                 UPDATE sidecar_secret SET v = zeroblob(4096);",
            )
            .unwrap();
        let journal_path = sqlite_sidecar(&journal_database, "-journal");
        let journal_bytes = std::fs::read(&journal_path).unwrap();
        assert!(
            !journal_bytes
                .windows(MARKER.len())
                .any(|bytes| bytes == MARKER)
        );
        journal_connection.execute_batch("ROLLBACK").unwrap();
        drop(journal_connection);
        std::fs::remove_file(&journal_database).unwrap();
        std::fs::remove_file(&journal_key).unwrap();
        std::fs::write(&journal_path, journal_bytes).unwrap();
        assert!(matches!(
            open_or_migrate_sqlcipher(&journal_database, &journal_key),
            Err(StorageError::RecoveryRequired { .. })
        ));
    }

    #[test]
    fn recovery_refuses_an_existing_writable_database_handle() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("recovery-fence.sqlite3");
        let key = dir.path().join("recovery-fence.key.protected");
        let (writer, _) = open_or_migrate_sqlcipher(&database, &key).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        let paths = OperationPaths::new(&database, &key);
        std::fs::write(&paths.marker, b"version=2\noperation=rotation\n").unwrap();

        assert!(matches!(
            open_or_migrate_sqlcipher(&database, &key),
            Err(StorageError::MaintenanceInProgress { .. })
        ));
        assert!(
            paths.marker.exists(),
            "unfenced recovery must not consume the journal"
        );
        writer.execute_batch("ROLLBACK").unwrap();
    }

    #[test]
    fn database_write_fence_is_bidirectional_across_processes() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("bidirectional.sqlite3");
        Connection::open(&database)
            .unwrap()
            .execute_batch("CREATE TABLE t(v)")
            .unwrap();

        let fence = DatabaseWriteFence::acquire(&database).unwrap();
        let denied = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("managed::tests::database_write_fence_child_probe")
            .arg("--nocapture")
            .env("LOKIAI_DB_FENCE_CHILD_MODE", "expect-denied")
            .env("LOKIAI_DB_FENCE_DATABASE", &database)
            .status()
            .unwrap();
        assert!(
            denied.success(),
            "child writable open bypassed the parent fence"
        );
        drop(fence);

        let ready = dir.path().join("writer.ready");
        let release = dir.path().join("writer.release");
        let mut writer = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("managed::tests::database_write_fence_child_probe")
            .arg("--nocapture")
            .env("LOKIAI_DB_FENCE_CHILD_MODE", "hold-writer")
            .env("LOKIAI_DB_FENCE_DATABASE", &database)
            .env("LOKIAI_DB_FENCE_READY", &ready)
            .env("LOKIAI_DB_FENCE_RELEASE", &release)
            .spawn()
            .unwrap();
        for _ in 0..500 {
            if ready.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            ready.exists(),
            "child did not establish its writable handle"
        );
        assert!(matches!(
            DatabaseWriteFence::acquire(&database),
            Err(StorageError::MaintenanceInProgress { .. })
        ));
        std::fs::write(&release, b"release").unwrap();
        assert!(writer.wait().unwrap().success());
    }

    #[test]
    fn database_write_fence_child_probe() {
        let Ok(mode) = std::env::var("LOKIAI_DB_FENCE_CHILD_MODE") else {
            return;
        };
        let database = PathBuf::from(std::env::var_os("LOKIAI_DB_FENCE_DATABASE").unwrap());
        if mode == "expect-denied" {
            assert!(Connection::open(database).is_err());
            return;
        }
        let connection = Connection::open(database).unwrap();
        connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        let ready = PathBuf::from(std::env::var_os("LOKIAI_DB_FENCE_READY").unwrap());
        let release = PathBuf::from(std::env::var_os("LOKIAI_DB_FENCE_RELEASE").unwrap());
        std::fs::write(ready, b"ready").unwrap();
        for _ in 0..1000 {
            if release.exists() {
                connection.execute_batch("ROLLBACK").unwrap();
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("parent did not release child writer");
    }

    #[test]
    fn cleanup_failure_keeps_the_recovery_marker_until_transients_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("cleanup.sqlite3");
        let key = dir.path().join("cleanup.key.protected");
        let paths = OperationPaths::new(&database, &key);
        std::fs::write(&paths.marker, b"version=2\noperation=rotation\n").unwrap();
        let locked = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&paths.replacement)
            .unwrap();

        assert!(finish_operation(&paths).is_err());
        assert!(
            paths.marker.exists(),
            "journal must be the final cleanup artifact"
        );
        drop(locked);
        finish_operation(&paths).unwrap();
        assert!(!paths.marker.exists());
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
