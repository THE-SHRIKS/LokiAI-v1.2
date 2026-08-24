//! Named-pipe server: accept loop, per-connection handshake, and RPC
//! dispatch with per-call deadlines, cancellation, and streaming
//! backpressure.
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
//! there is no separate "please stop soon" flag the work has to poll. This
//! mirrors `crates/loki-runtime/src/runtime.rs`'s established
//! shutdown-vs-heartbeat race pattern in this workspace.

use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::acl::{self, AclScope};
use crate::auth::{self, AuthToken, CredentialResolver, CredentialSet};
use crate::business::{
    AuthenticatedPrincipal, Authority, BUSINESS_CAPABILITY, BusinessEvent, BusinessRpcResult,
    BusinessStreamName, BusinessStreamParams, BusinessStreamSummary, MAX_STREAM_ITEMS,
    PrincipalKind, bounded, safe_digest, safe_id, valid_page,
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

#[derive(Clone)]
struct SessionAuthority {
    negotiated_capabilities: BTreeSet<String>,
    principal: AuthenticatedPrincipal,
    session_id: Uuid,
    business_service: Arc<dyn BusinessService>,
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
/// credential resolver binds each successful handshake to its own principal.
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

    /// Runs the accept loop, spawning [`Connection::handle`] for each
    /// client, until `shutdown` resolves. Accept errors are logged and do
    /// not stop the loop (a single failed accept must not take down the
    /// whole server).
    pub async fn serve<F>(&mut self, mut shutdown: F)
    where
        F: std::future::Future<Output = ()> + Unpin,
    {
        loop {
            tokio::select! {
                accepted = self.accept() => {
                    match accepted {
                        Ok(connection) => {
                            tokio::spawn(async move {
                                if let Err(err) = connection.handle().await {
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
    /// client disconnects or a protocol error ends the connection.
    pub async fn handle(self) -> Result<(), ConnectionError> {
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

        let hello_message = match read_frame(&mut reader, config.max_frame_bytes).await {
            Ok(Some(message)) => message,
            Ok(None) => return Err(ConnectionError::ClosedDuringHandshake),
            Err(err) => return Err(ConnectionError::Frame(err)),
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
                principal,
                session_id,
                business_service: Arc::clone(&config.business_service),
            },
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

    let outcome = loop {
        tokio::select! {
            frame = read_frame(&mut reader, config.max_frame_bytes) => {
                match frame {
                    Ok(Some(WireMessage::Call { correlation_id, deadline_unix_millis, method })) => {
                        let (cancel_tx, cancel_rx) = oneshot::channel();
                        cancel_senders.insert(correlation_id, cancel_tx);
                        spawn_unary_call(
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
        }
    };

    drop(outgoing_tx);
    // Bounded wait for the writer task to drain and exit on its own;
    // `.abort()` on the timeout branch (rather than letting the `timeout()`
    // future merely stop *polling* the JoinHandle) guarantees the task is
    // actually stopped and the pipe's write half released, not left running
    // detached in the background — the same "bounded, not a silent hang"
    // standard this crate applies to per-call deadlines.
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

fn spawn_unary_call(
    correlation_id: Uuid,
    deadline_unix_millis: u64,
    method: Method,
    session_authority: SessionAuthority,
    outgoing_tx: mpsc::Sender<WireMessage>,
    mut cancel_rx: oneshot::Receiver<()>,
    completed_tx: mpsc::UnboundedSender<Uuid>,
) {
    tokio::spawn(async move {
        let deadline = time::duration_until(deadline_unix_millis);
        let outcome = tokio::select! {
            outcome = execute_method(
                correlation_id,
                method,
                &session_authority.negotiated_capabilities,
                &session_authority.principal,
                session_authority.session_id,
                &session_authority.business_service,
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
    negotiated_capabilities: &std::collections::BTreeSet<String>,
    principal: &AuthenticatedPrincipal,
    session_id: Uuid,
    business_service: &Arc<dyn BusinessService>,
) -> CallOutcome {
    if let Err(error) = authorize_method(&method, negotiated_capabilities, principal) {
        return CallOutcome::Err { error };
    }
    match method {
        Method::Ping(params) => CallOutcome::Ok {
            result: MethodResult::Ping(PongResult {
                payload: params.payload,
                server_unix_millis: time::unix_millis_now(),
            }),
        },
        business => {
            let context = BusinessRequestContext {
                correlation_id,
                session_id,
                principal: principal.clone(),
            };
            match business_service.call(business.clone(), context).await {
                Ok(result)
                    if result.validate_contract()
                        && result.correlation_id() == correlation_id.to_string()
                        && business_result_matches_method(&business, &result) =>
                {
                    CallOutcome::Ok {
                        result: MethodResult::Business(result),
                    }
                }
                Ok(_) => CallOutcome::Err {
                    error: RpcError::Internal {
                        message: "business producer returned an invalid or mismatched result"
                            .to_string(),
                    },
                },
                Err(error) => CallOutcome::Err { error },
            }
        }
    }
}

fn spawn_stream_call(
    correlation_id: Uuid,
    deadline_unix_millis: u64,
    method: StreamMethod,
    session_authority: SessionAuthority,
    outgoing_tx: mpsc::Sender<WireMessage>,
    mut cancel_rx: oneshot::Receiver<()>,
    completed_tx: mpsc::UnboundedSender<Uuid>,
) {
    tokio::spawn(async move {
        let deadline = time::duration_until(deadline_unix_millis);
        let result = tokio::select! {
            result = execute_stream_method(
                correlation_id,
                method,
                &outgoing_tx,
                &session_authority.negotiated_capabilities,
                &session_authority.principal,
                session_authority.session_id,
                &session_authority.business_service,
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
    negotiated_capabilities: &std::collections::BTreeSet<String>,
    principal: &AuthenticatedPrincipal,
    session_id: Uuid,
    business_service: &Arc<dyn BusinessService>,
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
            authorize_stream(&params, negotiated_capabilities, principal)?;
            let (event_tx, mut event_rx) = mpsc::channel(16);
            let context = BusinessRequestContext {
                correlation_id,
                session_id,
                principal: principal.clone(),
            };
            let producer = business_service.stream(
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
        || event.entity_id != params.scope_id
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
        || !safe_id(&params.workspace_id, 200)
        || !safe_id(&params.scope_id, 200)
    {
        return Err(RpcError::InvalidRequest {
            message: "business stream parameters failed bounds validation".to_string(),
        });
    }
    if params.workspace_id != principal.workspace_id
        || !principal.scope_ids.contains(&params.scope_id)
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
    capabilities: &std::collections::BTreeSet<String>,
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
    let required = match method {
        Method::Ping(_) => return Ok(()),
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
        Method::ApprovalGrant(_) | Method::ApprovalDeny(_) => {
            if principal.kind != PrincipalKind::DesktopInstallation {
                return Err(RpcError::PermissionDenied);
            }
            Authority::DecideApproval
        }
    };
    principal
        .authorizes(required)
        .then_some(())
        .ok_or(RpcError::PermissionDenied)
}

pub(crate) fn validate_method(method: &Method) -> Result<(), RpcError> {
    let valid = match method {
        Method::Ping(_) | Method::RuntimeGetStatus(_) => true,
        Method::DevicesList(v) | Method::IncidentsList(v) => {
            valid_page(v.limit, v.cursor.as_deref())
        }
        Method::DeviceGet(v) => safe_id(&v.device_id, 200),
        Method::DeviceGetHealth(v) | Method::DeviceReadLogs(v) => {
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
                && safe_id(&v.idempotency_key, 200)
        }
        Method::WorkIntentAcknowledge(v) => {
            safe_id(&v.work_intent_id, 200)
                && safe_id(&v.run_id, 200)
                && safe_id(&v.idempotency_key, 200)
        }
        Method::HealthCheckRequest(v) | Method::PreflightRequest(v) | Method::RestartRequest(v) => {
            safe_id(&v.device_id, 200) && safe_id(&v.idempotency_key, 200)
        }
        Method::BuildRequest(v) => {
            safe_id(&v.source_snapshot_id, 200)
                && safe_id(&v.target_id, 200)
                && safe_id(&v.recipe_id, 200)
                && safe_id(&v.idempotency_key, 200)
        }
        Method::FlashRequest(v) => {
            safe_id(&v.device_id, 200)
                && safe_id(&v.artifact_id, 200)
                && safe_digest(&v.artifact_digest)
                && safe_id(&v.idempotency_key, 200)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct BusinessFixture {
        requests: Vec<Method>,
    }

    fn unsupported_service() -> Arc<dyn BusinessService> {
        Arc::new(UnsupportedBusinessService)
    }

    struct MismatchedService;

    impl BusinessService for MismatchedService {
        fn call(&self, _method: Method, context: BusinessRequestContext) -> BusinessCallFuture {
            Box::pin(async move {
                Ok(BusinessRpcResult::RuntimeStatus {
                    meta: crate::business::ReadMeta {
                        source_runtime_id: "runtime-1".to_string(),
                        observed_at: "2026-08-24T12:00:00Z".to_string(),
                        correlation_id: context.correlation_id.to_string(),
                    },
                    state: crate::business::RuntimeState::Connected,
                    runtime_id: "runtime-1".to_string(),
                })
            })
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

    fn expected_authority(method: &Method) -> Authority {
        match method {
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
            Method::Ping(_) => panic!("business fixture must not contain core ping"),
        }
    }

    #[test]
    fn every_business_method_requires_capability_and_its_exact_authority() {
        let fixture: BusinessFixture = serde_json::from_str(include_str!(
            "../../../packages/contracts/fixtures/business-rpc.json"
        ))
        .expect("business fixture");
        assert_eq!(
            fixture.requests.len(),
            23,
            "cover the complete method table"
        );

        for method in fixture.requests {
            let required = expected_authority(&method);
            let no_capabilities = BTreeSet::new();
            let denied = AuthenticatedPrincipal {
                kind: PrincipalKind::DesktopInstallation,
                principal_id: "desktop-denied".to_string(),
                workspace_id: "workspace-1".to_string(),
                scope_ids: BTreeSet::from(["scope-1".to_string()]),
                authority: BTreeSet::new(),
            };
            assert!(matches!(
                authorize_method(&method, &no_capabilities, &denied),
                Err(RpcError::MissingCapability { .. })
            ));
            assert!(matches!(
                authorize_method(&method, &crate::protocol::previous_capabilities(), &denied),
                Err(RpcError::MissingCapability { .. })
            ));
            assert!(matches!(
                authorize_method(&method, &baseline_capabilities(), &denied),
                Err(RpcError::PermissionDenied)
            ));

            let allowed = AuthenticatedPrincipal {
                kind: PrincipalKind::DesktopInstallation,
                principal_id: "desktop-allowed".to_string(),
                workspace_id: "workspace-1".to_string(),
                scope_ids: BTreeSet::from(["scope-1".to_string()]),
                authority: BTreeSet::from([required]),
            };
            assert!(authorize_method(&method, &baseline_capabilities(), &allowed).is_ok());

            if required == Authority::DecideApproval {
                let agent = AuthenticatedPrincipal {
                    kind: PrincipalKind::AgentConnection,
                    principal_id: "agent-denied".to_string(),
                    workspace_id: "workspace-1".to_string(),
                    scope_ids: BTreeSet::from(["scope-1".to_string()]),
                    authority: BTreeSet::from([Authority::DecideApproval]),
                };
                assert!(matches!(
                    authorize_method(&method, &baseline_capabilities(), &agent),
                    Err(RpcError::PermissionDenied)
                ));
            }
        }
    }

    #[tokio::test]
    async fn business_subscription_requires_capability_and_subscribe_authority() {
        let params = crate::business::BusinessStreamParams {
            stream: crate::business::BusinessStreamName::Jobs,
            workspace_id: "workspace-1".to_string(),
            scope_id: "job-1".to_string(),
            after_sequence: None,
            max_items: 10,
        };
        let (tx, _rx) = mpsc::channel(1);
        let denied = AuthenticatedPrincipal {
            kind: PrincipalKind::AgentConnection,
            principal_id: "agent".to_string(),
            workspace_id: "workspace-1".to_string(),
            scope_ids: BTreeSet::from(["job-1".to_string()]),
            authority: BTreeSet::new(),
        };
        assert!(matches!(
            execute_stream_method(
                Uuid::nil(),
                StreamMethod::BusinessEvents(params.clone()),
                &tx,
                &BTreeSet::new(),
                &denied,
                Uuid::nil(),
                &unsupported_service(),
            )
            .await,
            Err(RpcError::MissingCapability { .. })
        ));
        assert!(matches!(
            execute_stream_method(
                Uuid::nil(),
                StreamMethod::BusinessEvents(params.clone()),
                &tx,
                &baseline_capabilities(),
                &denied,
                Uuid::nil(),
                &unsupported_service(),
            )
            .await,
            Err(RpcError::PermissionDenied)
        ));
        let allowed = AuthenticatedPrincipal {
            kind: PrincipalKind::AgentConnection,
            principal_id: "agent".to_string(),
            workspace_id: "workspace-1".to_string(),
            scope_ids: BTreeSet::from(["job-1".to_string()]),
            authority: BTreeSet::from([Authority::Subscribe, Authority::ReadJobs]),
        };
        assert!(matches!(
            execute_stream_method(
                Uuid::nil(),
                StreamMethod::BusinessEvents(params),
                &tx,
                &baseline_capabilities(),
                &allowed,
                Uuid::nil(),
                &unsupported_service(),
            )
            .await,
            Err(RpcError::UnsupportedMethod { .. })
        ));
    }

    #[test]
    fn every_stream_requires_its_domain_authority_and_bound_workspace_scope() {
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
                workspace_id: "workspace-1".to_string(),
                scope_id: "scope-1".to_string(),
                after_sequence: None,
                max_items: 10,
            };
            let allowed = AuthenticatedPrincipal {
                kind: PrincipalKind::DesktopInstallation,
                principal_id: "desktop-1".to_string(),
                workspace_id: "workspace-1".to_string(),
                scope_ids: BTreeSet::from(["scope-1".to_string()]),
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
            let wrong_workspace = AuthenticatedPrincipal {
                workspace_id: "workspace-2".to_string(),
                ..allowed.clone()
            };
            assert!(matches!(
                authorize_stream(&params, &baseline_capabilities(), &wrong_workspace),
                Err(RpcError::PermissionDenied)
            ));
            let wrong_scope = AuthenticatedPrincipal {
                scope_ids: BTreeSet::from(["scope-2".to_string()]),
                ..allowed
            };
            assert!(matches!(
                authorize_stream(&params, &baseline_capabilities(), &wrong_scope),
                Err(RpcError::PermissionDenied)
            ));
        }
    }

    #[tokio::test]
    async fn outgoing_channel_with_capacity_one_reports_full_when_saturated() {
        // Deterministic (non-timing) proof of the backpressure primitive
        // itself: a bounded channel with capacity 1, no consumer reading,
        // reports `Full` on the second `try_send` rather than buffering
        // unboundedly. The end-to-end, real-pipe backpressure proof (a slow
        // client pacing a real server) lives in tests/streaming.rs.
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
    async fn execute_ping_echoes_payload() {
        let capabilities = baseline_capabilities();
        let principal = AuthenticatedPrincipal::runtime_service("test");
        let outcome = execute_method(
            Uuid::nil(),
            Method::Ping(crate::protocol::PingParams {
                payload: "hi".to_string(),
            }),
            &capabilities,
            &principal,
            Uuid::nil(),
            &unsupported_service(),
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
    async fn mismatched_business_producer_result_fails_closed() {
        let principal = AuthenticatedPrincipal {
            kind: PrincipalKind::DesktopInstallation,
            principal_id: "desktop-1".to_string(),
            workspace_id: "workspace-1".to_string(),
            scope_ids: BTreeSet::from(["device-1".to_string()]),
            authority: BTreeSet::from([Authority::ReadDevices]),
        };
        let outcome = execute_method(
            Uuid::nil(),
            Method::DevicesList(crate::business::PageParams {
                limit: 10,
                cursor: None,
            }),
            &baseline_capabilities(),
            &principal,
            Uuid::nil(),
            &(Arc::new(MismatchedService) as Arc<dyn BusinessService>),
        )
        .await;
        assert!(matches!(
            outcome,
            CallOutcome::Err {
                error: RpcError::Internal { .. }
            }
        ));
    }

    #[tokio::test]
    async fn echo_stream_over_the_configured_maximum_is_rejected_as_invalid_request() {
        let (tx, mut rx) = mpsc::channel(4);
        let capabilities = baseline_capabilities();
        let principal = AuthenticatedPrincipal::runtime_service("test");
        let result = execute_stream_method(
            Uuid::nil(),
            StreamMethod::EchoStream(crate::protocol::EchoStreamParams {
                payload: "x".to_string(),
                count: MAX_ECHO_STREAM_COUNT + 1,
            }),
            &tx,
            &capabilities,
            &principal,
            Uuid::nil(),
            &unsupported_service(),
        )
        .await;
        assert!(matches!(result, Err(RpcError::InvalidRequest { .. })));
        drop(tx);
        assert!(rx.recv().await.is_none(), "no items should have been sent");
    }

    #[tokio::test]
    async fn echo_stream_sends_every_item_in_order() {
        let (tx, mut rx) = mpsc::channel(16);
        let capabilities = baseline_capabilities();
        let principal = AuthenticatedPrincipal::runtime_service("test");
        let handle = tokio::spawn(async move {
            execute_stream_method(
                Uuid::nil(),
                StreamMethod::EchoStream(crate::protocol::EchoStreamParams {
                    payload: "x".to_string(),
                    count: 5,
                }),
                &tx,
                &capabilities,
                &principal,
                Uuid::nil(),
                &unsupported_service(),
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
}
