//! Bounded, device-family-neutral read projections over runtime-owned storage.

use loki_storage::devices::{DeviceRecord, DeviceRepository};
use loki_storage::ids::{ConnectorId, DeviceId, RuntimeId};
use loki_storage::states::{DeviceLifecycleState, SupportTier};
use loki_storage::{IntegrityStatus, Storage, StorageError};

use super::MutationAvailability;

/// Maximum persisted health document accepted at the service boundary.
/// Storage predates a canonical typed health document, so the runtime bounds
/// the opaque JSON rather than allowing an unbounded projection through.
const MAX_HEALTH_JSON_BYTES: usize = 64 * 1024;
const MAX_DEVICE_PAGE_SIZE: usize = 100;

/// Storage component readiness, not a claim about connector or device health.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageReadiness {
    Ready,
    Corrupt { problem_count: usize },
}

/// Honest runtime-owned status available without probing hardware.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStatus {
    pub storage: StorageReadiness,
    pub registered_device_count: usize,
    pub mutations: MutationAvailability,
}

/// Bounded device projection. Sensitive hardware identity and transport
/// references are deliberately excluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSummary {
    pub device_id: DeviceId,
    pub connector_id: ConnectorId,
    pub connector_version: String,
    pub family: String,
    pub board_identity: Option<String>,
    pub runtime_id: RuntimeId,
    pub support_tier: SupportTier,
    pub lifecycle_state: DeviceLifecycleState,
    pub last_seen_at: String,
}

/// Validated, stable-key pagination for the persisted device registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePageRequest {
    limit: usize,
    after_device_id: Option<DeviceId>,
}

impl DevicePageRequest {
    pub fn new(limit: usize, after_device_id: Option<DeviceId>) -> Result<Self, RuntimeReadError> {
        if !(1..=MAX_DEVICE_PAGE_SIZE).contains(&limit) {
            return Err(RuntimeReadError::InvalidPageLimit {
                supplied: limit,
                maximum: MAX_DEVICE_PAGE_SIZE,
            });
        }
        Ok(Self {
            limit,
            after_device_id,
        })
    }
}

/// One bounded page. `next_after_device_id` is present only when another
/// page exists; `total_registered_count` is registry metadata, not a claim
/// that every device is currently connected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePage {
    pub devices: Vec<DeviceSummary>,
    pub next_after_device_id: Option<DeviceId>,
    pub total_registered_count: usize,
}

/// Persisted connector health evidence. `document` is intentionally JSON
/// because storage's accepted contract declares this field opaque; the service
/// validates and bounds it but does not invent a schema or call it live data.
#[derive(Debug, Clone, PartialEq)]
pub struct PersistedHealthEvidence {
    /// Size of the validated persisted document. The opaque connector JSON is
    /// deliberately not projected through this application-service boundary.
    pub payload_size_bytes: usize,
    pub last_seen_at: String,
}

/// Whether the runtime has persisted any health evidence for the device.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceHealthState {
    NotObserved,
    Persisted(PersistedHealthEvidence),
}

/// Device health read projection. Lifecycle is storage state; health evidence
/// remains explicitly persisted rather than being represented as a live probe.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceHealthProjection {
    pub device_id: DeviceId,
    pub lifecycle_state: DeviceLifecycleState,
    pub health: DeviceHealthState,
}

#[derive(Debug)]
pub enum RuntimeReadError {
    Storage(StorageError),
    DeviceNotFound {
        device_id: String,
    },
    InvalidHealthProjection {
        device_id: String,
        reason: &'static str,
    },
    InvalidPageLimit {
        supplied: usize,
        maximum: usize,
    },
}

impl std::fmt::Display for RuntimeReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "runtime read failed: {error}"),
            Self::DeviceNotFound { device_id } => {
                write!(f, "device not found: {device_id}")
            }
            Self::InvalidHealthProjection { device_id, reason } => {
                write!(
                    f,
                    "invalid persisted health for device {device_id}: {reason}"
                )
            }
            Self::InvalidPageLimit { supplied, maximum } => write!(
                f,
                "invalid device page limit {supplied}: expected 1..={maximum}"
            ),
        }
    }
}

