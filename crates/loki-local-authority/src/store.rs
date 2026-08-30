use std::{path::Path, sync::Arc};

use loki_storage::local_authority::{
    AuthoritySessionRow, InstallationRow, LocalAuthorityRepository, NewAuthoritySessionRow,
};
use loki_storage::{Cipher, Storage};
use uuid::Uuid;

use crate::{
    AuthorityError, CloudCapability, LocalCapability, LocalContext, LocalInstallation, LocalSession,
};

/// Back-compat constructors ([`LocalAuthority::open`]/[`LocalAuthority::open_in_memory`])
/// build a private `Storage` and must supply *some* `applied_at` value to
/// [`Storage::run_migrations`]; this crate has no clock (that stance is
/// `loki-storage`'s own — see `loki_storage::timestamp`), and the value is
/// pure `schema_migrations` bookkeeping, never read back by this crate. A
/// fixed epoch sentinel keeps that honest rather than fabricating a
/// plausible-looking "now."
const BACK_COMPAT_MIGRATION_TIMESTAMP: &str = "1970-01-01T00:00:00Z";

/// Windows-bound local installation authority (`LOC-001`).
///
/// # `P1-ACCEPTANCE-STORE-001` consolidation
///
/// Previously wrapped its own independent `Mutex<Connection>`, opened via
/// `loki_storage::open_or_migrate_sqlcipher`/`Connection::open_in_memory()`,
/// running its own ad hoc `CREATE TABLE IF NOT EXISTS` schema. Now wraps a
/// shared `loki_storage::Storage` (the same one `crates/loki-approvals`,
/// `crates/loki-agents`, `crates/loki-agent-delivery`, and
/// `crates/loki-acceptance-store` can share), with schema owned by
/// `loki-storage`'s own versioned migrations
/// (`migrations/0006_agent_delivery_and_local_authority.sql`) and all SQL
/// moved into `loki_storage::local_authority`. [`LocalAuthority::open`]/
/// [`LocalAuthority::open_in_memory`] remain unmodified in signature and
/// behavior (each now builds its own private `Storage` internally) so
/// existing callers outside this consolidation (`crates/loki-local-devices`,
/// `crates/loki-local-setup`) keep working unmodified — [`LocalAuthority::new`]
/// is the new shared-`Storage` construction path this packet requires.
pub struct LocalAuthority {
    storage: Arc<Storage>,
}

impl LocalAuthority {
    /// Back-compat convenience constructor: builds a private, dedicated,
    /// fully migrated `Storage` at `path` (Windows DPAPI-managed SQLCipher),
    /// then delegates to [`LocalAuthority::new`].
    pub fn open(
        path: impl AsRef<Path>,
        protected_key_path: impl AsRef<Path>,
    ) -> Result<Self, AuthorityError> {
        let storage = Storage::open_managed(path.as_ref(), protected_key_path.as_ref())?;
        storage.run_migrations(BACK_COMPAT_MIGRATION_TIMESTAMP)?;
        Ok(Self::new(Arc::new(storage)))
    }

    /// Back-compat convenience constructor: builds a private, dedicated,
    /// fully migrated in-memory `Storage`, then delegates to
    /// [`LocalAuthority::new`].
    pub fn open_in_memory() -> Result<Self, AuthorityError> {
        let storage = Storage::open_in_memory(Cipher::None)?;
        storage.run_migrations(BACK_COMPAT_MIGRATION_TIMESTAMP)?;
        Ok(Self::new(Arc::new(storage)))
    }

    /// Constructs a `LocalAuthority` over an already-open, already-migrated,
    /// shared `Storage` — the `P1-ACCEPTANCE-STORE-001` composition path.
    /// Infallible: unlike the old per-crate connection, schema readiness is
    /// the caller's responsibility (via `Storage::run_migrations`), not
    /// this constructor's.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Creates the installation record once, or verifies that the existing
    /// record belongs to this Windows principal. A different principal can
    /// never replace or adopt the record through this API.
    pub fn initialize_current(
        &self,
        context: &LocalContext,
        now_ms: i64,
    ) -> Result<LocalInstallation, AuthorityError> {
        if now_ms < 0 {
            return Err(AuthorityError::InvalidInput {
                field: "now_ms",
                reason: "must be non-negative UTC epoch milliseconds".to_string(),
            });
        }
        let existing = self.storage.get_installation()?;
        let installation = match existing {
            Some(existing) => {
                if existing.owner_sid != context.principal_sid() {
                    return Err(AuthorityError::WrongPrincipal);
                }
                LocalInstallation {
                    installation_id: existing.installation_id,
                    owner_sid: existing.owner_sid,
                    created_at_ms: existing.created_at_ms,
                }
            }
            None => {
                let installation = LocalInstallation {
                    installation_id: format!("li_{}", Uuid::new_v4().simple()),
                    owner_sid: context.principal_sid().to_string(),
                    created_at_ms: now_ms,
                };
                self.storage.insert_installation(&InstallationRow {
                    installation_id: installation.installation_id.clone(),
                    owner_sid: installation.owner_sid.clone(),
                    created_at_ms: installation.created_at_ms,
                })?;
                installation
            }
        };
        Ok(installation)
    }

