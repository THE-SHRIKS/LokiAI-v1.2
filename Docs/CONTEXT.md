# LokiAI project context

Status: **Informative baseline**  
Updated: **2026-08-24**

## Product story

LokiAI is not an edge-model hosting dashboard and not merely a hardware monitor.
It is the controlled bridge between people, coding agents, local or gateway
runtimes, and physical devices. It lets an agent understand evidence and request
operations while LokiAI retains identity, permissions, device safety,
verification, and audit authority.

Version 1.1 established LokiAI's initial product direction through Android model
deployment. Version 1.2 expands LokiAI Main into a desktop and runtime system for
micro-device development and operation.

## Agreed user journey

1. The user creates a LokiAI account on the website.
2. The user downloads and installs LokiAI Main and its runtime.
3. The desktop app opens the system browser for secure account connection.
4. Authentication returns control to the desktop app.
5. The user creates or joins a workspace.
6. The user connects a coding agent such as Codex, Claude Code, or Cursor.
7. LokiAI asks whether the user has an existing device or is starting a new
   hardware project.
8. For an existing device, LokiAI discovers it and performs a real initial health
   check. For a new project, LokiAI captures the target and connector intent.
9. The user works with the coding agent. The agent uses LokiAI tools to inspect
   devices and request build, flash, restart, test, deploy, or rollback actions.
10. Runtime workers monitor connected devices, collect bounded evidence, and open
    incidents when rules match.
11. An agent may investigate and propose a correction. LokiAI enforces preflight,
    approval, execution, verification, and rollback policy.
12. LokiAI closes an incident only after the required real observations pass.

## Accepted product surfaces

The main navigation is:

1. Dashboard
2. Devices
3. Agents
4. Monitoring
5. Incidents
6. Deployments
7. Activity
8. Settings

The workspace switcher belongs near the top of the desktop navigation. Account
controls and logout belong at the bottom. The current Chamber & Olive UI is the
visual baseline and should be wired to production behavior rather than replaced.

## Accepted technical direction

- Windows-first Electron desktop app around the existing React/TypeScript UI.
- Secure external-browser OAuth using Authorization Code with PKCE.
- Separate Rust runtime for hardware, monitoring, jobs, and connector execution.
- Runtime continues independently of the visible window. An industrial gateway
  deployment can host the same runtime continuously.
- Provider-neutral MCP server with adapters/configuration for supported coding
  clients.
- Separate contained build executor for repository/toolchain execution; the
  privileged runtime never runs firmware project commands in-process.
- Connector SDK/contract supports multiple micro-device families.
- ESP32-S3 is the first physical reference target, not a shared-layer assumption.
- Production data and truthful unavailable states only.
- High-risk hardware operations require explicit human approval.
- Work advances in planned phases and dependency-ready graph nodes.

## Repository facts observed on 2026-08-14

- `LokiAI Desktop UI/` contains React 19.0.1, TypeScript 5.8, Vite 6.2,
  Tailwind integration, Motion, and Lucide icons. These preserve the accepted
  visual prototype but are not the reviewed 2026 production pins.
- Authentication, onboarding, navigation, dashboard, devices, agents, monitoring,
  incidents, deployments, activity, and settings views exist.
- `src/context/LokiContext.tsx` currently centralizes UI state and actions.
- The current renderer persists session/onboarding state through browser storage.
- The current renderer sources device, agent, incident, deployment, monitoring,
  approval, telemetry, log, and activity content from presentation fixtures.
- Unused AI Studio/Gemini, Express, and dotenv dependencies/configuration remain
  in the prototype package and are scheduled for reviewed removal unless a task
  proves a production use.
- No Electron, preload, runtime, MCP, database, or cloud integration package exists
  in the repository.
- The workspace is not currently initialized as a Git repository.
- Node package commands could not be run from the current shell because `npm` was
  not available on `PATH`; this is an environment finding, not a product failure.

## Hard constraints

- Do not redesign the accepted UI during backend wiring.
- Do not claim device support without a verified connector and physical evidence.
- Do not put privileged operations in the renderer.
- Do not let an AI agent bypass LokiAI policy or talk directly to raw hardware.
- Do not hide unavailable infrastructure behind successful-looking UI behavior.
- Do not require the user's visible desktop window to stay open for monitoring.
- Do not claim a powered-off workstation can service attached hardware. Continuous
  operation requires an always-on host or LokiAI Gateway.

## Inputs still requiring human confirmation

| ID | Decision | Why it matters | Required before |
|---|---|---|---|
| OQ-001 | Exact ESP32-S3 module and development-board SKU; `N1R8` may not uniquely define flash/PSRAM layout | Flash partitions, bootloader arguments, voltage assumptions, and recovery steps depend on it | Connector flash implementation |
| OQ-002 | LokiAI cloud API base URLs, identity provider, tenant model, and existing v1.1 API contracts | Desktop authentication and workspace sync must integrate rather than replace them | AUTH and cloud-client nodes |
| OQ-003 | Meaning and integration surface of “Gravity” | The provider adapter cannot be designed from a name alone | Gravity adapter node |
| OQ-004 | Windows distribution channel: direct signed installer, Microsoft Store, or both | Signing, update, and service-install behavior differ | Packaging architecture gate |
| OQ-005 | v1.2 commercial retention limits for telemetry, logs, incidents, and audit records | Storage, sync, privacy, and export behavior depend on policy | Persistence and release gates |
| OQ-006 | Whether v1.2 includes an embedded LokiAI operations chat, or intentionally keeps coding conversation in Codex/Claude/Cursor while LokiAI Main remains the control center | Earlier product intent mentioned chatting inside LokiAI, but the accepted eight-tab UI and non-goal currently define external coding clients; silently choosing either changes scope and UI/agent architecture | Any chat UI/model/provider task; not the core desktop/runtime/MCP path |

Only the affected graph branch should wait on an unresolved input. Independent
work may continue.

## Resolved inputs

| ID | Decision | Accepted by | Recorded effect |
|---|---|---|---|
| OQ-007 | SQLCipher Community Edition for the local-only v1.2 database implementation, with required attribution in release notices. The existing three bounded-context database files remain separate and each must receive full SQLCipher encryption with a shared Windows DPAPI-backed key lifecycle. | Human product owner, explicit “okay go ahead”, 2026-08-24 | Unblocks `DAT-002`; does not approve cloud, hardware, installer, or release work. See `Docs/graph/decisions/ADR-0001-local-data-encryption.md`. |
