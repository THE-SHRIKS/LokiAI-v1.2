//! Real `loki_ipc::server::BusinessService` implementation
//! (`P1-RUNTIME-COMP-001`, `Docs/graph/tasks/P1-RUNTIME-COMP-001.md`):
//! composes the already-accepted `crate::services::RuntimeServices`
//! (`P1-SERVICE-READINESS-001-R1`) behind the already-accepted `loki-ipc`
//! `BusinessService` seam (`P1-RPC-CONTRACT-001-R1`), replacing
//! `UnsupportedBusinessService` inside `crate::host::compose::ComposedHost`.
//!
//! Scope is narrowed to 8 of the 10 real-backed business methods (`Ping`/
//! `EchoStream` are already real and untouched by this module):
//! - 4 real read methods (`RuntimeGetStatus`, `DevicesList`, `DeviceGet`,
//!   `DeviceGetHealth`), wired to `RuntimeReadPort`;
//! - 3 agent-mutation methods (`AgentRunRegister`, `AgentRunUpdate`,
//!   `WorkIntentAcknowledge`), wired to `MutationSubmissionPort`.
//!
//! `RestartRequest`/`FlashRequest`/`BuildRequest` — the 3 desktop-
//! approval-gated mutation methods — are explicitly and deliberately **not**
//! wired here; see [`deferred_desktop_mutation`]'s doc comment. The 13
//! permanently-unsupported methods and all 10 business streams remain
//! `RpcError::UnsupportedMethod`, matching `UnsupportedBusinessService`
//! exactly.
//!
//! # The `principal_id == connection_id` convention (Deliverable 3)
//!
//! No wire method param carries a `principal_sid`/`grant_generation` field
//! (`crates/loki-ipc/src/business.rs`'s own top doc comment), so this module
//! derives the `loki_mutation_contract::MutationAcceptanceRequest`'s
//! `principal_sid`/`grant_generation` server-side from the already-
//! authenticated `AuthenticatedPrincipal`, never from the wire. For the 3
//! agent-mutation methods, that means treating `principal.principal_id` as a
//! real, persisted `agent_connections.connection_id` and reading its real,
//! current `generation` column via `Storage::get_agent_connection`.
//!
//! This convention is **narrow and additive**, not itself a resolved
//! production credential-identity design: it holds only because, at this
//! revision, an `AgentConnection`-kind principal's `principal_id` in the
//! only two places one is ever minted (`crates/loki-ipc/tests/support/
//! business.rs`'s fixtures and any future real agent-credential issuance
//! path) is exactly the same string as its `agent_connections.connection_id`
//! row. A future node that issues real per-agent IPC credentials must either
//! preserve this equality by construction or extend
//! `AuthenticatedPrincipal`/the credential-resolution path with an explicit,
//! separate `connection_id` field — silently drifting the two apart would
//! reopen exactly the impersonation risk the gate below closes.
//!
//! **This convention is safe only because of the mandatory kind-check gate**
//! in [`agent_mutation`]: `principal.kind` must equal
//! `PrincipalKind::AgentConnection` *before* `principal.principal_id` is
//! ever treated as a `connection_id`. A `DesktopInstallation` or
//! `RuntimeService` principal is rejected with `RpcError::PermissionDenied`
//! immediately, unconditionally, as the very first statement of
//! [`agent_mutation`] — never as an incidental side effect of the storage
//! lookup succeeding or failing. Without this gate, a `DesktopInstallation`
//! principal whose `principal_id` happened to collide with (or was crafted
//! to equal) a real `connection_id` could otherwise submit an agent-run
//! mutation it was never granted, or trigger a lookup against another
//! principal's connection row — this is the exact security property this
//! packet exists to fix (§1, §9 of the task packet).

use std::sync::Arc;

