//! Scoped, revocable local agent connections and durable run events (`AGT-001`).
//!
//! An MCP transport session is not authority. A caller must present the
//! one-time-returned connection secret and hold the specific registered scope.
//!
//! # `P1-ACCEPTANCE-STORE-001` consolidation
//!
//! Previously wrapped its own independent `Mutex<Connection>`, opened via
//! `loki_storage::open_or_migrate_sqlcipher`/`Connection::open_in_memory()`,
//! running its own ad hoc `CREATE TABLE IF NOT EXISTS` schema (three
//! tables). Now wraps a shared `loki_storage::Storage`, with schema owned
//! by `loki-storage`'s own versioned migrations (`migrations/0005_agents.sql`)
//! and all SQL moved into `loki_storage::agents`. This is what makes a real
//! cross-repository `Storage::transaction()`
//! (`Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`,
//! `crates/loki-acceptance-store`) possible for `AgentRunRegister`/
//! `AgentRunUpdate`.

use std::{fmt, path::Path, sync::Arc};

use loki_local_authority::{AuthorityError, LocalAuthority, LocalCapability, LocalContext};
use loki_storage::agents::{
    AgentConnectionRow, AgentRunRow, AgentsRepository, NewRunEvent, finish_agent_run_core,
    get_agent_connection_core, get_agent_run_core,
};
use loki_storage::{Cipher, Storage};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const MAX_FIELD: usize = 256;
const MAX_SUMMARY: usize = 2_000;

/// See `crates/loki-local-authority/src/store.rs`'s identical constant doc
/// comment — the same rationale applies here.
const BACK_COMPAT_MIGRATION_TIMESTAMP: &str = "1970-01-01T00:00:00Z";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AgentScope {
    ReadDiagnostics,
    RegisterRuns,
    RecordToolUse,
    RecordChanges,
    ReceiveWorkIntents,
}
impl AgentScope {
    fn wire(self) -> &'static str {
        match self {
            Self::ReadDiagnostics => "read_diagnostics",
            Self::RegisterRuns => "register_runs",
            Self::RecordToolUse => "record_tool_use",
            Self::RecordChanges => "record_changes",
            Self::ReceiveWorkIntents => "receive_work_intents",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        match value {
            "read_diagnostics" => Some(Self::ReadDiagnostics),
            "register_runs" => Some(Self::RegisterRuns),
            "record_tool_use" => Some(Self::RecordToolUse),
            "record_changes" => Some(Self::RecordChanges),
            "receive_work_intents" => Some(Self::ReceiveWorkIntents),
            _ => None,
        }
    }
}

pub struct AgentSecret(String);
impl AgentSecret {
    pub fn expose_for_client_configuration(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for AgentSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AgentSecret([REDACTED])")
    }
}

