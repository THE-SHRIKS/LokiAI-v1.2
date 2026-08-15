# MCP Server Specification — LokiAI Main v1.2

Status: **Normative proposed contract**  
Updated: **2026-08-15**

## 1. Purpose

The LokiAI MCP server gives coding agents a safe, provider-neutral way to inspect
hardware context and request LokiAI workflows. It is an adapter to LokiAI domain
authority, not a direct serial bridge, shell, or replacement policy engine.

One server serves Codex, Claude Code, Cursor, and compatible future clients.
Provider integrations differ only in discovery/configuration, credential handoff,
and optional run-event adapters.

## 2. Process and transport

- v1.2 default transport: local stdio, started by the coding client.
- MCP server authenticates to the local runtime through the versioned local IPC
  protocol and uses a workspace-scoped agent credential.
- The coding client never receives the runtime's installation secret.
- Remote Streamable HTTP is disabled until separately accepted and threat-modeled.
- One process may serve one client connection; runtime arbitrates shared device
  leases and policies across clients.
- The server is a standalone Node 24 LTS distribution using the official Tier-1
  TypeScript SDK for the accepted MCP specification. It ships a supported
  launcher/runtime and does not depend on Electron's embedded Node or a global
  user installation.

## 3. Connection flow

1. User selects a coding client in LokiAI Settings or onboarding.
2. LokiAI creates a revocable agent connection with workspace and capability
   scope.
3. LokiAI generates or installs client configuration using exact executable path
   and a credential reference/secure handoff mechanism.
4. Client starts MCP server over stdio.
5. Each MCP request carries the accepted protocol version and client identity/
   capabilities in request metadata. The server may answer optional
   `server/discover`, authenticates the workspace connection, and exposes only the
   authorized capability set.
6. LokiAI shows last handshake, client/server/runtime versions, scope, and revoke.

Configuration output must never print a reusable plaintext secret into shared
logs or agent conversation.

For MCP `2026-07-28`, LokiAI uses the stateless protocol: each request is
self-describing, optional `server/discover` replaces a required handshake, and
there is no MCP session ID. The server also maintains one explicitly bounded
previous-version edge adapter when required by the certified Codex, Claude Code,
or Cursor releases. A legacy initialize exchange is compatibility metadata only;
it never becomes authorization or hidden domain state. Application continuity
always uses explicit LokiAI agent-run, job, approval, and work-intent IDs.

## 4. Naming and schemas

- Proposed tool prefix: `loki_`.
- Tool names are stable, action-oriented, and provider-neutral.
- Every input/output has a runtime schema and version.
- Every response includes or can resolve a correlation ID and timestamp.
- Pagination uses opaque cursors and bounded limits.
- Long-running or device-changing tools return a durable `job_id` or
  `approval_required`; they do not block until an operation looks complete.

## 5. Proposed resources

| Resource URI family | Contents | Sensitivity |
|---|---|---|
| `loki://workspaces/{id}/summary` | authorized workspace/runtime/capability summary | internal |
| `loki://devices/{id}` | identity projection, connector, capability, health/freshness | sensitive |
| `loki://devices/{id}/logs` | bounded redacted log page/reference | sensitive, untrusted text |
| `loki://devices/{id}/telemetry` | bounded signal query/result with quality | sensitive |
| `loki://incidents/{id}` | state, affected scope, correlations, verification | sensitive |
| `loki://incidents/{id}/evidence` | bounded evidence manifest and authorized entries | sensitive, untrusted text |
| `loki://deployments/{id}` | artifact/target/stage/verification/rollback projection | sensitive |
| `loki://jobs/{id}` | durable stage/result/error projection | internal |
| `loki://agents/runs/{id}` | run hierarchy and LokiAI tool/job associations | internal |

Resources expose projections, not direct database rows or arbitrary files.

## 6. Proposed tools

Final schemas are owned by `CTR-001`; this list defines product intent.

### Inspection — low risk

- `loki_list_devices`
- `loki_get_device`
- `loki_get_device_health`
- `loki_read_device_logs`
- `loki_query_telemetry`
- `loki_list_incidents`
- `loki_get_incident`
- `loki_get_job`
- `loki_list_deployments`
- `loki_get_deployment`

All enforce workspace scope, pagination/bounds, redaction, source, and freshness.

### Diagnosis and build — low/medium risk by policy

- `loki_run_health_check`
- `loki_run_preflight`
- `loki_collect_incident_evidence`
- `loki_register_agent_run`
- `loki_update_agent_run`
- `loki_request_build`
- `loki_cancel_job`