use loki_ipc::business::{
    AuthenticatedPrincipal, BusinessRpcResult, BusinessStreamParams, OperationMethod,
    PrincipalKind, ReadMeta, WireDeviceHealthState, WireDeviceLifecycleState, WireDeviceSummary,
    WireMutationAvailability, WirePersistedHealthEvidence, WireStorageReadiness, WireSupportTier,
};
use loki_ipc::mutation_mapping::map_acceptance_result;
use loki_ipc::protocol::{Method, RpcError};
use loki_ipc::server::{
    BusinessCallFuture, BusinessEventEmitter, BusinessRequestContext, BusinessService,
    BusinessStreamFuture,
};
use loki_mutation_contract::{IdempotencyKey, MutationAcceptanceRequest, MutationMethod};
use loki_storage::Storage;
use loki_storage::agents::AgentsRepository;
use loki_storage::ids::DeviceId;
use loki_storage::states::{DeviceLifecycleState, SupportTier};

use crate::services::{
    DeviceHealthState, DevicePageRequest, DeviceSummary, MutationAvailability,
    MutationSubmissionPort, RuntimeReadError, RuntimeReadPort, RuntimeServices, StorageReadiness,
};

/// This crate has no per-process "runtime instance identity" concept
/// distinct from `loki_storage::ids::RuntimeId` (a per-device connector-
/// runtime binding, unrelated to which host process answered this read) —
/// so `ReadMeta::source_runtime_id` uses this crate's own stable name as an
/// honest, non-fabricated label for "which runtime produced this read,"
/// rather than inventing a per-process identifier no other part of this
/// workspace persists or exposes.
const SOURCE_RUNTIME_ID: &str = "loki-runtime";

/// The registry-scan page size [`find_device`] uses. Matches
/// `loki_ipc::business::MAX_PAGE_SIZE` / the read port's own
/// `MAX_DEVICE_PAGE_SIZE` (both `100`) so a single scan page is always the
/// largest bounded page the read port accepts.
const DEVICE_SCAN_PAGE_SIZE: usize = 100;

/// The not-yet-real target identifier a fresh `AgentRunRegister` call
/// supplies when the caller did not name a `parent_run_id`: registration is
/// the one agent-mutation method whose own new identity (its `run_id`) does
/// not exist yet at request time (`crate::server::target_entity_id` in
/// `loki-ipc` itself returns `None` for this method for the same reason —
/// there is no request-scoped target entity to bind authorization to).
/// `MutationAcceptanceRequest::new` still requires *some* `target_id`
/// string; this sentinel matches the exact convention already established
/// by `crates/loki-ipc/tests/support/business.rs`'s `TestBusinessService`
/// test double (`P1-RPC-CONTRACT-001-R1`'s own sanctioned test-construction
/// pattern), not an independently invented value.
const AGENT_RUN_REGISTER_ROOT_TARGET: &str = "agent-run:root";

/// The real `BusinessService` this node installs into `ComposedHost`. Holds
/// a shared handle to the already-composed `RuntimeServices` and the same
/// `Arc<Storage>` `ComposedHost` already opened — never a second database
/// open, never a second `RuntimeServices::compose` call.
pub struct RuntimeBusinessService {
    services: Arc<RuntimeServices>,
    storage: Arc<Storage>,
}

impl RuntimeBusinessService {
    pub fn new(services: Arc<RuntimeServices>, storage: Arc<Storage>) -> Self {
        Self { services, storage }
    }
}

impl BusinessService for RuntimeBusinessService {
    fn call(&self, method: Method, context: BusinessRequestContext) -> BusinessCallFuture {
        let services = Arc::clone(&self.services);
        let storage = Arc::clone(&self.storage);
        Box::pin(async move { dispatch(services.as_ref(), storage.as_ref(), method, &context) })
    }

    /// `RuntimeServices` exposes no event/subscription port — inventing one
    /// is out of this node's scope (§5). Matches
    /// `UnsupportedBusinessService::stream` exactly.
    fn stream(
        &self,
        _params: BusinessStreamParams,
        _context: BusinessRequestContext,
        _emitter: BusinessEventEmitter,
    ) -> BusinessStreamFuture {
        Box::pin(async {
            Err(RpcError::UnsupportedMethod {
                method: "business_events".to_string(),
            })
        })
    }
}

