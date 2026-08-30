# Task Packet - P1-AGENT-MUTATION-IDEMP-001 finish_run Retry Fix and Crash-Safe Two-Call Agent-Mutation Composition (ADR-0008 Option B)

Packet status: `proposed` (ready for orchestrator review/dispatch)
Graph status: `ready` - ADR-0008 accepted (Option B, human decision 2026-08-28)
Created by: `orchestrator (Claude Code), shaped by loki-planner`
Created at: `2026-08-28T13:40:00Z`
Starting revision: **stacked on branch `node/P1-ACCEPTANCE-STORE-001`'s current
uncommitted tip** in worktree `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001` -- **not**
`LokiAI-v1.2`'s tip, and **not** a specific commit hash. Same stacking pattern as
`P1-PRINCIPAL-GRANT-001.md` and `P1-APPROVAL-JOB-ACCEPT-001.md`. See Section 0
for why no more precise starting point can be named today.

## 0. Coordination state - read before dispatching this node

Direct inspection of `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001` at the time of
this planning pass (`git status`/`git log` run against that worktree, not
assumed) shows **all three predecessor nodes' work
(`P1-ACCEPTANCE-STORE-001`, `P1-PRINCIPAL-GRANT-001`, and the code
`P1-APPROVAL-JOB-ACCEPT-001` would touch) are still uncommitted working-tree
changes on the single branch `node/P1-ACCEPTANCE-STORE-001`** - `git log`
shows the branch tip is still `a06cd5d` (the same commit `LokiAI-v1.2`'s own
tip is at); `git status` shows a large set of modified/untracked files
(`crates/loki-agents/src/lib.rs`, `crates/loki-storage/src/agents.rs`,
`crates/loki-storage/src/local_authority.rs`, `crates/loki-acceptance-store/**`,
four new migrations, etc.) with **no intermediate commit boundary separating
`P1-ACCEPTANCE-STORE-001`'s own deliverables from `P1-PRINCIPAL-GRANT-001`'s**.
Neither node has reached `accepted` in `TASK_GRAPH.md`.

This means, per the orchestrator's own instruction, this packet **does not**
base its starting point on a specific commit hash (none exists yet for either
predecessor) and instead bases every signature/shape claim below on **direct
inspection of the actual uncommitted worktree files as they exist right now**,
not on the predecessor packets' prose alone - every signature quoted in this
packet was confirmed by reading the real file at the path cited, not
inferred from `P1-PRINCIPAL-GRANT-001.md`'s narrative. Confirmed-by-direct-read
items are marked **[confirmed]**; anything this packet could not directly
verify (because it does not exist in the tree yet) is marked **[assumed]** and
named explicitly. There are no `[assumed]` items load-bearing to this node's
core two deliverables (Sections 6.1/6.2) - both are grounded entirely in code
already present in the worktree.

**If either predecessor's own independent review later forces a code-shape
change** to `crates/loki-agents/src/lib.rs`, `crates/loki-acceptance-store/src/lib.rs`,
or `crates/loki-agent-delivery/src/lib.rs` after this node has already built on
today's shape, this node rebases against the corrected shape before its own
acceptance is final - not a silent divergence (same rule as
`P1-PRINCIPAL-GRANT-001.md`/`P1-APPROVAL-JOB-ACCEPT-001.md` Section 3/12).

## 1. Objective

Per `Docs/graph/decisions/ADR-0008-agent-mutation-idempotency-composition.md`
(accepted, Option B), this node's real, narrowed scope is exactly two things,
plus the dependency-graph correction ADR-0008 required (already applied - see
Section 3):

1. **Fix a real, confirmed bug in `AgentRegistry::finish_run`**
   (`crates/loki-agents/src/lib.rs`): a legitimate retry of an
   already-completed, identical terminal transition (same `run_id`, same
   target `RunState`) currently returns `Err(AgentError::TerminalRun)` instead
   of the original success. `start_run` (idempotency-key lookup) and
   `WorkIntentDeliveryStore::acknowledge` (already-acknowledged-by-this-run
   check) are **already** correctly idempotent by direct inspection -
   `finish_run` is the one gap.
