//! Business-RPC test fixtures for `P1-RPC-CONTRACT-001-R1`'s real-pipe
//! adversarial suite.
//!
//! [`TestBusinessService`] is the Deliverable-8 test double: for the six
//! mutation-backed methods it calls the REAL
//! `loki_acceptance_store::accept_mutation` against a real in-memory
//! `loki_storage::Storage`, then maps the result through
//! `loki_ipc::mutation_mapping::map_acceptance_result` — the exact
//! production construction path, not a hand-rolled fixture. For the four
//! read-backed methods it reads through `loki_storage::devices::
//! DeviceRepository` directly (a dev-dependency; production `loki-ipc`
//! never depends on `loki-storage`). Every other method returns
//! `RpcError::UnsupportedMethod`, matching the production
//! `UnsupportedBusinessService` default exactly (see
//! `crate::business`'s top doc comment for why).

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use loki_ipc::auth::{AuthToken, CredentialRecord, CredentialResolver, CredentialSet};
use loki_ipc::business::{
    AuthenticatedPrincipal, Authority, OperationMethod, PrincipalKind, ReadMeta,
    WireDeviceHealthState, WireDeviceLifecycleState, WireDeviceSummary, WireMutationAvailability,
    WirePersistedHealthEvidence, WireStorageReadiness,
};
use loki_ipc::protocol::{Method, RpcError};
use loki_ipc::server::{
    BusinessCallFuture, BusinessEventEmitter, BusinessRequestContext, BusinessService,
    BusinessStreamFuture,
};
use loki_mutation_contract::{IdempotencyKey, MutationAcceptanceRequest, MutationMethod};
use loki_storage::agents::AgentsRepository;
use loki_storage::devices::{DeviceRepository, NewDevice};
use loki_storage::ids::{ConnectorId, DeviceId, RuntimeId};
use loki_storage::local_authority::{LocalAuthorityRepository, NewAuthoritySessionRow};
use loki_storage::states::{DeviceLifecycleState, SupportTier};
use loki_storage::{Cipher, Storage};

pub const DESKTOP_PRINCIPAL_ID: &str = "desktop-full";
pub const AGENT_PRINCIPAL_ID: &str = "agent-full";
pub const DESKTOP_NO_ENTITY_PRINCIPAL_ID: &str = "desktop-no-entity";
pub const FIXTURE_DEVICE_ID: &str = "dev:fixture-1";
pub const OTHER_DEVICE_ID: &str = "dev:not-authorized";
pub const FIXTURE_RUN_ID: &str = "run-fixture-1";
pub const FIXTURE_WORK_INTENT_ID: &str = "wi-fixture-1";
pub const FIXTURE_APPROVAL_ID: &str = "apr-fixture-1";
const AGENT_CONNECTION_ID: &str = "ac_business_test_fixture";
const DESKTOP_LOCAL_AUTHORITY_SID: &str = "S-1-5-21-1111111111-2222222222-3333333333-9100";
const INSTALLATION_ID: &str = "li_business_test_fixture";
const SESSION_ID: &str = "ls_business_test_fixture";

fn all_authorities() -> BTreeSet<Authority> {
    BTreeSet::from([
        Authority::ReadRuntime,
        Authority::ReadDevices,
        Authority::ReadTelemetry,
        Authority::ReadIncidents,
        Authority::ReadJobs,
        Authority::ManageAgentRuns,
        Authority::AcknowledgeWorkIntents,
        Authority::RequestDiagnostics,
        Authority::CancelJobs,
        Authority::RequestBuild,
        Authority::RequestDeviceOperation,
        Authority::DecideApproval,
        Authority::ReadApprovals,
        Authority::ReadNotifications,
        Authority::ReadWorkIntents,
        Authority::Subscribe,
    ])
}

/// A desktop principal authorized for every domain authority and every
/// fixture entity id this module seeds.
pub fn desktop_principal() -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        kind: PrincipalKind::DesktopInstallation,
        principal_id: DESKTOP_PRINCIPAL_ID.to_string(),
        authorized_entity_ids: BTreeSet::from([
            FIXTURE_DEVICE_ID.to_string(),
            FIXTURE_RUN_ID.to_string(),
            FIXTURE_WORK_INTENT_ID.to_string(),
            FIXTURE_APPROVAL_ID.to_string(),
            "incident-1".to_string(),
            "job-1".to_string(),
        ]),
        authority: all_authorities(),
    }
}

