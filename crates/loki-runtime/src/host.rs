//! Background host composition (`RUN-003`): wires `RUN-001`'s process
//! skeleton (`crate::runtime`, `crate::health`, `crate::shutdown`) together
//! with `RUN-002`'s authenticated local IPC listener (`loki-ipc`) and
//! `DAT-001`'s persisted storage (`loki-storage`) into one supervised,
//! desktop-discoverable background host — see
//! `Docs/graph/tasks/RUN-003.md`.
//!
//! ## Module map
//!
//! - [`compose`]: starts and runs one background host instance (storage +
//!   IPC listener + discovery-file lifecycle), reusing `crate::runtime::run`
//!   unmodified for the shutdown/heartbeat race.
//! - [`supervisor`]: a userland process supervisor that restarts a crashed
//!   `loki-runtime` child, bounded by a restart budget. **Not** an OS
//!   service-manager integration — see that module's doc comment and
//!   `Docs/graph/tasks/RUN-003.md` §7's stop condition.
//! - [`discovery`]: the documented mechanism a desktop client uses to find a
//!   running host — a well-known per-user named pipe plus a discovery file,
//!   never trusting the file alone for a "healthy" verdict.
//! - [`paths`]: the on-disk layout one host instance's local state lives
//!   under.
//! - [`token`]: interim local persistence for `loki-ipc`'s shared-secret
//!   token (see that module's doc comment for the recorded DPAPI gap).
//! - [`clock`]: minimal UTC/Unix-millis "now" helpers this composition needs
//!   as a caller of `loki-storage` (which never reads a system clock itself)
//!   and of the discovery file format.
//! - [`business_service`] (`P1-RUNTIME-COMP-001`): the real
//!   `loki_ipc::server::BusinessService` implementation `compose` installs
//!   into the bound `Server`, replacing the fail-closed
//!   `UnsupportedBusinessService` default for the 8 in-scope business
//!   methods — see that module's doc comment for exactly which methods and
//!   the security convention it documents.
//! - [`credential`] (`P1-NODE-IPC-CLIENT-001`/`ADR-0009`): mints/loads the
//!   `DesktopInstallation`/`AgentConnection` DPAPI-protected credentials
//!   `compose` persists alongside the legacy token — additive, disk-only in
//!   this node (not yet read back into the live `Server`'s
//!   `CredentialResolver`; see that module's doc comment for why).
//!
//! ## Platform gate
//!
//! Windows-only, matching `loki-ipc`'s own scope: that crate refuses to
//! compile on any other target at all (`crates/loki-ipc/src/lib.rs`'s
//! top-level `compile_error!`), and `Cargo.toml` only pulls `loki-ipc` in as
//! a `cfg(windows)` target dependency. This module is gated the same way at
//! its declaration in `src/lib.rs` (`#[cfg(windows)] pub mod host;`) so
//! `cargo check -p loki-runtime` in isolation (not `--workspace`) still
//! compiles for a contributor on a non-Windows machine, exactly as
//! `Cargo.toml`'s dependency comment for `loki-ipc` describes.

pub mod business_service;
pub mod clock;
pub mod compose;
pub mod credential;
pub mod discovery;
pub mod paths;
pub mod supervisor;
pub mod token;
