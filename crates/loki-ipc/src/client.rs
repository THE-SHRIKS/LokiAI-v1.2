//! Named-pipe RPC client: connect, handshake, and typed `ping`/`echo_stream`
//! calls with per-call deadlines and cancellation.
//!
//! Reads and writes are each owned by one dedicated background task (a
//! writer task draining an outgoing queue, and a reader task demultiplexing
//! incoming frames by `correlation_id` into whichever caller is waiting),
//! so multiple calls may be in flight concurrently on one connection. A
//! `std::sync::Mutex`-guarded map tracks pending calls; the lock is only
//! ever held across synchronous map operations, never across an `.await`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::auth::AuthToken;
use crate::business::{
    AuthenticatedPrincipal, BusinessEvent, BusinessRpcResult, BusinessStreamParams,
    BusinessStreamSummary,
};
use crate::frame::{DEFAULT_MAX_FRAME_BYTES, FrameError, read_frame, write_frame};
use crate::protocol::{
    CallOutcome, EchoStreamItem, EchoStreamParams, EchoStreamSummary, HandshakeRejectReason,
    Method, MethodResult, PROTOCOL_VERSION, PingParams, PongResult, ProtocolVersion, RpcError,
    StreamItemPayload, StreamMethod, StreamOutcome, StreamSummary, WireMessage,
    baseline_capabilities, business_result_matches_request,
};
use crate::time;

/// `ERROR_PIPE_BUSY` (winerror.h): every server instance is currently
/// connected to a client. Transient by construction (the server always
/// prepares a fresh instance immediately after accepting — see
/// `crate::server::Server::accept`), so this is the one connect error this
/// client retries.
const ERROR_PIPE_BUSY: i32 = 231;

/// Extra time [`Client::call`]-family methods wait past the requested
/// deadline before giving up locally. The server is the primary
/// cancellation/deadline authority (it enforces `deadline_unix_millis`
/// itself), so under normal operation a response — including an explicit
/// `DeadlineExceeded` — arrives well within the requested timeout; this
/// grace only guards against the connection stalling so completely that not
/// even the server's own deadline response gets through.
const CLIENT_DEADLINE_GRACE: Duration = Duration::from_secs(2);

/// Client-side configuration.
#[derive(Clone)]
pub struct ClientConfig {
    pub pipe_name: String,
    pub token: AuthToken,
    pub client_type: String,
    pub client_version: String,
    pub protocol_version: ProtocolVersion,
    pub capabilities: std::collections::BTreeSet<String>,
    pub max_frame_bytes: u32,
    /// How long [`Client::connect`] retries `ERROR_PIPE_BUSY` before giving
    /// up.
    pub connect_timeout: Duration,
    pub outgoing_channel_capacity: usize,
    /// Bounded capacity of each [`StreamHandle`]'s event channel — the
    /// client-side half of the backpressure path described in
    /// `crate::server`'s docs (a slow caller not draining a `StreamHandle`
    /// eventually stalls the reader task, which stalls the OS pipe, which
    /// stalls the server).
    pub stream_event_capacity: usize,
}

impl ClientConfig {
    pub fn new(
        pipe_name: impl Into<String>,
        token: AuthToken,
        client_type: impl Into<String>,
    ) -> Self {
        Self {
            pipe_name: pipe_name.into(),
            token,
            client_type: client_type.into(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION,
            capabilities: baseline_capabilities(),
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            connect_timeout: Duration::from_secs(5),
            outgoing_channel_capacity: 16,
            stream_event_capacity: 16,
        }
    }
}

#[derive(Debug)]
pub enum ConnectError {
    Io(std::io::Error),
    Frame(FrameError),
    ConnectionClosedDuringHandshake,
    UnexpectedMessage,
    Rejected {
        reason: HandshakeRejectReason,
        server_protocol_version: ProtocolVersion,
    },
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::Io(err) => write!(f, "failed to open ipc connection: {err}"),
            ConnectError::Frame(err) => write!(f, "handshake frame error: {err}"),
            ConnectError::ConnectionClosedDuringHandshake => {
                write!(f, "server closed the connection during handshake")
            }
            ConnectError::UnexpectedMessage => {
                write!(f, "server sent an unexpected message during handshake")
            }
            ConnectError::Rejected {
                reason,
                server_protocol_version,
            } => write!(
                f,
                "handshake rejected by server (protocol {server_protocol_version}): {reason}"
            ),
        }
    }
}

