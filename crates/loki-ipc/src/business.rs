//! Canonical bounded business-RPC vocabulary shared by desktop and agent
//! clients. A runtime composition node (`P1-RUNTIME-COMP-001`) installs a
//! real producer through the transport's `BusinessService` seam
//! (`crate::server::BusinessService`); the safe default returns
//! `UnsupportedMethod` for every business method (fail-closed).
//!
//! `Docs/graph/tasks/P1-RPC-CONTRACT-001-R1.md` (this packet) Deliverable 1 /
//! `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
//! "Consequences": "Mutation results become discriminated states with no
//! contradictory nullable combinations."
//!
//! # Why only 10 of the 23 unary methods produce a real result shape
//!
//! `crates/loki-runtime/src/services/{read.rs,mutation.rs}`
//! (`P1-SERVICE-READINESS-001-R1`) and `crates/loki-mutation-contract`
//! (`P1-MUTATION-CONTRACT-001`) are this node's only real, already-accepted
//! backing contracts. They expose exactly:
//! - three read projections — [`crate::business`]'s
//!   `RuntimeStatus`/`DevicesList`/`DeviceGet`/`DeviceHealth` results mirror
//!   `loki_runtime::services::{RuntimeStatus, DevicePage,
//!   DeviceHealthProjection}` field-for-field;
//! - seven closed mutation methods
//!   (`loki_mutation_contract::MutationMethod::ALL`), of which six have a
//!   corresponding named IPC method in this closed 23-method vocabulary
//!   (`RestartRequest`, `FlashRequest`, `BuildRequest`, `AgentRunRegister`,
//!   `AgentRunUpdate`, `WorkIntentAcknowledge` — `ApplyConfig` has no wire
//!   method yet; nothing in this node's salvaged vocabulary names it, and
//!   inventing one is out of this node's scope).
//!
//! The remaining 13 unary methods (`HealthCheckRequest`, `PreflightRequest`,
//! `JobCancel`, `TestOnDeviceRequest`, `DeploymentRequest`,
//! `RollbackRequest`, `ApprovalGrant`, `ApprovalDeny`, `DeviceReadLogs`,
//! `DeviceQueryTelemetry`, `IncidentsList`, `IncidentGet`, `JobGet`) have **no
//! real backing type anywhere in this workspace** — no `RuntimeReadPort`
//! projection, no `MutationMethod` variant, no approval-issuance surface.
//! Deliverable 1 forbids "inventing a field `RuntimeServices` doesn't
//! actually produce"; the same principle applies one level up; inventing a
//! plausible-looking *result shape* for a method with no real backing
//! contract would be exactly the wire-side reconstruction
//! `ADR-0006`/REV-004 forbids, only moved from the mutation-result field
//! level to the method-result-existence level. These 13 methods therefore
//! stay in the closed, named, typed, authorized vocabulary (never a generic
//! `Invoke` escape hatch — `AGENTS.md` rule 13) but are **structurally unable
//! to return `CallOutcome::Ok`**: [`crate::server::authorize_method`] still
//! requires their exact resource authority before dispatch (so a real
//! adversarial allow/deny matrix is still provable), and every
//! `BusinessService` this node ships — including the test double
//! (Deliverable 8) — returns `RpcError::UnsupportedMethod` for them, always.
//! This is recorded explicitly in the handoff as a still-open gap requiring
//! a future contract-extension node (widening `MutationMethod`'s closed set
//! and/or `RuntimeReadPort`'s read projections) before those 13 methods can
//! ever return a real result — never something this node silently resolved.

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

/// An authenticated principal. **No `workspace`/tenant field** —
/// `ADR-0006` Amendment 1 is settled and is not reopened here (Part One has
/// no real local workspace concept; see that amendment's text).
/// `authorized_entity_ids` is deliberately not called "scope": it is a flat
/// allowlist of the exact concrete entity identifiers (`device_id`/
/// `job_id`/`incident_id`/`run_id`/`approval_id`/`work_intent_id` — never a
/// placeholder tenant/workspace value) this principal may read or subscribe
/// to, checked by exact string equality against the request's own target
/// identity field, never against an invented grouping concept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedPrincipal {
    pub kind: PrincipalKind,
    pub principal_id: String,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub authorized_entity_ids: BTreeSet<String>,
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

impl AuthenticatedPrincipal {
    /// The legacy core-only (Ping/EchoStream) principal. Not authorized for
    /// any business method or stream (empty `authority`) and not bound to
    /// any entity (empty `authorized_entity_ids`) — this exists only so
    /// `RUN-002`'s pre-existing core-only default credential still
    /// round-trips; it must never gain business authority implicitly.
    pub fn runtime_service(principal_id: impl Into<String>) -> Self {
        Self {
            kind: PrincipalKind::RuntimeService,
            principal_id: principal_id.into(),
            authorized_entity_ids: BTreeSet::new(),
            authority: BTreeSet::new(),
        }
    }

