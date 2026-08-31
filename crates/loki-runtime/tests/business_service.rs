//! `ComposedHost`-level integration tests for the real
//! `loki_ipc::server::BusinessService` implementation
//! (`crate::host::business_service::RuntimeBusinessService`,
//! `P1-RUNTIME-COMP-001`, `Docs/graph/tasks/P1-RUNTIME-COMP-001.md`).
//!
//! Every test here drives real, authorized business calls **through the
//! actual production `ComposedHost::run` wiring** — real named pipe, real
//! `Client`, real shutdown sequencing — using
//! `ComposedHost::open_with_test_arena_and_credential_resolver` to install a
//! test-constructed `CredentialSet` while keeping the real
//! `RuntimeBusinessService` installed (never a second `Server`/second
//! database open). This matches the testing approach
//! `P1-IPC-SHUTDOWN-CORR`'s independent-review re-verification established
//! (`tests/host_lifecycle.rs`'s `slow_business_call` module): testing only
//! `loki_ipc::Server::serve` in isolation, or a second, independently-bound
//! `Server` outside `ComposedHost::run`'s own lifecycle, does not exercise
//! the real shutdown-drain guarantees a live client actually depends on.

#![cfg(windows)]

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use loki_ipc::auth::{AuthToken, CredentialRecord, CredentialResolver, CredentialSet};
use loki_ipc::business::{
    AgentRunRegisterParams, AgentRunUpdateParams, ApprovalReferenceParams, AuthenticatedPrincipal,
    Authority, BuildRequestParams, BusinessRpcResult, DeviceListParams, DeviceParams, EmptyParams,
    FlashRequestParams, PrincipalKind, RestartRequestParams, WireMutationOutcome,
    WorkIntentAckParams,
};
use loki_ipc::client::{CallError, Client, ClientConfig};
use loki_ipc::protocol::{Method, RpcError};
use loki_runtime::host::compose::{ComposedHost, HostConfig};
use loki_runtime::host::paths::HostPaths;
use loki_runtime::runtime::RunOutcome;
use loki_runtime::shutdown::ShutdownReason;
use loki_storage::agents::AgentsRepository;
use loki_storage::devices::{DeviceRepository, NewDevice};
use loki_storage::ids::{ConnectorId, DeviceId, RuntimeId};
use loki_storage::states::{DeviceLifecycleState, SupportTier};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);
/// A bound for assertions that must not hang the suite if the implementation
/// regresses into a real deadlock — not a normal-operation timing
/// assumption. Comfortably above `loki_ipc::server`'s own internal shutdown-
/// drain grace periods (`CONNECTION_DRAIN_GRACE` = 20s), matching
/// `tests/host_lifecycle.rs`'s own established convention for a real
/// `ComposedHost::run` round trip.
const HANG_GUARD: Duration = Duration::from_secs(30);

const FIXTURE_DEVICE_ID: &str = "dev:business-service-fixture-1";
/// The real, persisted `agent_connections.connection_id` this suite seeds.
/// Also used, unmodified, as the agent principal's `principal_id` — the
/// exact `principal_id == connection_id` convention
/// `crate::host::business_service` documents.
const AGENT_CONNECTION_ID: &str = "ac_business_service_fixture_1";
const UNREGISTERED_CONNECTION_ID: &str = "ac_business_service_never_registered";
const DESKTOP_PRINCIPAL_ID: &str = "desktop-business-service-fixture";
const FIXTURE_RUN_ID: &str = "run-business-service-fixture-1";
const FIXTURE_WORK_INTENT_ID: &str = "wi-business-service-fixture-1";

fn unique_pipe_name(label: &str) -> String {
    format!(
        r"\\.\pipe\loki-runtime-business-service-test-{label}-{}",
        uuid::Uuid::new_v4()
    )
}

fn test_host_config(dir: &std::path::Path, label: &str) -> HostConfig {
    HostConfig {
        paths: HostPaths::new(dir),
        pipe_name: unique_pipe_name(label),
    }
}