    pub fn issue_session(
        &self,
        context: &LocalContext,
        now_ms: i64,
        lifetime_ms: i64,
    ) -> Result<LocalSession, AuthorityError> {
        let installation = self.storage.get_installation()?.ok_or_else(|| {
            AuthorityError::Conflict("installation authority is not initialized".to_string())
        })?;
        if installation.owner_sid != context.principal_sid() {
            return Err(AuthorityError::WrongPrincipal);
        }
        let installation = LocalInstallation {
            installation_id: installation.installation_id,
            owner_sid: installation.owner_sid,
            created_at_ms: installation.created_at_ms,
        };
        let session = LocalSession::issue(&installation, context, now_ms, lifetime_ms)?;
        // `P1-PRINCIPAL-GRANT-001`: the storage layer atomically assigns
        // this principal's next durable session generation and returns it
        // — this is the real value; `LocalSession::issue`'s own `0` is only
        // ever a placeholder overwritten right here, before this session is
        // returned to any caller.
        let generation = self
            .storage
            .insert_authority_session(&NewAuthoritySessionRow {
                session_id: session.session_id.clone(),
                installation_id: session.installation_id.clone(),
                principal_sid: session.principal_sid.clone(),
                ipc_session_id: session.ipc_session_id.clone(),
                correlation_id: session.correlation_id.clone(),
                issued_at_ms: session.issued_at_ms,
                expires_at_ms: session.expires_at_ms,
            })?;
        Ok(LocalSession {
            generation,
            ..session
        })
    }

    pub fn authorize_local(
        &self,
        context: &LocalContext,
        session_id: &str,
        capability: LocalCapability,
        now_ms: i64,
    ) -> Result<(), AuthorityError> {
        let session = self
            .storage
            .get_authority_session(session_id)?
            .ok_or(AuthorityError::SessionNotFound)?;
        evaluate_local_session(&session, context, capability, now_ms)?;
        Ok(())
    }

    pub fn authorize_cloud(
        &self,
        _context: &LocalContext,
        _session_id: &str,
        _capability: CloudCapability,
        _now_ms: i64,
    ) -> Result<(), AuthorityError> {
        Err(AuthorityError::CloudUnavailable)
    }

    pub fn revoke_session(
        &self,
        context: &LocalContext,
        session_id: &str,
        now_ms: i64,
    ) -> Result<(), AuthorityError> {
        let session = self
            .storage
            .get_authority_session(session_id)?
            .ok_or(AuthorityError::SessionNotFound)?;
        let session = row_to_session(session);
        validate_context(&session, context)?;
        if session.revoked_at_ms.is_some() {
            return Err(AuthorityError::SessionRevoked);
        }
        if now_ms < session.issued_at_ms {
            return Err(AuthorityError::InvalidInput {
                field: "now_ms",
                reason: "cannot predate session issuance".to_string(),
            });
        }
        let revoked = self.storage.revoke_authority_session(session_id, now_ms)?;
        if !revoked {
            return Err(AuthorityError::SessionRevoked);
        }
        Ok(())
    }
}

/// Pure authorization evaluation over a session row already read from the
/// caller's chosen transaction. Approval creation uses this function after
/// reading the row from the same connection that persists the approval, so
/// session validation and generation derivation cannot cross databases or
/// race a revocation between authorization and insert.
pub fn evaluate_local_session(
    session: &AuthoritySessionRow,
    context: &LocalContext,
    capability: LocalCapability,
    now_ms: i64,
) -> Result<u64, AuthorityError> {
    if session.principal_sid != context.principal_sid()
        || session.ipc_session_id != context.ipc_session_id()
    {
        return Err(AuthorityError::SessionContextMismatch);
    }
    if session.revoked_at_ms.is_some() {
        return Err(AuthorityError::SessionRevoked);
    }
    if now_ms < session.issued_at_ms {
        return Err(AuthorityError::InvalidInput {
            field: "now_ms",
            reason: "cannot predate session issuance".to_string(),
        });
    }
    if now_ms >= session.expires_at_ms {
        return Err(AuthorityError::SessionExpired);
    }
    let allowed = matches!(
        capability,
        LocalCapability::ReadLocalState
            | LocalCapability::RegisterAgent
            | LocalCapability::BindLocalDevice
            | LocalCapability::RequestConsequentialApproval
    );
    if !allowed || session.installation_id.is_empty() {
        return Err(AuthorityError::CapabilityDenied);
    }
    Ok(session.generation)
}