/// `P1-PRINCIPAL-GRANT-001` (ADR-0007 Option B): `generation` is a real,
/// durable, per-`connection_id` counter — starts at `1` on `register`
/// (a known constant, no storage round-trip needed for this value) and is
/// bumped by exactly `1`, atomically, by the storage layer on `rotate`,
/// `change_scopes`, and (defensively) `revoke`. This is entirely
/// independent of `crates/loki-local-authority`'s `LocalSession` generation
/// sequence: revoking an agent connection never cascades to revoke any
/// Windows-principal session, and vice versa (ADR-0007 explicitly declines
/// a shared "one kill switch" property).
#[derive(Debug)]
pub struct AgentCredential {
    pub connection_id: String,
    pub secret: AgentSecret,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
impl RunState {
    fn wire(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
    fn terminal(self) -> bool {
        self != Self::Running
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRun {
    pub run_id: String,
    pub connection_id: String,
    pub parent_run_id: Option<String>,
    pub correlation_id: String,
    pub idempotency_key: String,
    pub state: RunState,
    pub started_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Tool,
    Change,
}
impl EventKind {
    fn wire(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Change => "change",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEvent {
    pub run_id: String,
    pub sequence: i64,
    pub kind: EventKind,
    pub name: String,
    pub payload_digest: String,
    pub summary: String,
    pub occurred_at_ms: i64,
}

#[derive(Debug)]
pub enum AgentError {
    Authority(AuthorityError),
    Sqlite(rusqlite::Error),
    Storage(loki_storage::StorageError),
    InvalidInput { field: &'static str, reason: String },
    NotFound,
    Revoked,
    AuthenticationFailed,
    ScopeDenied,
    ParentMismatch,
    InvalidTransition,
    TerminalRun,
    SequenceConflict,
    Conflict(String),
}
impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Authority(_) => "local authority denied agent action",
                Self::Sqlite(_) => "agent database error",
                Self::Storage(_) => "encrypted agent database open failed",
                Self::InvalidInput { .. } => "invalid agent input",
                Self::NotFound => "agent record not found",
                Self::Revoked => "agent connection revoked",
                Self::AuthenticationFailed => "agent authentication failed",
                Self::ScopeDenied => "agent scope denied",
                Self::ParentMismatch => "parent run belongs to another connection",
                Self::InvalidTransition => "invalid run transition",
                Self::TerminalRun => "run is terminal",
                Self::SequenceConflict => "event sequence conflict",
                Self::Conflict(_) => "agent registry conflict",
            }
        )
    }
}
impl std::error::Error for AgentError {}
impl From<AuthorityError> for AgentError {
    fn from(v: AuthorityError) -> Self {
        Self::Authority(v)
    }
}
impl From<rusqlite::Error> for AgentError {
    fn from(v: rusqlite::Error) -> Self {
        Self::Sqlite(v)
    }
}
impl From<loki_storage::StorageError> for AgentError {
    fn from(value: loki_storage::StorageError) -> Self {
        Self::Storage(value)
    }
}

pub struct AgentRegistry {
    storage: Arc<Storage>,
}
impl AgentRegistry {
    /// Back-compat convenience constructor: builds a private, dedicated,
    /// fully migrated `Storage` at `path` (Windows DPAPI-managed SQLCipher),
    /// then delegates to [`AgentRegistry::new`].
    pub fn open(
        path: impl AsRef<Path>,
        protected_key_path: impl AsRef<Path>,
    ) -> Result<Self, AgentError> {
        let storage = Storage::open_managed(path.as_ref(), protected_key_path.as_ref())?;
        storage.run_migrations(BACK_COMPAT_MIGRATION_TIMESTAMP)?;
        Ok(Self::new(Arc::new(storage)))
    }

    /// Back-compat convenience constructor: builds a private, dedicated,
    /// fully migrated in-memory `Storage`, then delegates to
    /// [`AgentRegistry::new`].
    pub fn open_in_memory() -> Result<Self, AgentError> {
        let storage = Storage::open_in_memory(Cipher::None)?;
        storage.run_migrations(BACK_COMPAT_MIGRATION_TIMESTAMP)?;
        Ok(Self::new(Arc::new(storage)))
    }

    /// Constructs an `AgentRegistry` over an already-open, already-migrated,
    /// shared `Storage` — the `P1-ACCEPTANCE-STORE-001` composition path.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    pub fn register(
        &self,
        authority: &LocalAuthority,
        context: &LocalContext,
        session_id: &str,
        label: &str,
        scopes: &[AgentScope],
        now_ms: i64,
    ) -> Result<AgentCredential, AgentError> {
        authority.authorize_local(context, session_id, LocalCapability::RegisterAgent, now_ms)?;
        validate("label", label, MAX_FIELD)?;
        if scopes.is_empty() {
            return Err(AgentError::InvalidInput {
                field: "scopes",
                reason: "at least one scope required".into(),
            });
        }
        let mut normalized = scopes.to_vec();
        normalized.sort();
        normalized.dedup();
        let scopes_wire = normalized
            .iter()
            .map(|s| s.wire())
            .collect::<Vec<_>>()
            .join(",");
        let secret = AgentSecret(format!(
            "las_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ));
        let secret_hash = hash_secret(&secret.0);
        let credential = AgentCredential {
            connection_id: format!("ac_{}", Uuid::new_v4().simple()),
            secret,
            // A known constant: `insert_agent_connection_core` always sets
            // `generation = 1` for a brand-new row — no storage round-trip
            // needed to know this value (this module's/that module's doc
            // comments).
            generation: 1,
        };
        self.storage.insert_agent_connection(
            &credential.connection_id,
            label,
            &secret_hash,
            &scopes_wire,
            now_ms,
        )?;
        Ok(credential)
    }

    pub fn revoke(
        &self,
        authority: &LocalAuthority,
        context: &LocalContext,
        session_id: &str,
        connection_id: &str,
        now_ms: i64,
    ) -> Result<(), AgentError> {
        authority.authorize_local(context, session_id, LocalCapability::RegisterAgent, now_ms)?;
        let revoked = self
            .storage
            .revoke_agent_connection(connection_id, now_ms)?;
        if !revoked {
            let exists = self.storage.get_agent_connection(connection_id)?.is_some();
            return Err(if exists {
                AgentError::Revoked
            } else {
                AgentError::NotFound
            });
        }
        Ok(())
    }

    /// Mints a new one-time secret for an already-registered connection and
    /// atomically bumps its durable generation by exactly `1`
    /// (`P1-PRINCIPAL-GRANT-001`, ADR-0007 Option B). Uses the exact same
    /// `LocalCapability::RegisterAgent` authorization gate `register`/
    /// `revoke` already require — no new `LocalCapability` variant (out of
    /// this node's forbidden scope). The connection's prior secret stops
    /// authenticating the moment this call commits.
    pub fn rotate(
        &self,
        authority: &LocalAuthority,
        context: &LocalContext,
        session_id: &str,
        connection_id: &str,
        now_ms: i64,
    ) -> Result<AgentCredential, AgentError> {
        authority.authorize_local(context, session_id, LocalCapability::RegisterAgent, now_ms)?;
        let secret = AgentSecret(format!(
            "las_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ));
        let secret_hash = hash_secret(&secret.0);
        let new_generation =
            self.storage
                .rotate_agent_connection(connection_id, &secret_hash, now_ms)?;
        let generation = match new_generation {
            Some(generation) => generation,
            None => {
                let exists = self.storage.get_agent_connection(connection_id)?.is_some();
                return Err(if exists {
                    AgentError::Revoked
                } else {
                    AgentError::NotFound
                });
            }
        };
        Ok(AgentCredential {
            connection_id: connection_id.to_string(),
            secret,
            generation,
        })
    }

    /// Replaces an already-registered connection's authorized scope set and
    /// atomically bumps its durable generation by exactly `1`
    /// (`P1-PRINCIPAL-GRANT-001`, ADR-0007 Option B). Same authorization
    /// gate and same non-empty/sort/dedup scope validation `register`
    /// already applies. Returns the bare new generation number — no secret
    /// is involved in a scope change, so returning a fabricated/reused
    /// secret here would be dishonest (`AGENTS.md` rule 1).
    pub fn change_scopes(
        &self,
        authority: &LocalAuthority,
        context: &LocalContext,
        session_id: &str,
        connection_id: &str,
        new_scopes: &[AgentScope],
        now_ms: i64,
    ) -> Result<u64, AgentError> {
        authority.authorize_local(context, session_id, LocalCapability::RegisterAgent, now_ms)?;
        if new_scopes.is_empty() {
            return Err(AgentError::InvalidInput {
                field: "scopes",
                reason: "at least one scope required".into(),
            });
        }
        let mut normalized = new_scopes.to_vec();
        normalized.sort();
        normalized.dedup();
        let scopes_wire = normalized
            .iter()
            .map(|s| s.wire())
            .collect::<Vec<_>>()
            .join(",");
        let new_generation =
            self.storage
                .change_agent_scopes(connection_id, &scopes_wire, now_ms)?;
        match new_generation {
            Some(generation) => Ok(generation),
            None => {
                let exists = self.storage.get_agent_connection(connection_id)?.is_some();
                Err(if exists {
                    AgentError::Revoked
                } else {
                    AgentError::NotFound
                })
            }
        }
    }

    pub fn require_active_connection(&self, connection_id: &str) -> Result<(), AgentError> {
        let row = self.storage.get_agent_connection(connection_id)?;
        match row {
            None => Err(AgentError::NotFound),
            Some(row) if row.revoked_at_ms.is_some() => Err(AgentError::Revoked),
            Some(_) => Ok(()),
        }
    }

    /// Authenticates an active local connection for one explicit scope.
    /// Transport presence alone is never authority.
    pub fn authorize_scope(
        &self,
        connection_id: &str,
        secret: &str,
        scope: AgentScope,
    ) -> Result<(), AgentError> {
        self.authenticate(connection_id, secret, scope)
    }

    /// Returns a run only when it belongs to the named connection.
    pub fn require_run_for_connection(
        &self,
        connection_id: &str,
        run_id: &str,
    ) -> Result<AgentRun, AgentError> {
        let run = load_run(&self.storage, run_id)?.ok_or(AgentError::NotFound)?;
        if run.connection_id != connection_id {
            return Err(AgentError::AuthenticationFailed);
        }
        Ok(run)
    }

    pub fn start_run(
        &self,
        connection_id: &str,
        secret: &str,
        idempotency_key: &str,
        parent_run_id: Option<&str>,
        now_ms: i64,
    ) -> Result<AgentRun, AgentError> {
        self.authenticate(connection_id, secret, AgentScope::RegisterRuns)?;
        validate("idempotency_key", idempotency_key, MAX_FIELD)?;
        if let Some(existing) = self
            .storage
            .get_agent_run_by_idempotency_key(connection_id, idempotency_key)?
        {
            return row_to_agent_run(existing);
        }
        if let Some(parent) = parent_run_id {
            let p = load_run(&self.storage, parent)?.ok_or(AgentError::NotFound)?;
            if p.connection_id != connection_id {
                return Err(AgentError::ParentMismatch);
            }
        }
        let run = AgentRun {
            run_id: format!("ar_{}", Uuid::new_v4().simple()),
            connection_id: connection_id.into(),
            parent_run_id: parent_run_id.map(str::to_string),
            correlation_id: format!("corr_{}", Uuid::new_v4().simple()),
            idempotency_key: idempotency_key.into(),
            state: RunState::Running,
            started_at_ms: now_ms,
            completed_at_ms: None,
        };
        self.storage.insert_agent_run(&AgentRunRow {
            run_id: run.run_id.clone(),
            connection_id: run.connection_id.clone(),
            parent_run_id: run.parent_run_id.clone(),
            correlation_id: run.correlation_id.clone(),
            idempotency_key: run.idempotency_key.clone(),
            state: run.state.wire().to_string(),
            started_at_ms: run.started_at_ms,
            completed_at_ms: run.completed_at_ms,
        })?;
        Ok(run)
    }

    pub fn finish_run(
        &self,
        connection_id: &str,
        secret: &str,
        run_id: &str,
        state: RunState,
        now_ms: i64,
    ) -> Result<(), AgentError> {
        if !state.terminal() {
            return Err(AgentError::InvalidTransition);
        }
        self.storage.transaction(|tx| {
            let Some(connection) = get_agent_connection_core(tx, connection_id)? else {
                return Ok(Err(AgentError::AuthenticationFailed));
            };
            if let Err(error) =
                authenticate_connection(&connection, secret, AgentScope::RegisterRuns)
            {
                return Ok(Err(error));
            }
            let Some(run) = get_agent_run_core(tx, run_id)? else {
                return Ok(Err(AgentError::NotFound));
            };
            let run = match row_to_agent_run(run) {
                Ok(run) => run,
                Err(error) => return Ok(Err(error)),
            };
            if run.connection_id != connection_id {
                return Ok(Err(AgentError::AuthenticationFailed));
            }
            if run.state.terminal() {
                return Ok(if run.state == state {
                    Ok(())
                } else {
                    Err(AgentError::TerminalRun)
                });
            }
            if finish_agent_run_core(tx, run_id, state.wire(), now_ms)? {
                return Ok(Ok(()));
            }
            let Some(observed) = get_agent_run_core(tx, run_id)? else {
                return Ok(Err(AgentError::NotFound));
            };
            let observed = match row_to_agent_run(observed) {
                Ok(run) => run,
                Err(error) => return Ok(Err(error)),
            };
            Ok(if observed.state == state {
                Ok(())
            } else {
                Err(AgentError::TerminalRun)
            })
        })?
    }

    pub fn append_event(
        &self,
        connection_id: &str,
        secret: &str,
        event: &RunEvent,
    ) -> Result<(), AgentError> {
        let scope = match event.kind {
            EventKind::Tool => AgentScope::RecordToolUse,
            EventKind::Change => AgentScope::RecordChanges,
        };
        self.authenticate(connection_id, secret, scope)?;
        validate("name", &event.name, MAX_FIELD)?;
        validate("payload_digest", &event.payload_digest, MAX_FIELD)?;
        validate("summary", &event.summary, MAX_SUMMARY)?;
        if event.sequence <= 0 {
            return Err(AgentError::InvalidInput {
                field: "sequence",
                reason: "must be positive".into(),
            });
        }
        let run = load_run(&self.storage, &event.run_id)?.ok_or(AgentError::NotFound)?;
        if run.connection_id != connection_id {
            return Err(AgentError::AuthenticationFailed);
        }
        if run.state.terminal() {
            return Err(AgentError::TerminalRun);
        }
        let inserted = self.storage.insert_agent_run_event(&NewRunEvent {
            run_id: event.run_id.clone(),
            sequence: event.sequence,
            kind: event.kind.wire().to_string(),
            name: event.name.clone(),
            payload_digest: event.payload_digest.clone(),
            summary: event.summary.clone(),
            occurred_at_ms: event.occurred_at_ms,
        })?;
        if inserted {
            Ok(())
        } else {
            Err(AgentError::SequenceConflict)
        }
    }

    fn authenticate(
        &self,
        connection_id: &str,
        secret: &str,
        scope: AgentScope,
    ) -> Result<(), AgentError> {
        let row = self
            .storage
            .get_agent_connection(connection_id)?
            .ok_or(AgentError::AuthenticationFailed)?;
        authenticate_connection(&row, secret, scope)
    }
}

fn authenticate_connection(
    row: &AgentConnectionRow,
    secret: &str,
    scope: AgentScope,
) -> Result<(), AgentError> {
    if row.revoked_at_ms.is_some() {
        return Err(AgentError::Revoked);
    }
    if !constant_time_eq(row.secret_hash.as_bytes(), hash_secret(secret).as_bytes()) {
        return Err(AgentError::AuthenticationFailed);
    }
    if !row
        .scopes
        .split(',')
        .filter_map(AgentScope::parse)
        .any(|s| s == scope)
    {
        return Err(AgentError::ScopeDenied);
    }
    Ok(())
}

fn hash_secret(value: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
}
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for (i, j) in a.iter().zip(b) {
        d |= i ^ j;
    }
    d == 0
}
fn validate(field: &'static str, value: &str, max: usize) -> Result<(), AgentError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        Err(AgentError::InvalidInput {
            field,
            reason: format!("must contain 1..={max} non-control bytes"),
        })
    } else {
        Ok(())
    }
}

fn load_run(storage: &Storage, id: &str) -> Result<Option<AgentRun>, AgentError> {
    storage.get_agent_run(id)?.map(row_to_agent_run).transpose()
}

fn row_to_agent_run(row: AgentRunRow) -> Result<AgentRun, AgentError> {
    let state = RunState::parse(&row.state)
        .ok_or_else(|| AgentError::Conflict("stored run state invalid".into()))?;
    Ok(AgentRun {
        run_id: row.run_id,
        connection_id: row.connection_id,
        parent_run_id: row.parent_run_id,
        correlation_id: row.correlation_id,
        idempotency_key: row.idempotency_key,
        state,
        started_at_ms: row.started_at_ms,
        completed_at_ms: row.completed_at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(
        scopes: &[AgentScope],
    ) -> (
        LocalAuthority,
        LocalContext,
        String,
        AgentRegistry,
        AgentCredential,
    ) {
        // Even the in-memory path shares this process-wide SQLite
        // installation with this binary's `restart_preserves_revocation`
        // test, which opens a real SqlCipher-backed store through the
        // secure-memory arena (DAT-002-MEM-CODE-001). `sqlite3_config` can
        // only run before SQLite's first implicit initialization, so every
        // SQLite-touching test in this binary — in-memory or not — must
        // bootstrap the arena first, regardless of which test happens to
        // run first.
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let a = LocalAuthority::open_in_memory().unwrap();
        let c = LocalContext::current("ipc-agent").unwrap();
        a.initialize_current(&c, 1000).unwrap();
        let s = a.issue_session(&c, 2000, 100000).unwrap();
        let r = AgentRegistry::open_in_memory().unwrap();
        let cred = r
            .register(&a, &c, &s.session_id, "Codex local", scopes, 3000)
            .unwrap();
        (a, c, s.session_id, r, cred)
    }
    fn all() -> Vec<AgentScope> {
        vec![
            AgentScope::ReadDiagnostics,
            AgentScope::RegisterRuns,
            AgentScope::RecordToolUse,
            AgentScope::RecordChanges,
            AgentScope::ReceiveWorkIntents,
        ]
    }
    #[test]
    fn secret_is_redacted_and_wrong_secret_denied() {
        let (_, _, _, r, c) = setup(&all());
        assert_eq!(format!("{:?}", c.secret), "AgentSecret([REDACTED])");
        assert!(matches!(
            r.start_run(&c.connection_id, "wrong", "i", None, 4000),
            Err(AgentError::AuthenticationFailed)
        ));
    }
    #[test]
    fn scope_and_revocation_fail_closed() {
        let (a, ctx, s, r, c) = setup(&[AgentScope::ReadDiagnostics]);
        assert!(matches!(
            r.start_run(
                &c.connection_id,
                c.secret.expose_for_client_configuration(),
                "i",
                None,
                4000
            ),
            Err(AgentError::ScopeDenied)
        ));
        r.revoke(&a, &ctx, &s, &c.connection_id, 4100).unwrap();
        assert!(matches!(
            r.start_run(
                &c.connection_id,
                c.secret.expose_for_client_configuration(),
                "i",
                None,
                4200
            ),
            Err(AgentError::Revoked)
        ));
    }
    #[test]
    fn run_is_idempotent_and_parent_stays_in_connection() {
        let (_, _, _, r, c) = setup(&all());
        let secret = c.secret.expose_for_client_configuration();
        let p = r
            .start_run(&c.connection_id, secret, "p", None, 4000)
            .unwrap();
        assert_eq!(
            p,
            r.start_run(&c.connection_id, secret, "p", None, 4001)
                .unwrap()
        );
        let child = r
            .start_run(&c.connection_id, secret, "c", Some(&p.run_id), 4002)
            .unwrap();
        assert_eq!(child.parent_run_id.as_deref(), Some(p.run_id.as_str()));
    }
    #[test]
    fn events_are_ordered_unique_and_terminal_rejects_append() {
        let (_, _, _, r, c) = setup(&all());
        let secret = c.secret.expose_for_client_configuration();
        let run = r
            .start_run(&c.connection_id, secret, "i", None, 4000)
            .unwrap();
        let e = RunEvent {
            run_id: run.run_id.clone(),
            sequence: 1,
            kind: EventKind::Tool,
            name: "loki_get_health".into(),
            payload_digest: "sha256:x".into(),
            summary: "read health".into(),
            occurred_at_ms: 4100,
        };
        r.append_event(&c.connection_id, secret, &e).unwrap();
        assert!(matches!(
            r.append_event(&c.connection_id, secret, &e),
            Err(AgentError::SequenceConflict)
        ));
        r.finish_run(
            &c.connection_id,
            secret,
            &run.run_id,
            RunState::Succeeded,
            4200,
        )
        .unwrap();
        let mut next = e;
        next.sequence = 2;
        assert!(matches!(
            r.append_event(&c.connection_id, secret, &next),
            Err(AgentError::TerminalRun)
        ));
    }
    #[test]
    fn invalid_transition_is_denied() {
        let (_, _, _, r, c) = setup(&all());
        let secret = c.secret.expose_for_client_configuration();
        let run = r
            .start_run(&c.connection_id, secret, "i", None, 4000)
            .unwrap();
        assert!(matches!(
            r.finish_run(
                &c.connection_id,
                secret,
                &run.run_id,
                RunState::Running,
                4100
            ),
            Err(AgentError::InvalidTransition)
        ));
        r.finish_run(
            &c.connection_id,
            secret,
            &run.run_id,
            RunState::Failed,
            4200,
        )
        .unwrap();

        // An identical terminal retry is an idempotent replay. It must not
        // rewrite the completion timestamp supplied by the first commit.
        r.finish_run(
            &c.connection_id,
            secret,
            &run.run_id,
            RunState::Failed,
            4300,
        )
        .unwrap();
        let completed = r
            .require_run_for_connection(&c.connection_id, &run.run_id)
            .unwrap();
        assert_eq!(completed.state, RunState::Failed);
        assert_eq!(completed.completed_at_ms, Some(4200));

        // A different terminal result can never replace the committed one.
        assert!(matches!(
            r.finish_run(
                &c.connection_id,
                secret,
                &run.run_id,
                RunState::Succeeded,
                4400
            ),
            Err(AgentError::TerminalRun)
        ));
        let unchanged = r
            .require_run_for_connection(&c.connection_id, &run.run_id)
            .unwrap();
        assert_eq!(unchanged.state, RunState::Failed);
        assert_eq!(unchanged.completed_at_ms, Some(4200));
    }

    #[test]
    fn finish_revalidates_revocation_and_run_ownership_before_transition() {
        let (a, ctx, session_id, r, owner) = setup(&all());
        let other = r
            .register(&a, &ctx, &session_id, "other agent", &all(), 3100)
            .unwrap();
        let run = r
            .start_run(
                &owner.connection_id,
                owner.secret.expose_for_client_configuration(),
                "owned-run",
                None,
                4000,
            )
            .unwrap();

        assert!(matches!(
            r.finish_run(
                &other.connection_id,
                other.secret.expose_for_client_configuration(),
                &run.run_id,
                RunState::Succeeded,
                4100,
            ),
            Err(AgentError::AuthenticationFailed)
        ));
        let still_running = r
            .require_run_for_connection(&owner.connection_id, &run.run_id)
            .unwrap();
        assert_eq!(still_running.state, RunState::Running);
        assert_eq!(still_running.completed_at_ms, None);

        r.revoke(&a, &ctx, &session_id, &owner.connection_id, 4200)
            .unwrap();
        assert!(matches!(
            r.finish_run(
                &owner.connection_id,
                owner.secret.expose_for_client_configuration(),
                &run.run_id,
                RunState::Succeeded,
                4300,
            ),
            Err(AgentError::Revoked)
        ));
        let still_running = r
            .require_run_for_connection(&owner.connection_id, &run.run_id)
            .unwrap();
        assert_eq!(still_running.state, RunState::Running);
        assert_eq!(still_running.completed_at_ms, None);
    }

    #[test]
    fn file_backed_concurrent_terminal_attempts_commit_exactly_one_result() {
        use std::sync::{Arc, Barrier, mpsc};
        use std::time::Duration;

        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("agents.db");
        let key = d.path().join("agents.key.dpapi");
        let a = LocalAuthority::open_in_memory().unwrap();
        let ctx = LocalContext::current("ipc-agent-terminal-race").unwrap();
        a.initialize_current(&ctx, 1000).unwrap();
        let session = a.issue_session(&ctx, 2000, 100000).unwrap();
        let registry = Arc::new(AgentRegistry::open(&p, &key).unwrap());
        let credential = registry
            .register(&a, &ctx, &session.session_id, "agent", &all(), 3000)
            .unwrap();
        let connection_id = credential.connection_id;
        let secret = credential.secret.0;
        let run = registry
            .start_run(&connection_id, &secret, "terminal-race", None, 4000)
            .unwrap();

        let barrier = Arc::new(Barrier::new(3));
        let (send, receive) = mpsc::channel();
        let mut handles = Vec::new();
        for (state, completed_at_ms) in [(RunState::Succeeded, 5000), (RunState::Failed, 6000)] {
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            let send = send.clone();
            let connection_id = connection_id.clone();
            let secret = secret.clone();
            let run_id = run.run_id.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                let result =
                    registry.finish_run(&connection_id, &secret, &run_id, state, completed_at_ms);
                send.send((state, completed_at_ms, result)).unwrap();
            }));
        }
        drop(send);
        barrier.wait();

        let first = receive
            .recv_timeout(Duration::from_secs(5))
            .expect("first terminal attempt must return without re-locking Storage");
        let second = receive
            .recv_timeout(Duration::from_secs(5))
            .expect("second terminal attempt must return without re-locking Storage");
        for handle in handles {
            handle.join().unwrap();
        }

        let outcomes = [first, second];
        assert_eq!(
            outcomes
                .iter()
                .filter(|(_, _, result)| result.is_ok())
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|(_, _, result)| matches!(result, Err(AgentError::TerminalRun)))
                .count(),
            1
        );
        let (winning_state, winning_timestamp, _) = outcomes
            .iter()
            .find(|(_, _, result)| result.is_ok())
            .unwrap();
        let committed = registry
            .require_run_for_connection(&connection_id, &run.run_id)
            .unwrap();
        assert_eq!(committed.state, *winning_state);
        assert_eq!(committed.completed_at_ms, Some(*winning_timestamp));

