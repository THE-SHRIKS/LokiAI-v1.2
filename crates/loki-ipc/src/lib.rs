//! LokiAI Main v1.2 — authenticated local IPC transport (`RUN-002`,
//! extended by `P1-RPC-CONTRACT-001-R1`).
//!
//! An authenticated Windows named-pipe RPC transport: a version/capability
//! handshake, per-call deadlines and cancellation, server-to-client
//! streaming with real backpressure ([`RUN-002`]'s `ping`/`echo_stream`
//! proof-of-life pair), and a closed, typed, authenticated business RPC
//! vocabulary ([`business`]/[`protocol::Method`]/[`protocol::StreamMethod`]
//! — `P1-RPC-CONTRACT-001-R1`). This crate remains **transport, not
//! mutation authority**: every business mutation result this crate's wire
//! types can represent is a 1:1 mapping (via [`mutation_mapping`]) of a
//! value `loki-mutation-contract`'s closed, private-constructor
//! `AcceptanceResult`/`MutationRejection` types already produced elsewhere
//! (`ADR-0006`/REV-004 — "IPC cannot be the mutation-acceptance boundary,
//! only a transport in front of one"). No re-implementation of atomic
//! mutation acceptance, approval consumption, idempotency, or replay logic
//! exists anywhere in this crate.
//!
//! ## Scope boundary
//!
//! This crate has a read-only production dependency on
//! `loki-mutation-contract` (a pure, I/O-free contract crate) for its
//! result-vocabulary structural fidelity — see [`business`]'s and
//! [`mutation_mapping`]'s doc comments. It has **no** production dependency
//! on `loki-runtime` or `loki-acceptance-store`: composing this transport
//! against a live `RuntimeServices`-backed [`server::BusinessService`] is
//! `P1-RUNTIME-COMP-001`'s job, a separate `INTEGRATION` node.
//!
//! ## Connection-level authentication
//!
//! Per `Docs/SECURITY.md` §4's "Local client impersonation" threat row, two
//! independent controls gate every connection before any RPC call is
//! accepted:
//!
//! 1. **OS ACL** ([`acl`]): the named pipe's security descriptor restricts
//!    `CreateFile`/`ConnectNamedPipe` access to the current Windows user's
//!    SID. A process running as a different Windows user cannot open a
//!    handle to the pipe at all — enforced by the OS kernel before any
//!    application byte is exchanged.
//! 2. **Per-session token + nonce challenge** ([`auth`]): the server issues
//!    a fresh random nonce per connection; the client must prove knowledge
//!    of a shared secret token *and* echo that exact nonce back. Replaying
//!    a captured `Hello` from an earlier connection fails because the nonce
//!    it contains is stale (see `tests/auth.rs`).
//!
//! This is connection-level authentication only — proving the peer is a
//! legitimate local LokiAI process. User/role authorization (RBAC) is
//! explicitly out of scope (`APR-001`).
//!
//! ## Module map
//!
//! - [`protocol`]: wire message types, protocol version/capabilities, RPC
//!   method/result types.
//! - [`business`]: the closed business-RPC vocabulary (params, principals,
//!   authorities, read/mutation result wire types).
//! - [`mutation_mapping`]: the sole place this crate constructs a mutation
//!   wire result, from a real `loki_mutation_contract::AcceptanceResult`.
//! - [`time`]: UTC-millisecond deadline helpers (no new date/time crate).
//! - [`frame`]: length-prefixed frame codec (bounded size, malformed/
//!   truncated handling).
//! - [`acl`]: Windows named-pipe security-descriptor construction.
//! - [`auth`]: shared-secret token type, handshake accept/reject decision
//!   logic, and dynamic per-credential principal resolution/revalidation
//!   (pure — no I/O).
//! - [`server`]: named-pipe server, per-connection handshake + dispatch,
//!   deadlines/cancellation, streaming with backpressure, authorization,
//!   and entity-identity coupling.
//! - [`client`]: named-pipe client, handshake, typed `ping`/`echo_stream`/
//!   business calls and streams.

#[cfg(not(windows))]
compile_error!(
    "loki-ipc targets Windows named pipes only for LokiAI Main v1.2 (see \
     Docs/TECH_STACK.md §2: Windows 11 x64 is the only supported v1.2 \
     platform). A Unix-domain-socket transport is explicit future scope \
     (Docs/ARCHITECTURE.md §5: \"later macOS/Linux\") and is not \
     implemented by this crate — building it here would be inventing a \
     dependency-graph edge and a code path that is not this node's job."
);

// `protocol`/`frame`/`time`/`auth`/`business`/`mutation_mapping` hold no
// Windows-specific code (pure data types and generic-over-`AsyncRead`/
// `AsyncWrite` logic) and are left ungated. `acl`/`server`/`client` use
// `tokio::net::windows::named_pipe` and `windows-sys` directly, so they are
// gated to `cfg(windows)` explicitly — belt-and-suspenders with this file's
// top-level `compile_error!`, so a non-Windows build fails with exactly
// that one clear message instead of a cascade of "module not found"/
// unresolved-import errors from code that could never have compiled there
// anyway.
pub mod auth;
pub mod business;
pub mod frame;
pub mod mutation_mapping;
pub mod protocol;
pub mod time;

#[cfg(windows)]
pub mod acl;
#[cfg(windows)]
pub mod client;
#[cfg(windows)]
pub mod server;

#[cfg(windows)]
pub use client::{
    BusinessStreamEvent, BusinessStreamHandle, CallError, Client, ClientConfig, ConnectError,
    StreamHandle,
};
pub use protocol::{PROTOCOL_VERSION, ProtocolVersion};
#[cfg(windows)]
pub use server::{
    BusinessEventEmitter, BusinessRequestContext, BusinessService, Server, ServerConfig,
};
