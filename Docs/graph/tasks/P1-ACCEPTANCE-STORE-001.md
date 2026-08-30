# Task Packet — `P1-ACCEPTANCE-STORE-001` One-Transaction Mutation Acceptance Engine

Packet status: `proposed` (ready for orchestrator review/dispatch)
Graph status: `ready` once the human waiver below and this packet are accepted
Created by: `orchestrator (Claude Code), shaped by loki-planner`
Created at: `2026-08-28T00:00:00Z`
Starting revision: current tip of branch `LokiAI-v1.2` (post `a06cd5d`)

## 1. Objective

Implement ADR-0006's single encrypted-SQLite-transaction mutation acceptance
boundary for real, wired to `loki-mutation-contract`'s pure types
(`GrantGeneration`/`evaluate_grant`, `MutationAcceptanceRequest`,
`construct_accepted`/`evaluate_replay`/`evaluate_approval_reference`,
`AcceptanceResult`, `RevocationDisposition`) — and, as required scope for
this node (not a separate one), fix the underlying database topology so a
real cross-crate atomic transaction is even possible: `loki-approvals`,
`loki-agents`, `loki-agent-delivery`, and `loki-local-authority` each
currently open an independent SQLite connection
(`loki_storage::open_or_migrate_sqlcipher`) instead of sharing the one
`loki_storage::Storage` that `jobs`/`outbox` already use.

This node's transaction, inside one `Storage::transaction()` call, must in
order:

1. revalidate the principal's current `GrantGeneration` (via
   `evaluate_grant`);
2. bind method/digest/idempotency key/target/artifact/approval inputs (no
   workspace/scopes — ADR-0006 Amendment 1);
3. claim or replay the idempotency record (`evaluate_replay`/
   `replayed_result`), rejecting same-key/different-digest;
4. validate and consume the exact approval when required
   (`evaluate_approval_reference`, invoked through `construct_accepted`);
5. create or retrieve the durable job/mutation record;
6. append the transactional outbox record;
7. commit before returning any `AcceptanceResult`.

No connector/device/build/deploy effect runs inside the transaction.
`accepted_at_ms` is read from a trusted clock **inside** the transaction
closure, immediately before calling `construct_accepted` — never
client-supplied, never read before lock acquisition (closes the
time-of-check/time-of-use gap already flagged in `TASK_GRAPH.md`'s
`P1-MUTATION-CONTRACT-001` acceptance record).

## 2. Traceability

- Requirements: `NFR-SEC-002`, `NFR-SAFE-001`, `NFR-REL-001`
- Acceptance scenarios: `AS-006`–`AS-010`, `AS-015-L`
- Architecture: `Docs/ARCHITECTURE.md` §8 Persistence (repository-pattern
  convention this node extends); ADR-0001 "core storage" boundary (this node
  touches only that file, never monitoring's or incidents' separate
  encrypted files)
- Security: credential revalidation, single-use approval consumption,
  idempotency-key replay/conflict handling, fail-closed revocation
  (`Docs/SECURITY.md`)
- Parent/decisions: `P1-MUTATION-SEM-ADR-001` (ADR-0006, accepted);
  `P1-MUTATION-CONTRACT-001` (accepted, `crates/loki-mutation-contract`)

## 3. Dependencies and inputs

- `depends_on`: `P1-MUTATION-CONTRACT-001` (accepted — confirmed in
  `Docs/graph/TASK_GRAPH.md`), `LOC-001` (accepted), `AGT-001` (accepted),
  `APR-001` (accepted), `JOB-001` (accepted) — read-only consumers of their
  current public business APIs, per Section 5.
