//! Installation-scoped local device binding (`LDEV-001`).
//!
//! A binding is deliberately not cloud enrollment or ownership. It records that
//! a connector-qualified stable hardware identity was explicitly associated with
//! this local installation by its Windows-bound owner. Transport references such
//! as COM ports are never accepted as identity input.

use std::{fmt, path::Path, sync::Mutex};

use loki_local_authority::{AuthorityError, LocalAuthority, LocalCapability, LocalContext};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

const MAX_ID: usize = 128;
const MAX_EVIDENCE: usize = 512;

#[derive(Debug)]
pub enum DeviceBindingError {
    Authority(AuthorityError),
    Sqlite(rusqlite::Error),
    InvalidInput { field: &'static str, reason: String },
    Replay,
    Conflict(String),
    NotFound,
    Revoked,
    IdentityMismatch,
}

impl fmt::Display for DeviceBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authority(error) => write!(f, "local authority denied device binding: {error}"),
            Self::Sqlite(error) => write!(f, "local device database error: {error}"),
            Self::InvalidInput { field, reason } => {
                write!(f, "invalid local device field '{field}': {reason}")
            }
            Self::Replay => write!(f, "local device binding request nonce was already used"),
            Self::Conflict(detail) => write!(f, "local device binding conflict: {detail}"),
            Self::NotFound => write!(f, "local device binding was not found"),
            Self::Revoked => write!(f, "local device binding is revoked"),
            Self::IdentityMismatch => write!(f, "current device identity does not match binding"),
        }
    }
}

impl std::error::Error for DeviceBindingError {}

impl From<AuthorityError> for DeviceBindingError {
    fn from(value: AuthorityError) -> Self {
        Self::Authority(value)
    }
}

impl From<rusqlite::Error> for DeviceBindingError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindRequest {
    pub connector_id: String,
    pub hardware_identity: String,
    pub runtime_id: String,
    pub evidence_ref: String,
    pub request_nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDeviceBinding {
    pub binding_id: String,
    pub connector_id: String,
    pub hardware_identity: String,
    pub runtime_id: String,
    pub evidence_ref: String,
    pub bound_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindOutcome {
    Created(LocalDeviceBinding),
    Existing(LocalDeviceBinding),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentDeviceIdentity<'a> {
    pub connector_id: &'a str,
    pub hardware_identity: &'a str,
    pub runtime_id: &'a str,
}

pub struct LocalDeviceBindings {
    connection: Mutex<Connection>,
}

impl LocalDeviceBindings {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DeviceBindingError> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, DeviceBindingError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, DeviceBindingError> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS local_device_bindings (
               binding_id TEXT PRIMARY KEY,
               connector_id TEXT NOT NULL,
               hardware_identity TEXT NOT NULL,
               runtime_id TEXT NOT NULL,
               evidence_ref TEXT NOT NULL,
               bound_at_ms INTEGER NOT NULL CHECK (bound_at_ms >= 0),
               revoked_at_ms INTEGER
             );
             CREATE UNIQUE INDEX IF NOT EXISTS idx_active_local_device_identity
               ON local_device_bindings(connector_id, hardware_identity)
               WHERE revoked_at_ms IS NULL;
             CREATE TABLE IF NOT EXISTS local_device_binding_nonces (
               request_nonce TEXT PRIMARY KEY,
               used_at_ms INTEGER NOT NULL CHECK (used_at_ms >= 0)
             );",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn bind(
        &self,
        authority: &LocalAuthority,
        context: &LocalContext,
        authority_session_id: &str,
        request: &BindRequest,
        now_ms: i64,
    ) -> Result<BindOutcome, DeviceBindingError> {
        authority.authorize_local(
            context,
            authority_session_id,
            LocalCapability::BindLocalDevice,
            now_ms,
        )?;
        validate_request(request, now_ms)?;
        let mut connection = self.connection.lock().map_err(|_| {
            DeviceBindingError::Conflict("device-binding database lock poisoned".to_string())
        })?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let nonce_used: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM local_device_binding_nonces WHERE request_nonce = ?1)",
            [&request.request_nonce],
            |row| row.get(0),
        )?;
        if nonce_used {
            return Err(DeviceBindingError::Replay);
        }

