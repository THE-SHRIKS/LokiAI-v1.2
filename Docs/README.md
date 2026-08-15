# LokiAI engineering knowledge base

Status: **Normative index**  
Product: **LokiAI Main v1.2**  
Updated: **2026-08-15**

This directory is the source system for specification-driven, graph-executed
engineering. The codebase must implement these documents; the documents must be
updated when accepted product intent changes.

## Read order

| Order | Document | Purpose |
|---:|---|---|
| 1 | [`CONTEXT.md`](CONTEXT.md) | Current facts, decisions, constraints, and unresolved inputs |
| 2 | [`PRD.md`](PRD.md) | Product outcomes, users, scope, requirements, and success measures |
| 3 | [`SPEC.md`](SPEC.md) | Precise behavior, state machines, entities, events, and acceptance rules |
| 4 | [`ARCHITECTURE.md`](ARCHITECTURE.md) | Processes, trust boundaries, data flow, connector system, and repository target |
| 5 | [`TECH_STACK.md`](TECH_STACK.md) | Reviewed 2026 platform, versions, packaging, dependency, and migration baseline |
| 6 | [`SECURITY.md`](SECURITY.md) | Threat model, permissions, approvals, secrets, artifacts, and Electron controls |
| 7 | [`CLOUD_CONTRACT.md`](CLOUD_CONTRACT.md) | Required website/control-plane APIs, sync, enrollment, notifications, and agent delivery |
| 8 | [`BUILD_EXECUTION.md`](BUILD_EXECUTION.md) | Isolated firmware build boundary, recipe, containment, provenance, and tests |
| 9 | [`UI_SPEC.md`](UI_SPEC.md) | Desktop interaction, navigation, brand tokens, states, and accessibility |
| 10 | [`MCP_SPEC.md`](MCP_SPEC.md) | Agent-facing resources, tools, result contracts, and client setup boundary |
| 11 | [`CONNECTOR_SPEC.md`](CONNECTOR_SPEC.md) | Multi-device connector contract and ESP32-S3 certification rules |
| 12 | [`graph/README.md`](graph/README.md) | Single graph-engineering workspace for Claude, Codex, and other agents |
| 13 | [`graph/PLANNER.md`](graph/PLANNER.md) | How the orchestrator creates and schedules the dependency graph |
| 14 | [`graph/TASK_GRAPH.md`](graph/TASK_GRAPH.md) | Version 1.2 execution DAG, ownership, dependencies, and gates |
| 15 | [`graph/INTEGRATION.md`](graph/INTEGRATION.md) | Contract ownership, worker handoffs, merge order, and compatibility rules |
| 16 | [`TESTING.md`](TESTING.md) | Required verification layers and evidence |
| 17 | [`RELEASE.md`](RELEASE.md) | Packaging, signing, rollout, rollback, and human release gate |
| 18 | [`graph/TRACEABILITY.md`](graph/TRACEABILITY.md) | Requirement-to-task-to-test coverage map |
| 19 | [`graph/PROMPTS.md`](graph/PROMPTS.md) | Copy-ready orchestrator and worker prompts |
| 20 | [`RESEARCH.md`](RESEARCH.md) | Why this engineering method was selected and primary references |

## Graph workspace

- [`graph/templates/TASK_PACKET.md`](graph/templates/TASK_PACKET.md) — immutable
  contract for one worker node.
- [`graph/templates/WORKER_HANDOFF.md`](graph/templates/WORKER_HANDOFF.md) —
  evidence returned by a worker.
- [`graph/templates/ADR.md`](graph/templates/ADR.md) — architectural decision
  record.
- [`graph/templates/CHANGE_SPEC.md`](graph/templates/CHANGE_SPEC.md) — product or
  contract change proposal.
- [`graph/tasks/README.md`](graph/tasks/README.md) — task-packet registry and
  just-in-time packet policy.
- [`graph/handoffs/README.md`](graph/handoffs/README.md) — structured worker-result
  registry.
- [`graph/evidence/README.md`](graph/evidence/README.md) — verification-evidence
  location and retention rules.

## Agent role adapters

The canonical behavior is provider-neutral in `AGENTS.md` and `Docs/`. Claude
Code-compatible project roles are provided under `.claude/agents/` for planning,
implementation, test review, security review, and integration. Codex and other
orchestrators should apply the same role contracts through their native delegation
mechanism. Those role adapters remain in `.claude/agents/` because Claude
discovers them there; all project-specific execution state belongs in
`Docs/graph/`.

## Document classes

- **Normative:** `AGENTS.md`, PRD, spec, accepted ADRs, architecture, technology
  stack, security, cloud contract, build-execution boundary, testing, and release
  rules. Implementations must comply.
- **Operational:** planner, task graph, integration process, traceability, and
  prompts. These control execution and are updated as work advances.
- **Informative:** context and research. They explain why decisions exist but do
  not override normative documents.

## Change policy

- Requirements have stable IDs. Never renumber an accepted requirement; deprecate
  it and add a replacement.
- A changed behavior must update the PRD/spec, task graph, acceptance tests, and
  traceability in the same planning change.
- A changed process boundary, security control, persistence strategy, public
  contract, or supported platform requires an ADR.
- The orchestrator may update task status and evidence links. Workers may propose
  graph changes in handoffs but do not schedule themselves.
- Human acceptance is required before changing a locked architecture decision or
  release criterion.

## Current reality

The repository currently contains a polished React/Vite UI under
`LokiAI Desktop UI/`. It also contains presentation-only data and browser-local
session behavior. It does not yet contain the Electron main/preload processes,
the Rust runtime, isolated build executor, local IPC, durable encrypted storage,
cloud contracts, the MCP server, a connector implementation, signed packaging,
or release evidence. The task graph starts from that reality and never treats
visible UI behavior as proof of a working hardware operation.
