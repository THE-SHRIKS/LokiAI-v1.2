# LokiAI Main v1.2 Execution Graph

Status: **G0 accepted 2026-08-15; FND-001 accepted — repo initialized, baseline committed and pushed to `origin/LokiAI-v1.2`. Stopped per "Part One only"; FND-002 undispatched.**  
Graph owner: **Orchestrator**  
Updated: **2026-08-15**

## 1. Baseline

- `UI-BASE` exists: accepted visual/onboarding/dashboard renderer under
  `LokiAI Desktop UI/`.
- It is not evidence of working auth, hardware, monitoring, incidents,
  deployments, or agent integration.
- The repository has no Git baseline and no desktop/runtime/service structure.
- Tasks below are production implementation nodes. Device-changing verification
  waits for the user's confirmed physical board; independent software work does
  not stop.

## 2. High-level DAG

```mermaid
flowchart TD
  G0["G0 Product and architecture acceptance"] --> FND001["FND-001 Repository baseline"]
  FND001 --> FND002["FND-002 Monorepo/build foundation"]
  FND001 --> CTR001["CTR-001 Domain and protocol contracts"]
  CTR001 --> CLD001["CLD-001 Cloud API contract"]

  FND002 --> DSK001["DSK-001 Secure Electron shell"]
  FND002 --> RUN001["RUN-001 Rust runtime foundation"]
  FND002 --> MCP001["MCP-001 MCP server foundation"]
  CTR001 --> DSK002["DSK-002 Typed preload bridge"]
  CTR001 --> RUN002["RUN-002 Local IPC and handshake"]
  CTR001 --> CON001["CON-001 Connector SDK"]
  CTR001 --> DAT001["DAT-001 SQLite and migrations"]
  DAT001 --> DAT002["DAT-002 Encrypted local data and key lifecycle"]

  DSK001 --> AUTH001["AUTH-001 Browser authentication"]
  DSK002 --> AUTH001
  DAT001 --> AUTH001
  RUN001 --> RUN002
  RUN002 --> RUN003["RUN-003 Background host lifecycle"]
  DAT001 --> RUN003

  CON001 --> CON002["CON-002 Discovery and identity engine"]
  CON001 --> CON003["CON-003 ESP32-S3 connector"]
  RUN002 --> CON002
  RUN002 --> MCP002["MCP-002 Read/diagnostic tools"]
  MCP001 --> MCP002

  AUTH001 --> ONB001["ONB-001 Real onboarding workflow"]
  CON002 --> ENR001["ENR-001 Device enrollment and ownership"]
  AUTH001 --> ENR001
  CLD001 --> ENR001
  ENR001 --> ONB001
  MCP002 --> AGT001["AGT-001 Agent connection and runs"]

  DAT001 --> MON001["MON-001 Telemetry/log pipeline"]
  CON001 --> MON001
  MON001 --> MON002["MON-002 Rule evaluation"]
  MON002 --> INC001["INC-001 Incident/evidence engine"]

  DAT001 --> JOB001["JOB-001 Durable job/lease engine"]
  RUN002 --> JOB001
  JOB001 --> APR001["APR-001 Policy and approvals"]
  JOB001 --> BLD001["BLD-001 Contained build executor"]
  CON001 --> BLD001
  BLD001 --> ART001["ART-001 Artifact provenance and registry"]
  CON003 --> OPS001["OPS-001 Build/preflight/flash/restart"]
  APR001 --> OPS001
  ART001 --> OPS001
  OPS001 --> DEP001["DEP-001 Deployment/verification/rollback"]
  INC001 --> DEP001

  APR001 --> MCP003["MCP-003 Consequential MCP tools"]
  JOB001 --> MCP003
  AGT001 --> MCP003
  BLD001 --> MCP003

  RUN003 --> CLD002["CLD-002 Sync, heartbeat and cloud projections"]
  CLD001 --> CLD002
  DAT002 --> CLD002
  CLD002 --> NOT001["NOT-001 Notification delivery"]
  INC001 --> NOT001
  NOT001 --> AGT002["AGT-002 Incident work-intent delivery"]
  AGT001 --> AGT002
  INC001 --> AGT002

  DSK002 --> UI001["UI-001 Renderer application boundary"]
  UI001 --> UI002["UI-002 Auth, onboarding and devices"]
  ONB001 --> UI002
  ENR001 --> UI002
  UI001 --> UI003["UI-003 Agents and action center"]
  AGT002 --> UI003
  NOT001 --> UI003
  UI001 --> UI004["UI-004 Dashboard, monitoring and incidents"]
  MON002 --> UI004
  INC001 --> UI004
  NOT001 --> UI004
  UI001 --> UI005["UI-005 Deployments, activity and settings"]
  DEP001 --> UI005
  CLD002 --> UI005
  DAT002 --> UI005
  UI002 --> UI006["UI-006 Desktop route integration"]
  UI003 --> UI006
  UI004 --> UI006
  UI005 --> UI006

  DSK001 --> SEC001["SEC-001 Desktop/auth/IPC/data review"]
  DAT002 --> SEC001
  MCP003 --> SEC002["SEC-002 MCP/policy/artifact review"]
  DEP001 --> SEC002
  BLD001 --> SEC002
  AGT002 --> SEC002

  UI006 --> INT001["INT-001 Full workflow integration"]
  MCP003 --> INT001
  RUN003 --> INT001
  SEC001 --> INT001
  SEC002 --> INT001
  CLD002 --> INT001

  CON003 --> HIL001["HIL-001 Physical ESP32-S3 validation"]
  DEP001 --> HIL001
  INT001 --> HIL001

  INT001 --> PKG001["PKG-001 Signed installer and runtime registration"]
  PKG001 --> PKG002["PKG-002 Signed update/recovery"]
  HIL001 --> REL001["REL-001 Release readiness"]
  PKG002 --> REL001
  REL001 --> G6["G6 Human release gate"]
```

