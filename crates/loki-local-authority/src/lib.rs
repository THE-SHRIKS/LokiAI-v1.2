//! Windows-bound local installation authority (`LOC-001`).
//!
//! This is deliberately not account, workspace, role, OAuth, or cloud
//! authority. It binds one LokiAI installation to the Windows identity running
//! the local runtime and issues short-lived, revocable metadata sessions bound
//! to an already-authenticated RUN-002 IPC session. Session IDs are not bearer
//! credentials and must never be accepted without that IPC context.
//!
//! Cloud-required capabilities always fail closed. Consequential operations are
//! still subject to APR-001 policy and digest-bound human approval; this crate
//! cannot directly authorize a build, flash, restart, or deployment.

mod error;
mod model;
mod store;

pub use error::AuthorityError;
pub use model::{CloudCapability, LocalCapability, LocalContext, LocalInstallation, LocalSession};
pub use store::{LocalAuthority, evaluate_local_session};
