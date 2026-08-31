//! `P1-IPC-SHUTDOWN-CORR`: proves [`Server::serve`] actually waits for every
//! spawned per-connection dispatch task to finish before returning, instead
//! of returning the instant its own `shutdown` future resolves while
//! connections (and any `BusinessService` resource clone they still hold)
//! keep running detached in the background.
//!
//! This is the exact defect `P1-RUNTIME-COMP-001`'s implementer reproduced
//! as a real `STATUS_ACCESS_VIOLATION` (see its handoff,
//! `Docs/graph/handoffs/P1-RUNTIME-COMP-001-1.md` §1/§6/§7): a caller such as
//! `crate::host::compose::ComposedHost::run` tears down a resource
//! (`loki-storage`'s secure-memory arena) a still-running connection's
//! `BusinessService` clone may reference, with no way to know that teardown
//! was actually safe. Both tests below use a real `Server`/`Client` pair
//! over a real named pipe — never a synthetic in-process double for the
//! transport itself.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use loki_ipc::auth::{AuthToken, CredentialRecord, CredentialSet};
use loki_ipc::business::{
    AuthenticatedPrincipal, Authority, BusinessRpcResult, PrincipalKind, ReadMeta,
    WireMutationAvailability, WireStorageReadiness,
};
use loki_ipc::client::Client;
use loki_ipc::protocol::{Method, RpcError};
use loki_ipc::server::{
    BusinessCallFuture, BusinessEventEmitter, BusinessRequestContext, BusinessService,
    BusinessStreamFuture, Server, ServerConfig,
};

/// The only principal this test file's fixture resolver grants — full
/// `ReadRuntime` authority, enough to reach a real `BusinessService::call`
/// for `Method::RuntimeGetStatus`, and nothing else this file needs.
const PRINCIPAL_ID: &str = "shutdown-drain-test-principal";
const TOKEN: &str = "shutdown-drain-test-token";

fn principal() -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        kind: PrincipalKind::RuntimeService,
        principal_id: PRINCIPAL_ID.to_string(),
        authorized_entity_ids: Default::default(),
        authority: std::collections::BTreeSet::from([Authority::ReadRuntime]),
    }
}

/// A `BusinessService` whose `call` notifies `call_started` the instant it
/// begins, sleeps `delay`, then flips `call_completed` before returning a
/// real `RuntimeStatus` result — used to prove `Server::serve` genuinely
/// waits for an in-flight call's owning connection task to finish, not just
/// for the accept loop itself to stop.
struct SlowBusinessService {
    call_started: Arc<tokio::sync::Notify>,
    call_completed: Arc<AtomicBool>,
    delay: Duration,
}

impl BusinessService for SlowBusinessService {
    fn call(&self, method: Method, context: BusinessRequestContext) -> BusinessCallFuture {
        let call_started = Arc::clone(&self.call_started);
        let call_completed = Arc::clone(&self.call_completed);
        let delay = self.delay;
        Box::pin(async move {
            call_started.notify_one();
            tokio::time::sleep(delay).await;
            call_completed.store(true, Ordering::SeqCst);
            match method {
                Method::RuntimeGetStatus(_) => Ok(BusinessRpcResult::RuntimeStatus {
                    meta: ReadMeta {
                        source_runtime_id: "shutdown-drain-test".to_string(),
                        observed_at: "2026-08-31T00:00:00.000Z".to_string(),
                        correlation_id: context.correlation_id.to_string(),
                    },
                    storage: WireStorageReadiness::Ready,
                    registered_device_count: 0,
                    mutations: WireMutationAvailability::Available,
                }),
                other => Err(RpcError::UnsupportedMethod {
                    method: format!("{other:?}"),
                }),
            }
        })
    }

