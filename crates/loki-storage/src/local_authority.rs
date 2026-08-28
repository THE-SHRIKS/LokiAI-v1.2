//! Local installation authority repository (`LOC-001`'s
//! `local_installation_authority`/`local_authority_sessions` tables,
//! consolidated into this crate by `P1-ACCEPTANCE-STORE-001` — see
//! `migrations/0006_agent_delivery_and_local_authority.sql`).
//! `crates/loki-local-authority` owns the business rules (Windows-principal
//! binding, session issuance/expiry/revocation authorization); this module
//! owns only the SQL.
//!
//! # Real durable session-generation counter (`P1-PRINCIPAL-GRANT-001`, ADR-0007 Option B)
//!
//! `local_authority_sessions.generation` (`migrations/0008_local_authority_grant_generation.sql`)
//! is a real, durable, per-`principal_sid` counter — **not** the old
//! `find_grant_generation_core` shim this node replaces, which mapped
//! `generation == issued_at_ms` (documented Low-severity ambiguity on
//! duplicate `issued_at_ms` values for one principal). `LOC-001`'s row
//! lifecycle is per-SESSION, assigned once, immutable: every session is its
//! own row, many concurrent sessions exist per principal, and a row is
//! never mutated in place except `revoked_at_ms` (`LOC-001` has no rotate
//! operation, and this node does not add one). `generation` is therefore a
//! monotonically increasing integer scoped to `principal_sid`, assigned as
//! `MAX(generation)+1` and inserted in the **same** `Storage::lock()`-held
//! critical section
//! ([`insert_authority_session_core`]) — that single-writer mutex, not the
//! column itself, is what actually eliminates the old shim's collision
//! risk. Revocation never mints a new generation: [`revoke_authority_session_core`]
//! only ever sets `revoked_at_ms` on the already-issued row/generation.
//!
//! **No cross-model kill switch**: this generation sequence is entirely
//! independent of `crates/loki-storage::agents`'s `AGT-001` generation
//! sequence (ADR-0007's explicit "no shared storage/issuance-layer merge").
//! Revoking a Windows-principal's session here never revokes any agent
//! connection, and vice versa — see that decision's "Consequences" section
//! and `crates/loki-acceptance-store/src/lib.rs`'s top doc comment.
//!
//! # Connection-parameterized cores (`P1-ACCEPTANCE-STORE-001`)
//!
//! Same shape as `src/jobs.rs`'s deadlock-avoidance split — see that
//! module's top doc comment and `src/storage.rs`'s `Storage::transaction`
//! doc comment.

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::StorageError;
use crate::storage::Storage;

/// A persisted installation row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationRow {
    pub installation_id: String,
    pub owner_sid: String,
    pub created_at_ms: i64,
}

/// The caller-supplied fields needed to insert a new authority session row —
/// deliberately excludes `generation`, which [`insert_authority_session_core`]
/// assigns atomically and returns (mirrors `src/jobs.rs`'s existing
/// `NewJob`/`JobRecord` split convention already used in this crate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAuthoritySessionRow {
    pub session_id: String,
    pub installation_id: String,
    pub principal_sid: String,
    pub ipc_session_id: String,
    pub correlation_id: String,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
}

/// A persisted authority session row, read side — gains `generation` over
/// [`NewAuthoritySessionRow`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoritySessionRow {
    pub session_id: String,
    pub installation_id: String,
    pub principal_sid: String,
    pub ipc_session_id: String,
    pub correlation_id: String,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
    pub generation: u64,
}

/// The minimal fields `P1-ACCEPTANCE-STORE-001`'s acceptance transaction
/// needs to construct a `loki_mutation_contract::GrantGeneration` for a
/// Windows-SID (`LocalSession`) principal. Replaces the old, provisional
/// `GrantGenerationRow`/`find_grant_generation_core` shim — this row is
/// keyed by a real, durable `generation` column, incapable of that shim's
/// duplicate-`issued_at_ms` ambiguity by construction (unique-by-invariant
/// per `(principal_sid, generation)`, see this module's top doc comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalGrantGenerationRow {
    pub principal_sid: String,
    pub generation: u64,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
}