fn sample_device(device_id: &str) -> NewDevice {
    NewDevice {
        device_id: DeviceId::new(device_id).unwrap(),
        connector_id: ConnectorId::new("connector:business-service-test").unwrap(),
        connector_version: "1.0.0".to_string(),
        hardware_identity: "test-qualified-identity".to_string(),
        family: "test-family".to_string(),
        board_identity: Some("board-rev-a".to_string()),
        transport_reference: None,
        runtime_id: RuntimeId::new("runtime:business-service-test").unwrap(),
        support_tier: SupportTier::Custom,
        lifecycle_state: DeviceLifecycleState::Ready,
        capabilities_json: None,
        firmware_artifact_ref: None,
        config_artifact_ref: None,
        model_artifact_ref: None,
        health_json: Some(r#"{"source":"connector","ok":true}"#.to_string()),
        last_seen_at: "2026-08-31T00:00:00Z".to_string(),
        created_at: "2026-08-31T00:00:00Z".to_string(),
        updated_at: "2026-08-31T00:00:00Z".to_string(),
    }
}

/// Seeds the fixture device and the fixture agent connection (real,
/// currently-registered, `generation == 1`) directly through
/// `loki_storage`, mirroring `crates/loki-ipc/tests/support/business.rs`'s
/// established seeding convention. Called on the still-owned `ComposedHost`
/// before it is moved into `spawn_run`, via its existing `storage()`
/// accessor.
fn seed_fixtures(host: &ComposedHost) {
    host.storage()
        .insert_device(&sample_device(FIXTURE_DEVICE_ID))
        .expect("insert fixture device");
    host.storage()
        .insert_agent_connection(
            AGENT_CONNECTION_ID,
            "test-agent",
            "sha256:test-hash",
            "register_runs",
            0,
        )
        .expect("insert fixture agent connection");
}

/// A real `AgentConnection`-kind principal whose `principal_id` is exactly
/// the seeded, real `AGENT_CONNECTION_ID` — the convention this suite
/// exercises.
fn agent_principal() -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        kind: PrincipalKind::AgentConnection,
        principal_id: AGENT_CONNECTION_ID.to_string(),
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

/// A `DesktopInstallation`-kind principal with full read/device-operation
/// authority over the fixture device, and no agent-mutation authority of
/// its own (`ManageAgentRuns`/`AcknowledgeWorkIntents` are deliberately
/// absent — a real desktop installation is never issued them; the
/// adversarial test below grants them explicitly to isolate the kind check
/// from an authority-set difference).
fn desktop_principal() -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        kind: PrincipalKind::DesktopInstallation,
        principal_id: DESKTOP_PRINCIPAL_ID.to_string(),
        authorized_entity_ids: BTreeSet::from([FIXTURE_DEVICE_ID.to_string()]),
        authority: BTreeSet::from([
            Authority::ReadRuntime,
            Authority::ReadDevices,
            Authority::RequestDeviceOperation,
            Authority::RequestBuild,
        ]),
    }
}

/// Deliverable-3 security-regression fixture: a `DesktopInstallation`-kind
/// principal whose `principal_id` is deliberately set equal to the real,
/// currently-registered `AGENT_CONNECTION_ID` — and which is (unrealistically
/// generously) granted the exact agent-mutation authorities a real agent
/// principal would have, so a failure to deny it could only be explained by
/// the missing kind check, never by a missing `Authority`.
fn colliding_desktop_principal() -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        kind: PrincipalKind::DesktopInstallation,
        principal_id: AGENT_CONNECTION_ID.to_string(),
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

/// A real `AgentConnection`-kind principal whose `principal_id` names a
/// connection that was never seeded — the "unregistered connection" fixture.
fn unregistered_agent_principal() -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        kind: PrincipalKind::AgentConnection,
        principal_id: UNREGISTERED_CONNECTION_ID.to_string(),
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

/// A `DesktopInstallation`-kind principal authorized for nothing at all —
/// used by the "unauthorized principal denied before reaching the real
/// service" failure case.
fn unauthorized_principal() -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        kind: PrincipalKind::DesktopInstallation,
        principal_id: "desktop-business-service-unauthorized".to_string(),
        authorized_entity_ids: BTreeSet::new(),
        authority: BTreeSet::new(),
    }
}

const AGENT_TOKEN: &str = "agent-fixture-token";
const DESKTOP_TOKEN: &str = "desktop-fixture-token";
const COLLIDING_DESKTOP_TOKEN: &str = "colliding-desktop-fixture-token";
const UNREGISTERED_AGENT_TOKEN: &str = "unregistered-agent-fixture-token";
const UNAUTHORIZED_TOKEN: &str = "unauthorized-fixture-token";

