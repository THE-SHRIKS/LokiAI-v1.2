//! Wire message types for the loki-ipc protocol.
//!
//! Every message is a variant of [`WireMessage`], framed by [`crate::frame`]
//! as `[u32 LE length][JSON payload]`. The handshake sequence is:
//!
//! ```text
//! server --Challenge-->  client
//! client --Hello------>  server
//! server --HelloAck/HelloReject--> client
//! ```
//!
//! After a successful handshake, the client may send [`WireMessage::Call`]
//! (expects one [`WireMessage::CallResult`]) or [`WireMessage::CallStream`]
//! (expects zero or more [`WireMessage::StreamItem`] followed by exactly one
//! [`WireMessage::StreamEnd`]), and [`WireMessage::CancelCall`] to ask the
//! server to stop an in-flight call early.
//!
//! [`Method`]/[`StreamMethod`] are the entire RPC surface this crate ships:
//! `RUN-002`'s `Ping`/`EchoStream` proof-of-life pair plus
//! `P1-RPC-CONTRACT-001-R1`'s closed 23-method/10-stream business vocabulary
//! (`crate::business` — see that module's top doc comment for exactly which
//! of the 23 unary methods have a real backing result shape). Never a
//! generic `Invoke(name: String, params: serde_json::Value)` escape hatch
//! (`AGENTS.md` rule 13).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::business::{
    AgentRunRegisterParams, AgentRunUpdateParams, ApprovalDenyParams, ApprovalGrantParams,
    AuthenticatedPrincipal, BUSINESS_CAPABILITY, BuildRequestParams, BusinessEvent,
    BusinessRpcResult, BusinessStreamParams, BusinessStreamSummary, DeploymentRequestParams,
    DeviceListParams, DevicePageParams, DeviceParams, EmptyParams, FlashRequestParams,
    IdempotentDeviceParams, IncidentParams, JobCancelParams, JobParams, OperationMethod,
    PageParams, RestartRequestParams, RollbackRequestParams, TelemetryParams, TestOperationParams,
    WorkIntentAckParams,
};

