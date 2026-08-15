# Product Requirements Document — LokiAI Main v1.2

Status: **Draft for human acceptance**  
Owner: **LokiAI product owner**  
Updated: **2026-08-15**

## 1. Product definition

LokiAI Main is a desktop control center and local/gateway runtime that connects
coding agents to physical micro-device workflows without surrendering hardware
authority to those agents. LokiAI collects trustworthy evidence, presents device
state, brokers scoped agent tools, executes approved operations, verifies real
outcomes, and records a complete activity trail.

The product is successful when a user can move from installation to a connected
device or new hardware project, collaborate with a coding agent, detect an actual
device problem, approve a bounded correction, and see LokiAI verify recovery.

## 2. Problem

Coding agents can edit firmware but generally cannot safely understand or operate
the hardware on which it runs. Raw shell or serial access is too broad; ordinary
monitoring tools do not connect evidence to code changes and deployment actions;
and device-specific tooling does not provide a shared policy, approval, and audit
layer across agents and device families.

LokiAI solves this by separating reasoning from authority:

- coding agents investigate and propose;
- LokiAI authenticates, authorizes, executes, observes, and records;
- humans retain final authority over consequential actions.

## 3. Users

### P-01 Independent hardware developer

Needs one desktop application to connect a board, understand health and logs, let
an agent assist with firmware, and safely build/flash/test without assembling a
different toolchain for every workflow.

### P-02 Small hardware team

Needs shared workspaces, consistent device records, incident evidence, agent-run
visibility, approvals, deployment history, and reproducible verification.

### P-03 Industrial fleet operator — architectural target

Needs multiple always-on gateways, many devices and cameras or other edge assets,
fleet policies, staged deployment, audit, and role separation. Version 1.2 must
not promise full industrial fleet functionality, but its contracts and runtime
identity model must not block that direction.

## 4. Goals

- G-01 Preserve and productionize the accepted LokiAI Main UI as a secure Windows
  desktop application.
- G-02 Complete browser-based account connection and resumable onboarding.
- G-03 Discover and validate real connected hardware through a device-neutral
  connector contract.
- G-04 Certify one physical ESP32-S3 reference path end to end.
- G-05 Expose safe LokiAI capabilities to Codex, Claude Code, Cursor, and future
  MCP clients through one provider-neutral server.
- G-06 Collect device telemetry and bounded logs, evaluate rules, and create
  evidence-backed incidents.
- G-07 Execute builds, flashes, restarts, tests, deployments, observations, and
  rollbacks as durable, auditable jobs.
- G-08 Keep monitoring independent of the visible desktop window and make the
  runtime deployable to an always-on gateway later.
- G-09 Prevent unapproved or unverified high-risk hardware actions.
- G-10 Ship a signed, updateable, diagnosable release with a human release gate.
- G-11 Complete the control-plane loop for runtime enrollment, cloud sync,
  offline/stale state, notifications, and provider-aware incident delivery.
- G-12 Contain repository/toolchain execution so an agent-authored firmware
  change cannot inherit LokiAI credentials or device authority.

## 5. Non-goals for v1.2

- Manufacturing or designing physical hardware.
- A general-purpose remote shell for agents.
- Full autonomous remediation of production devices without policy and approval.
- Certification of every device family shown in the UI.
- A complete industrial multi-site management offering.
- Rebuilding the public LokiAI website.
- Replacing the user's coding agent with an embedded general-purpose code editor.

## 6. Functional requirements

### Account and onboarding

- **FR-AUTH-001:** The desktop app shall initiate sign-in in the user's default
  browser using Authorization Code with PKCE, state, nonce, expiry, and a local
  callback owned by the desktop process.
- **FR-AUTH-002:** The renderer shall receive only a minimal session projection;
  refresh tokens and secrets shall remain outside renderer storage.
- **FR-AUTH-003:** Logout shall revoke or invalidate the local session, clear
  protected local projections, disconnect agent credentials as policy requires,
  and return the app to the signed-out state.
- **FR-ONB-001:** Onboarding shall be resumable and shall cover account,
  workspace, agent connection, device-or-project choice, validation, and review.
- **FR-WS-001:** A user shall create or join a workspace according to cloud
  authorization and select an active workspace in the desktop app.
- **FR-WS-002:** Workspace membership, role, policy, and capability projections
  shall be versioned, refreshed from the cloud authority, and invalidated when
  expired or revoked.

### Devices and connector system

- **FR-DEV-001:** LokiAI shall discover devices only through registered connector
  implementations and report connector identity and evidence for every result.
- **FR-DEV-002:** A device record shall include stable hardware identity, family,
  connector, transport, capabilities, runtime/gateway, firmware/config/model
  versions when observable, and last-seen state.
- **FR-DEV-003:** Initial health checks shall report each check as passed, warning,
  failed, unavailable, or interrupted with measured evidence.
- **FR-DEV-004:** A discovered candidate shall complete an authorized,
  challenge-bound enrollment before it becomes a workspace-owned device or may
  receive a consequential operation.
- **FR-CON-001:** Shared services shall call a device-neutral connector contract;
  family-specific commands, partitions, probes, and recovery procedures remain in
  the connector package.
