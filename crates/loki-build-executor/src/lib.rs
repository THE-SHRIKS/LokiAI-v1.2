//! Unprivileged contracts for LokiAI's disposable per-job build capsule.
//!
//! This crate intentionally does not create, attach, mount, format, or detach
//! disks. It defines the authenticated broker boundary, fail-closed lifecycle,
//! capsule-internal access plan, contained executor policy, and external output
//! validation consumed by the separately approved host implementation.

#![forbid(unsafe_code)]

pub mod broker;
pub mod containment;
pub mod error;
pub mod journal;
pub mod lifecycle;
pub mod native;
pub mod protocol;
pub mod recipe;
pub mod windows_boundary;

pub use error::{BuildError, ErrorCode};
