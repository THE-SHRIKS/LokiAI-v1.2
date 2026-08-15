# Claude Code entrypoint

Read `AGENTS.md` in full before acting. It is the canonical project-wide agent
contract. Then enter through `Docs/graph/README.md`, read the assigned task packet,
and load only the task-specific documents listed in the `Required reading by task
type` table.

For multi-agent work:

- use `.claude/agents/planner.md` to analyze and shape task nodes;
- use `.claude/agents/implementer.md` for one bounded implementation node;
- use `.claude/agents/test-reviewer.md` and
  `.claude/agents/security-reviewer.md` as independent reviewers;
- use `.claude/agents/integrator.md` only after worker handoffs are complete;
- do not give two writable agents ownership of the same files;
- do not mark graph nodes complete yourself unless acting as the orchestrator.

All LokiAI-specific product, architecture, safety, testing, and release rules live
in `AGENTS.md` and `Docs/`. Do not duplicate or weaken them here.
