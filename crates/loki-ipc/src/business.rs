//! Canonical bounded business-RPC vocabulary shared by desktop and agent clients.
//! A runtime composition node installs real producers through the transport's
//! `BusinessService`; the safe default returns `UnsupportedMethod`.

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

pub const BUSINESS_CAPABILITY: &str = "loki-ipc.business.v1";
pub const MAX_PAGE_SIZE: u16 = 100;
pub const MAX_TEXT_BYTES: usize = 2_000;
pub const MAX_STREAM_ITEMS: u16 = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    DesktopInstallation,
    AgentConnection,
    RuntimeService,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    ReadRuntime,
    ReadDevices,
    ReadTelemetry,
    ReadIncidents,
    ReadJobs,
    ManageAgentRuns,
    AcknowledgeWorkIntents,
    RequestDiagnostics,
    CancelJobs,
    RequestBuild,
    RequestDeviceOperation,
    DecideApproval,
    ReadApprovals,
    ReadNotifications,
    ReadWorkIntents,
    Subscribe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedPrincipal {
    pub kind: PrincipalKind,
    pub principal_id: String,
    pub workspace_id: String,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub scope_ids: BTreeSet<String>,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub authority: BTreeSet<Authority>,
}

fn deserialize_unique_set<'de, D, T>(deserializer: D) -> Result<BTreeSet<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Ord,
{
    let values = Vec::<T>::deserialize(deserializer)?;
    let original_len = values.len();
    let values = values.into_iter().collect::<BTreeSet<_>>();
    if values.len() != original_len {
        return Err(serde::de::Error::custom("duplicate set member"));
    }
    Ok(values)
}

fn deserialize_required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

impl AuthenticatedPrincipal {
    pub fn runtime_service(principal_id: impl Into<String>) -> Self {
        Self {
            kind: PrincipalKind::RuntimeService,
            principal_id: principal_id.into(),
            workspace_id: "local".to_string(),
            scope_ids: BTreeSet::from(["local".to_string()]),
            authority: BTreeSet::new(),
        }
    }