fn dispatch(
    services: &RuntimeServices,
    storage: &Storage,
    method: Method,
    context: &BusinessRequestContext,
) -> Result<BusinessRpcResult, RpcError> {
    let correlation_id = context.correlation_id.to_string();
    match method {
        Method::Ping(_) => unreachable!(
            "Ping short-circuits inside crate::server::execute_method before \
             BusinessService::call is ever reached (loki-ipc, unmodified by this node)"
        ),
        Method::RuntimeGetStatus(_) => runtime_get_status(services, correlation_id),
        Method::DevicesList(params) => {
            let after_device_id = params
                .after_device_id
                .as_deref()
                .map(DeviceId::new)
                .transpose()
                .map_err(|_| invalid_request("after_device_id is not a valid device id"))?;
            let request = DevicePageRequest::new(usize::from(params.limit), after_device_id)
                .map_err(map_read_error)?;
            devices_list(services, correlation_id, request)
        }
        Method::DeviceGet(params) => {
            let target = DeviceId::new(params.device_id)
                .map_err(|_| invalid_request("device_id is not a valid device id"))?;
            device_get(services, correlation_id, target)
        }
        Method::DeviceGetHealth(params) => {
            let target = DeviceId::new(params.device_id)
                .map_err(|_| invalid_request("device_id is not a valid device id"))?;
            device_get_health(services, correlation_id, target)
        }
        Method::AgentRunRegister(params) => {
            let target_id = params
                .parent_run_id
                .unwrap_or_else(|| AGENT_RUN_REGISTER_ROOT_TARGET.to_string());
            agent_mutation(
                storage,
                services,
                &context.principal,
                correlation_id,
                MutationMethod::AgentRunRegister,
                OperationMethod::AgentRunRegister,
                target_id,
                params.parameters_digest,
                params.idempotency_key,
            )
        }
        Method::AgentRunUpdate(params) => agent_mutation(
            storage,
            services,
            &context.principal,
            correlation_id,
            MutationMethod::AgentRunUpdate,
            OperationMethod::AgentRunUpdate,
            params.run_id,
            params.parameters_digest,
            params.idempotency_key,
        ),
        Method::WorkIntentAcknowledge(params) => agent_mutation(
            storage,
            services,
            &context.principal,
            correlation_id,
            MutationMethod::WorkIntentAcknowledge,
            OperationMethod::WorkIntentAcknowledge,
            params.work_intent_id,
            params.parameters_digest,
            params.idempotency_key,
        ),
        // Deliverable 4: a dedicated arm per method (never folded into the
        // trailing wildcard below) — see `deferred_desktop_mutation`'s doc
        // comment for why these 3 stay unsupported in this node.
        Method::RestartRequest(_) => Err(deferred_desktop_mutation("restart_request")),
        Method::FlashRequest(_) => Err(deferred_desktop_mutation("flash_request")),
        Method::BuildRequest(_) => Err(deferred_desktop_mutation("build_request")),
        // The 13 permanently-unsupported methods (no real backing type
        // anywhere in this workspace — `crates/loki-ipc/src/business.rs`'s
        // top doc comment): identical, honest `UnsupportedMethod`, matching
        // `UnsupportedBusinessService` exactly (Deliverable 5).
        other => Err(RpcError::UnsupportedMethod {
            method: method_wire_name(&other).to_string(),
        }),
    }
}