- `depends_on` (graph-recorded) `DAT-002-MEM-IMPL-001`: **explicitly waived
  for this dispatch by human decision, recorded here for audit.** The human
  product/security owner reviewed the risk that `DAT-002-MEM-CODE-001`
  (the secure-memory-arena implementation this node's `Storage`/SQLCipher
  path depends on) already passed its own implementation review and
  independent security review and is accepted/integrated
  (`Docs/graph/TASK_GRAPH.md`, commit `5c58938`); `DAT-002-MEM-HOST-VERIFY-001`
  is confirmatory real-host/VM evidence-gathering (page-lock attestation,
  quota-failure, exhaustion, churn/soak, second-identity DPAPI matrix) and
  is not expected to change any code path this node builds on.
  `DAT-002-MEM-IMPL-001` (which formally depends on `DAT-002-MEM-HOST-VERIFY-001`)
  therefore remains a separately gated node and is **not** a precondition for
  starting or completing this node. If `DAT-002-MEM-HOST-VERIFY-001` or
  `DAT-002-MEM-REV-001` later surfaces a finding that changes
  `secure_memory.rs`/`secure_profile.rs`/`managed.rs` production behavior,
  that finding routes back through this node's own re-verification before
  release, not silently absorbed. This waiver does not authorize skipping
  `DAT-002-MEM-IMPL-001` itself — it only unblocks this node's own start.
- Consumed contracts/exact revisions:
  - ADR-0006 (accepted, plus Amendment 1 — no workspace binding)
  - `crates/loki-mutation-contract` (accepted revision) — used exactly as
    designed; zero I/O added to that crate
  - `loki_storage::{ids, states::JobState, error::StorageError,
    storage::Storage, jobs::{JobRepository, NewJob, JobRecord},
    outbox::{OutboxRepository, NewOutboxMessage}}` at current working-tree
    revision
- Required environment/credential/device: none beyond normal build/test
  toolchain (Windows, MSVC, existing SQLCipher build already proven by
  `DAT-002-MEM-CODE-001`).
- Known starting conditions (confirmed by direct inspection, not assumed):
  - `loki-approvals::ApprovalStore`, `loki-agents::AgentRegistry`,
    `loki-agent-delivery::WorkIntentDeliveryStore`, and
    `loki-local-authority::LocalAuthority` each call
    `Connection::open_in_memory()`/`loki_storage::open_or_migrate_sqlcipher`
    directly and run their own `CREATE TABLE IF NOT EXISTS ...` in
    `from_connection`, wrapped in their own `Mutex<Connection>`. None of
    their schemas are registered in `loki_storage::migrations::MIGRATIONS`.
  - `Storage::transaction<T>` (`crates/loki-storage/src/storage.rs:209`) is
    `pub(crate)`, already correct (commit-on-success,
    rollback-on-`Err`/panic-via-Drop), and its own doc comment names this
    exact node as the one meant to make it a real cross-repository
    primitive.
  - `jobs.rs`'s `JobRepository`/`outbox.rs`'s `OutboxRepository` trait
    methods call `self.lock()` internally — **calling either from inside a
    `Storage::transaction()` closure deadlocks today** (non-reentrant
    `std::sync::Mutex`, confirmed by direct code reading, not assumed). See
    Section 6 Deliverable 3.
  - `jobs.idempotency_key` already carries a `UNIQUE` index
    (`migrations/0002_jobs.sql`), owned by `JOB-001`. This node must not
    reuse that column/index for agent-mutation idempotency keys — see
    Section 6 Deliverable 5's `mutation_acceptances` table design.

## 4. Required reading