2. **Prove, with real tests that survive a simulated process restart (not
   sequential-only assertions), that composing `loki-acceptance-store::accept_mutation`
   (the generic ADR-0006 gate) with the real per-method domain effect
   (`AgentRegistry::start_run`/`finish_run`, `WorkIntentDeliveryStore::acknowledge`)
   as two separate, non-atomic calls is crash-safe**: a crash between the two
   calls - or after the second call completes but before the caller learns the
   result - never leaves the system in a state where retrying both calls
   produces a different, inconsistent, or duplicated outcome than the original
   attempt would have.

This node does **not** build new production "composition glue" code (a
reusable function/crate that a future RPC/IPC caller would call to perform
both steps in one call). No such caller exists yet in the repository today -
the wire-level shape of "how does a future RPC method's request payload carry
both the generic `MutationAcceptanceRequest` fields and the method-specific
business fields (`secret`, `parent_run_id`, event data, ...) to one call site"
is exactly the composition question ADR-0008 Option B declined to resolve by
extending the contract, and is a distinct future contract/RPC node's decision,
not this one's. This node proves the *pattern* - two independently-callable,
already-public APIs, called in sequence, are safe to retry across a crash -
via tests that call both real public APIs directly, exactly as the eventual
real caller will. Building speculative glue code with no real caller yet
would be exactly the kind of aspirational, unexercised production surface
`AGENTS.md` rule 1 and this graph's "small, executable graph" principle argue
against.

## 2. Traceability

- Requirements: `NFR-SEC-002`, `NFR-SAFE-001`, `NFR-REL-001`
- Acceptance scenarios: `AS-006`, `AS-009`, `AS-015` (agent/work-intent
  acknowledgement idempotency - `AGT-002`'s own scenario, now re-proven across
  the generic acceptance boundary)
- Architecture: `Docs/ARCHITECTURE.md` Section 8 Persistence; ADR-0001 "core storage"
  boundary (unaffected - this node adds no schema, no migration, no new
  table)
- Security: idempotent retry must never silently change an already-recorded
  terminal outcome, never duplicate a durable effect, never leave a run/work
  intent in an ambiguous state after a crash (`Docs/SECURITY.md`)
