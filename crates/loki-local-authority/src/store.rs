use std::{path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use crate::{
    AuthorityError, CloudCapability, LocalCapability, LocalContext, LocalInstallation, LocalSession,
};

pub struct LocalAuthority {
    connection: Mutex<Connection>,
}

impl LocalAuthority {
    pub fn open(
        path: impl AsRef<Path>,
        protected_key_path: impl AsRef<Path>,
    ) -> Result<Self, AuthorityError> {
        let (connection, _key) =
            loki_storage::open_or_migrate_sqlcipher(path.as_ref(), protected_key_path.as_ref())?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self, AuthorityError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, AuthorityError> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS local_installation_authority (
               singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
               installation_id TEXT NOT NULL UNIQUE,
               owner_sid TEXT NOT NULL,
               created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
             );
             CREATE TABLE IF NOT EXISTS local_authority_sessions (
               session_id TEXT PRIMARY KEY,
               installation_id TEXT NOT NULL,
               principal_sid TEXT NOT NULL,
               ipc_session_id TEXT NOT NULL,
               correlation_id TEXT NOT NULL UNIQUE,
               issued_at_ms INTEGER NOT NULL CHECK (issued_at_ms >= 0),
               expires_at_ms INTEGER NOT NULL CHECK (expires_at_ms > issued_at_ms),
               revoked_at_ms INTEGER,
               FOREIGN KEY (installation_id)
                 REFERENCES local_installation_authority(installation_id)
             );
             CREATE INDEX IF NOT EXISTS idx_local_sessions_expiry
               ON local_authority_sessions(expires_at_ms);",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
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
        let mut connection = self.connection.lock().map_err(|_| {
            AuthorityError::Conflict("authority database lock was poisoned".to_string())
        })?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = load_installation(&transaction)?;
        let installation = match existing {
            Some(existing) => {
                if existing.owner_sid != context.principal_sid() {
                    return Err(AuthorityError::WrongPrincipal);
                }
                existing
            }
            None => {
                let installation = LocalInstallation {
                    installation_id: format!("li_{}", Uuid::new_v4().simple()),
                    owner_sid: context.principal_sid().to_string(),
                    created_at_ms: now_ms,
                };
                transaction.execute(
                    "INSERT INTO local_installation_authority
                     (singleton, installation_id, owner_sid, created_at_ms)
                     VALUES (1, ?1, ?2, ?3)",
                    params![
                        installation.installation_id,
                        installation.owner_sid,
                        installation.created_at_ms
                    ],
                )?;
                installation
            }
        };
        transaction.commit()?;
        Ok(installation)
    }

    pub fn issue_session(
        &self,
        context: &LocalContext,
        now_ms: i64,
        lifetime_ms: i64,
    ) -> Result<LocalSession, AuthorityError> {
        let mut connection = self.connection.lock().map_err(|_| {
            AuthorityError::Conflict("authority database lock was poisoned".to_string())
        })?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let installation = load_installation(&transaction)?.ok_or_else(|| {
            AuthorityError::Conflict("installation authority is not initialized".to_string())
        })?;
        if installation.owner_sid != context.principal_sid() {
            return Err(AuthorityError::WrongPrincipal);
        }
        let session = LocalSession::issue(&installation, context, now_ms, lifetime_ms)?;
        transaction.execute(
            "INSERT INTO local_authority_sessions
             (session_id, installation_id, principal_sid, ipc_session_id,
              correlation_id, issued_at_ms, expires_at_ms, revoked_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
            params![
                session.session_id,
                session.installation_id,
                session.principal_sid,
                session.ipc_session_id,
                session.correlation_id,
                session.issued_at_ms,
                session.expires_at_ms
            ],
        )?;
        transaction.commit()?;
        Ok(session)
    }

    pub fn authorize_local(
        &self,
        context: &LocalContext,
        session_id: &str,
        capability: LocalCapability,
        now_ms: i64,
    ) -> Result<(), AuthorityError> {
        let session = self.require_live_session(context, session_id, now_ms)?;
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
        let mut connection = self.connection.lock().map_err(|_| {
            AuthorityError::Conflict("authority database lock was poisoned".to_string())
        })?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let session =
            load_session(&transaction, session_id)?.ok_or(AuthorityError::SessionNotFound)?;
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
        let changed = transaction.execute(
            "UPDATE local_authority_sessions SET revoked_at_ms = ?1
             WHERE session_id = ?2 AND revoked_at_ms IS NULL",
            params![now_ms, session_id],
        )?;
        if changed != 1 {
            return Err(AuthorityError::SessionRevoked);
        }
        transaction.commit()?;
        Ok(())
    }

    fn require_live_session(
        &self,
        context: &LocalContext,
        session_id: &str,
        now_ms: i64,
    ) -> Result<LocalSession, AuthorityError> {
        let connection = self.connection.lock().map_err(|_| {
            AuthorityError::Conflict("authority database lock was poisoned".to_string())
        })?;
        let session =
            load_session(&connection, session_id)?.ok_or(AuthorityError::SessionNotFound)?;
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
        if now_ms >= session.expires_at_ms {
            return Err(AuthorityError::SessionExpired);
        }
        Ok(session)
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

fn load_installation(connection: &Connection) -> Result<Option<LocalInstallation>, AuthorityError> {
    connection
        .query_row(
            "SELECT installation_id, owner_sid, created_at_ms
             FROM local_installation_authority WHERE singleton = 1",
            [],
            |row| {
                Ok(LocalInstallation {
                    installation_id: row.get(0)?,
                    owner_sid: row.get(1)?,
                    created_at_ms: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

fn load_session(
    connection: &Connection,
    session_id: &str,
) -> Result<Option<LocalSession>, AuthorityError> {
    connection
        .query_row(
            "SELECT session_id, installation_id, principal_sid, ipc_session_id,
                    correlation_id, issued_at_ms, expires_at_ms, revoked_at_ms
             FROM local_authority_sessions WHERE session_id = ?1",
            [session_id],
            |row| {
                Ok(LocalSession {
                    session_id: row.get(0)?,
                    installation_id: row.get(1)?,
                    principal_sid: row.get(2)?,
                    ipc_session_id: row.get(3)?,
                    correlation_id: row.get(4)?,
                    issued_at_ms: row.get(5)?,
                    expires_at_ms: row.get(6)?,
                    revoked_at_ms: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
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
                .windows(b"local_authority_state".len())
                .any(|window| window == b"local_authority_state"),
            "authority schema marker must not be plaintext on disk"
        );
    }

    #[test]
    fn concurrent_revocation_has_exactly_one_winner() {
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
}
