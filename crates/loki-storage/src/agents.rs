//! Local agent connection/run/event repository (`AGT-001`'s
//! `agent_connections`/`agent_runs`/`agent_run_events` tables, consolidated
//! into this crate by `P1-ACCEPTANCE-STORE-001` — see
//! `migrations/0005_agents.sql`). `crates/loki-agents` owns the business
//! rules (secret hashing/authentication, scope authorization, run state
//! machine); this module owns only the SQL.
//!
//! # Real durable connection-generation counter (`P1-PRINCIPAL-GRANT-001`, ADR-0007 Option B)
//!
//! `agent_connections.generation`/`generation_issued_at_ms`
//! (`migrations/0009_agent_grant_generation.sql`) are a real, durable,
//! per-`connection_id` counter. Unlike `LOC-001`'s per-session model
//! (`src/local_authority.rs`), `AGT-001`'s row lifecycle is per-CONNECTION,
//! mutable, bumped in place: a connection is a single row with exactly one
//! live `secret_hash` at a time. `generation` starts at `1` on
//! `insert_agent_connection_core` (a known constant — no computation, no
//! race possible) and increments by exactly `1`, atomically, under a
//! guarded `UPDATE ... WHERE connection_id = ? AND revoked_at_ms IS NULL`
//! on [`rotate_agent_connection_core`], [`change_agent_scopes_core`], and
//! (defensively, not itself a new fail-closed property —
//! `revoked_at_ms` already independently fail-closes)
//! [`revoke_agent_connection_core`].
//!
//! **`AGT-001` has no expiry concept and this module adds none** — no
//! `expires_at_ms`-shaped column/field exists anywhere in this module. Any
//! "never expires" sentinel a caller needs to construct a
//! `loki_mutation_contract::GrantGeneration` from an
//! [`AgentGrantGenerationRow`] belongs solely to
//! `crates/loki-acceptance-store` (ADR-0007's explicit boundary) — it must
//! never appear in this crate's row types or SQL.
//!
//! **No cross-model kill switch**: this generation sequence is entirely
//! independent of `crates/loki-storage::local_authority`'s `LOC-001`
//! generation sequence (ADR-0007's explicit "no shared storage/
//! issuance-layer merge"). Revoking an agent connection here never revokes
//! any Windows-principal session, and vice versa.
//!
//! # Connection-parameterized cores (`P1-ACCEPTANCE-STORE-001`)
//!
//! Same shape as `src/jobs.rs`'s deadlock-avoidance split — see that
//! module's top doc comment and `src/storage.rs`'s `Storage::transaction`
//! doc comment.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::StorageError;
use crate::storage::Storage;

const RUN_COLUMNS: &str = "run_id, connection_id, parent_run_id, correlation_id, idempotency_key, \
     state, started_at_ms, completed_at_ms";

/// A persisted agent connection row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConnectionRow {
    pub connection_id: String,
    pub label: String,
    pub secret_hash: String,
    /// Comma-joined wire scope strings, exactly as `crates/loki-agents`
    /// encodes/decodes them — this module does not parse the scope
    /// vocabulary (owned by that crate).
    pub scopes: String,
    pub created_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
    pub generation: u64,
    pub generation_issued_at_ms: i64,
}

/// The minimal fields `crates/loki-acceptance-store` needs to construct a
/// `loki_mutation_contract::GrantGeneration` for an agent-connection
/// principal. Deliberately has **no** `expires_at_ms` field — `AGT-001` has
/// no expiry concept (see this module's top doc comment); the "never
/// expires" sentinel a caller needs belongs solely to
/// `crates/loki-acceptance-store`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentGrantGenerationRow {
    pub connection_id: String,
    pub generation: u64,
    pub issued_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
}

/// A persisted agent run row. `state`/`kind` remain raw wire strings here —
/// `crates/loki-agents` owns their closed-enum parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRow {
    pub run_id: String,
    pub connection_id: String,
    pub parent_run_id: Option<String>,
    pub correlation_id: String,
    pub idempotency_key: String,
    pub state: String,
    pub started_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

/// Fields required to persist a new run event.
#[derive(Debug, Clone)]
pub struct NewRunEvent {
    pub run_id: String,
    pub sequence: i64,
    pub kind: String,
    pub name: String,
    pub payload_digest: String,
    pub summary: String,
    pub occurred_at_ms: i64,
}