## 3. Work waves

Waves are readiness groups, not permission to ignore graph edges.

| Wave | Purpose | Candidate parallel work |
|---|---|---|
| W0 | Accept intent and bootstrap control | G0, FND-001 |
| W1 | Establish build and contracts | FND-002, CTR-001 after baseline |
| W2 | Establish process foundations | DSK-001, RUN-001, MCP-001; DAT-001/CON-001 after contract readiness |
| W3 | Establish trust and connectivity | DSK-002, RUN-002, CLD-001, AUTH-001, CON-002 |
| W4 | Establish domain engines | RUN-003, DAT-002, MON-001, JOB-001, MCP-002, CON-003 after board decision |
| W5 | Establish workflows | ENR-001, ONB-001, AGT-001, MON-002, APR-001, INC-001, BLD-001 |
| W6 | Establish operations and delivery | ART-001, OPS-001, MCP-003, DEP-001, CLD-002, NOT-001, AGT-002 |
| W7 | Wire accepted UI | UI-001 foundation, then UI-002/UI-003/UI-004/UI-005 in parallel by disjoint route ownership, then UI-006 |
| W8 | Independent review and integration | SEC-001, SEC-002, INT-001 |
| W9 | Package and physical validation | PKG-001, HIL-001, then PKG-002 |
| W10 | Release | REL-001, G6 |

## 4. Node register

### Governance and foundation

| ID | Type | Outcome | Depends on | Exit evidence | Status |
|---|---|---|---|---|---|
| G0 | DECISION | Human accepts PRD, spec, Electron/Rust/MCP boundaries, scope, and open-decision routing | — | Accepted docs or recorded changes | accepted |
| FND-001 | MIGRATION | Git repository initialized with protected baseline, ignore rules, ownership, and current UI snapshot | G0 | clean reproducible baseline; no secret/generated content | accepted |
| FND-002 | IMPLEMENTATION | Accept and pin the `TECH_STACK.md` matrix; establish TypeScript/pnpm and Cargo workspaces, root commands, CI, formatting/lint/test config, dependency policy, and prototype dependency cleanup | FND-001 | clean reproducible install/check/build and packaged smoke on supported Windows; component/license record | proposed |
| CTR-001 | CONTRACT | Versioned shared IDs, envelopes, errors, state enums, IPC DTOs, MCP schemas, connector capability schemas | FND-001, G0 | TypeScript/Rust contract validation and compatibility fixtures | proposed |
| CLD-001 | CONTRACT | Accepted cloud identity/workspace/runtime/enrollment/policy/audit/artifact/notification contracts and signed fixtures | CTR-001, OQ-002 | contract schemas plus success/error/revocation/version fixtures accepted by cloud owner | blocked |

### Desktop, authentication, and persistence

