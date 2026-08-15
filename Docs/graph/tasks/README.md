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
| FND-002 and other FND-001 descendants | not created | dependency pending | FND-001 is accepted, so FND-002 is now dependency-ready, but human instruction scoped this turn to FND-001 only — no further packet has been created or dispatched |

## Naming and handoff

- Task packet: `Docs/graph/tasks/<NODE-ID>.md`
- Worker handoff: `Docs/graph/handoffs/<NODE-ID>-<ATTEMPT>.md`
- Verification evidence index: `Docs/graph/evidence/<wave>/<NODE-ID>/README.md`
- Independent review: `Docs/graph/evidence/<wave>/<NODE-ID>/REVIEW-<TYPE>.md`
- Large/sensitive test output: protected external artifact reference plus digest,
  never committed raw by default.

Only the orchestrator updates this registry and `Docs/graph/TASK_GRAPH.md` status.