/// `RestartRequest`/`FlashRequest`/`BuildRequest` are explicitly, temporarily
/// deferred by `P1-RUNTIME-COMP-001` (`Docs/graph/tasks/P1-RUNTIME-COMP-001.md`
/// §1, §6 Deliverable 4, §9, §11) to a not-yet-created follow-up node
/// (working title `P1-RUNTIME-COMP-DESKTOP-MUTATION-001`), itself gated on a
/// not-yet-opened decision covering how a Hello-authenticated
/// `DesktopInstallation` IPC principal is bound to a real `LocalAuthority`
/// session/generation — no code path anywhere in this workspace today binds
/// one (confirmed by direct inspection of `crates/loki-local-authority/src/
/// store.rs` and `crates/loki-storage/src/local_authority.rs`). This module
/// MUST NOT fabricate a `grant_generation`, a `LocalAuthority` session
/// lookup, or any placeholder/default value to make these three "work" —
/// that is exactly the invented-authority failure mode `AGENTS.md` rule 1
/// forbids. This is a tracked, named, temporary gap, distinct from the 13
/// permanently-unsupported methods that have no real backing contract at
/// all — hence its own dedicated, documented match arms in [`dispatch`]
/// rather than folding into the same wildcard.
fn deferred_desktop_mutation(method: &'static str) -> RpcError {
    RpcError::UnsupportedMethod {
        method: method.to_string(),
    }
}

fn runtime_get_status(
    services: &RuntimeServices,
    correlation_id: String,
) -> Result<BusinessRpcResult, RpcError> {
    let status = services.runtime_status().map_err(map_read_error)?;
    Ok(BusinessRpcResult::RuntimeStatus {
        meta: read_meta(correlation_id),
        storage: wire_storage_readiness(status.storage),
        registered_device_count: status.registered_device_count as u64,
        mutations: wire_mutation_availability(status.mutations),
    })
}

fn devices_list(
    services: &RuntimeServices,
    correlation_id: String,
    request: DevicePageRequest,
) -> Result<BusinessRpcResult, RpcError> {
    let page = services.list_devices(request).map_err(map_read_error)?;
    Ok(BusinessRpcResult::DevicesList {
        meta: read_meta(correlation_id),
        devices: page.devices.into_iter().map(wire_device_summary).collect(),
        next_after_device_id: page.next_after_device_id.map(DeviceId::into_string),
        total_registered_count: page.total_registered_count as u64,
    })
}

fn device_get(
    services: &RuntimeServices,
    correlation_id: String,
    target: DeviceId,
) -> Result<BusinessRpcResult, RpcError> {
    let device = find_device(services, &target)?;
    Ok(BusinessRpcResult::DeviceGet {
        meta: read_meta(correlation_id),
        device,
    })
}

fn device_get_health(
    services: &RuntimeServices,
    correlation_id: String,
    target: DeviceId,
) -> Result<BusinessRpcResult, RpcError> {
    let projection = services.device_health(&target).map_err(map_read_error)?;
    Ok(BusinessRpcResult::DeviceHealth {
        meta: read_meta(correlation_id),
        device_id: projection.device_id.into_string(),
        lifecycle_state: wire_lifecycle_state(projection.lifecycle_state),
        health: wire_health_state(projection.health),
    })
}

/// Scans the device registry through `RuntimeReadPort::list_devices`'s real,
/// bounded pagination until `target` is found or the registry is exhausted
/// — the only "get one device" mechanism available through the accepted
/// `RuntimeReadPort` contract (`P1-SERVICE-READINESS-001-R1` exposes no
/// single-device read; widening that already-accepted, deliberately bounded
/// port is out of this node's scope, §5). Bounded by the real, currently
/// registered device count — never fabricated, never a synthesized record.
fn find_device(
    services: &RuntimeServices,
    target: &DeviceId,
) -> Result<Option<WireDeviceSummary>, RpcError> {
    let mut after: Option<DeviceId> = None;
    loop {
        let request =
            DevicePageRequest::new(DEVICE_SCAN_PAGE_SIZE, after).map_err(map_read_error)?;
        let page = services.list_devices(request).map_err(map_read_error)?;
        if let Some(found) = page.devices.iter().find(|d| d.device_id == *target) {
            return Ok(Some(wire_device_summary(found.clone())));
        }
        match page.next_after_device_id {
            Some(next) => after = Some(next),
            None => return Ok(None),
        }
    }
}

