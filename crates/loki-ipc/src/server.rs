//! Named-pipe server: accept loop, per-connection handshake, and RPC
//! dispatch with per-call deadlines, cancellation, streaming backpressure,
//! fail-closed per-credential principal resolution, and dynamic
//! post-handshake session revalidation.
//!
//! ## Backpressure design
//!
//! Every outgoing frame for a connection (handshake replies aside, which
//! happen before this exists) is sent through one bounded
//! [`tokio::sync::mpsc::channel`] to a single dedicated writer task that
//! owns the pipe's write half exclusively. This does two things at once:
//! it serializes writes (no interleaving/corrupt frames from concurrent
//! calls without needing a lock), and it makes the channel's bounded
//! capacity ([`ServerConfig::outgoing_channel_capacity`]) the transport's
//! explicit backpressure knob — a slow-reading client stalls the writer
//! task, which fills the channel, which makes any in-flight `send().await`
//! (including a streaming method's per-item sends) wait, all the way back
//! to the method implementation producing items. See `tests/streaming.rs`.
//!
//! ## Cancellation design
//!
//! Each `Call`/`CallStream` runs inside one spawned task that races the
//! actual work against `tokio::time::sleep(deadline)` and an explicit
//! per-call `oneshot` cancel signal via `tokio::select!`. Whichever branch
//! loses is dropped by `select!` itself — that drop *is* the cancellation;
//! there is no separate "please stop soon" flag the work has to poll.
//!
//! ## Dynamic session revalidation (closes REVIEW-002 finding 1)
//!
//! `SessionAuthority` stores only the resolved principal's `principal_id`
//! (never the raw token) plus a handle to the
//! [`crate::auth::CredentialResolver`]. Every unary call and every stream
//! subscription calls [`crate::auth::CredentialResolver::revalidate`]
//! immediately before authorization — never a cached/cloned principal from
//! `Hello` — so a credential revoked or expired mid-session is denied on
//! its very next call, and an already-running subscription is denied on its
//! next forwarded event (see `execute_stream_method`), not merely at the
//! next `Hello`.
//!
//! ## Per-method target/entity-identity coupling (closes REVIEW-002 finding 2)
//!
//! [`target_entity_id`] names the exact `device_id`/`run_id`/
//! `work_intent_id` a single-entity method's request targets. Authorization
//! requires the principal's `authorized_entity_ids` to contain that exact
//! id (never a workspace/tenant grouping); [`execute_method`] additionally
//! rejects any business-service response whose own entity identity does not
//! match the request's — a mismatched or substituted response is a hard
//! `RpcError::Internal`, never silently returned.

use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::acl::{self, AclScope};
use crate::auth::{self, AuthToken, CredentialResolver, CredentialSet};
use crate::business::{
    AuthenticatedPrincipal, Authority, BUSINESS_CAPABILITY, BusinessEvent, BusinessRpcResult,
    BusinessStreamName, BusinessStreamParams, BusinessStreamSummary, MAX_STREAM_ITEMS,
    OperationMethod, PrincipalKind, bounded, safe_digest, safe_id, safe_request_digest, valid_page,
};
use crate::frame::{DEFAULT_MAX_FRAME_BYTES, FrameError, read_frame, write_frame};
use crate::protocol::{
    CORE_CAPABILITY, CallOutcome, EchoStreamItem, EchoStreamSummary, HandshakeRejectReason,
    MAX_ECHO_STREAM_COUNT, Method, MethodResult, PROTOCOL_VERSION, PongResult, RpcError,
    StreamItemPayload, StreamMethod, StreamOutcome, StreamSummary, WireMessage,
    baseline_capabilities, business_result_matches_method,
};
use crate::time;

/// Bounds how long a spawned per-call task will wait to place one more
/// frame onto the outgoing queue before giving up. A genuine, generous
/// safety net (not a normal-operation limit) so a per-call task can never
/// linger forever even after it has already been cancelled/deadline-out —
/// see [`send_message`].
const OUTGOING_SEND_TIMEOUT: Duration = Duration::from_secs(30);

/// How long [`Connection::handle`] waits for the writer task to drain and
/// exit after the dispatch loop ends, before giving up on a graceful
/// finish.
const WRITER_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// How long [`Server::serve`] waits, after its own `shutdown` future
/// resolves and every still-open connection has been asked to close, for
/// every spawned per-connection dispatch task to actually finish, before
/// forcibly aborting whichever ones have not (`JoinSet::abort_all`, which
/// drops that task's future — and therefore any `BusinessService`/resource
/// clone it still owns — at its very next await point). Deliberately larger
/// than [`WRITER_SHUTDOWN_GRACE`] (which bounds a single connection's own
/// writer-drain) since many connections' per-connection tasks race that
/// grace period concurrently, not sequentially, but a caller such as
/// `crate::host::compose::ComposedHost::run` still needs `serve` to return
/// in bounded time no matter how many connections are open or how a
/// misbehaving client stalls. See `P1-IPC-SHUTDOWN-CORR`'s handoff for the
/// use-after-free this constant closes.
///
/// This is a genuine, enforced upper bound on how long `serve()` itself can
/// take to return, because dropping a connection task's future (which
/// `abort_all` triggers) also drops that task's own local
/// [`tokio::task::JoinSet`] of per-call tasks (see [`CALL_TASK_DRAIN_GRACE`]),
/// and `JoinSet`'s documented `Drop` behavior is to abort every task it
/// still owns. A caller such as `ComposedHost::run` may therefore simply
/// `.await` the task `serve()` runs in directly instead of racing it against
/// a second, shorter, non-cancelling outer timeout — see that function's own
/// doc comment for why a second race would be actively unsafe here.
///
/// Set comfortably larger than [`CALL_TASK_DRAIN_GRACE`] + [`WRITER_SHUTDOWN_GRACE`]
/// (10s + 5s = 15s) so those two *inner*, sequential, per-connection grace
/// periods normally have room to let a genuinely slow but legitimate
/// in-flight call (the reproduced scenario is 6-9s) finish and drain on its
/// own before this *outer* bound would ever force-abort the whole
/// connection early — this outer bound exists purely as a last-resort
/// backstop for a connection that does not respond to the inner mechanisms
/// at all (e.g. is wedged in a way that never even reaches the per-call/
/// writer drain steps), not as the primary mechanism for ordinary slow
/// calls.
const CONNECTION_DRAIN_GRACE: Duration = Duration::from_secs(20);

/// How long a connection's dispatch loop waits, once it has ended (client
/// disconnect, protocol error, or server-initiated shutdown), for every
/// per-call task it spawned (`spawn_unary_call`/`spawn_stream_call`) to
/// actually finish, before forcibly aborting whichever ones have not.
///
/// Closes `P1-IPC-SHUTDOWN-CORR`'s independent-review finding: per-call
/// tasks used to be bare-`tokio::spawn`ed and untracked by any `JoinSet`,
/// so a connection's dispatch loop (and therefore the outer
/// [`Server::serve`]'s per-connection [`JoinSet`]) could report "done" while
/// a business call it spawned — holding its own clone of the
/// `BusinessService`/`SessionAuthority` — was still running detached,
/// unbounded by anything but the client-supplied `deadline_unix_millis`
/// (which the client fully controls and which has no server-side maximum).
/// Tracking these in a per-connection `JoinSet` and draining it before the
/// connection's own task is considered finished closes that gap completely:
/// once the outer `Server::serve`'s per-connection `JoinSet` reports a
/// connection task done, every call it ever spawned is provably also done
/// (either it finished on its own, or it was aborted here or via the
/// `Drop`-aborts-owned-tasks fallback described on [`CONNECTION_DRAIN_GRACE`]).
///
/// Set to 10s — comfortably above the 6-9s in-flight-call window the
/// independent review's required re-verification reproduces (a genuinely
/// slow but legitimate call, e.g. one gated by real backpressure on a slow
/// reader) — so that scenario naturally finishes and is observed to finish
/// (not merely safely aborted) before this grace elapses, while still
/// bounding the pathological case (a call truly wedged forever) to a
/// concrete, finite wait.
const CALL_TASK_DRAIN_GRACE: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct SessionAuthority {
    negotiated_capabilities: BTreeSet<String>,
    /// The resolved principal's opaque reference — NEVER the raw token —
    /// used to re-fetch the principal's *current* state on every dispatch.
    /// See this module's top doc comment.
    principal_id: String,
    credential_resolver: Arc<dyn CredentialResolver>,
    session_id: Uuid,
    business_service: Arc<dyn BusinessService>,
}