- Parent/decisions: `Docs/graph/decisions/ADR-0008-agent-mutation-idempotency-composition.md`
  (accepted, Option B - this packet operationalizes it exactly);
  `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
  (accepted); `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md` (accepted scope
  this node composes with, does not re-implement); `Docs/graph/tasks/P1-PRINCIPAL-GRANT-001.md`
  (accepted scope this node's tests build on for agent-connection grant
  seeding, and whose `rotate`/`change_scopes` additions this node's fix must
  not disturb)

## 3. Dependencies and inputs

- `depends_on` (graph-recorded, already correct in `TASK_GRAPH.md`'s
  `P1-AGENT-MUTATION-IDEMP-001` row - confirmed by direct read of that row,
  not assumed): `P1-ACCEPTANCE-STORE-001`, `P1-PRINCIPAL-GRANT-001`, `AGT-001`,
  `AGT-002`, `ADR-0008`. This satisfies ADR-0008's Consequences item
  ("`TASK_GRAPH.md`'s `P1-AGENT-MUTATION-IDEMP-001` row gains
  `P1-ACCEPTANCE-STORE-001` as an explicit dependency") - no further graph
  edit is needed by this packet; it is reflected here for the implementer's
  awareness, not re-authored.
- **Start-vs-integration distinction (explicit, not silent - same pattern as
  the two predecessor packets in this chain):** this node **starts** stacked
  on the uncommitted worktree described in Section 0 now. This node **cannot
  be marked `accepted` or integrated to `main`** before `P1-ACCEPTANCE-STORE-001`
  and `P1-PRINCIPAL-GRANT-001` both reach `accepted` and integrate first, in
  that order (`AGENTS.md` graph protocol point 8). `P1-APPROVAL-JOB-ACCEPT-001`
  is a sibling, genuinely parallel node (disjoint file ownership - see
  `P1-APPROVAL-JOB-ACCEPT-001.md` Section 14) and its presence/absence in the
  worktree does not block this node starting or finishing.
- Consumed contracts/exact revisions (read-only, unmodified by this node
  except where Section 5 names an edit):
  - `loki_acceptance_store::accept_mutation` **[confirmed]** - public entry
    point, signature `fn accept_mutation(storage: &Storage, request:
    &MutationAcceptanceRequest) -> Result<AcceptanceResult, AcceptanceStoreError>`
    (`crates/loki-acceptance-store/src/lib.rs:166`). Not modified by this
    node.
  - `loki_mutation_contract::{MutationAcceptanceRequest, MutationMethod,
    AcceptanceResult, IdempotencyKey}` **[confirmed]** - consumed exactly as
    designed; no edit.
  - `loki_agents::AgentRegistry::{new, register, start_run, finish_run}`
    **[confirmed]** exact current signatures (`crates/loki-agents/src/lib.rs`):
    - `pub fn new(storage: Arc<Storage>) -> Self` (line 242) - the
      `P1-ACCEPTANCE-STORE-001` shared-storage composition constructor this
      node's tests use to share one `Storage` with `accept_mutation`.
    - `pub fn start_run(&self, connection_id: &str, secret: &str,
      idempotency_key: &str, parent_run_id: Option<&str>, now_ms: i64) ->
      Result<AgentRun, AgentError>` (line 442) - already idempotent via
      `get_agent_run_by_idempotency_key` (line 452).
    - `pub fn finish_run(&self, connection_id: &str, secret: &str, run_id:
      &str, state: RunState, now_ms: i64) -> Result<(), AgentError>` (line
      487) - **the function this node fixes**; signature is **not** changed
      by the fix (Section 5).
  - `loki_agent_delivery::WorkIntentDeliveryStore::{new, enqueue, claim,
    acknowledge}` **[confirmed]** (`crates/loki-agent-delivery/src/lib.rs`):
    `pub fn new(storage: Arc<Storage>) -> Self` (line 193);
    `pub fn acknowledge(&self, agents: &AgentRegistry, request:
    AcknowledgeRequest<'_>) -> Result<DeliveryRecord, DeliveryError>` (line
    368) - already idempotent via its own `AlreadyAcknowledgedByThisRun`
    branch (line 384/409). Not modified by this node.
  - `loki_storage::agents::finish_agent_run_core` **[confirmed]**
    (`crates/loki-storage/src/agents.rs:470`) - a single atomic guarded
    `UPDATE agent_runs SET state=?,completed_at_ms=? WHERE run_id=? AND
    state='running'`, returning whether it performed the transition. This is
    already correct; the bug is entirely in `AgentRegistry::finish_run`'s
    business-logic wrapper around it (Section 5), not in this storage-layer
    function, so this node does not touch `loki-storage`.