/// Semantic protocol version. The server rejects a client whose `major`
/// differs (`Docs/ARCHITECTURE.md` §13: "Incompatible clients receive a
/// precise upgrade-required state, not generic disconnection"); the
/// business capability (`v1.1`) requires `minor >= 1` — see
/// [`capabilities_for_version`] — so a same-major-but-old-minor client
/// cannot self-advertise it (closes REVIEW-001 HIGH-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Whether a peer declaring `other` may interoperate with this version.
    /// Same-major is the compatibility rule for this node; capability
    /// negotiation (below) covers finer-grained feature gating.
    pub fn is_compatible_with(&self, other: &ProtocolVersion) -> bool {
        self.major == other.major
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// The protocol version this build of loki-ipc speaks.
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::new(1, 1);
/// The previous minor edge (`RUN-002`'s original ship version): core
/// transport only, no business capability.
pub const PREVIOUS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::new(1, 0);

/// A capability every client must declare for the handshake to succeed.
pub const CORE_CAPABILITY: &str = "loki-ipc.core.v1";

/// The capability set this build of loki-ipc declares by default.
pub fn baseline_capabilities() -> BTreeSet<String> {
    BTreeSet::from([CORE_CAPABILITY.to_string(), BUSINESS_CAPABILITY.to_string()])
}

/// The bounded previous edge supports transport health only, never business
/// authority.
pub fn previous_capabilities() -> BTreeSet<String> {
    BTreeSet::from([CORE_CAPABILITY.to_string()])
}

/// Capabilities legal on the negotiated same-major minor edge. A peer
/// declaring `client_protocol_version` with `minor < 1` (e.g. a real,
/// unmodified `v1.0` client, or a hostile client that lies about its
/// capability set while claiming `v1.0`) can never be granted
/// [`BUSINESS_CAPABILITY`] — this function, not the client's own declared
/// capability set, is the sole source of truth for what a given declared
/// version is legally allowed to negotiate. `crate::auth::evaluate_hello*`
/// intersects the client's declared capabilities with exactly this set, so
/// a client cannot self-upgrade by simply adding the string to its `Hello`.
pub fn capabilities_for_version(version: ProtocolVersion) -> BTreeSet<String> {
    if version.major == PROTOCOL_VERSION.major && version.minor >= PROTOCOL_VERSION.minor {
        baseline_capabilities()
    } else {
        previous_capabilities()
    }
}

/// Opaque per-call/per-connection correlation identifier
/// (AGENTS.md rule 5: "Use stable IDs ... and correlation IDs for
/// operations that may be retried").
pub type CorrelationId = Uuid;

// ---------------------------------------------------------------------
// RPC method surface.
// ---------------------------------------------------------------------

/// Unary (single-response) RPC methods. See `crate::business`'s top doc
/// comment for exactly which 10 of these 23 business variants (plus `Ping`)
/// produce a real result shape; the other 13 are authorized normally but
/// always fail closed with `RpcError::UnsupportedMethod` in this node (no
/// real backing contract exists for them yet anywhere in this workspace).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Method {
    /// Proves the round trip works: server echoes `payload` back plus its
    /// own clock reading.
    #[serde(rename = "Ping")]
    Ping(PingParams),
    RuntimeGetStatus(EmptyParams),
    DevicesList(DeviceListParams),
    DeviceGet(DeviceParams),
    DeviceGetHealth(DeviceParams),
    DeviceReadLogs(DevicePageParams),
    DeviceQueryTelemetry(TelemetryParams),
    IncidentsList(PageParams),
    IncidentGet(IncidentParams),
    JobGet(JobParams),
    AgentRunRegister(AgentRunRegisterParams),
    AgentRunUpdate(AgentRunUpdateParams),
    WorkIntentAcknowledge(WorkIntentAckParams),
    HealthCheckRequest(IdempotentDeviceParams),
    PreflightRequest(IdempotentDeviceParams),
    JobCancel(JobCancelParams),
    BuildRequest(BuildRequestParams),
    RestartRequest(RestartRequestParams),
    FlashRequest(FlashRequestParams),
    TestOnDeviceRequest(TestOperationParams),
    DeploymentRequest(DeploymentRequestParams),
    RollbackRequest(RollbackRequestParams),
    ApprovalGrant(ApprovalGrantParams),
    ApprovalDeny(ApprovalDenyParams),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingParams {
    pub payload: String,
}

/// Streaming (server-to-client, multi-response) RPC methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum StreamMethod {
    /// Proves server-to-client streaming works: the server sends `count`
    /// numbered items, each carrying `payload`, then a summary.
    #[serde(rename = "EchoStream")]
    EchoStream(EchoStreamParams),
    BusinessEvents(BusinessStreamParams),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoStreamParams {
    pub payload: String,
    pub count: u32,
}

/// The largest `count` `EchoStream` accepts. This is transport-layer input
/// validation (AGENTS.md rule 4: "Validate all data at every trust
/// boundary"), not a product limit — a hostile or buggy client cannot make
/// the server enqueue unbounded work for one call.
pub const MAX_ECHO_STREAM_COUNT: u32 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PongResult {
    pub payload: String,
    pub server_unix_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EchoStreamItem {
    pub seq: u32,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EchoStreamSummary {
    pub items_sent: u32,
}

/// The successful result of a unary [`Method`] call. `Business` results
/// reject unknown top-level fields on decode via
/// `BusinessRpcResult`'s own `#[serde(deny_unknown_fields)]` on every
/// variant (closes REVIEW-002 finding 3 — exact Rust/TS output parity).
///
/// `#[allow(clippy::large_enum_variant)]`: `Business(BusinessRpcResult)` is
/// legitimately larger than `Ping(PongResult)` (a closed multi-variant
/// read/mutation result vocabulary vs. two scalars). This type is decoded
/// once per unary call, not hot-looped, and `WireMessage`/`CallOutcome`
/// (which wrap it) are themselves already heap-boxed one level up wherever
/// they're moved through a channel (`tokio::sync::mpsc::Sender<WireMessage>`
/// stores by value into a heap-allocated channel slot) — boxing this inner
/// variant too would only relocate the allocation, not remove one, while
/// forcing every `MethodResult::Business(result)` match site in
/// `server.rs`/`client.rs` through an extra deref.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "result")]
#[allow(clippy::large_enum_variant)]
pub enum MethodResult {
    Ping(PongResult),
    Business(BusinessRpcResult),
}

/// Whether `result`'s shape corresponds to `method`. Only the 10 backed
/// business methods (`crate::business`'s top doc comment) ever appear on
/// the right-hand side here in a real server/client — the other 13 always
/// fail before producing any `BusinessRpcResult`.
pub fn business_result_matches_method(method: &Method, result: &BusinessRpcResult) -> bool {
    match (method, result) {
        (Method::RuntimeGetStatus(_), BusinessRpcResult::RuntimeStatus { .. })
        | (Method::DevicesList(_), BusinessRpcResult::DevicesList { .. })
        | (Method::DeviceGet(_), BusinessRpcResult::DeviceGet { .. })
        | (Method::DeviceGetHealth(_), BusinessRpcResult::DeviceHealth { .. }) => true,
        (
            method,
            BusinessRpcResult::Mutation {
                method: result_method,
                ..
            },
        ) => matches!(
            (method, result_method),
            (Method::RestartRequest(_), OperationMethod::Restart)
                | (Method::FlashRequest(_), OperationMethod::Flash)
                | (Method::BuildRequest(_), OperationMethod::ContainedBuild)
                | (
                    Method::AgentRunRegister(_),
                    OperationMethod::AgentRunRegister
                )
                | (Method::AgentRunUpdate(_), OperationMethod::AgentRunUpdate)
                | (
                    Method::WorkIntentAcknowledge(_),
                    OperationMethod::WorkIntentAcknowledge
                )
        ),
        _ => false,
    }
}

/// One streamed item for a [`StreamMethod`] call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "item")]
pub enum StreamItemPayload {
    EchoStream(EchoStreamItem),
    BusinessEvent(BusinessEvent),
}

/// The successful terminal summary of a [`StreamMethod`] call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "summary")]
pub enum StreamSummary {
    EchoStream(EchoStreamSummary),
    BusinessEvents(BusinessStreamSummary),
}

/// A named, typed RPC failure. Every rejection reason is explicit — never a
/// bare string alone standing in for a machine-distinguishable case
/// (AGENTS.md rule 6: "Make every state transition explicit").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RpcError {
    /// The call's deadline elapsed before it completed. The server-side
    /// work was cancelled, not merely abandoned by the client — see
    /// `tests/deadline.rs`.
    DeadlineExceeded,
    /// The client explicitly cancelled the call via [`WireMessage::CancelCall`].
    Cancelled,
    /// The request parameters failed validation (e.g. `EchoStream.count`
    /// exceeding [`MAX_ECHO_STREAM_COUNT`]).
    InvalidRequest {
        message: String,
    },
    MissingCapability {
        required: String,
    },
    PermissionDenied,
    /// The session's credential was found revoked/expired at
    /// revalidation time (not only at `Hello`) — distinct from
    /// `PermissionDenied` so a client can tell "your session ended" from
    /// "you never had this authority".
    CredentialRevalidationFailed,
    UnsupportedMethod {
        method: String,
    },
    /// An unexpected server-side failure (e.g. the handler task panicked).
    /// Never includes raw internal detail beyond a short, redacted summary.
    Internal {
        message: String,
    },
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpcError::DeadlineExceeded => write!(f, "deadline exceeded"),
            RpcError::Cancelled => write!(f, "cancelled"),
            RpcError::InvalidRequest { message } => write!(f, "invalid request: {message}"),
            RpcError::MissingCapability { required } => write!(f, "missing capability: {required}"),
            RpcError::PermissionDenied => write!(f, "permission denied"),
            RpcError::CredentialRevalidationFailed => {
                write!(
                    f,
                    "credential revalidation failed: session no longer active"
                )
            }
            RpcError::UnsupportedMethod { method } => write!(f, "unsupported method: {method}"),
            RpcError::Internal { message } => write!(f, "internal error: {message}"),
        }
    }
}