        // The result remains durable after closing and reopening the real
        // encrypted file-backed registry.
        drop(registry);
        let reopened = AgentRegistry::open(&p, &key).unwrap();
        let durable = reopened
            .require_run_for_connection(&connection_id, &run.run_id)
            .unwrap();
        assert_eq!(durable, committed);
    }
    #[test]
    fn restart_preserves_revocation() {
        // See `setup()`'s comment: every SQLite-touching test in this
        // binary must bootstrap the arena first, regardless of order.
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("agents.db");
        let key = d.path().join("agents.key.dpapi");
        let a = LocalAuthority::open_in_memory().unwrap();
        let ctx = LocalContext::current("ipc").unwrap();
        a.initialize_current(&ctx, 1000).unwrap();
        let s = a.issue_session(&ctx, 2000, 100000).unwrap();
        let id;
        let secret;
        {
            let r = AgentRegistry::open(&p, &key).unwrap();
            let c = r
                .register(&a, &ctx, &s.session_id, "agent", &all(), 3000)
                .unwrap();
            id = c.connection_id;
            secret = c.secret.0;
            r.revoke(&a, &ctx, &s.session_id, &id, 3100).unwrap();
        }
        let r = AgentRegistry::open(&p, &key).unwrap();
        assert!(matches!(
            r.start_run(&id, &secret, "i", None, 4000),
            Err(AgentError::Revoked)
        ));
        assert!(
            !std::fs::read(&p)
                .unwrap()
                .windows(b"agent_connections".len())
                .any(|window| window == b"agent_connections"),
            "agent schema marker must not be plaintext on disk"
        );
    }

