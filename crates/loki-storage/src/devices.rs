//! Device registry repository (`Docs/SPEC.md` 3.3 Device). One of this
//! node's three required repositories — see `Docs/graph/tasks/DAT-001.md`
//! Deliverable 3.

use rusqlite::{OptionalExtension, Row, params};

use crate::error::StorageError;
use crate::ids::{ConnectorId, DeviceId, RuntimeId};
use crate::states::{DeviceLifecycleState, SupportTier};
use crate::storage::Storage;
use crate::timestamp::validate_utc_timestamp;

const SELECT_COLUMNS: &str = "device_id, connector_id, connector_version, hardware_identity, family, \
     board_identity, transport_reference, runtime_id, support_tier, lifecycle_state, \
     capabilities_json, firmware_artifact_ref, config_artifact_ref, model_artifact_ref, \
     health_json, last_seen_at, created_at, updated_at";

/// Fields required to register a new device row. `Docs/SPEC.md` 3.3's
/// "where verified" / "when observable" language is why several fields are
/// `Option`: `None` is an honest "not observed," never a fabricated value
/// (`AGENTS.md` rule 1).
#[derive(Debug, Clone)]
pub struct NewDevice {
    pub device_id: DeviceId,
    pub connector_id: ConnectorId,
    pub connector_version: String,
    /// `Docs/SPEC.md` 3.3 "connector-qualified hardware identity" — assumed
    /// already namespaced/qualified by the connector so it is comparable
    /// without this crate re-qualifying it; combined with `connector_id` in
    /// a uniqueness constraint (`migrations/0001_devices.sql`).
    pub hardware_identity: String,
    pub family: String,
    pub board_identity: Option<String>,
    pub transport_reference: Option<String>,
    pub runtime_id: RuntimeId,
    pub support_tier: SupportTier,
    pub lifecycle_state: DeviceLifecycleState,
    /// Opaque, already-serialized JSON — this crate does not own or
    /// validate the connector capability descriptor shape
    /// (`Docs/CONNECTOR_SPEC.md` / `CON-001`).
    pub capabilities_json: Option<String>,
    pub firmware_artifact_ref: Option<String>,
    pub config_artifact_ref: Option<String>,
    pub model_artifact_ref: Option<String>,
    /// Opaque, already-serialized JSON health projection.
    pub health_json: Option<String>,
    pub last_seen_at: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A persisted device row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRecord {
    pub device_id: DeviceId,
    pub connector_id: ConnectorId,
    pub connector_version: String,
    pub hardware_identity: String,
    pub family: String,
    pub board_identity: Option<String>,
    pub transport_reference: Option<String>,
    pub runtime_id: RuntimeId,
    pub support_tier: SupportTier,
    pub lifecycle_state: DeviceLifecycleState,
    pub capabilities_json: Option<String>,
    pub firmware_artifact_ref: Option<String>,
    pub config_artifact_ref: Option<String>,
    pub model_artifact_ref: Option<String>,
    pub health_json: Option<String>,
    pub last_seen_at: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Repository pattern over the device registry table.
pub trait DeviceRepository {
    fn insert_device(&self, device: &NewDevice) -> Result<(), StorageError>;
    fn get_device(&self, id: &DeviceId) -> Result<Option<DeviceRecord>, StorageError>;
    fn list_devices(&self) -> Result<Vec<DeviceRecord>, StorageError>;
    /// Updates lifecycle state and last-seen time together — the common
    /// real write pattern (a connector observation both confirms liveness
    /// and may change lifecycle state). Returns
    /// [`StorageError::NotFound`] if `id` does not exist.
    fn update_device_lifecycle_state(
        &self,
        id: &DeviceId,
        state: DeviceLifecycleState,
        last_seen_at: &str,
        updated_at: &str,
    ) -> Result<(), StorageError>;
}

impl DeviceRepository for Storage {
    fn insert_device(&self, device: &NewDevice) -> Result<(), StorageError> {
        validate_utc_timestamp("last_seen_at", &device.last_seen_at)?;
        validate_utc_timestamp("created_at", &device.created_at)?;
        validate_utc_timestamp("updated_at", &device.updated_at)?;

        let conn = self.lock();
        conn.execute(
            "INSERT INTO devices (
                device_id, connector_id, connector_version, hardware_identity, family,
                board_identity, transport_reference, runtime_id, support_tier,
                lifecycle_state, capabilities_json, firmware_artifact_ref,
                config_artifact_ref, model_artifact_ref, health_json, last_seen_at,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                device.device_id.as_str(),
                device.connector_id.as_str(),
                device.connector_version,
                device.hardware_identity,
                device.family,
                device.board_identity,
                device.transport_reference,
                device.runtime_id.as_str(),
                device.support_tier.as_str(),
                device.lifecycle_state.as_str(),
                device.capabilities_json,
                device.firmware_artifact_ref,
                device.config_artifact_ref,
                device.model_artifact_ref,
                device.health_json,
                device.last_seen_at,
                device.created_at,
                device.updated_at,
            ],
        )?;
        Ok(())
    }

    fn get_device(&self, id: &DeviceId) -> Result<Option<DeviceRecord>, StorageError> {
        let conn = self.lock();
        let raw: Option<RawDeviceRow> = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM devices WHERE device_id = ?1"),
                params![id.as_str()],
                row_to_raw,
            )
            .optional()?;
        raw.map(raw_to_record).transpose()
    }

