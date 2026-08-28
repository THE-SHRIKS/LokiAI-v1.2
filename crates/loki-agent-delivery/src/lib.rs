//! Authenticated local incident work-intent delivery (`AGT-002`).
//!
//! This coordinator is deliberately a local pull contract. It never wakes a
//! remote provider, executes a command, or turns a work intent into approval.
//!
//! # `P1-ACCEPTANCE-STORE-001` consolidation
//!
//! Previously wrapped its own independent `Mutex<Connection>`, opened via
//! `loki_storage::open_or_migrate_sqlcipher`/`Connection::open_in_memory()`,
//! running its own ad hoc `CREATE TABLE IF NOT EXISTS work_intent_deliveries`
//! schema, with each multi-statement operation wrapped in its own
//! `transaction_with_behavior(Immediate)` call directly against that
//! connection. Now wraps a shared `loki_storage::Storage`, with schema
//! owned by `loki-storage`'s own versioned migrations
//! (`migrations/0006_agent_delivery_and_local_authority.sql`), all SQL
//! moved into `loki_storage::agent_delivery`, and every multi-statement
//! operation using `loki_storage::Storage::transaction` (the same real
//! cross-repository transaction primitive
//! `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
//! and `crates/loki-acceptance-store` use) instead of a private
//! `transaction_with_behavior` call — this crate's own atomicity
//! requirements (check-then-conditionally-write) are exactly the
//! `Storage::transaction` use case, not only the acceptance-store crate's.

use std::{fmt, path::Path, sync::Arc};

use loki_agents::{AgentError, AgentRegistry, AgentScope};
use loki_incidents::work_intent::WorkIntent;
use loki_local_notifications::{LocalNotificationStore, NotificationError};
use loki_storage::agent_delivery::{
    AgentDeliveryRepository, NewWorkIntentDelivery, WorkIntentDeliveryRow, WorkIntentDeliveryUpdate,
};
use loki_storage::error::StorageError as LokiStorageError;
use loki_storage::{Cipher, Storage};
use std::time::SystemTime;

const MAX_FAILURE_REASON: usize = 512;

/// See `crates/loki-local-authority/src/store.rs`'s identical constant doc
/// comment — the same rationale applies here.
const BACK_COMPAT_MIGRATION_TIMESTAMP: &str = "1970-01-01T00:00:00Z";

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
    storage: Arc<Storage>,
}

impl WorkIntentDeliveryStore {
    /// Back-compat convenience constructor: builds a private, dedicated,
    /// fully migrated `Storage` at `path` (Windows DPAPI-managed SQLCipher),
    /// then delegates to [`WorkIntentDeliveryStore::new`].
    pub fn open(
        path: impl AsRef<Path>,
        protected_key_path: impl AsRef<Path>,
    ) -> Result<Self, DeliveryError> {
        let storage = Storage::open_managed(path.as_ref(), protected_key_path.as_ref())?;
        storage.run_migrations(BACK_COMPAT_MIGRATION_TIMESTAMP)?;
        Ok(Self::new(Arc::new(storage)))
    }

    /// Back-compat convenience constructor: builds a private, dedicated,
    /// fully migrated in-memory `Storage`, then delegates to
    /// [`WorkIntentDeliveryStore::new`].
    pub fn open_in_memory() -> Result<Self, DeliveryError> {
        let storage = Storage::open_in_memory(Cipher::None)?;
        storage.run_migrations(BACK_COMPAT_MIGRATION_TIMESTAMP)?;
        Ok(Self::new(Arc::new(storage)))
    }

    /// Constructs a `WorkIntentDeliveryStore` over an already-open,
    /// already-migrated, shared `Storage` — the `P1-ACCEPTANCE-STORE-001`
    /// composition path.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
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

        enum EnqueueOutcome {
            UseExisting(Box<WorkIntentDeliveryRow>),
            ConflictingExisting,
            Inserted,
        }