impl std::error::Error for ConnectError {}

#[derive(Debug)]
pub enum CallError {
    /// The connection was lost before a response arrived.
    ConnectionClosed,
    /// Neither a response nor the server's own `DeadlineExceeded` arrived
    /// even after [`CLIENT_DEADLINE_GRACE`] past the requested deadline.
    ClientTimedOut,
    /// The server returned a named RPC failure (including a normal,
    /// server-acknowledged `DeadlineExceeded`/`Cancelled`).
    Rpc(RpcError),
    ProtocolMismatch,
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::ConnectionClosed => write!(f, "connection closed before a response arrived"),
            CallError::ClientTimedOut => write!(
                f,
                "no response from server within the deadline grace period"
            ),
            CallError::Rpc(err) => write!(f, "{err}"),
            CallError::ProtocolMismatch => write!(f, "server returned a mismatched typed result"),
        }
    }
}

impl std::error::Error for CallError {}

enum PendingCall {
    Unary(oneshot::Sender<CallOutcome>),
    EchoStream(mpsc::Sender<StreamEvent>),
    BusinessStream {
        sender: mpsc::Sender<BusinessStreamEvent>,
        expected: BusinessStreamParams,
        items_seen: u16,
        last_sequence: Option<u64>,
    },
}

type PendingCalls = Arc<StdMutex<HashMap<Uuid, PendingCall>>>;

/// One event from a [`StreamHandle`].
#[derive(Debug)]
pub enum StreamEvent {
    Item(EchoStreamItem),
    Ended(Result<EchoStreamSummary, RpcError>),
}

#[derive(Debug)]
pub enum BusinessStreamEvent {
    Item(BusinessEvent),
    Ended(Result<BusinessStreamSummary, RpcError>),
}

/// A handle to an in-flight streaming call's incoming events.
pub struct StreamHandle {
    correlation_id: Uuid,
    rx: mpsc::Receiver<StreamEvent>,
}

pub struct BusinessStreamHandle {
    correlation_id: Uuid,
    rx: mpsc::Receiver<BusinessStreamEvent>,
}

impl BusinessStreamHandle {
    pub fn correlation_id(&self) -> Uuid {
        self.correlation_id
    }

    pub async fn recv(&mut self) -> Option<BusinessStreamEvent> {
        self.rx.recv().await
    }
}

impl StreamHandle {
    pub fn correlation_id(&self) -> Uuid {
        self.correlation_id
    }

    /// Awaits the next event. `None` means the connection was lost before a
    /// terminal [`StreamEvent::Ended`] was delivered — the reader task
    /// always attempts to deliver `Ended` first (see [`Client::connect`]'s
    /// reader loop), so `None` specifically signals a dropped connection,
    /// never a normal end of stream.
    pub async fn recv(&mut self) -> Option<StreamEvent> {
        self.rx.recv().await
    }
}

/// A connected, handshaken IPC client.
pub struct Client {
    outgoing_tx: mpsc::Sender<WireMessage>,
    pending: PendingCalls,
    writer_task: JoinHandle<()>,
    reader_task: JoinHandle<()>,
    negotiated_capabilities: std::collections::BTreeSet<String>,
    session_id: Uuid,
    authenticated_principal: Option<AuthenticatedPrincipal>,
    stream_event_capacity: usize,
}

// Manual, not derived: several fields (`PendingCalls`'s internal state,
// `JoinHandle`) either don't implement `Debug` or would leak more than a test
// failure message needs. This exists so `{:?}`-formatting a `Client` (e.g. in
// a test's `.expect(...)`/`panic!(...)` on an unexpected `Ok(Client)`) works
// at all — session/capability identity is enough for that purpose.
impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("session_id", &self.session_id)
            .field("negotiated_capabilities", &self.negotiated_capabilities)
            .field("authenticated_principal", &self.authenticated_principal)
            .finish_non_exhaustive()
    }
}

