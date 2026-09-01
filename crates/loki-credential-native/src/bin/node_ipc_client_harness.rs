//! Test-only real-server harness for `packages/ipc-client`'s own real
//! integration tests (`P1-NODE-IPC-CLIENT-001` §6 Deliverable 7). Never
//! shipped — see this binary's own `[[bin]]` doc comment in
//! `crates/loki-credential-native/Cargo.toml` for why it lives here rather
//! than in `crates/loki-runtime/tests/**`.
//!
//! Two modes, selected by the first argument:
//!
//! - `real <work-dir>`: opens a real `ComposedHost` via
//!   `open_with_test_arena_and_credential_resolver`, so the real
//!   `RuntimeBusinessService` is installed. Mints both real
//!   `DesktopInstallation`/`AgentConnection` DPAPI-protected credentials to
//!   `<work-dir>` via the real `loki_storage::credential::
//!   mint_or_load_credential` (the same call `P1-NODE-IPC-CLIENT-001` §6
//!   Deliverable 6 proves round-trips through the compiled Node addon),
//!   hex-encodes each plaintext as the `AuthToken` a `CredentialSet` record
//!   uses (matching this crate's own `AuthToken`/`CredentialRecord`
//!   construction convention), and seeds one real, registered agent
//!   connection through `ComposedHost::storage()` (matching
//!   `crates/loki-runtime/tests/business_service.rs`'s own fixture
//!   convention) so a real `AgentRunRegister` call can succeed.
//! - `slow <work-dir> <delay-ms>`: opens a real `ComposedHost` via
//!   `open_with_test_arena_and_business_service`, installing a minimal
//!   `SlowBusinessService` (mirroring
//!   `crates/loki-runtime/tests/host_lifecycle.rs`'s own fixture) that
//!   sleeps `delay-ms` before answering `RuntimeGetStatus` — the deadline/
//!   cancellation test fixture named in §6 Deliverable 7.
//!
//! Either mode prints exactly one JSON line to stdout once the server is
//! bound and ready, then reads newline-delimited commands from stdin:
//!
//! - `REVOKE <principal_id>` (`real` mode only): revokes that principal's
//!   credential mid-connection via the shared, lockable `CredentialSet` —
//!   prints `REVOKED` once done.
//! - `SHUTDOWN`: signals a real, clean `ComposedHost::run` shutdown, awaits
//!   it, prints `STOPPED`, and exits `0`.
//!
//! No plaintext credential is ever printed to stdout/stderr or passed via
//! argv/env — only paths, principal ids, and the fixed protocol keywords
//! above.

use std::collections::BTreeSet;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use loki_ipc::auth::{AuthToken, CredentialResolver, CredentialSet};
use loki_ipc::business::{
    AuthenticatedPrincipal, Authority, BusinessRpcResult, PrincipalKind, ReadMeta,
    WireMutationAvailability, WireStorageReadiness,
};
use loki_ipc::protocol::Method;
use loki_ipc::server::{
    BusinessCallFuture, BusinessEventEmitter, BusinessRequestContext, BusinessService,
    BusinessStreamFuture,
};
use loki_runtime::host::compose::ComposedHost;
use loki_runtime::host::paths::HostPaths;
use loki_runtime::shutdown::ShutdownReason;
use loki_storage::agents::AgentsRepository;
use loki_storage::credential::CredentialKind;

const AGENT_CONNECTION_ID: &str = "ac_node_ipc_client_harness";
const DESKTOP_PRINCIPAL_ID: &str = "desktop_node_ipc_client_harness";

/// Wraps `CredentialSet` behind a `Mutex` so this process's stdin command
/// loop (`REVOKE <principal_id>`) can mutate it concurrently with the real
/// server's own handshake/revalidation reads — `CredentialSet` itself has
/// no interior mutability (`crates/loki-ipc/src/auth.rs`'s own `revoke`
/// takes `&mut self`, a "test/local-composition convenience", exactly this
/// harness's use case).
struct SharedCredentialSet(Mutex<CredentialSet>);

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

