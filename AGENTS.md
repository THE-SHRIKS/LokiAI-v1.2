# LokiAI Engineering Rules

This file is the repository-wide operating contract for Codex and every other
coding agent working on LokiAI. It is normative. Read it before planning,
editing, testing, reviewing, or delegating work.

## Product identity

- Product: LokiAI Main v1.2.
- Purpose: give users and approved coding agents a safe, observable way to
  discover, understand, monitor, build, flash, restart, test, deploy, and
  recover micro-device hardware.
- Initial certified hardware target: the exact ESP32-S3 N1R8 module/board SKU
  must be confirmed before any flash layout is implemented.
- Product direction: device-family-neutral contracts with connector-specific
  implementations. Never place ESP32-specific assumptions in shared domain or
  transport layers.
- Current repository state: `LokiAI Desktop UI/` is the accepted visual and
  interaction baseline. It is a React/Vite renderer, not yet a production
  desktop system. The Electron shell, trusted IPC, runtime, persistence, cloud
  integration, MCP server, and real hardware paths remain implementation work.

## Source-of-truth order

When instructions disagree, use this order:

1. The latest explicit human instruction for the current task.
2. The accepted task packet in `Docs/graph/templates/TASK_PACKET.md` format.
3. Accepted ADRs and `Docs/ARCHITECTURE.md`.
4. `Docs/SECURITY.md`, `Docs/BUILD_EXECUTION.md`,
   `Docs/CLOUD_CONTRACT.md`, and `Docs/TECH_STACK.md` within their declared
   boundaries.
5. `Docs/SPEC.md`.
6. `Docs/PRD.md`.
7. `Docs/graph/TASK_GRAPH.md` and `Docs/graph/PLANNER.md`.
8. This file and role guidance.

Do not silently resolve a contradiction that changes product behavior,
security, device safety, persistence, public contracts, or release scope.
Record it as a decision request and stop only the affected graph branch.

## Required reading by task type

Every worker reads this file and its task packet. Then read only the additional
documents needed for the assigned node:

| Task type | Required documents |
|---|---|
| Product behavior | `Docs/PRD.md`, `Docs/SPEC.md` |
| Process boundary, data flow, or dependency | `Docs/ARCHITECTURE.md` |
| Technology, dependency, toolchain, or packaging selection | `Docs/TECH_STACK.md` |
| Authentication, IPC, MCP, device action, artifact, or update | `Docs/SECURITY.md` |
| Cloud identity, workspace, sync, enrollment, notification, or dispatch | `Docs/CLOUD_CONTRACT.md` |
| Firmware build, repository command, toolchain, or artifact creation | `Docs/BUILD_EXECUTION.md` |
| UI/desktop behavior or design | `Docs/UI_SPEC.md` |
| MCP tools, resources, or client connection | `Docs/MCP_SPEC.md` |
| Connector, discovery, health, or physical device operation | `Docs/CONNECTOR_SPEC.md` |
| Task planning or delegation | `Docs/graph/README.md`, `Docs/graph/PLANNER.md`, `Docs/graph/TASK_GRAPH.md` |
| Cross-package integration | `Docs/graph/INTEGRATION.md` |
| Tests or verification | `Docs/TESTING.md` |
| Packaging or release | `Docs/RELEASE.md` |
| Requirement coverage | `Docs/graph/TRACEABILITY.md` |

Do not load every document into every worker. Bounded context is a deliberate
part of the graph-engineering method.

## Locked architecture boundaries

- Desktop shell: Electron.
- Renderer: the existing React + TypeScript interface.
- The renderer is untrusted presentation code. It never receives Node.js,
  filesystem, shell, serial, credential-store, or unrestricted network access.
- Electron main owns lifecycle, external-browser authentication, deep-link or
  loopback callback handling, window/tray behavior, updates, and a narrow typed
  preload bridge.
- Hardware and long-running monitoring run in a separate Rust runtime process.
- Repository code and firmware toolchains run only in a separate contained build
  executor. The privileged runtime coordinates and validates builds but never
  executes repository scripts in-process or through an unrestricted child shell.
- Agent access is provided by a provider-neutral MCP server. Provider setup is
  adapter/configuration work, not a separate MCP implementation per provider.
- Local persisted state uses full-database-encrypted SQLite. Its key and all
  credentials use the operating-system protection boundary and must never be
  placed in renderer storage, the database as plaintext, or plaintext
  configuration.
- Desktop-to-runtime communication uses an authenticated, versioned local IPC
  contract. Renderer-to-main communication uses an explicit allowlisted preload
  API. No generic `invoke`, raw command, or arbitrary path bridge is allowed.
- Cloud communication uses authenticated HTTPS/WSS contracts. The existing
  LokiAI website/control plane remains a separate system boundary.
- Cloud-dependent behavior follows `Docs/CLOUD_CONTRACT.md`; clients never invent
  tenant, role, endpoint, notification-delivery, or enrollment semantics.