/// Builds a `CredentialSet`-backed resolver from exactly the records a given
/// test needs. Deliberately **not** one single shared resolver holding every
/// fixture principal at once: `crate::server::SessionAuthority` revalidates
/// every call by `principal_id` alone (never the raw token — see
/// `crates/loki-ipc/src/server.rs`'s own top doc comment on dynamic session
/// revalidation), so `CredentialResolver::revalidate`/`CredentialSet`'s
/// lookup is only well-defined when `principal_id` is unique *within one
/// resolver*. [`colliding_desktop_principal`] deliberately shares its
/// `principal_id` with [`agent_principal`]'s (`AGENT_CONNECTION_ID` — the
/// real seeded `connection_id` under test) as the whole point of the
/// adversarial test below; putting both principals in the *same* resolver
/// would make `revalidate` itself ambiguous (a `CredentialSet` test-fixture
/// hazard, not anything `RuntimeBusinessService`/`crate::server` gets wrong)
/// and silently resolve to the wrong principal instead of exercising the
/// kind-check gate at all. Each test therefore builds its own minimal
/// resolver containing only the principal(s) it actually needs.
fn resolver(records: Vec<CredentialRecord>) -> Arc<dyn CredentialResolver> {
    Arc::new(CredentialSet::new(records))
}

fn startup_resolver() -> Arc<dyn CredentialResolver> {
    resolver(vec![
        CredentialRecord::active(AuthToken::new(AGENT_TOKEN), agent_principal()),
        CredentialRecord::active(AuthToken::new(DESKTOP_TOKEN), desktop_principal()),
        CredentialRecord::active(
            AuthToken::new(UNREGISTERED_AGENT_TOKEN),
            unregistered_agent_principal(),
        ),
    ])
}

fn collision_resolver() -> Arc<dyn CredentialResolver> {
    resolver(vec![CredentialRecord::active(
        AuthToken::new(COLLIDING_DESKTOP_TOKEN),
        colliding_desktop_principal(),
    )])
}

fn deferred_methods_resolver() -> Arc<dyn CredentialResolver> {
    resolver(vec![CredentialRecord::active(
        AuthToken::new(DESKTOP_TOKEN),
        desktop_principal(),
    )])
}

fn unauthorized_resolver() -> Arc<dyn CredentialResolver> {
    resolver(vec![CredentialRecord::active(
        AuthToken::new(UNAUTHORIZED_TOKEN),
        unauthorized_principal(),
    )])
}

/// Both `agent_principal`/`desktop_principal` at once — safe to combine (no
/// shared `principal_id`, unlike [`colliding_desktop_principal`]) — used by
/// the restart/reconcile test, which needs a real agent-mutation caller and
/// a real `Authority::ReadDevices`-holding caller (the agent principal
/// deliberately does not hold `ReadDevices`, matching production's expected
/// authority separation).
fn restart_reconcile_resolver() -> Arc<dyn CredentialResolver> {
    resolver(vec![
        CredentialRecord::active(AuthToken::new(AGENT_TOKEN), agent_principal()),
        CredentialRecord::active(AuthToken::new(DESKTOP_TOKEN), desktop_principal()),
    ])
}

/// Opens a real host with the real `RuntimeBusinessService` and
/// `credential_resolver` installed, through
/// `ComposedHost::open_with_test_arena_and_credential_resolver` — never a
/// second database open, never a second `Server`.
fn open_host(
    config: &HostConfig,
    credential_resolver: Arc<dyn CredentialResolver>,
) -> ComposedHost {
    ComposedHost::open_with_test_arena_and_credential_resolver(config, credential_resolver)
        .expect("host must start with the test credential resolver installed")
}

/// Spawns `host.run(...)`, returning the task handle and a sender that
/// triggers a real, clean shutdown when signalled.
fn spawn_run(
    host: ComposedHost,
) -> (
    tokio::task::JoinHandle<RunOutcome>,
    tokio::sync::oneshot::Sender<()>,
) {
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let run_task = tokio::spawn(async move {
        host.run(
            async {
                let _ = shutdown_rx.await;
                ShutdownReason::CtrlC
            },
            std::future::pending::<()>(),
        )
        .await
    });
    (run_task, shutdown_tx)
}

