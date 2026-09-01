# Task Packet — `P1-RUNTIME-COMP-001` Compose RuntimeServices Into the IPC Server: Real Read + Agent-Mutation Business RPC

Packet status: `proposed` (ready for orchestrator review/dispatch)
Graph status: `proposed` → recommend `ready` on save (both `depends_on` edges accepted)
Created by: `loki-planner` (read-only)
Created at: `2026-08-31T00:00:00Z`
Starting revision: current tip of branch `LokiAI-v1.2` (post `80bff05`).

## 1. Objective

Install a real `loki_ipc::server::BusinessService` implementation, backed by
the already-composed `crate::services::RuntimeServices`
(`P1-RUNTIME-SERVICE-READINESS-CORR`, `crates/loki-runtime/src/services/**`),
into the running `ComposedHost` (`crates/loki-runtime/src/host/compose.rs`),
replacing `UnsupportedBusinessService` — the first revision at which the
`loki-runtime` process can actually serve a real business RPC call, not just
`Ping`/`EchoStream`.

**Scope is narrowed to 8 of the 10 real-backed business methods**, by
orchestrator scheduling decision (2026-08-31, chat) following this planner's
direct-inspection findings:

- `Ping`, `EchoStream` — already real (`RUN-002`), unaffected, must keep
  passing.
- The 4 real read methods — `RuntimeGetStatus`, `DevicesList`, `DeviceGet`,
  `DeviceGetHealth` — wired to `RuntimeReadPort`. No blocker.
- The 3 agent-mutation methods — `AgentRunRegister`, `AgentRunUpdate`,
  `WorkIntentAcknowledge` — wired to `MutationSubmissionPort`, using a real,
  non-fabricated `grant_generation` read (`Storage::get_agent_connection`)
  under the `principal_id == connection_id` convention this packet fixes
  (§6 Deliverable 3, §9).
- **Explicitly out of scope, deferred to a named follow-up**: the 3
  desktop-approval-gated mutation methods — `RestartRequest`,
  `FlashRequest`, `BuildRequest`. These require a real, current
  `LocalAuthority` session/grant generation for the calling
  `DesktopInstallation` principal, and no code path anywhere in this
  workspace today binds a Hello-authenticated IPC session to an issued
  `LocalAuthority` session (confirmed by direct inspection of
  `crates/loki-local-authority/src/store.rs` and
  `crates/loki-storage/src/local_authority.rs` — see §3, §9). Wiring them
  now would require fabricating a `grant_generation`, forbidden by
  `AGENTS.md` rule 1 and the exact "never fabricate a grant/generation"
  ethos `crates/loki-runtime/src/services/mutation.rs`'s own doc comments
  state repeatedly. These 3 methods **stay `RpcError::UnsupportedMethod`**
  in this node's `BusinessService`, exactly as they are today under
  `UnsupportedBusinessService` — this is not a regression, and must not be
  reported as `COMPLETE` scope; it is a tracked, named gap (§9, §11).