/// Repository pattern over the installation/session tables.
pub trait LocalAuthorityRepository {
    fn get_installation(&self) -> Result<Option<InstallationRow>, StorageError>;
    fn insert_installation(&self, installation: &InstallationRow) -> Result<(), StorageError>;
    fn get_authority_session(
        &self,
        session_id: &str,
    ) -> Result<Option<AuthoritySessionRow>, StorageError>;
    /// Atomically assigns the next `generation` for `session.principal_sid`
    /// (`MAX(generation)+1`, or `1` if none exists yet) and inserts the new
    /// session row with it, inside one `Storage::lock()`-held critical
    /// section. Returns the assigned generation — see this module's top
    /// doc comment for why this signature change (over the prior
    /// `Result<(), StorageError>`) is in scope for this not-yet-accepted
    /// trait (`Docs/graph/tasks/P1-PRINCIPAL-GRANT-001.md` §9 item 1).
    fn insert_authority_session(
        &self,
        session: &NewAuthoritySessionRow,
    ) -> Result<u64, StorageError>;
    /// Atomically revokes an unrevoked session. Returns `true` if this call
    /// performed the revocation.
    fn revoke_authority_session(&self, session_id: &str, now_ms: i64)
    -> Result<bool, StorageError>;
    /// Real durable generation read for a Windows-SID principal — see this
    /// module's top doc comment. Replaces the old provisional
    /// `find_grant_generation` shim.
    fn find_local_generation(
        &self,
        principal_sid: &str,
        generation: u64,
    ) -> Result<Option<LocalGrantGenerationRow>, StorageError>;
}

impl LocalAuthorityRepository for Storage {
    fn get_installation(&self) -> Result<Option<InstallationRow>, StorageError> {
        let conn = self.lock();
        get_installation_core(&conn)
    }

    fn insert_installation(&self, installation: &InstallationRow) -> Result<(), StorageError> {
        let conn = self.lock();
        insert_installation_core(&conn, installation)
    }

    fn get_authority_session(
        &self,
        session_id: &str,
    ) -> Result<Option<AuthoritySessionRow>, StorageError> {
        let conn = self.lock();
        get_authority_session_core(&conn, session_id)
    }

    fn insert_authority_session(
        &self,
        session: &NewAuthoritySessionRow,
    ) -> Result<u64, StorageError> {
        let conn = self.lock();
        insert_authority_session_core(&conn, session)
    }

    fn revoke_authority_session(
        &self,
        session_id: &str,
        now_ms: i64,
    ) -> Result<bool, StorageError> {
        let conn = self.lock();
        revoke_authority_session_core(&conn, session_id, now_ms)
    }

    fn find_local_generation(
        &self,
        principal_sid: &str,
        generation: u64,
    ) -> Result<Option<LocalGrantGenerationRow>, StorageError> {
        let conn = self.lock();
        find_local_generation_core(&conn, principal_sid, generation)
    }
}