- **FR-CON-002:** The first certified connector shall support the confirmed
  ESP32-S3 reference board for discovery, identity, logs, health, build metadata,
  flash, restart, verification, and rollback/recovery where the board permits.
- **FR-CON-003:** Unsupported devices shall be reported honestly and shall never
  be offered operations their connector does not advertise.

### Coding agents and MCP

- **FR-AGT-001:** Users shall connect supported coding clients to a workspace with
  a revocable, scoped credential and explicit capability display.
- **FR-AGT-002:** LokiAI shall display active and historical agent runs, including
  parent/child relationships, phase, requested tools, changed-file summaries,
  associated devices/incidents/deployments, and final outcome when supplied.
- **FR-AGT-003:** An incident may create a provider-neutral work intent. LokiAI
  shall record real provider delivery/acknowledgement or an honest
  `user_action_required` fallback; delivery shall not grant device authority.
- **FR-MCP-001:** One MCP server shall expose versioned, documented LokiAI tools
  and resources independent of the client provider.
- **FR-MCP-002:** Read operations shall be scoped by workspace and device access.
- **FR-MCP-003:** Consequential tools shall return an approval request or durable
  job ID; they shall never imply success before verification.
- **FR-MCP-004:** Tool results shall include correlation IDs, structured errors,
  timestamps, evidence references, and retry safety.

### Monitoring and incidents

- **FR-MON-001:** The runtime shall ingest timestamped telemetry and bounded logs
  from connectors, preserve source quality, and expose connection gaps.
- **FR-MON-002:** Users shall configure enabled monitoring rules with scope,
  condition, duration/window, severity, and required signals.
- **FR-MON-003:** Runtime/gateway health shall be visible separately from device
  health so a disconnected observer is not reported as a healthy device.
- **FR-INC-001:** A matched rule shall create or correlate an incident with a
  bounded evidence snapshot, affected scope, occurrence information, and audit
  event.
- **FR-INC-002:** Incidents shall progress through open, investigating, verifying,
  resolved, and closed states using explicit transition rules.
- **FR-INC-003:** An incident shall not resolve until every required verification
  condition passes for its observation window.
- **FR-NOT-001:** LokiAI shall create deduplicated in-app/OS notification intents
  for configured incident, approval, operation, runtime-offline, update, and
  security events and track their delivery/read state.

### Cloud control plane

- **FR-CLD-001:** Desktop and runtime shall integrate with versioned cloud
  identity, workspace, runtime, policy, audit, artifact/evidence, and notification
  contracts; they shall not infer undocumented tenant or endpoint behavior.
- **FR-CLD-002:** Runtime/cloud synchronization shall use durable outbox/inbox
  checkpoints, stable event IDs, idempotency, bounded retry, and explicit conflict
  rules.
- **FR-CLD-003:** Runtime heartbeat shall distinguish online, stale, offline,
  revoked, and incompatible states, and all device projections shall expose the
  freshness of the observing runtime.

### Operations and deployments

- **FR-BLD-001:** Repository code and external firmware toolchains shall execute
  only in a dedicated, ephemeral, least-privilege build executor with explicit
  filesystem, network, process, resource, output, and time limits.
- **FR-BLD-002:** Every build shall use an immutable source snapshot and typed,
  allowlisted recipe, then register only externally validated, content-addressed
  output with complete provenance. Build success shall not authorize deployment.
- **FR-ART-001:** Artifacts shall declare type (`firmware`, `model`,
  `configuration`, `bundle`, or connector extension), target compatibility,
  resource requirements, provenance, digest, and verification/rollback contract.
  LokiAI shall offer only types and combinations the target connector advertises.

- **FR-JOB-001:** Build, preflight, flash, restart, test, deploy, observe, and
  rollback operations shall run as durable jobs with explicit stages and
  cancellation semantics.
- **FR-APR-001:** Approval requirements shall be based on operation, target,
  environment, scope, artifact digest, and policy—not merely on which UI button
  or agent requested the action.
- **FR-APR-002:** An approval shall be time-limited, single-purpose, attributable,
  and unusable after its bound inputs change.
- **FR-DEP-001:** A deployment shall bind an immutable artifact digest, source
  revision, target scope, rollout strategy, preflight, verification plan, and
  rollback plan.
- **FR-DEP-002:** LokiAI shall support a single-device rollout in v1.2 and retain
  contracts for canary and batched fleet rollout.
- **FR-DEP-003:** Failure during a safety-critical stage shall stop progression and
  offer or execute only the previously authorized recovery policy.

### Activity and settings

- **FR-AUD-001:** Security-relevant and device-changing events shall produce an
  append-only activity record with actor, action, target, result, timestamp,
  correlation, and redacted details.
- **FR-SET-001:** Settings shall cover workspace, agents, runtime, permissions,
  monitoring, notifications, security, and data/retention controls according to
  role.
- **FR-UI-001:** Dashboard, Devices, Agents, Monitoring, Incidents, Deployments,
  Activity, and Settings shall use real service projections and truthful
  unavailable states.
