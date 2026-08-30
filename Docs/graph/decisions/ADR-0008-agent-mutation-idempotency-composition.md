# ADR-0008 — Agent-mutation idempotency: two-call composition, not a contract change

Status: `accepted`
Date: `2026-08-28`
Decision owners: `human product/security owner`

## Context

`P1-AGENT-MUTATION-IDEMP-001` needs the generic ADR-0006 acceptance
transaction (already built by `P1-ACCEPTANCE-STORE-001`) to connect to the
actual domain effect for the three agent methods (`AgentRunRegister`,
`AgentRunUpdate`, `WorkIntentAcknowledge`) — creating/transitioning an
`agent_runs` row, acknowledging a work intent — inside one atomic step.

`loki-mutation-contract`'s request/result types (`MutationAcceptanceRequest`,
`AcceptedRecord`/`AcceptanceResult`) are generic and payload-agnostic by
design, and were deliberately hardened (private fields, closed constructors)
by an earlier security review specifically so nothing outside that crate can
construct an accepted result. They cannot currently carry or return the
method-specific data these three operations need.

loki-planner (2026-08-28) found this is a real design fork, not an
implementation detail, and also found a genuine pre-existing bug along the
way: `AgentRegistry::finish_run` has no idempotency — a legitimate retry of
an already-completed transition returns an error (`AgentError::TerminalRun`)
instead of the original success.

## Options considered

**A — Add method-specific payload/result fields to `loki-mutation-contract`.**
Strongest single-transaction guarantee, but reopens an already-hardened,
already-reviewed pure contract crate for the third time in this chain,
raising its own review burden each time.

**B — Two-call composition**: call the existing generic `accept_mutation`
first (idempotency/grant/digest gate, unchanged), then a separate call into
`AgentRegistry`/`WorkIntentDeliveryStore` to perform the real effect, with a
proven crash-safety invariant across the two non-atomic calls. Requires
fixing `finish_run`'s retry bug as a precondition. Smaller change, no
contract-crate edit, and fixes a real correctness bug along the way.

**C — Make these three methods asynchronous** (mirroring how approval-gated
device operations defer real work to a post-commit execution engine).
Would be a synchronicity/behavior change to a surface that has always
returned its result synchronously — likely itself triggers `AGENTS.md`'s
human-approval boundary for a protocol/compatibility change, and no existing
document proposes this.

## Decision

**Option B.** Keep `loki-mutation-contract` untouched. `loki-acceptance-store`'s
existing generic `accept_mutation` stays the idempotency/grant/digest gate
for these three methods, exactly as already built. `P1-AGENT-MUTATION-IDEMP-001`'s
real, narrowed scope is:

1. Fix `AgentRegistry::finish_run`'s retry bug (a legitimate retry of an
   already-completed, identical transition returns the original success, not
   `AgentError::TerminalRun`).
2. Prove — with real tests, not sequential-only assertions — that the
   two-call composition (`accept_mutation` then the real
   `AgentRegistry`/`WorkIntentDeliveryStore` call) is crash-safe: a crash
   between the two calls never leaves the system in a state where a retry
   produces a different, inconsistent, or duplicated outcome than the
   original attempt would have.
3. Add `P1-ACCEPTANCE-STORE-001` as an explicit `depends_on` for this node in
   `TASK_GRAPH.md` (currently absent, which is what let this gap go
   undetected until now).

Rationale (human, recorded via chat): smallest, lowest-risk change; does not
reopen the hardened contract crate a third time; incidentally fixes a real,
independently-found correctness bug.

## Consequences

- `loki-mutation-contract` remains untouched through this entire chain so
  far (`P1-ACCEPTANCE-STORE-001`, `P1-PRINCIPAL-GRANT-001`, `P1-APPROVAL-JOB-ACCEPT-001`,
  now `P1-AGENT-MUTATION-IDEMP-001`) — a deliberately narrow, well-guarded
  surface. **See Amendment 1 below — this premise was invalidated by a
  separate, human-authorized correction the day after this ADR was
  accepted.**
- The two-call composition's crash-safety is a property this node must
  actively prove (real process-kill/panic-between-calls tests), not assume
  from the individual calls' own internal atomicity.
- `TASK_GRAPH.md`'s `P1-AGENT-MUTATION-IDEMP-001` row gains
  `P1-ACCEPTANCE-STORE-001` as an explicit dependency, correcting a real gap
  the planner found in the existing graph.
- If a future need arises for true single-transaction agent-mutation
  semantics (Option A's property), that is a distinct future decision, not
  assumed here.

## Amendment 1 — `loki-mutation-contract` was touched by a separate,
   human-authorized correction (2026-08-29)

This ADR's "remains untouched" premise (§Consequences, first bullet) no
longer holds as stated. On 2026-08-29, one day after this ADR was accepted,
commit `7535e08` ("fix: unify approval mutation operation binding") — a
separate, human-authorized correction addressing two HIGH security findings
in the approval/operation-binding chain (`P1-APPROVAL-OPERATION-BINDING-CORR`,
not a node this ADR governs) — substantially rewrote
`crates/loki-mutation-contract/src/{digest.rs,request.rs,result.rs,lib.rs}`
(305 insertions / 150 deletions), removing `compute_request_digest` and
adding a new `OperationBinding` concept to close a real approval/mutation
digest mismatch the correction discovered. This was independently reviewed
(security review PASS, no Critical/High findings on the correction itself)
and integrated as part of that separate node's own gate discipline — see
`Docs/graph/handoffs/P1-APPROVAL-OPERATION-BINDING-CORR-1.md`.

This does not invalidate this ADR's actual DECISION (Option B — two-call
composition, no `loki-mutation-contract` change *by this node*) — the
`P1-AGENT-MUTATION-IDEMP-001` work itself (commits `988bff0`/`f15a526`)
genuinely made zero changes to that crate, confirmed by an independent
security review (`git diff --stat` empty for that crate across those
commits). What changed is the crate's *baseline state* underneath this
node, via a concurrent, independently-authorized workstream — not a
violation of this ADR's own scope discipline.

**Consequence for future work touching `loki-mutation-contract`**: the
"deliberately narrow, well-guarded surface, reopened multiple times" framing
in this ADR's original rationale is now stale — the crate has in fact been
reopened by a workstream this ADR did not track. Any future node touching
this crate should verify its current shape directly (per this project's
established practice — see `P1-PRINCIPAL-GRANT-001`'s planner notes on the
same principle) rather than trusting either this ADR's or any other
document's narrative about what the crate currently contains.

Recorded per explicit human instruction, 2026-08-29: "Do not silently claim
compliance with that obsolete statement. Record the
correction/supersession accurately after the implementation is
independently reviewed."
