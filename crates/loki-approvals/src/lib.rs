//! Local policy and immutable approvals (`APR-001`).
//!
//! Approval is authority to consume one exact request once; it is never a
//! general capability or operation result. This crate performs no operation.

use std::{fmt, path::Path, sync::Mutex};

use loki_local_authority::{AuthorityError, LocalAuthority, LocalCapability, LocalContext};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const MAX_FIELD: usize = 256;
const MAX_REASON: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    ApprovalRequired,
    Deny,
}

/// Closed local policy. Unknown strings never become an allowed enum variant.
pub fn evaluate_operation(operation: &str) -> PolicyDecision {
    match operation {
        "read_diagnostics" => PolicyDecision::Allow,
        "restart" | "flash" | "apply_config" | "contained_build" => {
            PolicyDecision::ApprovalRequired
        }
        "factory_reset" => PolicyDecision::Deny,
        _ => PolicyDecision::Deny,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub actor_id: String,
    pub operation: String,
    pub target_id: String,
    pub artifact_digest: Option<String>,
    pub parameters_digest: String,
    pub idempotency_key: String,
    pub reason: String,
}

impl ApprovalRequest {
    pub fn digest(&self, principal_sid: &str) -> Result<String, ApprovalError> {
        self.validate()?;
        validate("principal_sid", principal_sid, MAX_FIELD)?;
        let mut canonical = Vec::new();
        canonical.extend_from_slice(b"lokiai-approval-v1");
        for value in [
            principal_sid,
            &self.actor_id,
            &self.operation,
            &self.target_id,
            self.artifact_digest.as_deref().unwrap_or("<absent>"),
            &self.parameters_digest,
            &self.idempotency_key,
        ] {
            canonical.extend_from_slice(&(value.len() as u64).to_be_bytes());
            canonical.extend_from_slice(value.as_bytes());
        }
        Ok(format!("sha256:{:x}", Sha256::digest(canonical)))
    }

    fn validate(&self) -> Result<(), ApprovalError> {
        validate("actor_id", &self.actor_id, MAX_FIELD)?;
        validate("operation", &self.operation, MAX_FIELD)?;
        validate("target_id", &self.target_id, MAX_FIELD)?;
        if let Some(digest) = &self.artifact_digest {
            validate("artifact_digest", digest, MAX_FIELD)?;
        }
        validate("parameters_digest", &self.parameters_digest, MAX_FIELD)?;
        validate("idempotency_key", &self.idempotency_key, MAX_FIELD)?;
        validate("reason", &self.reason, MAX_REASON)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRecord {
    pub approval_id: String,
    pub request_digest: String,
    pub requester_sid: String,
    pub actor_id: String,
    pub operation: String,
    pub target_id: String,
    pub artifact_digest: Option<String>,
    pub parameters_digest: String,
    pub idempotency_key: String,
    pub reason: String,
    pub requested_at_ms: i64,
    pub expires_at_ms: i64,
    pub granted_at_ms: Option<i64>,
    pub granted_by_sid: Option<String>,
    pub consumed_at_ms: Option<i64>,
}

#[derive(Debug)]
pub enum ApprovalError {
    Authority(AuthorityError),
    Sqlite(rusqlite::Error),
    InvalidInput { field: &'static str, reason: String },
    PolicyDenied,
    ApprovalNotRequired,
    NotFound,
    NotGranted,
    Expired,
    AlreadyGranted,
    AlreadyConsumed,
    BindingMismatch,
    Conflict(String),
}

impl fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authority(error) => write!(f, "local authority denied approval action: {error}"),
            Self::Sqlite(error) => write!(f, "approval database error: {error}"),
            Self::InvalidInput { field, reason } => {
                write!(f, "invalid approval field '{field}': {reason}")
            }
            Self::PolicyDenied => write!(f, "local policy denied the operation"),
            Self::ApprovalNotRequired => write!(f, "operation does not require an approval"),
            Self::NotFound => write!(f, "approval was not found"),
            Self::NotGranted => write!(f, "approval has not been granted"),
            Self::Expired => write!(f, "approval has expired"),
            Self::AlreadyGranted => write!(f, "approval was already granted"),
            Self::AlreadyConsumed => write!(f, "approval was already consumed"),
            Self::BindingMismatch => write!(f, "approval does not match the exact request"),
            Self::Conflict(detail) => write!(f, "approval conflict: {detail}"),
        }
    }
}

impl std::error::Error for ApprovalError {}
impl From<AuthorityError> for ApprovalError {
    fn from(value: AuthorityError) -> Self {
        Self::Authority(value)
    }
}
impl From<rusqlite::Error> for ApprovalError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

pub struct ApprovalStore {
    connection: Mutex<Connection>,
}

impl ApprovalStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ApprovalError> {
        Self::from_connection(Connection::open(path)?)
    }
    pub fn open_in_memory() -> Result<Self, ApprovalError> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(connection: Connection) -> Result<Self, ApprovalError> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS local_approvals (
               approval_id TEXT PRIMARY KEY,
               request_digest TEXT NOT NULL UNIQUE,
               requester_sid TEXT NOT NULL,
               actor_id TEXT NOT NULL,
               operation TEXT NOT NULL,
               target_id TEXT NOT NULL,
               artifact_digest TEXT,
               parameters_digest TEXT NOT NULL,
               idempotency_key TEXT NOT NULL,
               reason TEXT NOT NULL,
               requested_at_ms INTEGER NOT NULL,
               expires_at_ms INTEGER NOT NULL,
               granted_at_ms INTEGER,
               granted_by_sid TEXT,
               consumed_at_ms INTEGER,
               CHECK (expires_at_ms > requested_at_ms)
             );",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn create(
        &self,
        authority: &LocalAuthority,
        context: &LocalContext,
        authority_session_id: &str,
        request: &ApprovalRequest,
        now_ms: i64,
        lifetime_ms: i64,
    ) -> Result<ApprovalRecord, ApprovalError> {
        authority.authorize_local(
            context,
            authority_session_id,
            LocalCapability::RequestConsequentialApproval,
            now_ms,
        )?;
        match evaluate_operation(&request.operation) {
            PolicyDecision::Deny => return Err(ApprovalError::PolicyDenied),
            PolicyDecision::Allow => return Err(ApprovalError::ApprovalNotRequired),
            PolicyDecision::ApprovalRequired => {}
        }
        if !(1_000..=3_600_000).contains(&lifetime_ms) || now_ms < 0 {
            return Err(ApprovalError::InvalidInput {
                field: "lifetime_ms",
                reason: "must be 1 second..=1 hour with non-negative now".to_string(),
            });
        }
        let expires_at_ms =
            now_ms
                .checked_add(lifetime_ms)
                .ok_or_else(|| ApprovalError::InvalidInput {
                    field: "lifetime_ms",
                    reason: "timestamp overflow".to_string(),
                })?;
        let digest = request.digest(context.principal_sid())?;
        let record = ApprovalRecord {
            approval_id: format!("apr_{}", Uuid::new_v4().simple()),
            request_digest: digest,
            requester_sid: context.principal_sid().to_string(),
            actor_id: request.actor_id.clone(),
            operation: request.operation.clone(),
            target_id: request.target_id.clone(),
            artifact_digest: request.artifact_digest.clone(),
            parameters_digest: request.parameters_digest.clone(),
            idempotency_key: request.idempotency_key.clone(),
            reason: request.reason.clone(),
            requested_at_ms: now_ms,
            expires_at_ms,
            granted_at_ms: None,
            granted_by_sid: None,
            consumed_at_ms: None,
        };
        let connection = self.lock()?;
        connection.execute(
            "INSERT INTO local_approvals
             (approval_id, request_digest, requester_sid, actor_id, operation,
              target_id, artifact_digest, parameters_digest, idempotency_key,
              reason, requested_at_ms, expires_at_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                record.approval_id,
                record.request_digest,
                record.requester_sid,
                record.actor_id,
                record.operation,
                record.target_id,
                record.artifact_digest,
                record.parameters_digest,
                record.idempotency_key,
                record.reason,
                record.requested_at_ms,
                record.expires_at_ms
            ],
        )?;
        Ok(record)
    }

    pub fn grant(
        &self,
        authority: &LocalAuthority,
        context: &LocalContext,
        authority_session_id: &str,
        approval_id: &str,
        now_ms: i64,
    ) -> Result<(), ApprovalError> {
        authority.authorize_local(
            context,
            authority_session_id,
            LocalCapability::RequestConsequentialApproval,
            now_ms,
        )?;
        let connection = self.lock()?;
        let record = load(&connection, approval_id)?.ok_or(ApprovalError::NotFound)?;
        if record.requester_sid != context.principal_sid() {
            return Err(ApprovalError::BindingMismatch);
        }
        if now_ms >= record.expires_at_ms {
            return Err(ApprovalError::Expired);
        }
        if record.granted_at_ms.is_some() {
            return Err(ApprovalError::AlreadyGranted);
        }
        let changed = connection.execute(
            "UPDATE local_approvals SET granted_at_ms=?1, granted_by_sid=?2
             WHERE approval_id=?3 AND granted_at_ms IS NULL",
            params![now_ms, context.principal_sid(), approval_id],
        )?;
        if changed != 1 {
            return Err(ApprovalError::AlreadyGranted);
        }
        Ok(())
    }

    pub fn consume(
        &self,
        authority: &LocalAuthority,
        context: &LocalContext,
        authority_session_id: &str,
        approval_id: &str,
        request: &ApprovalRequest,
        now_ms: i64,
    ) -> Result<(), ApprovalError> {
        authority.authorize_local(
            context,
            authority_session_id,
            LocalCapability::RequestConsequentialApproval,
            now_ms,
        )?;
        let expected = request.digest(context.principal_sid())?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = load(&transaction, approval_id)?.ok_or(ApprovalError::NotFound)?;
        if record.request_digest != expected || record.requester_sid != context.principal_sid() {
            return Err(ApprovalError::BindingMismatch);
        }
        if record.granted_at_ms.is_none() {
            return Err(ApprovalError::NotGranted);
        }
        if now_ms >= record.expires_at_ms {
            return Err(ApprovalError::Expired);
        }
        if record.consumed_at_ms.is_some() {
            return Err(ApprovalError::AlreadyConsumed);
        }
        let changed = transaction.execute(
            "UPDATE local_approvals SET consumed_at_ms=?1
             WHERE approval_id=?2 AND consumed_at_ms IS NULL",
            params![now_ms, approval_id],
        )?;
        if changed != 1 {
            return Err(ApprovalError::AlreadyConsumed);
        }
        transaction.commit()?;
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, ApprovalError> {
        self.connection
            .lock()
            .map_err(|_| ApprovalError::Conflict("approval database lock poisoned".to_string()))
    }
}

