//! Local policy and immutable approvals (`APR-001`).
//!
//! Approval is authority to consume one exact request once; it is never a
//! general capability or operation result. This crate performs no operation.
//!
//! # `P1-ACCEPTANCE-STORE-001` consolidation
//!
//! Previously wrapped its own independent `Mutex<Connection>`, opened via
//! `loki_storage::open_or_migrate_sqlcipher`/`Connection::open_in_memory()`,
//! running its own ad hoc `CREATE TABLE IF NOT EXISTS local_approvals`
//! schema. Now wraps a shared `loki_storage::Storage`, with schema owned by
//! `loki-storage`'s own versioned migrations (`migrations/0004_approvals.sql`)
//! and all SQL moved into `loki_storage::approvals`. This is what makes a
//! real cross-repository `Storage::transaction()`
//! (`Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`,
//! `crates/loki-acceptance-store`) possible: that transaction consumes an
//! approval via `loki_storage::approvals::consume_approval_core` in the
//! same transaction as its job/outbox/mutation-acceptance writes — no
//! second commit.

use std::{fmt, path::Path, sync::Arc};

use loki_local_authority::{AuthorityError, LocalCapability, LocalContext, evaluate_local_session};
use loki_mutation_contract::{IdempotencyKey, MutationMethod, OperationBinding};
use loki_storage::approvals::{self, NewApproval};
use loki_storage::local_authority;
use loki_storage::{Cipher, Storage};
use uuid::Uuid;

const MAX_FIELD: usize = 256;
const MAX_REASON: usize = 2_000;