fn row_to_session(row: AuthoritySessionRow) -> LocalSession {
    LocalSession {
        session_id: row.session_id,
        installation_id: row.installation_id,
        principal_sid: row.principal_sid,
        ipc_session_id: row.ipc_session_id,
        correlation_id: row.correlation_id,
        issued_at_ms: row.issued_at_ms,
        expires_at_ms: row.expires_at_ms,
        revoked_at_ms: row.revoked_at_ms,
        generation: row.generation,
    }
}

fn validate_context(session: &LocalSession, context: &LocalContext) -> Result<(), AuthorityError> {
    if session.principal_sid != context.principal_sid()
        || session.ipc_session_id != context.ipc_session_id()
    {
        return Err(AuthorityError::SessionContextMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    const OWNER: &str = "S-1-5-21-1000";
    const OTHER: &str = "S-1-5-21-2000";

    fn context(sid: &str, ipc: &str) -> LocalContext {
        LocalContext::verified_for_test(sid, ipc).unwrap()
    }

    fn initialized() -> (LocalAuthority, LocalContext) {
        // Even the in-memory path shares this process-wide SQLite
        // installation with `restart_preserves_installation_and_
        // revocation_metadata`, which opens a real SqlCipher-backed store
        // through the secure-memory arena (DAT-002-MEM-CODE-001).
        // `sqlite3_config` can only run before SQLite's first implicit
        // initialization, so every SQLite-touching test in this binary —
        // in-memory or not — must bootstrap the arena first, regardless of
        // which test happens to run first.
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let authority = LocalAuthority::open_in_memory().unwrap();
        let owner = context(OWNER, "ipc-1");
        authority.initialize_current(&owner, 1_000).unwrap();
        (authority, owner)
    }

    #[test]
    fn wrong_principal_cannot_adopt_or_issue_for_installation() {
        let (authority, _) = initialized();
        let other = context(OTHER, "ipc-2");
        assert!(matches!(
            authority.initialize_current(&other, 2_000),
            Err(AuthorityError::WrongPrincipal)
        ));
        assert!(matches!(
            authority.issue_session(&other, 2_000, 60_000),
            Err(AuthorityError::WrongPrincipal)
        ));
    }

    #[test]
    fn session_is_bound_to_principal_and_authenticated_ipc_context() {
        let (authority, owner) = initialized();
        let session = authority.issue_session(&owner, 2_000, 60_000).unwrap();
        authority
            .authorize_local(
                &owner,
                &session.session_id,
                LocalCapability::ReadLocalState,
                3_000,
            )
            .unwrap();
        let other_ipc = context(OWNER, "ipc-2");
        assert!(matches!(
            authority.authorize_local(
                &other_ipc,
                &session.session_id,
                LocalCapability::ReadLocalState,
                3_000
            ),
            Err(AuthorityError::SessionContextMismatch)
        ));
    }

    #[test]
    fn expiry_and_revocation_are_fail_closed_and_single_use() {
        let (authority, owner) = initialized();
        let expired = authority.issue_session(&owner, 2_000, 1_000).unwrap();
        assert!(matches!(
            authority.authorize_local(
                &owner,
                &expired.session_id,
                LocalCapability::RegisterAgent,
                3_000
            ),
            Err(AuthorityError::SessionExpired)
        ));

        let live = authority.issue_session(&owner, 4_000, 60_000).unwrap();
        authority
            .revoke_session(&owner, &live.session_id, 5_000)
            .unwrap();
        assert!(matches!(
            authority.authorize_local(
                &owner,
                &live.session_id,
                LocalCapability::RegisterAgent,
                5_001
            ),
            Err(AuthorityError::SessionRevoked)
        ));
        assert!(matches!(
            authority.revoke_session(&owner, &live.session_id, 5_002),
            Err(AuthorityError::SessionRevoked)
        ));
    }

    #[test]
    fn every_cloud_capability_is_explicitly_unavailable() {
        let (authority, owner) = initialized();
        let session = authority.issue_session(&owner, 2_000, 60_000).unwrap();
        let capabilities = [
            CloudCapability::Account,
            CloudCapability::Workspace,
            CloudCapability::Role,
            CloudCapability::Enrollment,
            CloudCapability::Synchronization,
            CloudCapability::RemoteNotification,
            CloudCapability::RemoteAgentDelivery,
        ];
        for capability in capabilities {
            assert!(matches!(
                authority.authorize_cloud(&owner, &session.session_id, capability, 3_000),
                Err(AuthorityError::CloudUnavailable)
            ));
        }
    }

    #[test]
    fn malformed_identity_and_unbounded_lifetime_are_rejected() {
        assert!(LocalContext::verified_for_test("not-a-sid", "ipc").is_err());
        let (authority, owner) = initialized();
        assert!(matches!(
            authority.issue_session(&owner, 2_000, 86_400_001),
            Err(AuthorityError::InvalidInput { .. })
        ));
    }

    #[test]
    fn restart_preserves_installation_and_revocation_metadata() {
        // `LocalAuthority::open` goes through `loki_storage::
        // open_or_migrate_sqlcipher`, which now requires the process-global
        // secure-memory arena to be ready first (DAT-002-MEM-CODE-001,
        // ADR-0005). Production never embeds an accepted bounds profile at
        // this revision (fail-closed by design), so this test bootstraps a
        // disposable, non-production test-only arena instead — see
        // `loki_storage::secure_memory::test_support` for why this bypass
        // cannot reach a release build.
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("authority.sqlite3");
        let key_path = directory.path().join("authority.key.dpapi");
        let owner = context(OWNER, "ipc-1");
        let installation_id;
        let session_id;
        {
            let authority = LocalAuthority::open(&path, &key_path).unwrap();
            installation_id = authority
                .initialize_current(&owner, 1_000)
                .unwrap()
                .installation_id;
            session_id = authority
                .issue_session(&owner, 2_000, 60_000)
                .unwrap()
                .session_id;
            authority
                .revoke_session(&owner, &session_id, 3_000)
                .unwrap();
        }
        let reopened = LocalAuthority::open(&path, &key_path).unwrap();
        assert_eq!(
            reopened
                .initialize_current(&owner, 4_000)
                .unwrap()
                .installation_id,
            installation_id
        );
        assert!(matches!(
            reopened.authorize_local(&owner, &session_id, LocalCapability::ReadLocalState, 4_000),
            Err(AuthorityError::SessionRevoked)
        ));
        assert!(
            !std::fs::read(&path)
                .unwrap()
                .windows(b"local_authority_sessions".len())
                .any(|window| window == b"local_authority_sessions"),
            "authority schema marker must not be plaintext on disk"
        );
    }

    #[test]
    fn concurrent_revocation_has_exactly_one_winner() {
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let authority = Arc::new(LocalAuthority::open_in_memory().unwrap());
        let owner = context(OWNER, "ipc-1");
        authority.initialize_current(&owner, 1_000).unwrap();
        let session = authority.issue_session(&owner, 2_000, 60_000).unwrap();
        let barrier = Arc::new(Barrier::new(3));

        let attempts: Vec<_> = (0..2)
            .map(|offset| {
                let authority = Arc::clone(&authority);
                let barrier = Arc::clone(&barrier);
                let owner = owner.clone();
                let session_id = session.session_id.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    authority.revoke_session(&owner, &session_id, 3_000 + offset)
                })
            })
            .collect();
        barrier.wait();

        let results: Vec<_> = attempts
            .into_iter()
            .map(|attempt| attempt.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(AuthorityError::SessionRevoked)))
                .count(),
            1
        );
    }

    /// `P1-PRINCIPAL-GRANT-001` acceptance criterion (Section 7/8): two real
    /// OS threads issuing sessions for the *same* principal simultaneously
    /// must never produce a duplicate `generation` — the atomic
    /// `MAX(generation)+1`-then-`INSERT` critical section inside
    /// `insert_authority_session_core`, serialized by `Storage`'s
    /// single-writer mutex, is what this test actually proves, not merely
    /// sequential-test-passing. Mirrors `concurrent_revocation_has_exactly_one_winner`'s
    /// real-thread style.
    #[test]
    fn concurrent_session_issuance_yields_distinct_generations() {
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let authority = Arc::new(LocalAuthority::open_in_memory().unwrap());
        let owner = context(OWNER, "ipc-1");
        authority.initialize_current(&owner, 1_000).unwrap();
        let barrier = Arc::new(Barrier::new(8));

        let attempts: Vec<_> = (0..8)
            .map(|offset| {
                let authority = Arc::clone(&authority);
                let barrier = Arc::clone(&barrier);
                let owner = owner.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    authority
                        .issue_session(&owner, 2_000 + offset, 60_000)
                        .unwrap()
                })
            })
            .collect();

        let sessions: Vec<_> = attempts
            .into_iter()
            .map(|attempt| attempt.join().unwrap())
            .collect();
        let mut generations: Vec<u64> = sessions.iter().map(|s| s.generation).collect();
        generations.sort_unstable();
        let mut deduped = generations.clone();
        deduped.dedup();
        assert_eq!(
            generations.len(),
            deduped.len(),
            "no two concurrently issued sessions for the same principal may share a generation: {generations:?}"
        );
        assert_eq!(generations, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }
}
