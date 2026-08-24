//! Durable local notification intents (`LNOT-001`).
//!
//! This crate owns a local action-center projection. It does not send Windows
//! toasts, push messages, cloud events, or agent deliveries. `Presented` means
//! only that an authenticated local consumer explicitly confirmed accepting the
//! projection; it is not evidence of remote or OS delivery.

use std::{fmt, path::Path, sync::Mutex, time::SystemTime};

use loki_incidents::work_intent::WorkIntent;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

const MAX_ID: usize = 256;
const MAX_TITLE: usize = 120;
const MAX_BODY: usize = 512;
const DEFAULT_LIFETIME_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationCategory {
    Incident,
    RuntimeOffline,
}

impl NotificationCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::Incident => "incident",
            Self::RuntimeOffline => "runtime_offline",
        }
    }

    fn parse(value: &str) -> Result<Self, NotificationError> {
        match value {
            "incident" => Ok(Self::Incident),
            "runtime_offline" => Ok(Self::RuntimeOffline),
            _ => Err(NotificationError::Corrupt("unknown category")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    fn parse(value: &str) -> Result<Self, NotificationError> {
        match value {
            "info" => Ok(Self::Info),
            "warning" => Ok(Self::Warning),
            "critical" => Ok(Self::Critical),
            _ => Err(NotificationError::Corrupt("unknown severity")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationState {
    Queued,
    Suppressed,
    Presented,
    Expired,
}

impl PresentationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Suppressed => "suppressed",
            Self::Presented => "presented",
            Self::Expired => "expired",
        }
    }

    fn parse(value: &str) -> Result<Self, NotificationError> {
        match value {
            "queued" => Ok(Self::Queued),
            "suppressed" => Ok(Self::Suppressed),
            "presented" => Ok(Self::Presented),
            "expired" => Ok(Self::Expired),
            _ => Err(NotificationError::Corrupt("unknown presentation state")),
        }
    }
}

/// Closed local actions. No URL, path, command, channel, or arbitrary IPC
/// method can be encoded in this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalActionRef {
    IncidentDetails { incident_id: String },
    RuntimeDiagnostics { runtime_id: String },
}

impl LocalActionRef {
    fn encode(&self) -> (&'static str, &str) {
        match self {
            Self::IncidentDetails { incident_id } => ("incident_details", incident_id),
            Self::RuntimeDiagnostics { runtime_id } => ("runtime_diagnostics", runtime_id),
        }
    }

    fn parse(kind: &str, target: String) -> Result<Self, NotificationError> {
        validate_identifier("action_target", &target)?;
        match kind {
            "incident_details" => Ok(Self::IncidentDetails {
                incident_id: target,
            }),
            "runtime_diagnostics" => Ok(Self::RuntimeDiagnostics { runtime_id: target }),
            _ => Err(NotificationError::Corrupt("unknown local action kind")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalNotification {
    pub notification_id: String,
    pub category: NotificationCategory,
    pub severity: Severity,
    pub deduplication_key: String,
    pub source_id: String,
    pub observer_id: Option<String>,
    pub last_seen_at_ms: Option<i64>,
    pub state: PresentationState,
    pub title: String,
    pub body: String,
    pub action: LocalActionRef,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub presented_at_ms: Option<i64>,
    pub read_at_ms: Option<i64>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPreference {
    pub category: NotificationCategory,
    pub enabled: bool,
    pub minimum_severity: Severity,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOfflineObservation {
    pub runtime_id: String,
    pub observer_id: String,
    /// Stable identifier for one offline episode; retries reuse it.
    pub episode_id: String,
    pub last_seen_at_ms: i64,
    pub detected_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug)]
pub enum NotificationError {
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

impl fmt::Display for NotificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Sqlite(_) => "local notification database error",
            Self::Storage(_) => "encrypted local notification database open failed",
            Self::InvalidInput { .. } => "invalid local notification input",
            Self::NotFound => "local notification not found",
            Self::Conflict => "local notification revision conflict",
            Self::InvalidTransition => "invalid local notification transition",
            Self::Expired => "local notification expired",
            Self::Corrupt(_) => "corrupt local notification record",
            Self::Time => "local notification timestamp is not representable",
            Self::Concurrency => "local notification store concurrency failure",
        };
        f.write_str(message)
    }
}

impl std::error::Error for NotificationError {}
impl From<rusqlite::Error> for NotificationError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}
impl From<loki_storage::StorageError> for NotificationError {
    fn from(value: loki_storage::StorageError) -> Self {
        Self::Storage(value)
    }
}

pub struct LocalNotificationStore {
    connection: Mutex<Connection>,
}

impl LocalNotificationStore {
    pub fn open(
        path: impl AsRef<Path>,
        protected_key_path: impl AsRef<Path>,
    ) -> Result<Self, NotificationError> {
        let (connection, _key) =
            loki_storage::open_or_migrate_sqlcipher(path.as_ref(), protected_key_path.as_ref())?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self, NotificationError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, NotificationError> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS local_notification_preferences (
               category TEXT PRIMARY KEY,
               enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
               minimum_severity TEXT NOT NULL,
               revision INTEGER NOT NULL CHECK (revision >= 1)
             );
             CREATE TABLE IF NOT EXISTS local_notifications (
               notification_id TEXT PRIMARY KEY,
               category TEXT NOT NULL,
               severity TEXT NOT NULL,
               deduplication_key TEXT NOT NULL UNIQUE,
               source_id TEXT NOT NULL,
               observer_id TEXT,
               last_seen_at_ms INTEGER,
               state TEXT NOT NULL,
               title TEXT NOT NULL,
               body TEXT NOT NULL,
               action_kind TEXT NOT NULL,
               action_target TEXT NOT NULL,
               created_at_ms INTEGER NOT NULL,
               expires_at_ms INTEGER NOT NULL,
               presented_at_ms INTEGER,
               read_at_ms INTEGER,
               revision INTEGER NOT NULL CHECK (revision >= 1),
               CHECK (expires_at_ms > created_at_ms)
             );",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn set_preference(
        &self,
        category: NotificationCategory,
        enabled: bool,
        minimum_severity: Severity,
        expected_revision: Option<u64>,
    ) -> Result<NotificationPreference, NotificationError> {
        let expected_revision = expected_revision.map(revision_to_i64).transpose()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<i64> = transaction
            .query_row(
                "SELECT revision FROM local_notification_preferences WHERE category = ?1",
                [category.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        match (current, expected_revision) {
            (None, None) => {
                transaction.execute(
                    "INSERT INTO local_notification_preferences
                     (category, enabled, minimum_severity, revision) VALUES (?1, ?2, ?3, 1)",
                    params![category.as_str(), enabled, minimum_severity.as_str()],
                )?;
            }
            (Some(current), Some(expected)) if current == expected => {
                transaction.execute(
                    "UPDATE local_notification_preferences
                     SET enabled = ?2, minimum_severity = ?3, revision = revision + 1
                     WHERE category = ?1",
                    params![category.as_str(), enabled, minimum_severity.as_str()],
                )?;
            }
            _ => return Err(NotificationError::Conflict),
        }
        transaction.commit()?;
        drop(connection);
        self.preference(category)
    }

    pub fn preference(
        &self,
        category: NotificationCategory,
    ) -> Result<NotificationPreference, NotificationError> {
        let connection = self.lock()?;
        preference_on(&connection, category)
    }

    pub fn enqueue_work_intent(
        &self,
        intent: &WorkIntent,
        now_ms: i64,
    ) -> Result<LocalNotification, NotificationError> {
        validate_now(now_ms)?;
        let created_at_ms = system_time_ms(intent.created_at)?;
        let expires_at_ms = system_time_ms(intent.expiry)?;
        if expires_at_ms <= created_at_ms {
            return Err(NotificationError::InvalidInput {
                field: "work_intent.expiry",
                reason: "must be after creation".to_string(),
            });
        }
        let incident_id = intent.incident_id.as_str().to_string();
        let work_intent_id = intent.work_intent_id.as_str().to_string();
        self.enqueue(NewNotification {
            category: NotificationCategory::Incident,
            severity: Severity::Warning,
            deduplication_key: format!("work-intent:{work_intent_id}"),
            source_id: work_intent_id,
            observer_id: None,
            last_seen_at_ms: None,
            title: "Incident requires attention",
            body: "A local incident work intent is waiting for review.",
            action: LocalActionRef::IncidentDetails { incident_id },
            created_at_ms,
            expires_at_ms,
            now_ms,
        })
    }

    pub fn enqueue_runtime_offline(
        &self,
        observation: &RuntimeOfflineObservation,
        now_ms: i64,
    ) -> Result<LocalNotification, NotificationError> {
        observation.validate(now_ms)?;
        self.enqueue(NewNotification {
            category: NotificationCategory::RuntimeOffline,
            severity: Severity::Warning,
            deduplication_key: format!(
                "runtime-offline:{}:{}",
                observation.runtime_id, observation.episode_id
            ),
            source_id: observation.episode_id.clone(),
            observer_id: Some(observation.observer_id.clone()),
            last_seen_at_ms: Some(observation.last_seen_at_ms),
            title: "Local runtime is offline",
            body: "LokiAI observed that a local runtime stopped responding.",
            action: LocalActionRef::RuntimeDiagnostics {
                runtime_id: observation.runtime_id.clone(),
            },
            created_at_ms: observation.detected_at_ms,
            expires_at_ms: observation.expires_at_ms,
            now_ms,
        })
    }

    fn enqueue(&self, new: NewNotification<'_>) -> Result<LocalNotification, NotificationError> {
        new.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = get_by_dedup(&transaction, &new.deduplication_key)? {
            if !new.matches_existing(&existing) {
                return Err(NotificationError::Conflict);
            }
            transaction.commit()?;
            return Ok(existing);
        }
        let preference = preference_on(&transaction, new.category)?;
        let state = if new.expires_at_ms <= new.now_ms {
            PresentationState::Expired
        } else if !preference.enabled || new.severity < preference.minimum_severity {
            PresentationState::Suppressed
        } else {
            PresentationState::Queued
        };
        let notification_id = Uuid::new_v4().to_string();
        let (action_kind, action_target) = new.action.encode();
        transaction.execute(
            "INSERT INTO local_notifications
             (notification_id, category, severity, deduplication_key, source_id,
              observer_id, last_seen_at_ms, state, title, body, action_kind,
              action_target, created_at_ms, expires_at_ms, revision)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 1)",
            params![
                notification_id,
                new.category.as_str(),
                new.severity.as_str(),
                new.deduplication_key,
                new.source_id,
                new.observer_id,
                new.last_seen_at_ms,
                state.as_str(),
                new.title,
                new.body,
                action_kind,
                action_target,
                new.created_at_ms,
                new.expires_at_ms,
            ],
        )?;
        transaction.commit()?;
        drop(connection);
        self.get(&notification_id)
    }

    pub fn mark_presented(
        &self,
        notification_id: &str,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<LocalNotification, NotificationError> {
        self.transition(
            notification_id,
            expected_revision,
            now_ms,
            PresentationState::Queued,
            "UPDATE local_notifications SET state = 'presented', presented_at_ms = ?3,
             revision = revision + 1 WHERE notification_id = ?1 AND revision = ?2",
        )
    }

    pub fn mark_read(
        &self,
        notification_id: &str,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<LocalNotification, NotificationError> {
        validate_now(now_ms)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = get_on(&transaction, notification_id)?.ok_or(NotificationError::NotFound)?;
        if current.revision != expected_revision {
            return Err(NotificationError::Conflict);
        }
        if current.state != PresentationState::Presented || current.read_at_ms.is_some() {
            return Err(NotificationError::InvalidTransition);
        }
        if now_ms >= current.expires_at_ms {
            return Err(NotificationError::Expired);
        }
        let expected_revision = revision_to_i64(expected_revision)?;
        transaction.execute(
            "UPDATE local_notifications SET read_at_ms = ?3, revision = revision + 1
             WHERE notification_id = ?1 AND revision = ?2",
            params![notification_id, expected_revision, now_ms],
        )?;
        transaction.commit()?;
        drop(connection);
        self.get(notification_id)
    }

    fn transition(
        &self,
        notification_id: &str,
        expected_revision: u64,
        now_ms: i64,
        required: PresentationState,
        sql: &str,
    ) -> Result<LocalNotification, NotificationError> {
        validate("notification_id", notification_id, MAX_ID)?;
        validate_now(now_ms)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = get_on(&transaction, notification_id)?.ok_or(NotificationError::NotFound)?;
        if current.revision != expected_revision {
            return Err(NotificationError::Conflict);
        }
        if current.state != required {
            return Err(NotificationError::InvalidTransition);
        }
        if now_ms >= current.expires_at_ms {
            return Err(NotificationError::Expired);
        }
        let expected_revision = revision_to_i64(expected_revision)?;
        transaction.execute(sql, params![notification_id, expected_revision, now_ms])?;
        transaction.commit()?;
        drop(connection);
        self.get(notification_id)
    }

    pub fn expire_due(&self, now_ms: i64) -> Result<usize, NotificationError> {
        validate_now(now_ms)?;
        let connection = self.lock()?;
        Ok(connection.execute(
            "UPDATE local_notifications SET state = 'expired', revision = revision + 1
             WHERE expires_at_ms <= ?1 AND state != 'expired'",
            [now_ms],
        )?)
    }

    pub fn get(&self, notification_id: &str) -> Result<LocalNotification, NotificationError> {
        validate("notification_id", notification_id, MAX_ID)?;
        let connection = self.lock()?;
        get_on(&connection, notification_id)?.ok_or(NotificationError::NotFound)
    }

    pub fn list(&self) -> Result<Vec<LocalNotification>, NotificationError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT notification_id, category, severity, deduplication_key,
                    source_id, observer_id, last_seen_at_ms, state, title, body,
                    action_kind, action_target, created_at_ms, expires_at_ms,
                    presented_at_ms, read_at_ms, revision
             FROM local_notifications ORDER BY created_at_ms, notification_id",
        )?;
        let rows = statement.query_map([], row_to_notification)?;
        let mut notifications = Vec::new();
        for row in rows {
            notifications.push(row??);
        }
        Ok(notifications)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, NotificationError> {
        self.connection
            .lock()
            .map_err(|_| NotificationError::Concurrency)
    }
}

struct NewNotification<'a> {
    category: NotificationCategory,
    severity: Severity,
    deduplication_key: String,
    source_id: String,
    observer_id: Option<String>,
    last_seen_at_ms: Option<i64>,
    title: &'a str,
    body: &'a str,
    action: LocalActionRef,
    created_at_ms: i64,
    expires_at_ms: i64,
    now_ms: i64,
}

impl NewNotification<'_> {
    fn validate(&self) -> Result<(), NotificationError> {
        validate("deduplication_key", &self.deduplication_key, MAX_ID)?;
        validate_identifier("source_id", &self.source_id)?;
        if let Some(observer_id) = &self.observer_id {
            validate_identifier("observer_id", observer_id)?;
        }
        validate("title", self.title, MAX_TITLE)?;
        validate("body", self.body, MAX_BODY)?;
        let (_, target) = self.action.encode();
        validate_identifier("action_target", target)?;
        validate_now(self.created_at_ms)?;
        validate_now(self.now_ms)?;
        if self.created_at_ms > self.now_ms {
            return Err(NotificationError::InvalidInput {
                field: "created_at_ms",
                reason: "must not be in the future".to_string(),
            });
        }
        if self.expires_at_ms <= self.created_at_ms {
            return Err(NotificationError::InvalidInput {
                field: "expires_at_ms",
                reason: "must be after created_at_ms".to_string(),
            });
        }
        Ok(())
    }

    fn matches_existing(&self, existing: &LocalNotification) -> bool {
        let (action_kind, action_target) = self.action.encode();
        let (existing_action_kind, existing_action_target) = existing.action.encode();
        existing.category == self.category
            && existing.severity == self.severity
            && existing.source_id == self.source_id
            && existing.observer_id == self.observer_id
            && existing.last_seen_at_ms == self.last_seen_at_ms
            && existing.title == self.title
            && existing.body == self.body
            && action_kind == existing_action_kind
            && action_target == existing_action_target
            && existing.created_at_ms == self.created_at_ms
            && existing.expires_at_ms == self.expires_at_ms
    }
}

impl RuntimeOfflineObservation {
    fn validate(&self, now_ms: i64) -> Result<(), NotificationError> {
        validate_identifier("runtime_id", &self.runtime_id)?;
        validate_identifier("observer_id", &self.observer_id)?;
        validate_identifier("episode_id", &self.episode_id)?;
        validate_now(now_ms)?;
        validate_now(self.last_seen_at_ms)?;
        validate_now(self.detected_at_ms)?;
        if self.last_seen_at_ms > self.detected_at_ms || self.detected_at_ms > now_ms {
            return Err(NotificationError::InvalidInput {
                field: "offline_observation",
                reason: "last_seen <= detected <= now is required".to_string(),
            });
        }
        if self.expires_at_ms <= self.detected_at_ms
            || self.expires_at_ms - self.detected_at_ms > DEFAULT_LIFETIME_MS
        {
            return Err(NotificationError::InvalidInput {
                field: "expires_at_ms",
                reason: "must be after detection and within seven days".to_string(),
            });
        }
        Ok(())
    }
}

fn preference_on(
    connection: &Connection,
    category: NotificationCategory,
) -> Result<NotificationPreference, NotificationError> {
    connection
        .query_row(
            "SELECT enabled, minimum_severity, revision
             FROM local_notification_preferences WHERE category = ?1",
            [category.as_str()],
            |row| {
                Ok((
                    row.get::<_, bool>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?
        .map(|(enabled, severity, revision)| {
            Ok(NotificationPreference {
                category,
                enabled,
                minimum_severity: Severity::parse(&severity)?,
                revision: revision_from_i64(revision)?,
            })
        })
        .unwrap_or_else(|| {
            Ok(NotificationPreference {
                category,
                enabled: true,
                minimum_severity: Severity::Info,
                revision: 0,
            })
        })
}

fn get_on(
    connection: &Connection,
    notification_id: &str,
) -> Result<Option<LocalNotification>, NotificationError> {
    let row = connection
        .query_row(
            "SELECT notification_id, category, severity, deduplication_key,
                    source_id, observer_id, last_seen_at_ms, state, title, body,
                    action_kind, action_target, created_at_ms, expires_at_ms,
                    presented_at_ms, read_at_ms, revision
             FROM local_notifications WHERE notification_id = ?1",
            [notification_id],
            row_to_notification,
        )
        .optional()?;
    row.transpose()
}

fn get_by_dedup(
    connection: &Connection,
    deduplication_key: &str,
) -> Result<Option<LocalNotification>, NotificationError> {
    let row = connection
        .query_row(
            "SELECT notification_id, category, severity, deduplication_key,
                    source_id, observer_id, last_seen_at_ms, state, title, body,
                    action_kind, action_target, created_at_ms, expires_at_ms,
                    presented_at_ms, read_at_ms, revision
             FROM local_notifications WHERE deduplication_key = ?1",
            [deduplication_key],
            row_to_notification,
        )
        .optional()?;
    row.transpose()
}

fn row_to_notification(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Result<LocalNotification, NotificationError>> {
    let category: String = row.get(1)?;
    let severity: String = row.get(2)?;
    let state: String = row.get(7)?;
    let action_kind: String = row.get(10)?;
    let action_target: String = row.get(11)?;
    Ok((|| {
        Ok(LocalNotification {
            notification_id: row.get(0)?,
            category: NotificationCategory::parse(&category)?,
            severity: Severity::parse(&severity)?,
            deduplication_key: row.get(3)?,
            source_id: row.get(4)?,
            observer_id: row.get(5)?,
            last_seen_at_ms: row.get(6)?,
            state: PresentationState::parse(&state)?,
            title: row.get(8)?,
            body: row.get(9)?,
            action: LocalActionRef::parse(&action_kind, action_target)?,
            created_at_ms: row.get(12)?,
            expires_at_ms: row.get(13)?,
            presented_at_ms: row.get(14)?,
            read_at_ms: row.get(15)?,
            revision: revision_from_i64(row.get(16)?)?,
        })
    })())
}

fn system_time_ms(value: SystemTime) -> Result<i64, NotificationError> {
    let millis = value
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| NotificationError::Time)?
        .as_millis();
    i64::try_from(millis).map_err(|_| NotificationError::Time)
}

fn revision_to_i64(value: u64) -> Result<i64, NotificationError> {
    i64::try_from(value).map_err(|_| NotificationError::InvalidInput {
        field: "revision",
        reason: "must fit a signed 64-bit database integer".to_string(),
    })
}

fn revision_from_i64(value: i64) -> Result<u64, NotificationError> {
    u64::try_from(value).map_err(|_| NotificationError::Corrupt("negative revision"))
}

fn validate_now(value: i64) -> Result<(), NotificationError> {
    if value < 0 {
        return Err(NotificationError::InvalidInput {
            field: "timestamp",
            reason: "must be non-negative".to_string(),
        });
    }
    Ok(())
}

fn validate(field: &'static str, value: &str, max: usize) -> Result<(), NotificationError> {
    if value.is_empty()
        || value.len() > max
        || value.chars().any(|character| character.is_control())
    {
        return Err(NotificationError::InvalidInput {
            field,
            reason: format!("must contain 1..={max} non-control bytes"),
        });
    }
    Ok(())
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), NotificationError> {
    validate(field, value, MAX_ID)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(NotificationError::InvalidInput {
            field,
            reason: "must be a stable identifier, not a URL or path".to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_connectors_sdk::ids::CapabilityId;
    use loki_incidents::{
        ids::IncidentId,
        work_intent::{DeliveryRequirements, WorkIntent},
    };
    use std::{sync::Arc, time::Duration};

    fn time(ms: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_millis(ms)
    }

    fn intent() -> WorkIntent {
        WorkIntent::emit(
            IncidentId::generate(),
            CapabilityId::new("read_health").unwrap(),
            &[],
            DeliveryRequirements::new("untrusted requirement text must not be displayed").unwrap(),
            time(10_000),
            time(1_000),
        )
    }

    fn offline() -> RuntimeOfflineObservation {
        RuntimeOfflineObservation {
            runtime_id: "runtime-local-1".to_string(),
            observer_id: "desktop-main-1".to_string(),
            episode_id: "episode-1".to_string(),
            last_seen_at_ms: 1_000,
            detected_at_ms: 2_000,
            expires_at_ms: 20_000,
        }
    }

    #[test]
    fn work_intent_projection_is_redacted_and_grants_no_authority() {
        let store = LocalNotificationStore::open_in_memory().unwrap();
        let intent = intent();
        let notification = store.enqueue_work_intent(&intent, 2_000).unwrap();
        assert_eq!(notification.state, PresentationState::Queued);
        assert_eq!(notification.source_id, intent.work_intent_id.as_str());
        assert!(!notification.body.contains("untrusted requirement"));
        assert!(!notification.body.contains("read_health"));
        assert_eq!(
            notification.action,
            LocalActionRef::IncidentDetails {
                incident_id: intent.incident_id.as_str().to_string()
            }
        );
    }

    #[test]
    fn concurrent_retry_deduplicates_to_one_record() {
        let store = Arc::new(LocalNotificationStore::open_in_memory().unwrap());
        let intent = Arc::new(intent());
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let intent = Arc::clone(&intent);
                std::thread::spawn(move || store.enqueue_work_intent(&intent, 2_000).unwrap())
            })
            .collect();
        let ids: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().notification_id)
            .collect();
        assert!(ids.iter().all(|id| id == &ids[0]));
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn dedup_collision_with_mutated_payload_is_rejected() {
        let store = LocalNotificationStore::open_in_memory().unwrap();
        let original = intent();
        store.enqueue_work_intent(&original, 2_000).unwrap();
        let mut mutated = original;
        mutated.incident_id = IncidentId::generate();
        assert!(matches!(
            store.enqueue_work_intent(&mutated, 2_000),
            Err(NotificationError::Conflict)
        ));
    }

    #[test]
    fn preference_suppresses_without_deleting_intent() {
        let store = LocalNotificationStore::open_in_memory().unwrap();
        store
            .set_preference(NotificationCategory::Incident, false, Severity::Info, None)
            .unwrap();
        let notification = store.enqueue_work_intent(&intent(), 2_000).unwrap();
        assert_eq!(notification.state, PresentationState::Suppressed);
        assert!(matches!(
            store.mark_presented(&notification.notification_id, 1, 2_100),
            Err(NotificationError::InvalidTransition)
        ));
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn presentation_read_revision_and_expiry_are_truthful() {
        let store = LocalNotificationStore::open_in_memory().unwrap();
        let notification = store.enqueue_work_intent(&intent(), 2_000).unwrap();
        assert!(matches!(
            store.mark_read(&notification.notification_id, 1, 2_100),
            Err(NotificationError::InvalidTransition)
        ));
        let presented = store
            .mark_presented(&notification.notification_id, 1, 2_100)
            .unwrap();
        assert_eq!(presented.state, PresentationState::Presented);
        assert!(matches!(
            store.mark_read(&presented.notification_id, 1, 2_200),
            Err(NotificationError::Conflict)
        ));
        let read = store
            .mark_read(&presented.notification_id, presented.revision, 2_200)
            .unwrap();
        assert_eq!(read.read_at_ms, Some(2_200));
        assert_eq!(store.expire_due(10_000).unwrap(), 1);
        assert_eq!(
            store.get(&read.notification_id).unwrap().state,
            PresentationState::Expired
        );
    }

    #[test]
    fn offline_requires_real_ordered_observation_and_deduplicates_episode() {
        let store = LocalNotificationStore::open_in_memory().unwrap();
        let observation = offline();
        let first = store.enqueue_runtime_offline(&observation, 2_000).unwrap();
        let second = store.enqueue_runtime_offline(&observation, 2_001).unwrap();
        assert_eq!(first.notification_id, second.notification_id);
        assert_eq!(
            first.action,
            LocalActionRef::RuntimeDiagnostics {
                runtime_id: "runtime-local-1".to_string()
            }
        );
        assert_eq!(first.observer_id.as_deref(), Some("desktop-main-1"));
        assert_eq!(first.last_seen_at_ms, Some(1_000));
        let mut invalid = observation;
        invalid.last_seen_at_ms = 3_000;
        assert!(matches!(
            store.enqueue_runtime_offline(&invalid, 3_000),
            Err(NotificationError::InvalidInput { .. })
        ));
        let mut unsafe_target = offline();
        unsafe_target.runtime_id = "https://attacker.invalid".to_string();
        assert!(matches!(
            store.enqueue_runtime_offline(&unsafe_target, 2_000),
            Err(NotificationError::InvalidInput { .. })
        ));
    }

    #[test]
    fn encrypted_restart_preserves_state_without_plaintext_markers() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("notifications.sqlite3");
        let key = directory.path().join("notifications.key.dpapi");
        let notification_id;
        {
            let store = LocalNotificationStore::open(&database, &key).unwrap();
            let queued = store.enqueue_runtime_offline(&offline(), 2_000).unwrap();
            notification_id = queued.notification_id.clone();
            store
                .mark_presented(&notification_id, queued.revision, 2_100)
                .unwrap();
        }
        let raw = std::fs::read(&database).unwrap();
        assert!(
            !raw.windows(b"local_notifications".len())
                .any(|window| window == b"local_notifications")
        );
        assert!(
            !raw.windows(b"runtime-local-1".len())
                .any(|window| window == b"runtime-local-1")
        );
        let reopened = LocalNotificationStore::open(&database, &key).unwrap();
        assert_eq!(
            reopened.get(&notification_id).unwrap().state,
            PresentationState::Presented
        );
    }
}
