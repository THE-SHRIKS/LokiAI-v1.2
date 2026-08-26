# Task Packet — `P1-MUTATION-CONTRACT-001 Runtime Mutation Contract Types`

Packet status: `accepted`
Graph status: `ready`
Created by: `orchestrator (Claude Code), shaped by loki-planner`
Created at: `2026-08-25T00:00:00Z`
Starting revision: `7a1115f5b291c71c4ffb0e13deee8d1af9654268`

## 1. Objective

Give ADR-0006's transactional runtime mutation acceptance decision a closed,
reviewable internal Rust type and pure-function contract — grant generations,
request digests, idempotency keys, the closed mutation-method vocabulary, and
discriminated acceptance/revocation states — with zero I/O, zero database
access, and zero executable-callback surface (no `dyn Fn`, no trait method
that performs a side effect, no `AtomicBool`/mutex-as-authorization). This is
the interface `P1-ACCEPTANCE-STORE-001` implements against; it does not
implement the transaction itself.

## 2. Traceability

- Requirements: `NFR-SEC-002`, `NFR-SAFE-001`, `NFR-REL-001`
- Acceptance scenarios: `AS-006`–`AS-010`, `AS-015-L`
- Architecture components: runtime-owned durable acceptance boundary (ADR-0006
  "Decision"); `Docs/ARCHITECTURE.md` §8 Persistence (repository-pattern
  convention this crate follows)
- Security controls: credential revalidation, approval single-use consumption,
  idempotency-key replay/conflict handling, fail-closed revocation
  (`Docs/SECURITY.md`)
- Parent/feature/incident: `P1-MUTATION-SEM-ADR-001` (accepted); ADR-0006

## 3. Dependencies and inputs

- `depends_on`: `P1-MUTATION-SEM-ADR-001` (accepted 2026-08-25)
- Consumed contracts and exact versions/revisions:
  - ADR-0006 — accepted revision at
    `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
  - `loki_storage::states::JobState`, `loki_storage::ids::{JobId,
    CorrelationId, MessageId, CausationId}`, `loki_storage::StorageError`
    pattern — current working-tree revision of `crates/loki-storage/src`
- Required environment/credential/device: none. Pure Rust type/unit-test work.
- Known starting conditions:
  - No "grant generation" type exists anywhere in the repo today —
    `crates/loki-local-authority/src/model.rs`'s `LocalSession` is a
    different, shorter-lived, per-IPC-session concept (see Section 12 stop
    conditions and the handoff's required "open questions" record — do not
    treat `LocalSession` as equivalent).
  - `crates/loki-approvals`, `crates/loki-agents`,
    `crates/loki-agent-delivery`, and `crates/loki-local-authority` each open
    an independent `rusqlite::Connection`/encrypted file via
    `loki_storage::open_or_migrate_sqlcipher`; only jobs/outbox share one
    `loki_storage::Storage`. ADR-0006's one-transaction requirement is
    therefore not satisfiable against today's connection topology — this
    node's types must be connection-agnostic (pure data + pure functions) so
    they do not assume or paper over that gap; resolving the gap is
    `P1-ACCEPTANCE-STORE-001`'s scope, and its feasibility must reach the
    human/security owner before that node is dispatched (see Section 9 item 4
    and the orchestrator's status record for this packet).

## 4. Required reading

- `AGENTS.md`
- `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
- `Docs/graph/tasks/P1-MUTATION-SEM-ADR-001.md` and
  `Docs/graph/handoffs/P1-MUTATION-SEM-ADR-001-1.md`
- `crates/loki-approvals/src/lib.rs` (canonical digest and closed-policy
  patterns to generalize, not duplicate)
- `crates/loki-storage/src/{ids.rs,states.rs,error.rs}` (reuse these types
  directly; do not redefine `JobId`/`JobState`/error conventions)
- `crates/loki-local-authority/src/model.rs` (why `LocalSession` is not
  "grant generation")
- `crates/loki-ipc/src/protocol.rs` (closed-enum/no-generic-invoke wire
  convention this internal contract should mirror in spirit)
