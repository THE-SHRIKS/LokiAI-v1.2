# Task Packet — `BLD-ARCH-ADR-001 Containment Workspace Replan`

Packet status: `accepted`  
Graph status: `running`  
Created by: `Codex orchestrator`  
Created at: `2026-08-24T15:00:00Z`  
Starting revision: `29770b5`

## 1. Objective

Produce an evidence-based replacement containment-workspace design that lets the
real AppContainer/Job Object executor run hostile fixtures without per-job ACL
mutation of an existing volume root, while retaining Win32k disable, no network,
no secret/device authority, typed recipes, immutable inputs, resource limits,
process-tree containment, attestation and teardown.

## 2. Traceability

- Requirements: `FR-BLD-001`, `FR-BLD-002`, `NFR-SAFE-001`
- Scenarios: `AS-012`, `AS-013`
- Architecture: separate contained build executor
- Security: AppContainer, Job Object, ACL, Win32k and hostile L8 matrix
- Parent: `BLD-001-CORR-001-R1`

## 3. Dependencies and inputs

- `depends_on`: R1 `NEEDS_REPLAN` handoff
- Environment: read-only main and BLD-001 worktree inspection
- Known condition: per-job traverse ACEs reach `L:\` and time out; R1 forbids
  existing-drive root mutation.

## 4. Required reading

- `AGENTS.md`
- `Docs/ARCHITECTURE.md`, `Docs/SECURITY.md`, `Docs/BUILD_EXECUTION.md`, `Docs/TESTING.md`
- all BLD packets/handoffs and the actual BLD worktree sources/tests

## 5. Ownership

Writable paths: none. Return design/review findings to the orchestrator.  
Forbidden: code/ACL/VM/security-setting changes, weaker group access,
unrestricted fallback, mocks as containment evidence.

## 6. Deliverables

1. Root cause and invariant analysis.
2. At least two viable production designs with privilege, operational, rollback,
   performance and attack-surface tradeoffs.
3. Recommended design and exact BLD-001-R2 node/acceptance matrix.
4. Human decision wording required before implementation.

## 7. Acceptance criteria

- [ ] Recommended design performs no per-job ACL mutation on an existing volume root.
- [ ] One job cannot read/write another job or host/toolchain/user data.
- [ ] Existing AppContainer/Job/Win32k/no-network/handle/resource controls remain.
- [ ] Installation/provisioning and crash cleanup are explicit and reversible.
- [ ] Real hostile/canary evidence is mandatory.

## 8. Required verification

Read-only source/docs/Windows ACL evidence review. No production claim.

## 9. Risks and safety

Risk: critical. No host ACL, device, virtualization or security mutation is authorized.

## 10. Human gate

Human architecture/security acceptance of the exact recommended design is
required before `BLD-001-R2` implementation.

## 11. Worker and reviewer

Accountable worker: independent containment architecture agent.  
Reviewer: orchestrator, followed by independent implementation security reviewer.  
Worktree: read-only main and `L:\LokiAI-v1.2-wt\BLD-001`.

## 12. Stop conditions

Return `NEEDS_REPLAN` if no design satisfies all retained controls without an
unaccepted locked-boundary change.