| ID | Type | Outcome | Depends on | Exit evidence | Status |
|---|---|---|---|---|---|
| DSK-001 | IMPLEMENTATION | Existing UI runs in a packaged secure Electron shell with single-instance and truthful app lifecycle | FND-002 | Electron config/security tests and packaged smoke run | proposed |
| DSK-002 | CONTRACT | Minimal typed preload API and event subscription bridge with validation/sender checks | CTR-001, DSK-001 | IPC contract tests; forbidden generic access tests | proposed |
| DAT-001 | IMPLEMENTATION | Runtime-owned SQLite schema, migrations, repositories, outbox/inbox, integrity/backup primitives | FND-002, CTR-001 | migration/restart/corruption/transaction tests | proposed |
| DAT-002 | IMPLEMENTATION | Full-database encryption, OS-protected key lifecycle, encrypted backup/WAL, rotation/recovery, and attribution/support decision | DAT-001, OQ-007 | wrong-key/key-loss/rotation/backup/service-user boundary tests | blocked |
| AUTH-001 | IMPLEMENTATION | External-browser PKCE sign-in, session refresh, credential-store integration, and real logout | DSK-002, DAT-001, OQ-002 | AS-001/AS-002 including replay/expiry/cleanup tests | blocked |
| ONB-001 | IMPLEMENTATION | Durable account/workspace/agent/device-or-project onboarding state machine | AUTH-001, ENR-001, MCP-002 | resume/back/failure/enrolled-device and project branch scenarios | proposed |

### Runtime and connector platform

| ID | Type | Outcome | Depends on | Exit evidence | Status |
|---|---|---|---|---|---|
| RUN-001 | IMPLEMENTATION | Rust runtime executable with health, structured diagnostics, graceful shutdown, and module boundaries | FND-002 | startup/shutdown/crash/error tests | proposed |
| RUN-002 | IMPLEMENTATION | Authenticated local named-pipe RPC, version/capability handshake, deadlines, cancellation, streams | RUN-001, CTR-001 | malformed/auth/replay/version/backpressure contract tests | proposed |
| RUN-003 | IMPLEMENTATION | User background host and gateway-service lifecycle with restart/recovery and desktop discovery | RUN-002, DAT-001 | window-close/relaunch/service recovery scenarios | proposed |
| CON-001 | CONTRACT | Device-neutral connector trait, manifest, capability, health, operation, verification, recovery contracts | CTR-001 | connector conformance harness | proposed |
| CON-002 | IMPLEMENTATION | Connector registry plus discovery, identity, support, capability, and health orchestration | CON-001, RUN-002 | AS-003/AS-004 through conformance connector inputs | proposed |
| CON-003 | IMPLEMENTATION | Confirmed ESP32-S3 connector and pinned toolchain integration | CON-001, OQ-001 | connector conformance plus physical identity/recovery prerequisites | blocked |
| ENR-001 | IMPLEMENTATION | Challenge-bound device enrollment, ownership, replay protection, revoke/move policy, and consequential-capability gating | CON-002, AUTH-001, CLD-001 | AS-011 including wrong-workspace/runtime and replay tests | proposed |

### Agent access and observability

| ID | Type | Outcome | Depends on | Exit evidence | Status |
|---|---|---|---|---|---|
| MCP-001 | IMPLEMENTATION | Provider-neutral stdio MCP server with `2026-07-28` per-request metadata/optional discovery plus one bounded previous-version edge adapter required by the certified clients; schemas, errors, diagnostics, and no transport-session authority | FND-002, CTR-001 | current/previous MCP protocol, transport/discovery/initialize-edge, malformed/version/client-metadata tests | proposed |
| MCP-002 | IMPLEMENTATION | Scoped device/workspace/incident read and diagnostic resources/tools | MCP-001, RUN-002, CON-002 | AS-005; auth/scope/pagination/size tests | proposed |
| AGT-001 | IMPLEMENTATION | Revocable agent connections plus run/parent-child/tool/change event registry | MCP-002, AUTH-001 | Codex/Claude configuration contract and run lifecycle tests | proposed |
| AGT-002 | IMPLEMENTATION | Provider-neutral incident work intents, provider adapter delivery/acknowledgement, run linking, expiry, and user-action fallback | AGT-001, INC-001, NOT-001 | AS-015 with supported-client acknowledgement and unavailable-client fallback | proposed |
| MON-001 | IMPLEMENTATION | Bounded telemetry/log ingestion, quality/freshness, buffering, storage, and subscriptions | CON-001, DAT-001 | loss/gap/backpressure/restart/retention tests | proposed |
| MON-002 | IMPLEMENTATION | Versioned monitoring rules, windows, debounce/cooldown, missing-data semantics | MON-001 | deterministic rule evaluation and restart tests | proposed |
| INC-001 | IMPLEMENTATION | Incident correlation, bounded evidence, assignment, timeline, verification, reopen/close, and work-intent emission | MON-002 | AS-008 and state-transition tests | proposed |
| CLD-002 | IMPLEMENTATION | Durable cloud outbox/inbox sync, conflicts, runtime heartbeat/freshness, control-plane projections, reconnect and backpressure | CLD-001, RUN-003, DAT-002 | AS-014 and cloud contract/revocation/deduplication tests | proposed |
| NOT-001 | IMPLEMENTATION | Deduplicated action-center and Windows notification intents, preferences, delivery/read state, secure deep links, and offline alerts | CLD-002, INC-001, DSK-001 | notification privacy/dedup/deep-link/runtime-offline tests | proposed |