- Known starting conditions (confirmed by direct inspection of the worktree
  file named, not assumed):
  - `crates/loki-agents/src/lib.rs:503-505`:
    ```rust
    if run.state.terminal() {
        return Err(AgentError::TerminalRun);
    }
    ```
    unconditionally errors once the run is already terminal, with no check
    for whether `state` (the caller's requested target) matches
    `run.state` (the already-recorded outcome). **This is the exact bug.**
  - The existing test `invalid_transition_is_denied`
    (`crates/loki-agents/src/lib.rs:752-787`) **currently encodes the bug as
    correct behavior**: it calls `finish_run(..., RunState::Failed, ...)`
    twice on the same run and asserts the **second** call returns
    `Err(AgentError::TerminalRun)` (lines 769-786). Fixing the bug requires
    changing this specific assertion - flagged here explicitly so the
    implementer does not mistake an existing green test for a reason not to
    change behavior.
  - `crates/loki-acceptance-store/src/lib.rs` has **zero references** to
    `AgentRunRegister`/`AgentRunUpdate`/`WorkIntentAcknowledge`-specific
    business logic and **zero dependency** on `loki-agents`/`loki-agent-delivery`
    (confirmed by grep and by reading `crates/loki-acceptance-store/Cargo.toml`'s
    own recorded, considered "deliberate deviation" comment: it declares this
    crate intentionally does not depend on those four business crates because
    its transaction never revalidates their IPC-session authorization layer).
    For agent methods, `accept_mutation` today only ever performs the generic
    ADR-0006 gate (grant/idempotency/outbox/`mutation_acceptances`) and
    synthesizes an `mjob_`-prefixed `job_id` - it never creates an
    `agent_runs` row, never transitions one, never acknowledges a work
    intent. The second, real-effect call is entirely this node's to compose
    and prove, not something partially built already (unlike
    `P1-APPROVAL-JOB-ACCEPT-001`'s finding for approval/job binding).
  - `crates/loki-acceptance-store/tests/acceptance_flow.rs` already has
    `seed_agent_connection_grant`/`agent_request` helpers (lines 100, 122)
    this node's own tests should reuse for grant-generation seeding style,
    but those helpers seed the `agent_connections` row **directly through
    `loki_storage::agents::AgentsRepository`** with a fabricated
    `secret_hash` ("sha256:test-hash") - a connection seeded this way has no
    real secret this node's tests can authenticate with via
    `AgentRegistry::start_run`/`finish_run`. This node's own tests must
    instead register the connection through the real `AgentRegistry::register`
    API (which mints a real secret) so the same connection can be both (a)
    read by `accept_mutation`'s grant check and (b) authenticated against by
    the real business calls - see Section 6.

## 4. Required reading

- `AGENTS.md`, `Docs/ARCHITECTURE.md` Section 8
- `Docs/graph/decisions/ADR-0008-agent-mutation-idempotency-composition.md`
  (in full - this packet operationalizes it exactly, Option B only)
- `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
- `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md` Section 6 Deliverable 6 (the seven-step
  transaction this node composes with, unmodified)
- `Docs/graph/tasks/P1-PRINCIPAL-GRANT-001.md` Sections 1.1, 5, 9 item 5 (the
  per-identity-model grant read and the `AgentRegistry::rotate`/`change_scopes`
  additions this node's fix must not regress)
- In the `node/P1-ACCEPTANCE-STORE-001` worktree, read in full:
  `crates/loki-agents/src/lib.rs` (all of `finish_run`, `start_run`, the
  `invalid_transition_is_denied` and `events_are_ordered_unique_and_terminal_rejects_append`
  tests, and `RunState`'s `terminal()`/`PartialEq`/`Eq` derive at line 92);
  `crates/loki-storage/src/agents.rs` (`finish_agent_run_core`,
  `get_agent_run_by_idempotency_key_core`); `crates/loki-agent-delivery/src/lib.rs`
  (`acknowledge`, and at least one existing full enqueue-claim-acknowledge
  test, e.g. around lines 750-1058, as the setup pattern to mirror);
  `crates/loki-acceptance-store/src/lib.rs` in full (the composition this node
  proves safe, not modified); `crates/loki-acceptance-store/tests/acceptance_flow.rs`
  (`open_storage`, `seed_agent_connection_grant`, `agent_request` helpers -
  read for style, do not edit that file); `crates/loki-acceptance-store/Cargo.toml`
  (the "deliberate deviation" comment this node's own dev-dependency addition
  must not contradict - see Section 5)

## 5. Ownership

Writable paths (all inside worktree `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001`,
new commits on top of branch `node/P1-ACCEPTANCE-STORE-001`, or a child branch
`node/P1-AGENT-MUTATION-IDEMP-001` stacked on it - orchestrator's choice,
recorded in the handoff either way):

- `crates/loki-agents/src/lib.rs`:
  - **The fix (Deliverable 1):** in `finish_run`, replace the unconditional
    `if run.state.terminal() { return Err(AgentError::TerminalRun); }` with a
    check that first compares `run.state` (the already-recorded terminal
    state) against `state` (the caller's requested target): if they are
    equal, return `Ok(())` **without calling `finish_agent_run`/writing
    anything** (the original success is being replayed, not re-performed -
    do not let a replay silently advance `completed_at_ms` to a new
    timestamp); if they differ, keep returning `Err(AgentError::TerminalRun)`
    (a genuinely conflicting second transition attempt, still correctly
    denied). No change to `finish_run`'s signature, no change to
    `start_run`/`register`/`revoke`/`rotate`/`change_scopes`/`append_event`/
    `authenticate`.
  - Update `invalid_transition_is_denied`'s second `finish_run(..., RunState::Failed,
    ...)` call (currently asserting `Err(AgentError::TerminalRun)`, lines
    769-786) to assert `Ok(())` instead, and add a **new** assertion in the
    same test (or a new test) that finishing the same already-terminal run
    with a **different** terminal state (e.g. `RunState::Succeeded` after it
    is already `Failed`) still returns `Err(AgentError::TerminalRun)` - this
    proves the fix narrows correctly to "identical retry only," not "any
    terminal run always succeeds."
  - Add a new test, e.g. `finish_run_retry_of_identical_transition_is_idempotent`:
    call `finish_run` twice with the same target `RunState` but two different
    `now_ms` values; assert both calls return `Ok(())`; then read the run back
    (`require_run_for_connection` or equivalent existing accessor) and assert
    `completed_at_ms` equals the **first** call's `now_ms`, not the second -
    proving the replay path performs no write.
  - Doc comment on `finish_run` citing this packet and ADR-0008, stating the
    exact idempotency rule in one sentence, and stating plainly that this is
    the "real domain effect" half of `P1-AGENT-MUTATION-IDEMP-001`'s two-call
    composition for `MutationMethod::AgentRunUpdate`.
- `crates/loki-acceptance-store/Cargo.toml`: add `loki-agents` and
  `loki-agent-delivery` under `[dev-dependencies]` **only** (never
  `[dependencies]` - the existing "deliberate deviation" comment about this
  crate's production dependency set stands unchanged and unchallenged by this
  node), with a short comment explaining why: this node's new integration
  test proves an external, cross-crate composition property (two independent
  public APIs are safe to call in sequence across a process restart), which
  is a test-only concern distinct from the crate's own production dependency
  graph.
- `crates/loki-acceptance-store/tests/agent_mutation_composition.rs` (new
  file): the crash-safety proof (Deliverable 2 - see Section 6.2). Does not
  modify `acceptance_flow.rs`.
- `Docs/graph/handoffs/P1-AGENT-MUTATION-IDEMP-001-1.md` (own handoff only).

Read-only dependency paths:

- `crates/loki-mutation-contract/src/**` (consume; do not edit)
- `crates/loki-acceptance-store/src/lib.rs` (consume; do not edit - this
  node's entire premise is that the generic gate is already correct and
  unmodified; if it is found insufficient, that is `NEEDS_REPLAN`, Section 12)
- `crates/loki-acceptance-store/tests/acceptance_flow.rs` (read for helper
  style only; do not edit - owned by `P1-ACCEPTANCE-STORE-001`/
  `P1-PRINCIPAL-GRANT-001`/`P1-APPROVAL-JOB-ACCEPT-001`)
- `crates/loki-storage/src/agents.rs`, `crates/loki-storage/src/agent_delivery.rs`,
  `crates/loki-storage/src/local_authority.rs`, `crates/loki-storage/src/storage.rs`
  (consume; do not edit)
- `crates/loki-agent-delivery/src/lib.rs` other than reading (do not edit -
  `acknowledge` is already correctly idempotent; no change needed or
  permitted)
- `Docs/graph/decisions/**`, `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md`,
  `Docs/graph/tasks/P1-PRINCIPAL-GRANT-001.md`

Forbidden scope:

- No edit to `crates/loki-mutation-contract/**` (ADR-0008 Option B's entire
  point - the contract stays untouched through this whole chain).
- No edit to any executable line in `crates/loki-acceptance-store/src/lib.rs`
  (only its `Cargo.toml` gains a `[dev-dependencies]` entry - no production
  dependency, no production code change).
- No new "composition glue" production function or crate that wraps
  `accept_mutation` plus the real business call into one reusable call site
  (Section 1) - that is a future RPC/contract node's decision once a real
  caller and its wire shape exist, not this node's to invent speculatively.
- No change to `WorkIntentDeliveryStore::acknowledge` or any other
  `loki-agent-delivery` code - it is already correctly idempotent; do not
  "fix" something that is not broken.
- No change to `AgentRegistry::start_run`, `register`, `revoke`, `rotate`,
  `change_scopes`, `authenticate`, or `append_event` - only `finish_run`'s
  internal logic changes (its signature does not).
- No new `MutationMethod`/`MutationRejection`/`AcceptanceResult` variant
  anywhere.
- No schema/migration change anywhere (`loki-storage/migrations/**` untouched
  - this node's fix and tests need no new column or table).
- No RPC/wire schema work.
- Do not resolve the `outcome_unknown` trigger contract (`P1-MUTATION-CONTRACT-001`
  Section 9 item 2, still open) - this node's composition never runs a physical
  device/connector effect, so it never reaches that decision point; confirm
  this in the handoff rather than silently assuming it.
- Do not attempt to reconcile `MutationAcceptanceRequest.target_id`'s meaning
  across the three agent methods into one canonical convention (Section 9
  item 1) - record the asymmetry, do not invent a fix for it here.

## 6. Deliverables

### 6.1 finish_run retry fix (Deliverable 1)

1. The fix itself, exactly as specified in Section 5's first bullet.
2. Updated `invalid_transition_is_denied` test (retry-of-identical-transition
   now asserts `Ok(())`; new assertion for retry-of-**different**-terminal-state
   still asserts `Err(AgentError::TerminalRun)`).
3. New test `finish_run_retry_of_identical_transition_is_idempotent` proving
   no write occurs on replay (Section 5).
4. Doc comment citing this packet and ADR-0008.

### 6.2 Crash-safe two-call composition proof (Deliverable 2)

New file `crates/loki-acceptance-store/tests/agent_mutation_composition.rs`.
Every test in this file:

- opens a real, on-disk, DPAPI-managed SQLCipher-encrypted `Storage` (mirror
  `acceptance_flow.rs`'s `open_storage` helper exactly - not `:memory:`);
- constructs `AgentRegistry::new(storage.clone())` (and, for the
  work-intent-acknowledge test, `WorkIntentDeliveryStore::new(storage.clone())`)
  over that **same** `Arc<Storage>` so `accept_mutation` and the real business
  call read/write the same database, exactly as a real future caller would;
  registers a real agent connection via `AgentRegistry::register` (not the
  `seed_agent_connection_grant` fabricated-secret helper - Section 3) so the
  test can authenticate the real business call with a real secret, while
  `accept_mutation`'s grant read still succeeds against the same
  `agent_connections` row;
- performs **step 1**: call `accept_mutation(&storage, &request)` for the
  target `MutationMethod`; assert `AcceptanceResult::Accepted(_)`;
- simulates **the crash**: drops every `Arc<Storage>`/`AgentRegistry`/
  `WorkIntentDeliveryStore` handle held so far (no in-process state survives);
- simulates **the restart**: opens a **new** `Storage::open_managed` (fresh
  `Arc`, same on-disk path/key) and fresh `AgentRegistry`/`WorkIntentDeliveryStore`
  instances over it - this is what makes the proof about real durable
  persistence, not merely in-memory object lifetime;
- performs **step 2, first real attempt**: calls the real business method
  (`start_run`/`finish_run`/`acknowledge`) with the exact same business
  parameters the original caller would have used; asserts success with the
  expected data;
- performs a **second full retry of both calls** (simulating "the caller
  never learned step 2 succeeded and retries everything from the top"): calls
  `accept_mutation` again with the identical request - asserts
  `AcceptanceResult::ReplayedIdenticalAccepted(_)` with the **same**
  `job_id()`/`correlation_id()`/`accepted_at_ms()` as attempt 1 (proving the
  generic gate's own idempotency, already established by
  `P1-ACCEPTANCE-STORE-001`, still holds in this composed context); then
  calls the real business method again with the same parameters - asserts it
  **also** succeeds with the identical result (not an error, not a different
  outcome);
- asserts, via a direct repository read, that exactly **one** durable row/state
  change exists for the effect (one `agent_runs` row for
  `AgentRunRegister`/`AgentRunUpdate`; the work intent's
  `acknowledged_agent_run`/state set exactly once for `WorkIntentAcknowledge`)
  - no duplication across the two full attempts.

Required tests in this file:

1. `agent_run_register_composition_survives_restart_between_accept_and_apply`
   - `MutationMethod::AgentRunRegister` + `AgentRegistry::start_run`.
2. `agent_run_update_composition_survives_restart_between_accept_and_apply`
   - `MutationMethod::AgentRunUpdate` + `AgentRegistry::start_run` (to create
   the run first, outside the measured composition) then `finish_run` as the
   measured second call. This is the test that most directly exercises
   Deliverable 1's fix in the composed context - it must fail red if the
   `finish_run` fix from Section 6.1 is reverted (state this as done in the
   handoff, mirroring `P1-APPROVAL-JOB-ACCEPT-001.md`'s equivalent
   red/green check).
3. `work_intent_acknowledge_composition_survives_restart_between_accept_and_apply`
   - `MutationMethod::WorkIntentAcknowledge` + a real enqueue-claim-acknowledge
   flow (mirror `loki-agent-delivery`'s own existing full-flow test setup) +
   `WorkIntentDeliveryStore::acknowledge` as the measured second call.
4. `finish_run_conflicting_retry_after_composition_is_still_rejected` - after
   a successful `AgentRunUpdate` composition (run finished as `Failed`), a
   **different** target state (`Succeeded`) passed to `finish_run` for the
   same run is rejected with `Err(AgentError::TerminalRun)` - the composed-context
   regression proof that Deliverable 1 does not overcorrect (mirrors 6.1
   item 2 but in the cross-crate composed context, not just `loki-agents`'s
   own unit test).

Doc comment at the top of the new file citing ADR-0008 and this packet, and
stating explicitly why no new production glue code exists (Section 1).

## 7. Acceptance criteria

- [ ] `cargo build --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` succeed with zero warnings.
- [ ] `cargo test -p loki-agents -- --test-threads=1` passes, including the updated `invalid_transition_is_denied` and the new `finish_run_retry_of_identical_transition_is_idempotent`, plus every existing test in the crate (including `P1-PRINCIPAL-GRANT-001`'s `rotate`/`change_scopes`/generation tests) unmodified elsewhere.
- [ ] `cargo test -p loki-acceptance-store -- --test-threads=1` passes, including all four new tests in `agent_mutation_composition.rs`, plus every existing test in `acceptance_flow.rs` unmodified.
- [ ] `cargo test --workspace -- --test-threads=1` passes (documented flake class only).
- [ ] `pnpm run verify` passes (one retry allowed on the documented flaky Windows Application Control error, per existing project convention).
- [ ] `git diff --stat` shows changes only to `crates/loki-agents/src/lib.rs`, `crates/loki-acceptance-store/Cargo.toml` (dev-dependencies only), the new `crates/loki-acceptance-store/tests/agent_mutation_composition.rs`, `Cargo.lock` (dependency-graph update only), and the handoff.
- [ ] Grep-verifiable: no new `[dependencies]` entry added to `crates/loki-acceptance-store/Cargo.toml` - only `[dev-dependencies]`.
- [ ] Grep-verifiable: `crates/loki-acceptance-store/src/lib.rs` has zero diff (confirms the "no executable-line change" claim independent of the `Cargo.toml`/test-only changes).
- [ ] A retry of `finish_run` with the identical target `RunState` after the run is already in that state returns `Ok(())`, not `Err(AgentError::TerminalRun)`, and performs no write (`completed_at_ms` unchanged from the first call) - reproduced by `finish_run_retry_of_identical_transition_is_idempotent`.
- [ ] A retry of `finish_run` with a **different** terminal `RunState` after the run is already terminal still returns `Err(AgentError::TerminalRun)` - reproduced by both the updated `invalid_transition_is_denied` and `finish_run_conflicting_retry_after_composition_is_still_rejected`.
- [ ] Each of the three composition tests proves: `accept_mutation` returns `Accepted` on the first attempt and `ReplayedIdenticalAccepted` with identical `job_id`/`correlation_id`/`accepted_at_ms` on the retry attempt (across a real, fresh `Storage` handle opened over the same on-disk file - not the same in-process object); the real business call succeeds identically on both the first post-restart attempt and the retry attempt; exactly one durable row/state change exists for the effect after both attempts (no duplication).
- [ ] The `agent_run_update_composition_survives_restart_between_accept_and_apply` test is confirmed, in the handoff, to fail red if Deliverable 1's `finish_run` fix is reverted (implementer verifies this locally during development, does not commit the reverted state - same convention as `P1-APPROVAL-JOB-ACCEPT-001.md`'s acceptance criterion).
- [ ] Independent test-reviewer sign-off confirming all four new composition tests genuinely span a real process-restart boundary (fresh `Storage`/`AgentRegistry`/`WorkIntentDeliveryStore` instances, not reused in-process handles) and genuinely exercise retry-after-success, not merely first-attempt success.
- [ ] Independent security-reviewer sign-off confirming: (a) the `finish_run` fix cannot be used to silently overwrite an already-recorded terminal outcome with a *different* one (the conflicting-retry case stays denied); (b) no new dependency was added to `crates/loki-acceptance-store`'s production `[dependencies]`; (c) no new production "composition glue" surface was introduced (Section 1/forbidden scope).
- [ ] Every open question in Section 9 is recorded as still-open in the handoff.

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `cargo fmt --all -- --check` | exit 0 | command output |
| Static | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | command output |
| Unit | `cargo test -p loki-agents -- --test-threads=1` | all existing tests pass plus the two new/updated `finish_run` tests | run summary / test names |
| Integration | `cargo test -p loki-acceptance-store -- --test-threads=1` | all existing tests pass plus all four new composition tests | run summary / test names |
| Crash-safety | real file-backed `Storage`, fresh handles reopened over the same on-disk file/key after each simulated crash point, for all three agent methods | identical outcome across original attempt and retry; no duplicate row/state; no panic | test output, row-count assertions |
| Regression | conflicting-retry (different terminal state) still denied, in both `loki-agents` unit context and the composed cross-crate context | `Err(AgentError::TerminalRun)` in both | test output |
| Integration | `cargo test --workspace -- --test-threads=1` | pass (documented flake class only) | run summary |
| Security | independent review per Section 7's security bullets | no finding | reviewer note in handoff |
| Desktop/Cloud/Hardware/Containment | n/a - no IPC, no cloud, no device, no build-executor surface in this node | n/a | n/a |