        let work_intent_id = intent.work_intent_id.as_str();
        let outcome = self.storage.transaction(|tx| {
            if let Some(existing) =
                loki_storage::agent_delivery::get_work_intent_delivery_core(tx, work_intent_id)?
            {
                let matches = existing.incident_id == intent.incident_id.as_str()
                    && existing.requested_capability == intent.requested_capability.as_str()
                    && existing.evidence_refs_json == refs_json
                    && existing.delivery_requirements == intent.delivery_requirements.description
                    && existing.created_at_ms == created_at_ms
                    && existing.expires_at_ms == expires_at_ms;
                if matches {
                    return Ok(EnqueueOutcome::UseExisting(Box::new(existing)));
                }
                return Ok(EnqueueOutcome::ConflictingExisting);
            }
            loki_storage::agent_delivery::insert_work_intent_delivery_core(
                tx,
                &NewWorkIntentDelivery {
                    work_intent_id: work_intent_id.to_string(),
                    incident_id: intent.incident_id.as_str().to_string(),
                    requested_capability: intent.requested_capability.as_str().to_string(),
                    evidence_refs_json: refs_json.clone(),
                    delivery_requirements: intent.delivery_requirements.description.clone(),
                    state_wire: state.wire().to_string(),
                    created_at_ms,
                    expires_at_ms,
                    terminal_at_ms: terminal,
                },
            )?;
            Ok(EnqueueOutcome::Inserted)
        })?;
        match outcome {
            EnqueueOutcome::ConflictingExisting => Err(DeliveryError::Conflict),
            EnqueueOutcome::UseExisting(row) => row_to_record(*row),
            EnqueueOutcome::Inserted => self.get(work_intent_id),
        }
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

        enum ClaimOutcome {
            NotFound,
            Expired,
            AlreadyClaimedByThisConnection(WorkIntentDeliveryRow),
            InvalidTransition,
            RevisionConflict,
            Claimed(WorkIntentDeliveryRow),
        }

        let outcome = self.storage.transaction(|tx| {
            let Some(current) =
                loki_storage::agent_delivery::get_work_intent_delivery_core(tx, work_intent_id)?
            else {
                return Ok(ClaimOutcome::NotFound);
            };
            let current_state = DeliveryState::parse(&current.state).map_err(|_| {
                LokiStorageError::InvalidState {
                    field: "state",
                    value: current.state.clone(),
                }
            })?;
            if now_ms >= current.expires_at_ms
                && matches!(
                    current_state,
                    DeliveryState::Queued | DeliveryState::Delivered
                )
            {
                loki_storage::agent_delivery::update_work_intent_delivery_core(
                    tx,
                    WorkIntentDeliveryUpdate::ExpireOne {
                        work_intent_id,
                        now_ms,
                    },
                )?;
                return Ok(ClaimOutcome::Expired);
            }
            if current_state == DeliveryState::Delivered
                && current.claimed_connection_id.as_deref() == Some(connection_id)
            {
                return Ok(ClaimOutcome::AlreadyClaimedByThisConnection(current));
            }
            if current_state != DeliveryState::Queued {
                return Ok(ClaimOutcome::InvalidTransition);
            }
            if u64::try_from(current.revision).unwrap_or(u64::MAX) != expected_revision {
                return Ok(ClaimOutcome::RevisionConflict);
            }
            let changed = loki_storage::agent_delivery::update_work_intent_delivery_core(
                tx,
                WorkIntentDeliveryUpdate::Claim {
                    work_intent_id,
                    connection_id,
                    now_ms,
                    expected_revision: expected_revision_sql,
                },
            )?;
            if changed != 1 {
                return Ok(ClaimOutcome::RevisionConflict);
            }
            let Some(updated) =
                loki_storage::agent_delivery::get_work_intent_delivery_core(tx, work_intent_id)?
            else {
                return Ok(ClaimOutcome::NotFound);
            };
            Ok(ClaimOutcome::Claimed(updated))
        })?;
        match outcome {
            ClaimOutcome::NotFound => Err(DeliveryError::NotFound),
            ClaimOutcome::Expired => Err(DeliveryError::Expired),
            ClaimOutcome::AlreadyClaimedByThisConnection(row) => row_to_record(row),
            ClaimOutcome::InvalidTransition => Err(DeliveryError::InvalidTransition),
            ClaimOutcome::RevisionConflict => Err(DeliveryError::Conflict),
            ClaimOutcome::Claimed(row) => row_to_record(row),
        }
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

        enum AcknowledgeOutcome {
            NotFound,
            AlreadyAcknowledgedByThisRun(WorkIntentDeliveryRow),
            Expired,
            InvalidTransition,
            RevisionConflict,
            Acknowledged(WorkIntentDeliveryRow),
        }