        let existing = find_active(
            &transaction,
            &request.connector_id,
            &request.hardware_identity,
        )?;
        transaction.execute(
            "INSERT INTO local_device_binding_nonces(request_nonce, used_at_ms) VALUES (?1, ?2)",
            params![request.request_nonce, now_ms],
        )?;
        let outcome = if let Some(existing) = existing {
            if existing.runtime_id != request.runtime_id
                || existing.evidence_ref != request.evidence_ref
            {
                return Err(DeviceBindingError::Conflict(
                    "active binding cannot be overwritten; revoke and rebind explicitly"
                        .to_string(),
                ));
            }
            BindOutcome::Existing(existing)
        } else {
            let binding = LocalDeviceBinding {
                binding_id: format!("ldb_{}", Uuid::new_v4().simple()),
                connector_id: request.connector_id.clone(),
                hardware_identity: request.hardware_identity.clone(),
                runtime_id: request.runtime_id.clone(),
                evidence_ref: request.evidence_ref.clone(),
                bound_at_ms: now_ms,
                revoked_at_ms: None,
            };
            transaction.execute(
                "INSERT INTO local_device_bindings
                 (binding_id, connector_id, hardware_identity, runtime_id,
                  evidence_ref, bound_at_ms, revoked_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
                params![
                    binding.binding_id,
                    binding.connector_id,
                    binding.hardware_identity,
                    binding.runtime_id,
                    binding.evidence_ref,
                    binding.bound_at_ms
                ],
            )?;
            BindOutcome::Created(binding)
        };
        transaction.commit()?;
        Ok(outcome)
    }

    pub fn revoke(
        &self,
        authority: &LocalAuthority,
        context: &LocalContext,
        authority_session_id: &str,
        binding_id: &str,
        now_ms: i64,
    ) -> Result<(), DeviceBindingError> {
        authority.authorize_local(
            context,
            authority_session_id,
            LocalCapability::BindLocalDevice,
            now_ms,
        )?;
        validate("binding_id", binding_id, MAX_ID)?;
        let connection = self.connection.lock().map_err(|_| {
            DeviceBindingError::Conflict("device-binding database lock poisoned".to_string())
        })?;
        let existing = get_binding(&connection, binding_id)?.ok_or(DeviceBindingError::NotFound)?;
        if existing.revoked_at_ms.is_some() {
            return Err(DeviceBindingError::Revoked);
        }
        let changed = connection.execute(
            "UPDATE local_device_bindings SET revoked_at_ms = ?1
             WHERE binding_id = ?2 AND revoked_at_ms IS NULL",
            params![now_ms, binding_id],
        )?;
        if changed != 1 {
            return Err(DeviceBindingError::Revoked);
        }
        Ok(())
    }

    pub fn require_consequential_match(
        &self,
        authority: &LocalAuthority,
        context: &LocalContext,
        authority_session_id: &str,
        binding_id: &str,
        current: &CurrentDeviceIdentity<'_>,
        now_ms: i64,
    ) -> Result<LocalDeviceBinding, DeviceBindingError> {
        authority.authorize_local(
            context,
            authority_session_id,
            LocalCapability::RequestConsequentialApproval,
            now_ms,
        )?;
        let connection = self.connection.lock().map_err(|_| {
            DeviceBindingError::Conflict("device-binding database lock poisoned".to_string())
        })?;
        let binding = get_binding(&connection, binding_id)?.ok_or(DeviceBindingError::NotFound)?;
        if binding.revoked_at_ms.is_some() {
            return Err(DeviceBindingError::Revoked);
        }
        if binding.connector_id != current.connector_id
            || binding.hardware_identity != current.hardware_identity
            || binding.runtime_id != current.runtime_id
        {
            return Err(DeviceBindingError::IdentityMismatch);
        }
        Ok(binding)
    }
}