/// An agent principal with `ManageAgentRuns`/`AcknowledgeWorkIntents` but
/// not `DecideApproval`/`RequestDeviceOperation` — used for cross-kind
/// approval-decision rejection tests.
pub fn agent_principal() -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        kind: PrincipalKind::AgentConnection,
        principal_id: AGENT_PRINCIPAL_ID.to_string(),
        authorized_entity_ids: BTreeSet::from([
            FIXTURE_RUN_ID.to_string(),
            FIXTURE_WORK_INTENT_ID.to_string(),
        ]),
        authority: BTreeSet::from([
            Authority::ManageAgentRuns,
            Authority::AcknowledgeWorkIntents,
        ]),
    }
}

/// A desktop principal with full domain authority but no authorized entity
/// ids at all — cross-identity rejection fixture.
pub fn desktop_no_entity_principal() -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        kind: PrincipalKind::DesktopInstallation,
        principal_id: DESKTOP_NO_ENTITY_PRINCIPAL_ID.to_string(),
        authorized_entity_ids: BTreeSet::new(),
        authority: all_authorities(),
    }
}

/// Thread-safe wrapper so tests can revoke a credential on the same
/// resolver instance the server holds (`CredentialSet::revoke` takes
/// `&mut self`; `ServerConfig::credential_resolver` is an immutable
/// `Arc<dyn CredentialResolver>`).
#[derive(Clone)]
pub struct SharedCredentialSet(Arc<Mutex<CredentialSet>>);

impl SharedCredentialSet {
    pub fn new(set: CredentialSet) -> Self {
        Self(Arc::new(Mutex::new(set)))
    }

    pub fn revoke(&self, principal_id: &str) {
        self.0.lock().unwrap().revoke(principal_id);
    }
}

impl CredentialResolver for SharedCredentialSet {
    fn resolve(
        &self,
        presented_token: &str,
        now_unix_millis: u64,
    ) -> Option<AuthenticatedPrincipal> {
        self.0
            .lock()
            .unwrap()
            .resolve(presented_token, now_unix_millis)
    }

    fn revalidate(
        &self,
        principal_id: &str,
        now_unix_millis: u64,
    ) -> Option<AuthenticatedPrincipal> {
        self.0
            .lock()
            .unwrap()
            .revalidate(principal_id, now_unix_millis)
    }
}

/// Builds the standard test credential set: `desktop_full_token` ->
/// [`desktop_principal`], `agent_full_token` -> [`agent_principal`],
/// `desktop_no_entity_token` -> [`desktop_no_entity_principal`].
pub fn business_credential_set() -> SharedCredentialSet {
    SharedCredentialSet::new(CredentialSet::new(vec![
        CredentialRecord::active(AuthToken::new("desktop_full_token"), desktop_principal()),
        CredentialRecord::active(AuthToken::new("agent_full_token"), agent_principal()),
        CredentialRecord::active(
            AuthToken::new("desktop_no_entity_token"),
            desktop_no_entity_principal(),
        ),
    ]))
}