### Jobs, approvals, and deployment

| ID | Type | Outcome | Depends on | Exit evidence | Status |
|---|---|---|---|---|---|
| JOB-001 | IMPLEMENTATION | Durable job stages, idempotency, per-device lease, cancel/restart/reconcile/quarantine | DAT-001, RUN-002 | crash/race/retry/unknown-outcome tests | proposed |
| APR-001 | IMPLEMENTATION | RBAC/policy decision and immutable, expiring, single-use approvals | JOB-001, AUTH-001 | AS-007 plus authorization/race/binding tests | proposed |
| BLD-001 | IMPLEMENTATION | Separate AppContainer/Job Object build executor, typed recipes, immutable snapshots, resource limits, no-network/secret/device authority, quarantine, and teardown | JOB-001, CON-001, RUN-002 | AS-012/AS-013 containment, escape, exhaustion, crash, and process-tree tests | proposed |
| ART-001 | IMPLEMENTATION | Content-addressed firmware/model/config/bundle registry, external output validation, target/resource compatibility, member ordering/atomicity, build provenance, signature status, and immutable retrieval | BLD-001, DAT-002 | substitution/mislabel/type/bundle/resource/size/format/provenance and cache-invalidation tests | proposed |
| OPS-001 | IMPLEMENTATION | Artifact preflight, approved flash, restart, test, recovery, and real verification stages for reference connector | JOB-001, APR-001, ART-001, CON-003 | AS-006 stage evidence and safe-failure cases | proposed |
| MCP-003 | IMPLEMENTATION | Agent-facing contained-build and consequential operation tools mapped to approval/job pipeline with Tasks-extension support and explicit polling fallback | MCP-002, AGT-001, APR-001, JOB-001, BLD-001 | side-effect, recipe, Tasks/fallback, approval, idempotency, cancellation tests | proposed |
| DEP-001 | IMPLEMENTATION | Immutable artifact deployment, target snapshot, single target rollout, observation, verify, rollback | OPS-001, INC-001 | AS-006/AS-008/AS-009 deployment evidence | proposed |

### UI, review, packaging, and release