impl SharedCredentialSet {
    fn revoke(&self, principal_id: &str) {
        self.0.lock().unwrap().revoke(principal_id);
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn print_ready_line(fields: &[(&str, String)]) {
    let mut json = String::from("{");
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push('"');
        json.push_str(key);
        json.push_str("\":\"");
        json.push_str(&value.replace('\\', "\\\\").replace('"', "\\\""));
        json.push('"');
    }
    json.push('}');
    println!("{json}");
    std::io::stdout().flush().ok();
}

/// Blocks reading stdin lines until `SHUTDOWN`, dispatching any
/// `REVOKE <principal_id>` line to `on_revoke` first. Runs on a dedicated
/// blocking thread (stdin is synchronous) and signals `shutdown_tx` when
/// `SHUTDOWN` (or EOF, defensively) is observed.
fn run_stdin_command_loop(
    on_revoke: Option<Arc<SharedCredentialSet>>,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
) {
    let stdin = std::io::stdin();
    let mut shutdown_tx = Some(shutdown_tx);
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line == "SHUTDOWN" {
            if let Some(tx) = shutdown_tx.take() {
                let _ = tx.send(());
            }
            break;
        }
        if let Some(principal_id) = line.strip_prefix("REVOKE ") {
            if let Some(resolver) = &on_revoke {
                resolver.revoke(principal_id);
                println!("REVOKED");
                std::io::stdout().flush().ok();
            }
        }
    }
    if let Some(tx) = shutdown_tx.take() {
        let _ = tx.send(());
    }
}

fn unique_pipe_name(label: &str) -> String {
    format!(
        r"\\.\pipe\loki-node-ipc-client-harness-{label}-{}",
        uuid::Uuid::new_v4()
    )
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("real") => run_real(&args).await,
        Some("slow") => run_slow(&args).await,
        _ => {
            eprintln!("usage: node_ipc_client_harness <real|slow> <work-dir> [delay-ms]");
            std::process::exit(2);
        }
    }
}

async fn run_real(args: &[String]) {
    let work_dir = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&work_dir).expect("create work dir");
    let paths = HostPaths::new(&work_dir);

    let desktop_plaintext = loki_storage::credential::mint_or_load_credential(
        CredentialKind::DesktopInstallation,
        &paths.desktop_installation_credential_file(),
    )
    .expect("mint desktop_installation credential");
    let agent_plaintext = loki_storage::credential::mint_or_load_credential(
        CredentialKind::AgentConnection,
        &paths.agent_connection_credential_file(),
    )
    .expect("mint agent_connection credential");

    let desktop_token = hex_encode(&desktop_plaintext);
    let agent_token = hex_encode(&agent_plaintext);

    let desktop_principal = AuthenticatedPrincipal {
        kind: PrincipalKind::DesktopInstallation,
        principal_id: DESKTOP_PRINCIPAL_ID.to_string(),
        authorized_entity_ids: BTreeSet::new(),
        authority: BTreeSet::from([Authority::ReadRuntime]),
    };
    let agent_principal = AuthenticatedPrincipal {
        kind: PrincipalKind::AgentConnection,
        principal_id: AGENT_CONNECTION_ID.to_string(),
        authorized_entity_ids: BTreeSet::new(),
        authority: BTreeSet::from([Authority::ManageAgentRuns]),
    };

    let resolver = Arc::new(SharedCredentialSet(Mutex::new(CredentialSet::new(vec![
        loki_ipc::auth::CredentialRecord::active(
            AuthToken::new(desktop_token.clone()),
            desktop_principal,
        ),
        loki_ipc::auth::CredentialRecord::active(
            AuthToken::new(agent_token.clone()),
            agent_principal,
        ),
    ]))));

    let pipe_name = unique_pipe_name("real");
    let config = loki_runtime::host::compose::HostConfig {
        paths,
        pipe_name: pipe_name.clone(),
    };

    let host = ComposedHost::open_with_test_arena_and_credential_resolver(
        &config,
        resolver.clone() as Arc<dyn CredentialResolver>,
    )
    .expect("host must start with the harness credential resolver installed");

    host.storage()
        .insert_agent_connection(
            AGENT_CONNECTION_ID,
            "node-ipc-client-harness",
            "sha256:harness",
            "register_runs",
            0,
        )
        .expect("seed real agent connection");

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

    print_ready_line(&[
        ("pipe_name", pipe_name),
        (
            "desktop_installation_credential_path",
            config_path_string(&config.paths.desktop_installation_credential_file()),
        ),
        (
            "agent_connection_credential_path",
            config_path_string(&config.paths.agent_connection_credential_file()),
        ),
        (
            "desktop_installation_principal_id",
            DESKTOP_PRINCIPAL_ID.to_string(),
        ),
        (
            "agent_connection_principal_id",
            AGENT_CONNECTION_ID.to_string(),
        ),
    ]);

    let (cmd_shutdown_tx, cmd_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let resolver_for_loop = Some(resolver);
    let stdin_thread =
        std::thread::spawn(move || run_stdin_command_loop(resolver_for_loop, cmd_shutdown_tx));

    let _ = cmd_shutdown_rx.await;
    let _ = shutdown_tx.send(());
    let _ = run_task.await;
    println!("STOPPED");
    std::io::stdout().flush().ok();
    let _ = stdin_thread.join();
}