fn validate_request(request: &BindRequest, now_ms: i64) -> Result<(), DeviceBindingError> {
    if now_ms < 0 {
        return Err(DeviceBindingError::InvalidInput {
            field: "now_ms",
            reason: "must be non-negative UTC epoch milliseconds".to_string(),
        });
    }
    validate("connector_id", &request.connector_id, MAX_ID)?;
    validate("hardware_identity", &request.hardware_identity, MAX_ID)?;
    validate("runtime_id", &request.runtime_id, MAX_ID)?;
    validate("evidence_ref", &request.evidence_ref, MAX_EVIDENCE)?;
    validate("request_nonce", &request.request_nonce, MAX_ID)
}

fn validate(field: &'static str, value: &str, max: usize) -> Result<(), DeviceBindingError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(DeviceBindingError::InvalidInput {
            field,
            reason: format!("must contain 1..={max} non-control bytes"),
        });
    }
    Ok(())
}

fn find_active(
    connection: &Connection,
    connector_id: &str,
    hardware_identity: &str,
) -> Result<Option<LocalDeviceBinding>, DeviceBindingError> {
    connection
        .query_row(
            "SELECT binding_id, connector_id, hardware_identity, runtime_id,
                    evidence_ref, bound_at_ms, revoked_at_ms
             FROM local_device_bindings
             WHERE connector_id = ?1 AND hardware_identity = ?2 AND revoked_at_ms IS NULL",
            params![connector_id, hardware_identity],
            row_to_binding,
        )
        .optional()
        .map_err(Into::into)
}

fn get_binding(
    connection: &Connection,
    binding_id: &str,
) -> Result<Option<LocalDeviceBinding>, DeviceBindingError> {
    connection
        .query_row(
            "SELECT binding_id, connector_id, hardware_identity, runtime_id,
                    evidence_ref, bound_at_ms, revoked_at_ms
             FROM local_device_bindings WHERE binding_id = ?1",
            [binding_id],
            row_to_binding,
        )
        .optional()
        .map_err(Into::into)
}