impl SessionAuthority {
    /// Re-resolves the current principal for this session, or
    /// `RpcError::CredentialRevalidationFailed` if it has since been
    /// revoked/expired/removed. Called before every dispatch — never
    /// cached.
    fn revalidate(&self) -> Result<AuthenticatedPrincipal, RpcError> {
        self.credential_resolver
            .revalidate(&self.principal_id, time::unix_millis_now())
            .ok_or(RpcError::CredentialRevalidationFailed)
    }
}

#[derive(Clone)]
pub struct BusinessRequestContext {
    pub correlation_id: Uuid,
    pub session_id: Uuid,
    pub principal: AuthenticatedPrincipal,
}

pub type BusinessCallFuture =
    Pin<Box<dyn Future<Output = Result<BusinessRpcResult, RpcError>> + Send>>;
pub type BusinessStreamFuture =
    Pin<Box<dyn Future<Output = Result<BusinessStreamSummary, RpcError>> + Send>>;

#[derive(Clone)]
pub struct BusinessEventEmitter {
    sender: mpsc::Sender<BusinessEvent>,
}

impl BusinessEventEmitter {
    pub async fn send(&self, event: BusinessEvent) -> Result<(), RpcError> {
        self.sender
            .send(event)
            .await
            .map_err(|_| RpcError::Cancelled)
    }
}

/// The producer-injection seam a runtime composition node
/// (`P1-RUNTIME-COMP-001`) installs a real implementation into, backed by a
/// live `RuntimeServices`. Preserved unchanged in shape from the salvaged
/// `codex/p1-rpc-contract-001` design — see
/// `Docs/graph/tasks/P1-RPC-CONTRACT-001-R1.md` §3's "known starting
/// conditions".
pub trait BusinessService: Send + Sync {
    fn call(&self, method: Method, context: BusinessRequestContext) -> BusinessCallFuture;
    fn stream(
        &self,
        params: BusinessStreamParams,
        context: BusinessRequestContext,
        emitter: BusinessEventEmitter,
    ) -> BusinessStreamFuture;
}

struct UnsupportedBusinessService;

impl BusinessService for UnsupportedBusinessService {
    fn call(&self, method: Method, _context: BusinessRequestContext) -> BusinessCallFuture {
        let name = method_name(&method).to_string();
        Box::pin(async move { Err(RpcError::UnsupportedMethod { method: name }) })
    }

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

/// Server-side configuration. The ACL protects the pipe handle while the
/// credential resolver binds each successful handshake to its own principal
/// and is re-consulted on every subsequent dispatch.
#[derive(Clone)]
pub struct ServerConfig {
    /// Full pipe address, e.g. `r"\\.\pipe\LokiAI-runtime"`.
    pub pipe_name: String,
    /// Per-credential authentication and principal resolution. Production
    /// composition replaces the core-only legacy resolver created by `new`.
    pub credential_resolver: Arc<dyn CredentialResolver>,
    /// Which principal the pipe's DACL grants access to. Defaults to
    /// [`AclScope::CurrentUser`]; see [`ServerConfig::new`].
    pub acl_scope: AclScope,
    pub max_frame_bytes: u32,
    pub out_buffer_size: u32,
    pub in_buffer_size: u32,
    /// Bounded capacity of the per-connection outgoing frame queue — the
    /// backpressure knob described in this module's top doc comment.
    pub outgoing_channel_capacity: usize,
    pub business_service: Arc<dyn BusinessService>,
}

impl ServerConfig {
    pub fn new(pipe_name: impl Into<String>, token: AuthToken) -> Self {
        let principal = AuthenticatedPrincipal::runtime_service("legacy-core-client");
        Self {
            pipe_name: pipe_name.into(),
            credential_resolver: Arc::new(CredentialSet::single(token, principal)),
            acl_scope: AclScope::CurrentUser,
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            out_buffer_size: 65536,
            in_buffer_size: 65536,
            outgoing_channel_capacity: 16,
            business_service: Arc::new(UnsupportedBusinessService),
        }
    }

    pub fn with_credential_resolver(mut self, resolver: Arc<dyn CredentialResolver>) -> Self {
        self.credential_resolver = resolver;
        self
    }

    pub fn with_business_service(mut self, service: Arc<dyn BusinessService>) -> Self {
        self.business_service = service;
        self
    }
}

/// Why a connection ended abnormally (a normal disconnect after a
/// successful handshake is `Ok(())` from [`Connection::handle`], not an
/// error).
#[derive(Debug)]
pub enum ConnectionError {
    Io(std::io::Error),
    Frame(FrameError),
    /// The client disconnected before completing the handshake.
    ClosedDuringHandshake,
    /// The handshake completed but was rejected — see [`HandshakeRejectReason`].
    HandshakeRejected(HandshakeRejectReason),
    /// The client sent a well-formed frame that is not valid in its current
    /// state (e.g. a `Call` before completing the handshake, or a second
    /// `Hello`).
    ProtocolViolation(String),
    /// The server-side caller (e.g. [`Server::serve`]'s `shutdown` future
    /// resolving) asked this connection to close before the client
    /// completed its handshake. Never caused by the client itself — see
    /// [`Connection::handle_until_shutdown`].
    ServerShutdown,
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::Io(err) => write!(f, "connection I/O error: {err}"),
            ConnectionError::Frame(err) => write!(f, "connection frame error: {err}"),
            ConnectionError::ClosedDuringHandshake => {
                write!(f, "client disconnected during handshake")
            }
            ConnectionError::HandshakeRejected(reason) => {
                write!(f, "handshake rejected: {reason}")
            }
            ConnectionError::ProtocolViolation(message) => {
                write!(f, "protocol violation: {message}")
            }
            ConnectionError::ServerShutdown => {
                write!(
                    f,
                    "server is shutting down; connection closed before handshake completed"
                )
            }
        }
    }
}

impl std::error::Error for ConnectionError {}

/// A Windows named-pipe RPC server. Owns exactly one "next" pipe instance
/// at all times so a connecting client is never spuriously refused with
/// `NotFound` between one client's accept and the next instance being
/// created (the pattern `tokio`'s own `named_pipe` module documents).
pub struct Server {
    config: Arc<ServerConfig>,
    server_run_id: Uuid,
    pending: NamedPipeServer,
}

impl Server {
    /// Creates the first pipe instance. `first_pipe_instance(true)` is used
    /// for this instance specifically so bind fails clearly if some other
    /// process already owns this pipe name, rather than silently attaching
    /// to (potentially squatted-on) infrastructure.
    pub fn bind(config: ServerConfig) -> std::io::Result<Self> {
        let config = Arc::new(config);
        let options = build_server_options(&config, true);
        let pending =
            acl::create_named_pipe_with_acl(&options, &config.pipe_name, &config.acl_scope)?;
        Ok(Self {
            config,
            server_run_id: Uuid::new_v4(),
            pending,
        })
    }

