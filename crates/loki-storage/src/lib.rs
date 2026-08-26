//! `loki-storage` — LokiAI Main v1.2 runtime-owned SQLite storage
//! (`DAT-001`).
//!
//! Provides: a versioned, idempotent SQL migration runner; a repository
//! pattern over three entities (device registry, durable job records, and
//! an outbox/inbox pair for future cloud sync); an integrity check
//! (`PRAGMA integrity_check`); and a consistency-guaranteed backup
//! primitive. See `Docs/graph/tasks/DAT-001.md` for exact scope and
//! `Docs/ARCHITECTURE.md` §8 "Persistence" for the broader persistence
//! contract this crate implements the runtime-owned SQLite part of.
//!
//! ## What this crate deliberately is not
//!
//! - **Not a credential store.** Managed Windows opens use SQLCipher and a
//!   DPAPI-protected database key, but credentials remain outside the database
//!   and belong to the operating-system protection boundary.
//! - **Not an IPC surface.** This is a plain Rust library with no network/
//!   transport code. `RUN-002` is expected to consume it, not the other way
//!   around (`Docs/graph/tasks/DAT-001.md` §5).
//! - **Not a sync/delivery engine.** The outbox/inbox tables
//!   ([`outbox`]/[`inbox`]) are durable structure and a repository
//!   read/write API only — no delivery/transport logic. That is `CLD-002`'s
//!   scope (`Docs/graph/tasks/DAT-001.md` §5).
//! - **Not a full entity model.** Only device registry ([`devices`]), job
//!   records ([`jobs`]), and outbox/inbox are implemented, deliberately, to
//!   exercise the repository pattern for later nodes to extend
//!   (`Docs/graph/tasks/DAT-001.md` Deliverable 3) — not to model every
//!   `Docs/SPEC.md` Section 3 entity speculatively.
//! - **Not a connection pool.** [`Storage`] wraps one `Mutex`-guarded
//!   connection; see `src/storage.rs` for why that is the right amount of
//!   concurrency support for this node's bounded scope.
//!
//! ## Timestamps
//!
//! This crate never reads a system clock. Every timestamp a repository
//! method accepts is a caller-supplied, UTC ISO 8601 string, validated at
//! the boundary (`src/timestamp.rs`). See that module's doc comment for the
//! rationale.

pub mod cipher;
pub mod devices;
pub mod error;
pub mod ids;
pub mod inbox;
pub mod jobs;
pub mod key;
#[cfg(windows)]
pub mod managed;
mod migrations;
pub mod outbox;
#[cfg(windows)]
pub mod secret;
#[cfg(windows)]
pub mod secure_memory;
#[cfg(windows)]
pub mod secure_profile;
pub mod states;
mod storage;
pub mod timestamp;

pub use cipher::Cipher;
pub use error::StorageError;
pub use key::DatabaseKey;
#[cfg(windows)]
pub use managed::{backup_sqlcipher_database, open_or_migrate_sqlcipher, rotate_sqlcipher_key};
pub use migrations::MigrationReport;
#[cfg(windows)]
pub use secure_memory::{SecureMemoryError, SecureMemoryRuntime, SecureMemoryStats};
#[cfg(windows)]
pub use secure_profile::{
    ProfileError, ProfileInspection, digest_profile_manifest, inspect_profile,
};
pub use storage::{IntegrityStatus, Storage, configure_connection, configure_sqlcipher};
