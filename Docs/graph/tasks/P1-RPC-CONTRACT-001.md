# Task Packet — `P1-RPC-CONTRACT-001 Canonical Authenticated Business RPC`

Packet status: `accepted`  
Graph status: `ready`  
Created by: `Codex orchestrator`  
Created at: `2026-08-24T14:58:00Z`  
Starting revision: `827d2a856af71fbd20e6aaf49fca47d8e9289133`

## 1. Objective

Define and implement the canonical, versioned, bounded Rust/TypeScript business
RPC contract required for production Electron/MCP/runtime integration, with
server-created principal context and explicit named methods/streams—never a
generic invoke or arbitrary command surface.

## 2. Traceability

- Requirements: `FR-DEV-*`, `FR-MCP-*`, `FR-MON-*`, `FR-JOB-*`, `FR-APR-*`
- Scenarios: `AS-003-L`, `AS-005-L`, `AS-006`-`AS-010`, `AS-014-L`, `AS-015-L`
- Architecture: authenticated local IPC and shared contracts
- Security: principal authentication, authorization context, bounded payloads
- Parent: `P1-INT-001-CORR-001`

## 3. Dependencies and inputs

- `depends_on`: CTR-001, RUN-002, ADR-0003
- Consumes existing protocol/version/ID/error contracts
- Environment: Windows named-pipe tests; no physical device required

## 4. Required reading

- `AGENTS.md`
- `Docs/ARCHITECTURE.md`, `Docs/SECURITY.md`, `Docs/MCP_SPEC.md`
- `Docs/TESTING.md`, `Docs/graph/INTEGRATION.md`
- P1 runtime audit handoff
- existing contract and `loki-ipc` sources/tests

## 5. Ownership

Writable:

- `packages/contracts/**`
- `crates/loki-ipc/**`
- `Docs/graph/handoffs/P1-RPC-CONTRACT-001-1.md`

Read-only: runtime, Electron, MCP and domain-service implementations.  
Forbidden: consumer implementation, generic method names, arbitrary JSON/shell/
path/URL/environment payloads, self-declared authorization, unbounded streams.

## 6. Deliverables

1. Canonical method/stream table and runtime-validated TS schemas.
2. Rust request/response/event types with positive/negative compatibility fixtures.
3. Authenticated server-created principal/session context distinct from client type.
4. IPC minor-version/capability negotiation and explicit unsupported behavior.
5. Bounded framing/stream/cancellation/deadline tests.

## 7. Acceptance criteria

- [ ] Named read methods cover runtime, devices/health/logs/telemetry, incidents,
      jobs, agent runs, work-intent acknowledgement, health/preflight and cancel.
- [ ] Consequential build/restart/flash/test/deploy/rollback requests are distinct
      request methods returning approval/job state, never execution authority.
- [ ] Approval grant/deny is a desktop-only method class and impossible for agent principals.
- [ ] Streams are named, bounded, cancellable, correlated and slow-consumer safe.
- [ ] Client-declared type/scope never creates authority.
- [ ] Current/previous compatibility and incompatible-version behavior are tested.
- [ ] No generic invoke/raw command/unrestricted path contract exists.
- [ ] No unrelated file changes.

## 8. Required verification

Contract format/lint/type/test; Rust format, warnings-denied Clippy and IPC tests;
real Windows named-pipe current/previous/malformed/replay/deadline/cancellation/
slow-consumer tests; complete repository verify after integration.

## 9. Risks and safety

Risk: critical. Public/cross-process contract changes require version and
compatibility classification. No device operation is performed by this node.

## 10. Human gate

None unless the worker discovers a public compatibility or locked-boundary change
outside the accepted explicit RPC design.

## 11. Worker and reviewer

Accountable worker: isolated RPC contract worker.  
Independent reviewer: assigned after implementation.  
Worktree: assigned after packet integration.

## 12. Stop conditions

Return `NEEDS_REPLAN` if a required operation cannot be expressed without a
generic command/path surface or if server-created principal context requires a
locked architecture change.
