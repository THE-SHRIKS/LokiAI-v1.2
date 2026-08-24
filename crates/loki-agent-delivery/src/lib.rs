//! Authenticated local incident work-intent delivery (`AGT-002`).
//!
//! This coordinator is deliberately a local pull contract. It never wakes a
//! remote provider, executes a command, or turns a work intent into approval.

use std::{fmt, path::Path, sync::Mutex, time::SystemTime};

use loki_agents::{AgentError, AgentRegistry, AgentScope};
use loki_incidents::work_intent::WorkIntent;
use loki_local_notifications::{LocalNotificationStore, NotificationError};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

const MAX_FAILURE_REASON: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryState {
    Queued,
    Delivered,
    Acknowledged,
    UserActionRequired,
    Failed,
    Expired,
}

impl DeliveryState {
    fn wire(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Delivered => "delivered",
            Self::Acknowledged => "acknowledged",
            Self::UserActionRequired => "user_action_required",
            Self::Failed => "failed",
            Self::Expired => "expired",
        }
    }

    fn parse(value: &str) -> Result<Self, DeliveryError> {
        match value {
            "queued" => Ok(Self::Queued),
            "delivered" => Ok(Self::Delivered),
            "acknowledged" => Ok(Self::Acknowledged),
            "user_action_required" => Ok(Self::UserActionRequired),
            "failed" => Ok(Self::Failed),
            "expired" => Ok(Self::Expired),
            _ => Err(DeliveryError::Corrupt("unknown delivery state")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryRecord {
    pub work_intent_id: String,
    pub incident_id: String,
    pub requested_capability: String,
    pub evidence_refs: Vec<String>,
    pub delivery_requirements: String,
    pub state: DeliveryState,
    pub delivery_attempts: u32,
    pub claimed_connection_id: Option<String>,
    pub acknowledged_agent_run: Option<String>,
    pub failure_reason: Option<String>,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub delivered_at_ms: Option<i64>,
    pub terminal_at_ms: Option<i64>,
    pub revision: u64,
}

pub struct AcknowledgeRequest<'a> {
    pub connection_id: &'a str,
    pub secret: &'a str,
    pub work_intent_id: &'a str,
    pub run_id: &'a str,
    pub expected_revision: u64,
    pub now_ms: i64,
}

#[derive(Debug)]
pub enum DeliveryError {
    Agent(AgentError),
    Notification(NotificationError),
    Sqlite(rusqlite::Error),
    Storage(loki_storage::StorageError),
    InvalidInput { field: &'static str, reason: String },
    NotFound,
    Conflict,
    InvalidTransition,
    Expired,
    Corrupt(&'static str),
    Time,
    Concurrency,
}

impl fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Agent(_) => "agent authorization denied work-intent delivery",
            Self::Notification(_) => "human fallback notification failed",
            Self::Sqlite(_) => "work-intent delivery database error",
            Self::Storage(_) => "encrypted work-intent database open failed",
            Self::InvalidInput { .. } => "invalid work-intent delivery input",
            Self::NotFound => "work intent not found",
            Self::Conflict => "work-intent revision conflict",
            Self::InvalidTransition => "invalid work-intent transition",
            Self::Expired => "work intent expired",
            Self::Corrupt(_) => "corrupt work-intent delivery record",
            Self::Time => "work-intent time is not representable",
            Self::Concurrency => "work-intent delivery lock failed",
        };
        f.write_str(message)
    }
}

impl std::error::Error for DeliveryError {}
impl From<AgentError> for DeliveryError {
    fn from(v: AgentError) -> Self {
        Self::Agent(v)
    }
}
impl From<NotificationError> for DeliveryError {
    fn from(v: NotificationError) -> Self {
        Self::Notification(v)
    }
}
impl From<rusqlite::Error> for DeliveryError {
    fn from(v: rusqlite::Error) -> Self {
        Self::Sqlite(v)
    }
}
impl From<loki_storage::StorageError> for DeliveryError {
    fn from(v: loki_storage::StorageError) -> Self {
        Self::Storage(v)
    }
}

pub struct WorkIntentDeliveryStore {
    connection: Mutex<Connection>,
}

impl WorkIntentDeliveryStore {
    pub fn open(
        path: impl AsRef<Path>,
        protected_key_path: impl AsRef<Path>,
    ) -> Result<Self, DeliveryError> {
        let (connection, _key) =
            loki_storage::open_or_migrate_sqlcipher(path.as_ref(), protected_key_path.as_ref())?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self, DeliveryError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, DeliveryError> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS work_intent_deliveries(
               work_intent_id TEXT PRIMARY KEY,
               incident_id TEXT NOT NULL,
               requested_capability TEXT NOT NULL,
               evidence_refs_json TEXT NOT NULL,
               delivery_requirements TEXT NOT NULL,
               state TEXT NOT NULL,
               delivery_attempts INTEGER NOT NULL CHECK(delivery_attempts>=0),
               claimed_connection_id TEXT,
               acknowledged_agent_run TEXT,
               failure_reason TEXT,
               created_at_ms INTEGER NOT NULL,
               expires_at_ms INTEGER NOT NULL,
               delivered_at_ms INTEGER,
               terminal_at_ms INTEGER,
               revision INTEGER NOT NULL CHECK(revision>0));",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn enqueue(
        &self,
        intent: &WorkIntent,
        now_ms: i64,
    ) -> Result<DeliveryRecord, DeliveryError> {
        validate_now(now_ms)?;
        if intent.delivery_attempts != 0
            || intent.acknowledged_agent_run.is_some()
            || intent.terminal_outcome.is_some()
        {
            return Err(DeliveryError::InvalidInput {
                field: "work_intent",
                reason: "only pristine emitted intents may be queued".into(),
            });
        }
        let created_at_ms = system_time_ms(intent.created_at)?;
        let expires_at_ms = system_time_ms(intent.expiry)?;
        if expires_at_ms <= created_at_ms {
            return Err(DeliveryError::InvalidInput {
                field: "work_intent.expiry",
                reason: "must be after creation".into(),
            });
        }
        let refs = intent
            .evidence_refs
            .iter()
            .map(|v| v.as_str())
            .collect::<Vec<_>>();
        let refs_json = serde_json::to_string(&refs)
            .map_err(|_| DeliveryError::Corrupt("evidence encoding failed"))?;
        let state = if now_ms >= expires_at_ms {
            DeliveryState::Expired
        } else {
            DeliveryState::Queued
        };
        let terminal = (state == DeliveryState::Expired).then_some(now_ms);
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = load(&tx, intent.work_intent_id.as_str())? {
            if existing.incident_id != intent.incident_id.as_str()
                || existing.requested_capability != intent.requested_capability.as_str()
                || existing.evidence_refs != refs
                || existing.delivery_requirements != intent.delivery_requirements.description
                || existing.created_at_ms != created_at_ms
                || existing.expires_at_ms != expires_at_ms
            {
                return Err(DeliveryError::Conflict);
            }
            tx.commit()?;
            return Ok(existing);
        }
        tx.execute(
            "INSERT INTO work_intent_deliveries(work_intent_id,incident_id,requested_capability,evidence_refs_json,delivery_requirements,state,delivery_attempts,created_at_ms,expires_at_ms,terminal_at_ms,revision) VALUES(?1,?2,?3,?4,?5,?6,0,?7,?8,?9,1)",
            params![intent.work_intent_id.as_str(), intent.incident_id.as_str(), intent.requested_capability.as_str(), refs_json, intent.delivery_requirements.description, state.wire(), created_at_ms, expires_at_ms, terminal]
        )?;
        tx.commit()?;
        drop(connection);
        self.get(intent.work_intent_id.as_str())
    }

    pub fn claim(
        &self,
        agents: &AgentRegistry,
        connection_id: &str,
        secret: &str,
        work_intent_id: &str,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<DeliveryRecord, DeliveryError> {
        validate_now(now_ms)?;
        let expected_revision_sql = revision_sql(expected_revision)?;
        agents.authorize_scope(connection_id, secret, AgentScope::ReceiveWorkIntents)?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = load(&tx, work_intent_id)?.ok_or(DeliveryError::NotFound)?;
        if now_ms >= current.expires_at_ms
            && matches!(
                current.state,
                DeliveryState::Queued | DeliveryState::Delivered
            )
        {
            expire_one(&tx, work_intent_id, now_ms)?;
            tx.commit()?;
            return Err(DeliveryError::Expired);
        }
        if current.state == DeliveryState::Delivered
            && current.claimed_connection_id.as_deref() == Some(connection_id)
        {
            tx.commit()?;
            return Ok(current);
        }
        if current.state != DeliveryState::Queued {
            return Err(DeliveryError::InvalidTransition);
        }
        if current.revision != expected_revision {
            return Err(DeliveryError::Conflict);
        }
        let changed = tx.execute(
            "UPDATE work_intent_deliveries SET state='delivered',delivery_attempts=delivery_attempts+1,claimed_connection_id=?1,delivered_at_ms=?2,revision=revision+1 WHERE work_intent_id=?3 AND state='queued' AND revision=?4",
            params![connection_id, now_ms, work_intent_id, expected_revision_sql]
        )?;
        if changed != 1 {
            return Err(DeliveryError::Conflict);
        }
        let record = load(&tx, work_intent_id)?.ok_or(DeliveryError::NotFound)?;
        tx.commit()?;
        Ok(record)
    }

    pub fn acknowledge(
        &self,
        agents: &AgentRegistry,
        request: AcknowledgeRequest<'_>,
    ) -> Result<DeliveryRecord, DeliveryError> {
        validate_now(request.now_ms)?;
        let expected_revision_sql = revision_sql(request.expected_revision)?;
        agents.authorize_scope(
            request.connection_id,
            request.secret,
            AgentScope::ReceiveWorkIntents,
        )?;
        agents.require_run_for_connection(request.connection_id, request.run_id)?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = load(&tx, request.work_intent_id)?.ok_or(DeliveryError::NotFound)?;
        if current.state == DeliveryState::Acknowledged
            && current.claimed_connection_id.as_deref() == Some(request.connection_id)
            && current.acknowledged_agent_run.as_deref() == Some(request.run_id)
        {
            tx.commit()?;
            return Ok(current);
        }
        if request.now_ms >= current.expires_at_ms && current.state == DeliveryState::Delivered {
            expire_one(&tx, request.work_intent_id, request.now_ms)?;
            tx.commit()?;
            return Err(DeliveryError::Expired);
        }
        if current.state != DeliveryState::Delivered
            || current.claimed_connection_id.as_deref() != Some(request.connection_id)
        {
            return Err(DeliveryError::InvalidTransition);
        }
        if current.revision != request.expected_revision {
            return Err(DeliveryError::Conflict);
        }
        tx.execute("UPDATE work_intent_deliveries SET state='acknowledged',acknowledged_agent_run=?1,terminal_at_ms=?2,revision=revision+1 WHERE work_intent_id=?3 AND revision=?4", params![request.run_id, request.now_ms, request.work_intent_id, expected_revision_sql])?;
        let record = load(&tx, request.work_intent_id)?.ok_or(DeliveryError::NotFound)?;
        tx.commit()?;
        Ok(record)
    }

    /// Records the absence of a safe local agent route. Notification enqueue
    /// happens first and is deduplicated, so a crash cannot create a false
    /// `user_action_required` state without a corresponding human signal.
    pub fn require_user_action(
        &self,
        intent: &WorkIntent,
        notifications: &LocalNotificationStore,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<DeliveryRecord, DeliveryError> {
        let expected_revision_sql = revision_sql(expected_revision)?;
        let id = intent.work_intent_id.as_str();
        let before = self.get(id)?;
        if before.state == DeliveryState::UserActionRequired {
            return Ok(before);
        }
        if before.state != DeliveryState::Queued {
            return Err(DeliveryError::InvalidTransition);
        }
        if before.revision != expected_revision {
            return Err(DeliveryError::Conflict);
        }
        if now_ms >= before.expires_at_ms {
            self.expire_due(now_ms)?;
            return Err(DeliveryError::Expired);
        }
        if !record_matches_intent(&before, intent)? {
            return Err(DeliveryError::Conflict);
        }
        notifications.enqueue_work_intent(intent, now_ms)?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute("UPDATE work_intent_deliveries SET state='user_action_required',terminal_at_ms=?1,revision=revision+1 WHERE work_intent_id=?2 AND state='queued' AND revision=?3", params![now_ms, id, expected_revision_sql])?;
        if changed != 1 {
            let current = load(&tx, id)?.ok_or(DeliveryError::NotFound)?;
            if current.state == DeliveryState::UserActionRequired {
                tx.commit()?;
                return Ok(current);
            }
            return Err(DeliveryError::Conflict);
        }
        let record = load(&tx, id)?.ok_or(DeliveryError::NotFound)?;
        tx.commit()?;
        Ok(record)
    }

    pub fn record_permanent_failure(
        &self,
        work_intent_id: &str,
        reason: &str,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<DeliveryRecord, DeliveryError> {
        validate_now(now_ms)?;
        let expected_revision_sql = revision_sql(expected_revision)?;
        validate_text("reason", reason, MAX_FAILURE_REASON)?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = load(&tx, work_intent_id)?.ok_or(DeliveryError::NotFound)?;
        if current.state != DeliveryState::Queued {
            return Err(DeliveryError::InvalidTransition);
        }
        if now_ms >= current.expires_at_ms {
            expire_one(&tx, work_intent_id, now_ms)?;
            tx.commit()?;
            return Err(DeliveryError::Expired);
        }
        if current.revision != expected_revision {
            return Err(DeliveryError::Conflict);
        }
        tx.execute("UPDATE work_intent_deliveries SET state='failed',failure_reason=?1,terminal_at_ms=?2,revision=revision+1 WHERE work_intent_id=?3 AND revision=?4", params![reason, now_ms, work_intent_id, expected_revision_sql])?;
        let record = load(&tx, work_intent_id)?.ok_or(DeliveryError::NotFound)?;
        tx.commit()?;
        Ok(record)
    }

    pub fn expire_due(&self, now_ms: i64) -> Result<usize, DeliveryError> {
        validate_now(now_ms)?;
        Ok(self.lock()?.execute("UPDATE work_intent_deliveries SET state='expired',terminal_at_ms=?1,revision=revision+1 WHERE expires_at_ms<=?1 AND state IN ('queued','delivered')", [now_ms])?)
    }

    pub fn get(&self, work_intent_id: &str) -> Result<DeliveryRecord, DeliveryError> {
        let connection = self.lock()?;
        load(&connection, work_intent_id)?.ok_or(DeliveryError::NotFound)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, DeliveryError> {
        self.connection
            .lock()
            .map_err(|_| DeliveryError::Concurrency)
    }
}

fn expire_one(connection: &Connection, id: &str, now_ms: i64) -> Result<(), DeliveryError> {
    connection.execute("UPDATE work_intent_deliveries SET state='expired',terminal_at_ms=?1,revision=revision+1 WHERE work_intent_id=?2 AND state IN ('queued','delivered')", params![now_ms, id])?;
    Ok(())
}

fn load(connection: &Connection, id: &str) -> Result<Option<DeliveryRecord>, DeliveryError> {
    connection.query_row(
        "SELECT work_intent_id,incident_id,requested_capability,evidence_refs_json,delivery_requirements,state,delivery_attempts,claimed_connection_id,acknowledged_agent_run,failure_reason,created_at_ms,expires_at_ms,delivered_at_ms,terminal_at_ms,revision FROM work_intent_deliveries WHERE work_intent_id=?1",
        [id],
        |row| {
            let refs_json: String = row.get(3)?;
            let refs: Vec<String> = serde_json::from_str(&refs_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
            let state: String = row.get(5)?;
            let attempts: i64 = row.get(6)?;
            let revision: i64 = row.get(14)?;
            Ok((state, attempts, revision, refs, row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(4)?, row.get::<_, Option<String>>(7)?, row.get::<_, Option<String>>(8)?, row.get::<_, Option<String>>(9)?, row.get::<_, i64>(10)?, row.get::<_, i64>(11)?, row.get::<_, Option<i64>>(12)?, row.get::<_, Option<i64>>(13)?))
        }
    ).optional()?.map(|(state, attempts, revision, evidence_refs, work_intent_id, incident_id, requested_capability, delivery_requirements, claimed_connection_id, acknowledged_agent_run, failure_reason, created_at_ms, expires_at_ms, delivered_at_ms, terminal_at_ms)| {
        Ok(DeliveryRecord {
            work_intent_id, incident_id, requested_capability, evidence_refs, delivery_requirements,
            state: DeliveryState::parse(&state)?,
            delivery_attempts: u32::try_from(attempts).map_err(|_| DeliveryError::Corrupt("invalid delivery attempts"))?,
            claimed_connection_id, acknowledged_agent_run, failure_reason, created_at_ms, expires_at_ms, delivered_at_ms, terminal_at_ms,
            revision: u64::try_from(revision).map_err(|_| DeliveryError::Corrupt("invalid revision"))?,
        })
    }).transpose()
}

fn validate_now(value: i64) -> Result<(), DeliveryError> {
    if value < 0 {
        Err(DeliveryError::InvalidInput {
            field: "now_ms",
            reason: "must be non-negative".into(),
        })
    } else {
        Ok(())
    }
}
fn revision_sql(value: u64) -> Result<i64, DeliveryError> {
    i64::try_from(value).map_err(|_| DeliveryError::InvalidInput {
        field: "expected_revision",
        reason: "exceeds durable revision range".into(),
    })
}
fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), DeliveryError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        Err(DeliveryError::InvalidInput {
            field,
            reason: format!("must contain 1..={max} non-control bytes"),
        })
    } else {
        Ok(())
    }
}
fn system_time_ms(value: SystemTime) -> Result<i64, DeliveryError> {
    let millis = value
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| DeliveryError::Time)?
        .as_millis();
    i64::try_from(millis).map_err(|_| DeliveryError::Time)
}

fn record_matches_intent(
    record: &DeliveryRecord,
    intent: &WorkIntent,
) -> Result<bool, DeliveryError> {
    Ok(record.work_intent_id == intent.work_intent_id.as_str()
        && record.incident_id == intent.incident_id.as_str()
        && record.requested_capability == intent.requested_capability.as_str()
        && record.evidence_refs
            == intent
                .evidence_refs
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>()
        && record.delivery_requirements == intent.delivery_requirements.description
        && record.created_at_ms == system_time_ms(intent.created_at)?
        && record.expires_at_ms == system_time_ms(intent.expiry)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_agents::{AgentCredential, RunState};
    use loki_connectors_sdk::ids::CapabilityId;
    use loki_incidents::{ids::IncidentId, work_intent::DeliveryRequirements};
    use loki_local_authority::{LocalAuthority, LocalContext};
    use std::{sync::Arc, thread, time::Duration};

    fn epoch(ms: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_millis(ms)
    }
    fn intent() -> WorkIntent {
        WorkIntent::emit(
            IncidentId::generate(),
            CapabilityId::new("read_health").unwrap(),
            &[],
            DeliveryRequirements::new("investigate locally").unwrap(),
            epoch(20_000),
            epoch(1_000),
        )
    }
    fn agent(
        scopes: &[AgentScope],
    ) -> (
        LocalAuthority,
        LocalContext,
        String,
        AgentRegistry,
        AgentCredential,
    ) {
        let authority = LocalAuthority::open_in_memory().unwrap();
        let context = LocalContext::current("agt-002-tests").unwrap();
        authority.initialize_current(&context, 100).unwrap();
        let session = authority.issue_session(&context, 200, 100_000).unwrap();
        let registry = AgentRegistry::open_in_memory().unwrap();
        let credential = registry
            .register(
                &authority,
                &context,
                &session.session_id,
                "local client",
                scopes,
                300,
            )
            .unwrap();
        (authority, context, session.session_id, registry, credential)
    }

    #[test]
    fn scoped_claim_and_owned_run_acknowledgement() {
        let store = WorkIntentDeliveryStore::open_in_memory().unwrap();
        let wi = intent();
        let queued = store.enqueue(&wi, 1_100).unwrap();
        let (_, _, _, agents, credential) =
            agent(&[AgentScope::ReceiveWorkIntents, AgentScope::RegisterRuns]);
        let secret = credential.secret.expose_for_client_configuration();
        let delivered = store
            .claim(
                &agents,
                &credential.connection_id,
                secret,
                &queued.work_intent_id,
                queued.revision,
                2_000,
            )
            .unwrap();
        assert_eq!(delivered.state, DeliveryState::Delivered);
        assert_eq!(delivered.delivery_attempts, 1);
        assert_eq!(
            store
                .claim(
                    &agents,
                    &credential.connection_id,
                    secret,
                    &queued.work_intent_id,
                    queued.revision,
                    2_100
                )
                .unwrap()
                .delivery_attempts,
            1
        );
        let run = agents
            .start_run(&credential.connection_id, secret, "intent-run", None, 2_200)
            .unwrap();
        let ack = store
            .acknowledge(
                &agents,
                AcknowledgeRequest {
                    connection_id: &credential.connection_id,
                    secret,
                    work_intent_id: &queued.work_intent_id,
                    run_id: &run.run_id,
                    expected_revision: delivered.revision,
                    now_ms: 2_300,
                },
            )
            .unwrap();
        assert_eq!(ack.state, DeliveryState::Acknowledged);
        assert_eq!(
            ack.acknowledged_agent_run.as_deref(),
            Some(run.run_id.as_str())
        );
        agents
            .finish_run(
                &credential.connection_id,
                secret,
                &run.run_id,
                RunState::Succeeded,
                2_400,
            )
            .unwrap();
    }

    #[test]
    fn wrong_secret_missing_scope_and_revocation_fail_closed() {
        let store = WorkIntentDeliveryStore::open_in_memory().unwrap();
        let wi = intent();
        let q = store.enqueue(&wi, 1_100).unwrap();
        let (authority, context, session, agents, credential) =
            agent(&[AgentScope::ReceiveWorkIntents]);
        assert!(matches!(
            store.claim(
                &agents,
                &credential.connection_id,
                "wrong",
                &q.work_intent_id,
                q.revision,
                2_000
            ),
            Err(DeliveryError::Agent(AgentError::AuthenticationFailed))
        ));
        let (_, _, _, no_scope, other) = agent(&[AgentScope::ReadDiagnostics]);
        assert!(matches!(
            store.claim(
                &no_scope,
                &other.connection_id,
                other.secret.expose_for_client_configuration(),
                &q.work_intent_id,
                q.revision,
                2_000
            ),
            Err(DeliveryError::Agent(AgentError::ScopeDenied))
        ));
        agents
            .revoke(
                &authority,
                &context,
                &session,
                &credential.connection_id,
                2_100,
            )
            .unwrap();
        assert!(matches!(
            store.claim(
                &agents,
                &credential.connection_id,
                credential.secret.expose_for_client_configuration(),
                &q.work_intent_id,
                q.revision,
                2_200
            ),
            Err(DeliveryError::Agent(AgentError::Revoked))
        ));
        assert_eq!(
            store.get(&q.work_intent_id).unwrap().state,
            DeliveryState::Queued
        );
    }

    #[test]
    fn cross_connection_run_cannot_acknowledge() {
        let store = WorkIntentDeliveryStore::open_in_memory().unwrap();
        let wi = intent();
        let q = store.enqueue(&wi, 1_100).unwrap();
        let (authority, context, session, agents, owner) =
            agent(&[AgentScope::ReceiveWorkIntents, AgentScope::RegisterRuns]);
        let secret = owner.secret.expose_for_client_configuration();
        let delivered = store
            .claim(
                &agents,
                &owner.connection_id,
                secret,
                &q.work_intent_id,
                q.revision,
                2_000,
            )
            .unwrap();
        let other = agents
            .register(
                &authority,
                &context,
                &session,
                "other client",
                &[AgentScope::ReceiveWorkIntents, AgentScope::RegisterRuns],
                2_050,
            )
            .unwrap();
        let other_run = agents
            .start_run(
                &other.connection_id,
                other.secret.expose_for_client_configuration(),
                "other",
                None,
                2_100,
            )
            .unwrap();
        assert!(matches!(
            store.acknowledge(
                &agents,
                AcknowledgeRequest {
                    connection_id: &other.connection_id,
                    secret: other.secret.expose_for_client_configuration(),
                    work_intent_id: &q.work_intent_id,
                    run_id: &other_run.run_id,
                    expected_revision: delivered.revision,
                    now_ms: 2_200,
                }
            ),
            Err(DeliveryError::InvalidTransition)
        ));
    }

    #[test]
    fn unavailable_client_falls_back_once_to_human() {
        let store = WorkIntentDeliveryStore::open_in_memory().unwrap();
        let notifications = LocalNotificationStore::open_in_memory().unwrap();
        let wi = intent();
        let q = store.enqueue(&wi, 1_100).unwrap();
        let fallback = store
            .require_user_action(&wi, &notifications, q.revision, 2_000)
            .unwrap();
        assert_eq!(fallback.state, DeliveryState::UserActionRequired);
        assert_eq!(
            store
                .require_user_action(&wi, &notifications, q.revision, 2_100)
                .unwrap(),
            fallback
        );
        assert_eq!(notifications.list().unwrap().len(), 1);
    }

    #[test]
    fn expiry_and_failure_are_terminal() {
        let store = WorkIntentDeliveryStore::open_in_memory().unwrap();
        let wi = intent();
        let q = store.enqueue(&wi, 1_100).unwrap();
        assert_eq!(store.expire_due(20_000).unwrap(), 1);
        assert_eq!(
            store.get(&q.work_intent_id).unwrap().state,
            DeliveryState::Expired
        );
        let wi2 = intent();
        let q2 = store.enqueue(&wi2, 1_100).unwrap();
        let failed = store
            .record_permanent_failure(
                &q2.work_intent_id,
                "unsupported local adapter contract",
                q2.revision,
                2_000,
            )
            .unwrap();
        assert_eq!(failed.state, DeliveryState::Failed);
        assert!(matches!(
            store.record_permanent_failure(&q2.work_intent_id, "again", failed.revision, 2_100),
            Err(DeliveryError::InvalidTransition)
        ));
    }

    #[test]
    fn concurrent_claim_has_one_winner_and_one_attempt() {
        let store = Arc::new(WorkIntentDeliveryStore::open_in_memory().unwrap());
        let wi = intent();
        let q = store.enqueue(&wi, 1_100).unwrap();
        let agents = Arc::new(agent(&[AgentScope::ReceiveWorkIntents]).3);
        let credentials = (0..2)
            .map(|i| {
                let authority = LocalAuthority::open_in_memory().unwrap();
                let context = LocalContext::current(format!("claim-{i}")).unwrap();
                authority.initialize_current(&context, 100).unwrap();
                let session = authority.issue_session(&context, 200, 10_000).unwrap();
                agents
                    .register(
                        &authority,
                        &context,
                        &session.session_id,
                        &format!("client-{i}"),
                        &[AgentScope::ReceiveWorkIntents],
                        300,
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let handles = credentials
            .into_iter()
            .map(|credential| {
                let store = Arc::clone(&store);
                let agents = Arc::clone(&agents);
                let id = q.work_intent_id.clone();
                thread::spawn(move || {
                    store.claim(
                        &agents,
                        &credential.connection_id,
                        credential.secret.expose_for_client_configuration(),
                        &id,
                        q.revision,
                        2_000,
                    )
                })
            })
            .collect::<Vec<_>>();
        let successes = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(Result::is_ok)
            .count();
        assert_eq!(successes, 1);
        assert_eq!(store.get(&q.work_intent_id).unwrap().delivery_attempts, 1);
    }

    #[test]
    fn encrypted_restart_hides_schema_and_preserves_state() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("delivery.db");
        let key = temp.path().join("delivery.key");
        let wi = intent();
        let id;
        {
            let store = WorkIntentDeliveryStore::open(&db, &key).unwrap();
            let q = store.enqueue(&wi, 1_100).unwrap();
            id = q.work_intent_id;
            store
                .record_permanent_failure(&id, "permanent local policy denial", q.revision, 2_000)
                .unwrap();
        }
        let raw = std::fs::read(&db).unwrap();
        assert!(
            !raw.windows(b"work_intent_deliveries".len())
                .any(|w| w == b"work_intent_deliveries")
        );
        let reopened = WorkIntentDeliveryStore::open(&db, &key).unwrap();
        assert_eq!(reopened.get(&id).unwrap().state, DeliveryState::Failed);

        let other_db = temp.path().join("other.db");
        let other_key = temp.path().join("other.key");
        drop(WorkIntentDeliveryStore::open(&other_db, &other_key).unwrap());
        assert!(WorkIntentDeliveryStore::open(&db, &other_key).is_err());
    }
}
