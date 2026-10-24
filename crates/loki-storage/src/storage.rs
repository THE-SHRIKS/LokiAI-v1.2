//! The [`Storage`] handle: opening, migrating, integrity-checking, and
//! backing up the runtime-owned SQLite database. See `Docs/ARCHITECTURE.md`
//! §8 "Persistence" and `Docs/graph/tasks/DAT-001.md`.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::Connection;

use crate::cipher::Cipher;
use crate::error::StorageError;
use crate::migrations::{self, MigrationReport};

/// Bounded busy timeout (`Docs/ARCHITECTURE.md` §8: "write-ahead logging
/// and bounded busy timeout") — how long a writer waits for a lock held by
/// another connection/process before failing with `SQLITE_BUSY`, rather
/// than either failing instantly or hanging indefinitely.
const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);

/// A runtime-owned SQLite database handle.
///
/// Internally single-connection (`Mutex<Connection>`): this node's bounded
/// scope is the schema/migration/repository pattern, not a connection pool
/// (`Docs/graph/tasks/DAT-001.md` §5 forbidden scope excludes IPC/
/// transport). A pool's real value shows up once concurrent callers exist —
/// that is `RUN-002`'s consumer-side concern, not this node's.
pub struct Storage {
    conn: Mutex<Connection>,
    key: Option<crate::key::DatabaseKey>,
}

impl std::fmt::Debug for Storage {
    /// Deliberately opaque (`finish_non_exhaustive`, no fields shown): a
    /// derived `Debug` would require locking the connection mutex just to
    /// format it and could print a filesystem path alongside internal
    /// driver state that has no diagnostic value at this layer.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage").finish_non_exhaustive()
    }
}

impl Storage {
    /// Opens using the Windows DPAPI-managed key, including recoverable
    /// plaintext migration for a pre-DAT-002 database.
    #[cfg(windows)]
    pub fn open_managed(path: &Path, protected_key_path: &Path) -> Result<Self, StorageError> {
        let (conn, key) = crate::managed::open_or_migrate_sqlcipher(path, protected_key_path)?;
        Ok(Self {
            conn: Mutex::new(conn),
            key: Some(key),
        })
    }

    /// Opens (creating if absent) the SQLite database at `path`.
    ///
    /// If `path` already exists, this refuses to silently treat a corrupt
    /// or non-SQLite file as a fresh empty database
    /// (`Docs/ARCHITECTURE.md` §8's key-recovery principle — "a wrong/
    /// missing key ... cannot create a replacement empty database" —
    /// applied here to file corruption instead of a missing key): it runs
    /// an integrity check immediately and returns
    /// [`StorageError::Corrupt`] rather than proceeding. A path that does
    /// not exist yet is a legitimate first-run case and creates a fresh
    /// database, as does a pre-existing zero-byte file (a valid, empty
    /// SQLite database by SQLite's own on-disk format rules).
    ///
    /// The integrity check runs *before* [`configure_connection`] on
    /// purpose: SQLite often only detects "this file is not a database"
    /// (`SQLITE_NOTADB`) lazily, on the first real page read — which can be
    /// triggered by something as innocuous as the `PRAGMA journal_mode=WAL`
    /// this function would otherwise run first. Checking integrity first
    /// guarantees a garbage file surfaces as [`StorageError::Corrupt`], not
    /// an unmapped raw driver error from an unrelated pragma.
    pub fn open(path: &Path, cipher: Cipher) -> Result<Self, StorageError> {
        let existed_before_open = path.exists();
        let conn = Connection::open(path)?;

        let key = match cipher {
            Cipher::SqlCipher(key) => {
                configure_sqlcipher(&conn, &key)?;
                key
            }
            // Plaintext disk databases are forbidden after DAT-002.  This is
            // intentionally not a compatibility fallback.
            Cipher::None => return Err(StorageError::UnsupportedCipher),
        };

        if existed_before_open {
            verify_integrity_or_err(&conn, &path.display().to_string())?;
        }

        configure_connection(&conn)?;

        tracing::info!(
            path = %path.display(),
            pre_existing = existed_before_open,
            "storage: opened database"
        );

        Ok(Self {
            conn: Mutex::new(conn),
            key: Some(key),
        })
    }