/// Repository pattern over the agent connection/run/event tables.
pub trait AgentsRepository {
    fn insert_agent_connection(
        &self,
        connection_id: &str,
        label: &str,
        secret_hash: &str,
        scopes_wire: &str,
        created_at_ms: i64,
    ) -> Result<(), StorageError>;
    fn get_agent_connection(
        &self,
        connection_id: &str,
    ) -> Result<Option<AgentConnectionRow>, StorageError>;
    /// Atomically revokes an unrevoked connection. Returns `true` if this
    /// call performed the revocation.
    fn revoke_agent_connection(
        &self,
        connection_id: &str,
        now_ms: i64,
    ) -> Result<bool, StorageError>;
    fn get_agent_run(&self, run_id: &str) -> Result<Option<AgentRunRow>, StorageError>;
    fn get_agent_run_by_idempotency_key(
        &self,
        connection_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<AgentRunRow>, StorageError>;
    fn insert_agent_run(&self, run: &AgentRunRow) -> Result<(), StorageError>;
    /// Atomically transitions a running run to a terminal state. Returns
    /// `true` if this call performed the transition (row existed and was
    /// still `running`).
    fn finish_agent_run(
        &self,
        run_id: &str,
        state_wire: &str,
        now_ms: i64,
    ) -> Result<bool, StorageError>;
    /// Returns `Ok(false)` (not `Err`) on a `UNIQUE(run_id, sequence)`
    /// constraint violation, mirroring `crates/loki-agents`'s existing
    /// `AgentError::SequenceConflict` mapping — the caller distinguishes
    /// "already recorded at this sequence" from every other database error.
    fn insert_agent_run_event(&self, event: &NewRunEvent) -> Result<bool, StorageError>;
    /// Real durable generation read for an agent-connection principal — see
    /// this module's top doc comment.
    fn find_agent_generation(
        &self,
        connection_id: &str,
        generation: u64,
    ) -> Result<Option<AgentGrantGenerationRow>, StorageError>;
    /// Atomically mints a new secret hash for an active connection and
    /// bumps its generation by exactly `1`, under a guarded
    /// `UPDATE ... WHERE connection_id = ? AND revoked_at_ms IS NULL`.
    /// Returns the new generation, or `None` if the connection does not
    /// exist or is already revoked (the caller disambiguates those two
    /// cases with a follow-up `get_agent_connection` read, mirroring
    /// `revoke_agent_connection`'s existing exists-vs-revoked pattern).
    fn rotate_agent_connection(
        &self,
        connection_id: &str,
        new_secret_hash: &str,
        now_ms: i64,
    ) -> Result<Option<u64>, StorageError>;
    /// Atomically replaces an active connection's stored scopes and bumps
    /// its generation by exactly `1`, same atomicity/guard shape as
    /// [`AgentsRepository::rotate_agent_connection`].
    fn change_agent_scopes(
        &self,
        connection_id: &str,
        new_scopes_wire: &str,
        now_ms: i64,
    ) -> Result<Option<u64>, StorageError>;
}

impl AgentsRepository for Storage {
    fn insert_agent_connection(
        &self,
        connection_id: &str,
        label: &str,
        secret_hash: &str,
        scopes_wire: &str,
        created_at_ms: i64,
    ) -> Result<(), StorageError> {
        let conn = self.lock();
        insert_agent_connection_core(
            &conn,
            connection_id,
            label,
            secret_hash,
            scopes_wire,
            created_at_ms,
        )
    }

    fn get_agent_connection(
        &self,
        connection_id: &str,
    ) -> Result<Option<AgentConnectionRow>, StorageError> {
        let conn = self.lock();
        get_agent_connection_core(&conn, connection_id)
    }

    fn revoke_agent_connection(
        &self,
        connection_id: &str,
        now_ms: i64,
    ) -> Result<bool, StorageError> {
        let conn = self.lock();
        revoke_agent_connection_core(&conn, connection_id, now_ms)
    }

    fn get_agent_run(&self, run_id: &str) -> Result<Option<AgentRunRow>, StorageError> {
        let conn = self.lock();
        get_agent_run_core(&conn, run_id)
    }