fn row_to_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalDeviceBinding> {
    Ok(LocalDeviceBinding {
        binding_id: row.get(0)?,
        connector_id: row.get(1)?,
        hardware_identity: row.get(2)?,
        runtime_id: row.get(3)?,
        evidence_ref: row.get(4)?,
        bound_at_ms: row.get(5)?,
        revoked_at_ms: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (LocalAuthority, LocalContext, String, LocalDeviceBindings) {
        let authority = LocalAuthority::open_in_memory().unwrap();
        let context = LocalContext::current("ipc-test").unwrap();
        authority.initialize_current(&context, 1_000).unwrap();
        let session = authority.issue_session(&context, 2_000, 60_000).unwrap();
        (
            authority,
            context,
            session.session_id,
            LocalDeviceBindings::open_in_memory().unwrap(),
        )
    }

    fn request(nonce: &str) -> BindRequest {
        BindRequest {
            connector_id: "esp32-s3".to_string(),
            hardware_identity: "chip-fingerprint-abc".to_string(),
            runtime_id: "runtime-local-1".to_string(),
            evidence_ref: "evidence://identity/abc".to_string(),
            request_nonce: nonce.to_string(),
        }
    }

    #[test]
    fn bind_is_authorized_and_replay_is_rejected() {
        let (authority, context, session, store) = setup();
        assert!(matches!(
            store.bind(&authority, &context, &session, &request("nonce-1"), 3_000),
            Ok(BindOutcome::Created(_))
        ));
        assert!(matches!(
            store.bind(&authority, &context, &session, &request("nonce-1"), 3_001),
            Err(DeviceBindingError::Replay)
        ));
    }

    #[test]
    fn same_identity_is_idempotent_with_new_nonce_but_cannot_be_overwritten() {
        let (authority, context, session, store) = setup();
        store
            .bind(&authority, &context, &session, &request("nonce-1"), 3_000)
            .unwrap();
        assert!(matches!(
            store.bind(&authority, &context, &session, &request("nonce-2"), 3_001),
            Ok(BindOutcome::Existing(_))
        ));
        let mut changed = request("nonce-3");
        changed.runtime_id = "runtime-other".to_string();
        assert!(matches!(
            store.bind(&authority, &context, &session, &changed, 3_002),
            Err(DeviceBindingError::Conflict(_))
        ));
    }

    #[test]
    fn eligibility_requires_exact_identity_and_active_binding() {
        let (authority, context, session, store) = setup();
        let binding = match store
            .bind(&authority, &context, &session, &request("nonce-1"), 3_000)
            .unwrap()
        {
            BindOutcome::Created(binding) => binding,
            BindOutcome::Existing(_) => unreachable!(),
        };
        assert!(
            store
                .require_consequential_match(
                    &authority,
                    &context,
                    &session,
                    &binding.binding_id,
                    &CurrentDeviceIdentity {
                        connector_id: "esp32-s3",
                        hardware_identity: "chip-fingerprint-abc",
                        runtime_id: "runtime-local-1",
                    },
                    3_001
                )
                .is_ok()
        );
        assert!(matches!(
            store.require_consequential_match(
                &authority,
                &context,
                &session,
                &binding.binding_id,
                &CurrentDeviceIdentity {
                    connector_id: "esp32-s3",
                    hardware_identity: "different-chip",
                    runtime_id: "runtime-local-1",
                },
                3_001
            ),
            Err(DeviceBindingError::IdentityMismatch)
        ));
        store
            .revoke(&authority, &context, &session, &binding.binding_id, 3_002)
            .unwrap();
        assert!(matches!(
            store.require_consequential_match(
                &authority,
                &context,
                &session,
                &binding.binding_id,
                &CurrentDeviceIdentity {
                    connector_id: "esp32-s3",
                    hardware_identity: "chip-fingerprint-abc",
                    runtime_id: "runtime-local-1",
                },
                3_003
            ),
            Err(DeviceBindingError::Revoked)
        ));
    }

    #[test]
    fn revoked_identity_can_be_rebound_only_with_a_fresh_nonce() {
        let (authority, context, session, store) = setup();
        let first = match store
            .bind(&authority, &context, &session, &request("nonce-1"), 3_000)
            .unwrap()
        {
            BindOutcome::Created(binding) => binding,
            BindOutcome::Existing(_) => unreachable!(),
        };
        store
            .revoke(&authority, &context, &session, &first.binding_id, 3_001)
            .unwrap();
        assert!(matches!(
            store.bind(&authority, &context, &session, &request("nonce-1"), 3_002),
            Err(DeviceBindingError::Replay)
        ));
        let second = store
            .bind(&authority, &context, &session, &request("nonce-2"), 3_003)
            .unwrap();
        assert!(matches!(second, BindOutcome::Created(_)));
    }

    #[test]
    fn expired_authority_session_cannot_bind() {
        let (authority, context, _, store) = setup();
        let short = authority.issue_session(&context, 10_000, 1_000).unwrap();
        assert!(matches!(
            store.bind(
                &authority,
                &context,
                &short.session_id,
                &request("nonce-1"),
                11_000
            ),
            Err(DeviceBindingError::Authority(
                AuthorityError::SessionExpired
            ))
        ));
    }

    #[test]
    fn restart_preserves_binding_and_revocation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("devices.sqlite3");
        let (authority, context, session, _) = setup();
        let binding_id;
        {
            let store = LocalDeviceBindings::open(&path).unwrap();
            binding_id = match store
                .bind(&authority, &context, &session, &request("nonce-1"), 3_000)
                .unwrap()
            {
                BindOutcome::Created(binding) => binding.binding_id,
                BindOutcome::Existing(_) => unreachable!(),
            };
            store
                .revoke(&authority, &context, &session, &binding_id, 3_001)
                .unwrap();
        }
        let reopened = LocalDeviceBindings::open(&path).unwrap();
        assert!(matches!(
            reopened.require_consequential_match(
                &authority,
                &context,
                &session,
                &binding_id,
                &CurrentDeviceIdentity {
                    connector_id: "esp32-s3",
                    hardware_identity: "chip-fingerprint-abc",
                    runtime_id: "runtime-local-1",
                },
                3_002
            ),
            Err(DeviceBindingError::Revoked)
        ));
    }
}