| ID | Type | Outcome | Depends on | Exit evidence | Status |
|---|---|---|---|---|---|
| UI-001 | CONTRACT/MIGRATION | Establish renderer application services, query/subscription contracts, common truthful states, error/freshness model, and removal boundary for browser-global authority without redesigning routes | DSK-002, CTR-001 | shared renderer contract tests; no generic IPC/storage authority | proposed |
| UI-002 | MIGRATION | Wire signed-out/auth, workspace, onboarding, discovery, enrollment, device health/detail, and project branch to real services | UI-001, ONB-001, ENR-001, CON-002 | AS-001–AS-004/AS-011 desktop E2E and route states | proposed |
| UI-003 | MIGRATION | Wire Agents, run graph, approvals/action center, notifications, and real incident work-intent delivery states | UI-001, AGT-002, NOT-001 | AS-005/AS-015 and agent/notification route E2E | proposed |
| UI-004 | MIGRATION | Wire Dashboard, Monitoring, Incidents, charts/logs, freshness/gaps, evidence, and verification timeline | UI-001, MON-002, INC-001, NOT-001 | AS-008/AS-014 and bounded stream/freshness route E2E | proposed |
| UI-005 | MIGRATION | Wire Deployments, build/provenance state, Activity, Settings, cloud/runtime/data controls, approval and rollback surfaces | UI-001, DEP-001, CLD-002, DAT-002, APR-001 | AS-006/AS-007/AS-009/AS-012/AS-016 route E2E | proposed |
| UI-006 | INTEGRATION | Integrate all eight routes, shared navigation/search/deep links, relaunch, accessibility, design regression, and remove every successful-looking presentation-only operation | UI-002, UI-003, UI-004, UI-005 | complete packaged desktop E2E and UI acceptance matrix | proposed |
| SEC-001 | VERIFICATION | Independent Electron, auth, credential, local IPC, encrypted-data/key, dependency, and notification review | DSK-001, DSK-002, AUTH-001, RUN-002, DAT-002, NOT-001 | security report; release blockers resolved | proposed |
| SEC-002 | VERIFICATION | Independent MCP, contained build, policy, approvals, connector, artifact, deployment, dispatch, and evidence review | MCP-003, BLD-001, DEP-001, AGT-002 | security report; release blockers resolved | proposed |
| INT-001 | INTEGRATION | Full desktop → runtime → build/connector/cloud/MCP/notification workflows pass on integrated baseline | UI-006, MCP-003, RUN-003, CLD-002, SEC-001, SEC-002 | software integration suite and traceability update | proposed |
| PKG-001 | IMPLEMENTATION | Signed Windows installer installs desktop/runtime/MCP, registers lifecycle, and uninstalls safely | INT-001, OQ-004 | clean install/repair/uninstall evidence | blocked |
| PKG-002 | IMPLEMENTATION | Signed atomic update with compatibility, failed-update recovery, and channel policy | PKG-001 | update/rollback/signature/tamper evidence | proposed |
| HIL-001 | HARDWARE | Required end-to-end and interruption/recovery scenarios pass on the confirmed physical ESP32-S3 | INT-001, DEP-001, CON-003 | signed operator test record, captured job/device evidence | blocked |
| REL-001 | RELEASE | Full release candidate evidence, SBOM, signing, diagnostics, traceability, known risks, and rollback reviewed | HIL-001, PKG-002 | release checklist complete | proposed |
| G6 | RELEASE | Human release owner approves merge/publish/deploy | REL-001 | attributed release decision | proposed |

## 5. Decision blockers

`OQ-*` values are defined in `Docs/CONTEXT.md` and are represented as external
decision nodes:

- `OQ-001` blocks only connector-specific flash/recovery and physical validation.
- `OQ-002` blocks cloud auth/client implementation, not Electron/runtime/contract
  foundations.
- `OQ-003` blocks only the Gravity adapter; provider-neutral MCP proceeds.
- `OQ-004` blocks final installer implementation, not packaging research.
- `OQ-005` blocks final retention defaults; persistence schemas should support
  configurable policy meanwhile.
- `OQ-006` blocks only an embedded operations-chat scope. External coding clients,
  the eight accepted tabs, runtime, and MCP proceed.
- `OQ-007` blocks the encrypted-SQLite implementation/package decision and final
  data-at-rest/release gates; schema, repository, and key-interface design may
  proceed without pretending an edition was accepted.

## 6. Ownership map

| Area | Single writer during active node |
|---|---|
| Root workspace/lockfiles/CI | Foundation or integrator |
| `packages/contracts` | CTR contract owner |
| Electron main/preload | Desktop owner |
| React route/components | UI owner, subdivided by route only after shared state contract is accepted |
| Rust domain/runtime/IPC/storage | Respective runtime package owner |
| Contained build executor and recipes | Build-executor owner; connector owner owns only connector-declared recipe contract |
| Cloud client/contracts and sync | Cloud-contract/runtime-sync owner |
| Notifications and provider delivery | Notification/agent-adapter owner after incident contract acceptance |
| Connector SDK | Connector contract owner |
| ESP32 connector | ESP32 connector owner |
| MCP server | MCP owner |
| Security documents/findings | Security reviewer |
| Task graph/status | Orchestrator |
| Release manifest/signing | Release owner |

## 7. Critical path

The likely critical path is:

```text
G0 → FND-001 → FND-002/CTR-001 → RUN-001/RUN-002 → DAT-001/JOB-001
→ CON-001/BLD-001/ART-001 → CON-003/APR-001/OPS-001 → DEP-001
→ UI-001 → UI-002/UI-003/UI-004/UI-005 → UI-006/INT-001
→ HIL-001/PKG-001/PKG-002 → REL-001 → G6
```