- **FR-UI-002:** Every high-risk action surface shall show target, risk, expected
  effect/downtime, preflight, verification, rollback, and approval status before
  execution.

## 7. Non-functional requirements

- **NFR-SEC-001:** The Electron renderer shall be sandboxed with Node integration
  disabled, context isolation enabled, restrictive CSP, blocked arbitrary
  navigation/window creation, validated IPC senders, and a minimal context bridge.
- **NFR-SEC-002:** Secrets shall be stored in operating-system-protected storage,
  redacted in diagnostics, and rotatable without reinstalling the app.
- **NFR-SAFE-001:** No device-changing operation may bypass capability checks,
  policy, locking, preflight, and operation-specific verification.
- **NFR-REL-001:** Jobs shall survive UI reload and app-window closure. Interrupted
  jobs shall recover to a known state or report an explicit unknown outcome that
  requires reconciliation.
- **NFR-REL-002:** A repeated command with the same idempotency key shall not start
  a second destructive operation.
- **NFR-DATA-001:** SQLite migrations shall be forward-tested, transactional, and
  backed up before irreversible changes.
- **NFR-DATA-002:** Sensitive local database content shall be encrypted at rest
  with key material protected by the operating-system credential boundary;
  plaintext secrets shall never enter the database, backups, WAL, or support
  bundles.
- **NFR-OBS-001:** Every cross-process request and durable job shall carry a
  correlation ID and emit structured, redacted diagnostics.
- **NFR-PERF-001:** The desktop shell shall reach an interactive cached state in
  five seconds on supported hardware, without blocking on device discovery or
  cloud availability.
- **NFR-PERF-002:** Live updates shall be backpressured and bounded; renderer memory
  shall not grow with unbounded telemetry or logs.
- **NFR-COMP-001:** Shared contracts shall be versioned and support at least one
  rolling-upgrade compatibility window between desktop, runtime, and MCP server.
- **NFR-PORT-001:** Core domain and connector contracts shall avoid Windows-only
  assumptions even though v1.2 distribution is Windows-first.
- **NFR-PRIV-001:** Evidence collection shall be bounded, purpose-specific,
  configurable, and redacted before cloud sync or agent exposure.
- **NFR-UX-001:** Existing brand tokens, density, navigation, and primary
  interaction patterns shall remain recognizable through production wiring.
- **NFR-PLAT-001:** v1.2 shall support Windows 11 x64 on supported Microsoft
  builds using a pinned, documented toolchain and signed component matrix; adding
  another OS/architecture requires its own qualification evidence.
- **NFR-SUP-001:** Users shall be able to inspect runtime/component health and
  export a consented, class-previewed, redacted diagnostic bundle without secret,
  unrestricted source, or customer payload disclosure.

## 8. Operation risk policy

| Risk | Examples | Default behavior |
|---|---|---|
| Low | List devices, read capabilities, read bounded logs, inspect incident | Execute when authorized; audit access |
| Medium | Run diagnostics, build artifact, change non-device local preference | Policy check; may execute without interactive approval in lab workspaces |
| High | Restart, flash, deploy config/model, alter monitoring affecting protection | Mandatory explicit approval and preflight |
| Critical | Fleet-wide deployment, safety interlock change, unsigned artifact override, bus injection | Denied by default; elevated role plus separate human gate and constrained environment |

## 9. Release acceptance

Version 1.2 is releasable only when:

1. External-browser auth and real logout pass end to end.
2. The full onboarding journey persists and resumes without renderer-held secrets.
3. The confirmed ESP32-S3 board completes discovery, identity, health, build,
   approved flash, restart, observation, and rollback/recovery evidence on
   physical hardware.
4. Codex and Claude Code both consume the same MCP contract; Cursor setup is
   verified where its supported MCP surface permits it.
5. A real rule can create an incident from captured evidence and close it only
   after verification.
6. The UI contains no successful-looking fabricated runtime behavior.
7. Electron, IPC, auth, MCP, artifact, update, and dependency security gates pass.
8. Build containment prevents repository/toolchain access to secrets, LokiAI
   storage/IPC, unrelated files, network, and physical-device handles.
9. Runtime enrollment, heartbeat expiry, reconnect, cloud event sync,
   notifications, and incident work-intent delivery/fallback pass end to end.
10. Sensitive local state, WAL/backups, and key lifecycle pass data-at-rest tests.
11. Installer, uninstall, update, recovery, and diagnostic export pass on supported
   Windows versions.
12. Required traceability has no uncovered release-blocking requirement.
13. The human release owner signs the final release decision.

## 10. Product measures

- Median installation-to-first-device-health-check time.
- Percentage of discovery attempts ending in identified, unsupported, or
  actionable failure states rather than unknown failure.
- Incident evidence completeness rate.
- Device-operation verification success rate and unknown-outcome rate.
- Mean time from incident creation to verified recovery.
- Approval decision latency by risk class.
- Runtime uptime and event-loss rate on supported hosts.
- Crash-free desktop sessions and crash-free runtime hours.
- Percentage of agent tool calls rejected before execution due to scope or policy.
- Rollback/recovery success rate during controlled failure testing.

Measures are operational signals, not permission to collect unnecessary customer
data.
