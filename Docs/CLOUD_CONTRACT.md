# Cloud Control-Plane Contract — LokiAI Main v1.2

Status: **Normative client-side contract; concrete endpoints blocked by OQ-002**  
Updated: **2026-08-15**

## 1. Boundary

The existing LokiAI website/control plane remains a separate deployment and is
not rebuilt in this repository. This document defines what LokiAI Main requires
from it so desktop, runtime, MCP, and website behavior form one end-to-end
product.

No worker may invent endpoint URLs, token claims, tenant semantics, successful
responses, artifact locations, or notification delivery. `CLD-001` converts the
accepted OQ-002 inputs into versioned schemas and contract fixtures before auth
or cloud synchronization is implemented.

## 2. Required capabilities

### Identity and workspace

- OAuth/OIDC Authorization Code with PKCE, exact redirect registration, state,
  nonce, token audience, expiry, refresh, and revocation behavior;
- account and workspace membership/role projection with version or ETag;
- create/join/select workspace according to server-side authorization;
- role/policy changes that invalidate stale local capability projections.

### Runtime and device enrollment

- one installation record per runtime identity, with host mode, supported
  protocols, version, update channel, public-key/credential reference, and
  revocation state;
- a short-lived, one-time enrollment challenge that binds a discovered physical
  identity to a workspace and runtime after an authorized human confirms it;
- heartbeat/lease state that distinguishes online, stale, offline, revoked, and
  version-incompatible runtimes;
- device/fleet metadata synchronization without exposing raw hardware identifiers
  outside authorized scope.

### Policy, audit, and configuration

- versioned policy and monitoring configuration with effective time and actor;
- append-only audit/event ingestion with event ID, idempotency key, correlation,
  causal metadata, and acknowledgement checkpoint;
- inbox events with stable IDs and ordered checkpoint semantics where order is
  required;
- retention, privacy, notification, and update-channel configuration.

### Artifacts and evidence

- authorized upload/download references with digest, size, media/type,
  compatibility claims, expiry, and immutable object identity;
- signature/provenance metadata separate from user-controlled labels;
- bounded incident evidence references with tenant authorization and retention;
- no cloud reference is considered proof until the receiver validates content
  digest and schema.

### Notifications and agent delivery

- durable notification intents for incident, approval, operation, runtime
  offline, update, and security events;
- delivery status and deduplication across in-app and OS notification surfaces;
- provider-adapter dispatch records when a supported coding client can receive or
  resume work;
- an honest `user_action_required` fallback when a provider cannot be awakened
  safely or no active client exists.

## 3. Contract rules

All APIs use authenticated HTTPS/WSS where streaming is accepted and shall:

- carry explicit API/schema version, request ID, correlation ID, UTC time, actor,
  workspace, and idempotency key where retried;
- enforce authorization in the cloud, not trust a desktop-supplied role;
- use cursor/checkpoint pagination for unbounded collections;
- publish maximum request, page, evidence, and stream-frame sizes;
- define timeouts, retryable error codes, retry-after, and rate limits;
- use conditional update/version checks for policy and mutable projections;
- reject cross-workspace identifiers even when syntactically valid;
- support at least the documented rolling compatibility window;
- provide a non-production contract environment or signed fixtures without
  sharing production credentials.

Cloud push messages are hints to fetch authorized current state. A push payload
does not directly approve, flash, restart, execute a command, or resolve an
incident.

## 4. Local synchronization

The runtime uses a transactional outbox and durable inbox checkpoints:

1. Commit the local domain event and outbox record together.
2. Send with stable event ID and idempotency key.
3. Validate the authenticated acknowledgement before advancing the checkpoint.
4. Retry with bounded exponential backoff and jitter.
5. Deduplicate inbound events before applying a state transition.
6. Resolve safety-critical conflicts using explicit version/policy rules, never
   timestamp-only last-write-wins.

The queue is bounded by retention policy. When it cannot safely accept more data,
LokiAI reports degraded synchronization and applies defined backpressure; it does
not silently drop audit or operation results.

## 5. Offline behavior

- Previously authenticated local development may continue only for capabilities
  allowed by the last unexpired policy and local role projection.
- Cloud-required actions, new agent connections, membership changes, remote
  approvals, and cloud artifact fetches fail with a precise offline state.
- A runtime whose heartbeat expires is shown as stale/offline, not as a healthy
  observer. Device state from it is marked last-known with freshness.
- A powered-off workstation cannot receive an incident or perform remediation.
  Continuous service requires an always-on gateway runtime.
- Queued events and notifications resume idempotently after reconnection.

## 6. Agent incident dispatch

Incident creation emits a provider-neutral `work_intent` containing only scoped,
bounded references: workspace, incident, severity, requested capability, evidence
references, correlation, and expiry. The delivery coordinator then:

1. evaluates workspace policy and connected-client capabilities;
2. chooses an enabled provider adapter without embedding provider logic in the
   incident domain;
3. records queued, delivered, acknowledged, expired, failed, or
   `user_action_required` state;
4. never grants additional MCP or device authority through delivery;
5. links the resulting agent run when the client acknowledges it.

Automatic remediation still follows normal policy, approval, job, verification,
and audit stages. Delivery is not authorization.

## 7. Acceptance

- Accepted OpenAPI/JSON Schema or equivalent contract exists for every required
  capability.
- Auth issuer, audience, redirect, tenant, role, revocation, and refresh rules are
  recorded from the live platform owner.
- Contract tests cover success, expiry, revocation, cross-tenant access,
  malformed/oversized data, retry, deduplication, rate limit, and version skew.
- Disconnect/reconnect preserves event and audit integrity.
- Runtime heartbeat expiry creates an honest stale/offline projection and
  notification.
- Incident dispatch records real provider delivery or a user-action fallback; it
  never fabricates an active agent run.