    /// Waits for and accepts the next client connection, immediately
    /// preparing a fresh pipe instance so a subsequent client is never
    /// refused while this one is being handed off.
    pub async fn accept(&mut self) -> std::io::Result<Connection> {
        self.pending.connect().await?;
        let next = {
            let options = build_server_options(&self.config, false);
            acl::create_named_pipe_with_acl(
                &options,
                &self.config.pipe_name,
                &self.config.acl_scope,
            )?
        };
        let connected = std::mem::replace(&mut self.pending, next);
        Ok(Connection {
            pipe: connected,
            config: Arc::clone(&self.config),
            server_run_id: self.server_run_id,
        })
    }

    /// Runs the accept loop, spawning [`Connection::handle_until_shutdown`]
    /// for each client, until `shutdown` resolves. Accept errors are logged
    /// and do not stop the loop (a single failed accept must not take down
    /// the whole server).
    ///
    /// Unlike the pre-`P1-IPC-SHUTDOWN-CORR` version of this method, `serve`
    /// does **not** return the instant `shutdown` resolves. It first asks
    /// every still-open connection to close (rather than waiting
    /// indefinitely for the client to disconnect on its own, which may
    /// never happen — see `tests/shutdown_drain.rs`), then actually waits
    /// for every spawned per-connection dispatch task to finish, bounded by
    /// [`CONNECTION_DRAIN_GRACE`] (after which any still-running tasks are
    /// forcibly aborted, guaranteeing any `BusinessService`/resource clone
    /// they still hold is dropped before this method returns). This closes
    /// a real, reproduced use-after-free: a caller such as
    /// `crate::host::compose::ComposedHost::run` tears down a resource
    /// (`loki-storage`'s secure-memory arena) a still-running connection's
    /// `BusinessService` clone may reference, and previously had no way to
    /// know that teardown was actually safe.
    pub async fn serve<F>(&mut self, mut shutdown: F)
    where
        F: std::future::Future<Output = ()> + Unpin,
    {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut connections: JoinSet<()> = JoinSet::new();
        loop {
            tokio::select! {
                accepted = self.accept() => {
                    match accepted {
                        Ok(connection) => {
                            let connection_shutdown = shutdown_rx.clone();
                            connections.spawn(async move {
                                if let Err(err) = connection.handle_until_shutdown(connection_shutdown).await {
                                    tracing::debug!(error = %err, "ipc connection ended");
                                }
                            });
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "failed to accept an ipc connection");
                        }
                    }
                }
                _ = &mut shutdown => break,
            }
        }

        // Ask every still-open connection to stop reading and close. A
        // connection blocked waiting for its next frame (the ordinary case
        // for a client that stays connected across a host shutdown/restart)
        // would otherwise never return on its own — see this method's doc
        // comment.
        let _ = shutdown_tx.send(true);

        let drain = async { while connections.join_next().await.is_some() {} };
        tokio::select! {
            () = drain => {}
            () = tokio::time::sleep(CONNECTION_DRAIN_GRACE) => {
                tracing::warn!(
                    "one or more ipc connections did not close within the shutdown drain grace \
                     period; aborting the remaining dispatch tasks"
                );
                connections.abort_all();
                while connections.join_next().await.is_some() {}
            }
        }
    }
}

fn build_server_options(config: &ServerConfig, first_instance: bool) -> ServerOptions {
    let mut options = ServerOptions::new();
    options.first_pipe_instance(first_instance);
    options.out_buffer_size(config.out_buffer_size);
    options.in_buffer_size(config.in_buffer_size);
    options
}

/// One accepted, not-yet-handshaken client connection.
pub struct Connection {
    pipe: NamedPipeServer,
    config: Arc<ServerConfig>,
    server_run_id: Uuid,
}

// Manual, not derived: `NamedPipeServer` doesn't implement `Debug`. This
// exists so `{:?}`-formatting a `Connection` (e.g. in a test's
// `.expect(...)`/`panic!(...)` on an unexpected accept outcome) works at all.
impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("server_run_id", &self.server_run_id)
            .finish_non_exhaustive()
    }
}

impl Connection {
    /// Runs the handshake, then the post-handshake dispatch loop, until the
    /// client disconnects or a protocol error ends the connection. Never
    /// closes a connection on its own initiative — equivalent to calling
    /// [`Self::handle_until_shutdown`] with a `shutdown` signal that never
    /// fires, so every existing caller of this method keeps its exact
    /// pre-`P1-IPC-SHUTDOWN-CORR` behavior.
    pub async fn handle(self) -> Result<(), ConnectionError> {
        let (_never_signals_shutdown, shutdown_rx) = watch::channel(false);
        self.handle_until_shutdown(shutdown_rx).await
    }