    fn list_devices(&self) -> Result<Vec<DeviceRecord>, StorageError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SELECT_COLUMNS} FROM devices ORDER BY device_id"
        ))?;
        let rows = stmt.query_map([], row_to_raw)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(raw_to_record(row?)?);
        }
        Ok(out)
    }

    fn update_device_lifecycle_state(
        &self,
        id: &DeviceId,
        state: DeviceLifecycleState,
        last_seen_at: &str,
        updated_at: &str,
    ) -> Result<(), StorageError> {
        validate_utc_timestamp("last_seen_at", last_seen_at)?;
        validate_utc_timestamp("updated_at", updated_at)?;

        let conn = self.lock();
        let changed = conn.execute(
            "UPDATE devices SET lifecycle_state = ?1, last_seen_at = ?2, updated_at = ?3 WHERE device_id = ?4",
            params![state.as_str(), last_seen_at, updated_at, id.as_str()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound {
                kind: "device",
                id: id.as_str().to_string(),
            });
        }
        Ok(())
    }
}

/// Raw column values pulled straight out of a `devices` row, before domain
/// validation. Separated from [`DeviceRecord`] so a SQL-level extraction
/// failure (`rusqlite::Error`, e.g. an unexpected column type) and a
/// domain-level validation failure (`StorageError::InvalidId`/
/// `InvalidState` — a corrupted or hand-edited row) stay distinguishable.
struct RawDeviceRow {
    device_id: String,
    connector_id: String,
    connector_version: String,
    hardware_identity: String,
    family: String,
    board_identity: Option<String>,
    transport_reference: Option<String>,
    runtime_id: String,
    support_tier: String,
    lifecycle_state: String,
    capabilities_json: Option<String>,
    firmware_artifact_ref: Option<String>,
    config_artifact_ref: Option<String>,
    model_artifact_ref: Option<String>,
    health_json: Option<String>,
    last_seen_at: String,
    created_at: String,
    updated_at: String,
}

fn row_to_raw(row: &Row<'_>) -> rusqlite::Result<RawDeviceRow> {
    Ok(RawDeviceRow {
        device_id: row.get(0)?,
        connector_id: row.get(1)?,
        connector_version: row.get(2)?,
        hardware_identity: row.get(3)?,
        family: row.get(4)?,
        board_identity: row.get(5)?,
        transport_reference: row.get(6)?,
        runtime_id: row.get(7)?,
        support_tier: row.get(8)?,
        lifecycle_state: row.get(9)?,
        capabilities_json: row.get(10)?,
        firmware_artifact_ref: row.get(11)?,
        config_artifact_ref: row.get(12)?,
        model_artifact_ref: row.get(13)?,
        health_json: row.get(14)?,
        last_seen_at: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

fn raw_to_record(raw: RawDeviceRow) -> Result<DeviceRecord, StorageError> {
    Ok(DeviceRecord {
        device_id: DeviceId::new(raw.device_id)?,
        connector_id: ConnectorId::new(raw.connector_id)?,
        connector_version: raw.connector_version,
        hardware_identity: raw.hardware_identity,
        family: raw.family,
        board_identity: raw.board_identity,
        transport_reference: raw.transport_reference,
        runtime_id: RuntimeId::new(raw.runtime_id)?,
        support_tier: SupportTier::parse("support_tier", &raw.support_tier)?,
        lifecycle_state: DeviceLifecycleState::parse("lifecycle_state", &raw.lifecycle_state)?,
        capabilities_json: raw.capabilities_json,
        firmware_artifact_ref: raw.firmware_artifact_ref,
        config_artifact_ref: raw.config_artifact_ref,
        model_artifact_ref: raw.model_artifact_ref,
        health_json: raw.health_json,
        last_seen_at: raw.last_seen_at,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
    })
}