impl std::error::Error for RpcError {}

/// `#[allow(clippy::large_enum_variant)]`: see [`MethodResult`]'s own doc
/// comment — the same reasoning applies one level up.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum CallOutcome {
    Ok { result: MethodResult },
    Err { error: RpcError },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum StreamOutcome {
    Ok { summary: StreamSummary },
    Err { error: RpcError },
}

/// Why the server rejected a [`WireMessage::Hello`]. Every reason is a named
/// variant with the context needed for a precise client-facing message —
/// never a generic "handshake failed" (`Docs/ARCHITECTURE.md` §13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum HandshakeRejectReason {
    VersionMismatch {
        server_version: ProtocolVersion,
        client_version: ProtocolVersion,
    },
    MissingCapability {
        required: String,
    },
    /// The declared token did not match the server's shared secret, or the
    /// echoed nonce did not match the nonce issued for *this* connection
    /// (see [`StaleNonce`](HandshakeRejectReason::StaleNonce) for the
    /// latter's dedicated, more specific variant — kept distinct so a
    /// replay attempt is diagnosable separately from a plain wrong secret).
    AuthenticationFailed,
    /// The `Hello`'s echoed nonce does not match the nonce this connection's
    /// `Challenge` carried — the classic signature of a captured `Hello`
    /// from an earlier connection being replayed against a new one.
    StaleNonce,
    MalformedHello {
        message: String,
    },
}