    /// Opens a private in-memory database. Useful for tests and any
    /// ephemeral use — there is no file, so there is no restart durability.
    pub fn open_in_memory(cipher: Cipher) -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        let key = match cipher {
            Cipher::SqlCipher(key) => {
                configure_sqlcipher(&conn, &key)?;
                Some(key)
            }
            Cipher::None => None,
        };
        configure_connection(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            key,
        })
    }

    /// Applies every not-yet-applied embedded migration, in version order,
    /// each inside its own transaction (see `src/migrations.rs`). Safe to
    /// call on every startup — already-applied versions are skipped, not
    /// reapplied (`Docs/graph/tasks/DAT-001.md` Deliverable 5: "migration
    /// is idempotent"). `applied_at` is a caller-supplied, validated UTC
    /// ISO 8601 timestamp (see `src/timestamp.rs` for why this crate does
    /// not call a system clock itself).
    pub fn run_migrations(&self, applied_at: &str) -> Result<MigrationReport, StorageError> {
        let mut conn = self.lock();
        let report = migrations::apply_all(&mut conn, migrations::MIGRATIONS, applied_at)?;
        tracing::info!(applied = ?report.applied_versions, "storage: migrations applied");
        Ok(report)
    }

    /// Runs `PRAGMA integrity_check` and reports the result. A failure to
    /// even run the check is propagated as an error, never treated as "ok"
    /// (`AGENTS.md` rule 1).
    pub fn integrity_check(&self) -> Result<IntegrityStatus, StorageError> {
        let conn = self.lock();
        let status = run_integrity_check(&conn)?;
        if let IntegrityStatus::Corrupt(problems) = &status {
            tracing::warn!(problems = ?problems, "storage: integrity check reported problems");
        }
        Ok(status)
    }

    /// Creates a consistent backup copy of this database at `destination`,
    /// using SQLite's Online Backup API (`rusqlite::backup`), which reads a
    /// transactionally consistent snapshot regardless of concurrent WAL
    /// writers. This satisfies `Docs/graph/tasks/DAT-001.md` Deliverable 4's
    /// "file-level copy with a documented consistency guarantee — WAL
    /// checkpoint before copy, or equivalent": this is the "equivalent" —
    /// it does not perform a raw filesystem copy of a live WAL-mode file at
    /// all, which is the actual hazard a manual copy would carry. A WAL
    /// checkpoint is also run first, as defense-in-depth and to keep the
    /// source's `-wal` file small.
    ///
    /// `destination` must not already exist as a non-empty file.
    pub fn backup_to(&self, destination: &Path) -> Result<(), StorageError> {
        let conn = self.lock();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

        let mut dest = Connection::open(destination)?;
        let key = self.key.as_ref().ok_or(StorageError::UnsupportedCipher)?;
        configure_sqlcipher(&dest, key)?;
        {
            let backup = rusqlite::backup::Backup::new(&conn, &mut dest)?;
            backup.run_to_completion(64, Duration::from_millis(50), None)?;
        }
        verify_integrity_or_err(&dest, &destination.display().to_string())?;

        tracing::info!(destination = %destination.display(), "storage: backup completed");
        Ok(())
    }

    /// Runs `f` inside a single SQLite transaction: if `f` returns `Err`,
    /// every statement `f` executed is rolled back — nothing partial is
    /// observable afterward (`Docs/graph/tasks/DAT-001.md` Deliverable 5).
    /// Crate-internal: used by the migration runner
    /// (`src/migrations.rs` calls `Connection::transaction` directly, not
    /// through here, since it also owns `&mut Connection` before `Storage`
    /// exists) and by this module's own rollback test. Not part of the
    /// public API yet — see `Docs/graph/handoffs/DAT-001-1.md` §7 for why a
    /// public cross-repository transactional primitive is left to the node
    /// that has a concrete multi-repository atomic write to build (the
    /// classic transactional-outbox use case), rather than speculatively
    /// shaped here.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn transaction<T>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
    }

    /// Locks and returns the underlying connection for repository modules
    /// in this crate (`src/devices.rs`, `src/jobs.rs`, `src/outbox.rs`,
    /// `src/inbox.rs`).
    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| {
            // A prior panic while holding the lock (e.g. inside a
            // repository call) poisons the mutex. Recovering the guard
            // anyway (rather than propagating a second, less-informative
            // panic here) matches this crate's stance of surfacing honest
            // `Result` errors instead of cascading failures — any
            // inconsistency from the original panic is a bug in the
            // panicking call, not something this accessor can fix by
            // itself either way.
            poisoned.into_inner()
        })
    }
}