impl Drop for Client {
    /// Ends the connection promptly when a `Client` is dropped. Each
    /// background task owns half of a `tokio::io::split` pair, which itself
    /// holds the underlying pipe behind an `Arc` — the OS handle is only
    /// closed once *both* halves are gone, so both tasks must be stopped
    /// here, not just one. Aborting (rather than relying on the tasks
    /// eventually erroring out on their own) means a dropped `Client` does
    /// not leave orphaned background tasks or a half-open pipe lingering.
    fn drop(&mut self) {
        self.writer_task.abort();
        self.reader_task.abort();
    }
}

impl Client {
    pub fn negotiated_capabilities(&self) -> &std::collections::BTreeSet<String> {
        &self.negotiated_capabilities
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn authenticated_principal(&self) -> Option<&AuthenticatedPrincipal> {
        self.authenticated_principal.as_ref()
    }

    /// Opens the pipe (retrying `ERROR_PIPE_BUSY` up to
    /// [`ClientConfig::connect_timeout`]) and performs the handshake.
    pub async fn connect(config: ClientConfig) -> Result<Self, ConnectError> {
        let pipe = open_with_retry(&config).await.map_err(ConnectError::Io)?;
        let (mut reader, mut writer) = tokio::io::split(pipe);

        let (server_protocol_version, nonce) =
            match read_frame(&mut reader, config.max_frame_bytes).await {
                Ok(Some(WireMessage::Challenge {
                    server_protocol_version,
                    nonce,
                    ..
                })) => (server_protocol_version, nonce),
                Ok(Some(_other)) => return Err(ConnectError::UnexpectedMessage),
                Ok(None) => return Err(ConnectError::ConnectionClosedDuringHandshake),
                Err(err) => return Err(ConnectError::Frame(err)),
            };

        let hello = WireMessage::Hello {
            client_protocol_version: config.protocol_version,
            client_capabilities: config.capabilities.clone(),
            client_type: config.client_type.clone(),
            client_version: config.client_version.clone(),
            nonce,
            token: config.token.as_str().to_string(),
        };
        write_frame(&mut writer, &hello)
            .await
            .map_err(ConnectError::Frame)?;

        let (negotiated_capabilities, session_id, authenticated_principal) =
            match read_frame(&mut reader, config.max_frame_bytes).await {
                Ok(Some(WireMessage::HelloAck {
                    negotiated_capabilities,
                    session_id,
                    principal,
                })) => (negotiated_capabilities, session_id, principal),
                Ok(Some(WireMessage::HelloReject {
                    reason,
                    server_protocol_version,
                })) => {
                    return Err(ConnectError::Rejected {
                        reason,
                        server_protocol_version,
                    });
                }
                Ok(Some(_other)) => return Err(ConnectError::UnexpectedMessage),
                Ok(None) => return Err(ConnectError::ConnectionClosedDuringHandshake),
                Err(err) => return Err(ConnectError::Frame(err)),
            };
        let legal_capabilities = crate::protocol::capabilities_for_version(config.protocol_version);
        if !negotiated_capabilities.is_subset(&config.capabilities)
            || !negotiated_capabilities.is_subset(&legal_capabilities)
            || authenticated_principal.is_none()
        {
            return Err(ConnectError::UnexpectedMessage);
        }
        let _ = server_protocol_version;

        let pending: PendingCalls = Arc::new(StdMutex::new(HashMap::new()));
        let (outgoing_tx, mut outgoing_rx) =
            mpsc::channel::<WireMessage>(config.outgoing_channel_capacity);

        let writer_task = tokio::spawn(async move {
            while let Some(message) = outgoing_rx.recv().await {
                if write_frame(&mut writer, &message).await.is_err() {
                    break;
                }
            }
        });

        let reader_pending = Arc::clone(&pending);
        let max_frame_bytes = config.max_frame_bytes;
        let reader_task = tokio::spawn(async move {
            run_reader_loop(reader, max_frame_bytes, reader_pending).await;
        });

        Ok(Self {
            outgoing_tx,
            pending,
            writer_task,
            reader_task,
            negotiated_capabilities,
            session_id,
            authenticated_principal,
            stream_event_capacity: config.stream_event_capacity,
        })
    }

    /// Calls the `ping` method. `timeout` becomes the call's absolute
    /// deadline, enforced primarily by the server (see [`crate::server`]).
    pub async fn ping(
        &self,
        payload: impl Into<String>,
        timeout: Duration,
    ) -> Result<PongResult, CallError> {
        let correlation_id = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap()
            .insert(correlation_id, PendingCall::Unary(tx));

        let call = WireMessage::Call {
            correlation_id,
            deadline_unix_millis: time::deadline_from_timeout(timeout),
            method: Method::Ping(PingParams {
                payload: payload.into(),
            }),
        };
        if self.outgoing_tx.send(call).await.is_err() {
            self.pending.lock().unwrap().remove(&correlation_id);
            return Err(CallError::ConnectionClosed);
        }

        match tokio::time::timeout(timeout + CLIENT_DEADLINE_GRACE, rx).await {
            Ok(Ok(CallOutcome::Ok {
                result: MethodResult::Ping(pong),
            })) => Ok(pong),
            Ok(Ok(CallOutcome::Ok { .. })) => Err(CallError::ProtocolMismatch),
            Ok(Ok(CallOutcome::Err { error })) => Err(CallError::Rpc(error)),
            Ok(Err(_recv_error)) => Err(CallError::ConnectionClosed),
            Err(_elapsed) => {
                self.pending.lock().unwrap().remove(&correlation_id);
                Err(CallError::ClientTimedOut)
            }
        }
    }

    /// Calls the `echo_stream` method, returning a [`StreamHandle`] to
    /// consume its items. `timeout` becomes the call's absolute deadline
    /// (covering the whole stream, enforced by the server).
    pub async fn echo_stream(
        &self,
        payload: impl Into<String>,
        count: u32,
        timeout: Duration,
    ) -> Result<StreamHandle, CallError> {
        let correlation_id = Uuid::new_v4();
        let (tx, rx) = mpsc::channel(self.stream_event_capacity);
        self.pending
            .lock()
            .unwrap()
            .insert(correlation_id, PendingCall::EchoStream(tx));

        let call = WireMessage::CallStream {
            correlation_id,
            deadline_unix_millis: time::deadline_from_timeout(timeout),
            method: StreamMethod::EchoStream(EchoStreamParams {
                payload: payload.into(),
                count,
            }),
        };
        if self.outgoing_tx.send(call).await.is_err() {
            self.pending.lock().unwrap().remove(&correlation_id);
            return Err(CallError::ConnectionClosed);
        }

        Ok(StreamHandle { correlation_id, rx })
    }

    pub async fn business_call(
        &self,
        method: Method,
        timeout: Duration,
    ) -> Result<BusinessRpcResult, CallError> {
        if matches!(method, Method::Ping(_)) {
            return Err(CallError::ProtocolMismatch);
        }
        let principal = self
            .authenticated_principal
            .as_ref()
            .ok_or(CallError::ProtocolMismatch)?;
        let correlation_id = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap()
            .insert(correlation_id, PendingCall::Unary(tx));
        let call = WireMessage::Call {
            correlation_id,
            deadline_unix_millis: time::deadline_from_timeout(timeout),
            method: method.clone(),
        };
        if self.outgoing_tx.send(call).await.is_err() {
            self.pending.lock().unwrap().remove(&correlation_id);
            return Err(CallError::ConnectionClosed);
        }
        match tokio::time::timeout(timeout + CLIENT_DEADLINE_GRACE, rx).await {
            Ok(Ok(CallOutcome::Ok {
                result: MethodResult::Business(result),
            })) if result.validate_contract()
                && result.correlation_id() == correlation_id.to_string()
                && business_result_matches_request(&method, &result, &principal.workspace_id) =>
            {
                Ok(result)
            }
            Ok(Ok(CallOutcome::Ok { .. })) => Err(CallError::ProtocolMismatch),
            Ok(Ok(CallOutcome::Err { error })) => Err(CallError::Rpc(error)),
            Ok(Err(_)) => Err(CallError::ConnectionClosed),
            Err(_) => {
                self.pending.lock().unwrap().remove(&correlation_id);
                Err(CallError::ClientTimedOut)
            }
        }
    }

    pub async fn business_stream(
        &self,
        params: BusinessStreamParams,
        timeout: Duration,
    ) -> Result<BusinessStreamHandle, CallError> {
        let correlation_id = Uuid::new_v4();
        let (tx, rx) = mpsc::channel(self.stream_event_capacity);
        self.pending.lock().unwrap().insert(
            correlation_id,
            PendingCall::BusinessStream {
                sender: tx,
                expected: params.clone(),
                items_seen: 0,
                last_sequence: params.after_sequence,
            },
        );
        let call = WireMessage::CallStream {
            correlation_id,
            deadline_unix_millis: time::deadline_from_timeout(timeout),
            method: StreamMethod::BusinessEvents(params),
        };
        if self.outgoing_tx.send(call).await.is_err() {
            self.pending.lock().unwrap().remove(&correlation_id);
            return Err(CallError::ConnectionClosed);
        }
        Ok(BusinessStreamHandle { correlation_id, rx })
    }

    /// Asks the server to stop an in-flight call early. Best-effort and
    /// idempotent: if the call already finished, this is a silent no-op —
    /// see `Docs/graph/tasks/RUN-002.md` Deliverable 4.
    pub async fn cancel_call(&self, correlation_id: Uuid) {
        let _ = self
            .outgoing_tx
            .send(WireMessage::CancelCall { correlation_id })
            .await;
    }
}

async fn open_with_retry(config: &ClientConfig) -> std::io::Result<NamedPipeClient> {
    let options = ClientOptions::new();
    let deadline = tokio::time::Instant::now() + config.connect_timeout;
    loop {
        match options.open(&config.pipe_name) {
            Ok(client) => return Ok(client),
            Err(err)
                if err.raw_os_error() == Some(ERROR_PIPE_BUSY)
                    && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(err) => return Err(err),
        }
    }
}

async fn run_reader_loop<R>(mut reader: R, max_frame_bytes: u32, pending: PendingCalls)
where
    R: tokio::io::AsyncRead + Unpin,
{
    loop {
        match read_frame(&mut reader, max_frame_bytes).await {
            Ok(Some(WireMessage::CallResult {
                correlation_id,
                outcome,
            })) => {
                let waiter = pending.lock().unwrap().remove(&correlation_id);
                if let Some(PendingCall::Unary(tx)) = waiter {
                    let _ = tx.send(outcome);
                }
            }
            Ok(Some(WireMessage::StreamItem {
                correlation_id,
                item,
                ..
            })) => match item {
                StreamItemPayload::EchoStream(echo_item) => {
                    let sender = {
                        let guard = pending.lock().unwrap();
                        match guard.get(&correlation_id) {
                            Some(PendingCall::EchoStream(tx)) => Some(tx.clone()),
                            _ => None,
                        }
                    };
                    if let Some(tx) = sender {
                        let _ = tx.send(StreamEvent::Item(echo_item)).await;
                    }
                }
                StreamItemPayload::BusinessEvent(event) => {
                    let delivery = {
                        let mut guard = pending.lock().unwrap();
                        match guard.get_mut(&correlation_id) {
                            Some(PendingCall::BusinessStream {
                                sender,
                                expected,
                                items_seen,
                                last_sequence,
                            }) => {
                                let valid = event.validate_contract()
                                    && event.stream == expected.stream
                                    && event.entity_id == expected.scope_id
                                    && event.correlation_id == correlation_id.to_string()
                                    && *items_seen < expected.max_items
                                    && last_sequence
                                        .as_ref()
                                        .is_none_or(|previous| event.sequence > *previous);
                                if valid {
                                    *items_seen += 1;
                                    *last_sequence = Some(event.sequence);
                                }
                                Some((sender.clone(), valid))
                            }
                            _ => None,
                        }
                    };
                    if let Some((sender, true)) = delivery {
                        let _ = sender.send(BusinessStreamEvent::Item(event)).await;
                    } else if let Some((sender, false)) = delivery {
                        pending.lock().unwrap().remove(&correlation_id);
                        let _ = sender
                            .send(BusinessStreamEvent::Ended(Err(RpcError::Internal {
                                message: "server emitted a mismatched business event".to_string(),
                            })))
                            .await;
                    }
                }
            },
            Ok(Some(WireMessage::StreamEnd {
                correlation_id,
                outcome,
            })) => {
                let waiter = pending.lock().unwrap().remove(&correlation_id);
                match waiter {
                    Some(PendingCall::EchoStream(tx)) => {
                        let result = match outcome {
                            StreamOutcome::Ok {
                                summary: StreamSummary::EchoStream(summary),
                            } => Ok(summary),
                            StreamOutcome::Ok { .. } => Err(RpcError::Internal {
                                message: "server returned a mismatched stream summary".to_string(),
                            }),
                            StreamOutcome::Err { error } => Err(error),
                        };
                        let _ = tx.send(StreamEvent::Ended(result)).await;
                    }
                    Some(PendingCall::BusinessStream {
                        sender,
                        expected,
                        items_seen,
                        last_sequence,
                    }) => {
                        let result = match outcome {
                            StreamOutcome::Ok {
                                summary: StreamSummary::BusinessEvents(summary),
                            } if summary.validate_contract(expected.max_items)
                                && summary.items_sent == items_seen
                                && summary.last_sequence == last_sequence =>
                            {
                                Ok(summary)
                            }
                            StreamOutcome::Ok { .. } => Err(RpcError::Internal {
                                message: "server returned a mismatched stream summary".to_string(),
                            }),
                            StreamOutcome::Err { error } => Err(error),
                        };
                        let _ = sender.send(BusinessStreamEvent::Ended(result)).await;
                    }
                    _ => {}
                }
            }
            Ok(Some(_other)) => {
                tracing::warn!("client received an unexpected message type post-handshake");
            }
            Ok(None) => break,
            Err(err) => {
                tracing::debug!(error = %err, "ipc client connection ended");
                break;
            }
        }
    }

    // The connection is gone. Fail every still-pending unary call rather
    // than leaving a caller `.await`-ing forever; a pending stream's sender
    // is simply dropped, which `StreamHandle::recv` surfaces as `None`.
    let mut pending = pending.lock().unwrap();
    for (_, call) in pending.drain() {
        if let PendingCall::Unary(tx) = call {
            let _ = tx.send(CallOutcome::Err {
                error: RpcError::Internal {
                    message: "connection closed".to_string(),
                },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_config_new_declares_the_baseline_capability() {
        let config = ClientConfig::new("pipe", AuthToken::new("t"), "test");
        assert!(
            config
                .capabilities
                .contains(crate::protocol::CORE_CAPABILITY)
        );
    }

    #[test]
    fn call_error_display_is_operator_readable() {
        assert!(CallError::ConnectionClosed.to_string().contains("closed"));
        assert!(CallError::ClientTimedOut.to_string().contains("deadline"));
        assert!(
            CallError::Rpc(RpcError::DeadlineExceeded)
                .to_string()
                .contains("deadline exceeded")
        );
    }

    #[test]
    fn connect_error_display_names_the_rejection_reason() {
        let err = ConnectError::Rejected {
            reason: HandshakeRejectReason::AuthenticationFailed,
            server_protocol_version: ProtocolVersion::new(1, 0),
        };
        assert!(err.to_string().contains("authentication failed"));
    }
}