- `AGENTS.md`, `Docs/ARCHITECTURE.md` §8, `Docs/graph/decisions/ADR-0001-local-data-encryption.md`
- `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
  (including Amendment 1)
- `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md` §9 (all five open questions
  — items 1, 2, 4 are this node's to act on; item 3 is `P1-PRINCIPAL-GRANT-001`'s;
  item 5 is already settled by Amendment 1) and its handoffs
- `crates/loki-mutation-contract/src/**` (consume exactly as designed — do
  not modify)
- `crates/loki-storage/src/{storage.rs,jobs.rs,outbox.rs,migrations.rs,error.rs,ids.rs,states.rs,timestamp.rs}`
  and `migrations/000{1,2,3}_*.sql`
- `crates/loki-approvals/src/lib.rs`, `crates/loki-agents/src/lib.rs`,
  `crates/loki-agent-delivery/src/lib.rs`,
  `crates/loki-local-authority/src/{store.rs,model.rs}` (current schema and
  business-method shape to preserve)
- `claudeplanner.md` §2.3, §3 (already-established reasoning this packet
  operationalizes)

## 5. Ownership

Writable paths:

- `crates/loki-storage/src/approvals.rs` (new), `agents.rs` (new),
  `agent_delivery.rs` (new), `local_authority.rs` (new) — repository trait
  modules mirroring `jobs.rs` exactly (trait + `impl X for Storage` +
  connection-parameterized free-function core, per Deliverable 3).
- `crates/loki-storage/src/mutation_acceptance.rs` (new) — the
  `mutation_acceptances` repository (Deliverable 5).
- `crates/loki-storage/src/{jobs.rs,outbox.rs}` — **narrow, additive-only**
  edit: extract each trait method's SQL into a private
  `fn write_x(conn: &rusqlite::Connection, ...)`-shaped helper the existing
  trait method calls via `self.lock()`, with no behavior change to any
  existing public method signature or SQL. This is the deadlock fix
  (Deliverable 3), not a schema or API change.
- `crates/loki-storage/src/storage.rs` — widen `transaction()` from
  `pub(crate)` to `pub`; no other change to this file.
- `crates/loki-storage/src/lib.rs` — add the new modules'
  `pub mod`/re-exports, mirroring existing ones.
- `crates/loki-storage/migrations/0004_approvals.sql`,
  `0005_agents.sql`, `0006_agent_delivery_and_local_authority.sql`,
  `0007_mutation_acceptances.sql` (new) — append-only to
  `migrations::MIGRATIONS`; never edit `0001`–`0003`.
- `crates/loki-approvals/src/lib.rs`, `crates/loki-agents/src/lib.rs`,
  `crates/loki-agent-delivery/src/lib.rs`,
  `crates/loki-local-authority/src/store.rs` — **constructor and internal
  storage-access changes only**: replace `Connection::open_in_memory()`/
  `loki_storage::open_or_migrate_sqlcipher` + `Mutex<Connection>` +
  in-crate `CREATE TABLE` with a constructor taking `&loki_storage::Storage`
  and business methods calling the new `loki-storage` repository traits.
  Every existing public business method name/signature/error type/behavior
  must be preserved (only how it reaches the database changes) — this is a
  migration node, not a redesign of these crates' APIs. Existing unit tests
  in these four crates must be updated to construct via `Storage` instead of
  `open_in_memory()`, but must keep asserting the same behavior.
- New crate `crates/loki-acceptance-store/**` — the actual ADR-0006
  transaction engine: depends on `loki-storage`, `loki-mutation-contract`,
  `loki-approvals`, `loki-agents`, `loki-agent-delivery`,
  `loki-local-authority`, and `rusqlite` (same pinned version as siblings,
  needed because the transaction closure operates on
  `&rusqlite::Transaction` directly — this crate, not `loki-mutation-contract`,
  is where I/O and transaction shape are allowed to appear).
- Root `Cargo.toml` — exactly one added line registering
  `"crates/loki-acceptance-store",` in `[workspace] members`; no other edit.
- `Docs/graph/handoffs/P1-ACCEPTANCE-STORE-001-1.md` (own handoff only).

Read-only dependency paths:

- `crates/loki-mutation-contract/src/**` (consume; do not edit)
- `crates/loki-jobs/src/**` (read to confirm no collision with the new
  `mutation_acceptances` table or the `jobs.rs` helper extraction)
- `Docs/graph/decisions/**`, `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md`

Forbidden scope:

- No edit to `crates/loki-mutation-contract/**` (pure contract crate,
  already accepted — if this node finds its shape insufficient, that is
  `NEEDS_REPLAN`, not a silent patch here).
- No edit to `crates/loki-storage/migrations/000{1,2,3}_*.sql` or any
  existing `jobs`/`outbox`/`devices`/`inbox` column/index (only additive new
  tables/migrations).
- No edit to `crates/loki-jobs/**` (JOB-001's own lease/execution engine —
  out of this node's scope; this node only inserts/retrieves `jobs` rows
  through the existing `JobRepository` trait, never touches lease/execution
  logic).
- No edit to monitoring's or incidents' separate encrypted database files or
  crates (`loki-monitoring`, `loki-incidents`) — ADR-0001's three-file
  boundary stays exactly as accepted; this node's consolidation is scoped to
  the "core storage" file only.
- No workspace/tenant/scopes field invented anywhere (ADR-0006 Amendment 1
  is settled — do not reopen it).
- No `dyn Fn`/`Box<dyn Trait>`-as-callback/`Arc<Mutex<...>>`-as-authorization
  pattern anywhere in `loki-acceptance-store` (ADR-0006 "Rejected
  alternatives").
- No connector/device/build/deploy/network effect inside
  `Storage::transaction()`'s closure, or anywhere in this crate at all
  (ADR-0006: "No connector, device, build, deployment, or other external
  effect runs inside that transaction").
- No RPC/wire schema work (`P1-RPC-CONTRACT-001-R1`'s scope).
- Do not silently resolve the `outcome_unknown` detection trigger contract
  (`P1-MUTATION-CONTRACT-001` §9 item 2) or the "declared safe point"
  mechanism (§9 item 1) — this node may *use* `RevocationDisposition` and
  `JobState::OutcomeUnknown` as already defined, but must not invent the
  missing detection/trigger logic; record as still open in the handoff.
- Do not decide `GrantGeneration` vs. `LocalSession` reconciliation (§9 item
  3 — `P1-PRINCIPAL-GRANT-001`'s named scope). This node's transaction
  revalidates whatever `GrantGeneration` row already exists at the time it
  runs (sourced via a read this node defines against the new consolidated
  storage), but does not decide how a `GrantGeneration` is issued, rotated,
  or reconciled with `LocalSession`.

## 6. Deliverables

1. **Migration-owned schema for the four consolidated crates**
   (`0004`–`0006`), each mirroring the exact existing `CREATE TABLE`
   statements found in `loki-approvals`/`loki-agents`/`loki-agent-delivery`/
   `loki-local-authority`'s current `from_connection` (column-for-column;
   this is a mechanical schema move, not a redesign), registered in
   `migrations::MIGRATIONS` in that order after `0003`.
2. **Repository trait modules** (`approvals.rs`, `agents.rs`,
   `agent_delivery.rs`, `local_authority.rs`) in `loki-storage`, each:
   - a public trait over that bounded context's rows (`ApprovalsRepository`,
     `AgentsRepository`, `AgentDeliveryRepository`, `LocalAuthorityRepository`
     — exact names at implementer's discretion, mirroring `JobRepository`'s
     shape/naming convention);
   - `impl X for Storage`, using `self.lock()` for the standalone entry
     points (same pattern as `jobs.rs`);
   - a private connection-parameterized core (`fn insert_x(conn: &Connection, ...)`)
     that both the `impl X for Storage` methods and this node's transaction
     closure call directly — this is the deadlock fix from Section 3,
     applied uniformly to the four new modules plus the two edited existing
     ones (`jobs.rs`, `outbox.rs`).
3. **`jobs.rs`/`outbox.rs` refactor** (no behavior change): extract each
   trait method's SQL execution into the same connection-parameterized
   private-function shape as Deliverable 2, called via `self.lock()` from
   the existing trait impl. Prove no behavior change via the existing test
   suite passing unmodified.
4. **`Storage::transaction()` made `pub`**, with an updated doc comment
   naming this node (replacing the current "left to the node that has a
   concrete multi-repository atomic write to build" placeholder) and stating
   the concurrency contract explicitly: callers must never call a
   `self.lock()`-based repository method from inside the closure (deadlock);
   only the connection-parameterized core functions.
5. **`mutation_acceptances` table and repository** (`mutation_acceptance.rs`,
   migration `0007`): the canonical ADR-0006 step 2/3 record — principal
   SID, grant generation, method, target, artifact digest, full request
   digest, idempotency key (`UNIQUE`, independent of `jobs.idempotency_key`),
   job_id (nullable — see below), correlation_id, accepted_at_ms. A read by
   idempotency key returns an `AcceptanceRecordView`-shaped row for
   `evaluate_replay`. `job_id` is populated only for methods that create a
   real `jobs` row (approval-gated device operations, via `JobRepository`);
   agent methods (`AgentRunRegister`/`AgentRunUpdate`/`WorkIntentAcknowledge`)
   populate a synthesized stable identifier through the same `JobId` shape
   (this node decides the exact minting scheme, e.g. a UUID prefixed
   consistently with existing conventions in `loki-approvals`/`loki-agents`)
   without a corresponding `jobs` table row, and documents this distinction
   in a doc comment citing this packet.
6. **`loki-acceptance-store` crate**: the real ADR-0006 seven-step
   transaction, taking a `&Storage` plus a `MutationAcceptanceRequest`,
   returning `loki_mutation_contract::AcceptanceResult`. Internally:
   - reads the current `GrantGeneration` for the principal (this node
     defines the minimal read query against whatever the current
     `LocalAuthority`/`AgentRegistry` session/credential model exposes,
     without deciding the full `GrantGeneration`-vs-`LocalSession`
     reconciliation — see forbidden scope);
   - calls `evaluate_grant`;
   - looks up `mutation_acceptances` by idempotency key; if found, calls
     `evaluate_replay`/`replayed_result`;
   - if not found, calls `construct_accepted` (which internally validates/
     consumes the approval reference via `evaluate_approval_reference`);
   - on `Accepted`, atomically: inserts the `jobs` row (approval-gated
     methods) or synthesizes the agent-method identifier, consumes the
     approval row (via the consolidated `approvals.rs` repository, in the
     same transaction — no second commit), appends the outbox record, and
     inserts the `mutation_acceptances` row — all through the
     connection-parameterized core functions inside one
     `Storage::transaction()` call;
   - reads `accepted_at_ms` from a clock **inside** the closure (hand-rolled
     Unix-epoch-millis read local to this crate — see Section 9 item 1 for
     why this deliberately does not import `loki-runtime`'s clock helper).
7. **Fault-injection and race test harness** against a real file-backed
   `Storage` (not in-memory-only): fault injected after each of the seven
   steps leaves zero orphan rows across `mutation_acceptances`/`jobs`/
   `local_approvals`/outbox; concurrent submission of the same idempotency
   key from two threads yields exactly one committed acceptance; a
   real-transaction rollback (panic inside the closure) leaves the shared
   connection usable for the next caller.
8. Doc comments citing ADR-0006 (by step number) and this packet, matching
   sibling-crate citation convention.
9. Handoff recording every open question this node does not resolve
   (Section 9 below) as still open, plus which future node each blocks.

## 7. Acceptance criteria

- [ ] `cargo build --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` succeed with zero warnings.
- [ ] `cargo test -p loki-storage -p loki-approvals -p loki-agents -p loki-agent-delivery -p loki-local-authority -p loki-acceptance-store -- --test-threads=1` passes, including every existing test in the four consolidated crates unmodified in intent (same assertions, updated only for the new construction path).
- [ ] `pnpm run verify` passes (one retry allowed on the documented flaky Windows Application Control error, per existing project convention).
- [ ] No existing public business method signature changed in `loki-approvals`, `loki-agents`, `loki-agent-delivery`, `loki-local-authority` except each store's constructor.
- [ ] `Storage::transaction()` is the only path `loki-acceptance-store` uses to reach the database for a mutation acceptance — grep-verifiable: no `Connection::open`/`open_or_migrate_sqlcipher` call anywhere in the new crate.
- [ ] Fault injection after each of the seven ADR-0006 steps (real file-backed SQLite, process-killed or panic-simulated mid-transaction) leaves zero orphan rows in `mutation_acceptances`, `jobs`, `local_approvals`, or `outbox_messages` — reproduced at least twice.
- [ ] Concurrent submission of the same idempotency key from two real OS threads against one `Storage` yields exactly one committed `mutation_acceptances` row and one `Accepted`/one `ReplayedIdenticalAccepted` result, never two `Accepted`s, never a panic.
- [ ] An approval expiring between request-receipt and lock-acquisition (simulated by delaying acquisition past `expires_at_ms`) is rejected, not accepted.
- [ ] A same-key/different-digest replay is rejected as `DigestConflict`, not silently accepted or silently returning the old result.
- [ ] Revocation racing acceptance yields exactly one legal `RevocationDisposition` outcome, reproduced under real thread interleaving, not just sequential unit assertions.
- [ ] No code path outside `loki-mutation-contract`'s own constructors (`construct_accepted`/`replayed_result`) can produce an `AcceptanceResult::Accepted`/`ReplayedIdenticalAccepted` — grep/review-verifiable (no struct-literal construction of `AcceptedRecord` is possible from outside `loki-mutation-contract` per its existing private-field design; confirm `loki-acceptance-store` never attempts to bypass it).
- [ ] `git diff --stat` shows changes only under the writable paths named in Section 5, one added line in root `Cargo.toml`, and the handoff.
- [ ] Independent test-reviewer and security-reviewer sign-off with no Critical/High finding (ADR-0006 "Required evidence": "Independent mutation-boundary ... reviews have no critical/high finding").
- [ ] Every open question in Section 9 is recorded as still-open in the handoff, not silently resolved.
- [ ] No prohibited security or architecture boundary weakened (ADR-0001 three-file boundary intact; `secure_memory`/`require_ready()` fail-closed gates untouched).

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `cargo fmt --all -- --check` | exit 0 | command output |
| Static | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | command output |
| Unit | `cargo test -p loki-storage --all-targets -- --test-threads=1` | all existing tests pass unmodified plus new repository-module tests | run summary |
| Unit | `cargo test -p loki-approvals -p loki-agents -p loki-agent-delivery -p loki-local-authority -- --test-threads=1` | all existing tests pass against the new `&Storage`-backed construction | run summary |
| Unit | `cargo test -p loki-acceptance-store` | all pass, including: `same_key_same_digest_replays`; `same_key_different_digest_rejected`; `expired_grant_denies_acceptance`; `revoked_grant_denies_acceptance`; `approval_required_without_reference_never_reaches_construct_accepted_bypassed`; `agent_method_accepts_without_jobs_row` | run summary / test names |
| Fault injection | real file-backed `Storage`, kill/panic simulated after each of the 7 ADR-0006 steps | zero orphan rows every time, reproduced ≥2 runs | test names + row-count assertions in output |
| Concurrency/race | two real OS threads, same idempotency key, one `Storage` | exactly one committed acceptance, no panic, no deadlock | test output, timing log |
| Integration | `cargo test --workspace -- --test-threads=1` | pass (documented flake class only) | run summary |
| Contract | n/a — no new wire schema (future `P1-RPC-CONTRACT-001-R1` consumes this crate) | n/a | n/a |
| Security | independent review: grep for `dyn Fn`/`Box<dyn ...>`-callback/`Arc<Mutex<...>>`-as-authorization; confirm `AcceptedRecord` never struct-literal-constructed outside `loki-mutation-contract`; confirm no connector/device effect inside the transaction closure | no finding | reviewer note in handoff |
| Desktop/Cloud/Hardware/Containment | n/a — no IPC, no cloud, no device, no build-executor surface in this node | n/a | n/a |

## 9. Known open questions — do not resolve silently

1. **Clock source deviates from `claudeplanner.md`'s recommendation, by
   necessity, not preference.** `claudeplanner.md` §2.3 recommends reusing
   `crates/loki-runtime/src/host/clock.rs`. Direct inspection shows
   `loki-runtime` depends only on `loki-storage`/`loki-ipc` today, and
   `P1-RUNTIME-COMP-001` (a later node) will make `loki-runtime` depend on
   this crate (and the other service crates) for composition — if this
   crate also depended on `loki-runtime` for its clock, that would be a
   real dependency cycle the moment `P1-RUNTIME-COMP-001` lands, not a
   theoretical one. This packet therefore requires a small,
   locally-owned Unix-epoch-millis clock read in `loki-acceptance-store`
   itself (same `SystemTime::now()` technique, not a shared crate — `AGENTS.md`
   rule 11 does not require avoiding a two-line duplication to prevent a
   structural cycle). Flagging this explicitly because it overrides a
   specific claudeplanner.md recommendation with a concrete, code-verified
   reason, not a silent deviation.
2. **`GrantGeneration` source of truth is still undefined.** This node must
   read *some* current `GrantGeneration` row to call `evaluate_grant`, but
   `P1-PRINCIPAL-GRANT-001` (a later, currently-`proposed` node) is where
   `GrantGeneration` becomes authoritative and reconciled with
   `LocalSession`/`AgentCredential`. This node should define the minimal
   read this transaction needs against whatever already exists
   (`LocalAuthority`'s session table, post-consolidation) as a narrow,
   explicitly-provisional shim, and the handoff must state plainly that
   `P1-PRINCIPAL-GRANT-001` may need to change this read's shape. Do not
   over-invest here.
3. **`outcome_unknown` trigger contract remains undefined** (§9 item 2 from
   `P1-MUTATION-CONTRACT-001`). This node's transaction never runs a
   physical effect, so it should never itself need to decide "uncertain vs.
   clearly failed" — but the handoff must confirm this node's scope
   genuinely never reaches that decision point (i.e., that nothing in the
   seven-step sequence can itself produce a physical/irreversible result),
   not merely assert it.
4. **Lock-contention/throughput risk** (`claudeplanner.md` §3.4): consolidating
   four connections behind one mutex is a deliberate ADR-0006 trade-off, but
   this node should record, not necessarily fix, any measured contention
   under its own concurrency test (Section 8) as a named risk for
   `P1-RUNTIME-COMP-001`/`P1-MUTATION-REV-001` to verify at realistic load.
5. **Agent-method `job_id` minting scheme** (Deliverable 5): this packet
   requires a decision (UUID-based synthesized identifier, consistent with
   existing `loki-approvals`/`loki-agents` ID-minting conventions) but
   leaves the exact prefix/format to the implementer — record the chosen
   scheme in the handoff so `P1-AGENT-MUTATION-IDEMP-001` does not have to
   reverse-engineer it.

## 10. Risks and safety

- Risk level: `critical` — this is the single highest-risk, most load-bearing
  node in the entire mutation-acceptance chain (per `claudeplanner.md` §2.3
  and §4.5's explicit recommendation for an independent security review
  checkpoint immediately after this node, ahead of the formal
  `P1-MUTATION-REV-001` review two waves later).
- Failure modes: (g) unsynchronized shared state — four independent
  single-writer stores becoming one multi-writer shared connection under a
  `Mutex`; (c) structurally-bypassable result types — a second, less-guarded
  path to committing a job/outbox row without going through
  `construct_accepted`/`evaluate_replay`; (f) silent success on
  crash-recovery — a fault after partial writes that looks committed but
  isn't, or vice versa.
- Rollback/recovery: `Storage::transaction()`'s existing
  commit-only-on-`Ok`/rollback-via-`Drop` semantics is the safety net;
  this node's own tests must independently prove it holds under the new
  multi-repository composition, not merely trust the existing
  single-repository tests.
- Data/secret impact: none beyond what the four consolidated crates already
  persist (approval reasons, agent scopes, session identifiers) — no new
  secret class introduced; SQLCipher-at-rest protection unchanged.
- Concurrency impact: real — see Section 9 item 4.
- Cloud/offline impact: none (no workspace binding, per Amendment 1).
- Device impact: none (no connector/device effect permitted inside the
  transaction).

## 11. Human gate

- Gate: none required to *start* this node — the `DAT-002-MEM-HOST-VERIFY-001`
  dependency is explicitly waived per Section 3 (recorded, auditable, not a
  silent skip).
- Gate required before this node's acceptance is treated as final: an
  independent security review with specific focus on (a) the
  deadlock-avoidance refactor (Deliverable 3) not accidentally reintroducing
  a lock-order or reentrancy bug, and (b) no code path outside
  `loki-mutation-contract`'s constructors can produce an `Accepted`/
  `ReplayedIdenticalAccepted` result. This mirrors ADR-0006's own "Required
  evidence" list, not a new requirement invented here.
- Approver role: human product/security owner, on the independent review's
  findings — same pattern already used for `DAT-002-MEM-CODE-001` and
  `P1-MUTATION-CONTRACT-001`.
- Approval evidence: independent review record under
  `Docs/graph/evidence/W6/P1-ACCEPTANCE-STORE-001/`.

## 12. Stop conditions

Return `BLOCKED_DEPENDENCY` if `P1-MUTATION-CONTRACT-001` is reverted from
`accepted`. Return `NEEDS_REPLAN` if the deadlock-avoidance refactor
(Deliverable 3) turns out to require changing `jobs.rs`/`outbox.rs`'s public
trait signatures (not just extracting a private helper) — that means the
existing repository pattern cannot support a real cross-repository
transaction as designed, which is exactly the kind of structural finding
that must return to planning rather than be worked around. Return
`NEEDS_REPLAN` rather than inventing a `GrantGeneration` source of truth if
Section 9 item 2's minimal read cannot be defined without effectively
pre-deciding `P1-PRINCIPAL-GRANT-001`'s scope. Return `NEEDS_REPLAN` rather
than reusing `jobs.idempotency_key` for agent methods if the implementer
finds the separate `mutation_acceptances` table (Deliverable 5) is
insufficient for some case this packet did not anticipate.

## 13. Worker and reviewer

- Accountable worker: `loki-implementer` (one bounded node; broad
  ownership set by necessity — this is the DB-topology-consolidation node —
  but every writable path is named exactly in Section 5, no adjacent
  subsystem).
- Independent reviewer: `loki-security-reviewer` (transaction-boundary,
  deadlock, and result-type-bypass focus per Section 11) and
  `loki-test-reviewer` (fault-injection/concurrency matrix actually
  exercised, not merely named).
- Worktree/branch: `node/P1-ACCEPTANCE-STORE-001`.
- Handoff record: `Docs/graph/handoffs/P1-ACCEPTANCE-STORE-001-1.md`.

## 14. Human direction and waivers on record for this node (2026-08-28)

- `DAT-002-MEM-HOST-VERIFY-001` dependency waived for this node's start —
  see Section 3. Confirmed by the human product/security owner in chat
  ("GO ... DONT STOP UNTIL PART1 ALL FEATURES AND BUILDING STUFF IS DONE ...
  second EXCLUDE SECURITY PART THAT VM AND ALL FROM IT ILL ALLOW YOU FOR
  THAT"), recorded verbatim in `Docs/graph/TASK_GRAPH.md`'s status-update
  log under the 2026-08-28 entry for this node.
- ADR-0006 Amendment 1 (no workspace binding) already accepted and applies
  unchanged here.
- The DB topology consolidation direction (one shared local database, real
  transactions) was already confirmed by the human in chat, recorded in
  `P1-MUTATION-CONTRACT-001.md` §14 — this packet operationalizes that
  direction; it does not reopen it.

## 15. Planning provenance

Shaped by `loki-planner` (read-only), 2026-08-28, against the working tree
at the head of `LokiAI-v1.2` (post `a06cd5d`). Verified by direct
inspection, not by trusting `claudeplanner.md`'s narrative alone: current
schema/constructor shape of `loki-approvals`, `loki-agents`,
`loki-agent-delivery`, `loki-local-authority`; `Storage::transaction()`'s
exact mutex/lock semantics; `jobs.idempotency_key`'s existing `UNIQUE`
index; `loki-runtime`'s current dependency set (confirming the clock-source
deviation in Section 9 item 1). Two findings not present in
`claudeplanner.md` at this resolution were surfaced and folded into
Deliverables 3 and 5: the `self.lock()`-inside-`transaction()` deadlock
hazard, and the `mutation_acceptances`-vs-`jobs.idempotency_key` key-space
collision risk.