/// Result of [`Storage::integrity_check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrityStatus {
    Ok,
    Corrupt(Vec<String>),
}

pub fn configure_connection(conn: &Connection) -> Result<(), StorageError> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // journal_mode=WAL is a no-op for `:memory:` connections (SQLite keeps
    // those in "memory" journal mode); execute_batch does not error on
    // that, it simply does not change anything.
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;",
    )?;
    Ok(())
}

pub fn configure_sqlcipher(
    conn: &Connection,
    key: &crate::key::DatabaseKey,
) -> Result<(), StorageError> {
    configure_global_cipher_memory_security(conn)?;
    // SQLCipher requires the key before *any* schema/page access. The key is
    // only materialized for this one native pragma call and is never logged.
    let mut statement = format!(
        "PRAGMA key = \"x'{}'\";
         PRAGMA cipher_compatibility = 4;
         PRAGMA cipher_page_size = 4096;
         PRAGMA kdf_iter = 256000;
         PRAGMA cipher_hmac_algorithm = HMAC_SHA512;
         PRAGMA cipher_kdf_algorithm = PBKDF2_HMAC_SHA512;
         PRAGMA cipher_use_hmac = ON;
         PRAGMA cipher_plaintext_header_size = 0;
         PRAGMA temp_store = MEMORY;",
        key.sqlcipher_hex()
    );
    let result = conn.execute_batch(&statement).map_err(StorageError::from);
    wipe_string(&mut statement);
    result?;

    let cipher_version: String = conn
        .query_row("PRAGMA cipher_version", [], |row| row.get(0))
        .map_err(|error| StorageError::CipherConfiguration {
            detail: format!("cipher_version is unavailable: {error}"),
        })?;
    if cipher_version.trim().is_empty() {
        return Err(StorageError::CipherConfiguration {
            detail: "cipher_version is empty".to_string(),
        });
    }
    verify_sqlcipher_profile(conn)
}

fn configure_global_cipher_memory_security(conn: &Connection) -> Result<(), StorageError> {
    static CONFIGURED: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    let configured = CONFIGURED.get_or_init(|| {
        conn.execute_batch("PRAGMA cipher_memory_security = ON")
            .map_err(|error| error.to_string())
    });
    configured
        .clone()
        .map_err(|detail| StorageError::CipherConfiguration {
            detail: format!("cipher_memory_security initialization failed: {detail}"),
        })
}

fn pragma_text(conn: &Connection, name: &'static str) -> Result<String, StorageError> {
    conn.query_row(&format!("PRAGMA {name}"), [], |row| {
        match row.get_ref(0)? {
            rusqlite::types::ValueRef::Integer(value) => Ok(value.to_string()),
            rusqlite::types::ValueRef::Text(value) => {
                Ok(String::from_utf8_lossy(value).into_owned())
            }
            value => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                value.data_type(),
                "pragma value is neither text nor integer".into(),
            )),
        }
    })
    .map_err(|error| StorageError::CipherConfiguration {
        detail: format!("{name} cannot be verified: {error}"),
    })
}

