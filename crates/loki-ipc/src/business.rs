//! Canonical bounded business-RPC vocabulary shared by desktop and agent clients.
//! This module is contract only: the transport returns `UnsupportedMethod` until
//! a runtime composition node installs real producers.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

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
    Subscribe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticatedPrincipal {
    pub kind: PrincipalKind,
    pub principal_id: String,
    pub authority: BTreeSet<Authority>,
}

impl AuthenticatedPrincipal {
    pub fn runtime_service(principal_id: impl Into<String>) -> Self {
        Self {
            kind: PrincipalKind::RuntimeService,
            principal_id: principal_id.into(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[serde(tag = "kind", rename_all = "snake_case")]
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
        incident: Option<IncidentSummary>,
    },
    JobGet {
        meta: ReadMeta,
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
        approval_id: Option<String>,
        job_id: Option<String>,
        bound_request_digest: Option<String>,
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
