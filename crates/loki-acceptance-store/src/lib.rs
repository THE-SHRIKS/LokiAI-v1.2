//! `loki-acceptance-store` — the real ADR-0006 one-transaction mutation
//! acceptance engine (`P1-ACCEPTANCE-STORE-001`).
//!
//! `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
//! step-by-step, all inside one `loki_storage::Storage::transaction()` call:
//!
//! 1. revalidate the principal's current `GrantGeneration`
//!    (`loki_mutation_contract::evaluate_grant`);
//! 2. bind method/digest/idempotency key/target/artifact/approval inputs —
//!    already done by the caller via `MutationAcceptanceRequest::new`
//!    (no workspace/scopes, ADR-0006 Amendment 1);
//! 3. claim or replay the idempotency record
//!    (`loki_mutation_contract::evaluate_replay`/`replayed_result`),
//!    rejecting same-key/different-digest;
//! 4. validate and consume the exact approval when required
//!    (`loki_mutation_contract::evaluate_approval_reference`, invoked
//!    through `construct_accepted`);
//! 5. create or retrieve the durable job/mutation record;
//! 6. append the transactional outbox record;
//! 7. commit before returning any `AcceptanceResult`.
//!
//! No connector/device/build/deploy effect runs inside the transaction, or
//! anywhere in this crate at all.
//!
//! # No bypass of `loki-mutation-contract`'s constructors
//!
//! This crate never struct-literal-constructs an `AcceptedRecord` or an
//! `AcceptanceResult::Accepted`/`ReplayedIdenticalAccepted` — both are only
//! ever obtained from `loki_mutation_contract::construct_accepted` or
//! `loki_mutation_contract::replayed_result`, which is enforced by that
//! crate's own private-field `AcceptedRecord` design (see its doc comment).
//!
//! # `P1-PRINCIPAL-GRANT-001` (ADR-0007 Option B): per-identity-model grant read
//!
//! Step 1's `GrantGeneration` source of truth is no longer the old,
//! explicitly-provisional `loki_storage::local_authority::find_grant_generation_core`
//! shim (which mapped `generation == issued_at_ms`, with a documented
//! Low-severity ambiguity on duplicate `issued_at_ms` values for one
//! principal). It is now a real per-identity-model read, branched on
//! `request.method.requires_approval()` — already-accepted, unmodified
//! `loki_mutation_contract::MutationMethod`'s exact, exhaustive, 1:1
//! discriminant for "which identity model does this request's
//! `principal_sid` belong to" (the four `ApprovalRequired` methods are
//! always Windows-SID `LocalSession` principals; the three `NotRequired`
//! methods are always agent-connection `AgentCredential` principals — see
//! `Docs/graph/decisions/ADR-0007-grant-generation-identity-unification.md`
//! and `Docs/graph/tasks/P1-PRINCIPAL-GRANT-001.md` §1.1):
//!
//! - `requires_approval() == true` reads
//!   `loki_storage::local_authority::find_local_generation_core` — a real
//!   durable per-session generation, with a real `expires_at_ms`.
//! - `requires_approval() == false` reads
//!   `loki_storage::agents::find_agent_generation_core` — a real durable
//!   per-connection generation. `AGT-001` has no expiry concept at all
//!   (ADR-0007), so this crate alone — never `loki-storage` — supplies the
//!   local `AGENT_GRANT_NEVER_EXPIRES_MS` sentinel at the single point it
//!   constructs a `GrantGeneration` for an agent-connection read.
//!
//! **No cross-model kill switch exists after this replacement** (ADR-0007's
//! explicit, declined-scope limitation, restated here so no future reader
//! assumes a stronger guarantee): revoking a Windows-principal's
//! `LocalSession` generation never cascades to revoke any `AgentCredential`,
//! and vice versa. Each branch converts its own row type into a
//! `GrantGeneration` at its own call site — no shared conversion helper
//! across the two branches (ADR-0007's "no shared merge" requirement).
//!
//! Known, inherited naming overload (not introduced by this node, not fixed
//! here): `MutationAcceptanceRequest.principal_sid: String` is an
//! already-accepted field name from `loki-mutation-contract` that, for
//! agent methods, actually carries an `AgentCredential::connection_id`, not
//! a Windows SID — see `Docs/graph/tasks/P1-PRINCIPAL-GRANT-001.md` §1.1
//! and §9 item 2.
//!
//! # `P1-APPROVAL-JOB-ACCEPT-001`: changed-request rejection and cancellation/
//! consumption isolation (evidence, not new behavior)
//!
//! Two properties this packet's own review needed direct, runnable evidence
//! for — both already true of the code above, neither changed by this
//! packet:
//!
//! (a) A fresh request (new idempotency key) that reuses a previously
//! granted approval's `approval_id` but changes a bound field (e.g.
//! `target_id`) is rejected as [`loki_mutation_contract::MutationRejection::ApprovalNotConsumable`].
//! Step 4 above always refreshes the caller-supplied `ApprovalReference`
//! from the approval row's own stored `request_digest` before calling
//! `construct_accepted`, so `evaluate_approval_reference`'s digest
//! comparison is against the *original* request's digest, not whatever the
//! caller's `ApprovalReference` claims — and `loki_storage::approvals::consume_approval_core`
//! independently re-derives and re-checks that same binding directly against
//! the row before its one permitted write, so this property holds even if
//! `evaluate_approval_reference`'s check were ever removed (two independent
//! layers, not one). Proven by
//! `crates/loki-acceptance-store/tests/acceptance_flow.rs`'s
//! `retry_with_changed_target_after_approval_is_rejected_as_not_consumable`
//! (`Docs/graph/handoffs/P1-APPROVAL-JOB-ACCEPT-001-1.md` records that this
//! test was confirmed to fail red with *both* layers' digest checks
//! disabled locally, not committed).
//!
//! (b) No code path anywhere in this workspace can un-consume an approval
//! (reset `local_approvals.consumed_at_ms` back to `NULL`) — in particular,
//! cancelling a job whose approval was already consumed never reverses that
//! consumption. This is a structural absence: `loki_jobs::JobEngine`'s
//! entire cancellation model (`request_cancel`/`acknowledge_cancelled`,
//! `crates/loki-jobs/src/engine.rs`) has zero reference anywhere to
//! `loki_storage::approvals`/`ApprovalsRepository`/`local_approvals`, and no
//! function in `crates/loki-storage`, `crates/loki-approvals`, or
//! `crates/loki-jobs` ever writes `consumed_at_ms = NULL`. Confirmed by
//! repository-wide grep (commands and output recorded in
//! `Docs/graph/handoffs/P1-APPROVAL-JOB-ACCEPT-001-1.md`) and proven
//! end-to-end against the real crate by
//! `job_cancellation_has_no_approval_consumption_interaction` in the same
//! test file, which runs a real `JobEngine` cancellation and reconfirms the
//! real approval row's `consumed_at_ms` is unchanged afterward. No claim is
//! made here that a cross-crate "cancellation reverses approval" guard
//! exists as code — only that no reversal path exists at all, so there is
//! nothing for such a guard to prevent.
//!
//! # Open questions this crate does not resolve
//!
//! See `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md` §9,
//! `Docs/graph/tasks/P1-PRINCIPAL-GRANT-001.md` §9, and this node's
//! handoffs for the full record. In summary: (1) the clock source
//! deliberately does not import `loki-runtime`'s clock — see [`clock`]'s
//! doc comment; (2) the `outcome_unknown` detection trigger and "declared
//! safe point" mechanism remain undefined — this crate's transaction never
//! runs a physical effect, so it never reaches that decision point; (3)
//! lock-contention under load is a named risk for `P1-RUNTIME-COMP-001`/
//! `P1-MUTATION-REV-001` to verify, not fixed here (incrementally larger
//! now that `rotate`/`change_scopes` add two more brief-lock code paths);
//! (4) the agent-method `job_id` minting scheme (`mjob_<uuid>`, see
//! [`synthesize_agent_job_id`]) is recorded here for
//! `P1-AGENT-MUTATION-IDEMP-001`'s benefit; (5) `MutationAcceptanceRequest`
//! never gaining a real principal-type discriminant field (deriving it
//! instead from `method.requires_approval()`) is a deliberate, currently
//! out-of-scope choice — a future `loki-mutation-contract` change to add
//! one is its own review, not assumed here.