/// Signals a clean shutdown and awaits `ComposedHost::run`'s task to
/// completion — the real teardown path `P1-IPC-SHUTDOWN-CORR` fixed. Every
/// test in this file calls this before its own function returns, so no
/// spawned connection/call task or `RuntimeBusinessService`
/// `Storage`/`RuntimeServices` clone ever outlives the test.
async fn stop_host(
    run_task: tokio::task::JoinHandle<RunOutcome>,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
) {
    let _ = shutdown_tx.send(());
    let outcome = tokio::time::timeout(HANG_GUARD, run_task)
        .await
        .expect("host run task must not hang")
        .expect("host run task must not panic");
    assert!(
        matches!(outcome, RunOutcome::Shutdown(_)),
        "expected a clean shutdown outcome, got {outcome:?}"
    );
}

async fn connect_as(pipe_name: &str, token: &str) -> Client {
    let config = ClientConfig::new(
        pipe_name.to_string(),
        AuthToken::new(token),
        "business-service-test-client",
    );
    tokio::time::timeout(HANG_GUARD, Client::connect(config))
        .await
        .expect("connect must not hang")
        .expect("connect as fixture principal must succeed")
}

async fn call(client: &Client, method: Method) -> Result<BusinessRpcResult, CallError> {
    tokio::time::timeout(HANG_GUARD, client.business_call(method, CALL_TIMEOUT))
        .await
        .expect("business call must not hang")
}

fn agent_run_register(idempotency_key: &str) -> Method {
    Method::AgentRunRegister(AgentRunRegisterParams {
        parent_run_id: None,
        parameters_digest: "params-digest-fixture-01".to_string(),
        idempotency_key: idempotency_key.to_string(),
    })
}

fn agent_run_update(idempotency_key: &str) -> Method {
    Method::AgentRunUpdate(AgentRunUpdateParams {
        run_id: FIXTURE_RUN_ID.to_string(),
        phase: "working".to_string(),
        summary: "in progress".to_string(),
        parameters_digest: "params-digest-fixture-02".to_string(),
        idempotency_key: idempotency_key.to_string(),
    })
}

fn work_intent_acknowledge(idempotency_key: &str) -> Method {
    Method::WorkIntentAcknowledge(WorkIntentAckParams {
        work_intent_id: FIXTURE_WORK_INTENT_ID.to_string(),
        run_id: FIXTURE_RUN_ID.to_string(),
        parameters_digest: "params-digest-fixture-03".to_string(),
        idempotency_key: idempotency_key.to_string(),
    })
}

fn restart_request() -> Method {
    Method::RestartRequest(RestartRequestParams {
        device_id: FIXTURE_DEVICE_ID.to_string(),
        parameters_digest: "params-digest-fixture-restart".to_string(),
        idempotency_key: "idem-restart-1".to_string(),
        approval_reference: Some(ApprovalReferenceParams {
            approval_id: "apr-well-formed".to_string(),
            bound_request_digest: "sha256:".to_string() + &"a".repeat(64),
            expires_at_ms: 9_999_999_999_999,
        }),
    })
}

fn flash_request() -> Method {
    Method::FlashRequest(FlashRequestParams {
        device_id: FIXTURE_DEVICE_ID.to_string(),
        artifact_id: "artifact-1".to_string(),
        artifact_digest: "b".repeat(64),
        parameters_digest: "params-digest-fixture-flash".to_string(),
        idempotency_key: "idem-flash-1".to_string(),
        approval_reference: Some(ApprovalReferenceParams {
            approval_id: "apr-well-formed".to_string(),
            bound_request_digest: "sha256:".to_string() + &"c".repeat(64),
            expires_at_ms: 9_999_999_999_999,
        }),
    })
}

fn build_request() -> Method {
    Method::BuildRequest(BuildRequestParams {
        source_snapshot_id: "snap-1".to_string(),
        target_id: FIXTURE_DEVICE_ID.to_string(),
        recipe_id: "recipe-1".to_string(),
        parameters_digest: "params-digest-fixture-build".to_string(),
        idempotency_key: "idem-build-1".to_string(),
        approval_reference: Some(ApprovalReferenceParams {
            approval_id: "apr-well-formed".to_string(),
            bound_request_digest: "sha256:".to_string() + &"d".repeat(64),
            expires_at_ms: 9_999_999_999_999,
        }),
    })
}