    #[test]
    fn rotate_replaces_secret_and_bumps_generation() {
        let (a, ctx, s, r, c) = setup(&all());
        assert_eq!(c.generation, 1);
        let old_secret = c.secret.expose_for_client_configuration().to_string();

        let rotated = r.rotate(&a, &ctx, &s, &c.connection_id, 5000).unwrap();
        assert_eq!(rotated.connection_id, c.connection_id);
        assert_eq!(rotated.generation, 2);

        // Old secret authentication fails after rotate.
        assert!(matches!(
            r.start_run(&c.connection_id, &old_secret, "i", None, 5100),
            Err(AgentError::AuthenticationFailed)
        ));
        // New secret authenticates.
        r.start_run(
            &c.connection_id,
            rotated.secret.expose_for_client_configuration(),
            "i",
            None,
            5200,
        )
        .unwrap();
    }

    #[test]
    fn rotate_denied_for_revoked_connection() {
        let (a, ctx, s, r, c) = setup(&all());
        r.revoke(&a, &ctx, &s, &c.connection_id, 5000).unwrap();
        assert!(matches!(
            r.rotate(&a, &ctx, &s, &c.connection_id, 5100),
            Err(AgentError::Revoked)
        ));
    }

    #[test]
    fn rotate_denied_for_unknown_connection() {
        let (a, ctx, s, r, _c) = setup(&all());
        assert!(matches!(
            r.rotate(&a, &ctx, &s, "ac_does_not_exist", 5000),
            Err(AgentError::NotFound)
        ));
    }