pub fn get_installation_core(conn: &Connection) -> Result<Option<InstallationRow>, StorageError> {
    conn.query_row(
        "SELECT installation_id, owner_sid, created_at_ms
         FROM local_installation_authority WHERE singleton = 1",
        [],
        |row| {
            Ok(InstallationRow {
                installation_id: row.get(0)?,
                owner_sid: row.get(1)?,
                created_at_ms: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn insert_installation_core(
    conn: &Connection,
    installation: &InstallationRow,
) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO local_installation_authority
         (singleton, installation_id, owner_sid, created_at_ms)
         VALUES (1, ?1, ?2, ?3)",
        params![
            installation.installation_id,
            installation.owner_sid,
            installation.created_at_ms
        ],
    )?;
    Ok(())
}

pub fn get_authority_session_core(
    conn: &Connection,
    session_id: &str,
) -> Result<Option<AuthoritySessionRow>, StorageError> {
    conn.query_row(
        "SELECT session_id, installation_id, principal_sid, ipc_session_id,
                correlation_id, issued_at_ms, expires_at_ms, revoked_at_ms, generation
         FROM local_authority_sessions WHERE session_id = ?1",
        [session_id],
        row_to_session,
    )
    .optional()
    .map_err(Into::into)
}

/// Atomically assigns the next `generation` for `session.principal_sid` and
/// inserts the new row with it. The caller must already hold the
/// `Storage::lock()` mutex for the whole `SELECT MAX(...)` + `INSERT` pair
/// (true for every call site: [`Storage`]'s trait impl above locks once
/// before calling this; `Storage::transaction` callers already hold the
/// connection-level lock for the whole closure) — that single-writer
/// serialization is what makes this atomic, not any SQL-level locking
/// clause.
pub fn insert_authority_session_core(
    conn: &Connection,
    session: &NewAuthoritySessionRow,
) -> Result<u64, StorageError> {
    let next_generation: i64 = conn.query_row(
        "SELECT COALESCE(MAX(generation), 0) + 1 FROM local_authority_sessions WHERE principal_sid = ?1",
        params![session.principal_sid],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO local_authority_sessions
         (session_id, installation_id, principal_sid, ipc_session_id,
          correlation_id, issued_at_ms, expires_at_ms, revoked_at_ms, generation)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
        params![
            session.session_id,
            session.installation_id,
            session.principal_sid,
            session.ipc_session_id,
            session.correlation_id,
            session.issued_at_ms,
            session.expires_at_ms,
            next_generation
        ],
    )?;
    u64::try_from(next_generation).map_err(|_| StorageError::InvalidId {
        kind: "LocalSessionGeneration",
        reason: "generation exceeds representable range".to_string(),
    })
}

pub fn revoke_authority_session_core(
    conn: &Connection,
    session_id: &str,
    now_ms: i64,
) -> Result<bool, StorageError> {
    let changed = conn.execute(
        "UPDATE local_authority_sessions SET revoked_at_ms = ?1
         WHERE session_id = ?2 AND revoked_at_ms IS NULL",
        params![now_ms, session_id],
    )?;
    Ok(changed == 1)
}

/// Real durable generation read for a Windows-SID principal — replaces the
/// old provisional `find_grant_generation_core` shim. Unique-by-invariant
/// per `(principal_sid, generation)` (this module's top doc comment), so
/// unlike the old shim there is no duplicate-match ambiguity to document.
pub fn find_local_generation_core(
    conn: &Connection,
    principal_sid: &str,
    generation: u64,
) -> Result<Option<LocalGrantGenerationRow>, StorageError> {
    let generation_i64 = i64::try_from(generation).map_err(|_| StorageError::InvalidId {
        kind: "LocalSessionGeneration",
        reason: "generation exceeds representable range".to_string(),
    })?;
    conn.query_row(
        "SELECT principal_sid, issued_at_ms, expires_at_ms, revoked_at_ms
         FROM local_authority_sessions WHERE principal_sid = ?1 AND generation = ?2",
        params![principal_sid, generation_i64],
        |row| {
            Ok(LocalGrantGenerationRow {
                principal_sid: row.get(0)?,
                generation,
                issued_at_ms: row.get(1)?,
                expires_at_ms: row.get(2)?,
                revoked_at_ms: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuthoritySessionRow> {
    Ok(AuthoritySessionRow {
        session_id: row.get(0)?,
        installation_id: row.get(1)?,
        principal_sid: row.get(2)?,
        ipc_session_id: row.get(3)?,
        correlation_id: row.get(4)?,
        issued_at_ms: row.get(5)?,
        expires_at_ms: row.get(6)?,
        revoked_at_ms: row.get(7)?,
        generation: {
            let raw: i64 = row.get(8)?;
            u64::try_from(raw).unwrap_or(0)
        },
    })
}