pub mod clock;

use loki_mutation_contract::{
    AcceptanceRecordView, AcceptanceResult, ApprovalReference, MutationAcceptanceRequest,
    MutationContractError, MutationRejection, RequestDigest, construct_accepted, evaluate_grant,
    evaluate_replay, replayed_result,
};
use loki_storage::Storage;
use loki_storage::StorageError;
use loki_storage::agents::{self, AgentGrantGenerationRow};
use loki_storage::approvals::{self, ConsumeOutcome};
use loki_storage::ids::{CorrelationId, JobId, MessageId};
use loki_storage::jobs::{self, NewJob};
use loki_storage::local_authority::{self, LocalGrantGenerationRow};
use loki_storage::mutation_acceptance::{self, MutationAcceptanceRow, NewMutationAcceptance};
use loki_storage::outbox::{self, NewOutboxMessage};
use loki_storage::states::JobState;
use uuid::Uuid;

/// `P1-PRINCIPAL-GRANT-001` (ADR-0007 Option B): `AGT-001` has no expiry
/// concept at all — `loki_mutation_contract::GrantGeneration::new` requires
/// `expires_at_ms > issued_at_ms` unconditionally, so an agent-connection
/// read (which has no real expiry timestamp to supply) uses this sentinel
/// instead. This constant is deliberately confined to this crate: it must
/// never appear in `loki-storage`'s row types or SQL (ADR-0007's explicit
/// boundary — see `loki_storage::agents`'s module doc comment).
const AGENT_GRANT_NEVER_EXPIRES_MS: i64 = i64::MAX;

