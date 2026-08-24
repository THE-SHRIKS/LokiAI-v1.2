# Task Packet — `P1-GRAPH-AUD-001 Part One Production DAG Audit`

Packet status: `accepted`  
Graph status: `running`  
Created by: `Codex orchestrator`  
Created at: `2026-08-24T14:47:13Z`  
Starting revision: `827d2a856af71fbd20e6aaf49fca47d8e9289133`

## 1. Objective

Reconstruct the authoritative cloud-deferred Part One production DAG after the
invalid broad acceptance of `P1-INT-001`, identifying every required correction
node, dependency, human gate, and evidence requirement without changing code or
silently moving Part Two work into Part One.

## 2. Traceability

- Requirements: all Part One W0-W6 requirements
- Acceptance scenarios: Part One portions of `AS-003` through `AS-016`
- Architecture components: graph, runtime, connector, MCP, build and deployment
- Security controls: all Part One controls
- Parent: `P1-INT-001-CORR-001`

## 3. Dependencies and inputs

- `depends_on`: `P1-INT-001` audit finding
- Required environment: repository read access only
- Known condition: current gate claims services run together although production
  runtime and MCP paths do not compose them.

## 4. Required reading

- `AGENTS.md`
- `Docs/graph/README.md`
- `Docs/graph/PLANNER.md`
- `Docs/graph/TASK_GRAPH.md`
- `Docs/graph/INTEGRATION.md`
- `Docs/graph/TRACEABILITY.md`
- `Docs/PRD.md`, `Docs/SPEC.md`, `Docs/TESTING.md`
- `Docs/graph/decisions/ADR-0002-local-only-part-one.md`

## 5. Ownership

Writable paths: none. Return findings to the orchestrator only.  
Read-only: repository.  
Forbidden: code/doc edits, status changes, Part Two dispatch, cloud implementation.

## 6. Deliverables

1. Corrected Part One scope and DAG.
2. Exact correction nodes with dependencies and ownership.
3. Contradictions requiring ADR or human acceptance.
4. Binary completion criteria and required evidence.

## 7. Acceptance criteria

- [ ] Every W0-W6 requirement is accounted for as accepted, correction, blocked,
      or explicitly cloud-deferred.
- [ ] No package test is treated as cross-process integration evidence.
- [ ] Human gates and device safety boundaries are explicit.
- [ ] No repository file changes.

## 8. Required verification

| Layer | Scenario | Expected result | Evidence |
|---|---|---|---|
| Static | Cross-check graph against normative docs | no omitted Part One capability | cited findings |
| Integration | Validate dependency order | acyclic executable frontier | proposed DAG |
| Security | Check all retained controls | no waived boundary | cited findings |

## 9. Risks and safety

Risk: high; an incorrect DAG can falsely certify production. No device/data impact.

## 10. Human gate

Gate: any locked-boundary ADR discovered by the audit.

## 11. Worker and reviewer

Accountable worker: independent graph audit agent.  
Independent reviewer: orchestrator.  
Worktree: read-only shared baseline.

## 12. Stop conditions

Return `NEEDS_REPLAN` for contradictory authoritative requirements that cannot be
resolved by source-of-truth precedence.
