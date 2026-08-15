# LokiAI graph-engineering workspace

Status: **Operational entrypoint**  
Product: **LokiAI Main v1.2**  
Updated: **2026-08-15**

This is the single home for LokiAI execution-graph state. Claude Code, Codex, and
any future orchestrator use the same files here. Product intent and system
contracts remain in the parent `Docs/` directory; provider-specific role adapters
remain in their native discovery directories and may not redefine these rules.

## Directory contract

```text
Docs/graph/
├── README.md                 # this entrypoint
├── PLANNER.md                # graph-construction and scheduling rules
├── TASK_GRAPH.md             # accepted nodes, dependencies, gates, and state
├── INTEGRATION.md            # integration order and contract ownership
├── TRACEABILITY.md           # requirement → node → test/evidence coverage
├── PROMPTS.md                # provider-neutral role prompts
├── tasks/                    # accepted immutable task packets
├── handoffs/                 # structured worker results
├── evidence/                 # verification and review evidence indexes
└── templates/                # task, handoff, ADR, and change templates
```

No task packets, handoffs, graph status files, or verification indexes may be
created elsewhere without an accepted process change.

## Claude execution flow

1. Read repository-root `AGENTS.md` completely.
2. Read the accepted packet at `Docs/graph/tasks/<NODE-ID>.md`.
3. Read only the task-type documents required by `AGENTS.md`.
4. Confirm dependencies, ownership, environment, and human gates before editing.
5. Perform exactly the packet scope in its isolated branch or worktree.
6. Return the result using `Docs/graph/templates/WORKER_HANDOFF.md` and register it
   as `Docs/graph/handoffs/<NODE-ID>-<ATTEMPT>.md` when the orchestrator requests a
   repository record.
7. Store durable, non-sensitive verification indexes under
   `Docs/graph/evidence/<wave>/<NODE-ID>/`; reference protected external artifacts
   by digest instead of committing raw secrets or customer/device payloads.
8. The orchestrator integrates in dependency order, runs the graph gates, updates
   traceability, and alone changes node status.

## Ownership

| Artifact | Accountable owner | Worker permissions |
|---|---|---|
| `TASK_GRAPH.md` and task registry | Orchestrator | Read; propose changes in handoff |
| Accepted task packet | Orchestrator | Read-only after dispatch |
| Implementation paths | Assigned worker | Only paths named by its packet |
| Worker handoff | Assigned worker | Create its own result only |
| Review evidence | Assigned independent reviewer | Create its assigned review only |
| Integration record and traceability | Orchestrator/integrator as assigned | Update only when packet authorizes it |

The files under `.claude/agents/` stay outside this folder solely because Claude
Code auto-discovers roles there. They are thin adapters into this provider-neutral
workspace, not a second planning system.

## Naming rules

- Task packet: `tasks/<NODE-ID>.md`
- Revised task packet: `tasks/<NODE-ID>-R<N>.md`
- Worker handoff: `handoffs/<NODE-ID>-<ATTEMPT>.md`
- Evidence index: `evidence/<wave>/<NODE-ID>/README.md`
- Independent review: `evidence/<wave>/<NODE-ID>/REVIEW-<TYPE>.md`

An absent artifact means “not produced”; never create a convincing placeholder or
claim verification that did not run.