/// The agent-mutation submission path shared by `AgentRunRegister`,
/// `AgentRunUpdate`, and `WorkIntentAcknowledge` — see this module's top doc
/// comment for the `principal_id == connection_id` convention and the
/// mandatory kind-check gate this function opens with.
#[allow(clippy::too_many_arguments)]
fn agent_mutation(
    storage: &Storage,
    services: &RuntimeServices,
    principal: &AuthenticatedPrincipal,
    correlation_id: String,
    contract_method: MutationMethod,
    operation_method: OperationMethod,
    target_id: String,
    parameters_digest: String,
    idempotency_key: String,
) -> Result<BusinessRpcResult, RpcError> {
    // Deliverable 3's mandatory, unconditional, early gate: never treat a
    // non-agent principal's `principal_id` as a `connection_id`. This check
    // runs before any storage lookup and before any other field of
    // `principal` is consulted.
    if principal.kind != PrincipalKind::AgentConnection {
        return Err(RpcError::PermissionDenied);
    }
    let connection_id = principal.principal_id.as_str();
    let connection =
        storage
            .get_agent_connection(connection_id)
            .map_err(|err| RpcError::Internal {
                message: err.to_string(),
            })?;
    // An unknown/never-registered connection is a real, honest, explicit
    // rejection — never a default generation (Deliverable 3).
    let Some(connection) = connection else {
        return Err(RpcError::PermissionDenied);
    };
    let idempotency_key =
        IdempotencyKey::new(idempotency_key).map_err(|err| RpcError::InvalidRequest {
            message: err.to_string(),
        })?;
    let request = MutationAcceptanceRequest::new(
        connection_id,
        principal.principal_id.clone(),
        connection.generation,
        contract_method,
        target_id.clone(),
        None,
        parameters_digest,
        idempotency_key,
        None,
    )
    .map_err(|err| RpcError::InvalidRequest {
        message: err.to_string(),
    })?;
    // The real, atomic acceptance transaction — including whether this
    // connection is currently revoked/its generation stale — is evaluated
    // for real inside `submit_mutation` (`loki_acceptance_store::
    // accept_mutation`), never re-derived or second-guessed here.
    let outcome = services
        .submit_mutation(&request)
        .map_err(|err| RpcError::Internal {
            message: err.to_string(),
        })?;
    Ok(BusinessRpcResult::Mutation {
        method: operation_method,
        correlation_id,
        target_id,
        outcome: map_acceptance_result(&outcome),
    })
}

fn invalid_request(message: &str) -> RpcError {
    RpcError::InvalidRequest {
        message: message.to_string(),
    }
}

fn map_read_error(error: RuntimeReadError) -> RpcError {
    match error {
        RuntimeReadError::DeviceNotFound { device_id } => RpcError::InvalidRequest {
            message: format!("device not found: {device_id}"),
        },
        RuntimeReadError::InvalidPageLimit { supplied, maximum } => RpcError::InvalidRequest {
            message: format!("invalid device page limit {supplied}: expected 1..={maximum}"),
        },
        RuntimeReadError::InvalidHealthProjection { device_id, reason } => RpcError::Internal {
            message: format!("invalid persisted health for device {device_id}: {reason}"),
        },
        RuntimeReadError::Storage(err) => RpcError::Internal {
            message: err.to_string(),
        },
    }
}

fn read_meta(correlation_id: String) -> ReadMeta {
    ReadMeta {
        source_runtime_id: SOURCE_RUNTIME_ID.to_string(),
        observed_at: crate::host::clock::now_utc_iso8601(),
        correlation_id,
    }
}

fn wire_storage_readiness(value: StorageReadiness) -> WireStorageReadiness {
    match value {
        StorageReadiness::Ready => WireStorageReadiness::Ready,
        StorageReadiness::Corrupt { problem_count } => WireStorageReadiness::Corrupt {
            problem_count: problem_count as u64,
        },
    }
}

fn wire_mutation_availability(value: MutationAvailability) -> WireMutationAvailability {
    match value {
        MutationAvailability::Available => WireMutationAvailability::Available,
    }
}