/// Startup: a real client Hello, `Ping`, each of the 4 real reads, and each
/// of the 3 agent-mutation methods (including one real rejection case: an
/// unregistered connection) round-trip over a real named pipe against a real
/// `ComposedHost::run`.
#[tokio::test]
async fn startup_serves_ping_all_four_reads_and_all_three_agent_mutations_over_a_real_pipe() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_host_config(dir.path(), "startup");
    let pipe_name = config.pipe_name.clone();

    let host = open_host(&config, startup_resolver());
    seed_fixtures(&host);
    let (run_task, shutdown_tx) = spawn_run(host);

    let agent_client = connect_as(&pipe_name, AGENT_TOKEN).await;

    let pong = tokio::time::timeout(
        HANG_GUARD,
        agent_client.ping("business-service-test", Duration::from_secs(3)),
    )
    .await
    .expect("ping must not hang")
    .expect("ping should succeed");
    assert_eq!(pong.payload, "business-service-test");

    let desktop_client = connect_as(&pipe_name, DESKTOP_TOKEN).await;

    let status = call(&desktop_client, Method::RuntimeGetStatus(EmptyParams {}))
        .await
        .expect("runtime_get_status must succeed");
    assert!(matches!(status, BusinessRpcResult::RuntimeStatus { .. }));

    let devices = call(
        &desktop_client,
        Method::DevicesList(DeviceListParams {
            limit: 10,
            after_device_id: None,
        }),
    )
    .await
    .expect("devices_list must succeed");
    match devices {
        BusinessRpcResult::DevicesList { devices, .. } => {
            assert!(devices.iter().any(|d| d.device_id == FIXTURE_DEVICE_ID));
        }
        other => panic!("expected DevicesList, got {other:?}"),
    }

    let device = call(
        &desktop_client,
        Method::DeviceGet(DeviceParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
        }),
    )
    .await
    .expect("device_get must succeed");
    match device {
        BusinessRpcResult::DeviceGet { device, .. } => {
            let device = device.expect("fixture device must be found, not a synthesized None");
            assert_eq!(device.device_id, FIXTURE_DEVICE_ID);
        }
        other => panic!("expected DeviceGet, got {other:?}"),
    }

    let health = call(
        &desktop_client,
        Method::DeviceGetHealth(DeviceParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
        }),
    )
    .await
    .expect("device_get_health must succeed");
    assert!(matches!(health, BusinessRpcResult::DeviceHealth { .. }));

    // Real agent-mutation round trips, over the real agent connection this
    // suite seeded — connecting as the agent principal whose `principal_id`
    // equals the real, seeded `AGENT_CONNECTION_ID`.
    let register = call(&agent_client, agent_run_register("idem-register-1"))
        .await
        .expect("agent_run_register must succeed");
    let BusinessRpcResult::Mutation {
        outcome: WireMutationOutcome::Accepted { .. },
        ..
    } = register
    else {
        panic!("expected Accepted, got {register:?}");
    };

    let update = call(&agent_client, agent_run_update("idem-update-1"))
        .await
        .expect("agent_run_update must succeed");
    let BusinessRpcResult::Mutation {
        outcome: WireMutationOutcome::Accepted { .. },
        ..
    } = update
    else {
        panic!("expected Accepted, got {update:?}");
    };

    let acknowledge = call(&agent_client, work_intent_acknowledge("idem-ack-1"))
        .await
        .expect("work_intent_acknowledge must succeed");
    let BusinessRpcResult::Mutation {
        outcome: WireMutationOutcome::Accepted { .. },
        ..
    } = acknowledge
    else {
        panic!("expected Accepted, got {acknowledge:?}");
    };

    // The real rejection case: an unregistered connection is denied honestly
    // — a fresh connection using the unregistered-agent fixture principal.
    let unregistered_client = connect_as(&pipe_name, UNREGISTERED_AGENT_TOKEN).await;
    let denied = call(
        &unregistered_client,
        agent_run_register("idem-unregistered-1"),
    )
    .await;
    assert!(
        matches!(denied, Err(CallError::Rpc(RpcError::PermissionDenied))),
        "expected PermissionDenied for an unregistered connection, got {denied:?}"
    );

    drop(agent_client);
    drop(desktop_client);
    drop(unregistered_client);
    stop_host(run_task, shutdown_tx).await;
}

