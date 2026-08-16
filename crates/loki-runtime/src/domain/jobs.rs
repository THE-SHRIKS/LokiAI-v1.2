//! Jobs, Approvals, and Policy (`Docs/ARCHITECTURE.md` §7, bounded context
//! 6): durable job state machines, policy enforcement, and approval
//! validation for the operation pipeline in `Docs/ARCHITECTURE.md` §10.
//! Owned by `JOB-001` and later policy nodes. Not implemented by `RUN-001` —
//! empty on purpose; see `src/domain.rs`.
