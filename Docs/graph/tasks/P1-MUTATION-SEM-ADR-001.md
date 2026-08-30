# Task Packet — `P1-MUTATION-SEM-ADR-001 Runtime Mutation Semantics`

Packet status: `accepted`  
Graph status: `accepted`  
Created by: `Codex orchestrator`  
Created at: `2026-08-24T19:30:00Z`  
Starting revision: `7a1115f5b291c71c4ffb0e13deee8d1af9654268`

## 1. Objective

Obtain a human decision on the enforceable transaction, idempotency, approval,
revocation, cancellation, and uncertain-physical-outcome semantics required to
replace the failed cooperative IPC mutation boundary.

## 2. Traceability

- Requirements: `NFR-SEC-002`, `NFR-SAFE-001`, `NFR-REL-001`
- Scenarios: `AS-006`-`AS-010`, `AS-015-L`
- Architecture: runtime-owned jobs, approvals, persistence, execution staging
- Security: credential revocation, approval consumption, audit, fail-closed effects
- Parent: `P1-RPC-CONTRACT-001` REV-004 `NEEDS_REPLAN`

## 3. Dependencies and inputs

- `depends_on`: `ADR-0003`, `CTR-001`, `RUN-002`, `LOC-001`, `AGT-001`,
  `APR-001`, `JOB-001`, `AGT-002`
- Inputs: RPC REV-003/004 evidence and actual approval/job/agent store boundaries
- Environment/device: none; decision work is read-only

## 4. Required reading

- `AGENTS.md`; `Docs/ARCHITECTURE.md`; `Docs/SECURITY.md`; `Docs/TESTING.md`
- current P1 RPC packet/handoff and REV-004 findings
- focused local-authority, approvals, jobs, agents, delivery, storage code

## 5. Ownership

Writable: ADR-0006, this packet, decision handoff, focused graph status record.
Read-only: all product code. Forbidden: implementation, protocol release, host or
device operation, and silent resolution of revocation/cancellation semantics.

## 6. Deliverables

1. Threat- and failure-aware ADR-0006.
2. Explicit pre/post-commit revocation and physical cancellation semantics.
3. Approval for replacing the still-unreleased RPC v1.1 result contract.
4. Acyclic implementation/review frontier and decision handoff.

## 7. Acceptance criteria

- [x] One enforceable runtime transaction owns durable acceptance. (decided —
  ADR-0006 "Decision" section; not yet implemented, see follow-on nodes)
- [x] Approval, job/mutation, idempotency, grant generation, and outbox cannot
  split. (decided — same transaction, ADR-0006 steps 3-6)
- [x] External effects occur only after commit through durable stages. (decided
  — ADR-0006 "No connector, device, build, deployment, or other external effect
  runs inside that transaction")
- [x] Revocation/cancellation/outcome-unknown behavior is explicit. (decided —
  ADR-0006 "Consequences" and rejected-alternatives sections)
- [x] Human accepts the exact security/public-contract semantics. (2026-08-25,
  shrustixrajput@gmail.com, in chat — see ADR-0006 "Human decision")
- [x] No host/device/system operation occurs. (none performed; this node was
  decision/documentation only)

This is a `DECISION` node: acceptance criteria are satisfied by the ADR
establishing these properties and the human accepting them, not by running
code. Implementation of the decided semantics is explicitly out of this node's
scope and is tracked by the follow-on nodes this packet's Section 4 deliverable
("acyclic implementation/review frontier") names — see `unblocked_nodes` in the
handoff and `TASK_GRAPH.md`.

## 8. Required verification

Independent architecture/security review of the ADR against actual code and
graph dependencies; `git diff --check`; exact attributed human decision record.

## 9. Risks and safety

Risk: `critical`. A weak decision permits authorization races, consumed approvals
without jobs, duplicate effects, or false cancellation/success. This node changes
no data, executable, host, cloud, or device state.

## 10. Human gate

- Gate: exact ADR-0006 decision
- Approver: human product/security owner
- Evidence: attributed approval transcript and accepted ADR revision

## 11. Worker and reviewer

- Worker: orchestrator decision author
- Reviewer: independent mutation architecture/security reviewer
- Worktree: main documentation baseline only
- Handoff: `Docs/graph/handoffs/P1-MUTATION-SEM-ADR-001-1.md`

## 12. Stop conditions

Return `BLOCKED_DEPENDENCY` if the human decision is absent. Return `NEEDS_REPLAN`
if authority, approval, durable acceptance, and outbox cannot share one encrypted
transaction or if physical cancellation is represented as atomic rollback.