Cloud contract/sync, auth, enrollment, MCP, telemetry, incident, notification, and
agent-delivery work should run beside this path when their dependencies and write
ownership permit.

## 8. Status-update record

For every status change, the orchestrator records:

```text
node_id:
from:
to:
timestamp:
actor:
starting_revision:
result_revision:
evidence:
reason:
unblocked_nodes:
new_or_changed_risks:
```

Do not place verbose worker logs in this always-loaded graph. Link their handoff
and evidence records.

```text
node_id: G0
from: proposed
to: accepted
timestamp: 2026-08-15T00:00:00Z
actor: human product/release owner (shrustixrajput@gmail.com), recorded by orchestrator (Claude Code)
starting_revision: n/a (repository not yet under Git; see FND-001)
result_revision: n/a
evidence: explicit chat acceptance — "I accept G0 for LokiAI Main v1.2: the PRD, system
  specification, locked Electron/React/Rust/MCP architecture, and routing of unresolved
  decisions are accepted as the implementation baseline. OQ-001 through OQ-007 remain
  blockers only for their affected branches."
reason: Human product/release owner exercised the G0 human gate defined in
  Docs/graph/PLANNER.md section 10 and AGENTS.md human-approval boundaries, accepting
  Docs/PRD.md, Docs/SPEC.md, and the locked architecture in Docs/ARCHITECTURE.md
  (Electron shell / untrusted React renderer / narrow preload / separate Rust runtime /
  contained build executor / provider-neutral MCP server) as the implementation
  baseline for LokiAI Main v1.2.
unblocked_nodes: FND-001 (packet drafted this record; not yet dispatched — see
  Docs/graph/tasks/FND-001.md)
new_or_changed_risks: OQ-001 (ESP32-S3 SKU), OQ-002 (cloud/auth contracts), OQ-003
  (Gravity adapter), OQ-004 (Windows distribution channel), OQ-005 (retention
  defaults), OQ-006 (embedded ops chat scope), OQ-007 (encrypted-SQLite edition)
  remain open per Docs/CONTEXT.md and continue to block only their named descendant
  nodes/branches per TASK_GRAPH.md section 5; no new risk introduced by this
  acceptance alone.
```

```text
node_id: FND-001
from: ready
to: accepted
timestamp: 2026-08-15T11:01:01Z (commit); force-push correction below at a later
  timestamp on the same date (real wall-clock time not re-queried for the
  correction step)
actor: orchestrator (Claude Code), executing under explicit human approval recorded
  in Docs/graph/tasks/FND-001.md Section 10
starting_revision: pre-Git filesystem snapshot, 2026-08-15 (no .git existed)
result_revision: e5a75c3a06dc1f73c486eb7e4bac4c20b71b5b47 (branch LokiAI-v1.2, tree
  69ba2b2d8c989161a31c25706ee428223bed1229), pushed to
  https://github.com/The-Shriks/LokiAI-v1.2
evidence: Docs/graph/handoffs/FND-001-1.md (worker handoff); git log/status/
  ls-remote output quoted in that handoff; branch name, remote, PromptForYou.md
  exclusion, and one-time push all per explicit human instruction.
reason: FND-001 packet accepted, dispatched, and executed to completion this turn
  per human approval. All Section 7/8 acceptance and verification criteria in
  Docs/graph/tasks/FND-001.md passed. Correction: the first pushed commit
  (0ad5a7f0c1d5e71acbffb2b1b6d984bede7229fd) included a "Co-Authored-By: Claude"
  trailer that GitHub rendered as a second co-author on the commit; the human
  flagged this as looking like an unauthorized second account and asked for a
  single-author commit. The commit message (not its file content — tree hash
  unchanged: 69ba2b2d8c989161a31c25706ee428223bed1229) was amended to remove the
  trailer, producing e5a75c3, and force-pushed with `--force-with-lease` guarded
  against the exact prior known remote SHA, with explicit human approval for that
  specific force-push. No new GitHub account or repository was created at any
  point; only the pre-existing empty remote the human named was used.
unblocked_nodes: FND-002 (Monorepo/build foundation) is now dependency-ready but
  remains undispatched — human instruction scoped this turn to FND-001 only
  ("Part One"); no further node was started.
new_or_changed_risks: none beyond those already tracked under G0's status-update
  record above.
```