- `Docs/SPEC.md` Section 1 (`Envelope<T>`, `Error`, `OperationResult<T>`) and
  Section 4.7 (durable job) for provenance citations only — this crate does
  not depend on `packages/contracts`

## 5. Ownership

Writable paths:

- `crates/loki-mutation-contract/**` (new crate — create it)
- `Cargo.toml` (root workspace manifest) — exactly one added line registering
  `"crates/loki-mutation-contract",` in `[workspace] members`; no other edit
- `Docs/graph/handoffs/P1-MUTATION-CONTRACT-001-1.md` (own handoff only)

Read-only dependency paths:

- `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
- `crates/loki-approvals/src/**`, `crates/loki-jobs/src/**`,
  `crates/loki-agents/src/**`, `crates/loki-agent-delivery/src/**`,
  `crates/loki-local-authority/src/**`, `crates/loki-storage/src/**`,
  `crates/loki-ipc/src/**`, `packages/contracts/src/**`

Forbidden scope:

- No edit to any file inside `crates/loki-approvals`, `crates/loki-jobs`,
  `crates/loki-agents`, `crates/loki-agent-delivery`,
  `crates/loki-local-authority`, `crates/loki-storage`, or `crates/loki-ipc`.
  Wiring this contract into those crates is `P1-ACCEPTANCE-STORE-001`'s scope.
- No SQL, no `rusqlite`, no filesystem/network I/O, no `async`, in the new
  crate. This is a pure value-type + pure-function library.
- No trait with a side-effecting or injectable method (`dyn Fn`, `Box<dyn
  Trait>` used as a callback, `Arc<Mutex<...>>` as an authorization/commit
  boundary) — ADR-0006 "Rejected alternatives" explicitly forbids the
  callback/lease pattern this rule exists to prevent.
- No RPC/wire schema change (`P1-RPC-CONTRACT-001-R1`'s scope) and no
  dependency on `packages/contracts`.
- No workspace/tenant/cloud modeling beyond what ADR-0002 already defers —
  do not invent a `WorkspaceId`/tenant concept to satisfy ADR-0006's
  "binds workspace" clause; see Section 12 stop conditions.
- Do not silently resolve any of the five open questions this packet names in
  Section 9 ("Known open questions — do not resolve silently"). Record them
  in the handoff as still-open; do not encode one option as fact in a doc
  comment or type name.

## 6. Deliverables

1. New crate `crates/loki-mutation-contract` registered in root `Cargo.toml`.
2. Closed types, reusing `loki_storage::ids`/`states`/error conventions
   verbatim where they already cover the concept (do not redefine `JobId` or
   `JobState`):
   - `GrantGeneration` — opaque principal/grant identity + generation number +
     issued/expiry/revoked timestamps, explicitly documented as distinct from
     `loki_local_authority::LocalSession` (cite why in a doc comment, per
     Section 9 item 3 — document the distinction, not a resolution of how
     they will later relate).
   - `RequestDigest` — a canonical, order-stable SHA-256 digest type plus a
     pure function generalizing `loki_approvals::ApprovalRequest::digest()`'s
     length-prefixed field-concatenation pattern over an explicit, closed
     field list.
   - `IdempotencyKey` — bounded, validated newtype (reuse the
     `loki-approvals`/`loki-agents` `validate()` shape/bounds convention).
   - `MutationMethod` — closed enum covering the two ADR-0006-named
     categories: approval-gated device operations (mirroring
     `loki_approvals::evaluate_operation`'s existing closed operation set:
     `Restart`, `Flash`, `ApplyConfig`, `ContainedBuild`) and
     approval-not-required durable mutations (`AgentRunRegister`,
     `AgentRunUpdate`, `WorkIntentAcknowledge`, per ADR-0006
     "Consequences"). No open/string variant.
   - `MutationAcceptanceRequest` — the envelope binding principal, grant
     generation, method, target, artifact digest (optional), full canonical
     request digest, idempotency key, and an `Option<ApprovalReference>`
     (populated iff the method's policy tier requires it).
   - `AcceptanceResult` — closed discriminated enum:
     `Accepted { job_id: JobId, correlation_id: CorrelationId, accepted_at }`,
     `ApprovalRequired { approval_id, bound_request_digest, expires_at }`,
     `ReplayedIdenticalAccepted { <same fields as original Accepted> }`
     (same-key/same-digest), `Rejected(MutationRejection)` where
     `MutationRejection` is itself closed
     (`DigestConflict`/`GrantInvalid`/`GrantExpired`/`GrantRevoked`/
     `PolicyDenied`/`ApprovalNotConsumable`/`InvalidInput{..}`). No variant
     combination may carry contradictory/nullable fields (ADR-0006 "Mutation
     results become discriminated states with no contradictory nullable
     combinations").
   - `RevocationDisposition` — closed enum distinguishing
     `DeniesAcceptance` (pre-commit) from
     `RequiresPostCommitReconciliation` (post-commit; references
     `loki_storage::states::JobState::OutcomeUnknown`/`Cancelled` rather than
     redefining physical-outcome states) — deliberately does not invent a
     generic "safe point" mechanism; documents that as an open extension
     point per Section 9 item 1.
   - `MutationContractError` — this crate's own honest, panic-free error type
     (construction-time validation failures only — this crate performs no
     I/O so it has no `Sqlite`/`Io` variants).
3. Pure functions only, no trait objects: e.g.
   `compute_request_digest(...) -> RequestDigest`,
   `evaluate_replay(existing: &AcceptanceRecordView, incoming:
   &MutationAcceptanceRequest) -> ReplayDecision`,
   `evaluate_grant(generation: &GrantGeneration, now_ms: i64) ->
   Result<(), MutationRejection>`. `P1-ACCEPTANCE-STORE-001` calls these from
   inside its real transaction; this crate never opens a connection.
4. Unit tests (see Section 8) covering every named adversarial case.
5. Doc comments citing ADR-0006 (by section), `Docs/SPEC.md` Section 1/4.7,
   and this packet, matching the sibling crates' citation convention.
6. Handoff recording the five open questions (Section 9) as explicitly
   unresolved, plus which future node each blocks.

## 7. Acceptance criteria

- [ ] `cargo build -p loki-mutation-contract` succeeds with zero warnings.
- [ ] `cargo test -p loki-mutation-contract` passes, including every
      adversarial case named in Section 8.
- [ ] The new crate's `Cargo.toml` dependency list contains no I/O crate
      (`rusqlite`, `tokio`, `std::net`/`std::fs` use) and depends on
      `loki-storage` only for its `ids`/`states`/error-pattern reuse, not its
      `storage`/`managed`/`key`/`secret` modules.
- [ ] No `dyn Fn`, `Box<dyn ...>`-as-callback, or `Arc<Mutex<...>>`-as-
      authorization-boundary pattern exists anywhere in the new crate
      (grep-verifiable).
- [ ] Every closed enum this crate defines has at least one internal
      exhaustiveness-proof `match` (no wildcard arm) demonstrating it cannot
      silently accept an unlisted variant.
- [ ] `git diff --stat` shows changes only under `crates/loki-mutation-
      contract/`, one added line in root `Cargo.toml`, and
      `Docs/graph/handoffs/P1-MUTATION-CONTRACT-001-1.md`.
- [ ] The handoff's "Decisions and assumptions" section records all five
      Section 9 open questions as unresolved, each with the exact type/
      doc-comment location where the contract deliberately left an extension
      point instead of deciding.
- [ ] Failure/unavailable behavior is explicit and tested.
- [ ] No prohibited security or architecture boundary is weakened.
- [ ] No unrelated files change.

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `cargo fmt -p loki-mutation-contract -- --check` | exit 0 | command output |
| Static | `cargo clippy -p loki-mutation-contract -- -D warnings` | exit 0 | command output |
| Unit | `cargo test -p loki-mutation-contract` | all pass, including: `same_key_same_digest_replays_original_accepted_result`; `same_key_different_digest_is_rejected_as_digest_conflict`; `malformed_grant_generation_is_rejected` (empty/oversized/control-char principal or generation fields); `revoked_grant_before_evaluation_time_denies_acceptance`; `expired_grant_denies_acceptance`; `approval_required_method_cannot_construct_accepted_without_approval_reference`; `unknown_mutation_method_string_is_not_constructible` (closed-enum proof); `acceptance_result_variants_carry_no_contradictory_fields` | run summary / test names |
| Contract | n/a — pure internal Rust contract; no wire schema yet (future `P1-RPC-CONTRACT-001-R1` consumes these types) | n/a | n/a |
| Integration | n/a — no I/O in this crate | n/a | n/a |
| Desktop | n/a | n/a | n/a |
| Cloud | n/a | n/a | n/a |
| Containment | n/a | n/a | n/a |
| Hardware | n/a | n/a | n/a |
| Security | manual/grep review: no `dyn Fn`/`Box<dyn ...>`-callback/`Arc<Mutex<...>>`-as-authorization pattern present; every closed enum has an exhaustiveness-proof test | no finding | reviewer note in handoff |

## 9. Known open questions — do not resolve silently

1. **"Declared safe point" for cooperative post-commit cancellation.**
   ADR-0006 says revocation-after-commit "requests cancellation at declared
   safe points for active work" but does not define who declares a safe
   point or how it is represented generically across mutation kinds. This
   packet requires `RevocationDisposition` to expose this as a documented
   open extension point (referencing existing
   `loki_storage::states::JobState`/`loki-jobs`'s own cooperative-cancel
   pattern in `crates/loki-jobs/src/engine.rs`'s
   `request_cancel`/`is_cancel_requested`/`acknowledge_cancelled` as prior
   art), not a generic mechanism this node invents.
2. **`outcome_unknown` detection contract.** ADR-0006 states the *policy*
   ("an irreversible or uncertain physical result is recorded as
   `outcome_unknown`/quarantined") but not the exact trigger contract (what
   counts as "uncertain" vs. "clearly failed"). This node reuses
   `JobState::OutcomeUnknown` as the closed *state* but must not invent
   detection heuristics — flag for `P1-ACCEPTANCE-STORE-001`/
   `P1-MUTATION-REV-001`.
3. **`GrantGeneration` vs. `LocalSession`.** No code today models a
   credential epoch that survives across multiple IPC sessions of the same
   principal (which is what ADR-0006's "grant generation" implies).
   `LocalSession` is per-IPC-session. This node defines `GrantGeneration` as
   a new, distinct type and does not assume how (or whether) it will later
   be derived from or replace `LocalSession` — that reconciliation is
   `P1-PRINCIPAL-GRANT-001`'s named scope.
4. **Cross-crate single-transaction feasibility.** `loki-approvals`,
   `loki-agents`, `loki-agent-delivery`, and `loki-local-authority` each hold
   an independent SQLite connection/file; only `loki-jobs`/outbox share one
   `loki_storage::Storage`, whose `transaction()` is `pub(crate)`-only. ADR-
   0006's one-transaction requirement cannot be satisfied against this
   topology without consolidation work in `P1-ACCEPTANCE-STORE-001`. This
   node's types are deliberately connection-agnostic (pure data/functions)
   so they do not assume a topology that does not yet exist. **This is a
   feasibility risk to ADR-0006 itself, not just an implementation detail —
   flagged to the human/security owner before `P1-ACCEPTANCE-STORE-001` is
   dispatched.**
5. **"Binds workspace" with no workspace concept in Part One.**
   ADR-0006 step 2 requires binding "workspace" (and "scopes") into the
   digest/transaction. `crates/loki-local-authority` confirms workspace/
   tenant is a *cloud* concept (`CloudCapability::Workspace`, which fails
   closed/`CloudUnavailable`) and is explicitly deferred under ADR-0002 for
   all of Part One. There is currently no local workspace identifier to
   bind. This is a live tension between ADR-0006's decision text and the
   accepted Part One cloud deferral. This node does **not** invent a
   placeholder/local workspace ID to paper over the gap — it must be raised
   to the human/security owner via the handoff, not silently decided either
   way (e.g. by omitting the field, which would itself be a silent
   resolution).

## 10. Risks and safety

- Risk level: `medium` (no I/O, no host/device effect, but an incorrect
  closed-set/enum shape here becomes load-bearing for every downstream
  mutation-boundary node — `P1-ACCEPTANCE-STORE-001` through
  `P1-AGENT-MUTATION-IDEMP-001` and `P1-MUTATION-REV-001` all build on it).
- Failure modes: an under-specified enum forces a later node to either widen
  this contract (churn) or work around it with an untyped escape hatch
  (exactly what ADR-0006 forbids).
- Rollback/recovery: revert the new crate directory and the one `Cargo.toml`
  line; no persisted state exists to roll back.
- Data/secret impact: none — no secret material, no database.
- Executable/toolchain impact: none.
- Cloud/offline impact: none directly, but Section 9 item 5 is a live
  cloud-deferral tension that must reach the human/security owner.
- Device impact: none.

## 11. Human gate

- Gate: none required to *start* or *complete* this node's own acceptance
  criteria (pure type/test work; no host/device/cloud effect).
- Exact decision/operation requiring approval: none for this node. However,
  the orchestrator must separately route Section 9 items 4 and 5 (transaction
  topology feasibility; workspace-binding tension with ADR-0002) to the human
  product/security owner before `P1-ACCEPTANCE-STORE-001` is dispatched — not
  before this node, which does not depend on their resolution.
- Approver role: human product/security owner (for the open questions,
  separately from this node's completion).
- Approval evidence: n/a for this packet.

## 12. Stop conditions

Return `BLOCKED_DEPENDENCY` if ADR-0006 is reverted to `proposed` before this
node completes. Return `NEEDS_REPLAN` if satisfying ADR-0006's closed-state
requirement (no contradictory nullable combinations) turns out to require
touching an existing crate's public API — that means the contract and its
consumers were decomposed wrong and this node's boundary needs to move, not be
worked around. Return `NEEDS_REPLAN` rather than inventing a local
workspace/tenant identifier if Section 9 item 5 cannot be deferred cleanly.

## 13. Worker and reviewer

- Accountable worker: `loki-implementer` (one bounded node, `crates/loki-
  mutation-contract` only)
- Independent reviewer: `loki-security-reviewer` (checks for reintroduced
  callback/lease/`AtomicBool` patterns per ADR-0006 "Rejected alternatives")
  and `loki-test-reviewer` (checks the adversarial-case matrix is actually
  exercised, not merely named)
- Worktree/branch: `node/P1-MUTATION-CONTRACT-001`
- Handoff record: `Docs/graph/handoffs/P1-MUTATION-CONTRACT-001-1.md`

## 14. Human direction on Section 9 items 4-5 (2026-08-25, chat)

Recorded here so it is not lost to chat history; not yet a formal ADR revision
— that should still happen before `P1-ACCEPTANCE-STORE-001` is dispatched.

- **Item 4 (transaction topology):** human confirmed the target architecture
  is one shared local database with real transactions (single-machine
  desktop app), not a distributed/multi-database design. Direction only —
  the actual consolidation of `loki-approvals`, `loki-agents`,
  `loki-agent-delivery`, and `loki-local-authority` onto one shared
  `loki-storage`-style connection remains unbuilt and is
  `P1-ACCEPTANCE-STORE-001`'s scope.
- **Item 5 (workspace binding):** human approved dropping the "workspace"
  field entirely for Part One rather than inventing a placeholder value,
  explicitly by analogy to how `LDEV-001`/`ENR-001` already split local
  device identity (now) from cloud ownership (later, Part Two): mutation
  records carry no workspace field locally; a future cloud-sync step
  (`CLD-002`'s outbox mechanism) attaches the real workspace retroactively
  when cloud/login exists. This should be reflected as a short amendment to
  ADR-0006 itself (its "Decision" step 2 currently still says "bind
  workspace") before `P1-ACCEPTANCE-STORE-001` treats it as settled — flagged
  here, not yet done.

## 15. Planning provenance

Shaped by `loki-planner` background analysis, 2026-08-25, against working-tree
revision `7a1115f5b291c71c4ffb0e13deee8d1af9654268`. Files it read are listed
in its report; not reproduced here. Full report retained in this session's
transcript; a durable copy should be filed under
`Docs/graph/evidence/W6/P1-MUTATION-CONTRACT-001/planning-report.md` before
this node is treated as fully traceable without the chat transcript.