        let outcome = self.storage.transaction(|tx| {
            let Some(current) = loki_storage::agent_delivery::get_work_intent_delivery_core(
                tx,
                request.work_intent_id,
            )?
            else {
                return Ok(AcknowledgeOutcome::NotFound);
            };
            let current_state = DeliveryState::parse(&current.state).map_err(|_| {
                LokiStorageError::InvalidState {
                    field: "state",
                    value: current.state.clone(),
                }
            })?;
            if current_state == DeliveryState::Acknowledged
                && current.claimed_connection_id.as_deref() == Some(request.connection_id)
                && current.acknowledged_agent_run.as_deref() == Some(request.run_id)
            {
                return Ok(AcknowledgeOutcome::AlreadyAcknowledgedByThisRun(current));
            }
            if request.now_ms >= current.expires_at_ms && current_state == DeliveryState::Delivered
            {
                loki_storage::agent_delivery::update_work_intent_delivery_core(
                    tx,
                    WorkIntentDeliveryUpdate::ExpireOne {
                        work_intent_id: request.work_intent_id,
                        now_ms: request.now_ms,
                    },
                )?;
                return Ok(AcknowledgeOutcome::Expired);
            }
            if current_state != DeliveryState::Delivered
                || current.claimed_connection_id.as_deref() != Some(request.connection_id)
            {
                return Ok(AcknowledgeOutcome::InvalidTransition);
            }
            if u64::try_from(current.revision).unwrap_or(u64::MAX) != request.expected_revision {
                return Ok(AcknowledgeOutcome::RevisionConflict);
            }
            loki_storage::agent_delivery::update_work_intent_delivery_core(
                tx,
                WorkIntentDeliveryUpdate::Acknowledge {
                    work_intent_id: request.work_intent_id,
                    run_id: request.run_id,
                    now_ms: request.now_ms,
                    expected_revision: expected_revision_sql,
                },
            )?;
            let Some(updated) = loki_storage::agent_delivery::get_work_intent_delivery_core(
                tx,
                request.work_intent_id,
            )?
            else {
                return Ok(AcknowledgeOutcome::NotFound);
            };
            Ok(AcknowledgeOutcome::Acknowledged(updated))
        })?;
        match outcome {
            AcknowledgeOutcome::NotFound => Err(DeliveryError::NotFound),
            AcknowledgeOutcome::AlreadyAcknowledgedByThisRun(row) => row_to_record(row),
            AcknowledgeOutcome::Expired => Err(DeliveryError::Expired),
            AcknowledgeOutcome::InvalidTransition => Err(DeliveryError::InvalidTransition),
            AcknowledgeOutcome::RevisionConflict => Err(DeliveryError::Conflict),
            AcknowledgeOutcome::Acknowledged(row) => row_to_record(row),
        }
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

        enum RequireUserActionOutcome {
            NotFound,
            AlreadyUserActionRequired(WorkIntentDeliveryRow),
            Conflict,
            Updated(WorkIntentDeliveryRow),
        }

        let outcome = self.storage.transaction(|tx| {
            let changed = loki_storage::agent_delivery::update_work_intent_delivery_core(
                tx,
                WorkIntentDeliveryUpdate::RequireUserAction {
                    work_intent_id: id,
                    now_ms,
                    expected_revision: expected_revision_sql,
                },
            )?;
            if changed != 1 {
                let Some(current) =
                    loki_storage::agent_delivery::get_work_intent_delivery_core(tx, id)?
                else {
                    return Ok(RequireUserActionOutcome::NotFound);
                };
                if current.state == DeliveryState::UserActionRequired.wire() {
                    return Ok(RequireUserActionOutcome::AlreadyUserActionRequired(current));
                }
                return Ok(RequireUserActionOutcome::Conflict);
            }
            let Some(updated) =
                loki_storage::agent_delivery::get_work_intent_delivery_core(tx, id)?
            else {
                return Ok(RequireUserActionOutcome::NotFound);
            };
            Ok(RequireUserActionOutcome::Updated(updated))
        })?;
        match outcome {
            RequireUserActionOutcome::NotFound => Err(DeliveryError::NotFound),
            RequireUserActionOutcome::AlreadyUserActionRequired(row) => row_to_record(row),
            RequireUserActionOutcome::Conflict => Err(DeliveryError::Conflict),
            RequireUserActionOutcome::Updated(row) => row_to_record(row),
        }
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

        enum FailureOutcome {
            NotFound,
            InvalidTransition,
            Expired,
            RevisionConflict,
            Recorded(Box<WorkIntentDeliveryRow>),
        }