fn wire_lifecycle_state(state: DeviceLifecycleState) -> WireDeviceLifecycleState {
    match state {
        DeviceLifecycleState::Discovered => WireDeviceLifecycleState::Discovered,
        DeviceLifecycleState::Identifying => WireDeviceLifecycleState::Identifying,
        DeviceLifecycleState::Identified => WireDeviceLifecycleState::Identified,
        DeviceLifecycleState::Checking => WireDeviceLifecycleState::Checking,
        DeviceLifecycleState::EnrollmentRequired => WireDeviceLifecycleState::EnrollmentRequired,
        DeviceLifecycleState::Unsupported => WireDeviceLifecycleState::Unsupported,
        DeviceLifecycleState::Degraded => WireDeviceLifecycleState::Degraded,
        DeviceLifecycleState::Enrolled => WireDeviceLifecycleState::Enrolled,
        DeviceLifecycleState::Ready => WireDeviceLifecycleState::Ready,
        DeviceLifecycleState::Offline => WireDeviceLifecycleState::Offline,
        DeviceLifecycleState::Busy => WireDeviceLifecycleState::Busy,
    }
}

fn wire_support_tier(tier: SupportTier) -> WireSupportTier {
    match tier {
        SupportTier::Certified => WireSupportTier::Certified,
        SupportTier::Supported => WireSupportTier::Supported,
        SupportTier::Custom => WireSupportTier::Custom,
        SupportTier::Unsupported => WireSupportTier::Unsupported,
    }
}

fn wire_device_summary(summary: DeviceSummary) -> WireDeviceSummary {
    WireDeviceSummary {
        device_id: summary.device_id.into_string(),
        connector_id: summary.connector_id.into_string(),
        connector_version: summary.connector_version,
        family: summary.family,
        board_identity: summary.board_identity,
        runtime_id: summary.runtime_id.into_string(),
        support_tier: wire_support_tier(summary.support_tier),
        lifecycle_state: wire_lifecycle_state(summary.lifecycle_state),
        last_seen_at: summary.last_seen_at,
    }
}

fn wire_health_state(value: DeviceHealthState) -> WireDeviceHealthState {
    match value {
        DeviceHealthState::NotObserved => WireDeviceHealthState::NotObserved,
        DeviceHealthState::Persisted(evidence) => WireDeviceHealthState::Persisted {
            evidence: WirePersistedHealthEvidence {
                payload_size_bytes: evidence.payload_size_bytes as u64,
                last_seen_at: evidence.last_seen_at,
            },
        },
    }
}

/// Exhaustive, no wildcard arm — mirrors `crate::server::method_name`
/// (`loki-ipc`, private to that crate) so a variant added later to
/// `Method` without a case here fails to compile, matching that module's
/// own completeness discipline.
fn method_wire_name(method: &Method) -> &'static str {
    match method {
        Method::Ping(_) => "ping",
        Method::RuntimeGetStatus(_) => "runtime_get_status",
        Method::DevicesList(_) => "devices_list",
        Method::DeviceGet(_) => "device_get",
        Method::DeviceGetHealth(_) => "device_get_health",
        Method::DeviceReadLogs(_) => "device_read_logs",
        Method::DeviceQueryTelemetry(_) => "device_query_telemetry",
        Method::IncidentsList(_) => "incidents_list",
        Method::IncidentGet(_) => "incident_get",
        Method::JobGet(_) => "job_get",
        Method::AgentRunRegister(_) => "agent_run_register",
        Method::AgentRunUpdate(_) => "agent_run_update",
        Method::WorkIntentAcknowledge(_) => "work_intent_acknowledge",
        Method::HealthCheckRequest(_) => "health_check_request",
        Method::PreflightRequest(_) => "preflight_request",
        Method::JobCancel(_) => "job_cancel",
        Method::BuildRequest(_) => "build_request",
        Method::RestartRequest(_) => "restart_request",
        Method::FlashRequest(_) => "flash_request",
        Method::TestOnDeviceRequest(_) => "test_on_device_request",
        Method::DeploymentRequest(_) => "deployment_request",
        Method::RollbackRequest(_) => "rollback_request",
        Method::ApprovalGrant(_) => "approval_grant",
        Method::ApprovalDeny(_) => "approval_deny",
    }
}
