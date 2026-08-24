# ADR-0003 — Corrected production Part One scope and evidence boundary

Status: `accepted`  
Date: `2026-08-24`  
Decision owners: `human product owner; orchestrator`  
Supersedes: `P1-INT-001`'s broadened exclusions, not ADR-0002's cloud decision

## Context

The first `P1-INT-001` record treated aggregate package tests as proof that the
local services ran together and excluded contained builds, artifacts, device
operations, deployment, exact-board work, and blocked DAT-002 evidence. A
production audit proved that Electron and MCP still return
`runtime_unavailable`, the runtime does not compose the accepted domain
services, connector discovery is mock-backed, and mandatory Part One nodes
remain blocked or proposed.

The human owner has now explicitly required Part One to be completed as a real,
production-grade, end-to-end local product, using graph engineering, real
integrations, independent review, and no mock/simulation evidence. The earlier
instruction to defer cloud remains in force.

## Decision

Part One is every non-cloud W0-W6 capability in the original graph:

- secure Electron main/preload boundary and authenticated runtime IPC;
- encrypted local persistence and Windows-bound local authority;
- real runtime composition, connector discovery/identity/health, MCP, agents,
  monitoring, incidents, notifications, durable jobs, policy and approvals;
- contained builds, immutable artifacts, consequential device operations,
  verification, recovery, and deployment services;
- real cross-process and narrowly scoped physical evidence required by the
  scenarios below.

ADR-0002 continues to defer only cloud contracts, cloud authentication,
workspace/enrollment/onboarding, cloud synchronization, remote notifications,
and remote provider wake/delivery. Cloud absence must fail closed.

Renderer route wiring, the complete release-wide HIL suite, packaging, updates,
release-wide security gates, and release remain Part Two.

## Named local scenario variants

- `AS-003-L`: Windows-bound local owner session -> discover the confirmed
  ESP32-S3 -> stable identify -> local bind -> initial health. Minimum L1-L6.
- `AS-005-L`: locally registered/revocable agent -> MCP capability negotiation
  -> authorized read/diagnostic request -> real runtime result. Minimum L1-L4;
  positive device evidence uses `AS-003-L`'s physical connector.
- `AS-014-L`: real local runtime/device state -> deduplicated, redacted local
  notification intent -> durable read/expiry state. Minimum L1-L5.
- `AS-015-L`: real incident -> scoped local agent work-intent pull/claim/ack or
  durable human fallback. Minimum L1-L5.

`AS-006`, `AS-008`, `AS-009`, `AS-012`, `AS-013`, and `AS-016` are not partial:
their documented operation, containment, recovery, persistence, and required
physical evidence remain mandatory for Part One. `AS-001`, `AS-002`, `AS-011`,
and cloud/remote portions of `AS-014`/`AS-015` remain deferred.

Part One physical evidence is the minimum L6 subset necessary to prove the
confirmed connector and real operations for `AS-003-L`, `AS-006`, and `AS-008`.
It does not replace Part Two's broader release HIL matrix.

## Required gates

1. No required Part One dependency may be blocked, deferred, or merely integrated.
2. Package tests cannot substitute for real cross-process, containment, or
   physical evidence.
3. Exact-board flash/recovery work waits for OQ-001.
4. Every device mutation requires exact operation/target/artifact/effect/expiry
   approval.
5. DAT-002 and BLD-001 require independent security acceptance.
6. `P1-INT-001-R1` supersedes the invalid broad acceptance only after the
   corrected system-integration node passes with zero mandatory skips.

## Human acceptance

- Decision: `accepted`
- Actor: `human product owner`
- Timestamp: `2026-08-24`
- Evidence: explicit instruction to build Part One fully end-to-end at
  production quality, follow the repository graph, use independent agents, and
  verify every task without mocks, toy work, or hallucinated success; earlier
  explicit cloud deferral retained.
