# ADR-0007 — Grant-generation identity unification (DRAFT — human decision required)

Status: `accepted`
Date: `2026-08-28` (drafted by orchestrator from loki-planner's decision brief; accepted same day)
Decision owners: `human product/security owner`

## Context

`P1-ACCEPTANCE-STORE-001` (implemented, pending independent review, not yet
integrated) needs a durable `GrantGeneration` to revalidate on every mutation
acceptance, per ADR-0006 step 1. `crates/loki-mutation-contract::grant`
deliberately defines `GrantGeneration` as a new, independent type and
explicitly declines to assume any relationship to existing session/credential
models — its own doc comment names reconciling that relationship as
`P1-PRINCIPAL-GRANT-001`'s job.

Direct code inspection (by `loki-planner`, 2026-08-28, against the
`P1-ACCEPTANCE-STORE-001` worktree) found the two things that job must
reconcile are more incompatible than `claudeplanner.md` §2.4 anticipated:

- `LOC-001`'s `LocalSession` (`crates/loki-local-authority`): keyed by
  Windows `principal_sid`, short-lived (1s–24h), independently revocable,
  no "generation" concept, many concurrent sessions per principal.
- `AGT-001`'s `AgentCredential` (`crates/loki-agents`): keyed by an opaque
  `connection_id` + one-time secret, **no `principal_sid` field, no
  expiry, no generation, and no rotate/scope-change operation at all
  today.** An agent connection is authorized once at registration using a
  human's `LocalSession`, then lives and revokes entirely independently of
  that session.

`P1-ACCEPTANCE-STORE-001`'s implementer already had to build a narrow,
explicitly-provisional shim (`loki_storage::local_authority::find_grant_generation_core`,
mapping `generation == issued_at_ms` of a session row) just to make progress,
and that shim has no path at all for agent-originated mutations — exactly
what the next node, `P1-AGENT-MUTATION-IDEMP-001`, needs.

Per `AGENTS.md`'s human-approval boundary ("changing authentication,
signing, update, RBAC, or approval policy" requires human approval before
code) and `Docs/graph/PLANNER.md` ("create DECISION nodes for unresolved
choices; do not create implementation nodes whose acceptance depends on an
unresolved decision"), this is a decision node, not a planning detail to
absorb silently into a task packet.

## Options

**A — `GrantGeneration` becomes the sole authoritative row; `LocalSession`/
`AgentCredential` become thin projections that must always resolve to a
live, unrevoked generation.**
Revoking a generation cascades to invalidate every session and every agent
credential built on it, in one transaction. Requires: inventing a principal
identity space covering both Windows SIDs and agent connections (agents have
none today); adding rotate/scope-change operations to `AgentCredential` that
don't exist; a schema/behavior change to two already-accepted, already-
integrated components (`LOC-001`, `AGT-001`).
Strongest security property (single revocation point), largest change
surface, needs a migration/compat story for already-issued sessions/
credentials.

**B — `GrantGeneration` scoped per identity model** (one generation
sequence for Windows-principal sessions, a separate one for agent
connections), unified only at the mutation-acceptance transaction's read
boundary, not at the storage/issuance layer.
`LocalSession`/`AgentCredential` keep issuing/revoking exactly as today;
this node adds an independent `generation` counter to each.
Smaller change to `LOC-001`/`AGT-001`, but only partly satisfies "one
authoritative concept" — mostly formalizes the current provisional shim.

**C — `GrantGeneration` stays request-scoped and ephemeral**: minted per
accepted mutation from whatever session/credential state is live at
acceptance time (generation = the session's/credential's own timestamp or
equivalent monotonic stamp), no separate durable "grant" table.
Cheapest, no new schema, but no real rotate semantics anywhere, and
"restart reloads the correct current generation for every principal"
(`claudeplanner.md` §2.4's acceptance criterion) is only weakly satisfied —
there is no real "current generation," only "whatever the last live token
happened to be."

**D — Process step, not a design option**: land a narrow ADR now deciding
only the identity-unification question (pick A, B, C, or a hybrid) and
state explicitly whether rotate/scope-change for agent credentials is in
`P1-PRINCIPAL-GRANT-001`'s scope or deferred to a new node. This document
*is* that step — the human's answer below fills in the actual decision.

## Recommendation (loki-planner, via orchestrator)

Resolve via D — i.e., use this ADR to pick among A/B/C narrowly, rather than
let an implementer or planner infer it. A and B both require changing
already-accepted `LOC-001`/`AGT-001` behavior, which independently triggers
`AGENTS.md`'s human-approval requirement regardless of how any task packet
is scoped — so this cannot be delegated further down the pipeline.

No default is silently assumed. Every downstream node in the remaining
Part One feature chain (`P1-APPROVAL-JOB-ACCEPT-001`,
`P1-AGENT-MUTATION-IDEMP-001`, and everything sequenced after them through
`P1-RUNTIME-E2E-001`) is blocked behind this decision — see
`claudeplanner.md` §1.2 steps 5–14.

## Decision

**Option B.** `GrantGeneration` is scoped per identity model: one generation
counter/sequence for Windows-principal (`LocalSession`) grants, a separate
one for agent (`AgentCredential`) grants. `LocalSession` and `AgentCredential`
keep their existing issuance/revocation mechanics unchanged; each gets an
additive `generation` counter that increments on issue/rotate/revoke.
`P1-ACCEPTANCE-STORE-001`'s mutation-acceptance transaction (and any future
caller needing "is this principal's grant still current") reads whichever
generation sequence matches the request's principal type at its own read
boundary — no shared storage-layer/issuance-layer merge between the two
models.

Rationale (human, recorded via chat): smaller, lower-risk change than Option
A (which would require rewriting two already-accepted, already-integrated
components, `LOC-001` and `AGT-001`); still delivers a real, durable
generation counter unlike Option C's ephemeral timestamp-as-generation
placeholder, which was explicitly flagged as fragile (collision risk on
identical timestamps) and not fit to keep as a permanent design.

**Scope clarification for `P1-PRINCIPAL-GRANT-001`:** rotate and
scope-change operations for `AgentCredential` do not exist today (per
loki-planner's finding) and the node's own title ("issue, rotate, scope
change, expiry and revoke") requires them. These are **in scope** for
`P1-PRINCIPAL-GRANT-001` to add as new `AgentCredential` methods — this is
an additive capability gap being filled, not a redesign of `AGT-001`'s
existing accepted behavior, and does not itself require a further ADR.

## Consequences

- `crates/loki-local-authority` and `crates/loki-agents` each gain an
  additive `generation` column/counter and (for agents) new rotate/
  scope-change methods; no existing accepted behavior of either crate
  changes.
- `P1-ACCEPTANCE-STORE-001`'s provisional shim
  (`loki_storage::local_authority::find_grant_generation_core`, keyed on
  `issued_at_ms`) is replaced by a real generation-counter read as part of
  `P1-PRINCIPAL-GRANT-001` — expected and already flagged as provisional by
  that node's own doc comment.
- No shared "one kill switch covers both humans and agents" property is
  established at the storage layer — revoking a human's session does not
  automatically revoke agent credentials issued because of that session
  (that would be Option A's property). If that stronger guarantee is
  needed later, it is a distinct future decision, not assumed here.
- `P1-AGENT-MUTATION-IDEMP-001` (the node after this one) can now rely on a
  real agent-side generation counter instead of having no path at all, as
  loki-planner's finding noted was missing before this decision.