    fn get_agent_run_by_idempotency_key(
        &self,
        connection_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<AgentRunRow>, StorageError> {
        let conn = self.lock();
        get_agent_run_by_idempotency_key_core(&conn, connection_id, idempotency_key)
    }

    fn insert_agent_run(&self, run: &AgentRunRow) -> Result<(), StorageError> {
        let conn = self.lock();
        insert_agent_run_core(&conn, run)
    }

    fn finish_agent_run(
        &self,
        run_id: &str,
        state_wire: &str,
        now_ms: i64,
    ) -> Result<bool, StorageError> {
        let conn = self.lock();
        finish_agent_run_core(&conn, run_id, state_wire, now_ms)
    }

    fn insert_agent_run_event(&self, event: &NewRunEvent) -> Result<bool, StorageError> {
        let conn = self.lock();
        insert_agent_run_event_core(&conn, event)
    }

    fn find_agent_generation(
        &self,
        connection_id: &str,
        generation: u64,
    ) -> Result<Option<AgentGrantGenerationRow>, StorageError> {
        let conn = self.lock();
        find_agent_generation_core(&conn, connection_id, generation)
    }

    fn rotate_agent_connection(
        &self,
        connection_id: &str,
        new_secret_hash: &str,
        now_ms: i64,
    ) -> Result<Option<u64>, StorageError> {
        let conn = self.lock();
        rotate_agent_connection_core(&conn, connection_id, new_secret_hash, now_ms)
    }

    fn change_agent_scopes(
        &self,
        connection_id: &str,
        new_scopes_wire: &str,
        now_ms: i64,
    ) -> Result<Option<u64>, StorageError> {
        let conn = self.lock();
        change_agent_scopes_core(&conn, connection_id, new_scopes_wire, now_ms)
    }
}

/// `generation` starts at `1` and `generation_issued_at_ms` is set to
/// `created_at_ms` — both are already-known constants derived from this
/// function's own parameters, so no additional Rust-level input is needed
/// (this module's top doc comment).
pub fn insert_agent_connection_core(
    conn: &Connection,
    connection_id: &str,
    label: &str,
    secret_hash: &str,
    scopes_wire: &str,
    created_at_ms: i64,
) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO agent_connections(connection_id,label,secret_hash,scopes,created_at_ms,generation,generation_issued_at_ms) VALUES(?1,?2,?3,?4,?5,1,?5)",
        params![connection_id, label, secret_hash, scopes_wire, created_at_ms],
    )?;
    Ok(())
}

pub fn get_agent_connection_core(
    conn: &Connection,
    connection_id: &str,
) -> Result<Option<AgentConnectionRow>, StorageError> {
    conn.query_row(
        "SELECT connection_id,label,secret_hash,scopes,created_at_ms,revoked_at_ms,generation,generation_issued_at_ms FROM agent_connections WHERE connection_id=?1",
        params![connection_id],
        row_to_connection,
    )
    .optional()
    .map_err(Into::into)
}

/// Also bumps `generation` by exactly `1` (defensive — see this module's
/// top doc comment; `revoked_at_ms` already independently fail-closes).
pub fn revoke_agent_connection_core(
    conn: &Connection,
    connection_id: &str,
    now_ms: i64,
) -> Result<bool, StorageError> {
    let changed = conn.execute(
        "UPDATE agent_connections SET revoked_at_ms=?1, generation = generation + 1 WHERE connection_id=?2 AND revoked_at_ms IS NULL",
        params![now_ms, connection_id],
    )?;
    Ok(changed == 1)
}

