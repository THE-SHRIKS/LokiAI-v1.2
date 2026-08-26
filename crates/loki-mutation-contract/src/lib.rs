//! Runtime mutation acceptance contract (`P1-MUTATION-CONTRACT-001`).
//!
//! Encodes ADR-0006 ("Transactional runtime mutation acceptance",
//! `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`)
//! as closed, reviewable internal Rust types and pure functions: grant
//! generations, request digests, idempotency keys, the closed
//! mutation-method vocabulary, and discriminated acceptance/revocation
//! states. See `Docs/SPEC.md` Section 1 (`Envelope<T>`, `Error`,
//! `OperationResult<T>`) and Section 4.7 (durable job) for the wire-shape
//! provenance this crate's identifiers/states cite by reuse
//! (`loki_storage::ids`, `loki_storage::states::JobState`) — this crate does
//! not itself depend on `packages/contracts` or define any wire schema.
//!
//! # Zero I/O, zero callback surface
//!
//! This crate performs no filesystem/network/database access, is not
//! `async`, and defines no trait with a side-effecting or injectable method
//! (no `dyn Fn`, no `Box<dyn Trait>` used as a callback, no
//! `Arc<Mutex<...>>` used as an authorization/commit boundary). ADR-0006
//! "Rejected alternatives" explicitly forbids the callback/lease pattern
//! this rule exists to prevent — independent RPC review REV-004 proved that
//! pattern cannot make credential revalidation and a side effect atomic.
//! `P1-ACCEPTANCE-STORE-001` is the node that implements the real one-
//! transaction acceptance flow against these types; this crate only defines
//! the shapes and the pure decision functions it calls from inside that
//! transaction.
//!
//! # Five open questions this crate does not resolve
//!
//! `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md` §9 names five open
//! questions this crate deliberately leaves unresolved rather than
//! silently deciding. See each cited module's doc comment for where the
//! extension point lives, and this node's handoff
//! (`Docs/graph/handoffs/P1-MUTATION-CONTRACT-001-1.md`) for the full
//! record:
//!
//! 1. "Declared safe point" for cooperative post-commit cancellation — see
//!    [`revocation`].
//! 2. `outcome_unknown` detection contract — see [`revocation`].
//! 3. `GrantGeneration` vs. `loki_local_authority::LocalSession` — see
//!    [`grant`].
//! 4. Cross-crate single-transaction feasibility — not a type in this
//!    crate; flagged in the handoff only (this crate's types are
//!    connection-agnostic specifically so they do not assume a transaction
//!    topology that does not exist yet).
//! 5. "Binds workspace" with no local workspace concept in Part One — see
//!    [`request`].

pub mod digest;
pub mod error;
pub mod grant;
pub mod method;
pub mod request;
pub mod result;
pub mod revocation;

pub use digest::{IdempotencyKey, RequestDigest, compute_request_digest};
pub use error::MutationContractError;
pub use grant::{GrantGeneration, evaluate_grant};
pub use method::{MutationMethod, PolicyTier};
pub use request::{ApprovalReference, MutationAcceptanceRequest, evaluate_approval_reference};
pub use result::{
    AcceptanceRecordView, AcceptanceResult, AcceptedRecord, MutationRejection, ReplayDecision,
    construct_accepted, evaluate_replay, replayed_result,
};
pub use revocation::{RevocationDisposition, evaluate_revocation_disposition};