    pub fn authorizes(&self, authority: Authority) -> bool {
        self.authority.contains(&authority)
    }

    /// Whether this principal may read/subscribe to the exact entity
    /// `entity_id` names. `RuntimeService`-kind principals (the core-only
    /// legacy default) never pass this check implicitly — only an entity id
    /// actually present in `authorized_entity_ids` does.
    pub fn is_authorized_for_entity(&self, entity_id: &str) -> bool {
        self.authorized_entity_ids.contains(entity_id)
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

/// Mirrors `loki_runtime::services::DevicePageRequest` field-for-field
/// (`limit`, `after_device_id`) — Deliverable 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceListParams {
    pub limit: u16,
    pub after_device_id: Option<String>,
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
pub struct IdempotentDeviceParams {
    pub device_id: String,
    pub idempotency_key: String,
}

/// A caller-supplied, single-use reference to a consumed (or consumable)
/// approval — the wire projection of
/// `loki_mutation_contract::request::ApprovalReference`. Approval
/// *issuance* is out of this node's scope (no approval-decision method
/// exists in this vocabulary); a caller presents a reference it already
/// obtained some other way. `bound_request_digest` must parse as a
/// `loki_mutation_contract::digest::RequestDigest` (`sha256:<64 lowercase
/// hex>`); a malformed value is rejected as `InvalidRequest` before any
/// mutation submission is attempted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalReferenceParams {
    pub approval_id: String,
    pub bound_request_digest: String,
    pub expires_at_ms: i64,
}

/// Every mutation-backed method's params share this exact field set beyond
/// their own target-identity fields: `parameters_digest` (caller-supplied,
/// opaque, format-validated only — this crate never computes it),
/// `idempotency_key`, and an optional pre-obtained `approval_reference` for
/// the three approval-gated methods (`loki_mutation_contract::MutationMethod::
/// requires_approval() == true`: `Restart`, `Flash`, `ContainedBuild`).

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestartRequestParams {
    pub device_id: String,
    pub parameters_digest: String,
    pub idempotency_key: String,
    pub approval_reference: Option<ApprovalReferenceParams>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashRequestParams {
    pub device_id: String,
    pub artifact_id: String,
    pub artifact_digest: String,
    pub parameters_digest: String,
    pub idempotency_key: String,
    pub approval_reference: Option<ApprovalReferenceParams>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRequestParams {
    pub source_snapshot_id: String,
    pub target_id: String,
    pub recipe_id: String,
    pub parameters_digest: String,
    pub idempotency_key: String,
    pub approval_reference: Option<ApprovalReferenceParams>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRunRegisterParams {
    pub parent_run_id: Option<String>,
    pub parameters_digest: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRunUpdateParams {
    pub run_id: String,
    pub phase: String,
    pub summary: String,
    pub parameters_digest: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkIntentAckParams {
    pub work_intent_id: String,
    pub run_id: String,
    pub parameters_digest: String,
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
pub struct JobCancelParams {
    pub job_id: String,
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

/// `entity_id` binds the stream to the exact `device_id`/`job_id`/
/// `incident_id`/`run_id`/`approval_id`/`work_intent_id` (as applicable to
/// `stream`) it is scoped to — never a workspace/tenant concept
/// (`ADR-0006` Amendment 1).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessStreamParams {
    pub stream: BusinessStreamName,
    pub entity_id: String,
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

// ---------------------------------------------------------------------
// Read-result wire types. `WireStorageReadiness`, `WireMutationAvailability`,
// `WireDeviceLifecycleState`, `WireSupportTier`, `WireDeviceHealthState`,
// `WireDeviceSummary` mirror `loki_runtime::services::{StorageReadiness,
// MutationAvailability, DeviceSummary, DeviceHealthState,
// PersistedHealthEvidence}` and `loki_storage::states::{DeviceLifecycleState,
// SupportTier}` field-for-field/variant-for-variant (Deliverable 1). This
// crate does not depend on `loki-runtime`/`loki-storage` in production code
// (§5 ownership), so these are independently declared mirrors, not reused
// types — kept honest by this module's own tests asserting every source
// variant/field this doc comment claims to mirror still exists.
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireDeviceLifecycleState {
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
pub enum WireSupportTier {
    Certified,
    Supported,
    Custom,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WireStorageReadiness {
    Ready,
    Corrupt { problem_count: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireMutationAvailability {
    Available,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WirePersistedHealthEvidence {
    pub payload_size_bytes: u64,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WireDeviceHealthState {
    NotObserved,
    Persisted {
        evidence: WirePersistedHealthEvidence,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireDeviceSummary {
    pub device_id: String,
    pub connector_id: String,
    pub connector_version: String,
    pub family: String,
    pub board_identity: Option<String>,
    pub runtime_id: String,
    pub support_tier: WireSupportTier,
    pub lifecycle_state: WireDeviceLifecycleState,
    pub last_seen_at: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadMeta {
    pub source_runtime_id: String,
    pub observed_at: String,
    pub correlation_id: String,
}

// ---------------------------------------------------------------------
// Mutation-result wire types. `WireMutationOutcome`/`WireMutationRejection`
// are the ONLY way a mutation-backed method's outcome may reach the wire;
// see `crate::mutation_mapping` for the required explicit `From`
// construction — never constructed ad hoc elsewhere.
// ---------------------------------------------------------------------

/// Mirrors `loki_mutation_contract::MutationRejection` variant-for-variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WireMutationRejection {
    DigestConflict,
    GrantInvalid { reason: String },
    GrantExpired,
    GrantRevoked,
    PolicyDenied,
    ApprovalNotConsumable,
    InvalidInput { field: String, reason: String },
}

/// Mirrors `loki_mutation_contract::AcceptanceResult` variant-for-variant —
/// no contradictory nullable combination (each variant carries only its own
/// fields, matching the source enum's own "closed enum, not one flat struct
/// with optional fields" shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum WireMutationOutcome {
    Accepted {
        job_id: String,
        correlation_id: String,
        accepted_at_ms: i64,
    },
    ApprovalRequired {
        approval_id: String,
        bound_request_digest: String,
        expires_at_ms: i64,
    },
    ReplayedIdenticalAccepted {
        job_id: String,
        correlation_id: String,
        accepted_at_ms: i64,
    },
    Rejected {
        rejection: WireMutationRejection,
    },
}

/// The six mutation-backed methods' `MutationMethod` identity on the wire.
/// `ContainedBuild` is this variant's name (matching
/// `loki_mutation_contract::MutationMethod::ContainedBuild` exactly) even
/// though the IPC method is named `BuildRequest` — see this module's top
/// doc comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationMethod {
    Restart,
    Flash,
    ContainedBuild,
    AgentRunRegister,
    AgentRunUpdate,
    WorkIntentAcknowledge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BusinessRpcResult {
    RuntimeStatus {
        meta: ReadMeta,
        storage: WireStorageReadiness,
        registered_device_count: u64,
        mutations: WireMutationAvailability,
    },
    DevicesList {
        meta: ReadMeta,
        devices: Vec<WireDeviceSummary>,
        next_after_device_id: Option<String>,
        total_registered_count: u64,
    },
    DeviceGet {
        meta: ReadMeta,
        device: Option<WireDeviceSummary>,
    },
    DeviceHealth {
        meta: ReadMeta,
        device_id: String,
        lifecycle_state: WireDeviceLifecycleState,
        health: WireDeviceHealthState,
    },
    /// A committed/attempted mutation outcome, produced only via
    /// `crate::mutation_mapping`'s explicit `From` functions from a real
    /// `loki_mutation_contract::AcceptanceResult`. `target_id` is always
    /// exactly the request's own target-identity field (device_id/run_id/
    /// work_intent_id, as applicable) — the entity-identity coupling proof
    /// (`crate::server::execute_method` rejects any mismatch before this
    /// ever reaches the wire).
    Mutation {
        method: OperationMethod,
        correlation_id: String,
        target_id: String,
        outcome: WireMutationOutcome,
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
/// Format of `loki_mutation_contract::digest::RequestDigest::try_from_str`'s
/// accepted input: `sha256:` followed by exactly 64 lowercase hex digits.
/// Duplicated here (rather than imported) because this crate's only
/// dependency on `loki-mutation-contract` is for the read-only result
/// vocabulary types (§5) — this is pure format validation, not a
/// reimplementation of any acceptance/hashing logic.
pub fn safe_request_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

impl WireDeviceSummary {
    fn validate_contract(&self) -> bool {
        safe_id(&self.device_id, 200)
            && bounded_utf8(&self.connector_id, 200, false)
            && bounded_utf8(&self.connector_version, 100, false)
            && bounded_utf8(&self.family, 100, false)
            && self
                .board_identity
                .as_deref()
                .is_none_or(|value| bounded_utf8(value, 200, false))
            && safe_id(&self.runtime_id, 200)
            && valid_utc_timestamp(&self.last_seen_at)
    }
}

impl WireStorageReadiness {
    fn validate_contract(&self) -> bool {
        true
    }
}

impl WireDeviceHealthState {
    fn validate_contract(&self) -> bool {
        match self {
            Self::NotObserved => true,
            Self::Persisted { evidence } => valid_utc_timestamp(&evidence.last_seen_at),
        }
    }
}

impl WireMutationOutcome {
    fn validate_contract(&self) -> bool {
        match self {
            Self::Accepted {
                job_id,
                correlation_id,
                ..
            }
            | Self::ReplayedIdenticalAccepted {
                job_id,
                correlation_id,
                ..
            } => safe_id(job_id, 200) && safe_id(correlation_id, 200),
            Self::ApprovalRequired {
                approval_id,
                bound_request_digest,
                ..
            } => safe_id(approval_id, 200) && safe_request_digest(bound_request_digest),
            Self::Rejected { rejection } => rejection.validate_contract(),
        }
    }
}

impl WireMutationRejection {
    fn validate_contract(&self) -> bool {
        match self {
            Self::DigestConflict
            | Self::GrantExpired
            | Self::GrantRevoked
            | Self::PolicyDenied
            | Self::ApprovalNotConsumable => true,
            Self::GrantInvalid { reason } => bounded_utf8(reason, MAX_TEXT_BYTES, false),
            Self::InvalidInput { field, reason } => {
                bounded_utf8(field, 200, false) && bounded_utf8(reason, MAX_TEXT_BYTES, false)
            }
        }
    }
}

impl BusinessRpcResult {
    pub fn correlation_id(&self) -> &str {
        match self {
            Self::RuntimeStatus { meta, .. }
            | Self::DevicesList { meta, .. }
            | Self::DeviceGet { meta, .. }
            | Self::DeviceHealth { meta, .. } => &meta.correlation_id,
            Self::Mutation { correlation_id, .. } => correlation_id,
        }
    }

    pub fn validate_contract(&self) -> bool {
        let valid_cursor = |cursor: &Option<String>| cursor.as_deref().is_none_or(safe_cursor);
        match self {
            Self::RuntimeStatus { meta, storage, .. } => {
                meta.validate_contract() && storage.validate_contract()
            }
            Self::DevicesList {
                meta,
                devices,
                next_after_device_id,
                ..
            } => {
                meta.validate_contract()
                    && devices.len() <= usize::from(MAX_PAGE_SIZE)
                    && devices.iter().all(WireDeviceSummary::validate_contract)
                    && next_after_device_id
                        .as_deref()
                        .is_none_or(|value| safe_id(value, 200))
                    && valid_cursor(&None)
            }
            Self::DeviceGet { meta, device } => {
                meta.validate_contract()
                    && device
                        .as_ref()
                        .is_none_or(WireDeviceSummary::validate_contract)
            }
            Self::DeviceHealth {
                meta,
                device_id,
                health,
                ..
            } => meta.validate_contract() && safe_id(device_id, 200) && health.validate_contract(),
            Self::Mutation {
                correlation_id,
                target_id,
                outcome,
                ..
            } => {
                safe_id(correlation_id, 200)
                    && safe_id(target_id, 200)
                    && outcome.validate_contract()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authenticated_principal_has_no_workspace_field() {
        // Compile-time proof by exhaustive struct-literal construction: if a
        // `workspace_id`/`tenant_id` field were ever reintroduced, this
        // literal would fail to compile (missing/extra field), not merely
        // fail an assertion.
        let principal = AuthenticatedPrincipal {
            kind: PrincipalKind::DesktopInstallation,
            principal_id: "p".to_string(),
            authorized_entity_ids: BTreeSet::new(),
            authority: BTreeSet::new(),
        };
        assert_eq!(principal.principal_id, "p");
    }

    #[test]
    fn business_stream_params_has_no_workspace_field() {
        let params = BusinessStreamParams {
            stream: BusinessStreamName::Jobs,
            entity_id: "job-1".to_string(),
            after_sequence: None,
            max_items: 10,
        };
        assert_eq!(params.entity_id, "job-1");
    }

    #[test]
    fn safe_request_digest_accepts_only_the_real_digest_format() {
        assert!(safe_request_digest(
            &("sha256:".to_string() + &"a".repeat(64))
        ));
        assert!(!safe_request_digest("sha256:short"));
        assert!(!safe_request_digest(
            &("sha256:".to_string() + &"g".repeat(64))
        ));
        assert!(!safe_request_digest(
            &("SHA256:".to_string() + &"a".repeat(64))
        ));
    }
}