        let outcome = self.storage.transaction(|tx| {
            let Some(current) =
                loki_storage::agent_delivery::get_work_intent_delivery_core(tx, work_intent_id)?
            else {
                return Ok(FailureOutcome::NotFound);
            };
            if current.state != DeliveryState::Queued.wire() {
                return Ok(FailureOutcome::InvalidTransition);
            }
            if now_ms >= current.expires_at_ms {
                loki_storage::agent_delivery::update_work_intent_delivery_core(
                    tx,
                    WorkIntentDeliveryUpdate::ExpireOne {
                        work_intent_id,
                        now_ms,
                    },
                )?;
                return Ok(FailureOutcome::Expired);
            }
            if u64::try_from(current.revision).unwrap_or(u64::MAX) != expected_revision {
                return Ok(FailureOutcome::RevisionConflict);
            }
            loki_storage::agent_delivery::update_work_intent_delivery_core(
                tx,
                WorkIntentDeliveryUpdate::RecordPermanentFailure {
                    work_intent_id,
                    reason,
                    now_ms,
                    expected_revision: expected_revision_sql,
                },
            )?;
            let Some(updated) =
                loki_storage::agent_delivery::get_work_intent_delivery_core(tx, work_intent_id)?
            else {
                return Ok(FailureOutcome::NotFound);
            };
            Ok(FailureOutcome::Recorded(Box::new(updated)))
        })?;
        match outcome {
            FailureOutcome::NotFound => Err(DeliveryError::NotFound),
            FailureOutcome::InvalidTransition => Err(DeliveryError::InvalidTransition),
            FailureOutcome::Expired => Err(DeliveryError::Expired),
            FailureOutcome::RevisionConflict => Err(DeliveryError::Conflict),
            FailureOutcome::Recorded(row) => row_to_record(*row),
        }
    }

    pub fn expire_due(&self, now_ms: i64) -> Result<usize, DeliveryError> {
        validate_now(now_ms)?;
        Ok(self.storage.expire_due_work_intent_deliveries(now_ms)?)
    }

    pub fn get(&self, work_intent_id: &str) -> Result<DeliveryRecord, DeliveryError> {
        let row = self
            .storage
            .get_work_intent_delivery(work_intent_id)?
            .ok_or(DeliveryError::NotFound)?;
        row_to_record(row)
    }
}

fn row_to_record(row: WorkIntentDeliveryRow) -> Result<DeliveryRecord, DeliveryError> {
    let evidence_refs: Vec<String> = serde_json::from_str(&row.evidence_refs_json)
        .map_err(|_| DeliveryError::Corrupt("evidence decoding failed"))?;
    Ok(DeliveryRecord {
        work_intent_id: row.work_intent_id,
        incident_id: row.incident_id,
        requested_capability: row.requested_capability,
        evidence_refs,
        delivery_requirements: row.delivery_requirements,
        state: DeliveryState::parse(&row.state)?,
        delivery_attempts: u32::try_from(row.delivery_attempts)
            .map_err(|_| DeliveryError::Corrupt("invalid delivery attempts"))?,
        claimed_connection_id: row.claimed_connection_id,
        acknowledged_agent_run: row.acknowledged_agent_run,
        failure_reason: row.failure_reason,
        created_at_ms: row.created_at_ms,
        expires_at_ms: row.expires_at_ms,
        delivered_at_ms: row.delivered_at_ms,
        terminal_at_ms: row.terminal_at_ms,
        revision: u64::try_from(row.revision)
            .map_err(|_| DeliveryError::Corrupt("invalid revision"))?,
    })
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

    /// Every SQLite-touching test in this binary — in-memory or not — must
    /// bootstrap the disposable, non-production secure-memory arena first
    /// (DAT-002-MEM-CODE-001, ADR-0005), regardless of which test happens to
    /// run first: `sqlite3_config` can only run before SQLite's first
    /// implicit initialization, and this binary's
    /// `encrypted_restart_hides_schema_and_preserves_state` test opens a
    /// real SqlCipher-backed store through that arena via
    /// `loki_storage::open_or_migrate_sqlcipher`. Production never embeds an
    /// accepted bounds profile at this revision (fail-closed by design), so
    /// this is test-only — see `loki_storage::secure_memory::test_support`
    /// for why this bypass cannot reach a release build.
    fn ensure_ready() {
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
    }

    #[test]
    fn scoped_claim_and_owned_run_acknowledgement() {
        ensure_ready();
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
        ensure_ready();
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
        ensure_ready();
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
        ensure_ready();
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
        ensure_ready();
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
        ensure_ready();
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
        ensure_ready();
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
