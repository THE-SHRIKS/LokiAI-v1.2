//! LokiAI Main v1.2 — authenticated local IPC transport (`RUN-002`).
//!
//! An authenticated Windows named-pipe RPC transport: a version/capability
//! handshake, per-call deadlines and cancellation, and server-to-client
//! streaming with real backpressure. This crate is **pure transport** — see
//! `Docs/graph/tasks/RUN-002.md` §1: "No business logic — pure transport +
//! handshake + framing." It ships exactly one proof-of-life RPC method
//! (`ping`) and one proof-of-life streaming method (`echo_stream`) so the
//! pipe is demonstrably exercised end-to-end; real business methods are a
//! later node's job (see [`protocol::Method`]/[`protocol::StreamMethod`]'s
//! docs for where those land).
//!
//! ## Scope boundary
//!
//! This crate depends on `loki-runtime` for nothing (it has no IPC surface
//! of its own yet — `RUN-001` deliberately excluded that). `loki-runtime`
//! does not depend on this crate either; wiring loki-ipc into the runtime's
//! actual process is `RUN-003`'s job, per the task packet.
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
//! - [`time`]: UTC-millisecond deadline helpers (no new date/time crate).
//! - [`frame`]: length-prefixed frame codec (bounded size, malformed/
//!   truncated handling).
//! - [`acl`]: Windows named-pipe security-descriptor construction.
//! - [`auth`]: shared-secret token type and handshake accept/reject
//!   decision logic (pure — no I/O).
//! - [`server`]: named-pipe server, per-connection handshake + dispatch,
//!   deadlines/cancellation, streaming with backpressure.
//! - [`client`]: named-pipe client, handshake, typed `ping`/`echo_stream`
//!   calls.

#[cfg(not(windows))]
compile_error!(
    "loki-ipc targets Windows named pipes only for LokiAI Main v1.2 (see \
     Docs/TECH_STACK.md §2: Windows 11 x64 is the only supported v1.2 \
     platform). A Unix-domain-socket transport is explicit future scope \
     (Docs/ARCHITECTURE.md §5: \"later macOS/Linux\") and is not \
     implemented by this crate — building it here would be inventing a \
     dependency-graph edge and a code path that is not this node's job."
);

// `protocol`/`frame`/`time`/`auth` hold no Windows-specific code (pure data
// types and generic-over-`AsyncRead`/`AsyncWrite` logic) and are left
// ungated. `acl`/`server`/`client` use `tokio::net::windows::named_pipe`
// and `windows-sys` directly, so they are gated to `cfg(windows)`
// explicitly — belt-and-suspenders with this file's top-level
// `compile_error!`, so a non-Windows build fails with exactly that one
// clear message instead of a cascade of "module not found"/unresolved-
// import errors from code that could never have compiled there anyway.
pub mod auth;
pub mod business;
pub mod frame;
pub mod protocol;
pub mod time;

#[cfg(windows)]
pub mod acl;
#[cfg(windows)]
pub mod client;
#[cfg(windows)]
pub mod server;

#[cfg(windows)]
pub use client::{CallError, Client, ClientConfig, ConnectError, StreamHandle};
pub use protocol::{PROTOCOL_VERSION, ProtocolVersion};
#[cfg(windows)]
pub use server::{Server, ServerConfig};
