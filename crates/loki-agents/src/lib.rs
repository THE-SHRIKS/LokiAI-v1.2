//! Scoped, revocable local agent connections and durable run events (`AGT-001`).
//!
//! An MCP transport session is not authority. A caller must present the
//! one-time-returned connection secret and hold the specific registered scope.

use std::{fmt, path::Path, sync::Mutex};

use loki_local_authority::{AuthorityError, LocalAuthority, LocalCapability, LocalContext};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const MAX_FIELD: usize = 256;
const MAX_SUMMARY: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AgentScope {
    ReadDiagnostics,
    RegisterRuns,
    RecordToolUse,
    RecordChanges,
}
impl AgentScope {
    fn wire(self) -> &'static str {
        match self {
            Self::ReadDiagnostics => "read_diagnostics",
            Self::RegisterRuns => "register_runs",
            Self::RecordToolUse => "record_tool_use",
            Self::RecordChanges => "record_changes",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        match value {
            "read_diagnostics" => Some(Self::ReadDiagnostics),
            "register_runs" => Some(Self::RegisterRuns),
            "record_tool_use" => Some(Self::RecordToolUse),
            "record_changes" => Some(Self::RecordChanges),
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

#[derive(Debug)]
pub struct AgentCredential {
    pub connection_id: String,
    pub secret: AgentSecret,
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
    connection: Mutex<Connection>,
}
impl AgentRegistry {
    pub fn open(
        path: impl AsRef<Path>,
        protected_key_path: impl AsRef<Path>,
    ) -> Result<Self, AgentError> {
        let (connection, _key) =
            loki_storage::open_or_migrate_sqlcipher(path.as_ref(), protected_key_path.as_ref())?;
        Self::from_connection(connection)
    }
    pub fn open_in_memory() -> Result<Self, AgentError> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(connection: Connection) -> Result<Self, AgentError> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS agent_connections(
               connection_id TEXT PRIMARY KEY, label TEXT NOT NULL, secret_hash TEXT NOT NULL,
               scopes TEXT NOT NULL, created_at_ms INTEGER NOT NULL, revoked_at_ms INTEGER);
             CREATE TABLE IF NOT EXISTS agent_runs(
               run_id TEXT PRIMARY KEY, connection_id TEXT NOT NULL, parent_run_id TEXT,
               correlation_id TEXT NOT NULL UNIQUE, idempotency_key TEXT NOT NULL,
               state TEXT NOT NULL, started_at_ms INTEGER NOT NULL, completed_at_ms INTEGER,
               UNIQUE(connection_id,idempotency_key),
               FOREIGN KEY(connection_id) REFERENCES agent_connections(connection_id),
               FOREIGN KEY(parent_run_id) REFERENCES agent_runs(run_id));
             CREATE TABLE IF NOT EXISTS agent_run_events(
               run_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK(sequence>0), kind TEXT NOT NULL,
               name TEXT NOT NULL, payload_digest TEXT NOT NULL, summary TEXT NOT NULL,
               occurred_at_ms INTEGER NOT NULL, PRIMARY KEY(run_id,sequence),
               FOREIGN KEY(run_id) REFERENCES agent_runs(run_id));")?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
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
        };
        self.lock()?.execute("INSERT INTO agent_connections(connection_id,label,secret_hash,scopes,created_at_ms) VALUES(?1,?2,?3,?4,?5)",params![credential.connection_id,label,secret_hash,scopes_wire,now_ms])?;
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
        let changed=self.lock()?.execute("UPDATE agent_connections SET revoked_at_ms=?1 WHERE connection_id=?2 AND revoked_at_ms IS NULL",params![now_ms,connection_id])?;
        if changed == 0 {
            let exists = self.connection_exists(connection_id)?;
            return Err(if exists {
                AgentError::Revoked
            } else {
                AgentError::NotFound
            });
        }
        Ok(())
    }

    pub fn require_active_connection(&self, connection_id: &str) -> Result<(), AgentError> {
        let connection = self.lock()?;
        let revoked: Option<Option<i64>> = connection
            .query_row(
                "SELECT revoked_at_ms FROM agent_connections WHERE connection_id=?1",
                [connection_id],
                |row| row.get(0),
            )
            .optional()?;
        match revoked {
            None => Err(AgentError::NotFound),
            Some(Some(_)) => Err(AgentError::Revoked),
            Some(None) => Ok(()),
        }
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
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = load_run_by_key(&tx, connection_id, idempotency_key)? {
            return Ok(existing);
        }
        if let Some(parent) = parent_run_id {
            let p = load_run(&tx, parent)?.ok_or(AgentError::NotFound)?;
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
        tx.execute("INSERT INTO agent_runs(run_id,connection_id,parent_run_id,correlation_id,idempotency_key,state,started_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![run.run_id,run.connection_id,run.parent_run_id,run.correlation_id,run.idempotency_key,run.state.wire(),run.started_at_ms])?;
        tx.commit()?;
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
        self.authenticate(connection_id, secret, AgentScope::RegisterRuns)?;
        if !state.terminal() {
            return Err(AgentError::InvalidTransition);
        }
        let connection = self.lock()?;
        let run = load_run(&connection, run_id)?.ok_or(AgentError::NotFound)?;
        if run.connection_id != connection_id {
            return Err(AgentError::AuthenticationFailed);
        }
        if run.state.terminal() {
            return Err(AgentError::TerminalRun);
        }
        connection.execute(
            "UPDATE agent_runs SET state=?1,completed_at_ms=?2 WHERE run_id=?3 AND state='running'",
            params![state.wire(), now_ms, run_id],
        )?;
        Ok(())
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
        let connection = self.lock()?;
        let run = load_run(&connection, &event.run_id)?.ok_or(AgentError::NotFound)?;
        if run.connection_id != connection_id {
            return Err(AgentError::AuthenticationFailed);
        }
        if run.state.terminal() {
            return Err(AgentError::TerminalRun);
        }
        let result=connection.execute("INSERT INTO agent_run_events(run_id,sequence,kind,name,payload_digest,summary,occurred_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![event.run_id,event.sequence,event.kind.wire(),event.name,event.payload_digest,event.summary,event.occurred_at_ms]);
        match result {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(AgentError::SequenceConflict)
            }
            Err(e) => Err(e.into()),
        }
    }

    fn authenticate(
        &self,
        connection_id: &str,
        secret: &str,
        scope: AgentScope,
    ) -> Result<(), AgentError> {
        let connection = self.lock()?;
        let row:Option<(String,String,Option<i64>)>=connection.query_row("SELECT secret_hash,scopes,revoked_at_ms FROM agent_connections WHERE connection_id=?1",[connection_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (stored, scopes, revoked) = row.ok_or(AgentError::AuthenticationFailed)?;
        if revoked.is_some() {
            return Err(AgentError::Revoked);
        }
        if !constant_time_eq(stored.as_bytes(), hash_secret(secret).as_bytes()) {
            return Err(AgentError::AuthenticationFailed);
        }
        if !scopes
            .split(',')
            .filter_map(AgentScope::parse)
            .any(|s| s == scope)
        {
            return Err(AgentError::ScopeDenied);
        }
        Ok(())
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AgentError> {
        self.connection
            .lock()
            .map_err(|_| AgentError::Conflict("agent database lock poisoned".into()))
    }
    fn connection_exists(&self, id: &str) -> Result<bool, AgentError> {
        Ok(self.lock()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM agent_connections WHERE connection_id=?1)",
            [id],
            |r| r.get(0),
        )?)
    }
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
fn load_run(c: &Connection, id: &str) -> Result<Option<AgentRun>, AgentError> {
    load_run_query(
        c,
        "SELECT run_id,connection_id,parent_run_id,correlation_id,idempotency_key,state,started_at_ms,completed_at_ms FROM agent_runs WHERE run_id=?1",
        id,
    )
}
fn load_run_by_key(
    c: &Connection,
    connection_id: &str,
    key: &str,
) -> Result<Option<AgentRun>, AgentError> {
    c.query_row("SELECT run_id,connection_id,parent_run_id,correlation_id,idempotency_key,state,started_at_ms,completed_at_ms FROM agent_runs WHERE connection_id=?1 AND idempotency_key=?2",params![connection_id,key],row_run).optional()?.map_or(Ok(None),|r|Ok(Some(r?)))
}
fn load_run_query(c: &Connection, q: &str, v: &str) -> Result<Option<AgentRun>, AgentError> {
    c.query_row(q, [v], row_run)
        .optional()?
        .map_or(Ok(None), |r| Ok(Some(r?)))
}
fn row_run(r: &rusqlite::Row<'_>) -> rusqlite::Result<Result<AgentRun, AgentError>> {
    let state: String = r.get(5)?;
    Ok(RunState::parse(&state)
        .ok_or(AgentError::Conflict("stored run state invalid".into()))
        .map(|state| AgentRun {
            run_id: r.get(0).unwrap(),
            connection_id: r.get(1).unwrap(),
            parent_run_id: r.get(2).unwrap(),
            correlation_id: r.get(3).unwrap(),
            idempotency_key: r.get(4).unwrap(),
            state,
            started_at_ms: r.get(6).unwrap(),
            completed_at_ms: r.get(7).unwrap(),
        }))
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
        assert!(matches!(
            r.finish_run(
                &c.connection_id,
                secret,
                &run.run_id,
                RunState::Failed,
                4300
            ),
            Err(AgentError::TerminalRun)
        ));
    }
    #[test]
    fn restart_preserves_revocation() {
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
                .windows(b"local_agents".len())
                .any(|window| window == b"local_agents"),
            "agent schema marker must not be plaintext on disk"
        );
    }
}