- Every consequential device operation passes through policy, preflight,
  approval when required, execution, verification, and auditable result stages.

Changing a locked boundary requires an ADR and human acceptance before code.

## Non-negotiable implementation rules

1. Build production behavior only. Do not invent device readings, sessions,
   agent runs, incidents, deployments, job results, or successful operations.
   If a source is unavailable, expose an honest loading, empty, disconnected, or
   error state.
2. Preserve the existing Chamber & Olive visual language unless a task explicitly
   changes design. Wiring real behavior does not authorize redesign.
3. Never allow a renderer component or coding agent to execute arbitrary shell,
   serial, USB, network, filesystem, or firmware operations.
4. Validate all data at every trust boundary. Types alone are not runtime
   validation.
5. Use stable IDs, UTC timestamps, correlation IDs, and idempotency keys for
   operations that may be retried.
6. Make every state transition explicit. Do not infer success from process exit
   alone; use operation-specific verification.
7. Keep provider names out of core domain logic. Codex, Claude Code, Cursor, and
   future clients consume the same capabilities and policies.
8. Keep device-family logic behind the connector contract.
9. Never commit secrets, tokens, private keys, production device identifiers, or
   captured customer payloads. Redact sensitive values from logs and evidence.
10. Preserve existing user work. Never overwrite unrelated changes or use
    destructive version-control commands to make a task easier.
11. No dependency may be added merely for convenience. Record why it is needed,
    its maintenance state, license, security impact, and removal cost.
12. A task is not complete because code was written. It is complete only when its
    acceptance criteria and required evidence pass.
13. Build recipes are typed and allowlisted. No UI, MCP request, repository file,
    or connector parameter may become an arbitrary executable, command string,
    shell fragment, URL, environment variable, or unrestricted path.

## Graph execution protocol

The orchestrator is the only role that schedules nodes or changes graph status.

1. Select only nodes whose `depends_on` edges are satisfied.
2. Check file and contract ownership before parallel dispatch.
3. Create one bounded task packet per worker.
4. Assign one accountable worker and, when needed, a separate reviewer.
5. Use an isolated branch/worktree once the repository is initialized. Workers
   must not share a writable checkout.
6. A worker edits only its ownership set. Cross-owner changes return to the
   orchestrator for a new node or explicit scope update.
7. The worker returns a structured handoff with changed files, commands, results,
   risks, decisions, and remaining work.
8. Integration happens one node at a time in dependency order.
9. Test, build, security, and contract gates run after integration, not only in
   worker branches.
10. A failed gate returns to the worker responsible for the failing node. If the
    failure reveals a bad contract or plan, re-plan the affected descendants
    instead of patching around it.
11. A human gate is mandatory where `Docs/SECURITY.md` or `Docs/RELEASE.md`
    requires one.

Parallelism is allowed only when tasks are genuinely independent. More agents are
not automatically faster; overlapping ownership creates hidden rework.

## Task hygiene

Before editing:

- Re-read the task objective, acceptance criteria, dependencies, allowed files,
  forbidden files, and required verification.
- Inspect current files and repository status. Do not assume the task packet is
  newer than the code.
- Confirm prerequisites exist. If they do not, return `BLOCKED_DEPENDENCY` with
  evidence instead of creating a substitute implementation.
- State any necessary assumption in the handoff.

While editing:

- Prefer the smallest coherent change that satisfies the task contract.
- Keep domain rules out of UI components and transport details out of domain
  types.
- Update tests and documentation in the same node when behavior or contracts
  change.
- Do not refactor unrelated code.

After editing:

- Run the verification required by the task packet and `Docs/TESTING.md`.
- Inspect the final diff for accidental secrets, generated files, unrelated
  changes, and weakened security controls.
- Update traceability for any added or changed requirement.
- Return the handoff format from `Docs/graph/templates/WORKER_HANDOFF.md`.

## Human approval boundaries

Agents may inspect and propose freely inside the assigned repository scope. Human
approval is required before:

- flashing firmware, deploying an artifact, changing device configuration,
  restarting a production device, rolling back a fleet, or injecting bus traffic;
- changing authentication, signing, update, RBAC, or approval policy;
- deleting persisted user data or audit evidence;
- changing a public protocol or compatibility promise;
- publishing, merging to a protected branch, releasing, or deploying;
- rotating or accessing production credentials.

Approval is scoped to the exact operation, target, artifact digest, expected
effect, and expiry. Approval for one operation never implies approval for another.

## Completion vocabulary

Use only these task results:

- `COMPLETE`: all acceptance criteria and required evidence pass.
- `BLOCKED_DEPENDENCY`: a required predecessor, environment, device, credential,
  or human decision is missing.
- `FAILED_VERIFICATION`: implementation exists but a required gate failed.
- `NEEDS_REPLAN`: the accepted contract or dependency graph is incorrect.
- `CANCELLED`: the orchestrator or human explicitly stopped the node.

Never report `COMPLETE` with skipped mandatory verification.