impl std::fmt::Display for HandshakeRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandshakeRejectReason::VersionMismatch {
                server_version,
                client_version,
            } => write!(
                f,
                "protocol version mismatch: server={server_version} client={client_version}"
            ),
            HandshakeRejectReason::MissingCapability { required } => {
                write!(f, "missing required capability: {required}")
            }
            HandshakeRejectReason::AuthenticationFailed => write!(f, "authentication failed"),
            HandshakeRejectReason::StaleNonce => {
                write!(f, "stale or replayed handshake nonce")
            }
            HandshakeRejectReason::MalformedHello { message } => {
                write!(f, "malformed hello: {message}")
            }
        }
    }
}

impl std::error::Error for HandshakeRejectReason {}

/// A message on the wire, in either direction. See this module's top doc
/// comment for the message sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WireMessage {
    /// Server -> client, always the first frame on a new connection.
    Challenge {
        server_protocol_version: ProtocolVersion,
        server_capabilities: BTreeSet<String>,
        server_run_id: Uuid,
        nonce: Uuid,
    },
    /// Client -> server, always the second frame (the response to
    /// `Challenge`). `nonce` must echo the `Challenge`'s `nonce` exactly.
    Hello {
        client_protocol_version: ProtocolVersion,
        client_capabilities: BTreeSet<String>,
        client_type: String,
        client_version: String,
        nonce: Uuid,
        token: String,
    },
    /// Server -> client: handshake accepted.
    HelloAck {
        negotiated_capabilities: BTreeSet<String>,
        session_id: Uuid,
        /// Authenticated by server configuration/credential resolution.
        /// Never derived from `client_type`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        principal: Option<AuthenticatedPrincipal>,
    },
    /// Server -> client: handshake rejected. The connection is closed
    /// immediately after this frame.
    HelloReject {
        reason: HandshakeRejectReason,
        server_protocol_version: ProtocolVersion,
    },
    /// Client -> server: invoke a unary method. `deadline_unix_millis` is an
    /// absolute deadline (see [`crate::time`]); the server cancels the call
    /// if it has not completed by then.
    Call {
        correlation_id: CorrelationId,
        deadline_unix_millis: u64,
        method: Method,
    },
    /// Server -> client: the terminal (and only) response to a `Call`.
    CallResult {
        correlation_id: CorrelationId,
        outcome: CallOutcome,
    },
    /// Client -> server: invoke a streaming method.
    CallStream {
        correlation_id: CorrelationId,
        deadline_unix_millis: u64,
        method: StreamMethod,
    },
    /// Server -> client: one item of a streaming call's response.
    StreamItem {
        correlation_id: CorrelationId,
        seq: u32,
        item: StreamItemPayload,
    },
    /// Server -> client: the terminal frame of a streaming call. Exactly
    /// one is sent per `CallStream`, always (success, error, deadline, or
    /// cancellation all produce this — the client never has to infer stream
    /// end from a dropped connection alone).
    StreamEnd {
        correlation_id: CorrelationId,
        outcome: StreamOutcome,
    },
    /// Client -> server: ask the server to stop an in-flight `Call`/
    /// `CallStream` early. Best-effort: if the call already finished, this
    /// is a silent no-op.
    CancelCall { correlation_id: CorrelationId },
}

