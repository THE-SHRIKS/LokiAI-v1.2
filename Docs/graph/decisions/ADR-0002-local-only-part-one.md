# ADR-0002 — Local-only Part One milestone

Status: `accepted`  
Date: `2026-08-24`  
Decision owners: `human product owner; orchestrator`  
Supersedes: `none`  
Superseded by: `none`

## Context

The original Part One graph requires an accepted cloud identity, workspace,
enrollment, synchronization, notification, and work-intent contract. Those
inputs remain blocked by `OQ-002`. On 2026-08-24 the human product owner
explicitly instructed the orchestrator to exclude cloud work for now and finish
the remaining Part One milestone without stopping.

Simply deleting the two `CLD-*` nodes would be unsafe: the current graph also
makes browser authentication, workspace onboarding, device enrollment, agent
connections, approvals, notifications, and work-intent delivery depend on cloud
authority. The milestone therefore needs an explicit local-only security and
scope boundary rather than a silent dependency bypass.

## Decision drivers

- Deliver a real, testable local core without waiting for an undefined cloud API.
- Retain authentication, authorization, approval, encryption, IPC, containment,
  stable-device-identity, and audit controls.
- Avoid inventing cloud tenant, account, workspace, role, enrollment, or
  revocation semantics.
- Keep the deferred cloud slice additive and recoverable in a later graph wave.

## Considered options

### Option A — Keep Part One blocked on the cloud contract

- How it works: retain the existing graph and wait for `OQ-002`.
- Benefits: no scope or dependency changes.
- Costs/risks: local implementation remains structurally blocked by an external
  system that the owner has explicitly deferred.
- Migration/rollback: none.

### Option B — Remove cloud checks and trust local callers

- How it works: bypass authentication and policy when cloud is unavailable.
- Benefits: minimal implementation effort.
- Costs/risks: unacceptable security downgrade; renderer or agent compromise
  could become device or host authority.
- Migration/rollback: rejected; this option must never be implemented.

### Option C — Local installation authority with fail-closed cloud separation

- How it works: define a local installation-owner/session authority protected by
  the Windows identity and existing authenticated IPC boundary. Local agent
  connections are explicitly registered and revocable. Consequential requests
  still pass policy and require immutable, expiring, single-use, digest-bound
  human approvals. Cloud-only operations return an explicit unavailable/deferred
  result. Local device binding is installation-scoped and is never represented
  as cloud ownership or workspace enrollment.
- Benefits: preserves the security boundary while unblocking a useful local core.
- Costs/risks: cloud account/workspace/RBAC/enrollment/revocation and remote
  notification/work-intent behavior remain unavailable until a later milestone.
- Migration/rollback: add the accepted cloud authority later through versioned
  contracts and an explicit migration from local installation records; never
  silently reinterpret a local owner as a cloud principal.

## Decision

Adopt Option C. Revise the Part One completion gate to certify a **local-only
core baseline**. Defer `CLD-001`, cloud PKCE `AUTH-001`, cloud workspace
`ONB-001`, cloud ownership `ENR-001`, `CLD-002`, and all remote notification or
remote provider-delivery behavior. Replan the local portions of agent
registration, policy/approval, notification intents, and incident work-intent
delivery around a Windows-protected installation authority. Keep every
consequential hardware/build operation behind the existing job, policy,
approval, containment, target-identity, verification, and audit boundaries.

## Consequences

### Positive

- The local runtime, encrypted persistence, MCP, monitoring, incidents, jobs,
  agents, approvals, connector, artifact, and operation services can be tested
  together without a fake cloud.
- Cloud absence cannot silently grant authority.
- Later cloud integration remains a separately reviewable contract and migration.

### Negative/tradeoffs

- No real cloud login, multi-user workspace, remote RBAC, remote enrollment,
  cloud sync, remote notification delivery, or remote agent wake/resume exists in
  the revised Part One milestone.
- Local installation ownership is single-host authority, not an enterprise
  account/workspace substitute.

### Required controls

- Bind local authority to the authenticated Windows user/service identity and
  OS-protected secrets; never expose authority material to renderer or MCP.
- Default-deny unknown local callers and cloud-required operations.
- Keep agent connections scoped, revocable, allowlisted, and auditable.
- Keep consequential approvals immutable, expiring, single-use, and bound to the
  canonical request, actor, target, operation, and artifact digest.
- Preserve AppContainer/Job Object containment and stable device identity gates;
  deferring cloud never waives `BLD-001`, `CON-003`, or physical-operation safety.
- Use honest `cloud_unavailable`/`not_configured` states; no mock endpoints,
  accounts, roles, workspaces, notifications, or acknowledgements.

## Compatibility and migration

- Affected components/contracts: task graph, local identity/session service,
  agent registry, policy/approval service, notification intents, incident
  work-intents, Part One integration gate, and traceability.
- Data/protocol migration: local authority records use a distinct namespace and
  schema from future cloud identities. A future cloud node must define explicit
  link/claim/revoke migration rules.
- Rolling compatibility: cloud capabilities are absent from negotiated local
  capability sets until their contracts are accepted.
- Rollback/reversal plan: restore the original cloud-inclusive milestone only
  after `OQ-002`, `CLD-001`, and its security review are accepted.

## Verification

- Unknown Windows principals and invalid IPC sessions cannot acquire local owner
  or agent authority.
- Cloud-required requests fail closed and do not create fake local success.
- Approval mutation, expiry, replay, actor/target mismatch, and reuse tests pass.
- Local agent registration/revocation and incident work-intent acknowledgement
  tests pass without a network dependency.
- The revised integrated baseline passes the documented format, lint, type,
  unit, contract, process-integration, dependency/licence, and secret checks.

## Human acceptance

- Decision: `accepted`
- Actor: `human product owner`
- Timestamp: `2026-08-24`
- Conditions: `Exclude cloud work for now; continue until the revised local-only Part One milestone is complete. Do not weaken security controls, fabricate cloud behavior, or delete deferred cloud requirements.`