/// A real, migrated in-memory `Storage` seeded with: one fixture device
/// (`FIXTURE_DEVICE_ID`), one real agent-connection grant
/// (`AGENT_CONNECTION_ID`, generation 1), and one real, currently-valid
/// local-authority session (`DESKTOP_LOCAL_AUTHORITY_SID`, some generation)
/// for the desktop principal's approval-gated methods.
pub fn seeded_storage() -> Arc<Storage> {
    loki_storage::secure_memory::test_support::ensure_test_arena_ready();
    let storage = Storage::open_in_memory(Cipher::None).expect("open in-memory storage");
    storage
        .run_migrations("2026-08-30T00:00:00.000Z")
        .expect("run migrations");

    storage
        .insert_device(&NewDevice {
            device_id: DeviceId::new(FIXTURE_DEVICE_ID).unwrap(),
            connector_id: ConnectorId::new("connector:business-test").unwrap(),
            connector_version: "1.0.0".into(),
            hardware_identity: "test-qualified-identity".into(),
            family: "test-family".into(),
            board_identity: Some("board-rev-a".into()),
            transport_reference: None,
            runtime_id: RuntimeId::new("runtime:business-test").unwrap(),
            support_tier: SupportTier::Custom,
            lifecycle_state: DeviceLifecycleState::Ready,
            capabilities_json: None,
            firmware_artifact_ref: None,
            config_artifact_ref: None,
            model_artifact_ref: None,
            health_json: Some(r#"{"source":"connector","ok":true}"#.to_string()),
            last_seen_at: "2026-08-30T00:00:00.000Z".into(),
            created_at: "2026-08-30T00:00:00.000Z".into(),
            updated_at: "2026-08-30T00:00:00.000Z".into(),
        })
        .expect("insert fixture device");

    storage
        .insert_agent_connection(
            AGENT_CONNECTION_ID,
            "test-agent",
            "sha256:test-hash",
            "register_runs",
            0,
        )
        .expect("insert agent connection");

    storage
        .insert_installation(&loki_storage::local_authority::InstallationRow {
            installation_id: INSTALLATION_ID.to_string(),
            owner_sid: DESKTOP_LOCAL_AUTHORITY_SID.to_string(),
            created_at_ms: 0,
        })
        .ok();
    let now_ms = loki_acceptance_store::clock::unix_millis_now();
    storage
        .insert_authority_session(&NewAuthoritySessionRow {
            session_id: SESSION_ID.to_string(),
            installation_id: INSTALLATION_ID.to_string(),
            principal_sid: DESKTOP_LOCAL_AUTHORITY_SID.to_string(),
            ipc_session_id: "ipc-business-test-fixture".to_string(),
            correlation_id: "corr-business-test-fixture".to_string(),
            issued_at_ms: now_ms - 1_000,
            expires_at_ms: now_ms + 3_600_000,
        })
        .expect("insert authority session");

    Arc::new(storage)
}

/// The Deliverable-8 test double.
pub struct TestBusinessService {
    storage: Arc<Storage>,
    /// Only set by [`TestBusinessService::new_with_live_stream_gate`] — a
    /// real, ordered two-event `BusinessEvents` producer used by the
    /// mid-subscription-revocation coverage test. Every other test double
    /// instance leaves this `None` and stays fail-closed for all 10
    /// streams, exactly like production `UnsupportedBusinessService` (see
    /// the doc comment on `stream()` below).
    live_stream_gate: Option<Arc<tokio::sync::Notify>>,
}

impl TestBusinessService {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            live_stream_gate: None,
        }
    }

    /// A test double that, for exactly one live `BusinessEvents`
    /// subscription, emits one real event immediately (proving live
    /// delivery), then waits for the returned [`tokio::sync::Notify`] to be
    /// notified before emitting a second real event — giving the calling
    /// test a deterministic window to revoke the credential between the
    /// first delivered event and the second forwarded one. Used only by
    /// `mid_subscription_revocation_denies_the_very_next_forwarded_event`.
    pub fn new_with_live_stream_gate(storage: Arc<Storage>) -> (Self, Arc<tokio::sync::Notify>) {
        let gate = Arc::new(tokio::sync::Notify::new());
        (
            Self {
                storage,
                live_stream_gate: Some(Arc::clone(&gate)),
            },
            gate,
        )
    }

    /// Resolves the real `(principal_sid, grant_generation)` this test
    /// double's seeded storage recognizes for `principal`, per mutation
    /// method's identity model — agent methods key off the seeded agent
    /// connection; approval-gated device methods key off the seeded
    /// local-authority session. Production composition
    /// (`P1-RUNTIME-COMP-001`) resolves this from the real session/grant
    /// state instead of a fixture map.
    fn grant_identity(
        &self,
        method: MutationMethod,
        principal: &AuthenticatedPrincipal,
    ) -> (String, u64) {
        match method {
            MutationMethod::AgentRunRegister
            | MutationMethod::AgentRunUpdate
            | MutationMethod::WorkIntentAcknowledge => {
                if principal.principal_id == AGENT_PRINCIPAL_ID {
                    (AGENT_CONNECTION_ID.to_string(), 1)
                } else {
                    // No seeded agent-connection row for this principal:
                    // deliberately produces a real `GrantInvalid`
                    // rejection, not a fabricated success — used by
                    // `agent_run_register_with_no_seeded_grant_surfaces_a_real_rejection_not_a_fabricated_success`.
                    ("ac_unseeded".to_string(), 1)
                }
            }
            MutationMethod::Restart | MutationMethod::Flash | MutationMethod::ContainedBuild => {
                if principal.principal_id == DESKTOP_PRINCIPAL_ID {
                    (DESKTOP_LOCAL_AUTHORITY_SID.to_string(), 1)
                } else {
                    // No seeded session for this principal: deliberately
                    // produces a real `GrantInvalid` rejection, not a
                    // fabricated success.
                    ("S-1-5-21-unseeded".to_string(), 1)
                }
            }
            MutationMethod::ApplyConfig => unreachable!("no wire method targets ApplyConfig"),
        }
    }

    #[allow(clippy::too_many_arguments)] // test fixture; explicit closed field list mirrors MutationAcceptanceRequest::new
    async fn submit(
        &self,
        contract_method: MutationMethod,
        target_id: String,
        artifact_digest: Option<String>,
        parameters_digest: String,
        idempotency_key: String,
        approval_reference: Option<loki_mutation_contract::ApprovalReference>,
        principal: &AuthenticatedPrincipal,
    ) -> Result<loki_ipc::business::WireMutationOutcome, RpcError> {
        let (principal_sid, grant_generation) = self.grant_identity(contract_method, principal);
        let request = MutationAcceptanceRequest::new(
            principal_sid,
            principal.principal_id.clone(),
            grant_generation,
            contract_method,
            target_id,
            artifact_digest,
            parameters_digest,
            IdempotencyKey::new(idempotency_key).map_err(|err| RpcError::InvalidRequest {
                message: err.to_string(),
            })?,
            approval_reference,
        )
        .map_err(|err| RpcError::InvalidRequest {
            message: err.to_string(),
        })?;
        let result = loki_acceptance_store::accept_mutation(self.storage.as_ref(), &request)
            .map_err(|err| RpcError::Internal {
                message: err.to_string(),
            })?;
        Ok(loki_ipc::mutation_mapping::map_acceptance_result(&result))
    }

    fn device_summary(&self, device_id: &str) -> Option<WireDeviceSummary> {
        let record = self
            .storage
            .get_device(&DeviceId::new(device_id).ok()?)
            .ok()??;
        Some(WireDeviceSummary {
            device_id: record.device_id.as_str().to_string(),
            connector_id: record.connector_id.as_str().to_string(),
            connector_version: record.connector_version,
            family: record.family,
            board_identity: record.board_identity,
            runtime_id: record.runtime_id.as_str().to_string(),
            support_tier: wire_support_tier(record.support_tier),
            lifecycle_state: wire_lifecycle_state(record.lifecycle_state),
            last_seen_at: record.last_seen_at,
        })
    }

    fn read_meta(&self, correlation_id: String) -> ReadMeta {
        ReadMeta {
            source_runtime_id: "runtime:business-test".to_string(),
            observed_at: "2026-08-30T00:00:00.000Z".to_string(),
            correlation_id,
        }
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

fn wire_support_tier(tier: SupportTier) -> loki_ipc::business::WireSupportTier {
    match tier {
        SupportTier::Certified => loki_ipc::business::WireSupportTier::Certified,
        SupportTier::Supported => loki_ipc::business::WireSupportTier::Supported,
        SupportTier::Custom => loki_ipc::business::WireSupportTier::Custom,
        SupportTier::Unsupported => loki_ipc::business::WireSupportTier::Unsupported,
    }
}

impl BusinessService for TestBusinessService {
    fn call(&self, method: Method, context: BusinessRequestContext) -> BusinessCallFuture {
        let storage = Arc::clone(&self.storage);
        Box::pin(async move {
            let service = TestBusinessService {
                storage,
                live_stream_gate: None,
            };
            let correlation_id = context.correlation_id.to_string();
            match method {
                Method::RuntimeGetStatus(_) => {
                    let registered_device_count = service
                        .storage
                        .list_devices()
                        .map_err(|err| RpcError::Internal {
                            message: err.to_string(),
                        })?
                        .len() as u64;
                    Ok(loki_ipc::business::BusinessRpcResult::RuntimeStatus {
                        meta: service.read_meta(correlation_id),
                        storage: WireStorageReadiness::Ready,
                        registered_device_count,
                        mutations: WireMutationAvailability::Available,
                    })
                }
                Method::DevicesList(params) => {
                    let records =
                        service
                            .storage
                            .list_devices()
                            .map_err(|err| RpcError::Internal {
                                message: err.to_string(),
                            })?;
                    let total_registered_count = records.len() as u64;
                    let devices = records
                        .into_iter()
                        .filter(|r| {
                            params
                                .after_device_id
                                .as_deref()
                                .is_none_or(|after| r.device_id.as_str() > after)
                        })
                        .take(usize::from(params.limit))
                        .filter_map(|r| service.device_summary(r.device_id.as_str()))
                        .collect::<Vec<_>>();
                    Ok(loki_ipc::business::BusinessRpcResult::DevicesList {
                        meta: service.read_meta(correlation_id),
                        devices,
                        next_after_device_id: None,
                        total_registered_count,
                    })
                }
                Method::DeviceGet(params) => Ok(loki_ipc::business::BusinessRpcResult::DeviceGet {
                    meta: service.read_meta(correlation_id),
                    device: service.device_summary(&params.device_id),
                }),
                Method::DeviceGetHealth(params) => {
                    let record = service
                        .storage
                        .get_device(&DeviceId::new(&params.device_id).map_err(|_| {
                            RpcError::InvalidRequest {
                                message: "invalid device id".to_string(),
                            }
                        })?)
                        .map_err(|err| RpcError::Internal {
                            message: err.to_string(),
                        })?
                        .ok_or(RpcError::UnsupportedMethod {
                            method: "device_get_health".to_string(),
                        })?;
                    let health = match record.health_json {
                        None => WireDeviceHealthState::NotObserved,
                        Some(doc) => WireDeviceHealthState::Persisted {
                            evidence: WirePersistedHealthEvidence {
                                payload_size_bytes: doc.len() as u64,
                                last_seen_at: record.last_seen_at.clone(),
                            },
                        },
                    };
                    Ok(loki_ipc::business::BusinessRpcResult::DeviceHealth {
                        meta: service.read_meta(correlation_id),
                        device_id: record.device_id.as_str().to_string(),
                        lifecycle_state: wire_lifecycle_state(record.lifecycle_state),
                        health,
                    })
                }
                Method::RestartRequest(params) => {
                    let outcome = service
                        .submit(
                            MutationMethod::Restart,
                            params.device_id.clone(),
                            None,
                            params.parameters_digest,
                            params.idempotency_key,
                            approval_reference(params.approval_reference),
                            &context.principal,
                        )
                        .await?;
                    Ok(mutation_result(
                        OperationMethod::Restart,
                        correlation_id,
                        params.device_id,
                        outcome,
                    ))
                }
                Method::FlashRequest(params) => {
                    let outcome = service
                        .submit(
                            MutationMethod::Flash,
                            params.device_id.clone(),
                            Some(params.artifact_digest),
                            params.parameters_digest,
                            params.idempotency_key,
                            approval_reference(params.approval_reference),
                            &context.principal,
                        )
                        .await?;
                    Ok(mutation_result(
                        OperationMethod::Flash,
                        correlation_id,
                        params.device_id,
                        outcome,
                    ))
                }
                Method::BuildRequest(params) => {
                    let outcome = service
                        .submit(
                            MutationMethod::ContainedBuild,
                            params.target_id.clone(),
                            None,
                            params.parameters_digest,
                            params.idempotency_key,
                            approval_reference(params.approval_reference),
                            &context.principal,
                        )
                        .await?;
                    Ok(mutation_result(
                        OperationMethod::ContainedBuild,
                        correlation_id,
                        params.target_id,
                        outcome,
                    ))
                }
                Method::AgentRunRegister(params) => {
                    let target = params
                        .parent_run_id
                        .clone()
                        .unwrap_or_else(|| "agent-run:root".to_string());
                    let outcome = service
                        .submit(
                            MutationMethod::AgentRunRegister,
                            target.clone(),
                            None,
                            params.parameters_digest,
                            params.idempotency_key,
                            None,
                            &context.principal,
                        )
                        .await?;
                    Ok(mutation_result(
                        OperationMethod::AgentRunRegister,
                        correlation_id,
                        target,
                        outcome,
                    ))
                }
                Method::AgentRunUpdate(params) => {
                    let outcome = service
                        .submit(
                            MutationMethod::AgentRunUpdate,
                            params.run_id.clone(),
                            None,
                            params.parameters_digest,
                            params.idempotency_key,
                            None,
                            &context.principal,
                        )
                        .await?;
                    Ok(mutation_result(
                        OperationMethod::AgentRunUpdate,
                        correlation_id,
                        params.run_id,
                        outcome,
                    ))
                }
                Method::WorkIntentAcknowledge(params) => {
                    let outcome = service
                        .submit(
                            MutationMethod::WorkIntentAcknowledge,
                            params.work_intent_id.clone(),
                            None,
                            params.parameters_digest,
                            params.idempotency_key,
                            None,
                            &context.principal,
                        )
                        .await?;
                    Ok(mutation_result(
                        OperationMethod::WorkIntentAcknowledge,
                        correlation_id,
                        params.work_intent_id,
                        outcome,
                    ))
                }
                other => Err(RpcError::UnsupportedMethod {
                    method: format!("{other:?}"),
                }),
            }
        })
    }

    fn stream(
        &self,
        params: loki_ipc::business::BusinessStreamParams,
        context: BusinessRequestContext,
        emitter: BusinessEventEmitter,
    ) -> BusinessStreamFuture {
        // No real event producer exists anywhere in this workspace yet
        // (see `crate::business`'s top doc comment) — authorization is
        // fully provable without one; this test double stays fail-closed
        // for all 10 streams, exactly like production
        // `UnsupportedBusinessService`, EXCEPT when constructed via
        // `new_with_live_stream_gate`, which installs a real, deterministic
        // two-event producer used only by the mid-subscription-revocation
        // coverage test.
        if let Some(gate) = self.live_stream_gate.clone() {
            let correlation_id = context.correlation_id.to_string();
            let source_runtime_id = "runtime:business-test".to_string();
            return Box::pin(async move {
                let make_event = |sequence: u64, state: &str| loki_ipc::business::BusinessEvent {
                    stream: params.stream,
                    sequence,
                    correlation_id: correlation_id.clone(),
                    occurred_at: "2026-08-30T00:00:00.000Z".to_string(),
                    entity_id: params.entity_id.clone(),
                    state: state.to_string(),
                    source_runtime_id: source_runtime_id.clone(),
                };
                emitter.send(make_event(1, "heartbeat-1")).await?;
                // Give the calling test a deterministic window to revoke
                // the credential after observing the first, real, delivered
                // event and before this second event is forwarded.
                gate.notified().await;
                emitter.send(make_event(2, "heartbeat-2")).await?;
                Ok(loki_ipc::business::BusinessStreamSummary {
                    items_sent: 2,
                    last_sequence: Some(2),
                })
            });
        }
        Box::pin(async {
            Err(RpcError::UnsupportedMethod {
                method: "business_events".to_string(),
            })
        })
    }
}

fn approval_reference(
    params: Option<loki_ipc::business::ApprovalReferenceParams>,
) -> Option<loki_mutation_contract::ApprovalReference> {
    params.map(|p| loki_mutation_contract::ApprovalReference {
        approval_id: p.approval_id,
        bound_request_digest: loki_mutation_contract::digest::RequestDigest::try_from_str(
            p.bound_request_digest,
        )
        .expect("test fixture supplies a well-formed digest"),
        expires_at_ms: p.expires_at_ms,
    })
}

fn mutation_result(
    method: OperationMethod,
    correlation_id: String,
    target_id: String,
    outcome: loki_ipc::business::WireMutationOutcome,
) -> loki_ipc::business::BusinessRpcResult {
    loki_ipc::business::BusinessRpcResult::Mutation {
        method,
        correlation_id,
        target_id,
        outcome,
    }
}