    #[test]
    fn scope_change_updates_authorization_and_bumps_generation() {
        let (a, ctx, s, r, c) = setup(&[AgentScope::ReadDiagnostics]);
        let secret = c.secret.expose_for_client_configuration();
        assert!(matches!(
            r.start_run(&c.connection_id, secret, "i", None, 4000),
            Err(AgentError::ScopeDenied)
        ));

        let new_generation = r
            .change_scopes(
                &a,
                &ctx,
                &s,
                &c.connection_id,
                &[AgentScope::RegisterRuns],
                5000,
            )
            .unwrap();
        assert_eq!(new_generation, 2);

        // Old scope set (ReadDiagnostics) is no longer sufficient, but the
        // newly granted scope (RegisterRuns) now authorizes the same call
        // that was denied above.
        r.start_run(&c.connection_id, secret, "i", None, 5100)
            .unwrap();
    }

    #[test]
    fn scope_change_denied_for_revoked_connection() {
        let (a, ctx, s, r, c) = setup(&all());
        r.revoke(&a, &ctx, &s, &c.connection_id, 5000).unwrap();
        assert!(matches!(
            r.change_scopes(
                &a,
                &ctx,
                &s,
                &c.connection_id,
                &[AgentScope::RegisterRuns],
                5100
            ),
            Err(AgentError::Revoked)
        ));
    }