    fn stream(
        &self,
        _params: loki_ipc::business::BusinessStreamParams,
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

fn fixture_server_config(pipe_name: &str, service: Arc<dyn BusinessService>) -> ServerConfig {
    let resolver = CredentialSet::new(vec![CredentialRecord::active(
        AuthToken::new(TOKEN),
        principal(),
    )]);
    ServerConfig::new(pipe_name.to_string(), AuthToken::new(TOKEN))
        .with_credential_resolver(Arc::new(resolver))
        .with_business_service(service)
}

/// The core regression test for the reproduced crash: a client connects,
/// completes the handshake, and — exactly like
/// `crates/loki-runtime/tests/host_lifecycle.rs`'s
/// `startup_accepts_a_real_ipc_connection_and_is_located_as_healthy` — never
/// disconnects on its own. Before `P1-IPC-SHUTDOWN-CORR`, `Server::serve`
/// returned the instant `shutdown` resolved regardless, leaving this
/// connection's dispatch task (and its `BusinessService` clone) running
/// detached forever. After the fix, `serve` must actively close the
/// connection and only return once that dispatch task has actually
/// finished — bounded well under `CONNECTION_DRAIN_GRACE`, not by luck.
#[tokio::test]
async fn serve_closes_a_still_open_idle_connection_and_returns_promptly_after_shutdown() {
    let pipe_name = support::unique_pipe_name("shutdown-drain-idle");
    let service: Arc<dyn BusinessService> = Arc::new(SlowBusinessService {
        call_started: Arc::new(tokio::sync::Notify::new()),
        call_completed: Arc::new(AtomicBool::new(false)),
        delay: Duration::ZERO,
    });
    let service_handle = Arc::clone(&service);

    let mut server = Server::bind(fixture_server_config(&pipe_name, service)).expect("bind server");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let serve_task = tokio::spawn(async move {
        let shutdown: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
            Box::pin(async move {
                let _ = shutdown_rx.await;
            });
        server.serve(shutdown).await;
    });

    // A real client connects and completes the handshake, then is
    // deliberately kept alive (never dropped, never disconnected) for the
    // rest of this test — the exact shape of the reproduced crash.
    let client = tokio::time::timeout(
        support::TEST_HANG_GUARD,
        Client::connect(support::client_config_with_token(&pipe_name, TOKEN)),
    )
    .await
    .expect("connect must not hang")
    .expect("handshake should succeed");

    let _ = shutdown_tx.send(());

    let started = Instant::now();
    tokio::time::timeout(support::TEST_HANG_GUARD, serve_task)
        .await
        .expect("serve() must not hang even though the client never disconnected")
        .expect("serve task must not panic");
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(3),
        "serve() took {elapsed:?} to return after shutdown for an idle connection; \
         it should have actively closed the connection almost immediately, not \
         waited out the full drain grace period"
    );

    // The only remaining strong reference to the business service is this
    // test's own `service_handle` — proving the connection's dispatch task
    // (and every clone of the service it held via `SessionAuthority`) has
    // actually finished and dropped, not merely been abandoned as a
    // detached background task.
    assert_eq!(
        Arc::strong_count(&service_handle),
        1,
        "a BusinessService clone is still held somewhere after serve() returned"
    );

    // The still-open client handle is dropped only now, after every
    // assertion above — proving none of them depended on the client having
    // disconnected.
    drop(client);
}

/// A slower, more direct proof that `serve` waits for a spawned dispatch
/// task to *actually finish its work*, not merely for the connection's
/// socket to close: a real in-flight `business_call` is still executing
/// when shutdown is triggered, and `serve()` must not return until that
/// call has completed.
#[tokio::test]
async fn serve_does_not_return_until_an_in_flight_call_actually_finishes() {
    let pipe_name = support::unique_pipe_name("shutdown-drain-in-flight");
    let call_started = Arc::new(tokio::sync::Notify::new());
    let call_completed = Arc::new(AtomicBool::new(false));
    let delay = Duration::from_millis(700);
    let service: Arc<dyn BusinessService> = Arc::new(SlowBusinessService {
        call_started: Arc::clone(&call_started),
        call_completed: Arc::clone(&call_completed),
        delay,
    });

    let mut server = Server::bind(fixture_server_config(&pipe_name, service)).expect("bind server");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let serve_task = tokio::spawn(async move {
        let shutdown: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
            Box::pin(async move {
                let _ = shutdown_rx.await;
            });
        server.serve(shutdown).await;
    });

    let client = tokio::time::timeout(
        support::TEST_HANG_GUARD,
        Client::connect(support::client_config_with_token(&pipe_name, TOKEN)),
    )
    .await
    .expect("connect must not hang")
    .expect("handshake should succeed");

    // Fire the slow call without awaiting its response, then wait for the
    // business service to confirm it has actually started before this test
    // triggers shutdown — deterministic, not timing-guessed.
    let call_task = tokio::spawn(async move {
        client
            .business_call(
                Method::RuntimeGetStatus(loki_ipc::business::EmptyParams {}),
                Duration::from_secs(5),
            )
            .await
    });
    tokio::time::timeout(support::TEST_HANG_GUARD, call_started.notified())
        .await
        .expect("business_service.call must start promptly");

    let _ = shutdown_tx.send(());

    // The in-flight call has not had time to complete yet; `serve` must
    // still be running.
    assert!(
        !call_completed.load(Ordering::SeqCst),
        "test timing assumption violated: the slow call finished before shutdown could race it"
    );
    assert!(
        !serve_task.is_finished(),
        "serve() returned before the in-flight call's owning connection task finished"
    );

    tokio::time::timeout(support::TEST_HANG_GUARD, serve_task)
        .await
        .expect("serve() must not hang")
        .expect("serve task must not panic");

    assert!(
        call_completed.load(Ordering::SeqCst),
        "serve() returned before the in-flight business call actually finished"
    );

    let _ = tokio::time::timeout(support::TEST_HANG_GUARD, call_task).await;
}