/// Errors this crate can return. `Storage(...)` is a genuine database
/// failure (a real bug or I/O fault, not a business rejection —
/// `AcceptanceResult::Rejected` covers every legitimate business
/// rejection). `Contract(...)` surfaces
/// `loki_mutation_contract::MutationContractError` — always a caller/
/// construction bug per that crate's own documented contract (e.g. an
/// approval-gated method reaching this crate without an
/// `approval_reference` at all), never a legitimate runtime rejection.
#[derive(Debug)]
pub enum AcceptanceStoreError {
    Storage(StorageError),
    Contract(MutationContractError),
}

impl std::fmt::Display for AcceptanceStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(e) => write!(f, "mutation acceptance storage error: {e}"),
            Self::Contract(e) => write!(f, "mutation acceptance contract error: {e}"),
        }
    }
}

impl std::error::Error for AcceptanceStoreError {}

impl From<StorageError> for AcceptanceStoreError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<MutationContractError> for AcceptanceStoreError {
    fn from(value: MutationContractError) -> Self {
        Self::Contract(value)
    }
}

/// Runs the full ADR-0006 seven-step acceptance sequence for `request`
/// inside exactly one `Storage::transaction()` call, and returns the
/// resulting `AcceptanceResult` only after that transaction has committed
/// (or, for a purely-rejected outcome, after the same single transaction
/// has committed with no side-effecting rows written — a rejection is
/// still recorded nowhere, by construction, since no repository write
/// happens on any path that returns `AcceptanceResult::Rejected`).
pub fn accept_mutation(
    storage: &Storage,
    request: &MutationAcceptanceRequest,
) -> Result<AcceptanceResult, AcceptanceStoreError> {
    let outcome: Result<AcceptanceResult, MutationContractError> =
        storage.transaction(|tx| accept_mutation_in_transaction(tx, request))?;
    Ok(outcome?)
}