Cancellation works only at declared safe points. Build inputs are constrained to
an approved repository root, revision, target, and typed toolchain recipe. They
cannot supply a raw command, executable, script, URL, environment map, or
unrestricted path. Execution follows
[`BUILD_EXECUTION.md`](BUILD_EXECUTION.md).

### Consequential operations — high risk by default

- `loki_request_restart`
- `loki_request_flash`
- `loki_request_test_on_device`
- `loki_request_deployment`
- `loki_request_rollback`

These tools create a canonical request, evaluate policy, and return either denial,
approval requirement, or durable job. They never accept raw commands or bypass
approval with a boolean supplied by the agent.

When the client supports `io.modelcontextprotocol/tasks`, long-running build and
operation calls expose the LokiAI durable job through the Tasks extension. The
runtime job ID and state machine remain authoritative. Clients without the
extension receive the same explicit job ID and may call `loki_get_job`; no
behavioral capability is lost.

## 7. Common result behavior

Successful read:

```text
{
  data,
  source,
  observed_at,
  freshness,
  correlation_id,
  next_cursor?
}
```

Operation request:

```text
{
  outcome: "rejected" | "approval_required" | "accepted" | "deduplicated",
  approval_id?,
  job_id?,
  bound_request_digest?,
  policy_reason,
  correlation_id,
  status_resource?
}
```

Errors use stable codes such as authentication required, permission denied,
workspace mismatch, runtime unavailable, device unavailable, unsupported
capability, device busy, approval required/expired, artifact mismatch, invalid
state, version incompatible, rate limited, and internal correlation error.

## 8. Agent-run graph

- MCP connections identify client and optional run/session ID.
- Run registration records provider-reported parent/child relationships and phase.
- LokiAI independently links tool requests, approvals, jobs, incidents, and
  deployments using correlation IDs.
- Provider-reported changed files or phase are claims, not proof of repository or
  device state.
- A subagent inherits only the capability scope actually attached to its
  connection/run; parent authority is not assumed.
- Incident delivery creates a provider-neutral work intent, not a fabricated run
  or expanded scope. A run is linked only after a real client acknowledgement;
  otherwise the state remains `user_action_required` as defined by
  [`CLOUD_CONTRACT.md`](CLOUD_CONTRACT.md).

## 9. Security and safety

- Inputs are schema-validated and bounded before domain calls.
- Every request validates its declared MCP protocol version, client metadata, and
  capabilities; authorization never depends on a hidden transport session.
- A previous-protocol adapter is isolated at the MCP edge, has compatibility
  fixtures and a removal date, and cannot weaken current schemas or policy.
- Repository paths are canonicalized under approved roots.
- Device/log/repository text is returned with untrusted provenance.
- No prompt content can set role, policy, approval, target identity, or artifact
  digest.
- Read tools cannot mutate through cache refresh side effects beyond ordinary
  audited observation.
- Consequential tools require stable target identity and connector capability.
- Tool requests and results are auditable and redact secrets.
- Rate/concurrency limits apply per agent connection, workspace, runtime, device,
  and costly operation.

## 10. Provider adapter requirements

For each supported client, provide:

- supported versions/platforms;
- executable/config discovery;
- exact MCP configuration shape;
- secure credential handoff;
- connect, validate, revoke, and repair flows;
- provider limitations, especially automatic wake/resume and run-event detail;
- protocol-version, optional discovery, Tasks-extension, and fallback support;
- an end-to-end test using the same core MCP schemas.

Do not promise that every client can be automatically awakened after an incident.
LokiAI may create/queue the incident and expose it; automatic dispatch/resume is a
provider-specific capability and must be labeled accurately.

## 11. Acceptance

- Codex and Claude Code can connect to the same server contract and perform AS-005.
- Unauthorized workspace/device access is rejected without data leakage.
- Oversized/malformed/replayed requests fail safely.
- An agent can request but cannot self-approve a flash/deployment.
- Approval input changes invalidate the request.
- Device operation result tracks the durable job through verification.
- Agent run, tool, approval, job, deployment, incident, and activity correlations
  appear consistently in the desktop UI.
- The installed launcher works on a clean supported Windows system without a
  globally installed Node and reports its own/runtime protocol versions.
- Build inputs cannot escape the registered recipe and contained executor.
- Incident work-intent delivery records real acknowledgement or
  `user_action_required`, never invented activity.
- MCP `2026-07-28` per-request metadata and optional discovery pass. The one
  accepted previous-version adapter also passes for provider releases that still
  require it, without using transport session state for authorization/domain
  continuity.
- Supported clients use the Tasks extension for long-running jobs where available
  and fall back to explicit `loki_get_job` polling without changing authority.
