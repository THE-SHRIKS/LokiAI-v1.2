# ADR-0006 — Transactional runtime mutation acceptance

Status: `accepted`  
Date: `2026-08-24` (proposed); accepted `2026-08-25`  
Decision owners: `human product/security owner; orchestrator`

## Context

Independent RPC REV-004 proved that IPC cannot make credential revalidation and
an arbitrary service-owned database or physical side effect atomic. A callback,
flag, or process-local authorization lease remains cooperative: service code can
mutate before it, delay after it, or hold device/database authority outside it.
Post-call revalidation can suppress a response but cannot undo a side effect.

The locked architecture already assigns durable jobs, approvals, persistence,
leases, execution staging, cancellation, and recovery to the runtime. The wire
contract must therefore adapt to an enforceable runtime transaction rather than
pretend that IPC owns mutation atomicity.

## Decision

The runtime will own one durable acceptance serialization point. Before a
consequential command or durable agent mutation is accepted, one encrypted
SQLite transaction must:

1. revalidate the opaque principal credential and exact durable grant generation;
2. bind method, full canonical request digest, idempotency key, target,
   artifact, policy, and approval inputs (workspace/scopes binding is
   deferred — see amendment below);
3. claim or replay the idempotency record, rejecting same-key/different-digest;
4. validate and consume the exact approval when required;
5. create or retrieve the durable job/command or agent mutation;
6. append its transactional outbox record; and
7. commit before returning any accepted result.

No connector, device, build, deployment, or other external effect runs inside
that transaction. Durable workers act only after commit and revalidate current
authority/policy before each not-yet-started consequential stage.

Revocation or expiry before commit denies acceptance. Revocation after commit
does not erase the audit record or pretend the commit never happened. It prevents
not-yet-started stages, requests cancellation at declared safe points for active
work, and requires reconciliation. An irreversible or uncertain physical result
is recorded as `outcome_unknown`/quarantined—not guessed cancelled or successful.

The still-unreleased business RPC v1.1 result semantics will be replaced before
any released consumer. Mutation results become discriminated states with no
contradictory nullable combinations. Read results bind the exact query digest,
workspace/scope, cursor/filter/limit, requested signals, source, and correlation.

## Consequences

- IPC retains authentication, framing, bounded dispatch, dynamic read/stream
  revalidation, and typed transport validation, but is not final mutation authority.
- Approval consumption and job/outbox acceptance can no longer be separate commits.
- Principal issue/rotation/revocation/scope changes use durable grant generations
  visible inside the same acceptance transaction.
- `AgentRunRegister`, `AgentRunUpdate`, and `WorkIntentAcknowledge` receive full
  request-digest idempotency through the same serialization boundary.
- Runtime/service implementation precedes final public RPC adapter acceptance.
- Physical cancellation is checkpointed recovery behavior, never atomic rollback.

## Rejected alternatives

- IPC `AtomicBool`, callback, closure, mutex, or lease as the commit boundary.
- Pre-call plus post-call credential checks around arbitrary service code.
- Consuming approval and submitting a job in separate transactions.
- Treating post-commit revocation as deletion or rollback of accepted work.
- Executing device/build/deploy effects inside the acceptance transaction.

## Required evidence

- Revocation/expiry racing acceptance yields exactly one legal serialized result.
- Same-key/same-digest returns the original committed result; changed digest conflicts.
- Fault injection after every transaction statement produces no orphan approval,
  job, mutation, or outbox record.
- No external command starts before commit.
- Post-commit revoke/cancel/reconcile behavior matches this decision.
- Independent mutation-boundary and RPC reviews have no critical/high finding.

## Human decision

**Accepted.** The human product/security owner (shrustixrajput@gmail.com)
approved this ADR as drafted, in chat, 2026-08-25:

> "okay i approve go for it after completion of 1 use start using graph
> enginerring and subagents from 2-5"

recorded by the orchestrator (Claude Code) against the exact requested approval
text this ADR asked for (see revision history / prior "Pending" text this
paragraph replaces): full transactional runtime mutation acceptance design as
specified in the "Decision" section above, and replacement of the unreleased
business RPC v1.1 result semantics.

This decision does not approve any device operation, VHDX/elevation, VM run,
credential access, C-drive/system change, or Windows account/admin/host-policy
change. Those gates remain separate and were not addressed by this approval.

## Amendment 1 — workspace/scopes binding deferred (2026-08-25)

Human product/security owner, in chat, approved dropping "workspace" (and
"scopes") from step 2's binding requirement for all of Part One, rather than
inventing a placeholder value: no local workspace concept exists (workspace
is a cloud/account concept, deferred by `ADR-0002`), so there is nothing real
to bind yet. Mutation records carry no workspace field locally. When cloud
sync (`CLD-002`) is built, it attaches the real workspace to historical local
records retroactively at that point — the same pattern already used for
device identity (`LDEV-001` local-only now, `ENR-001` cloud ownership later).
`P1-MUTATION-CONTRACT-001`'s implementation (`crates/loki-mutation-contract`)
already reflects this: its digest/request types omit workspace entirely.
Whichever future node introduces workspace scoping must either fold workspace
into the digest (a breaking change) or scope the idempotency-record lookup by
workspace independent of the digest — this is not yet decided and must not be
silently resolved when that node starts.