fn config_path_string(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Sleeps `delay` before answering `RuntimeGetStatus`; every other method
/// fails closed with `UnsupportedMethod` — mirrors
/// `crates/loki-runtime/tests/host_lifecycle.rs`'s own `SlowBusinessService`
/// fixture exactly (duplicated, not shared, matching that file's own
/// documented reason: no dev-dependency seam exists to share it).
struct SlowBusinessService {
    delay: Duration,
}

impl BusinessService for SlowBusinessService {
    fn call(&self, method: Method, context: BusinessRequestContext) -> BusinessCallFuture {
        let delay = self.delay;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            match method {
                Method::RuntimeGetStatus(_) => Ok(BusinessRpcResult::RuntimeStatus {
                    meta: ReadMeta {
                        source_runtime_id: "node-ipc-client-harness-slow".to_string(),
                        observed_at: "2026-09-01T00:00:00.000Z".to_string(),
                        correlation_id: context.correlation_id.to_string(),
                    },
                    storage: WireStorageReadiness::Ready,
                    registered_device_count: 0,
                    mutations: WireMutationAvailability::Available,
                }),
                other => Err(loki_ipc::protocol::RpcError::UnsupportedMethod {
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
            Err(loki_ipc::protocol::RpcError::UnsupportedMethod {
                method: "business_events".to_string(),
            })
        })
    }
}

async fn run_slow(args: &[String]) {
    let work_dir = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&work_dir).expect("create work dir");
    let delay_ms: u64 = args
        .get(3)
        .and_then(|value| value.parse().ok())
        .unwrap_or(3_000);

    let paths = HostPaths::new(&work_dir);
    let desktop_plaintext = loki_storage::credential::mint_or_load_credential(
        CredentialKind::DesktopInstallation,
        &paths.desktop_installation_credential_file(),
    )
    .expect("mint desktop_installation credential");
    let desktop_token = hex_encode(&desktop_plaintext);

    let desktop_principal = AuthenticatedPrincipal {
        kind: PrincipalKind::DesktopInstallation,
        principal_id: DESKTOP_PRINCIPAL_ID.to_string(),
        authorized_entity_ids: BTreeSet::new(),
        authority: BTreeSet::from([Authority::ReadRuntime]),
    };
    let resolver: Arc<dyn CredentialResolver> = Arc::new(CredentialSet::single(
        AuthToken::new(desktop_token),
        desktop_principal,
    ));
    let service: Arc<dyn BusinessService> = Arc::new(SlowBusinessService {
        delay: Duration::from_millis(delay_ms),
    });

    let pipe_name = unique_pipe_name("slow");
    let config = loki_runtime::host::compose::HostConfig {
        paths: paths.clone(),
        pipe_name: pipe_name.clone(),
    };

    let host = ComposedHost::open_with_test_arena_and_business_service(&config, resolver, service)
        .expect("host must start with the harness slow business service installed");

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

    print_ready_line(&[
        ("pipe_name", pipe_name),
        (
            "desktop_installation_credential_path",
            config_path_string(&paths.desktop_installation_credential_file()),
        ),
        (
            "desktop_installation_principal_id",
            DESKTOP_PRINCIPAL_ID.to_string(),
        ),
    ]);

    let (cmd_shutdown_tx, cmd_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let stdin_thread = std::thread::spawn(move || run_stdin_command_loop(None, cmd_shutdown_tx));

    let _ = cmd_shutdown_rx.await;
    let _ = shutdown_tx.send(());
    let _ = run_task.await;
    println!("STOPPED");
    std::io::stdout().flush().ok();
    let _ = stdin_thread.join();
}