impl WireMessage {
    /// Enforces semantic bounds immediately after JSON decoding, before dispatch.
    pub fn validate_contract(&self) -> Result<(), String> {
        match self {
            WireMessage::Hello {
                client_capabilities,
                client_type,
                client_version,
                token,
                ..
            } => {
                if client_capabilities.len() > 64
                    || client_capabilities
                        .iter()
                        .any(|value| value.is_empty() || value.len() > 100)
                    || client_type.is_empty()
                    || client_type.len() > 100
                    || client_version.is_empty()
                    || client_version.len() > 100
                    || token.is_empty()
                    || token.len() > 4096
                {
                    return Err("handshake fields exceed contract bounds".to_string());
                }
            }
            WireMessage::HelloAck {
                negotiated_capabilities,
                principal,
                ..
            } => {
                if negotiated_capabilities.len() > 64
                    || negotiated_capabilities
                        .iter()
                        .any(|value| value.is_empty() || value.len() > 100)
                    || principal.as_ref().is_some_and(|value| {
                        !crate::business::safe_id(&value.principal_id, 200)
                            || value.authorized_entity_ids.len() > 4096
                            || value
                                .authorized_entity_ids
                                .iter()
                                .any(|entity| !crate::business::safe_id(entity, 200))
                            || value.authority.len() > 16
                    })
                {
                    return Err("server principal projection exceeds contract bounds".to_string());
                }
            }
            WireMessage::Call { method, .. } => {
                crate::server::validate_method(method).map_err(|error| error.to_string())?
            }
            WireMessage::CallStream {
                method: StreamMethod::BusinessEvents(params),
                ..
            } => {
                if params.max_items == 0
                    || params.max_items > crate::business::MAX_STREAM_ITEMS
                    || !crate::business::safe_id(&params.entity_id, 200)
                {
                    return Err("business stream item bound is invalid".to_string());
                }
            }
            WireMessage::CallResult {
                outcome:
                    CallOutcome::Ok {
                        result: MethodResult::Business(result),
                    },
                ..
            } if !result.validate_contract() => {
                return Err("business result exceeds contract bounds".to_string());
            }
            WireMessage::StreamItem {
                item: StreamItemPayload::BusinessEvent(event),
                ..
            } if !event.validate_contract() => {
                return Err("business event exceeds contract bounds".to_string());
            }
            WireMessage::StreamEnd {
                outcome:
                    StreamOutcome::Ok {
                        summary: StreamSummary::BusinessEvents(summary),
                    },
                ..
            } if !summary.validate_contract(crate::business::MAX_STREAM_ITEMS) => {
                return Err("business stream summary exceeds contract bounds".to_string());
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_same_major_is_compatible_regardless_of_minor() {
        let v1_0 = ProtocolVersion::new(1, 0);
        let v1_5 = ProtocolVersion::new(1, 5);
        assert!(v1_0.is_compatible_with(&v1_5));
        assert!(v1_5.is_compatible_with(&v1_0));
    }

    #[test]
    fn protocol_version_different_major_is_incompatible() {
        let v1 = ProtocolVersion::new(1, 0);
        let v2 = ProtocolVersion::new(2, 0);
        assert!(!v1.is_compatible_with(&v2));
    }

    #[test]
    fn protocol_version_display_is_dotted() {
        assert_eq!(ProtocolVersion::new(1, 2).to_string(), "1.2");
    }

    #[test]
    fn wire_message_round_trips_through_json() {
        let msg = WireMessage::Call {
            correlation_id: Uuid::nil(),
            deadline_unix_millis: 42,
            method: Method::Ping(PingParams {
                payload: "hi".to_string(),
            }),
        };
        let encoded = serde_json::to_string(&msg).expect("serialize");
        let decoded: WireMessage = serde_json::from_str(&encoded).expect("deserialize");
        match decoded {
            WireMessage::Call {
                correlation_id,
                deadline_unix_millis,
                method: Method::Ping(params),
            } => {
                assert_eq!(correlation_id, Uuid::nil());
                assert_eq!(deadline_unix_millis, 42);
                assert_eq!(params.payload, "hi");
            }
            other => panic!("unexpected round trip: {other:?}"),
        }
    }

    #[test]
    fn baseline_capabilities_contains_the_core_marker() {
        assert!(baseline_capabilities().contains(CORE_CAPABILITY));
    }

    #[test]
    fn v1_0_may_not_be_granted_the_business_capability() {
        let legal = capabilities_for_version(PREVIOUS_PROTOCOL_VERSION);
        assert!(!legal.contains(BUSINESS_CAPABILITY));
        let legal_current = capabilities_for_version(PROTOCOL_VERSION);
        assert!(legal_current.contains(BUSINESS_CAPABILITY));
    }

    #[test]
    fn rpc_error_display_is_operator_readable() {
        assert_eq!(RpcError::DeadlineExceeded.to_string(), "deadline exceeded");
        assert_eq!(RpcError::Cancelled.to_string(), "cancelled");
        assert!(
            RpcError::InvalidRequest {
                message: "count too large".to_string()
            }
            .to_string()
            .contains("count too large")
        );
    }

    #[test]
    fn handshake_reject_reason_display_names_the_case() {
        let reason = HandshakeRejectReason::VersionMismatch {
            server_version: ProtocolVersion::new(2, 0),
            client_version: ProtocolVersion::new(1, 0),
        };
        let rendered = reason.to_string();
        assert!(rendered.contains("2.0"));
        assert!(rendered.contains("1.0"));
    }

    #[test]
    fn unknown_wire_message_type_is_a_clear_deserialize_error_not_a_panic() {
        let bogus = r#"{"type":"NotARealMessage"}"#;
        let result: Result<WireMessage, _> = serde_json::from_str(bogus);
        assert!(result.is_err());
    }
}