/// Deliverable-3 security regression: a `DesktopInstallation`-kind principal
/// whose `principal_id` collides with a real, currently-registered agent
/// `connection_id` must be denied `AgentRunRegister`/`AgentRunUpdate`/
/// `WorkIntentAcknowledge` with `PermissionDenied` — never silently served
/// against the other principal's connection. The colliding principal is
/// (unrealistically) granted the exact agent-mutation authorities a real
/// agent principal would have, so only the missing kind check can explain a
/// denial here.
#[tokio::test]
async fn desktop_principal_colliding_with_a_real_connection_id_is_denied_not_served() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_host_config(dir.path(), "collision");
    let pipe_name = config.pipe_name.clone();

    let host = open_host(&config, collision_resolver());
    seed_fixtures(&host);
    let (run_task, shutdown_tx) = spawn_run(host);

    let client = connect_as(&pipe_name, COLLIDING_DESKTOP_TOKEN).await;

    for method in [
        agent_run_register("idem-collision-register"),
        agent_run_update("idem-collision-update"),
        work_intent_acknowledge("idem-collision-ack"),
    ] {
        let result = call(&client, method.clone()).await;
        assert!(
            matches!(result, Err(CallError::Rpc(RpcError::PermissionDenied))),
            "expected PermissionDenied for colliding DesktopInstallation principal calling \
             {method:?}, got {result:?}"
        );
    }

    drop(client);
    stop_host(run_task, shutdown_tx).await;
}

/// Failure case 1 (Deliverable 4): `RestartRequest`/`FlashRequest`/
/// `BuildRequest` each return `RpcError::UnsupportedMethod` from the real
/// `BusinessService`, even for an otherwise-fully-authorized principal —
/// proving Deliverable 4's dedicated match arms are real, not merely
/// present in source.
#[tokio::test]
async fn desktop_mutation_methods_remain_unsupported_even_for_a_fully_authorized_principal() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_host_config(dir.path(), "deferred");
    let pipe_name = config.pipe_name.clone();

    let host = open_host(&config, deferred_methods_resolver());
    seed_fixtures(&host);
    let (run_task, shutdown_tx) = spawn_run(host);

    let client = connect_as(&pipe_name, DESKTOP_TOKEN).await;

    for method in [restart_request(), flash_request(), build_request()] {
        let result = call(&client, method.clone()).await;
        assert!(
            matches!(
                result,
                Err(CallError::Rpc(RpcError::UnsupportedMethod { .. }))
            ),
            "expected UnsupportedMethod for deferred method {method:?}, got {result:?}"
        );
    }

    drop(client);
    stop_host(run_task, shutdown_tx).await;
}

/// Failure case 2: a call with a principal lacking the relevant `Authority`
/// is denied before reaching the real service — matching `crate::server`'s
/// existing authorization tests but now exercised against the real
/// `BusinessService`, not the stub.
#[tokio::test]
async fn unauthorized_principal_is_denied_before_reaching_the_real_service() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_host_config(dir.path(), "unauthorized");
    let pipe_name = config.pipe_name.clone();

    let host = open_host(&config, unauthorized_resolver());
    seed_fixtures(&host);
    let (run_task, shutdown_tx) = spawn_run(host);

    let client = connect_as(&pipe_name, UNAUTHORIZED_TOKEN).await;

    let result = call(&client, Method::RuntimeGetStatus(EmptyParams {})).await;
    assert!(matches!(
        result,
        Err(CallError::Rpc(RpcError::PermissionDenied))
    ));

    drop(client);
    stop_host(run_task, shutdown_tx).await;
}