impl std::error::Error for RuntimeReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::DeviceNotFound { .. }
            | Self::InvalidHealthProjection { .. }
            | Self::InvalidPageLimit { .. } => None,
        }
    }
}

impl From<StorageError> for RuntimeReadError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

/// Read-only internal service port consumed by future authenticated adapters.
pub trait RuntimeReadPort {
    fn runtime_status(&self) -> Result<RuntimeStatus, RuntimeReadError>;
    fn list_devices(&self, request: DevicePageRequest) -> Result<DevicePage, RuntimeReadError>;
    fn device_health(
        &self,
        device_id: &DeviceId,
    ) -> Result<DeviceHealthProjection, RuntimeReadError>;
}

pub(super) fn runtime_status(
    storage: &Storage,
    mutations: MutationAvailability,
) -> Result<RuntimeStatus, RuntimeReadError> {
    let storage_status = match storage.integrity_check()? {
        IntegrityStatus::Ok => StorageReadiness::Ready,
        IntegrityStatus::Corrupt(problems) => StorageReadiness::Corrupt {
            problem_count: problems.len(),
        },
    };
    let registered_device_count = storage.list_devices()?.len();
    Ok(RuntimeStatus {
        storage: storage_status,
        registered_device_count,
        mutations,
    })
}

pub(super) fn list_devices(
    storage: &Storage,
    request: DevicePageRequest,
) -> Result<DevicePage, RuntimeReadError> {
    let records = storage.list_devices()?;
    let total_registered_count = records.len();
    let start = match request.after_device_id.as_ref() {
        Some(after) => {
            records.partition_point(|record| record.device_id.as_str() <= after.as_str())
        }
        None => 0,
    };
    let mut devices = records
        .into_iter()
        .skip(start)
        .take(request.limit + 1)
        .map(DeviceSummary::from)
        .collect::<Vec<_>>();
    let has_more = devices.len() > request.limit;
    if has_more {
        devices.pop();
    }
    let next_after_device_id = has_more
        .then(|| devices.last().map(|device| device.device_id.clone()))
        .flatten();
    Ok(DevicePage {
        devices,
        next_after_device_id,
        total_registered_count,
    })
}

pub(super) fn device_health(
    storage: &Storage,
    device_id: &DeviceId,
) -> Result<DeviceHealthProjection, RuntimeReadError> {
    let record =
        storage
            .get_device(device_id)?
            .ok_or_else(|| RuntimeReadError::DeviceNotFound {
                device_id: device_id.as_str().to_owned(),
            })?;
    let health = match record.health_json {
        None => DeviceHealthState::NotObserved,
        Some(document) => {
            if document.len() > MAX_HEALTH_JSON_BYTES {
                return Err(RuntimeReadError::InvalidHealthProjection {
                    device_id: device_id.as_str().to_owned(),
                    reason: "document exceeds service limit",
                });
            }
            let _: serde_json::Value = serde_json::from_str(&document).map_err(|_| {
                RuntimeReadError::InvalidHealthProjection {
                    device_id: device_id.as_str().to_owned(),
                    reason: "document is not valid JSON",
                }
            })?;
            DeviceHealthState::Persisted(PersistedHealthEvidence {
                payload_size_bytes: document.len(),
                last_seen_at: record.last_seen_at,
            })
        }
    };
    Ok(DeviceHealthProjection {
        device_id: record.device_id,
        lifecycle_state: record.lifecycle_state,
        health,
    })
}

impl From<DeviceRecord> for DeviceSummary {
    fn from(record: DeviceRecord) -> Self {
        Self {
            device_id: record.device_id,
            connector_id: record.connector_id,
            connector_version: record.connector_version,
            family: record.family,
            board_identity: record.board_identity,
            runtime_id: record.runtime_id,
            support_tier: record.support_tier,
            lifecycle_state: record.lifecycle_state,
            last_seen_at: record.last_seen_at,
        }
    }
}