pub(crate) fn verify_sqlcipher_profile(conn: &Connection) -> Result<(), StorageError> {
    for (name, expected) in [
        ("cipher_page_size", "4096"),
        ("kdf_iter", "256000"),
        ("cipher_hmac_algorithm", "HMAC_SHA512"),
        ("cipher_kdf_algorithm", "PBKDF2_HMAC_SHA512"),
        ("cipher_use_hmac", "1"),
        ("cipher_plaintext_header_size", "0"),
        ("cipher_memory_security", "1"),
        ("temp_store", "2"),
    ] {
        let actual = pragma_text(conn, name)?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(StorageError::CipherConfiguration {
                detail: format!("{name} must be {expected}, got {actual}"),
            });
        }
    }
    Ok(())
}

/// Runs SQLCipher's page/HMAC check and SQLite's logical integrity check.
/// SQLCipher reports success as either no rows or a single `ok` row depending
/// on the linked compatible release.
pub(crate) fn verify_cipher_integrity(conn: &Connection, path: &Path) -> Result<(), StorageError> {
    let mut cipher_stmt = conn.prepare("PRAGMA cipher_integrity_check")?;
    let cipher_rows: Vec<String> = cipher_stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    if !(cipher_rows.is_empty() || (cipher_rows.len() == 1 && cipher_rows[0] == "ok")) {
        return Err(StorageError::Corrupt {
            path: path.display().to_string(),
            detail: format!("cipher_integrity_check: {}", cipher_rows.join("; ")),
        });
    }
    verify_integrity_or_err(conn, &path.display().to_string())
}