    pub fn authorizes(&self, authority: Authority) -> bool {
        self.authority.contains(&authority)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyParams {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageParams {
    pub limit: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceParams {
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevicePageParams {
    pub device_id: String,
    pub limit: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryParams {
    pub device_id: String,
    pub limit: u16,
    pub cursor: Option<String>,
    pub signal_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncidentParams {
    pub incident_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobParams {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRunRegisterParams {
    pub parent_run_id: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdempotentDeviceParams {
    pub device_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobCancelParams {
    pub job_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRunUpdateParams {
    pub run_id: String,
    pub phase: String,
    pub summary: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkIntentAckParams {
    pub work_intent_id: String,
    pub run_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRequestParams {
    pub source_snapshot_id: String,
    pub target_id: String,
    pub recipe_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactOperationParams {
    pub device_id: String,
    pub artifact_id: String,
    pub artifact_digest: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestOperationParams {
    pub device_id: String,
    pub test_suite_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentRequestParams {
    pub device_id: String,
    pub artifact_id: String,
    pub artifact_digest: String,
    pub verification_plan_id: String,
    pub rollback_plan_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackRequestParams {
    pub device_id: String,
    pub deployment_id: String,
    pub recovery_policy_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalGrantParams {
    pub approval_id: String,
    pub bound_request_digest: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalDenyParams {
    pub approval_id: String,
    pub bound_request_digest: String,
    pub reason: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BusinessStreamName {
    RuntimeStatus,
    DeviceProjection,
    DeviceHealth,
    Telemetry,
    Logs,
    Incidents,
    Jobs,
    Approvals,
    Notifications,
    WorkIntents,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessStreamParams {
    pub stream: BusinessStreamName,
    pub workspace_id: String,
    pub scope_id: String,
    pub after_sequence: Option<u64>,
    pub max_items: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessEvent {
    pub stream: BusinessStreamName,
    pub sequence: u64,
    pub correlation_id: String,
    pub occurred_at: String,
    pub entity_id: String,
    pub state: String,
    pub source_runtime_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessStreamSummary {
    pub items_sent: u16,
    pub last_sequence: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Unknown,
    Starting,
    Connected,
    Degraded,
    Disconnected,
    #[serde(rename = "version-incompatible")]
    VersionIncompatible,
    Unauthorized,
    Updating,
    Stopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    Discovered,
    Identifying,
    Identified,
    Checking,
    #[serde(rename = "enrollment-required")]
    EnrollmentRequired,
    Unsupported,
    Degraded,
    Enrolled,
    Ready,
    Offline,
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Passed,
    Warning,
    Failed,
    Unavailable,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IncidentState {
    Open,
    Investigating,
    WaitingApproval,
    Executing,
    Verifying,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobState {
    Queued,
    PolicyCheck,
    WaitingApproval,
    Preflight,
    Executing,
    Restarting,
    Testing,
    Observing,
    Succeeded,
    Failed,
    Cancelled,
    RollbackPending,
    RollingBack,
    RolledBack,
    OutcomeUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunState {
    Planning,
    Working,
    Waiting,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadMeta {
    pub source_runtime_id: String,
    pub observed_at: String,
    pub correlation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceSummary {
    pub device_id: String,
    pub lifecycle_state: DeviceState,
    pub connector_id: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthEntry {
    pub check_id: String,
    pub status: HealthStatus,
    pub observed_at: String,
    pub evidence_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogEntry {
    pub sequence: u64,
    pub observed_at: String,
    pub level: String,
    pub message: String,
    pub untrusted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryEntry {
    pub sequence: u64,
    pub signal_id: String,
    pub observed_at: String,
    pub value_text: String,
    pub unit: Option<String>,
    pub quality: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncidentSummary {
    pub incident_id: String,
    pub state: IncidentState,
    pub severity: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobSummary {
    pub job_id: String,
    pub state: JobState,
    pub updated_at: String,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationMethod {
    HealthCheckRequest,
    PreflightRequest,
    JobCancel,
    BuildRequest,
    RestartRequest,
    FlashRequest,
    TestOnDeviceRequest,
    DeploymentRequest,
    RollbackRequest,
    ApprovalGrant,
    ApprovalDeny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    Rejected,
    ApprovalRequired,
    Accepted,
    Deduplicated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BusinessRpcResult {
    RuntimeStatus {
        meta: ReadMeta,
        state: RuntimeState,
        runtime_id: String,
    },
    DevicesList {
        meta: ReadMeta,
        devices: Vec<DeviceSummary>,
        next_cursor: Option<String>,
    },
    DeviceGet {
        meta: ReadMeta,
        #[serde(deserialize_with = "deserialize_required_nullable")]
        device: Option<DeviceSummary>,
    },
    DeviceHealth {
        meta: ReadMeta,
        device_id: String,
        entries: Vec<HealthEntry>,
        next_cursor: Option<String>,
    },
    DeviceLogs {
        meta: ReadMeta,
        device_id: String,
        entries: Vec<LogEntry>,
        next_cursor: Option<String>,
    },
    DeviceTelemetry {
        meta: ReadMeta,
        device_id: String,
        entries: Vec<TelemetryEntry>,
        next_cursor: Option<String>,
    },
    IncidentsList {
        meta: ReadMeta,
        incidents: Vec<IncidentSummary>,
        next_cursor: Option<String>,
    },
    IncidentGet {
        meta: ReadMeta,
        #[serde(deserialize_with = "deserialize_required_nullable")]
        incident: Option<IncidentSummary>,
    },
    JobGet {
        meta: ReadMeta,
        #[serde(deserialize_with = "deserialize_required_nullable")]
        job: Option<JobSummary>,
    },
    AgentRun {
        meta: ReadMeta,
        run_id: String,
        state: AgentRunState,
    },
    WorkIntentAcknowledged {
        meta: ReadMeta,
        work_intent_id: String,
        run_id: String,
    },
    Operation {
        method: OperationMethod,
        outcome: OperationOutcome,
        correlation_id: String,
        policy_reason: String,
        idempotency_key: String,
        #[serde(deserialize_with = "deserialize_required_nullable")]
        artifact_digest: Option<String>,
        #[serde(deserialize_with = "deserialize_required_nullable")]
        request_reason: Option<String>,
        approval_id: Option<String>,
        job_id: Option<String>,
        bound_request_digest: Option<String>,
        #[serde(deserialize_with = "deserialize_unique_set")]
        scope_ids: BTreeSet<String>,
    },
}

pub fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max
}
pub fn safe_id(value: &str, max: usize) -> bool {
    bounded(value, max)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}
pub fn safe_cursor(value: &str) -> bool {
    bounded(value, 512)
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b':' | b'-')
        })
}
pub fn safe_digest(value: &str) -> bool {
    (16..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
pub fn valid_page(limit: u16, cursor: Option<&str>) -> bool {
    (1..=MAX_PAGE_SIZE).contains(&limit) && cursor.is_none_or(safe_cursor)
}

pub fn bounded_utf8(value: &str, max_bytes: usize, allow_empty: bool) -> bool {
    (allow_empty || !value.is_empty()) && value.len() <= max_bytes
}

pub fn valid_utc_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes.last() != Some(&b'Z')
        || !bytes.iter().all(u8::is_ascii)
    {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| {
        bytes[range.clone()]
            .iter()
            .all(u8::is_ascii_digit)
            .then(|| {
                bytes[range]
                    .iter()
                    .fold(0_u32, |value, digit| value * 10 + u32::from(digit - b'0'))
            })
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        digits(0..4),
        digits(5..7),
        digits(8..10),
        digits(11..13),
        digits(14..16),
        digits(17..19),
    ) else {
        return false;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    if day == 0 || day > max_day || hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    bytes.len() == 20
        || (bytes[19] == b'.'
            && bytes.len() > 21
            && bytes[20..bytes.len() - 1].iter().all(u8::is_ascii_digit))
}

impl ReadMeta {
    fn validate_contract(&self) -> bool {
        safe_id(&self.source_runtime_id, 200)
            && valid_utc_timestamp(&self.observed_at)
            && safe_id(&self.correlation_id, 200)
    }
}

impl DeviceSummary {
    fn validate_contract(&self) -> bool {
        safe_id(&self.device_id, 200)
            && bounded_utf8(&self.connector_id, 200, false)
            && valid_utc_timestamp(&self.last_seen_at)
    }
}

impl IncidentSummary {
    fn validate_contract(&self) -> bool {
        safe_id(&self.incident_id, 200)
            && bounded_utf8(&self.severity, 100, false)
            && valid_utc_timestamp(&self.updated_at)
    }
}

impl JobSummary {
    fn validate_contract(&self) -> bool {
        safe_id(&self.job_id, 200)
            && valid_utc_timestamp(&self.updated_at)
            && self
                .error_code
                .as_deref()
                .is_none_or(|value| bounded_utf8(value, 100, false))
    }
}

impl BusinessRpcResult {
    pub fn correlation_id(&self) -> &str {
        match self {
            Self::RuntimeStatus { meta, .. }
            | Self::DevicesList { meta, .. }
            | Self::DeviceGet { meta, .. }
            | Self::DeviceHealth { meta, .. }
            | Self::DeviceLogs { meta, .. }
            | Self::DeviceTelemetry { meta, .. }
            | Self::IncidentsList { meta, .. }
            | Self::IncidentGet { meta, .. }
            | Self::JobGet { meta, .. }
            | Self::AgentRun { meta, .. }
            | Self::WorkIntentAcknowledged { meta, .. } => &meta.correlation_id,
            Self::Operation { correlation_id, .. } => correlation_id,
        }
    }

    pub fn validate_contract(&self) -> bool {
        let valid_cursor = |cursor: &Option<String>| cursor.as_deref().is_none_or(safe_cursor);
        match self {
            Self::RuntimeStatus {
                meta, runtime_id, ..
            } => meta.validate_contract() && safe_id(runtime_id, 200),
            Self::DevicesList {
                meta,
                devices,
                next_cursor,
            } => {
                meta.validate_contract()
                    && devices.len() <= usize::from(MAX_PAGE_SIZE)
                    && devices.iter().all(DeviceSummary::validate_contract)
                    && valid_cursor(next_cursor)
            }
            Self::DeviceGet { meta, device } => {
                meta.validate_contract()
                    && device.as_ref().is_none_or(DeviceSummary::validate_contract)
            }
            Self::DeviceHealth {
                meta,
                device_id,
                entries,
                next_cursor,
            } => {
                meta.validate_contract()
                    && safe_id(device_id, 200)
                    && entries.len() <= usize::from(MAX_PAGE_SIZE)
                    && entries.iter().all(|entry| {
                        bounded_utf8(&entry.check_id, 128, false)
                            && valid_utc_timestamp(&entry.observed_at)
                            && entry
                                .evidence_ref
                                .as_deref()
                                .is_none_or(|value| bounded_utf8(value, 512, false))
                    })
                    && valid_cursor(next_cursor)
            }
            Self::DeviceLogs {
                meta,
                device_id,
                entries,
                next_cursor,
            } => {
                meta.validate_contract()
                    && safe_id(device_id, 200)
                    && entries.len() <= usize::from(MAX_PAGE_SIZE)
                    && entries.iter().all(|entry| {
                        valid_utc_timestamp(&entry.observed_at)
                            && bounded_utf8(&entry.level, 32, false)
                            && bounded_utf8(&entry.message, MAX_TEXT_BYTES, true)
                    })
                    && valid_cursor(next_cursor)
            }
            Self::DeviceTelemetry {
                meta,
                device_id,
                entries,
                next_cursor,
            } => {
                meta.validate_contract()
                    && safe_id(device_id, 200)
                    && entries.len() <= usize::from(MAX_PAGE_SIZE)
                    && entries.iter().all(|entry| {
                        bounded_utf8(&entry.signal_id, 128, false)
                            && valid_utc_timestamp(&entry.observed_at)
                            && bounded_utf8(&entry.value_text, 512, true)
                            && entry
                                .unit
                                .as_deref()
                                .is_none_or(|value| bounded_utf8(value, 64, false))
                            && bounded_utf8(&entry.quality, 64, false)
                    })
                    && valid_cursor(next_cursor)
            }
            Self::IncidentsList {
                meta,
                incidents,
                next_cursor,
            } => {
                meta.validate_contract()
                    && incidents.len() <= usize::from(MAX_PAGE_SIZE)
                    && incidents.iter().all(IncidentSummary::validate_contract)
                    && valid_cursor(next_cursor)
            }
            Self::IncidentGet { meta, incident } => {
                meta.validate_contract()
                    && incident
                        .as_ref()
                        .is_none_or(IncidentSummary::validate_contract)
            }
            Self::JobGet { meta, job } => {
                meta.validate_contract() && job.as_ref().is_none_or(JobSummary::validate_contract)
            }
            Self::AgentRun { meta, run_id, .. } => meta.validate_contract() && safe_id(run_id, 200),
            Self::WorkIntentAcknowledged {
                meta,
                work_intent_id,
                run_id,
            } => meta.validate_contract() && safe_id(work_intent_id, 200) && safe_id(run_id, 200),
            Self::Operation {
                correlation_id,
                policy_reason,
                idempotency_key,
                artifact_digest,
                request_reason,
                approval_id,
                job_id,
                bound_request_digest,
                scope_ids,
                ..
            } => {
                safe_id(correlation_id, 200)
                    && bounded_utf8(policy_reason, MAX_TEXT_BYTES, false)
                    && safe_id(idempotency_key, 200)
                    && artifact_digest.as_deref().is_none_or(safe_digest)
                    && request_reason
                        .as_deref()
                        .is_none_or(|value| bounded_utf8(value, MAX_TEXT_BYTES, false))
                    && approval_id
                        .as_deref()
                        .is_none_or(|value| safe_id(value, 200))
                    && job_id.as_deref().is_none_or(|value| safe_id(value, 200))
                    && bound_request_digest.as_deref().is_none_or(safe_digest)
                    && !scope_ids.is_empty()
                    && scope_ids.len() <= 16
                    && scope_ids.iter().all(|scope| safe_id(scope, 200))
            }
        }
    }
}

impl BusinessEvent {
    pub fn validate_contract(&self) -> bool {
        safe_id(&self.correlation_id, 200)
            && valid_utc_timestamp(&self.occurred_at)
            && safe_id(&self.entity_id, 200)
            && bounded_utf8(&self.state, 100, false)
            && safe_id(&self.source_runtime_id, 200)
    }
}

impl BusinessStreamSummary {
    pub fn validate_contract(&self, requested_max: u16) -> bool {
        self.items_sent <= requested_max && self.items_sent <= MAX_STREAM_ITEMS
    }
}