    #[test]
    fn scope_change_rejects_empty_scope_set() {
        let (a, ctx, s, r, c) = setup(&all());
        assert!(matches!(
            r.change_scopes(&a, &ctx, &s, &c.connection_id, &[], 5000),
            Err(AgentError::InvalidInput {
                field: "scopes",
                ..
            })
        ));
    }

    /// Mirrors `restart_preserves_revocation`'s real on-disk-DB style:
    /// a rotated connection's generation must survive a full close/reopen
    /// of the encrypted database, not merely an in-process value.
    #[test]
    fn restart_preserves_generation_across_reopen() {
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("agents.db");
        let key = d.path().join("agents.key.dpapi");
        let a = LocalAuthority::open_in_memory().unwrap();
        let ctx = LocalContext::current("ipc").unwrap();
        a.initialize_current(&ctx, 1000).unwrap();
        let s = a.issue_session(&ctx, 2000, 100000).unwrap();
        let id;
        let new_secret;
        {
            let r = AgentRegistry::open(&p, &key).unwrap();
            let c = r
                .register(&a, &ctx, &s.session_id, "agent", &all(), 3000)
                .unwrap();
            id = c.connection_id;
            let rotated = r.rotate(&a, &ctx, &s.session_id, &id, 3100).unwrap();
            assert_eq!(rotated.generation, 2);
            new_secret = rotated.secret.0;
        }
        let r = AgentRegistry::open(&p, &key).unwrap();
        // The rotated (not the original) secret must still authenticate
        // after reopening.
        r.start_run(&id, &new_secret, "i", None, 4000).unwrap();
        // The generation must have durably survived at its post-rotate
        // value (2), not reset to 1 across the close/reopen: rotating
        // again after reopen must yield 3, not 2.
        let rotated_again = r.rotate(&a, &ctx, &s.session_id, &id, 4100).unwrap();
        assert_eq!(rotated_again.generation, 3);
    }
}