pub(crate) fn wipe_string(value: &mut String) {
    // SAFETY: no references into the String are live, length/capacity do not
    // change, and every byte is valid to overwrite. The String is dropped
    // immediately after callers finish with it.
    let bytes = unsafe { value.as_mut_vec() };
    for byte in bytes {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

fn run_integrity_check(conn: &Connection) -> Result<IntegrityStatus, StorageError> {
    let mut stmt = conn.prepare("PRAGMA integrity_check;")?;
    let rows: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;

    if rows.len() == 1 && rows[0] == "ok" {
        Ok(IntegrityStatus::Ok)
    } else {
        Ok(IntegrityStatus::Corrupt(rows))
    }
}

/// Runs the integrity check and turns *any* failure to confirm a healthy
/// database — a reported problem, or the query itself failing because
/// SQLite does not recognize the file at all (`SQLITE_NOTADB`) — into
/// [`StorageError::Corrupt`]. Used by `Storage::open` on a pre-existing
/// file so a garbage/non-SQLite file is never silently treated as "create a
/// fresh empty database instead."
fn verify_integrity_or_err(conn: &Connection, path_display: &str) -> Result<(), StorageError> {
    match run_integrity_check(conn) {
        Ok(IntegrityStatus::Ok) => Ok(()),
        Ok(IntegrityStatus::Corrupt(problems)) => Err(StorageError::Corrupt {
            path: path_display.to_string(),
            detail: problems.join("; "),
        }),
        Err(StorageError::Sqlite(e)) => Err(StorageError::Corrupt {
            path: path_display.to_string(),
            detail: e.to_string(),
        }),
        Err(other) => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn storage_is_send_and_sync() {
        // Required for the eventual RUN-002 consumer to share a `Storage`
        // across async tasks (e.g. behind an `Arc<Storage>`).
        assert_send_sync::<Storage>();
    }

    #[test]
    fn open_in_memory_accepts_the_only_implemented_cipher() {
        // `Cipher` (`src/cipher.rs`) is `#[non_exhaustive]` with exactly one
        // constructible variant today (`None`) — DAT-002 adds the next one.
        // There is therefore no way, even from inside this crate, to build
        // a `Cipher` value that exercises `Storage::open`/`open_in_memory`'s
        // `StorageError::UnsupportedCipher` guard yet; that guard is real,
        // reviewable-by-inspection code (see `cipher.rs`'s doc comment) that
        // becomes exercisable — and gets its own test — the moment DAT-002
        // adds a second variant. Asserting the one real variant works end
        // to end is what this crate can honestly test today.
        assert!(Storage::open_in_memory(Cipher::None).is_ok());
    }

    #[test]
    fn integrity_check_reports_ok_on_a_freshly_migrated_database() {
        let storage = Storage::open_in_memory(Cipher::None).unwrap();
        storage.run_migrations("2026-01-01T00:00:00Z").unwrap();
        assert_eq!(storage.integrity_check().unwrap(), IntegrityStatus::Ok);
    }

    #[test]
    fn every_locked_sqlcipher_profile_setting_is_verified_against_perturbation() {
        let key = crate::key::DatabaseKey::from_bytes([0x5a; 32]);
        for perturbation in [
            "PRAGMA cipher_page_size = 1024",
            "PRAGMA kdf_iter = 64000",
            "PRAGMA cipher_hmac_algorithm = HMAC_SHA256",
            "PRAGMA cipher_kdf_algorithm = PBKDF2_HMAC_SHA256",
            "PRAGMA cipher_use_hmac = OFF",
            "PRAGMA cipher_plaintext_header_size = 32",
            "PRAGMA temp_store = FILE",
        ] {
            let connection = Connection::open_in_memory().unwrap();
            configure_sqlcipher(&connection, &key).unwrap();
            connection.execute_batch(perturbation).unwrap();
            assert!(
                matches!(
                    verify_sqlcipher_profile(&connection),
                    Err(StorageError::CipherConfiguration { .. })
                ),
                "profile verifier accepted {perturbation}"
            );
        }
    }

    #[test]
    fn global_cipher_memory_security_perturbation_is_detected_in_a_child_process() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("storage::tests::cipher_memory_security_child_probe")
            .arg("--nocapture")
            .env("LOKIAI_CIPHER_MEMORY_SECURITY_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn cipher_memory_security_child_probe() {
        if std::env::var_os("LOKIAI_CIPHER_MEMORY_SECURITY_CHILD").is_none() {
            return;
        }
        let key = crate::key::DatabaseKey::from_bytes([0x6b; 32]);
        let connection = Connection::open_in_memory().unwrap();
        configure_sqlcipher(&connection, &key).unwrap();
        connection
            .execute_batch("PRAGMA cipher_memory_security = OFF")
            .unwrap();
        assert!(matches!(
            verify_sqlcipher_profile(&connection),
            Err(StorageError::CipherConfiguration { .. })
        ));
    }

    #[test]
    fn transaction_commits_on_success() {
        let storage = Storage::open_in_memory(Cipher::None).unwrap();
        storage.run_migrations("2026-01-01T00:00:00Z").unwrap();

        storage
            .transaction(|tx| {
                tx.execute(
                    "INSERT INTO jobs (job_id, state, idempotency_key, correlation_id, created_at, updated_at)
                     VALUES ('job-1', 'queued', 'idem-1', 'corr-1', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    [],
                )
                .map_err(StorageError::from)?;
                Ok(())
            })
            .unwrap();

        let count: i64 = storage
            .lock()
            .query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn failed_multi_step_transaction_leaves_no_partial_write() {
        let storage = Storage::open_in_memory(Cipher::None).unwrap();
        storage.run_migrations("2026-01-01T00:00:00Z").unwrap();

        let insert_job = |tx: &rusqlite::Transaction<'_>, job_id: &str, idempotency_key: &str| {
            tx.execute(
                "INSERT INTO jobs (job_id, state, idempotency_key, correlation_id, created_at, updated_at)
                 VALUES (?1, 'queued', ?2, 'corr-1', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                rusqlite::params![job_id, idempotency_key],
            )
        };

        // Two statements in one transaction: the first succeeds, the
        // second violates the UNIQUE idempotency_key index (both rows use
        // the same idempotency key) and fails mid-way.
        let result = storage.transaction(|tx| {
            insert_job(tx, "job-1", "same-idempotency-key").map_err(StorageError::from)?;
            insert_job(tx, "job-2", "same-idempotency-key").map_err(StorageError::from)?;
            Ok(())
        });
        assert!(
            result.is_err(),
            "the second insert's constraint violation must surface as an error"
        );

        let count: i64 = storage
            .lock()
            .query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 0,
            "the first insert must not remain committed once the transaction as a whole failed"
        );
    }
}