/// See `crates/loki-local-authority/src/store.rs`'s identical constant doc
/// comment — the same rationale applies here.
const BACK_COMPAT_MIGRATION_TIMESTAMP: &str = "1970-01-01T00:00:00Z";

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
    fn digest(&self, principal_sid: &str, grant_generation: u64) -> Result<String, ApprovalError> {
        self.validate()?;
        validate("principal_sid", principal_sid, MAX_FIELD)?;
        let method = MutationMethod::parse(&self.operation).map_err(|error| {
            ApprovalError::InvalidInput {
                field: "operation",
                reason: error.to_string(),
            }
        })?;
        let key = IdempotencyKey::new(self.idempotency_key.clone()).map_err(|error| {
            ApprovalError::InvalidInput {
                field: "idempotency_key",
                reason: error.to_string(),
            }
        })?;
        let binding = OperationBinding::new(
            principal_sid,
            self.actor_id.clone(),
            grant_generation,
            method,
            self.target_id.clone(),
            self.artifact_digest.clone(),
            self.parameters_digest.clone(),
            key,
        )
        .map_err(|error| ApprovalError::InvalidInput {
            field: "operation_binding",
            reason: error.to_string(),
        })?;
        Ok(binding.digest().to_string())
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
    pub grant_generation: u64,
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
    Storage(loki_storage::StorageError),
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
            Self::Storage(error) => write!(f, "encrypted approval database open failed: {error}"),
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
impl From<loki_storage::StorageError> for ApprovalError {
    fn from(value: loki_storage::StorageError) -> Self {
        Self::Storage(value)
    }
}

pub struct ApprovalStore {
    storage: Arc<Storage>,
}

impl ApprovalStore {
    /// Back-compat convenience constructor: builds a private, dedicated,
    /// fully migrated `Storage` at `path` (Windows DPAPI-managed SQLCipher),
    /// then delegates to [`ApprovalStore::new`].
    pub fn open(
        path: impl AsRef<Path>,
        protected_key_path: impl AsRef<Path>,
    ) -> Result<Self, ApprovalError> {
        let storage = Storage::open_managed(path.as_ref(), protected_key_path.as_ref())?;
        storage.run_migrations(BACK_COMPAT_MIGRATION_TIMESTAMP)?;
        Ok(Self::new(Arc::new(storage)))
    }

    /// Back-compat convenience constructor: builds a private, dedicated,
    /// fully migrated in-memory `Storage`, then delegates to
    /// [`ApprovalStore::new`].
    pub fn open_in_memory() -> Result<Self, ApprovalError> {
        let storage = Storage::open_in_memory(Cipher::None)?;
        storage.run_migrations(BACK_COMPAT_MIGRATION_TIMESTAMP)?;
        Ok(Self::new(Arc::new(storage)))
    }

    /// Constructs an `ApprovalStore` over an already-open, already-migrated,
    /// shared `Storage` — the `P1-ACCEPTANCE-STORE-001` composition path.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    pub fn create(
        &self,
        context: &LocalContext,
        authority_session_id: &str,
        request: &ApprovalRequest,
        now_ms: i64,
        lifetime_ms: i64,
    ) -> Result<ApprovalRecord, ApprovalError> {
        request.validate()?;
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
        self.storage.transaction(|tx| {
            let generation =
                match authorized_generation_core(tx, context, authority_session_id, now_ms)? {
                    Ok(generation) => generation,
                    Err(error) => return Ok(Err(ApprovalError::Authority(error))),
                };
            let digest = match request.digest(context.principal_sid(), generation) {
                Ok(digest) => digest,
                Err(error) => return Ok(Err(error)),
            };
            let record = ApprovalRecord {
                approval_id: format!("apr_{}", Uuid::new_v4().simple()),
                request_digest: digest,
                requester_sid: context.principal_sid().to_string(),
                actor_id: request.actor_id.clone(),
                grant_generation: generation,
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
            approvals::insert_approval_core(
                tx,
                &NewApproval {
                    approval_id: record.approval_id.clone(),
                    request_digest: record.request_digest.clone(),
                    binding_version: approvals::CURRENT_BINDING_VERSION,
                    requester_sid: record.requester_sid.clone(),
                    actor_id: record.actor_id.clone(),
                    grant_generation: generation,
                    operation: record.operation.clone(),
                    target_id: record.target_id.clone(),
                    artifact_digest: record.artifact_digest.clone(),
                    parameters_digest: record.parameters_digest.clone(),
                    idempotency_key: record.idempotency_key.clone(),
                    reason: record.reason.clone(),
                    requested_at_ms: record.requested_at_ms,
                    expires_at_ms: record.expires_at_ms,
                },
            )?;
            Ok(Ok(record))
        })?
    }

    pub fn grant(
        &self,
        context: &LocalContext,
        authority_session_id: &str,
        approval_id: &str,
        now_ms: i64,
    ) -> Result<(), ApprovalError> {
        self.storage.transaction(|tx| {
            if let Err(error) =
                authorized_generation_core(tx, context, authority_session_id, now_ms)?
            {
                return Ok(Err(ApprovalError::Authority(error)));
            }
            let Some(record) = approvals::get_approval_core(tx, approval_id)? else {
                return Ok(Err(ApprovalError::NotFound));
            };
            if record.requester_sid != context.principal_sid() {
                return Ok(Err(ApprovalError::BindingMismatch));
            }
            if now_ms >= record.expires_at_ms {
                return Ok(Err(ApprovalError::Expired));
            }
            if record.granted_at_ms.is_some() {
                return Ok(Err(ApprovalError::AlreadyGranted));
            }
            if !approvals::grant_approval_core(tx, approval_id, context.principal_sid(), now_ms)? {
                return Ok(Err(ApprovalError::AlreadyGranted));
            }
            Ok(Ok(()))
        })?
    }
}

fn authorized_generation_core(
    conn: &rusqlite::Connection,
    context: &LocalContext,
    authority_session_id: &str,
    now_ms: i64,
) -> Result<Result<u64, AuthorityError>, loki_storage::StorageError> {
    let Some(session) = local_authority::get_authority_session_core(conn, authority_session_id)?
    else {
        return Ok(Err(AuthorityError::SessionNotFound));
    };
    Ok(evaluate_local_session(
        &session,
        context,
        LocalCapability::RequestConsequentialApproval,
        now_ms,
    ))
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
    use loki_local_authority::LocalAuthority;
    use loki_storage::approvals::ApprovalsRepository;

    fn setup() -> (Arc<Storage>, LocalContext, String, u64, ApprovalStore) {
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let storage = Arc::new(Storage::open_in_memory(Cipher::None).unwrap());
        storage.run_migrations("1970-01-01T00:00:00Z").unwrap();
        let authority = LocalAuthority::new(Arc::clone(&storage));
        let context = LocalContext::current("ipc-approval-test").unwrap();
        authority.initialize_current(&context, 1_000).unwrap();
        let session = authority.issue_session(&context, 2_000, 100_000).unwrap();
        (
            Arc::clone(&storage),
            context,
            session.session_id,
            session.generation,
            ApprovalStore::new(storage),
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
        let digest = base.digest(context.principal_sid(), 1).unwrap();
        let mut changed = base.clone();
        changed.artifact_digest = None;
        assert_ne!(digest, changed.digest(context.principal_sid(), 1).unwrap());
        changed = base.clone();
        changed.target_id.push('x');
        assert_ne!(digest, changed.digest(context.principal_sid(), 1).unwrap());
    }

    #[test]
    fn create_derives_generation_from_same_store_session_and_grant_persists() {
        let (storage, context, session, generation, store) = setup();
        let req = request();
        let record = store
            .create(&context, &session, &req, 3_000, 10_000)
            .unwrap();
        assert_eq!(record.grant_generation, generation);
        store
            .grant(&context, &session, &record.approval_id, 3_200)
            .unwrap();
        let persisted = storage.get_approval(&record.approval_id).unwrap().unwrap();
        assert_eq!(persisted.grant_generation, generation);
        assert!(persisted.granted_at_ms.is_some());
        assert!(persisted.consumed_at_ms.is_none());
    }

    #[test]
    fn session_from_different_storage_cannot_authorize_approval_creation() {
        let (_storage, context, session, _generation, _store) = setup();
        let other_storage = Arc::new(Storage::open_in_memory(Cipher::None).unwrap());
        other_storage
            .run_migrations("1970-01-01T00:00:00Z")
            .unwrap();
        let other_store = ApprovalStore::new(other_storage);
        assert!(matches!(
            other_store.create(&context, &session, &request(), 3_000, 10_000),
            Err(ApprovalError::Authority(AuthorityError::SessionNotFound))
        ));
    }

    #[test]
    fn revoked_session_cannot_create_or_grant() {
        let (storage, context, session, _generation, store) = setup();
        let authority = LocalAuthority::new(storage);
        let record = store
            .create(&context, &session, &request(), 3_000, 10_000)
            .unwrap();
        authority.revoke_session(&context, &session, 3_100).unwrap();
        assert!(matches!(
            store.grant(&context, &session, &record.approval_id, 3_200),
            Err(ApprovalError::Authority(AuthorityError::SessionRevoked))
        ));
        assert!(matches!(
            store.create(&context, &session, &request(), 3_200, 10_000),
            Err(ApprovalError::Authority(AuthorityError::SessionRevoked))
        ));
    }

    #[test]
    fn expired_and_policy_denied_requests_fail_closed() {
        let (_storage, context, session, _generation, store) = setup();
        let req = request();
        let record = store
            .create(&context, &session, &req, 3_000, 1_000)
            .unwrap();
        assert!(matches!(
            store.grant(&context, &session, &record.approval_id, 4_000),
            Err(ApprovalError::Expired)
        ));
        let mut denied = req;
        denied.operation = "factory_reset".into();
        assert!(matches!(
            store.create(&context, &session, &denied, 4_000, 1_000),
            Err(ApprovalError::PolicyDenied)
        ));
    }
}