/// Real durable generation read for an agent-connection principal —
/// unique-by-invariant per `(connection_id, generation)` (this module's
/// top doc comment: a connection is a single mutable row with exactly one
/// live generation at a time).
pub fn find_agent_generation_core(
    conn: &Connection,
    connection_id: &str,
    generation: u64,
) -> Result<Option<AgentGrantGenerationRow>, StorageError> {
    let generation_i64 = i64::try_from(generation).map_err(|_| StorageError::InvalidId {
        kind: "AgentConnectionGeneration",
        reason: "generation exceeds representable range".to_string(),
    })?;
    conn.query_row(
        "SELECT connection_id, generation_issued_at_ms, revoked_at_ms FROM agent_connections
         WHERE connection_id = ?1 AND generation = ?2",
        params![connection_id, generation_i64],
        |row| {
            Ok(AgentGrantGenerationRow {
                connection_id: row.get(0)?,
                generation,
                issued_at_ms: row.get(1)?,
                revoked_at_ms: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

/// Atomically mints a new secret hash for an active connection and bumps
/// its generation by exactly `1`. See this module's top doc comment and
/// `AgentsRepository::rotate_agent_connection`'s doc comment.
pub fn rotate_agent_connection_core(
    conn: &Connection,
    connection_id: &str,
    new_secret_hash: &str,
    now_ms: i64,
) -> Result<Option<u64>, StorageError> {
    let changed = conn.execute(
        "UPDATE agent_connections
         SET secret_hash = ?1, generation = generation + 1, generation_issued_at_ms = ?2
         WHERE connection_id = ?3 AND revoked_at_ms IS NULL",
        params![new_secret_hash, now_ms, connection_id],
    )?;
    if changed != 1 {
        return Ok(None);
    }
    read_generation(conn, connection_id)
}

/// Atomically replaces an active connection's stored scopes and bumps its
/// generation by exactly `1`. See this module's top doc comment and
/// `AgentsRepository::change_agent_scopes`'s doc comment.
pub fn change_agent_scopes_core(
    conn: &Connection,
    connection_id: &str,
    new_scopes_wire: &str,
    now_ms: i64,
) -> Result<Option<u64>, StorageError> {
    let changed = conn.execute(
        "UPDATE agent_connections
         SET scopes = ?1, generation = generation + 1, generation_issued_at_ms = ?2
         WHERE connection_id = ?3 AND revoked_at_ms IS NULL",
        params![new_scopes_wire, now_ms, connection_id],
    )?;
    if changed != 1 {
        return Ok(None);
    }
    read_generation(conn, connection_id)
}

fn read_generation(conn: &Connection, connection_id: &str) -> Result<Option<u64>, StorageError> {
    let generation: i64 = conn.query_row(
        "SELECT generation FROM agent_connections WHERE connection_id = ?1",
        params![connection_id],
        |row| row.get(0),
    )?;
    Ok(Some(u64::try_from(generation).unwrap_or(0)))
}

fn row_to_connection(row: &Row<'_>) -> rusqlite::Result<AgentConnectionRow> {
    Ok(AgentConnectionRow {
        connection_id: row.get(0)?,
        label: row.get(1)?,
        secret_hash: row.get(2)?,
        scopes: row.get(3)?,
        created_at_ms: row.get(4)?,
        revoked_at_ms: row.get(5)?,
        generation: {
            let raw: i64 = row.get(6)?;
            u64::try_from(raw).unwrap_or(0)
        },
        generation_issued_at_ms: row.get(7)?,
    })
}

pub fn get_agent_run_core(
    conn: &Connection,
    run_id: &str,
) -> Result<Option<AgentRunRow>, StorageError> {
    conn.query_row(
        &format!("SELECT {RUN_COLUMNS} FROM agent_runs WHERE run_id=?1"),
        params![run_id],
        row_to_run,
    )
    .optional()
    .map_err(Into::into)
}

pub fn get_agent_run_by_idempotency_key_core(
    conn: &Connection,
    connection_id: &str,
    idempotency_key: &str,
) -> Result<Option<AgentRunRow>, StorageError> {
    conn.query_row(
        &format!(
            "SELECT {RUN_COLUMNS} FROM agent_runs WHERE connection_id=?1 AND idempotency_key=?2"
        ),
        params![connection_id, idempotency_key],
        row_to_run,
    )
    .optional()
    .map_err(Into::into)
}

pub fn insert_agent_run_core(conn: &Connection, run: &AgentRunRow) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO agent_runs(run_id,connection_id,parent_run_id,correlation_id,idempotency_key,state,started_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            run.run_id,
            run.connection_id,
            run.parent_run_id,
            run.correlation_id,
            run.idempotency_key,
            run.state,
            run.started_at_ms,
        ],
    )?;
    Ok(())
}

pub fn finish_agent_run_core(
    conn: &Connection,
    run_id: &str,
    state_wire: &str,
    now_ms: i64,
) -> Result<bool, StorageError> {
    let changed = conn.execute(
        "UPDATE agent_runs SET state=?1,completed_at_ms=?2 WHERE run_id=?3 AND state='running'",
        params![state_wire, now_ms, run_id],
    )?;
    Ok(changed == 1)
}

pub fn insert_agent_run_event_core(
    conn: &Connection,
    event: &NewRunEvent,
) -> Result<bool, StorageError> {
    let result = conn.execute(
        "INSERT INTO agent_run_events(run_id,sequence,kind,name,payload_digest,summary,occurred_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            event.run_id,
            event.sequence,
            event.kind,
            event.name,
            event.payload_digest,
            event.summary,
            event.occurred_at_ms,
        ],
    );
    match result {
        Ok(_) => Ok(true),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Ok(false)
        }
        Err(e) => Err(e.into()),
    }
}

fn row_to_run(row: &Row<'_>) -> rusqlite::Result<AgentRunRow> {
    Ok(AgentRunRow {
        run_id: row.get(0)?,
        connection_id: row.get(1)?,
        parent_run_id: row.get(2)?,
        correlation_id: row.get(3)?,
        idempotency_key: row.get(4)?,
        state: row.get(5)?,
        started_at_ms: row.get(6)?,
        completed_at_ms: row.get(7)?,
    })
}
