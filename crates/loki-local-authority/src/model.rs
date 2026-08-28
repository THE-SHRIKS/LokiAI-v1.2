use uuid::Uuid;

use crate::AuthorityError;

const MAX_SID_LEN: usize = 184;
const MAX_ID_LEN: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalContext {
    principal_sid: String,
    ipc_session_id: String,
}

impl LocalContext {
    /// Resolves the caller from the current Windows process token. The IPC
    /// session ID must come from a completed RUN-002 handshake; it is binding
    /// metadata, not a credential created by this crate.
    pub fn current(ipc_session_id: impl Into<String>) -> Result<Self, AuthorityError> {
        let principal_sid = loki_ipc::acl::current_user_sid_string()?;
        Self::validated(principal_sid, ipc_session_id.into())
    }

    fn validated(principal_sid: String, ipc_session_id: String) -> Result<Self, AuthorityError> {
        validate_bounded("principal_sid", &principal_sid, MAX_SID_LEN)?;
        if !principal_sid.starts_with("S-1-") {
            return Err(AuthorityError::InvalidInput {
                field: "principal_sid",
                reason: "must be a Windows SID".to_string(),
            });
        }
        validate_bounded("ipc_session_id", &ipc_session_id, MAX_ID_LEN)?;
        Ok(Self {
            principal_sid,
            ipc_session_id,
        })
    }

    pub fn principal_sid(&self) -> &str {
        &self.principal_sid
    }

    pub fn ipc_session_id(&self) -> &str {
        &self.ipc_session_id
    }

    #[cfg(test)]
    pub(crate) fn verified_for_test(
        principal_sid: &str,
        ipc_session_id: &str,
    ) -> Result<Self, AuthorityError> {
        Self::validated(principal_sid.to_string(), ipc_session_id.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalInstallation {
    pub installation_id: String,
    pub owner_sid: String,
    pub created_at_ms: i64,
}

/// `P1-PRINCIPAL-GRANT-001` (ADR-0007 Option B): `generation` is a real,
/// durable, per-`principal_sid` counter assigned atomically by the storage
/// layer (`loki_storage::local_authority::insert_authority_session_core`'s
/// `MAX(generation)+1`-then-`INSERT`, under one `Storage::lock()`-held
/// critical section) — never computed or assumed by this crate. This is
/// entirely independent of `crates/loki-agents`'s `AgentCredential`
/// generation sequence: revoking one Windows-principal's session generation
/// never cascades to revoke any agent connection, and vice versa (ADR-0007
/// explicitly declines a shared "one kill switch" property — see that
/// decision's "Consequences" section).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSession {
    pub session_id: String,
    pub installation_id: String,
    pub principal_sid: String,
    pub ipc_session_id: String,
    pub correlation_id: String,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
    /// Always overwritten by the storage layer's atomic assignment before
    /// this value reaches any caller — never trust this placeholder
    /// directly. [`LocalSession::issue`] sets it to `0` only because a
    /// value must exist before the storage layer's real assignment is
    /// known; `crates/loki-local-authority/src/store.rs`'s `issue_session`
    /// always substitutes the real, storage-assigned generation before
    /// returning.
    pub generation: u64,
}

impl LocalSession {
    pub(crate) fn issue(
        installation: &LocalInstallation,
        context: &LocalContext,
        now_ms: i64,
        lifetime_ms: i64,
    ) -> Result<Self, AuthorityError> {
        if now_ms < 0 {
            return Err(AuthorityError::InvalidInput {
                field: "now_ms",
                reason: "must be non-negative UTC epoch milliseconds".to_string(),
            });
        }
        if !(1_000..=86_400_000).contains(&lifetime_ms) {
            return Err(AuthorityError::InvalidInput {
                field: "lifetime_ms",
                reason: "must be between one second and 24 hours".to_string(),
            });
        }
        let expires_at_ms =
            now_ms
                .checked_add(lifetime_ms)
                .ok_or_else(|| AuthorityError::InvalidInput {
                    field: "lifetime_ms",
                    reason: "timestamp overflow".to_string(),
                })?;
        Ok(Self {
            session_id: format!("ls_{}", Uuid::new_v4().simple()),
            installation_id: installation.installation_id.clone(),
            principal_sid: context.principal_sid.clone(),
            ipc_session_id: context.ipc_session_id.clone(),
            correlation_id: format!("corr_{}", Uuid::new_v4().simple()),
            issued_at_ms: now_ms,
            expires_at_ms,
            revoked_at_ms: None,
            // Placeholder — see this field's doc comment above.
            generation: 0,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalCapability {
    ReadLocalState,
    RegisterAgent,
    BindLocalDevice,
    RequestConsequentialApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudCapability {
    Account,
    Workspace,
    Role,
    Enrollment,
    Synchronization,
    RemoteNotification,
    RemoteAgentDelivery,
}

pub(crate) fn validate_bounded(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), AuthorityError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(AuthorityError::InvalidInput {
            field,
            reason: format!("must contain 1..={max} non-control bytes"),
        });
    }
    Ok(())
}
