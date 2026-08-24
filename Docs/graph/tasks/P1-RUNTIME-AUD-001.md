# Task Packet — `P1-RUNTIME-AUD-001 Production Composition Audit`

Packet status: `accepted`  
Graph status: `running`  
Created by: `Codex orchestrator`  
Created at: `2026-08-24T14:47:13Z`  
Starting revision: `827d2a856af71fbd20e6aaf49fca47d8e9289133`

## 1. Objective

Determine the smallest production correction sequence that composes the accepted
Rust services in the shipped runtime and connects authenticated business IPC to
Electron and MCP, replacing production `runtime_unavailable` defaults without
weakening authorization or inventing device evidence.

## 2. Traceability

- Requirements: `FR-DEV-*`, `FR-MCP-*`, `FR-MON-*`, `FR-JOB-*`, `FR-APR-*`
- Acceptance scenarios: `AS-003`-`AS-010`, `AS-014`, `AS-015`
- Architecture: Rust runtime, local IPC, Electron main, MCP server
- Security: renderer distrust, authenticated IPC, MCP scope, approval boundary
- Parent: `P1-INT-001-CORR-001`

## 3. Dependencies and inputs

- `depends_on`: accepted local library nodes
- Environment: repository read access only
- Known condition: runtime composes only storage and IPC; Electron/MCP return
  `runtime_unavailable` in production.

## 4. Required reading

- `AGENTS.md`
- `Docs/ARCHITECTURE.md`, `Docs/SECURITY.md`, `Docs/MCP_SPEC.md`
- `Docs/CONNECTOR_SPEC.md`, `Docs/TESTING.md`
- runtime, IPC, Electron, MCP, and accepted local-service manifests/sources

## 5. Ownership

Writable paths: none. Return findings only.  
Forbidden: implementation edits, contract changes, mock-backed production plans.

## 6. Deliverables

1. Exact missing runtime composition and RPC surfaces.
2. Node decomposition with non-overlapping file ownership.
3. Compatibility, authorization, migration, and failure requirements.
4. End-to-end verification scenarios.

## 7. Acceptance criteria

- [ ] Every production unavailable/default-deny path is traced to its missing producer.
- [ ] Proposed nodes preserve one contract writer and fail closed.
- [ ] Tests include real process boundaries, not injected fake data sources.
- [ ] No repository file changes.

## 8. Required verification

Read-only source and manifest dependency inspection with cited evidence.

## 9. Risks and safety

Risk: critical; IPC/MCP mistakes can grant host or device authority. No device action.

## 10. Human gate

None unless the audit finds a locked architecture change is required.

## 11. Worker and reviewer

Accountable worker: independent runtime integration audit agent.  
Reviewer: orchestrator.  
Worktree: read-only shared baseline.

## 12. Stop conditions

Return `NEEDS_REPLAN` if production composition requires a locked-boundary change.
