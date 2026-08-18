# Task Packet Registry

Status: **Operational**  
Graph source: [`../TASK_GRAPH.md`](../TASK_GRAPH.md)  
Template: [`../templates/TASK_PACKET.md`](../templates/TASK_PACKET.md)

## Policy

The task graph is planned globally, but detailed worker packets are created just
in time. Pre-writing every packet would freeze assumptions before dependencies and
contracts are accepted, create stale context, and invite workers to start blocked
work.

The orchestrator creates `Docs/graph/tasks/<NODE-ID>.md` only when:

- all predecessor nodes are accepted;
- required decisions/human gates are recorded;
- consumed contract revisions are known;
- writable ownership is collision-free;
- acceptance criteria and commands are executable from the starting revision;
- the worker environment/device prerequisites are available or not required.

Once accepted and dispatched, a packet is immutable. Scope changes supersede it
with a revision recorded inside the same task history or a new node, rather than
silently editing a worker's contract.

## Current packet index

| Node | Packet | State | Reason |
|---|---|---|---|
| G0 | n/a (DECISION node; no worker packet) | accepted 2026-08-15 | Human product/release owner accepted PRD, spec, and locked architecture in chat; recorded in `../TASK_GRAPH.md` section 8 status-update record |
| FND-001 | [`FND-001.md`](FND-001.md) | accepted 2026-08-15 | Repo initialized on branch `LokiAI-v1.2`, baseline committed and pushed to `https://github.com/The-Shriks/LokiAI-v1.2`. Handoff: [`../handoffs/FND-001-1.md`](../handoffs/FND-001-1.md) |
| FND-002 | [`FND-002.md`](FND-002.md) | accepted | Real toolchain (Node 24.19.0, Rust 1.97.1, pnpm 11.21.0) installed and pinned; `pnpm run verify` passes with real evidence. Handoff: [`../handoffs/FND-002-1.md`](../handoffs/FND-002-1.md) |
| CTR-001 | [`CTR-001.md`](CTR-001.md) | accepted, integrated | Handoff: [`../handoffs/CTR-001-1.md`](../handoffs/CTR-001-1.md). JSON-Schema-vs-zod deviation flagged for human review. |
| DSK-001 | [`DSK-001.md`](DSK-001.md) | accepted, integrated | Handoff: [`../handoffs/DSK-001-1.md`](../handoffs/DSK-001-1.md). Packaged app proven; `pnpm run electron:package` reproducibility gap tracked (§7a). |
| RUN-001 | [`RUN-001.md`](RUN-001.md) | accepted, integrated | Handoff: [`../handoffs/RUN-001-1.md`](../handoffs/RUN-001-1.md). Cargo workspace now has its first real member; guard script's honest-skip no longer applies. |
| DSK-002 | [`DSK-002.md`](DSK-002.md) | accepted, integrated | Handoff: [`../handoffs/DSK-002-1.md`](../handoffs/DSK-002-1.md). 77/77 tests, including sender-spoofing rejection and honest `runtime_unavailable` results pending `RUN-002`. |
| DAT-001 | [`DAT-001.md`](DAT-001.md) | accepted, integrated | Handoff: [`../handoffs/DAT-001-1.md`](../handoffs/DAT-001-1.md). 63/63 tests, no bugs found. `cipher.rs` confirmed a genuine swappable seam, not an invented scheme. |
| CON-001 | [`CON-001.md`](CON-001.md) | accepted, integrated | Handoff: [`../handoffs/CON-001-1.md`](../handoffs/CON-001-1.md). 52/52 tests (44 unit + 8 conformance). |
| MCP-001 | [`MCP-001.md`](MCP-001.md) | accepted, integrated | Handoff: [`../handoffs/MCP-001-1.md`](../handoffs/MCP-001-1.md). Broken `pnpm-workspace.yaml` placeholder + 10 lint findings fixed; 36/36 tests. |
| RUN-002 | [`RUN-002.md`](RUN-002.md) | accepted, integrated | Handoff: [`../handoffs/RUN-002-1.md`](../handoffs/RUN-002-1.md). 6 real bugs found and fixed (incl. a hanging test from a split-pipe half never dropped); 69/69 tests. |
| RUN-003 | [`RUN-003.md`](RUN-003.md) | dispatched (worktree `RUN-003`, resumed after a session-limit interruption) | `crates/loki-runtime/**` additions |
| CON-002 | [`CON-002.md`](CON-002.md) | accepted, integrated | Handoff: [`../handoffs/CON-002-1.md`](../handoffs/CON-002-1.md). 35/35 tests (26 unit + 9 integration); own `TestConnector` used in place of `CON-001`'s private `MockConnector` (flagged, accepted). |
| MON-001 | [`MON-001.md`](MON-001.md) | accepted, integrated | Handoff: [`../handoffs/MON-001-1.md`](../handoffs/MON-001-1.md). 58/58 tests (42 unit + 16 integration). **Flagged deviation:** telemetry lives in its own self-contained second SQLite database, not `loki-storage`'s — the packet's literal migration instruction and its forbidden-scope boundary were mutually exclusive as written (verified against actual source); open ADR-level question for a human/follow-up node (§9 of the handoff), also affects `DAT-002` encryption scope. |
| JOB-001 | [`JOB-001.md`](JOB-001.md) | accepted, integrated | Handoff: [`../handoffs/JOB-001-1.md`](../handoffs/JOB-001-1.md). 44/44 tests (25 unit + 19 integration); in-memory per-process lease + `JobEngine::reconcile()` crash-recovery, not a new SQLite table (flagged, accepted). |
| MCP-002 | [`MCP-002.md`](MCP-002.md) | dispatched (worktree `MCP-002`) | new read-only tools in `services/mcp-server/src/**` |
| BLD-001 | [`BLD-001.md`](BLD-001.md) | dispatched (worktree `BLD-001`) | new `crates/loki-build-executor/**`; requires independent `loki-security-reviewer` pass before acceptance (safety-boundary node) |
| Remaining Part One (W0–W6) nodes | created just-in-time as each becomes dependency-ready | dispatching continuously | Human authorized continuous Part One orchestration 2026-08-15 (see `../TASK_GRAPH.md` section 1a and `../PLANNER.md` section 12). Not paused between nodes; only genuinely blocked branches (`OQ-001`–`OQ-007`, missing credential/device/approval) wait for a human decision. |
| `P1-INT-001` (Part One completion gate) | not created | dependency pending | Depends on all seven W6 terminal nodes (`ART-001`, `OPS-001`, `MCP-003`, `DEP-001`, `CLD-002`, `NOT-001`, `AGT-002`) being accepted first; see `../TASK_GRAPH.md` section 4 "Part One completion gate" |
| Part Two (`UI-001` and W7–W10 descendants) | not created | dependency + human-approval pending | Blocked on `P1-INT-001` acceptance **and** a separate human go-ahead per `../TASK_GRAPH.md` section 1a; do not create Part Two packets before both exist |

## Naming and handoff

- Task packet: `Docs/graph/tasks/<NODE-ID>.md`
- Worker handoff: `Docs/graph/handoffs/<NODE-ID>-<ATTEMPT>.md`
- Verification evidence index: `Docs/graph/evidence/<wave>/<NODE-ID>/README.md`
- Independent review: `Docs/graph/evidence/<wave>/<NODE-ID>/REVIEW-<TYPE>.md`
- Large/sensitive test output: protected external artifact reference plus digest,
  never committed raw by default.

Only the orchestrator updates this registry and `Docs/graph/TASK_GRAPH.md` status.