fn load(
    connection: &Connection,
    approval_id: &str,
) -> Result<Option<ApprovalRecord>, ApprovalError> {
    connection
        .query_row(
            "SELECT approval_id,request_digest,requester_sid,actor_id,operation,
                target_id,artifact_digest,parameters_digest,idempotency_key,reason,
                requested_at_ms,expires_at_ms,granted_at_ms,granted_by_sid,consumed_at_ms
         FROM local_approvals WHERE approval_id=?1",
            [approval_id],
            |row| {
                Ok(ApprovalRecord {
                    approval_id: row.get(0)?,
                    request_digest: row.get(1)?,
                    requester_sid: row.get(2)?,
                    actor_id: row.get(3)?,
                    operation: row.get(4)?,
                    target_id: row.get(5)?,
                    artifact_digest: row.get(6)?,
                    parameters_digest: row.get(7)?,
                    idempotency_key: row.get(8)?,
                    reason: row.get(9)?,
                    requested_at_ms: row.get(10)?,
                    expires_at_ms: row.get(11)?,
                    granted_at_ms: row.get(12)?,
                    granted_by_sid: row.get(13)?,
                    consumed_at_ms: row.get(14)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

fn validate(field: &'static str, value: &str, max: usize) -> Result<(), ApprovalError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(ApprovalError::InvalidInput {
            field,
            reason: format!("must contain 1..={max} non-control bytes"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn setup() -> (LocalAuthority, LocalContext, String, ApprovalStore) {
        let authority = LocalAuthority::open_in_memory().unwrap();
        let context = LocalContext::current("ipc-approval-test").unwrap();
        authority.initialize_current(&context, 1_000).unwrap();
        let session = authority.issue_session(&context, 2_000, 100_000).unwrap();
        (
            authority,
            context,
            session.session_id,
            ApprovalStore::open_in_memory().unwrap(),
        )
    }
    fn request() -> ApprovalRequest {
        ApprovalRequest {
            actor_id: "agent-1".into(),
            operation: "flash".into(),
            target_id: "binding-1".into(),
            artifact_digest: Some("sha256:artifact".into()),
            parameters_digest: "sha256:params".into(),
            idempotency_key: "idem-1".into(),
            reason: "install verified firmware".into(),
        }
    }

    #[test]
    fn policy_is_closed_and_unknown_defaults_deny() {
        assert_eq!(
            evaluate_operation("read_diagnostics"),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate_operation("flash"),
            PolicyDecision::ApprovalRequired
        );
        assert_eq!(evaluate_operation("factory_reset"), PolicyDecision::Deny);
        assert_eq!(evaluate_operation("invented"), PolicyDecision::Deny);
    }

    #[test]
    fn canonical_digest_distinguishes_absence_and_field_boundaries() {
        let context = LocalContext::current("ipc").unwrap();
        let base = request();
        let digest = base.digest(context.principal_sid()).unwrap();
        let mut changed = base.clone();
        changed.artifact_digest = None;
        assert_ne!(digest, changed.digest(context.principal_sid()).unwrap());
        changed = base.clone();
        changed.target_id.push('x');
        assert_ne!(digest, changed.digest(context.principal_sid()).unwrap());
    }

    #[test]
    fn grant_then_exact_request_consumes_once() {
        let (authority, context, session, store) = setup();
        let req = request();
        let record = store
            .create(&authority, &context, &session, &req, 3_000, 10_000)
            .unwrap();
        assert!(matches!(
            store.consume(
                &authority,
                &context,
                &session,
                &record.approval_id,
                &req,
                3_100
            ),
            Err(ApprovalError::NotGranted)
        ));
        store
            .grant(&authority, &context, &session, &record.approval_id, 3_200)
            .unwrap();
        store
            .consume(
                &authority,
                &context,
                &session,
                &record.approval_id,
                &req,
                3_300,
            )
            .unwrap();
        assert!(matches!(
            store.consume(
                &authority,
                &context,
                &session,
                &record.approval_id,
                &req,
                3_400
            ),
            Err(ApprovalError::AlreadyConsumed)
        ));
    }

    #[test]
    fn any_bound_request_mutation_is_rejected() {
        let (authority, context, session, store) = setup();
        let req = request();
        let record = store
            .create(&authority, &context, &session, &req, 3_000, 10_000)
            .unwrap();
        store
            .grant(&authority, &context, &session, &record.approval_id, 3_100)
            .unwrap();
        let mut changed = req.clone();
        changed.parameters_digest = "sha256:changed".into();
        assert!(matches!(
            store.consume(
                &authority,
                &context,
                &session,
                &record.approval_id,
                &changed,
                3_200
            ),
            Err(ApprovalError::BindingMismatch)
        ));
    }

    #[test]
    fn expired_and_policy_denied_requests_fail_closed() {
        let (authority, context, session, store) = setup();
        let req = request();
        let record = store
            .create(&authority, &context, &session, &req, 3_000, 1_000)
            .unwrap();
        assert!(matches!(
            store.grant(&authority, &context, &session, &record.approval_id, 4_000),
            Err(ApprovalError::Expired)
        ));
        let mut denied = req;
        denied.operation = "factory_reset".into();
        assert!(matches!(
            store.create(&authority, &context, &session, &denied, 4_000, 1_000),
            Err(ApprovalError::PolicyDenied)
        ));
    }

    #[test]
    fn concurrent_consumption_has_exactly_one_winner() {
        let (authority, context, session, store) = setup();
        let req = request();
        let record = store
            .create(&authority, &context, &session, &req, 3_000, 10_000)
            .unwrap();
        store
            .grant(&authority, &context, &session, &record.approval_id, 3_100)
            .unwrap();
        let authority = Arc::new(authority);
        let store = Arc::new(store);
        let barrier = Arc::new(Barrier::new(3));
        let attempts: Vec<_> = (0..2)
            .map(|offset| {
                let authority = Arc::clone(&authority);
                let store = Arc::clone(&store);
                let barrier = Arc::clone(&barrier);
                let context = context.clone();
                let session = session.clone();
                let approval_id = record.approval_id.clone();
                let req = req.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    store.consume(
                        &authority,
                        &context,
                        &session,
                        &approval_id,
                        &req,
                        3_200 + offset,
                    )
                })
            })
            .collect();
        barrier.wait();
        let results: Vec<_> = attempts.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|r| matches!(r, Err(ApprovalError::AlreadyConsumed)))
                .count(),
            1
        );
    }
}