/// The transaction closure body. Returns `Result<Result<AcceptanceResult,
/// MutationContractError>, StorageError>`: the outer `Result` is
/// `Storage::transaction`'s own required error channel (a genuine database
/// failure aborts/rolls back the transaction); the inner `Result` carries
/// `loki_mutation_contract`'s own construction-error channel, which is
/// never itself a reason to roll back statements already validated as
/// legitimate — by the time a `MutationContractError` can occur here
/// (missing `approval_reference` on a method that requires one), no write
/// has happened yet on this call, so returning it via `Ok(Err(...))`
/// (a value, not this function's own `Err`) still rolls back nothing
/// because nothing was written — it is functionally equivalent either way,
/// but keeping it as a value rather than mixing two unrelated error types
/// behind one `?` chain is what stays honest about which failure class each
/// one is.
fn accept_mutation_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    request: &MutationAcceptanceRequest,
) -> Result<Result<AcceptanceResult, MutationContractError>, StorageError> {
    // ADR-0006: `accepted_at_ms` is read from a trusted clock *inside* the
    // transaction closure — this is that read. It also doubles as `now_ms`
    // for the grant-expiry check (step 1): both checks execute inside the
    // same continuously-held `Storage` mutex guard with no intervening I/O,
    // so a single read here is the honest "now" for the whole atomic
    // sequence, not a stale pre-lock value (see `clock`'s doc comment and
    // this crate's top doc comment).
    let now_ms = clock::unix_millis_now();

    // --- Step 1: revalidate the principal's current GrantGeneration. ---
    // `P1-PRINCIPAL-GRANT-001` (ADR-0007 Option B): `request.method.requires_approval()`
    // is the exact, exhaustive, 1:1 discriminant for which identity model
    // `request.principal_sid` belongs to (see this crate's top doc
    // comment) — no shared read/conversion path between the two branches
    // (ADR-0007's "no shared merge" requirement).
    let generation = if request.method.requires_approval() {
        let Some(grant_row): Option<LocalGrantGenerationRow> =
            local_authority::find_local_generation_core(
                tx,
                &request.principal_sid,
                request.grant_generation,
            )?
        else {
            return Ok(Ok(AcceptanceResult::Rejected(
                MutationRejection::GrantInvalid {
                    reason: "no local authority session matches this principal/grant generation"
                        .to_string(),
                },
            )));
        };
        match loki_mutation_contract::GrantGeneration::new(
            grant_row.principal_sid,
            grant_row.generation,
            grant_row.issued_at_ms,
            grant_row.expires_at_ms,
            grant_row.revoked_at_ms,
        ) {
            Ok(generation) => generation,
            Err(e) => {
                return Ok(Ok(AcceptanceResult::Rejected(
                    MutationRejection::GrantInvalid {
                        reason: e.to_string(),
                    },
                )));
            }
        }
    } else {
        let Some(grant_row): Option<AgentGrantGenerationRow> = agents::find_agent_generation_core(
            tx,
            &request.principal_sid,
            request.grant_generation,
        )?
        else {
            return Ok(Ok(AcceptanceResult::Rejected(
                MutationRejection::GrantInvalid {
                    reason: "no agent connection matches this principal/grant generation"
                        .to_string(),
                },
            )));
        };
        // `AGT-001` has no expiry concept — see this crate's top doc
        // comment and `AGENT_GRANT_NEVER_EXPIRES_MS`'s own doc comment.
        match loki_mutation_contract::GrantGeneration::new(
            grant_row.connection_id,
            grant_row.generation,
            grant_row.issued_at_ms,
            AGENT_GRANT_NEVER_EXPIRES_MS,
            grant_row.revoked_at_ms,
        ) {
            Ok(generation) => generation,
            Err(e) => {
                return Ok(Ok(AcceptanceResult::Rejected(
                    MutationRejection::GrantInvalid {
                        reason: e.to_string(),
                    },
                )));
            }
        }
    };
    if let Err(rejection) = evaluate_grant(&generation, now_ms) {
        return Ok(Ok(AcceptanceResult::Rejected(rejection)));
    }

    // --- Step 3: claim or replay the idempotency record. ---
    if let Some(existing) =
        mutation_acceptance::get_mutation_acceptance_core(tx, request.idempotency_key.as_str())?
    {
        let view = match existing_row_to_view(existing) {
            Ok(view) => view,
            Err(e) => return Ok(Err(e)),
        };
        return Ok(Ok(match evaluate_replay(&view, request) {
            loki_mutation_contract::ReplayDecision::Identical => replayed_result(&view),
            loki_mutation_contract::ReplayDecision::DigestConflict => {
                AcceptanceResult::Rejected(MutationRejection::DigestConflict)
            }
        }));
    }

    // --- Step 4: validate and consume the exact approval when required. ---
    let mut approval_id_to_consume: Option<String> = None;
    let request_for_construct = if request.method.requires_approval() {
        match &request.approval_reference {
            None => {
                // No DB read/write has happened above that this transaction
                // needs to undo — `construct_accepted` below will raise the
                // exact `MutationContractError::InvalidInput` this
                // caller/construction bug deserves.
                request.clone()
            }
            Some(reference) => {
                let Some(row) = approvals::get_approval_core(tx, &reference.approval_id)? else {
                    return Ok(Ok(AcceptanceResult::Rejected(
                        MutationRejection::ApprovalNotConsumable,
                    )));
                };
                let consumable = row.requester_sid == request.principal_sid
                    && row.granted_at_ms.is_some()
                    && row.consumed_at_ms.is_none();
                if !consumable {
                    return Ok(Ok(AcceptanceResult::Rejected(
                        MutationRejection::ApprovalNotConsumable,
                    )));
                }
                let refreshed_digest = match RequestDigest::try_from_str(row.request_digest.clone())
                {
                    Ok(digest) => digest,
                    Err(e) => return Ok(Err(e)),
                };
                approval_id_to_consume = Some(row.approval_id.clone());
                let mut refreshed_request = request.clone();
                refreshed_request.approval_reference = Some(ApprovalReference {
                    approval_id: row.approval_id,
                    bound_request_digest: refreshed_digest,
                    expires_at_ms: row.expires_at_ms,
                });
                refreshed_request
            }
        }
    } else {
        request.clone()
    };

    // --- Steps 5-6: create or retrieve the durable job/mutation record,
    // append the transactional outbox record, on `Accepted` only. ---
    let job_id = if request.method.requires_approval() {
        JobId::new(format!("job_{}", Uuid::new_v4().simple()))?
    } else {
        synthesize_agent_job_id()?
    };
    let correlation_id = CorrelationId::new(format!("corr_{}", Uuid::new_v4().simple()))?;

    let accepted = match construct_accepted(
        &request_for_construct,
        job_id.clone(),
        correlation_id.clone(),
        now_ms,
    ) {
        Ok(result) => result,
        Err(e) => return Ok(Err(e)),
    };

    let AcceptanceResult::Accepted(record) = &accepted else {
        // `construct_accepted` itself decided this is a `Rejected` outcome
        // (e.g. the refreshed approval reference failed its own digest/
        // expiry re-check) — nothing to write.
        return Ok(Ok(accepted));
    };

    if let Some(approval_id) = &approval_id_to_consume {
        let outcome = approvals::consume_approval_core(
            tx,
            approval_id,
            request.request_digest.as_str(),
            &request.principal_sid,
            now_ms,
        )?;
        if !matches!(outcome, ConsumeOutcome::Consumed(_)) {
            return Ok(Ok(AcceptanceResult::Rejected(
                MutationRejection::ApprovalNotConsumable,
            )));
        }
    }

    if request.method.requires_approval() {
        jobs::insert_job_core(
            tx,
            &NewJob {
                job_id: job_id.clone(),
                state: JobState::Queued,
                idempotency_key: request.idempotency_key.as_str().to_string(),
                correlation_id: correlation_id.clone(),
                created_at: clock::format_unix_millis_as_iso8601(now_ms),
                updated_at: clock::format_unix_millis_as_iso8601(now_ms),
            },
        )?;
    }

    let message_id = MessageId::new(format!("msg_{}", Uuid::new_v4().simple()))?;
    let payload_json = mutation_accepted_payload_json(request, &job_id, record.accepted_at_ms());
    outbox::enqueue_outbox_message_core(
        tx,
        &NewOutboxMessage {
            message_id,
            stream_key: format!("job:{}", job_id.as_str()),
            message_type: format!("mutation_accepted:{}", request.method.as_str()),
            correlation_id: correlation_id.clone(),
            causation_id: None,
            payload_json,
            created_at: clock::format_unix_millis_as_iso8601(now_ms),
            updated_at: clock::format_unix_millis_as_iso8601(now_ms),
        },
    )?;

    mutation_acceptance::insert_mutation_acceptance_core(
        tx,
        &NewMutationAcceptance {
            idempotency_key: request.idempotency_key.as_str().to_string(),
            principal_sid: request.principal_sid.clone(),
            grant_generation: request.grant_generation,
            method: request.method.as_str().to_string(),
            target_id: request.target_id.clone(),
            artifact_digest: request.artifact_digest.clone(),
            request_digest: request.request_digest.as_str().to_string(),
            job_id: Some(job_id.into_string()),
            correlation_id: correlation_id.into_string(),
            accepted_at_ms: record.accepted_at_ms(),
        },
    )?;

    Ok(Ok(accepted))
}

