# Task Packet — `P1-APPROVAL-JOB-ACCEPT-001` Approval-Consumption/Job-Acceptance Regression Evidence

Packet status: `proposed` (ready for orchestrator review/dispatch)
Graph status: `ready` — see Section 3 for the scope finding this packet is built on
Created by: `orchestrator (Claude Code), shaped by loki-planner`
Created at: `2026-08-28T12:52:00Z`
Starting revision: **stacked on branch `node/P1-ACCEPTANCE-STORE-001`'s current
uncommitted tip** in worktree `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001` — **not**
`LokiAI-v1.2`'s tip. Same stacking pattern as `P1-PRINCIPAL-GRANT-001.md` Section 3, for
the same reason: every file this node touches only exists in that worktree today.

## 0. Planner scope finding — read before dispatching this as a normal
   implementation node

This node's title, and `claudeplanner.md` Section 2.5's own framing ("bind approval
consumption and job/outbox creation into the same commit"), describe work that
**direct inspection of the current `node/P1-ACCEPTANCE-STORE-001` worktree shows
is already implemented, not still to be built.** This is not a restatement of the
task title — it is a specific, code-verified finding:

- `crates/loki-acceptance-store/src/lib.rs` (`accept_mutation_in_transaction`,
  lines ~292-421 at the time of this reading) already, inside the single
  `Storage::transaction()` call: looks up the approval row
  (`loki_storage::approvals::get_approval_core`), refreshes the request's
  `ApprovalReference` from the row's own stored fields, calls
  `loki_mutation_contract::construct_accepted` (which internally calls
  `evaluate_approval_reference`), and — only on a genuine `Accepted` outcome —
  calls `loki_storage::approvals::consume_approval_core` and inserts the
  `jobs` row (`loki_storage::jobs::insert_job_core`) and appends the outbox
  record and inserts the `mutation_acceptances` row, all before that one
  transaction commits. There is no second commit, no separate approval-consumption
  step outside this transaction, and no code path that can commit a job/outbox
  row without also consuming the approval in the same commit (or vice versa).
- `accepted_at_ms`/`now_ms` is already read inside the transaction closure
  (`clock::unix_millis_now()`, called before any lookup), closing the
  time-of-check/time-of-use gap this task's `claudeplanner.md` entry names as
  its own "most likely recurring failure class."
- `crates/loki-acceptance-store/tests/acceptance_flow.rs` already has a passing
  test for "approval expiring between request-receipt and lock-acquisition is
  rejected" (`approval_already_expired_by_the_time_the_transaction_runs_is_rejected`),
  and already proves, via `fault_injection_after_each_step_leaves_zero_orphan_rows`,
  that a fault after approval consumption but before commit leaves the approval
  unconsumed (rollback), and that a fault after job/outbox insertion but before
  the final `mutation_acceptances` insert also rolls back the already-consumed
  approval back to unconsumed state (real `Storage::transaction` rollback
  semantics, not simulated).
- `crates/loki-storage/src/approvals.rs`'s `consume_approval_core` already
  independently re-derives and re-checks digest/requester/grant/expiry/consumption
  state directly against the row (not merely trusting the caller's already-passed
  `construct_accepted` check) before performing its one permitted write — a
  defense-in-depth property already built, already documented in that function's
  own doc comment.

**What genuinely remains, found by the same direct inspection, not assumed:**

1. `crates/loki-mutation-contract::request::evaluate_approval_reference` already
   rejects a bound-digest mismatch as `MutationRejection::ApprovalNotConsumable`
   (`request.rs:62-64`), and `loki-acceptance-store` already feeds it the
   approval row's own stored `request_digest` as the comparison target — so
   "a changed-request during retry is rejected with `ApprovalNotConsumable`"
   (this node's own named acceptance criterion, and `Docs/SPEC.md` **AS-007**)
   is already mechanically enforced. **But no test in `acceptance_flow.rs`
   exercises this specific scenario today**: an approval granted for one
   target/artifact, then a *fresh* request (different idempotency key, so it is
   not intercepted earlier by step 3's `evaluate_replay`/`DigestConflict` path)
   naming the same `approval_id` but a different `target_id`/`artifact_digest`.
   This is the one concrete, named, currently-untested acceptance scenario this
   node exists to close.
2. "Cancellation of a job whose approval was already consumed does not
   un-consume the approval" is true today **by construction**, confirmed by a
   repository-wide grep: no function anywhere in `crates/loki-storage`,
   `crates/loki-approvals`, or `crates/loki-jobs` ever resets
   `local_approvals.consumed_at_ms` back to `NULL`, and `loki-jobs::JobEngine`'s
   entire cancellation model (`request_cancel`/`acknowledge_cancelled`,
   `crates/loki-jobs/src/engine.rs`) is an in-memory `cancel_requests:
   Mutex<HashSet<JobId>>` set with **zero reference to `loki-storage::approvals`
   or `local_approvals` anywhere** — there is no code path capable of
   un-consuming an approval, cancellation-triggered or otherwise. This is a
   structural absence, not a passing test, and this node's only remaining
   obligation regarding it is to **assert and document** the absence (a
   grep-verifiable acceptance criterion plus a doc-comment citation), not to
   write a behavioral test for a code path that does not exist.

Given this, **this node is not a second implementation of the ADR-0006
approval/job binding** — that binding is `P1-ACCEPTANCE-STORE-001`'s own
already-delivered Deliverable 6, confirmed here by direct code and test
reading, not by trusting that packet's narrative. This node's real, bounded
job is: (a) add the one missing regression test named above (AS-007's changed-
request-after-approval scenario), (b) add the grep-verifiable structural
assertion/doc-comment for the cancellation-non-interaction property, and
(c) carry independent test-review/security-review sign-off confirming both,
closing out `claudeplanner.md` Section 2.5 and `TASK_GRAPH.md`'s existing
`P1-APPROVAL-JOB-ACCEPT-001` row on the evidence that is actually still
missing. **If a reviewer or implementer finds, on deeper inspection, that the
existing binding logic has a real defect the tests above don't already catch,
that is `NEEDS_REPLAN`, not something to silently patch while "just adding a
test" — see Section 12.**

## 1. Objective

Close the one remaining named gap in ADR-0006's approval-consumption/job-
acceptance atomic-binding evidence (`claudeplanner.md` Section 2.5, `TASK_GRAPH.md`'s
`P1-APPROVAL-JOB-ACCEPT-001` row): add a real, file-backed integration test
proving that a fresh request (new idempotency key) which reuses a previously
granted approval's `approval_id` but changes the bound target/artifact is
rejected as `MutationRejection::ApprovalNotConsumable`, and never consumes the
approval or writes any job/outbox/mutation_acceptances row (AS-007). Add a
narrow, additive, grep-verifiable structural assertion (test plus doc comment)
that no code path anywhere in the workspace can un-consume an approval,
specifically including job cancellation. No change to any production
transaction/binding logic — that logic already exists and is not this node's
to rebuild.

## 2. Traceability

- Requirements: `NFR-SEC-002`, `NFR-SAFE-001`, `NFR-REL-001`
- Acceptance scenarios: **AS-007** (changed request after approval — this
  node's primary target), `AS-006`, `AS-009`
- Architecture: `Docs/ARCHITECTURE.md` Section 8 Persistence; ADR-0001 "core storage"
  boundary (untouched — this node adds no schema, no migration, no new table)
- Security: single-use approval consumption re-validated under a changed
  request (`Docs/SECURITY.md`); fail-closed cancellation/approval isolation
- Parent/decisions: `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
  (accepted); `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md` (accepted scope
  this node verifies, does not re-implement); `Docs/graph/tasks/P1-PRINCIPAL-GRANT-001.md`
  (accepted scope this node's tests build on for grant seeding)

## 3. Dependencies and inputs

- `depends_on` (graph-recorded): `P1-PRINCIPAL-GRANT-001`, `APR-001`, `JOB-001`.
- **Start-vs-integration distinction (explicit, not silent — same pattern as
  `P1-PRINCIPAL-GRANT-001.md` Section 3):** `P1-ACCEPTANCE-STORE-001` and
  `P1-PRINCIPAL-GRANT-001` are both implemented in worktree
  `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001` (branch
  `node/P1-ACCEPTANCE-STORE-001`) as of this reading, but **neither has reached
  `accepted` status in `TASK_GRAPH.md`** (both remain `proposed`/in-review per
  their own task packets' Section 11 human gates). This node **starts** stacked
  on that exact worktree/branch now. This node **cannot be marked `accepted` or
  integrated to `main`** before both predecessors reach `accepted` and integrate
  first, in that order (`AGENTS.md` graph protocol point 8). If either
  predecessor's independent review forces a code-shape change to
  `crates/loki-acceptance-store/src/lib.rs` or
  `crates/loki-acceptance-store/tests/acceptance_flow.rs` after this node has
  already added its own test(s), this node rebases against the corrected shape
  before its own acceptance is final — not a silent patch around the
  divergence.
- Consumed contracts/exact revisions (read-only, unmodified by this node):
  - `crates/loki-mutation-contract::{result::construct_accepted,
    request::evaluate_approval_reference, MutationRejection::ApprovalNotConsumable}`
    — consumed exactly as designed; this node's new test proves an existing
    contract behavior, it does not add to the contract.
  - `crates/loki-storage::approvals::{ApprovalsRepository, NewApproval,
    ConsumeOutcome, consume_approval_core, get_approval_core}` — consumed
    exactly as designed.
  - `crates/loki-acceptance-store::accept_mutation` — the public entry point
    this node's new test calls; **not modified** by this node.
  - `crates/loki-jobs::engine::JobEngine` — read-only, to confirm (already
    confirmed by this packet's own Section 0 grep) that its cancellation model
    has no reference to `local_approvals`/`ApprovalsRepository` anywhere.
- Required environment/credential/device: none beyond normal build/test
  toolchain (same as `P1-ACCEPTANCE-STORE-001`/`P1-PRINCIPAL-GRANT-001`).
- Known starting conditions (confirmed by direct inspection of the worktree,
  not assumed): see Section 0 above in full — this section is not a summary,
  it is this packet's primary evidentiary basis and must be re-read by the
  implementer before writing anything.

## 4. Required reading

- `AGENTS.md`, `Docs/ARCHITECTURE.md` Section 8, `Docs/SPEC.md` AS-006/AS-007/AS-009
- `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
- `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md` (in full — this node verifies
  its Deliverable 6, does not re-implement it)
- `Docs/graph/tasks/P1-PRINCIPAL-GRANT-001.md` Section 5 (the `seed_agent_connection_grant`/
  `seed_grant` test-seeding helpers this node's new test reuses)
- In the `node/P1-ACCEPTANCE-STORE-001` worktree:
  `crates/loki-acceptance-store/src/lib.rs` (read only — do not edit its
  transaction/binding logic); `crates/loki-acceptance-store/tests/acceptance_flow.rs`
  (read in full — this node appends to it); `crates/loki-mutation-contract/src/request.rs`
  (`evaluate_approval_reference`, lines ~57-69); `crates/loki-storage/src/approvals.rs`
  (`consume_approval_core`); `crates/loki-jobs/src/engine.rs` (`request_cancel`/
  `acknowledge_cancelled`, to confirm no approval interaction before writing the
  structural-absence assertion)

## 5. Ownership

Writable paths (all inside worktree `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001`,
new commits on top of branch `node/P1-ACCEPTANCE-STORE-001`, or a child branch
`node/P1-APPROVAL-JOB-ACCEPT-001` stacked on it — orchestrator's choice,
recorded in the handoff either way, same pattern as `P1-PRINCIPAL-GRANT-001.md`
Section 5):

- `crates/loki-acceptance-store/tests/acceptance_flow.rs` — **append-only**:
  add exactly two new `#[test]` functions (Section 6 Deliverables 1-2) plus
  any small helper function(s) they need (e.g. a variant of
  `flash_request_with_granted_approval` that returns a *second*, differently-
  targeted request reusing the same `approval_id`). Do not modify, rename, or
  remove any existing test or helper.
- `crates/loki-acceptance-store/src/lib.rs` — **doc-comment-only** edit:
  add a short doc comment on `accept_mutation_in_transaction` (or its existing
  module-level "Open questions" section) citing this packet and explicitly
  stating the "no un-consume code path exists" structural property, with a
  pointer to the new grep-verifiable test/assertion. **No change to any
  executable statement, function signature, or control flow in this file.**
- `Docs/graph/handoffs/P1-APPROVAL-JOB-ACCEPT-001-1.md` (own handoff only).

Read-only dependency paths:

- `crates/loki-mutation-contract/src/**` (consume; do not edit)
- `crates/loki-storage/src/{approvals.rs,jobs.rs,outbox.rs,mutation_acceptance.rs,local_authority.rs,agents.rs,storage.rs}`
  (consume; do not edit)
- `crates/loki-jobs/src/**` (read only, to confirm the structural-absence claim
  before asserting it — do not edit)
- `crates/loki-approvals/src/**` (read only — do not edit)
- `Docs/graph/decisions/**`, `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md`,
  `Docs/graph/tasks/P1-PRINCIPAL-GRANT-001.md`

Forbidden scope:

- No edit to `crates/loki-mutation-contract/**` (pure contract crate, already
  accepted).
- No edit to any executable line in `crates/loki-acceptance-store/src/**`
  (the ADR-0006 transaction/binding logic is `P1-ACCEPTANCE-STORE-001`'s
  already-delivered scope — see Section 0; if this node finds it insufficient,
  that is `NEEDS_REPLAN`, not a silent patch here).
- No edit to `crates/loki-storage/**`, `crates/loki-approvals/**`,
  `crates/loki-jobs/**`, `crates/loki-agents/**`, `crates/loki-local-authority/**`
  — this node adds test coverage and documentation only, no production code,
  no schema, no migration.
- No new `MutationMethod`/`MutationRejection`/`AcceptanceResult` variant
  anywhere (none is needed — `ApprovalNotConsumable` already exists and is
  already the correct outcome for this scenario, per Section 0).
- No workspace/tenant/scopes field invented anywhere (ADR-0006 Amendment 1
  remains settled).
- No RPC/wire schema work.
- Do not resolve the `outcome_unknown` trigger contract or the "declared safe
  point" mechanism (`P1-MUTATION-CONTRACT-001` Section 9 items 1-2, still open,
  unaffected by this node — this node adds no physical-effect code path).
- Do not attempt to give `loki-jobs::JobEngine` durable/cross-crate
  cancellation semantics, or wire it to `loki-approvals`/`loki-storage::approvals`
  in any way — that would be new production behavior invented to make a test
  "more thorough" and is explicitly out of scope; the correct evidence for
  "cancellation cannot un-consume an approval" today is the structural absence
  itself (Section 0 item 2), asserted and documented, not a new integration.

## 6. Deliverables

1. **New test: `retry_with_changed_target_after_approval_is_rejected_as_not_consumable`**
   (or equivalent name) in `acceptance_flow.rs`: grants an approval for
   `(PRINCIPAL_SID, "target-1", artifact-A)` exactly as
   `flash_request_with_granted_approval` already does; then constructs a
   **second, fresh** `MutationAcceptanceRequest` — a **different idempotency
   key** (so step 3's `evaluate_replay` is never reached — this must genuinely
   exercise step 4's approval-binding check, not step 3's idempotency-digest
   check, which `same_key_different_digest_rejected` already covers), the
   **same `approval_id`**, but a **different `target_id`** (e.g. `"target-2"`)
   reusing the *original* approval's `bound_request_digest`/`approval_id` in
   its `ApprovalReference` (simulating a client that still holds the old
   reference and tries to apply it to a modified operation — exactly AS-007's
   "any bound flash input changes after approval" scenario). Asserts:
   `accept_mutation` returns `AcceptanceResult::Rejected(MutationRejection::ApprovalNotConsumable)`;
   the approval row's `consumed_at_ms` remains `None` afterward
   (`storage.get_approval(...)`); no `jobs` row exists for any job id that
   test could have produced; `mutation_acceptance_exists` is `false` for the
   new idempotency key.
2. **New test: `job_cancellation_has_no_approval_consumption_interaction`** (or
   equivalent name) in `acceptance_flow.rs`: performs a real
   `approval_gated_accept_consumes_approval_and_creates_real_job`-style
   accepted flow (approval consumed, real `jobs` row created), then
   constructs a `loki_jobs::JobEngine` **fresh, independent of `storage`**
   (per that crate's existing in-memory constructor — no wiring is added), and
   calls `request_cancel`/`acknowledge_cancelled` against the resulting job id
   directly to prove the call succeeds/fails on its own terms **without ever
   touching `storage`**, then re-reads the approval row from `storage`
   afterward and asserts `consumed_at_ms` is unchanged (still `Some`, same
   value as before the cancellation calls). This proves the property
   end-to-end against the real crate, not merely by citing the grep in
   Section 0 — the grep is corroborating evidence in the handoff, not a
   substitute for a runnable assertion.
3. Doc-comment addition (Section 5) in `loki-acceptance-store/src/lib.rs`
   citing this packet, stating: (a) the changed-request-after-approval
   property is enforced by `evaluate_approval_reference`'s digest check,
   proven by Deliverable 1; (b) no code path in the workspace can un-consume
   an approval, proven structurally (grep) and by Deliverable 2.
4. Handoff confirming: the exact grep commands run to support the Section 0
   structural claims (reproducible, included verbatim); confirmation that no
   production line changed outside the one doc comment; every open question
   this node does not resolve (Section 9).

## 7. Acceptance criteria

- [ ] `cargo build --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` succeed with zero warnings.
- [ ] `cargo test -p loki-acceptance-store -- --test-threads=1` passes, including both new tests plus every existing test in that crate unmodified.
- [ ] `cargo test --workspace -- --test-threads=1` passes (documented flake class only).
- [ ] `pnpm run verify` passes (one retry allowed on the documented flaky Windows Application Control error, per existing project convention).
- [ ] `git diff --stat` against the starting worktree tip shows changes only to `crates/loki-acceptance-store/tests/acceptance_flow.rs` (additive), `crates/loki-acceptance-store/src/lib.rs` (doc-comment lines only — zero executable-line diff, verifiable by confirming every changed line starts with `///` or `//`), and the handoff.
- [ ] Grep-verifiable: `consumed_at_ms` is never set to `NULL`/reset anywhere in `crates/loki-storage`, `crates/loki-approvals`, or `crates/loki-jobs` (reproduces Section 0 item 2's claim; command recorded in handoff).
- [ ] Grep-verifiable: `crates/loki-jobs/src/**` contains no reference to `ApprovalsRepository`, `local_approvals`, or `loki_storage::approvals` (reproduces Section 0 item 2's claim; command recorded in handoff).
- [ ] The new changed-request test fails if `evaluate_approval_reference`'s digest check is hypothetically removed (verified by the implementer temporarily commenting out that check locally during development and observing the test fail red, then restoring it — not committed, but stated as done in the handoff) — this proves the test actually exercises the property, not a vacuous pass.
- [ ] Independent test-reviewer sign-off confirming both new tests genuinely exercise the named scenarios (not merely passing) and that no existing test's assertions were weakened.
- [ ] Independent security-reviewer sign-off confirming: (a) no production code in `loki-acceptance-store`/`loki-storage`/`loki-approvals`/`loki-jobs` changed; (b) the changed-request rejection path cannot be bypassed by any request shape reachable from `MutationAcceptanceRequest::new`'s existing validation; (c) the doc comment added does not overstate what is proven (no claim of a cross-crate "cancellation reverses approval" guard that doesn't exist as code — only that no reversal path exists).
- [ ] Every open question in Section 9 is recorded as still-open in the handoff.

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `cargo fmt --all -- --check` | exit 0 | command output |
| Static | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | command output |
| Unit/Integration | `cargo test -p loki-acceptance-store -- --test-threads=1` | all pass, including `retry_with_changed_target_after_approval_is_rejected_as_not_consumable` and `job_cancellation_has_no_approval_consumption_interaction` | run summary / test names |
| Structural | `grep -rn "consumed_at_ms" crates/loki-storage crates/loki-approvals crates/loki-jobs` reviewed by hand for any reset/`NULL`-setting statement | none found | grep output pasted into handoff |
| Structural | `grep -rn "ApprovalsRepository\|local_approvals\|loki_storage::approvals" crates/loki-jobs/src` | no matches | grep output pasted into handoff |
| Integration | `cargo test --workspace -- --test-threads=1` | pass (documented flake class only) | run summary |
| Security | independent review per Section 7's security bullet | no finding | reviewer note in handoff |
| Desktop/Cloud/Hardware/Containment | n/a — no IPC, no cloud, no device, no build-executor surface in this node | n/a | n/a |

## 9. Known open questions — do not resolve silently

1. **This packet assumes `P1-ACCEPTANCE-STORE-001`'s current uncommitted
   binding logic (Section 0) is correct as read.** It has not yet passed its
   own independent security/test review at the time this packet was written
   (both remain `proposed`). If that review finds a real defect in the
   approval-consumption/job-creation atomicity this node's tests currently
   assume, this node's tests may need to change shape (or may newly start
   failing, which is itself valuable signal) — not a reason to weaken the new
   tests to make them pass.
2. **AS-007's full scope ("any bound flash input changes after approval")
   is only partially exercised here.** This node's Deliverable 1 covers a
   changed `target_id`. It does not add a second, near-duplicate test for a
   changed `artifact_digest` alone (same target, different artifact) — the
   underlying mechanism (`request.request_digest` recomputation via
   `compute_request_digest` over all bound fields) is identical for either
   field, so this packet judges one representative case sufficient rather
   than a combinatorial matrix; a future hardening/verification node
   (`P1-MUTATION-REV-001`) may choose to broaden this.
3. **This node does not address `P1-AGENT-MUTATION-IDEMP-001`'s scope at
   all** (agent-method digest-bound idempotency across
   `AgentRunRegister`/`AgentRunUpdate`/`WorkIntentAcknowledge`) — that
   remains a fully separate, genuinely parallel node per
   `claudeplanner.md` Section 4.3, unaffected by this node's narrower-than-expected
   scope.
4. **`outcome_unknown` trigger contract remains undefined** — unaffected by
   this node; still open for whichever future node runs a physical effect.

## 10. Risks and safety

- Risk level: `low` — this node adds test coverage and one doc comment only;
  it changes zero executable production code. The residual risk is entirely
  in Section 9 item 1 (dependency on unreviewed predecessor code) and in the
  possibility that Section 0's "already implemented" finding is itself wrong
  in some way not caught by direct reading — mitigated by acceptance
  criterion 7's "test fails red without the property" check.
- Failure modes: none of the seven classes named in prior packets in this
  chain apply meaningfully here beyond (b) unvalidated approval expiry/binding
  — and this node's entire purpose is closing the one remaining gap in that
  class's evidence, not introducing new surface.
- Rollback/recovery: n/a — no schema, no migration, no persisted-data change.
- Data/secret impact: none.
- Concurrency impact: none (no new production code path).
- Cloud/offline impact: none.
- Device impact: none.

## 11. Human gate

- Gate: none required to *start* this node.
- Gate required before this node's acceptance is treated as final: independent
  test-reviewer and security-reviewer sign-off per Section 7, **and** this
  node cannot be marked `accepted`/integrated before both
  `P1-ACCEPTANCE-STORE-001` and `P1-PRINCIPAL-GRANT-001` reach `accepted` and
  integrate first (Section 3).
- Approver role: human product/security owner, on the independent review's
  findings — same pattern as the two predecessor packets in this chain.
- Approval evidence: independent review record under
  `Docs/graph/evidence/W6/P1-APPROVAL-JOB-ACCEPT-001/`.

## 12. Stop conditions

Return `BLOCKED_DEPENDENCY` if the `node/P1-ACCEPTANCE-STORE-001` worktree is
abandoned/reset before this node starts. Return `NEEDS_REPLAN` — not a silent
patch — if, while writing Deliverable 1 or 2, the implementer finds that the
approval-consumption/job-creation binding described in Section 0 does **not**
actually behave as read (e.g., the changed-request test unexpectedly passes
"Accepted" instead of `ApprovalNotConsumable`, or the cancellation test finds
any actual interaction with `storage`) — that would mean Section 0's finding
was wrong and this packet's entire scope-narrowing premise is invalid, which
must return to planning, not be worked around by rewriting the test to match
the unexpected behavior. Return `NEEDS_REPLAN` if either predecessor node's
own independent review forces a code-shape change to
`crates/loki-acceptance-store/src/lib.rs` that this node's new tests were
written against — rebase, do not silently diverge.

## 13. Worker and reviewer

- Accountable worker: `loki-implementer` (one bounded, test-only node —
  narrowest ownership set in this chain by design, per Section 0's finding).
- Independent reviewer: `loki-test-reviewer` (confirms both new tests
  genuinely exercise their named scenarios, per Section 7) and
  `loki-security-reviewer` (confirms zero production-code change and no
  overstated doc-comment claim, per Section 7).
- Worktree/branch: continue in `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001` on
  branch `node/P1-ACCEPTANCE-STORE-001`, or a child branch
  `node/P1-APPROVAL-JOB-ACCEPT-001` stacked on it — orchestrator's choice,
  recorded in the handoff either way (Section 3).
- Handoff record: `Docs/graph/handoffs/P1-APPROVAL-JOB-ACCEPT-001-1.md`.

## 14. Human direction and waivers on record for this node (2026-08-28)

- No new human waiver required to start — this packet's only judgment call is
  the scope-narrowing finding in Section 0, which is a planner scheduling/
  scoping determination made by direct code inspection, not a security/policy
  waiver, and is fully recorded here for audit and for the orchestrator's
  review before dispatch.
- The parallel-with-`P1-AGENT-MUTATION-IDEMP-001` scheduling recommendation
  from `claudeplanner.md` Section 4.3 is unaffected by this scope narrowing — the two
  nodes remain genuinely independent (disjoint file ownership: this node
  touches only `loki-acceptance-store`'s tests/doc comments;
  `P1-AGENT-MUTATION-IDEMP-001` touches agent-method idempotency surfaces).

## 15. Planning provenance

Shaped by `loki-planner` (read-only), 2026-08-28, against the uncommitted
worktree at `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001` (branch
`node/P1-ACCEPTANCE-STORE-001`), by direct inspection of:
`crates/loki-acceptance-store/src/lib.rs` in full;
`crates/loki-acceptance-store/tests/acceptance_flow.rs` in full;
`crates/loki-mutation-contract/src/{request.rs,result.rs}`;
`crates/loki-storage/src/approvals.rs` in full; `crates/loki-jobs/src/engine.rs`;
and a repository-wide grep for `consumed_at_ms`/`ApprovalsRepository`/
`local_approvals` inside `crates/loki-jobs`. The key finding, not present in
`claudeplanner.md` or either predecessor task packet at this resolution: the
scope `claudeplanner.md` Section 2.5 and `TASK_GRAPH.md` assign to this node —
atomically binding approval consumption to job/outbox creation — was already
fully implemented as part of `P1-ACCEPTANCE-STORE-001`'s actual delivered code
(not merely its packet's stated intent), collapsing what the wave plan
(`claudeplanner.md` Section 4.4 Wave 2 vs. Wave 4) assumed would be built across two
sequential waves into one. This packet's scope is narrowed accordingly to the
one concrete, still-missing piece of evidence (AS-007's changed-request
scenario) plus one structural assertion, rather than re-authorizing a second,
redundant pass at already-existing, already-correct binding logic.