    /// Identical to [`Self::handle`], except the handshake wait for `Hello`
    /// and the post-handshake dispatch loop's wait for the next frame are
    /// both additionally raced against `shutdown` becoming `true`. Used by
    /// [`Server::serve`] so a server-initiated shutdown can actually close
    /// every still-open connection — including one that is otherwise idle,
    /// waiting on a client that has no reason to disconnect on its own —
    /// instead of leaving it running detached forever. Reaching the
    /// `shutdown` branch during the handshake ends the connection with
    /// [`ConnectionError::ServerShutdown`]; reaching it during dispatch ends
    /// the connection the same way a normal client disconnect would
    /// (`Ok(())`), after the same graceful writer-drain every other
    /// disconnect path already goes through.
    pub(crate) async fn handle_until_shutdown(
        self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectionError> {
        let Connection {
            pipe,
            config,
            server_run_id,
        } = self;
        let (mut reader, mut writer) = tokio::io::split(pipe);

        let nonce = Uuid::new_v4();
        let challenge = WireMessage::Challenge {
            server_protocol_version: PROTOCOL_VERSION,
            server_capabilities: baseline_capabilities(),
            server_run_id,
            nonce,
        };
        write_frame(&mut writer, &challenge)
            .await
            .map_err(ConnectionError::Frame)?;

        let hello_message = tokio::select! {
            frame = read_frame(&mut reader, config.max_frame_bytes) => {
                match frame {
                    Ok(Some(message)) => message,
                    Ok(None) => return Err(ConnectionError::ClosedDuringHandshake),
                    Err(err) => return Err(ConnectionError::Frame(err)),
                }
            }
            _ = shutdown.changed() => return Err(ConnectionError::ServerShutdown),
        };

        let hello = match auth::expect_hello(hello_message) {
            Ok(hello) => hello,
            Err(reason) => {
                reject(&mut writer, reason.clone()).await;
                return Err(ConnectionError::HandshakeRejected(reason));
            }
        };

        let accepted = auth::evaluate_hello_with_resolver(
            hello.client_protocol_version,
            &hello.client_capabilities,
            hello.nonce,
            &hello.token,
            PROTOCOL_VERSION,
            nonce,
            config.credential_resolver.as_ref(),
            CORE_CAPABILITY,
            time::unix_millis_now(),
        );

        let (negotiated_capabilities, principal) = match accepted {
            Ok((accept, principal)) => (accept.negotiated_capabilities, principal),
            Err(reason) => {
                reject(&mut writer, reason.clone()).await;
                return Err(ConnectionError::HandshakeRejected(reason));
            }
        };

        let session_id = Uuid::new_v4();
        write_frame(
            &mut writer,
            &WireMessage::HelloAck {
                negotiated_capabilities: negotiated_capabilities.clone(),
                session_id,
                principal: Some(principal.clone()),
            },
        )
        .await
        .map_err(ConnectionError::Frame)?;

        tracing::info!(
            session_id = %session_id,
            client_type = %hello.client_type,
            client_version = %hello.client_version,
            "ipc connection established"
        );

        run_dispatch_loop(
            reader,
            writer,
            &config,
            SessionAuthority {
                negotiated_capabilities,
                // Only the opaque reference is retained past this point —
                // never `hello.token`, never the resolved `principal`
                // itself (both are dropped at the end of this scope).
                principal_id: principal.principal_id.clone(),
                credential_resolver: Arc::clone(&config.credential_resolver),
                session_id,
                business_service: Arc::clone(&config.business_service),
            },
            &mut shutdown,
        )
        .await
    }
}

/// Sends a `HelloReject` best-effort; failure to even write the rejection
/// (e.g. the client already hung up) is not itself escalated — the caller
/// already has the real reason to report.
async fn reject<W: tokio::io::AsyncWrite + Unpin>(writer: &mut W, reason: HandshakeRejectReason) {
    let _ = write_frame(
        writer,
        &WireMessage::HelloReject {
            reason,
            server_protocol_version: PROTOCOL_VERSION,
        },
    )
    .await;
}

async fn run_dispatch_loop<R, W>(
    mut reader: R,
    mut writer: W,
    config: &Arc<ServerConfig>,
    session_authority: SessionAuthority,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<(), ConnectionError>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (outgoing_tx, mut outgoing_rx) =
        mpsc::channel::<WireMessage>(config.outgoing_channel_capacity);
    let writer_task = tokio::spawn(async move {
        while let Some(message) = outgoing_rx.recv().await {
            if write_frame(&mut writer, &message).await.is_err() {
                break;
            }
        }
    });

    let mut cancel_senders: HashMap<Uuid, oneshot::Sender<()>> = HashMap::new();
    let (completed_tx, mut completed_rx) = mpsc::unbounded_channel::<Uuid>();
    // Every per-call task (`spawn_unary_call`/`spawn_stream_call`) is spawned
    // into this connection-local `JoinSet` rather than a bare `tokio::spawn`,
    // and drained (bounded by `CALL_TASK_DRAIN_GRACE`, see its doc comment)
    // below before this dispatch loop's own future resolves — see
    // `CALL_TASK_DRAIN_GRACE`'s doc comment for the use-after-free-class gap
    // this closes.
    let mut call_tasks: JoinSet<()> = JoinSet::new();

    let outcome = loop {
        tokio::select! {
            frame = read_frame(&mut reader, config.max_frame_bytes) => {
                match frame {
                    Ok(Some(WireMessage::Call { correlation_id, deadline_unix_millis, method })) => {
                        let (cancel_tx, cancel_rx) = oneshot::channel();
                        cancel_senders.insert(correlation_id, cancel_tx);
                        spawn_unary_call(
                            &mut call_tasks,
                            correlation_id,
                            deadline_unix_millis,
                            method,
                            session_authority.clone(),
                            outgoing_tx.clone(),
                            cancel_rx,
                            completed_tx.clone(),
                        );
                    }
                    Ok(Some(WireMessage::CallStream { correlation_id, deadline_unix_millis, method })) => {
                        let (cancel_tx, cancel_rx) = oneshot::channel();
                        cancel_senders.insert(correlation_id, cancel_tx);
                        spawn_stream_call(
                            &mut call_tasks,
                            correlation_id,
                            deadline_unix_millis,
                            method,
                            session_authority.clone(),
                            outgoing_tx.clone(),
                            cancel_rx,
                            completed_tx.clone(),
                        );
                    }
                    Ok(Some(WireMessage::CancelCall { correlation_id })) => {
                        if let Some(tx) = cancel_senders.remove(&correlation_id) {
                            let _ = tx.send(());
                        }
                    }
                    Ok(Some(unexpected)) => {
                        tracing::warn!(
                            message = ?unexpected,
                            "protocol violation: unexpected message type after handshake"
                        );
                        break Err(ConnectionError::ProtocolViolation(
                            "unexpected message type after handshake".to_string(),
                        ));
                    }
                    Ok(None) => break Ok(()),
                    Err(err) => {
                        tracing::warn!(error = %err, "malformed frame after handshake; closing connection");
                        break Err(ConnectionError::Frame(err));
                    }
                }
            }
            Some(done_id) = completed_rx.recv() => {
                cancel_senders.remove(&done_id);
            }
            _ = shutdown.changed() => {
                // A server-initiated shutdown (see `Server::serve`), not a
                // client-side protocol violation or disconnect — end the
                // connection the same way a clean disconnect would.
                break Ok(());
            }
        }
    };

    // Bounded wait for every per-call task spawned during this connection's
    // life to actually finish — before dropping this function's own
    // `outgoing_tx` clone or touching the writer task — so a still-running
    // business call (bounded only by its own client-supplied
    // `deadline_unix_millis`, not by anything server-side) cannot outlive
    // this dispatch loop's own future undetected. Any per-call task still
    // running once `CALL_TASK_DRAIN_GRACE` elapses is forcibly aborted; the
    // writer task (still running throughout this wait) keeps draining
    // whatever those calls already sent, same as during normal operation.
    let call_drain = async { while call_tasks.join_next().await.is_some() {} };
    tokio::select! {
        () = call_drain => {}
        () = tokio::time::sleep(CALL_TASK_DRAIN_GRACE) => {
            tracing::warn!(
                "one or more in-flight calls did not finish within the call-task drain grace \
                 period; aborting the remaining per-call tasks"
            );
            call_tasks.abort_all();
            while call_tasks.join_next().await.is_some() {}
        }
    }

    drop(outgoing_tx);
    // Bounded wait for the writer task to drain and exit on its own;
    // `.abort()` on the timeout branch (rather than letting the `timeout()`
    // future merely stop *polling* the JoinHandle) guarantees the task is
    // actually stopped and the pipe's write half released, not left running
    // detached in the background. Every per-call task's own `outgoing_tx`
    // clone has already been dropped by the drain above (each task drops
    // its clone when it finishes, is aborted, or was never given a chance
    // to run), so this wait is now bounded purely by the writer task's own
    // drain of whatever is left in the channel — it can never be kept alive
    // by a still-running call task holding a live sender clone.
    let mut writer_task = writer_task;
    tokio::select! {
        _ = &mut writer_task => {}
        () = tokio::time::sleep(WRITER_SHUTDOWN_GRACE) => {
            writer_task.abort();
        }
    }

    outcome
}

/// Sends `message`, giving up after [`OUTGOING_SEND_TIMEOUT`] if the
/// channel stays full that long (e.g. a client that stopped reading
/// entirely and was never cancelled). Returns whether the send succeeded;
/// callers along the streaming path use this to stop producing further
/// items once the connection is clearly gone.
async fn send_message(tx: &mpsc::Sender<WireMessage>, message: WireMessage) -> bool {
    matches!(
        tokio::time::timeout(OUTGOING_SEND_TIMEOUT, tx.send(message)).await,
        Ok(Ok(()))
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_unary_call(
    call_tasks: &mut JoinSet<()>,
    correlation_id: Uuid,
    deadline_unix_millis: u64,
    method: Method,
    session_authority: SessionAuthority,
    outgoing_tx: mpsc::Sender<WireMessage>,
    mut cancel_rx: oneshot::Receiver<()>,
    completed_tx: mpsc::UnboundedSender<Uuid>,
) {
    call_tasks.spawn(async move {
        let deadline = time::duration_until(deadline_unix_millis);
        let outcome = tokio::select! {
            outcome = execute_method(
                correlation_id,
                method,
                &session_authority,
            ) => outcome,
            _ = tokio::time::sleep(deadline) => CallOutcome::Err { error: RpcError::DeadlineExceeded },
            _ = &mut cancel_rx => CallOutcome::Err { error: RpcError::Cancelled },
        };
        send_message(
            &outgoing_tx,
            WireMessage::CallResult {
                correlation_id,
                outcome,
            },
        )
        .await;
        let _ = completed_tx.send(correlation_id);
    });
}

async fn execute_method(
    correlation_id: Uuid,
    method: Method,
    session_authority: &SessionAuthority,
) -> CallOutcome {
    if let Method::Ping(params) = method {
        return CallOutcome::Ok {
            result: MethodResult::Ping(PongResult {
                payload: params.payload,
                server_unix_millis: time::unix_millis_now(),
            }),
        };
    }

    let principal = match session_authority.revalidate() {
        Ok(principal) => principal,
        Err(error) => return CallOutcome::Err { error },
    };
    if let Err(error) = authorize_method(
        &method,
        &session_authority.negotiated_capabilities,
        &principal,
    ) {
        return CallOutcome::Err { error };
    }
    let expected_target = target_entity_id(&method).map(str::to_string);
    let context = BusinessRequestContext {
        correlation_id,
        session_id: session_authority.session_id,
        principal,
    };
    match session_authority
        .business_service
        .call(method.clone(), context)
        .await
    {
        Ok(result)
            if result.validate_contract()
                && result.correlation_id() == correlation_id.to_string()
                && business_result_matches_method(&method, &result)
                && result_target_id(&result).is_none_or(|actual| {
                    expected_target
                        .as_deref()
                        .is_none_or(|expected| expected == actual)
                }) =>
        {
            CallOutcome::Ok {
                result: MethodResult::Business(result),
            }
        }
        Ok(_) => CallOutcome::Err {
            error: RpcError::Internal {
                message: "business producer returned an invalid or mismatched result".to_string(),
            },
        },
        Err(error) => CallOutcome::Err { error },
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_stream_call(
    call_tasks: &mut JoinSet<()>,
    correlation_id: Uuid,
    deadline_unix_millis: u64,
    method: StreamMethod,
    session_authority: SessionAuthority,
    outgoing_tx: mpsc::Sender<WireMessage>,
    mut cancel_rx: oneshot::Receiver<()>,
    completed_tx: mpsc::UnboundedSender<Uuid>,
) {
    call_tasks.spawn(async move {
        let deadline = time::duration_until(deadline_unix_millis);
        let result = tokio::select! {
            result = execute_stream_method(
                correlation_id,
                method,
                &outgoing_tx,
                &session_authority,
            ) => result,
            _ = tokio::time::sleep(deadline) => Err(RpcError::DeadlineExceeded),
            _ = &mut cancel_rx => Err(RpcError::Cancelled),
        };
        let outcome = match result {
            Ok(summary) => StreamOutcome::Ok { summary },
            Err(error) => StreamOutcome::Err { error },
        };
        send_message(
            &outgoing_tx,
            WireMessage::StreamEnd {
                correlation_id,
                outcome,
            },
        )
        .await;
        let _ = completed_tx.send(correlation_id);
    });
}

async fn execute_stream_method(
    correlation_id: Uuid,
    method: StreamMethod,
    outgoing_tx: &mpsc::Sender<WireMessage>,
    session_authority: &SessionAuthority,
) -> Result<StreamSummary, RpcError> {
    match method {
        StreamMethod::EchoStream(params) => {
            if params.count > MAX_ECHO_STREAM_COUNT {
                return Err(RpcError::InvalidRequest {
                    message: format!(
                        "count {} exceeds maximum {MAX_ECHO_STREAM_COUNT}",
                        params.count
                    ),
                });
            }
            for seq in 0..params.count {
                let item = WireMessage::StreamItem {
                    correlation_id,
                    seq,
                    item: StreamItemPayload::EchoStream(EchoStreamItem {
                        seq,
                        payload: params.payload.clone(),
                    }),
                };
                if !send_message(outgoing_tx, item).await {
                    return Err(RpcError::Internal {
                        message: "connection closed while streaming".to_string(),
                    });
                }
            }
            Ok(StreamSummary::EchoStream(EchoStreamSummary {
                items_sent: params.count,
            }))
        }
        StreamMethod::BusinessEvents(params) => {
            let principal = session_authority.revalidate()?;
            authorize_stream(
                &params,
                &session_authority.negotiated_capabilities,
                &principal,
            )?;
            let (event_tx, mut event_rx) = mpsc::channel(16);
            let context = BusinessRequestContext {
                correlation_id,
                session_id: session_authority.session_id,
                principal,
            };
            let producer = session_authority.business_service.stream(
                params.clone(),
                context,
                BusinessEventEmitter { sender: event_tx },
            );
            tokio::pin!(producer);
            let mut items_sent = 0_u16;
            let mut last_sequence = params.after_sequence;
            let producer_summary = loop {
                tokio::select! {
                    result = &mut producer => {
                        let summary = result?;
                        while let Some(event) = event_rx.recv().await {
                            // Dynamic revalidation before every forwarded
                            // event — an already-running subscription is
                            // denied as soon as its credential is revoked/
                            // expired, not only at the next `Hello`.
                            session_authority.revalidate()?;
                            forward_business_event(
                                correlation_id,
                                &params,
                                event,
                                outgoing_tx,
                                &mut items_sent,
                                &mut last_sequence,
                            ).await?;
                        }
                        break summary;
                    }
                    event = event_rx.recv() => {
                        let Some(event) = event else {
                            break producer.await?;
                        };
                        session_authority.revalidate()?;
                        forward_business_event(
                            correlation_id,
                            &params,
                            event,
                            outgoing_tx,
                            &mut items_sent,
                            &mut last_sequence,
                        ).await?;
                    }
                }
            };
            if !producer_summary.validate_contract(params.max_items)
                || producer_summary.items_sent != items_sent
                || producer_summary.last_sequence != last_sequence
            {
                return Err(RpcError::Internal {
                    message: "business producer returned a mismatched stream summary".to_string(),
                });
            }
            Ok(StreamSummary::BusinessEvents(producer_summary))
        }
    }
}

async fn forward_business_event(
    correlation_id: Uuid,
    params: &BusinessStreamParams,
    event: BusinessEvent,
    outgoing_tx: &mpsc::Sender<WireMessage>,
    items_sent: &mut u16,
    last_sequence: &mut Option<u64>,
) -> Result<(), RpcError> {
    if !event.validate_contract()
        || event.stream != params.stream
        || event.entity_id != params.entity_id
        || event.correlation_id != correlation_id.to_string()
        || last_sequence.is_some_and(|previous| event.sequence <= previous)
        || *items_sent >= params.max_items
    {
        return Err(RpcError::Internal {
            message: "business producer emitted an invalid or out-of-scope event".to_string(),
        });
    }
    let seq = u32::from(*items_sent);
    if !send_message(
        outgoing_tx,
        WireMessage::StreamItem {
            correlation_id,
            seq,
            item: StreamItemPayload::BusinessEvent(event.clone()),
        },
    )
    .await
    {
        return Err(RpcError::Internal {
            message: "connection closed while streaming".to_string(),
        });
    }
    *items_sent += 1;
    *last_sequence = Some(event.sequence);
    Ok(())
}

fn authorize_stream(
    params: &BusinessStreamParams,
    negotiated_capabilities: &BTreeSet<String>,
    principal: &AuthenticatedPrincipal,
) -> Result<(), RpcError> {
    if !negotiated_capabilities.contains(BUSINESS_CAPABILITY) {
        return Err(RpcError::MissingCapability {
            required: BUSINESS_CAPABILITY.to_string(),
        });
    }
    if params.max_items == 0
        || params.max_items > MAX_STREAM_ITEMS
        || !safe_id(&params.entity_id, 200)
    {
        return Err(RpcError::InvalidRequest {
            message: "business stream parameters failed bounds validation".to_string(),
        });
    }
    if !principal.is_authorized_for_entity(&params.entity_id)
        || !principal.authorizes(Authority::Subscribe)
    {
        return Err(RpcError::PermissionDenied);
    }
    let domain_authority = match params.stream {
        BusinessStreamName::RuntimeStatus => Authority::ReadRuntime,
        BusinessStreamName::DeviceProjection | BusinessStreamName::DeviceHealth => {
            Authority::ReadDevices
        }
        BusinessStreamName::Telemetry | BusinessStreamName::Logs => Authority::ReadTelemetry,
        BusinessStreamName::Incidents => Authority::ReadIncidents,
        BusinessStreamName::Jobs => Authority::ReadJobs,
        BusinessStreamName::Approvals => Authority::ReadApprovals,
        BusinessStreamName::Notifications => Authority::ReadNotifications,
        BusinessStreamName::WorkIntents => Authority::ReadWorkIntents,
    };
    if !principal.authorizes(domain_authority)
        || (params.stream == BusinessStreamName::Approvals
            && principal.kind != PrincipalKind::DesktopInstallation)
    {
        return Err(RpcError::PermissionDenied);
    }
    Ok(())
}

fn authorize_method(
    method: &Method,
    capabilities: &BTreeSet<String>,
    principal: &AuthenticatedPrincipal,
) -> Result<(), RpcError> {
    if matches!(method, Method::Ping(_)) {
        return Ok(());
    }
    if !capabilities.contains(BUSINESS_CAPABILITY) {
        return Err(RpcError::MissingCapability {
            required: BUSINESS_CAPABILITY.to_string(),
        });
    }
    validate_method(method)?;
    let required = required_authority(method);
    if !principal.authorizes(required) {
        return Err(RpcError::PermissionDenied);
    }
    if let Some(entity_id) = target_entity_id(method)
        && !principal.is_authorized_for_entity(entity_id)
    {
        return Err(RpcError::PermissionDenied);
    }
    if matches!(method, Method::ApprovalGrant(_) | Method::ApprovalDeny(_))
        && principal.kind != PrincipalKind::DesktopInstallation
    {
        return Err(RpcError::PermissionDenied);
    }
    Ok(())
}

fn required_authority(method: &Method) -> Authority {
    match method {
        Method::Ping(_) => Authority::ReadRuntime, // unreachable (Ping short-circuits above)
        Method::RuntimeGetStatus(_) => Authority::ReadRuntime,
        Method::DevicesList(_) | Method::DeviceGet(_) | Method::DeviceGetHealth(_) => {
            Authority::ReadDevices
        }
        Method::DeviceReadLogs(_) | Method::DeviceQueryTelemetry(_) => Authority::ReadTelemetry,
        Method::IncidentsList(_) | Method::IncidentGet(_) => Authority::ReadIncidents,
        Method::JobGet(_) => Authority::ReadJobs,
        Method::AgentRunRegister(_) | Method::AgentRunUpdate(_) => Authority::ManageAgentRuns,
        Method::WorkIntentAcknowledge(_) => Authority::AcknowledgeWorkIntents,
        Method::HealthCheckRequest(_) | Method::PreflightRequest(_) => {
            Authority::RequestDiagnostics
        }
        Method::JobCancel(_) => Authority::CancelJobs,
        Method::BuildRequest(_) => Authority::RequestBuild,
        Method::RestartRequest(_)
        | Method::FlashRequest(_)
        | Method::TestOnDeviceRequest(_)
        | Method::DeploymentRequest(_)
        | Method::RollbackRequest(_) => Authority::RequestDeviceOperation,
        Method::ApprovalGrant(_) | Method::ApprovalDeny(_) => Authority::DecideApproval,
    }
}

/// The exact `device_id`/`run_id`/`work_intent_id`/etc. field a
/// single-entity method's request targets, or `None` for list/global
/// methods (`RuntimeGetStatus`, `DevicesList`, `AgentRunRegister` — which
/// targets a not-yet-existing entity) that have no single request-scoped
/// entity to bind authorization/response-identity to.
fn target_entity_id(method: &Method) -> Option<&str> {
    match method {
        Method::Ping(_) | Method::RuntimeGetStatus(_) | Method::DevicesList(_) => None,
        Method::DeviceGet(v) => Some(&v.device_id),
        Method::DeviceGetHealth(v) => Some(&v.device_id),
        Method::DeviceReadLogs(v) => Some(&v.device_id),
        Method::DeviceQueryTelemetry(v) => Some(&v.device_id),
        Method::IncidentsList(_) => None,
        Method::IncidentGet(v) => Some(&v.incident_id),
        Method::JobGet(v) => Some(&v.job_id),
        Method::AgentRunRegister(_) => None,
        Method::AgentRunUpdate(v) => Some(&v.run_id),
        Method::WorkIntentAcknowledge(v) => Some(&v.work_intent_id),
        Method::HealthCheckRequest(v) | Method::PreflightRequest(v) => Some(&v.device_id),
        Method::JobCancel(v) => Some(&v.job_id),
        Method::BuildRequest(v) => Some(&v.target_id),
        Method::RestartRequest(v) => Some(&v.device_id),
        Method::FlashRequest(v) => Some(&v.device_id),
        Method::TestOnDeviceRequest(v) => Some(&v.device_id),
        Method::DeploymentRequest(v) => Some(&v.device_id),
        Method::RollbackRequest(v) => Some(&v.device_id),
        Method::ApprovalGrant(v) => Some(&v.approval_id),
        Method::ApprovalDeny(v) => Some(&v.approval_id),
    }
}

/// The entity identity actually carried by a `BusinessRpcResult`, for
/// comparison against [`target_entity_id`]. `None` means "this result shape
/// carries no single entity identity to check" (list results, or a `None`
/// `device`/`incident`/`job` — a legitimate not-found response, not a
/// mismatch).
fn result_target_id(result: &BusinessRpcResult) -> Option<&str> {
    match result {
        BusinessRpcResult::RuntimeStatus { .. } | BusinessRpcResult::DevicesList { .. } => None,
        BusinessRpcResult::DeviceGet { device, .. } => {
            device.as_ref().map(|d| d.device_id.as_str())
        }
        BusinessRpcResult::DeviceHealth { device_id, .. } => Some(device_id),
        BusinessRpcResult::Mutation { target_id, .. } => Some(target_id),
    }
}

pub(crate) fn validate_method(method: &Method) -> Result<(), RpcError> {
    // `loki_acceptance_store::accept_mutation`'s real behavior (verified by
    // direct inspection): an approval-gated method with no
    // `approval_reference` at all is a caller/construction error
    // (`MutationContractError::InvalidInput`), never a graceful
    // `AcceptanceResult::ApprovalRequired` — that variant exists in the
    // closed contract type but nothing in `loki-acceptance-store`
    // currently constructs it (approval *issuance* is a different,
    // not-yet-built surface, out of this node's scope). This crate
    // therefore requires the field to be present and well-formed before
    // ever attempting submission, turning what would otherwise be an ugly
    // `RpcError::Internal` from a downstream contract error into an
    // honest, precise `RpcError::InvalidRequest` at the transport
    // boundary — never silently attempting a submission this crate already
    // knows must fail.
    let approval_reference_required =
        |reference: &Option<crate::business::ApprovalReferenceParams>| {
            reference.as_ref().is_some_and(|reference| {
                safe_id(&reference.approval_id, 200)
                    && safe_request_digest(&reference.bound_request_digest)
                    && reference.expires_at_ms > 0
            })
        };
    let valid = match method {
        Method::Ping(_) | Method::RuntimeGetStatus(_) => true,
        Method::DevicesList(v) => {
            (1..=crate::business::MAX_PAGE_SIZE).contains(&v.limit)
                && v.after_device_id
                    .as_deref()
                    .is_none_or(|id| safe_id(id, 200))
        }
        Method::IncidentsList(v) => valid_page(v.limit, v.cursor.as_deref()),
        Method::DeviceGet(v) => safe_id(&v.device_id, 200),
        Method::DeviceGetHealth(v) => safe_id(&v.device_id, 200),
        Method::DeviceReadLogs(v) => {
            safe_id(&v.device_id, 200) && valid_page(v.limit, v.cursor.as_deref())
        }
        Method::DeviceQueryTelemetry(v) => {
            safe_id(&v.device_id, 200)
                && valid_page(v.limit, v.cursor.as_deref())
                && v.signal_ids.len() <= 64
                && v.signal_ids.iter().all(|s| bounded(s, 128))
        }
        Method::IncidentGet(v) => safe_id(&v.incident_id, 200),
        Method::JobGet(v) => safe_id(&v.job_id, 200),
        Method::AgentRunRegister(v) => {
            v.parent_run_id.as_deref().is_none_or(|id| safe_id(id, 200))
                && safe_digest(&v.parameters_digest)
                && safe_id(&v.idempotency_key, 200)
        }
        Method::JobCancel(v) => safe_id(&v.job_id, 200) && safe_id(&v.idempotency_key, 200),
        Method::AgentRunUpdate(v) => {
            safe_id(&v.run_id, 200)
                && matches!(
                    v.phase.as_str(),
                    "planning" | "working" | "waiting" | "completed" | "failed"
                )
                && bounded(&v.summary, crate::business::MAX_TEXT_BYTES)
                && safe_digest(&v.parameters_digest)
                && safe_id(&v.idempotency_key, 200)
        }
        Method::WorkIntentAcknowledge(v) => {
            safe_id(&v.work_intent_id, 200)
                && safe_id(&v.run_id, 200)
                && safe_digest(&v.parameters_digest)
                && safe_id(&v.idempotency_key, 200)
        }
        Method::HealthCheckRequest(v) | Method::PreflightRequest(v) => {
            safe_id(&v.device_id, 200) && safe_id(&v.idempotency_key, 200)
        }
        Method::BuildRequest(v) => {
            safe_id(&v.source_snapshot_id, 200)
                && safe_id(&v.target_id, 200)
                && safe_id(&v.recipe_id, 200)
                && safe_digest(&v.parameters_digest)
                && safe_id(&v.idempotency_key, 200)
                && approval_reference_required(&v.approval_reference)
        }
        Method::RestartRequest(v) => {
            safe_id(&v.device_id, 200)
                && safe_digest(&v.parameters_digest)
                && safe_id(&v.idempotency_key, 200)
                && approval_reference_required(&v.approval_reference)
        }
        Method::FlashRequest(v) => {
            safe_id(&v.device_id, 200)
                && safe_id(&v.artifact_id, 200)
                && safe_digest(&v.artifact_digest)
                && safe_digest(&v.parameters_digest)
                && safe_id(&v.idempotency_key, 200)
                && approval_reference_required(&v.approval_reference)
        }
        Method::TestOnDeviceRequest(v) => {
            safe_id(&v.device_id, 200)
                && safe_id(&v.test_suite_id, 200)
                && safe_id(&v.idempotency_key, 200)
        }
        Method::DeploymentRequest(v) => {
            safe_id(&v.device_id, 200)
                && safe_id(&v.artifact_id, 200)
                && safe_digest(&v.artifact_digest)
                && bounded(&v.verification_plan_id, 200)
                && bounded(&v.rollback_plan_id, 200)
                && safe_id(&v.idempotency_key, 200)
        }
        Method::RollbackRequest(v) => {
            safe_id(&v.device_id, 200)
                && safe_id(&v.deployment_id, 200)
                && bounded(&v.recovery_policy_id, 200)
                && safe_id(&v.idempotency_key, 200)
        }
        Method::ApprovalGrant(v) => {
            safe_id(&v.approval_id, 200)
                && safe_digest(&v.bound_request_digest)
                && safe_id(&v.idempotency_key, 200)
        }
        Method::ApprovalDeny(v) => {
            safe_id(&v.approval_id, 200)
                && safe_digest(&v.bound_request_digest)
                && bounded(&v.reason, crate::business::MAX_TEXT_BYTES)
                && safe_id(&v.idempotency_key, 200)
        }
    };
    valid.then_some(()).ok_or_else(|| RpcError::InvalidRequest {
        message: "business RPC parameters failed bounds validation".to_string(),
    })
}

fn method_name(method: &Method) -> &'static str {
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

/// The six mutation-backed `Method`s' expected `OperationMethod` wire tag —
/// used by tests/support fixtures (and any future real `BusinessService`)
/// to know which `loki_mutation_contract::MutationMethod` a given `Method`
/// corresponds to. `None` for the other 17 methods (4 real-backed reads +
/// 13 permanently-unsupported — see `crate::business`'s top doc comment).
pub fn mutation_method_for(method: &Method) -> Option<OperationMethod> {
    match method {
        Method::RestartRequest(_) => Some(OperationMethod::Restart),
        Method::FlashRequest(_) => Some(OperationMethod::Flash),
        Method::BuildRequest(_) => Some(OperationMethod::ContainedBuild),
        Method::AgentRunRegister(_) => Some(OperationMethod::AgentRunRegister),
        Method::AgentRunUpdate(_) => Some(OperationMethod::AgentRunUpdate),
        Method::WorkIntentAcknowledge(_) => Some(OperationMethod::WorkIntentAcknowledge),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::PrincipalKind;

    fn unsupported_service() -> Arc<dyn BusinessService> {
        Arc::new(UnsupportedBusinessService)
    }

    fn session(principal: AuthenticatedPrincipal) -> SessionAuthority {
        let resolver: Arc<dyn CredentialResolver> = Arc::new(CredentialSet::single(
            AuthToken::new("t"),
            principal.clone(),
        ));
        SessionAuthority {
            negotiated_capabilities: baseline_capabilities(),
            principal_id: principal.principal_id,
            credential_resolver: resolver,
            session_id: Uuid::nil(),
            business_service: unsupported_service(),
        }
    }

    #[tokio::test]
    async fn outgoing_channel_with_capacity_one_reports_full_when_saturated() {
        let (tx, _rx) = mpsc::channel::<WireMessage>(1);
        let msg = || WireMessage::CancelCall {
            correlation_id: Uuid::nil(),
        };
        assert!(tx.try_send(msg()).is_ok(), "first send fills the one slot");
        let second = tx.try_send(msg());
        assert!(
            matches!(second, Err(mpsc::error::TrySendError::Full(_))),
            "a bounded channel at capacity must report Full, not buffer unboundedly"
        );
    }

    #[tokio::test]
    async fn execute_ping_echoes_payload_with_no_authority_required() {
        let principal = AuthenticatedPrincipal::runtime_service("test");
        let outcome = execute_method(
            Uuid::nil(),
            Method::Ping(crate::protocol::PingParams {
                payload: "hi".to_string(),
            }),
            &session(principal),
        )
        .await;
        match outcome {
            CallOutcome::Ok {
                result: MethodResult::Ping(pong),
            } => assert_eq!(pong.payload, "hi"),
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[tokio::test]
    async fn echo_stream_over_the_configured_maximum_is_rejected_as_invalid_request() {
        let (tx, mut rx) = mpsc::channel(4);
        let principal = AuthenticatedPrincipal::runtime_service("test");
        let result = execute_stream_method(
            Uuid::nil(),
            StreamMethod::EchoStream(crate::protocol::EchoStreamParams {
                payload: "x".to_string(),
                count: MAX_ECHO_STREAM_COUNT + 1,
            }),
            &tx,
            &session(principal),
        )
        .await;
        assert!(matches!(result, Err(RpcError::InvalidRequest { .. })));
        drop(tx);
        assert!(rx.recv().await.is_none(), "no items should have been sent");
    }

    #[tokio::test]
    async fn echo_stream_sends_every_item_in_order() {
        let (tx, mut rx) = mpsc::channel(16);
        let principal = AuthenticatedPrincipal::runtime_service("test");
        let handle = tokio::spawn(async move {
            execute_stream_method(
                Uuid::nil(),
                StreamMethod::EchoStream(crate::protocol::EchoStreamParams {
                    payload: "x".to_string(),
                    count: 5,
                }),
                &tx,
                &session(principal),
            )
            .await
        });

        let mut seen = Vec::new();
        while let Some(WireMessage::StreamItem { seq, .. }) = rx.recv().await {
            seen.push(seq);
        }
        assert_eq!(seen, vec![0, 1, 2, 3, 4]);
        let result = handle.await.unwrap();
        assert!(matches!(
            result,
            Ok(StreamSummary::EchoStream(EchoStreamSummary {
                items_sent: 5
            }))
        ));
    }

    #[tokio::test]
    async fn business_call_without_authority_is_denied_before_reaching_the_unsupported_service() {
        let principal = AuthenticatedPrincipal {
            kind: PrincipalKind::DesktopInstallation,
            principal_id: "desktop-denied".to_string(),
            authorized_entity_ids: BTreeSet::from(["dev:1".to_string()]),
            authority: BTreeSet::new(),
        };
        let outcome = execute_method(
            Uuid::nil(),
            Method::DeviceGet(crate::business::DeviceParams {
                device_id: "dev:1".to_string(),
            }),
            &session(principal),
        )
        .await;
        assert!(matches!(
            outcome,
            CallOutcome::Err {
                error: RpcError::PermissionDenied
            }
        ));
    }

    #[tokio::test]
    async fn business_call_for_an_unauthorized_entity_is_denied_even_with_the_domain_authority() {
        // Cross-identity rejection: the principal has ReadDevices but is
        // not authorized for THIS device.
        let principal = AuthenticatedPrincipal {
            kind: PrincipalKind::DesktopInstallation,
            principal_id: "desktop-1".to_string(),
            authorized_entity_ids: BTreeSet::from(["dev:authorized".to_string()]),
            authority: BTreeSet::from([Authority::ReadDevices]),
        };
        let outcome = execute_method(
            Uuid::nil(),
            Method::DeviceGet(crate::business::DeviceParams {
                device_id: "dev:other".to_string(),
            }),
            &session(principal),
        )
        .await;
        assert!(matches!(
            outcome,
            CallOutcome::Err {
                error: RpcError::PermissionDenied
            }
        ));
    }

    #[tokio::test]
    async fn revoked_credential_is_denied_on_the_next_call_not_only_at_hello() {
        let principal = AuthenticatedPrincipal {
            kind: PrincipalKind::DesktopInstallation,
            principal_id: "desktop-1".to_string(),
            authorized_entity_ids: BTreeSet::new(),
            authority: BTreeSet::from([Authority::ReadRuntime]),
        };
        let mut resolver = CredentialSet::single(AuthToken::new("t"), principal.clone());
        let outcome_before = execute_method(
            Uuid::nil(),
            Method::RuntimeGetStatus(crate::business::EmptyParams {}),
            &SessionAuthority {
                negotiated_capabilities: baseline_capabilities(),
                principal_id: principal.principal_id.clone(),
                credential_resolver: Arc::new(resolver.clone()),
                session_id: Uuid::nil(),
                business_service: unsupported_service(),
            },
        )
        .await;
        // Fails closed with UnsupportedMethod (no real backing service in
        // this test), proving authorization passed.
        assert!(matches!(
            outcome_before,
            CallOutcome::Err {
                error: RpcError::UnsupportedMethod { .. }
            }
        ));

        resolver.revoke("desktop-1");
        let outcome_after = execute_method(
            Uuid::nil(),
            Method::RuntimeGetStatus(crate::business::EmptyParams {}),
            &SessionAuthority {
                negotiated_capabilities: baseline_capabilities(),
                principal_id: principal.principal_id,
                credential_resolver: Arc::new(resolver),
                session_id: Uuid::nil(),
                business_service: unsupported_service(),
            },
        )
        .await;
        assert!(matches!(
            outcome_after,
            CallOutcome::Err {
                error: RpcError::CredentialRevalidationFailed
            }
        ));
    }

    #[test]
    fn every_stream_requires_its_domain_authority_and_bound_entity_id() {
        let cases = [
            (BusinessStreamName::RuntimeStatus, Authority::ReadRuntime),
            (BusinessStreamName::DeviceProjection, Authority::ReadDevices),
            (BusinessStreamName::DeviceHealth, Authority::ReadDevices),
            (BusinessStreamName::Telemetry, Authority::ReadTelemetry),
            (BusinessStreamName::Logs, Authority::ReadTelemetry),
            (BusinessStreamName::Incidents, Authority::ReadIncidents),
            (BusinessStreamName::Jobs, Authority::ReadJobs),
            (BusinessStreamName::Approvals, Authority::ReadApprovals),
            (
                BusinessStreamName::Notifications,
                Authority::ReadNotifications,
            ),
            (BusinessStreamName::WorkIntents, Authority::ReadWorkIntents),
        ];
        for (stream, required) in cases {
            let params = BusinessStreamParams {
                stream,
                entity_id: "entity-1".to_string(),
                after_sequence: None,
                max_items: 10,
            };
            let allowed = AuthenticatedPrincipal {
                kind: PrincipalKind::DesktopInstallation,
                principal_id: "desktop-1".to_string(),
                authorized_entity_ids: BTreeSet::from(["entity-1".to_string()]),
                authority: BTreeSet::from([Authority::Subscribe, required]),
            };
            assert!(authorize_stream(&params, &baseline_capabilities(), &allowed).is_ok());
            let wrong_domain = AuthenticatedPrincipal {
                authority: BTreeSet::from([Authority::Subscribe]),
                ..allowed.clone()
            };
            assert!(matches!(
                authorize_stream(&params, &baseline_capabilities(), &wrong_domain),
                Err(RpcError::PermissionDenied)
            ));
            let wrong_entity = AuthenticatedPrincipal {
                authorized_entity_ids: BTreeSet::from(["entity-2".to_string()]),
                ..allowed.clone()
            };
            assert!(matches!(
                authorize_stream(&params, &baseline_capabilities(), &wrong_entity),
                Err(RpcError::PermissionDenied)
            ));
            let missing_subscribe = AuthenticatedPrincipal {
                authority: BTreeSet::from([required]),
                ..allowed
            };
            assert!(matches!(
                authorize_stream(&params, &baseline_capabilities(), &missing_subscribe),
                Err(RpcError::PermissionDenied)
            ));
        }
    }

    #[test]
    fn approvals_stream_requires_a_desktop_installation_principal() {
        let params = BusinessStreamParams {
            stream: BusinessStreamName::Approvals,
            entity_id: "approval-1".to_string(),
            after_sequence: None,
            max_items: 10,
        };
        let agent = AuthenticatedPrincipal {
            kind: PrincipalKind::AgentConnection,
            principal_id: "agent-1".to_string(),
            authorized_entity_ids: BTreeSet::from(["approval-1".to_string()]),
            authority: BTreeSet::from([Authority::Subscribe, Authority::ReadApprovals]),
        };
        assert!(matches!(
            authorize_stream(&params, &baseline_capabilities(), &agent),
            Err(RpcError::PermissionDenied)
        ));
    }

    #[test]
    fn approval_decision_methods_require_a_desktop_installation_principal() {
        let method = Method::ApprovalGrant(crate::business::ApprovalGrantParams {
            approval_id: "apr_1".to_string(),
            bound_request_digest: "a".repeat(64),
            idempotency_key: "idem-1".to_string(),
        });
        let agent = AuthenticatedPrincipal {
            kind: PrincipalKind::AgentConnection,
            principal_id: "agent-1".to_string(),
            authorized_entity_ids: BTreeSet::from(["apr_1".to_string()]),
            authority: BTreeSet::from([Authority::DecideApproval]),
        };
        assert!(matches!(
            authorize_method(&method, &baseline_capabilities(), &agent),
            Err(RpcError::PermissionDenied)
        ));
        let desktop = AuthenticatedPrincipal {
            kind: PrincipalKind::DesktopInstallation,
            ..agent
        };
        assert!(authorize_method(&method, &baseline_capabilities(), &desktop).is_ok());
    }
}
