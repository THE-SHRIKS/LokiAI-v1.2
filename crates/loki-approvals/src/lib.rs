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

use loki_local_authority::{AuthorityError, LocalAuthority, LocalCapability, LocalContext};
use loki_storage::approvals::{ApprovalsRepository, ConsumeOutcome, NewApproval};
use loki_storage::{Cipher, Storage};
use sha2::{Digest, Sha256};
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
        self.storage.insert_approval(&NewApproval {
            approval_id: record.approval_id.clone(),
            request_digest: record.request_digest.clone(),
            requester_sid: record.requester_sid.clone(),
            actor_id: record.actor_id.clone(),
            operation: record.operation.clone(),
            target_id: record.target_id.clone(),
            artifact_digest: record.artifact_digest.clone(),
            parameters_digest: record.parameters_digest.clone(),
            idempotency_key: record.idempotency_key.clone(),
            reason: record.reason.clone(),
            requested_at_ms: record.requested_at_ms,
            expires_at_ms: record.expires_at_ms,
        })?;
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
        let record = self
            .storage
            .get_approval(approval_id)?
            .ok_or(ApprovalError::NotFound)?;
        if record.requester_sid != context.principal_sid() {
            return Err(ApprovalError::BindingMismatch);
        }
        if now_ms >= record.expires_at_ms {
            return Err(ApprovalError::Expired);
        }
        if record.granted_at_ms.is_some() {
            return Err(ApprovalError::AlreadyGranted);
        }
        let granted = self
            .storage
            .grant_approval(approval_id, context.principal_sid(), now_ms)?;
        if !granted {
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
        match self.storage.consume_approval(
            approval_id,
            &expected,
            context.principal_sid(),
            now_ms,
        )? {
            ConsumeOutcome::NotFound => Err(ApprovalError::NotFound),
            ConsumeOutcome::BindingMismatch => Err(ApprovalError::BindingMismatch),
            ConsumeOutcome::NotGranted => Err(ApprovalError::NotGranted),
            ConsumeOutcome::Expired => Err(ApprovalError::Expired),
            ConsumeOutcome::AlreadyConsumed => Err(ApprovalError::AlreadyConsumed),
            ConsumeOutcome::Consumed(_) => Ok(()),
        }
    }
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
    use std::sync::Barrier;

    fn setup() -> (LocalAuthority, LocalContext, String, ApprovalStore) {
        // Even the in-memory path shares this process-wide SQLite
        // installation with
        // `encrypted_restart_preserves_grant_and_single_use_state`, which
        // opens a real SqlCipher-backed store through the secure-memory
        // arena (DAT-002-MEM-CODE-001). `sqlite3_config` can only run
        // before SQLite's first implicit initialization, so every
        // SQLite-touching test in this binary — in-memory or not — must
        // bootstrap the arena first, regardless of which test happens to
        // run first.
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
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
    fn encrypted_restart_preserves_grant_and_single_use_state() {
        // `ApprovalStore::open` goes through `loki_storage::
        // open_or_migrate_sqlcipher`, which now requires the process-global
        // secure-memory arena to be ready first (DAT-002-MEM-CODE-001,
        // ADR-0005). Production never embeds an accepted bounds profile at
        // this revision (fail-closed by design), so this test bootstraps a
        // disposable, non-production test-only arena instead — see
        // `loki_storage::secure_memory::test_support` for why this bypass
        // cannot reach a release build.
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let (authority, context, session, _) = setup();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("approvals.sqlite3");
        let key_path = directory.path().join("approvals.key.dpapi");
        let req = request();
        let approval_id;
        {
            let store = ApprovalStore::open(&path, &key_path).unwrap();
            let record = store
                .create(&authority, &context, &session, &req, 3_000, 10_000)
                .unwrap();
            approval_id = record.approval_id;
            store
                .grant(&authority, &context, &session, &approval_id, 3_100)
                .unwrap();
        }

        assert!(
            !std::fs::read(&path)
                .unwrap()
                .windows(b"local_approvals".len())
                .any(|window| window == b"local_approvals"),
            "approval schema marker must not be plaintext on disk"
        );
        let reopened = ApprovalStore::open(&path, &key_path).unwrap();
        reopened
            .consume(&authority, &context, &session, &approval_id, &req, 3_200)
            .unwrap();
        assert!(matches!(
            reopened.consume(&authority, &context, &session, &approval_id, &req, 3_300),
            Err(ApprovalError::AlreadyConsumed)
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