- The 13 permanently-unsupported methods (per `crates/loki-ipc/src/
  business.rs`'s own top doc comment) and all 10 business streams remain
  `RpcError::UnsupportedMethod` — no real backing type exists for them
  anywhere in this workspace; inventing one is out of this node's scope.

This node also proves real `ComposedHost`-level process lifecycle behavior
(startup, in-process restart/reconcile via reopening the same on-disk state,
and failure handling) for the newly-real business-RPC path — bounded by the
`DAT-002-MEM-HOST-VERIFY-001` gate already documented in
`crates/loki-runtime/tests/process_behavior.rs` (§7, §9): true black-box
spawns of the compiled `loki-runtime` binary in `Run` mode are not currently
possible (the binary fails closed at `SecureMemoryRuntime::bootstrap()`
until that separately-gated node lands), so this node must not attempt to
force such a test to pass or weaken that fail-closed gate to get evidence.

## 2. Traceability

- Requirements/architecture: `RUN-002`, `CTR-001`, `ADR-0003`, `ADR-0006`
  (+Amendment 1), `ADR-0007` (grant-generation identity unification — the
  agent-connection half of this node's mutation wiring rests directly on
  Option B's real `agent_connections.generation` column).
- Security: `Docs/SECURITY.md` IPC/local-client-impersonation section;
  `AGENTS.md` locked boundary — "Desktop-to-runtime communication uses an
  authenticated, versioned local IPC contract"; `AGENTS.md` rule 1 ("do not
  invent... sessions... successful operations").
- Parent/consumed contracts: `P1-SERVICE-READINESS-001-R1` (accepted,
  `crates/loki-runtime/src/services/**`), `P1-RPC-CONTRACT-001-R1` (accepted,
  `crates/loki-ipc/src/{server.rs,business.rs,auth.rs}` — the
  `BusinessService` seam and closed 23-method vocabulary this node fills).
- Sibling/related: `P1-NODE-IPC-CLIENT-001` (`proposed`) — its own pending
  credential-distribution decision does **not** block this node (§3, §9);
  this node's own newly-found session/grant-binding gap for
  `DesktopInstallation` principals should be tracked alongside, not
  conflated with, that decision.
- Follow-up this node's handoff must name explicitly: a not-yet-created node
  (working title `P1-RUNTIME-COMP-DESKTOP-MUTATION-001`) for
  `RestartRequest`/`FlashRequest`/`BuildRequest`, itself gated on a not-yet-
  opened decision covering how a Hello-authenticated `DesktopInstallation`
  IPC principal is bound to a real `LocalAuthority` session/generation.

## 3. Dependencies and inputs

- `depends_on`: `P1-SERVICE-READINESS-001-R1` (accepted), `P1-RPC-CONTRACT-001-R1`
  (accepted). Both satisfied at current tip `80bff05` — confirmed by direct
  inspection, not narrative (see §13).
- Consumed contracts/exact revisions:
  - `loki_runtime::services::{RuntimeReadPort, MutationSubmissionPort,
    RuntimeServices, RuntimeStatus, DevicePage, DevicePageRequest,
    DeviceSummary, DeviceHealthProjection, DeviceHealthState,
    PersistedHealthEvidence, StorageReadiness, MutationAvailability,
    MutationSubmissionError, AcceptanceResult, MutationAcceptanceRequest}` —
    read-only, current working-tree shape.
  - `loki_ipc::server::{BusinessService, BusinessCallFuture,
    BusinessStreamFuture, BusinessRequestContext, BusinessEventEmitter,
    ServerConfig, mutation_method_for}`, `loki_ipc::business::{Method,
    StreamMethod, BusinessRpcResult, AuthenticatedPrincipal, PrincipalKind,
    Authority}` — read-only, accepted revision.
  - `loki_mutation_contract::{MutationAcceptanceRequest, MutationMethod,
    IdempotencyKey, AcceptanceResult}` — read-only, accepted revision.
  - `loki_storage::agents::{AgentsRepository, AgentConnectionRow}` — new
    read-only use of `get_agent_connection` (a plain, ungated SELECT,
    already present — no schema/API change needed).
- Required environment/credential/device: none beyond what already exists
  (real Windows named pipes, existing DPAPI-protected token path). No new
  credential class, no device/hardware.
- Known starting conditions (confirmed by direct inspection, 2026-08-31):
  - `ComposedHost::open_with_arena` (`crates/loki-runtime/src/host/
    compose.rs:158-203`) already composes `RuntimeServices::compose(storage)`
    and binds a real `Server` via `ServerConfig::new(pipe_name,
    AuthToken::new(raw_token))`, but that config installs
    `Arc::new(UnsupportedBusinessService)` — every business method fails
    closed today. This is the exact line this node changes
    (`.with_business_service(...)`).
  - `ServerConfig::new`'s single legacy principal
    (`AuthenticatedPrincipal::runtime_service("legacy-core-client")`) has
    **empty `authority` and empty `authorized_entity_ids`**
    (`crates/loki-ipc/src/business.rs:132-139`). This node does not change
    that — production business RPC remains unreachable by any real caller
    through today's single-token path regardless of this node's own
    correctness; this is `P1-NODE-IPC-CLIENT-001`'s pending
    credential-distribution decision to resolve, not this node's. This
    node's own tests supply their own `CredentialSet`/principal (§7), which
    does not touch or require that decision.
  - No wire method param (`RestartRequestParams`, `AgentRunRegisterParams`,
    etc., `crates/loki-ipc/src/business.rs`) carries a `principal_sid` or
    `grant_generation` field — confirmed by reading every `*Params` struct.
    `MutationAcceptanceRequest::new` requires both
    (`crates/loki-mutation-contract/src/request.rs:83-108`). The
    `BusinessService` implementation must derive them server-side from the
    authenticated `AuthenticatedPrincipal`, never accept them from the wire.
  - For `PrincipalKind::AgentConnection`: `Storage::get_agent_connection
    (connection_id)` (`crates/loki-storage/src/agents.rs:118-121,201-207,
    301-309`) returns a real `AgentConnectionRow` including a real,
    currently-persisted `generation: u64` column — an honest, non-fabricated
    read, no authorization gate needed for the read itself (authorization is
    separately enforced upstream by `crate::server::authorize_method` before
    this is ever reached).
  - For `PrincipalKind::DesktopInstallation`: no equivalent "current
    generation for this principal" read exists.
    `local_authority_sessions.generation` (ADR-0007 Option B,
    `crates/loki-storage/src/local_authority.rs`) is assigned per-session at
    `issue_session` time; `find_local_generation(principal_sid, generation)`
    only *verifies* a caller-presented candidate, it does not discover
    "current." No code anywhere binds the IPC transport's own `session_id`
    (`crate::server::SessionAuthority.session_id`, a fresh UUID minted at
    every `Hello`) to a `LocalAuthority` session. This is a real, structural
    gap, not a "which token format" question — hence the explicit exclusion
    of the 3 desktop-mutation methods (§1).
  - `RuntimeServices` already composes `_local_authority: LocalAuthority` and
    `_agents: AgentRegistry` but both fields are `_`-prefixed (dead code,
    unused) and neither is exposed through `RuntimeReadPort`/
    `MutationSubmissionPort`. This node does not need to change that for its
    narrowed scope (agent-generation resolution goes through `Storage`
    directly inside the new `BusinessService`, not through
    `RuntimeServices`/`AgentRegistry` — see §5 ownership boundary note).
  - `crates/loki-runtime/tests/process_behavior.rs` already has two
    `#[ignore]`d black-box tests
    (`ctrl_break_triggers_graceful_shutdown`,
    `run_mode_composes_a_real_ipc_listener_that_accepts_a_connection_then_shuts_down_cleanly`)
    citing `DAT-002-MEM-CODE-001`/`DAT-002-MEM-HOST-VERIFY-001`: the real
    compiled `loki-runtime` binary fails closed in `Run` mode until that
    separately-gated node (not yet accepted, blocked on VM/elevation
    approval per `TASK_GRAPH.md`) lands. This node's own new black-box tests
    must follow the same pattern (§7).
  - `ComposedHost::open_with_test_arena` (`crates/loki-runtime/src/host/
    compose.rs:148-153`) already exists, compiled only under `cfg(test)` or
    the opt-in `test-support` feature, specifically so a consumer of
    `ComposedHost` can bypass the production secure-memory bootstrap gate in
    tests without touching `open()`'s real production path. This is this
    node's primary process-level test seam (§7).

## 4. Required reading

- `AGENTS.md`, `Docs/ARCHITECTURE.md` §3 (host modes), §9 (event/sync flow),
  `Docs/SECURITY.md` (IPC section), `Docs/TESTING.md`
- `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
  (+Amendment 1), `Docs/graph/decisions/ADR-0007-grant-generation-identity-unification.md`
- `crates/loki-runtime/src/host/compose.rs` (edit target)
- `crates/loki-runtime/src/host/{token.rs,discovery.rs,paths.rs,supervisor.rs}`
  (existing startup/restart/discovery conventions to extend, not replace —
  no edits expected/needed to these files)
- `crates/loki-runtime/src/services/{mod.rs,read.rs,mutation.rs}` (consume
  exactly as designed — do not edit)
- `crates/loki-ipc/src/{server.rs,business.rs,auth.rs}` (consume the
  `BusinessService` trait, `Method`/`StreamMethod`/`BusinessRpcResult`
  vocabulary, and `CredentialSet`/`CredentialResolver` test-construction
  pattern exactly as designed — do not edit)
- `crates/loki-mutation-contract/src/{request.rs,method.rs,result.rs}`
  (consume; do not edit)
- `crates/loki-storage/src/agents.rs` (consume `get_agent_connection`/
  `AgentConnectionRow`; do not edit)
- `crates/loki-runtime/tests/{process_behavior.rs,host_lifecycle.rs,
  support/mod.rs}` (existing black-box/lifecycle test conventions to extend)

## 5. Ownership

Writable paths:

- `crates/loki-runtime/src/host/compose.rs` — narrow edit: install the new
  real `BusinessService` via `ServerConfig::new(...).with_business_service(...)`
  inside `open_with_arena`. No other behavior change to this file.
- `crates/loki-runtime/src/host/business_service.rs` (new file) — the real
  `BusinessService` implementation and its `Method`/`StreamMethod` → real
  read/mutation-port mapping. Register it as a module from
  `crates/loki-runtime/src/host.rs` or `host/mod.rs` per existing convention
  (whichever the current module layout uses — read `src/host.rs` first;
  do not restructure the module tree beyond adding this one file).
- `crates/loki-runtime/tests/business_service.rs` (new) — `ComposedHost`-
  level integration tests using `open_with_test_arena` (§7).
- `crates/loki-runtime/tests/process_behavior.rs` — additive only: new
  `#[ignore]`d black-box tests citing `DAT-002-MEM-HOST-VERIFY-001`,
  matching the two existing ones' style exactly. No edit to existing tests.
- `Docs/graph/handoffs/P1-RUNTIME-COMP-001-1.md` (own handoff only).

Read-only dependency paths:

- `crates/loki-runtime/src/services/**` (consume `RuntimeReadPort`/
  `MutationSubmissionPort`/`RuntimeServices`; do not edit).
- `crates/loki-ipc/src/**` (consume `BusinessService`, wire vocabulary,
  `CredentialSet`/`CredentialResolver`; do not edit — including `server.rs`,
  `business.rs`, `auth.rs`).
- `crates/loki-mutation-contract/src/**` (consume; do not edit).
- `crates/loki-storage/src/agents.rs` (consume `get_agent_connection`; do
  not edit — the accessor already exists with the right shape).
- `crates/loki-runtime/src/host/{token.rs,discovery.rs,paths.rs,
  supervisor.rs,clock.rs}` (consume existing conventions; do not edit).

Forbidden scope:

- No edit to `ServerConfig`'s credential mechanism or to
  `crates/loki-ipc/src/auth.rs` — production `compose.rs` keeps using
  `ServerConfig::new`'s single legacy shared-secret token exactly as today.
  Swapping to `CredentialSet`/`CredentialResolver`-based multi-credential
  production distribution is `P1-NODE-IPC-CLIENT-001`'s pending decision and
  a separate, later, additive node — do not anticipate or partially build it
  here.
- No wiring of `RestartRequest`, `FlashRequest`, or `BuildRequest` to
  `MutationSubmissionPort` — must remain `RpcError::UnsupportedMethod` in
  this node's `BusinessService` (§1, §9). Do not fabricate a
  `grant_generation`, a `LocalAuthority` session lookup, or any placeholder/
  default value to make these three "work" — that is exactly the invented-
  data failure mode `AGENTS.md` rule 1 and this codebase's own repeated
  "never fabricate a generation/grant" doc-comment ethos forbid.
- No new `LocalCapability`, no edit to
  `crates/loki-local-authority/src/**`, no edit to
  `crates/loki-agents/src/**`, no edit to `crates/loki-storage/src/
  local_authority.rs` — the session/grant-binding gap for desktop principals
  is a design question for a separate follow-up node, not a silent patch
  here.
- No exposure of `RuntimeServices`' private `_local_authority`/`_agents`
  fields through `RuntimeReadPort`/`MutationSubmissionPort` or any new
  public method on `RuntimeServices` — this node's agent-generation read
  goes through `Storage` directly inside `business_service.rs` (which
  already holds an `Arc<Storage>` via `ComposedHost`), not through a new
  `RuntimeServices` port, to avoid widening an already-accepted, deliberately
  bounded internal contract for one narrow node's convenience. If a reviewer
  judges this wrong, that is a contract-widening finding to route back, not
  something to resolve unilaterally mid-implementation.
- No wiring of any of the 10 business streams (`BusinessService::stream`
  stays `RpcError::UnsupportedMethod`, matching `UnsupportedBusinessService`
  today) — `RuntimeServices` has no event/subscription port; inventing one
  is out of scope.
- No wiring of the 13 permanently-unsupported methods (`HealthCheckRequest`,
  `PreflightRequest`, `JobCancel`, `TestOnDeviceRequest`,
  `DeploymentRequest`, `RollbackRequest`, `ApprovalGrant`, `ApprovalDeny`,
  `DeviceReadLogs`, `DeviceQueryTelemetry`, `IncidentsList`, `IncidentGet`,
  `JobGet`) — no real backing type exists anywhere in this workspace.
- No attempt to make `DAT-002-MEM-HOST-VERIFY-001`'s currently-`#[ignore]`d
  black-box tests (or this node's new ones) pass by weakening, bypassing, or
  conditionally disabling `SecureMemoryRuntime::bootstrap()`'s fail-closed
  behavior in `ComposedHost::open()`. `open_with_test_arena` is the only
  sanctioned bypass, and only for tests.
- No IPC/MCP/Electron caller wiring (later nodes: `P1-NODE-IPC-CLIENT-001`,
  `P1-ELECTRON-RUNTIME-001`, `P1-MCP-RUNTIME-001`).

## 6. Deliverables

1. **`crates/loki-runtime/src/host/business_service.rs`**: a real
   `BusinessService` implementation (e.g. `RuntimeBusinessService`) wrapping
   a shared handle to `RuntimeServices` (and, only for the agent-generation
   read, a shared `Arc<Storage>` — same handle `ComposedHost` already holds,
   passed in at construction, never opened a second time).
2. **Read-method mapping**: `RuntimeGetStatus` → `RuntimeReadPort::runtime_status`,
   `DevicesList` → `list_devices` (validating `DeviceListParams.limit`/
   `after_device_id` into `DevicePageRequest` — an invalid limit is a real
   `RpcError::InvalidRequest`, not a panic or a silently-clamped value),
   `DeviceGet`/`DeviceGetHealth` → `device_health`/lookup, each mapped to
   `BusinessRpcResult`'s corresponding variant field-for-field, matching
   `P1-RPC-CONTRACT-001-R1`'s existing field-for-field mirroring (no
   fabricated field). `DeviceGet`'s not-found case returns a real `None`
   device (an honest empty result), never a synthesized record.
3. **Agent-mutation mapping, with the explicit `principal_id == connection_id`
   convention**: for `AgentRunRegister`, `AgentRunUpdate`, and
   `WorkIntentAcknowledge`, the implementation must, before constructing any
   `MutationAcceptanceRequest`:
   - **Assert `context.principal.kind == PrincipalKind::AgentConnection`.**
     If the authenticated principal is `DesktopInstallation` or
     `RuntimeService`, return `RpcError::PermissionDenied` immediately —
     **never** treat a non-agent principal's `principal_id` as an agent
     `connection_id`. This is the security property this packet fixes
     explicitly: without this check, a `DesktopInstallation` principal
     whose `principal_id` happened to collide with (or be crafted to equal)
     a real `connection_id` string could otherwise submit an agent-run
     mutation it was never granted, or trigger a lookup against another
     principal's connection row. The check must be a hard, early gate in
     `business_service.rs`, not an incidental side effect of the
     `Storage::get_agent_connection` lookup succeeding or failing.
   - Only once that kind check passes, treat `context.principal.principal_id`
     as the real `connection_id` and call
     `Storage::get_agent_connection(connection_id)` to read the current,
     real, persisted `generation`. A `None` result (unknown/never-registered
     connection) is a real, honest `RpcError::PermissionDenied` (or an
     equivalent explicit rejection) — never a default generation.
   - Build `MutationAcceptanceRequest::new(connection_id, actor_id, generation,
     MutationMethod::{AgentRunRegister,AgentRunUpdate,WorkIntentAcknowledge},
     target_id, None, parameters_digest, idempotency_key, None)` from the
     already-validated wire params (`crate::server::validate_method` has
     already run before this code is reached — do not re-derive or
     second-guess its validation, only consume the validated params) and
     submit via `MutationSubmissionPort::submit_mutation`, forwarding the
     real `AcceptanceResult` to `BusinessRpcResult::Mutation` unmodified —
     no independent construction of `Accepted`/`Rejected`, matching
     `P1-RPC-CONTRACT-001-R1`'s own "never a wire-side reconstruction" rule.
   - Document this convention with a doc comment on the new module citing
     this packet and stating plainly that it is a narrow, additive
     assumption pending the real production credential-identity design
     (§9), not itself that design.
4. **Explicit `UnsupportedMethod` for the 3 desktop-mutation methods**: a
   dedicated match arm (not a catch-all `_ =>`) for `RestartRequest`,
   `FlashRequest`, `BuildRequest`, each returning
   `RpcError::UnsupportedMethod` with a doc comment citing this packet and
   naming the follow-up node (§1, §9) — so a future reader sees this is a
   deliberate, temporary, tracked gap distinct from the 13 permanently-
   unsupported methods' identical-looking return value.
5. **`Ping`/`EchoStream` and the 13 permanently-unsupported methods and all
   10 streams**: delegate to the exact same `RpcError::UnsupportedMethod`/
   `Ping` behavior `UnsupportedBusinessService` already provides (`Ping`
   itself never reaches `BusinessService::call` at all — it short-circuits
   in `crate::server::execute_method` before dispatch, per existing code —
   so no change is needed there; only the business-method arms and the
   stream method need an explicit, honest `UnsupportedMethod`).
6. **`ComposedHost::open_with_arena` wiring**: replace
   `ServerConfig::new(config.pipe_name.clone(), AuthToken::new(raw_token))`
   with the same call plus
   `.with_business_service(Arc::new(RuntimeBusinessService::new(...)))`,
   passing whatever the new service needs from the already-open `services`/
   `storage` — no second database open, no second `RuntimeServices::compose`
   call.
7. **`ComposedHost`-level lifecycle tests** (`crates/loki-runtime/tests/
   business_service.rs`), using `ComposedHost::open_with_test_arena`:
   - **Startup**: open → real client Hello (test-constructed `CredentialSet`
     granting the exact authority/entity-ids the test needs, per
     `crate::server`'s own established test pattern — not production
     distribution) → `Ping` → each of the 4 real reads → each of the 3
     agent-mutation methods (including one real rejection case, e.g.
     unregistered connection) round-trips over a real named pipe.
   - **Security regression for Deliverable 3's kind check**: a
     `DesktopInstallation`-kind test principal whose `principal_id` is
     deliberately set equal to a real, currently-registered agent
     `connection_id` must still be denied `AgentRunRegister`/
     `AgentRunUpdate`/`WorkIntentAcknowledge` with `PermissionDenied` — not
     silently succeed against the other principal's connection.
   - **Restart/reconcile**: open → run → clean shutdown → reopen
     `ComposedHost::open_with_test_arena` against the exact same on-disk
     `HostConfig`/paths → run again → prove a device/agent-connection/
     mutation-result persisted before the "restart" is still readable
     afterward, and a fresh mutation submitted after the "restart" succeeds
     — proving business-RPC continuity across a simulated restart, the same
     property `tests/host_lifecycle.rs`'s existing
     `simulated_crash_and_restart_preserves_persisted_device_state` already
     proves for storage alone, now proven through the real business-RPC
     path.
   - **Failure**: a call for one of the 3 desktop-mutation methods returns
     `RpcError::UnsupportedMethod` (proving Deliverable 4 is real, not
     merely present in source); a call with an unauthorized principal
     (missing the relevant `Authority`) is denied before reaching the real
     service, matching `crate::server`'s existing authorization tests but
     now exercised against the real `BusinessService`, not the stub.
8. **New black-box tests in `crates/loki-runtime/tests/process_behavior.rs`**,
   marked `#[ignore]` citing `DAT-002-MEM-HOST-VERIFY-001` exactly like the
   two existing tests, proving (once that gate lifts) a spawned real
   `loki-runtime` `Run`-mode process serves a real business RPC call
   end-to-end — written now so they are ready to enable the moment that gate
   lands, not deferred to a later node to rediscover.
9. Doc comments on the new module citing `P1-RUNTIME-COMP-001` and this
   packet, matching existing citation convention.
10. Handoff recording exactly which of the 8 in-scope methods were wired,
    the security check in Deliverable 3 with its test evidence, the 3
    deferred methods named explicitly with the follow-up node's working
    title, and the `DAT-002-MEM-HOST-VERIFY-001`-gated tests left `#[ignore]`d
    — none of this silently absorbed as "done."

## 7. Acceptance criteria

- [ ] `cargo build --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` succeed with zero warnings.
- [ ] `cargo test -p loki-runtime -- --test-threads=1` passes, including the new `tests/business_service.rs` suite (§6 Deliverable 7).
- [ ] `cargo test -p loki-ipc -p loki-storage -p loki-mutation-contract -- --test-threads=1` unchanged, still green (sanity that this node did not touch their behavior).
- [ ] A real client (test-constructed `CredentialSet`) receives real, correct results for `Ping`, `RuntimeGetStatus`, `DevicesList`, `DeviceGet`, `DeviceGetHealth`, `AgentRunRegister`, `AgentRunUpdate`, `WorkIntentAcknowledge` over a real named pipe against `ComposedHost::open_with_test_arena` — reproduced by real tests, not narrative.
- [ ] `RestartRequest`, `FlashRequest`, `BuildRequest` each return `RpcError::UnsupportedMethod` from the real `BusinessService` — reproduced by a real test per method, not inferred from source reading.
- [ ] The Deliverable-3 security check is proven by a real adversarial test: a `DesktopInstallation` principal whose `principal_id` collides with a real `connection_id` is denied, not served.
- [ ] The restart/reconcile scenario (§6 Deliverable 7) passes: state persisted before a simulated restart is present after, and a fresh mutation after the restart succeeds.
- [ ] No `MutationAcceptanceRequest` is ever constructed with a fabricated/default/placeholder `grant_generation` anywhere in `business_service.rs` (grep/review-verifiable: every construction site's `grant_generation` argument traces to a `Storage::get_agent_connection(...)`.generation` read, never a literal or a default).
- [ ] `git diff --stat` shows changes only under the writable paths named in §5.
- [ ] `ComposedHost::open_with_arena`'s only behavioral change is the `with_business_service(...)` call — no change to migration, discovery-file, token-loading, or pipe-bind order/behavior.
- [ ] The two pre-existing `#[ignore]`d tests in `tests/process_behavior.rs` remain `#[ignore]`d and unmodified (this node does not touch `DAT-002-MEM-HOST-VERIFY-001`'s gate); this node's own new black-box tests (§6 Deliverable 8) are also `#[ignore]`d with the same citation.
- [ ] Independent test-reviewer and security-reviewer sign-off with no Critical/High finding.
- [ ] Handoff explicitly names the 3 deferred methods and the follow-up node's working title — not silently omitted.

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `cargo fmt --all -- --check` | exit 0 | command output |
| Static | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | command output |
| Unit/Integration | `cargo test -p loki-runtime -- --test-threads=1` | new `business_service.rs` suite + all existing suites pass | run summary + test names |
| Regression | `cargo test -p loki-ipc -p loki-storage -p loki-mutation-contract -- --test-threads=1` | unchanged, still green | run summary |
| Functional | real-pipe Ping + 4 reads + 3 agent-mutations against `open_with_test_arena` | all succeed with correct, non-fabricated values | test names + assertions |
| Adversarial | `DesktopInstallation` principal with a colliding `principal_id` attempts an agent-mutation method | `PermissionDenied`, not served | test name + assertion |
| Adversarial | unregistered/unknown `connection_id` attempts an agent-mutation method | explicit rejection, not a default-generation submission | test name + assertion |
| Negative | `RestartRequest`/`FlashRequest`/`BuildRequest` called with an otherwise-fully-authorized principal | `RpcError::UnsupportedMethod`, one test per method | test names |
| Lifecycle | open → run → clean shutdown → reopen same paths → run → mutation submitted post-reopen | state persisted across the simulated restart; new mutation succeeds | test name + assertions |
| Black-box (deferred evidence) | spawned real `loki-runtime` binary serving a real business call | `#[ignore]`d, citing `DAT-002-MEM-HOST-VERIFY-001`, ready to enable once that gate lands | test present, marked ignore, citation checked in review |
| Security | independent review: confirm the kind-check gate in Deliverable 3 is unconditional and precedes the storage lookup; confirm no `grant_generation` literal/default anywhere; confirm the 3 deferred methods are an explicit match arm, not a silent catch-all | no finding | reviewer note in handoff |
| Desktop/Cloud/Hardware | n/a — no Electron/MCP/device wiring in this node | n/a | n/a |

## 9. Risks and safety

- Risk level: `high` — this is the first node at which the runtime process
  serves real, storage-mutating business RPC; an error here (fabricated
  generation, missing kind check, wire-side reconstruction of an
  `AcceptanceResult`) is a real security/data-integrity defect, not a
  cosmetic one.
- Failure modes: (a) the `principal_id == connection_id` convention used
  without the mandatory `PrincipalKind::AgentConnection` gate, letting a
  desktop principal impersonate/target an agent connection (§6 Deliverable
  3 — this is the specific finding this packet fixes by construction, not
  merely by convention); (b) fabricating a `grant_generation` to make the
  3 desktop-mutation methods "work," silently reintroducing exactly the
  invented-authority failure mode `AGENTS.md` rule 1 forbids; (c) a
  wire-side reconstruction of `AcceptanceResult` that diverges from
  `loki-acceptance-store`'s real transaction outcome (forbidden identically
  to `P1-RPC-CONTRACT-001-R1`'s own rule); (d) silently reporting the 3
  deferred methods as in-scope-and-complete rather than an explicit, named,
  tracked gap.
- Rollback/recovery: `compose.rs`'s change is a single call-site swap
  (`.with_business_service(...)`); reverting is mechanical. No schema
  migration, no persisted-format change.
- Data/secret impact: none beyond what `RUN-002`'s existing token/nonce auth
  and the already-accepted mutation-acceptance transaction already handle;
  no new secret class, no new persisted field.
- Device impact: none — no connector/device operation is reachable through
  the 8 in-scope methods (`RestartRequest`/`FlashRequest`, the only
  device-operation-shaped methods among the 23, are explicitly excluded).
- Known, explicitly out-of-this-node's-control blocker: `DAT-002-MEM-HOST-VERIFY-001`
  not yet accepted means this node's own black-box process evidence (§6
  Deliverable 8) cannot run to completion yet — recorded as `#[ignore]`d,
  not worked around.

## 10. Human gate

- Gate: none required to *start* this node.
- Gate required before this node's acceptance is treated as final:
  independent test-reviewer and security-reviewer sign-off with no
  critical/high finding, with the security reviewer specifically confirming
  the Deliverable-3 kind-check gate and the absence of any fabricated
  `grant_generation` (§7, §8).
- Approver role: human product/security owner, on the independent review's
  findings.
- Approval evidence: independent review record under
  `Docs/graph/evidence/P1/P1-RUNTIME-COMP-001/`.
- No separate human gate is required for this node's narrowed scope
  (composing already-accepted contracts, no new authentication/RBAC/signing
  policy change) — the desktop-mutation follow-up node will need one once
  its session/grant-binding design is proposed, per `AGENTS.md`'s
  "changing authentication... RBAC" approval boundary; that gate belongs to
  that later node, not this one.

## 11. Stop conditions

Return `BLOCKED_DEPENDENCY` if, at start, either `P1-SERVICE-READINESS-001-R1`
or `P1-RPC-CONTRACT-001-R1` is found to have an outstanding independent-review
finding that changes `RuntimeReadPort`/`MutationSubmissionPort`/
`BusinessService`'s public shape — confirm current graph status before
starting, do not assume this packet's "accepted" note is still current.

Return `NEEDS_REPLAN` if `Storage::get_agent_connection`'s real shape (row
fields, especially `generation`) has changed incompatibly since this
packet was written, or if achieving the Deliverable-3 kind-check property
turns out to require a change to `crate::business::AuthenticatedPrincipal`'s
public shape (an already-accepted `loki-ipc` contract this node must not
edit) — that would mean the contract itself needs to carry more identity
information than `principal_id`/`kind` alone, a finding to route back rather
than patch around here.

Do not attempt to wire `RestartRequest`/`FlashRequest`/`BuildRequest` even
if doing so looks locally convenient (e.g. "just reuse the agent-connection
path") — a `DesktopInstallation` principal is never a `connection_id`, and
substituting `LocalAuthority`'s session model without a real per-IPC-session
binding is exactly the fabrication this packet forbids (§1, §6, §9). If a
worker discovers a genuine, real (non-fabricated) way to resolve this within
the current accepted contracts, that is a `NEEDS_REPLAN`-worthy discovery to
report, not something to build silently inside this node's scope.

Do not weaken, bypass, or feature-gate around `SecureMemoryRuntime::bootstrap()`'s
fail-closed behavior to make a black-box process test pass — return the new
tests `#[ignore]`d instead, exactly as directed (§6 Deliverable 8, §9).

## 12. Worker and reviewer

- Accountable worker: `loki-implementer` (one bounded node; writable set
  exactly as named in §5).
- Independent reviewer: `loki-security-reviewer` (Deliverable-3 kind-check
  gate, no-fabricated-generation property, no-wire-side-reconstruction
  property, correct exclusion of the 3 desktop-mutation methods) and
  `loki-test-reviewer` (every acceptance-criteria scenario in §7 actually
  exercised by a real test, not merely named; the restart/reconcile test
  genuinely reopens the same on-disk state, not a fresh one).
- Worktree/branch: `node/P1-RUNTIME-COMP-001`.
- Handoff record: `Docs/graph/handoffs/P1-RUNTIME-COMP-001-1.md`.

## 13. Planning provenance

Shaped by `loki-planner` (read-only), 2026-08-31, against `LokiAI-v1.2`@`80bff05`.
Verified by direct inspection, not narrative alone: read
`crates/loki-runtime/src/host/compose.rs` in full to confirm the exact
`UnsupportedBusinessService`→real-service seam and that `RuntimeServices`
composition is already correct and untouched; read
`crates/loki-ipc/src/server.rs` in full to confirm `BusinessService`'s
signature, `mutation_method_for`'s 6-method claim, and that
`AuthenticatedPrincipal::runtime_service`'s default authority/entity sets
are empty; read `crates/loki-ipc/src/business.rs`'s every `*Params` struct
to confirm none carries `principal_sid`/`grant_generation`; read
`crates/loki-mutation-contract/src/request.rs` to confirm
`MutationAcceptanceRequest::new`'s exact required fields; read
`crates/loki-local-authority/src/store.rs` and
`crates/loki-storage/src/local_authority.rs` to confirm no per-principal
"current generation" read exists and no IPC-session-to-LocalAuthority-session
binding exists anywhere; read `crates/loki-storage/src/agents.rs` to confirm
`get_agent_connection` is a real, plain, ungated read returning a real
`generation` column; read `Docs/graph/decisions/ADR-0007-grant-generation-identity-unification.md`
to confirm the LocalSession-vs-AgentCredential model split this finding
rests on; read `crates/loki-runtime/tests/process_behavior.rs` in full to
confirm the two existing `#[ignore]`d tests and their
`DAT-002-MEM-HOST-VERIFY-001` citation; confirmed
`DAT-002-MEM-HOST-VERIFY-001` is still not accepted in `Docs/graph/TASK_GRAPH.md`.
Scope split (8 in-scope / 3 deferred methods) is an orchestrator scheduling
decision recorded 2026-08-31 (chat), adopting this planner's recommendation
verbatim — not a human product/security decision, per the orchestrator's own
framing of that message.