/// Restart/reconcile: open -> run -> real agent mutation -> clean shutdown
/// -> reopen `ComposedHost::open_with_test_arena_and_credential_resolver`
/// against the exact same on-disk `HostConfig`/paths -> run again -> prove
/// the device and the mutation result persisted before the clean shutdown
/// are both still readable/replayable afterward, and a fresh mutation
/// submitted after the restart succeeds — the same property
/// `tests/host_lifecycle.rs`'s `simulated_crash_and_restart_preserves_persisted_device_state`
/// proves for storage alone, now proven through the real business-RPC path,
/// exercised through the real, now-fixed `ComposedHost::run` shutdown/drain
/// sequence on both sides of the restart (`P1-IPC-SHUTDOWN-CORR`).
#[tokio::test]
async fn restart_reconcile_preserves_state_and_serves_a_fresh_mutation_through_the_real_business_rpc_path()
 {
    let dir = tempfile::tempdir().unwrap();
    let config = test_host_config(dir.path(), "restart-before");
    let pipe_name = config.pipe_name.clone();

    let first_job_id;
    {
        let host = open_host(&config, restart_reconcile_resolver());
        seed_fixtures(&host);
        let (run_task, shutdown_tx) = spawn_run(host);

        let client = connect_as(&pipe_name, AGENT_TOKEN).await;

        let result = call(&client, agent_run_register("idem-restart-persisted"))
            .await
            .expect("agent_run_register before the restart must succeed");
        let BusinessRpcResult::Mutation {
            outcome: WireMutationOutcome::Accepted { job_id, .. },
            ..
        } = result
        else {
            panic!("expected Accepted before the restart, got {result:?}");
        };
        first_job_id = job_id;

        // A real, clean shutdown — `ComposedHost::run`'s task is fully
        // awaited to completion before this block ends, so every
        // connection/call task and every `RuntimeBusinessService`
        // `Storage`/`RuntimeServices` clone is provably gone before the
        // "restart" below reopens the same on-disk paths.
        drop(client);
        stop_host(run_task, shutdown_tx).await;
    }

    // A fresh pipe name for the restarted instance — a real restart binds a
    // brand new `Server`, never reuses the previous instance's OS handle.
    let restarted_config = HostConfig {
        paths: config.paths.clone(),
        pipe_name: unique_pipe_name("restart-after"),
    };
    let restarted_pipe_name = restarted_config.pipe_name.clone();

    let restarted_host = open_host(&restarted_config, restart_reconcile_resolver());

    // The fixture device inserted before the restart is still present, read
    // directly through storage — the same proof `host_lifecycle.rs` already
    // makes for storage alone.
    let restarted_device = restarted_host
        .storage()
        .get_device(&DeviceId::new(FIXTURE_DEVICE_ID).unwrap())
        .expect("device lookup must succeed after the restart")
        .expect("the device inserted before the restart must still be present");
    assert_eq!(restarted_device.device_id.as_str(), FIXTURE_DEVICE_ID);

    let (run_task, shutdown_tx) = spawn_run(restarted_host);
    let agent_client = connect_as(&restarted_pipe_name, AGENT_TOKEN).await;
    // A separate desktop-principal connection for the `DeviceGet` read below
    // — the agent principal deliberately does not hold `Authority::ReadDevices`
    // (matching production's expected authority separation; see
    // `restart_reconcile_resolver`'s doc comment).
    let desktop_client = connect_as(&restarted_pipe_name, DESKTOP_TOKEN).await;

    // The device persisted before the restart is still readable through the
    // real business-RPC path, not only through direct storage access.
    let device = call(
        &desktop_client,
        Method::DeviceGet(DeviceParams {
            device_id: FIXTURE_DEVICE_ID.to_string(),
        }),
    )
    .await
    .expect("device_get after the restart must succeed");
    match device {
        BusinessRpcResult::DeviceGet { device, .. } => {
            assert!(device.is_some(), "device must still be found after restart");
        }
        other => panic!("expected DeviceGet, got {other:?}"),
    }

    // The mutation result persisted before the restart replays identically
    // through the real business-RPC path — proving business-RPC continuity,
    // not merely storage continuity.
    let replayed = call(&agent_client, agent_run_register("idem-restart-persisted"))
        .await
        .expect("replaying the pre-restart idempotency key must succeed");
    match replayed {
        BusinessRpcResult::Mutation {
            outcome: WireMutationOutcome::ReplayedIdenticalAccepted { job_id, .. },
            ..
        } => assert_eq!(job_id, first_job_id),
        other => panic!("expected ReplayedIdenticalAccepted, got {other:?}"),
    }

    // A fresh mutation submitted after the restart succeeds.
    let fresh = call(&agent_client, agent_run_register("idem-restart-fresh"))
        .await
        .expect("a fresh mutation after the restart must succeed");
    let BusinessRpcResult::Mutation {
        outcome:
            WireMutationOutcome::Accepted {
                job_id: fresh_job_id,
                ..
            },
        ..
    } = fresh
    else {
        panic!("expected Accepted for a fresh post-restart mutation, got {fresh:?}");
    };
    assert_ne!(
        fresh_job_id, first_job_id,
        "a fresh mutation must not be conflated with the replayed pre-restart one"
    );

    drop(agent_client);
    drop(desktop_client);
    stop_host(run_task, shutdown_tx).await;
}