/// The agent-method `job_id` minting scheme (`P1-MUTATION-CONTRACT-001` §9
/// item 5 / this packet's Deliverable 5): a UUID-based synthesized
/// identifier, prefixed `mjob_` — consistent with existing per-crate
/// conventions this consolidation absorbed (`loki-approvals`'s `apr_`,
/// `loki-agents`'s `ac_`/`ar_`/`las_`, `loki-local-authority`'s `li_`/`ls_`)
/// — through the same `loki_storage::ids::JobId` shape `AcceptedRecord`
/// already requires, without ever inserting a corresponding `jobs` table
/// row. `P1-AGENT-MUTATION-IDEMP-001` should treat this prefix as the
/// stable, already-decided scheme rather than reverse-engineering it.
fn synthesize_agent_job_id() -> Result<JobId, StorageError> {
    JobId::new(format!("mjob_{}", Uuid::new_v4().simple()))
}

fn existing_row_to_view(
    row: MutationAcceptanceRow,
) -> Result<AcceptanceRecordView, MutationContractError> {
    let job_id_str = row.job_id.unwrap_or_default();
    Ok(AcceptanceRecordView {
        job_id: JobId::new(job_id_str).map_err(|e| MutationContractError::InvalidInput {
            field: "job_id",
            reason: e.to_string(),
        })?,
        correlation_id: CorrelationId::new(row.correlation_id).map_err(|e| {
            MutationContractError::InvalidInput {
                field: "correlation_id",
                reason: e.to_string(),
            }
        })?,
        accepted_at_ms: row.accepted_at_ms,
        request_digest: RequestDigest::try_from_str(row.request_digest)?,
    })
}

/// A small, hand-built JSON payload (this crate already depends on
/// `serde_json` — see `Cargo.toml` — an already-pinned workspace
/// dependency, used here to safely encode field values that might
/// otherwise need manual escaping). Opaque to every downstream consumer of
/// the outbox (`loki_storage::outbox`'s own doc comment: "no delivery/
/// transport logic").
fn mutation_accepted_payload_json(
    request: &MutationAcceptanceRequest,
    job_id: &JobId,
    accepted_at_ms: i64,
) -> String {
    serde_json::json!({
        "method": request.method.as_str(),
        "principal_sid": request.principal_sid,
        "target_id": request.target_id,
        "artifact_digest": request.artifact_digest,
        "job_id": job_id.as_str(),
        "idempotency_key": request.idempotency_key.as_str(),
        "request_digest": request.request_digest.as_str(),
        "accepted_at_ms": accepted_at_ms,
    })
    .to_string()
}
