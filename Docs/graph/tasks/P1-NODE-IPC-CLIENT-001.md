# Task Packet — `P1-NODE-IPC-CLIENT-001` Reusable Production Node Named-Pipe Client With OS-Protected, Distinct Installation/Agent Credentials

Packet status: `proposed` (ready for orchestrator review/dispatch)
Graph status: `proposed` -> recommend `ready` on save (`depends_on` edge `P1-RPC-CONTRACT-001-R1` accepted; `P1-RUNTIME-COMP-001` accepted; ADR-0009 accepted, settles the credential-distribution mechanism this node was blocked on)
Created by: `loki-planner` (read-only)
Created at: `2026-09-01T00:00:00Z`
Starting revision: current tip of branch `LokiAI-v1.2` (post `41178b7`, post `ADR-0009` acceptance).

## 1. Objective

Build the first real, reusable Node.js named-pipe client for `loki-ipc`'s
business RPC protocol (`crates/loki-ipc/src/{client.rs,frame.rs,protocol.rs,
business.rs}`), wire-compatible byte-for-byte with the just-integrated Rust
client and the real, running `RuntimeBusinessService`
(`P1-RUNTIME-COMP-001`) — the first node at which any Node.js process
(Electron main or the standalone MCP server) can genuinely reach the
`loki-runtime` process at all, closing `ADR-0009`'s "neither can currently
obtain, decrypt, or use a credential" gap.

Per `ADR-0009` (accepted, Option A, 2026-09-01), this node also builds the
one small shared Windows-native DPAPI credential component both future
consumers (Electron main, MCP server) will use identically, and the narrow
Rust-side minting extension that writes both `DesktopInstallation`/
`AgentConnection` credentials in the exact DPAPI entropy/blob convention that
component reads — proven end-to-end (Rust writes, Node reads,
byte-for-byte), not assumed compatible by construction.

**This node does not swap `ComposedHost::open()`'s production
`ServerConfig`/credential resolver off the single legacy shared-secret
token.** It mints and persists both real credentials to disk and proves the
Node side can genuinely decrypt them, and it proves the Node client's own
handshake/call/reconnect/deadline/cancellation/revocation-handling behavior
against the real `RuntimeBusinessService` using
`ComposedHost::open_with_test_arena_and_credential_resolver` (already exists,
`P1-RUNTIME-COMP-001`) with a test-constructed `CredentialSet` seeded from
the real DPAPI-decrypted plaintext — genuinely proving reachability without
requiring the still-open Authority-provisioning-policy question (§9 Decision
Brief) to be answered first. Wiring `ComposedHost::open()` itself to
`CredentialSet`/`CredentialResolver` in production is explicitly named by
`ADR-0009` as authorized for "this node and/or its natural follow-ups" — this
packet defers it to the Electron-wiring (`P1-ELECTRON-RUNTIME-001`) and
MCP-wiring (`P1-MCP-RUNTIME-001`) nodes, each of which actually needs a real
per-installation/per-agent-connection principal record (not yet designed —
see §9).

## 2. Traceability

- Requirements/architecture: `CTR-001`, `RUN-002`, `P1-RPC-CONTRACT-001-R1`,
  `Docs/ARCHITECTURE.md` locked boundary — "Desktop-to-runtime communication
  uses an authenticated, versioned local IPC contract"; `Docs/SECURITY.md`
  "Local database theft"/DPAPI-key-lifecycle row and "IPC confused deputy"
  row; `AGENTS.md` rule 4 (validate at every trust boundary), rule 9 (never
  commit/leak secrets, redact from logs), rule 11 (dependency justification).
- Decision: `Docs/graph/decisions/ADR-0009-node-ipc-credential-distribution.md`
  (accepted) — this node implements Option A against its exact constraints;
  does not reopen the distribution-mechanism question.
- Parent/consumed contracts: `P1-RPC-CONTRACT-001-R1` (accepted,
  `crates/loki-ipc/src/{protocol.rs,frame.rs,business.rs,auth.rs,client.rs}`
  — read-only, consume exactly), `packages/contracts/src/businessRpc.ts`
  (accepted, business method/result payload shapes — read-only, consume
  exactly, do not reinvent), `P1-RUNTIME-COMP-001` (accepted,
  `crates/loki-runtime/src/host/{compose.rs,business_service.rs}` — the real
  server this client's tests round-trip against).
- Sibling/follow-up nodes this packet's handoff must name explicitly:
  `P1-ELECTRON-RUNTIME-001`, `P1-MCP-RUNTIME-001` (both `depends_on` this
  node; both blocked additionally on the Authority-provisioning-policy
  decision, §9), `P1-RUNTIME-E2E-001` (depends on both of those).

## 3. Dependencies and inputs

- `depends_on`: `P1-RPC-CONTRACT-001-R1` (accepted). `P1-RUNTIME-COMP-001`
  (accepted) is also a hard practical dependency for this node's own
  integration tests (needs `ComposedHost::open_with_test_arena_and_
  credential_resolver`, `RuntimeBusinessService`) even though it is not this
  node's `depends_on` edge in `TASK_GRAPH.md` today — confirm both are still
  `accepted` at dispatch time, not assumed from this packet's text.
- Consumed contracts/exact revisions (read-only, do not edit unless named in
  §5 as this node's own ownership):
  - `loki_ipc::protocol::{WireMessage, Method, StreamMethod, MethodResult,
    CallOutcome, StreamOutcome, RpcError, HandshakeRejectReason,
    ProtocolVersion, PROTOCOL_VERSION, CORE_CAPABILITY}` — the wire envelope
    this node's Node client must speak exactly; note **no TypeScript
    envelope-level contract exists yet** (`packages/contracts/src/
    businessRpc.ts` covers only business method/result *payloads*, confirmed
    by direct inspection — it imports nothing named `WireMessage`/
    `CallOutcome`/`HandshakeRejectReason` and defines none). This node must
    create that envelope-level TypeScript contract (§6 Deliverable 3) as an
    **additive-only** new file inside the already-accepted `packages/
    contracts` package, mirroring `protocol.rs` field-for-field/tag-for-tag
    (`#[serde(tag = "type")]` on `WireMessage`, `tag = "kind"` on `RpcError`,
    `tag = "outcome"` on `CallOutcome`/`StreamOutcome`, `tag = "reason"` on
    `HandshakeRejectReason`, all `rename_all = "snake_case"` where declared)
    — never a shape invented independently of the Rust source.
  - `loki_ipc::frame::{DEFAULT_MAX_FRAME_BYTES, read_frame, write_frame}` —
    the exact `[u32 LE length][JSON payload]` codec (`crates/loki-ipc/src/
    frame.rs:1`) this node's Node client must reimplement byte-compatibly
    (Node has no shared-crate seam here; this is genuinely a second
    implementation of the same wire codec, so §7 requires a real
    interop-proving test, not code-reading confidence alone).
  - `loki_ipc::client::{ClientConfig, CLIENT_DEADLINE_GRACE, ERROR_PIPE_BUSY}`
    (`crates/loki-ipc/src/client.rs`) — the exact reconnect-retry-on-busy,
    deadline-plus-grace, and no-auto-reconnect conventions this node's Node
    client must match (§6 Deliverable 4). **`Client` in Rust never
    reconnects on its own after a connection is lost** — a caller must call
    `connect` again; `ERROR_PIPE_BUSY` retry applies only during the initial
    `connect`. The Node client must not invent a silent background
    auto-reconnect loop not present in the reference implementation.
  - `loki_ipc::auth::{AuthToken, CredentialSet, CredentialRecord,
    CredentialResolver, PrincipalKind, Authority, AuthenticatedPrincipal}`
    — read-only test-construction pattern this node's own integration tests
    use (matching `P1-RUNTIME-COMP-001`'s own test convention), not a
    contract this node changes.
  - `loki_storage::secret::{SecretError, load_secret, store_new_secret,
    ENTROPY}` (`crates/loki-storage/src/secret.rs`, read in full) — the
    existing DPAPI convention (`CryptProtectData`/`CryptUnprotectData`,
    `CRYPTPROTECT_UI_FORBIDDEN`, a fixed `entropy` blob, atomic
    write-temp-then-rename persistence, `wipe()` of plaintext buffers before
    drop) this node's new credential module must follow **without editing**
    `secret.rs`'s existing DB-key/legacy-IPC-token functions or its single
    `ENTROPY` constant — zero regression risk to already-accepted secret
    persistence (DB key sidecar, legacy IPC token).
  - `packages/contracts/src/businessRpc.ts` — business method/result payload
    zod schemas this node's Node client validates every inbound business
    frame against before returning it to a caller (mirrors
    `BusinessRpcResult::validate_contract()`/`deny_unknown_fields` strictness
    on the Rust side) — consume exactly, no duplicate/parallel TS type
    definitions for the same payloads.
- Required environment/credential/device: a real Windows machine (DPAPI is
  per-user and Windows-only — same constraint every existing `loki-storage`
  secret test already has). No device/hardware. No production credential
  access, no elevation, no VM.
- Known starting conditions (confirmed by direct inspection, 2026-09-01):
  - `crates/loki-runtime/src/host/token.rs` mints exactly **one** credential
    (the legacy shared-secret token), DPAPI-protected via `loki_storage::
    secret::{load_secret,store_new_secret}` — i.e. via `secret.rs`'s single
    shared `ENTROPY` constant, with no per-kind distinction. Nothing in this
    workspace mints a `DesktopInstallation` or `AgentConnection` credential
    today.
  - `crates/loki-ipc/src/auth.rs`'s `CredentialSet`/`CredentialResolver` are
    real and already exercised end-to-end by `P1-RUNTIME-COMP-001`'s own
    tests (`ComposedHost::open_with_test_arena_and_credential_resolver`), but
    are **in-memory-only fixtures** — there is no persistent, durable,
    process-restart-surviving credential/principal store anywhere in this
    workspace (`CredentialSet::revoke` is explicitly documented in `auth.rs`
    as "Test/local-composition convenience — production credential mutation
    is a future node's job"). This means genuine server-side "revoke this
    installation's credential and have it take effect without restarting the
    whole `loki-runtime` process" is **not yet a real production capability**
    anywhere in this workspace, regardless of what this node builds — see §9
    for why this bounds this node's own revocation acceptance criteria to
    what is actually real (file-level independent rotation/deletion plus
    correct client-side handling of a mid-connection
    `CredentialRevalidationFailed`/`HelloReject{AuthenticationFailed}`), not
    a claim of full production hot-revocation.
  - `AuthenticatedPrincipal` has exactly one named constructor,
    `runtime_service(principal_id)` (`crates/loki-ipc/src/business.rs:132`).
    No `desktop_installation(...)`/`agent_connection(...)` constructor
    exists. This node does not add one to `loki-ipc` (that crate's public
    shape is `P1-RPC-CONTRACT-001-R1`'s already-accepted contract, out of
    this node's ownership per §5) — this node's own tests construct
    `AuthenticatedPrincipal` via struct literal, matching `auth.rs`'s own
    existing test convention (see `auth.rs` line ~633).
  - No `napi`/`neon`/any native-Node-addon build tooling exists anywhere in
    `package.json`, `pnpm-workspace.yaml`, or `Cargo.lock` today — this is a
    genuinely new dependency class this node introduces (§6 Deliverable 1,
    §9).
  - `pnpm-workspace.yaml`'s own `DSK-001` override comment already
    anticipated native-Node-addon builds as a considered pattern in this
    repo (currently inert, no addon exists yet) — this node is the first to
    actually add one.
  - `crates/loki-storage/tests/dat002_security.rs` already has a real,
    on-disk DPAPI round-trip test (`loki_storage::secret::store_new_secret`/
    `load_secret` against a real protected file) — this node's own Rust <->
    Node round-trip test follows that same "real DPAPI call, no mock" style,
    extended across the process/language boundary.

## 4. Required reading

- `AGENTS.md`, `Docs/ARCHITECTURE.md` (IPC/process-boundary sections),
  `Docs/SECURITY.md` (IPC confused-deputy row, local-secret/DPAPI rows),
  `Docs/TECH_STACK.md` (dependency-addition rule), `Docs/TESTING.md`
- `Docs/graph/decisions/ADR-0009-node-ipc-credential-distribution.md` (in full)
- `crates/loki-ipc/src/client.rs` (in full — the Rust reference
  implementation this node's Node client must be wire- and
  convention-compatible with)
- `crates/loki-ipc/src/{frame.rs,protocol.rs,auth.rs,business.rs}` (in full
  — wire codec, envelope types, credential/principal machinery, business
  vocabulary; consume, do not edit)
- `crates/loki-storage/src/secret.rs` (in full — the existing DPAPI
  convention this node's new credential module must follow without touching)
- `crates/loki-runtime/src/host/{token.rs,paths.rs,compose.rs}` (existing
  minting/path/composition conventions to extend)
- `crates/loki-runtime/tests/business_service.rs` (existing real-pipe,
  test-arena, `CredentialSet` test convention to reuse for this node's own
  integration tests)
- `packages/contracts/src/businessRpc.ts` (in full — the payload contract to
  consume exactly) and `packages/contracts/src/index.ts` (export surface
  convention)
- `services/mcp-server/package.json`, `electron/main/index.cts` (read only,
  to confirm neither currently has any IPC-client scaffolding to reuse —
  confirmed by direct inspection: `packages/` today has only `contracts/`,
  `electron/main` has no runtime-client code, `services/mcp-server/src` has
  no runtime-client code; this node is genuinely new, not an extension of
  existing scaffolding)

## 5. Ownership

Writable paths:

- `crates/loki-storage/src/credential.rs` (new file) — the purpose-bound,
  additive DPAPI credential API (§6 Deliverable 2). Registered as a new
  `pub mod credential;` in `crates/loki-storage/src/lib.rs` (single new line
  in that file's existing module-declaration list — no other edit to
  `lib.rs`). **No edit to `crates/loki-storage/src/secret.rs`.**
- `crates/loki-runtime/src/host/credential.rs` (new file) — mints/loads both
  `DesktopInstallation`/`AgentConnection` credentials via `loki_storage::
  credential`, following `token.rs`'s existing idempotent
  load-or-create-with-legacy-migration shape where applicable (§6
  Deliverable 2). Registered as a new module from `crates/loki-runtime/src/
  host.rs`/`host/mod.rs` per existing convention (read the current module
  layout first; do not restructure beyond adding this one file).
- `crates/loki-runtime/src/host/paths.rs` — narrow additive edit: two new
  path helpers (`desktop_installation_credential_file()`,
  `agent_connection_credential_file()`) alongside the existing
  `protected_token_file()`/`token_file()` helpers. No removal/rename of
  existing helpers.
- `crates/loki-runtime/src/host/compose.rs` — narrow additive edit:
  `open_with_arena` also mints/loads both new credentials (writes their
  protected files) alongside the existing legacy-token load. **Does not**
  change `ServerConfig`'s credential resolver or business-service wiring —
  those two new credentials are persisted to disk only in this node; nothing
  yet reads them back into the live `Server`'s `CredentialResolver` (§1, §9).
- `crates/loki-credential-native/` (new Cargo crate, cdylib, napi-rs-bound) —
  the shared native Node addon (§6 Deliverable 1). Depends on
  `loki-storage` (specifically the new `credential` module) — never
  reimplements the DPAPI call independently.
- `Cargo.toml` (root) — single-line addition: `"crates/loki-credential-native"`
  to `[workspace] members`. No other edit to this file.
- `packages/native-credential-store/` (new npm package) — the thin TS/JS
  wrapper around the compiled native addon (§6 Deliverable 1): `package.json`,
  generated `.d.ts`, a loader (`index.js`/`index.ts`), its own `build`/`test`
  scripts (picked up automatically by the root `pnpm -r --if-present`
  scripts — no root `package.json` edit needed).
- `packages/contracts/src/ipcProtocol.ts` (new file) — the envelope-level
  wire contract (§6 Deliverable 3), exported from `packages/contracts/src/
  index.ts` (one new export line; no edit to any existing export).
- `packages/ipc-client/` (new npm package) — the Node named-pipe client
  itself (§6 Deliverable 4): frame codec, handshake, `call`/`stream`/
  `cancel`, reconnect-on-busy-during-connect only, deadline/grace, and its
  own real integration tests (§7) against a real running `loki-ipc` server.
- `Docs/graph/handoffs/P1-NODE-IPC-CLIENT-001-1.md` (own handoff only).

Read-only dependency paths:

- `crates/loki-ipc/src/**` (consume `WireMessage`, frame codec semantics,
  `ClientConfig`/`CLIENT_DEADLINE_GRACE`/`ERROR_PIPE_BUSY` conventions,
  `CredentialSet`/`CredentialResolver`/`AuthenticatedPrincipal`/
  `PrincipalKind`/`Authority`; do not edit — this crate's public shape is
  `P1-RPC-CONTRACT-001-R1`'s already-accepted contract).
- `crates/loki-runtime/src/{services/**,host/business_service.rs,host/
  discovery.rs,host/supervisor.rs,host/clock.rs}` (consume; do not edit).
- `crates/loki-storage/src/secret.rs` (consume its DPAPI *convention*
  faithfully in the new `credential.rs`; do not edit the file itself).
- `packages/contracts/src/businessRpc.ts` and every other existing file in
  `packages/contracts/src/` (consume; do not edit any existing file — only
  add the one new `ipcProtocol.ts` file plus its one export line, per §5
  writable paths above).

Forbidden scope:

- No edit to `crates/loki-ipc/src/**` of any kind — its handshake, frame
  format, `Method`/`StreamMethod`/`BusinessRpcResult` vocabulary, and
  `CredentialResolver`/`CredentialSet` machinery are an already-accepted
  contract this node only consumes. If this node's implementer discovers the
  Node client genuinely cannot be wire-compatible without a Rust-side change,
  that is a `NEEDS_REPLAN`-worthy finding to route back (§11), never a
  silent edit here.
- No swap of `ComposedHost::open()`'s production `ServerConfig` from the
  single legacy token to `CredentialSet`/`CredentialResolver` (§1, §9) — the
  two new credentials this node mints are persisted to disk only; they are
  not wired into the live production `Server`'s authentication path in this
  node. Doing so requires deciding each credential kind's default
  `Authority`/`authorized_entity_ids` provisioning policy, which is a
  genuinely open, un-decided question (§9 Decision Brief) — not this node's
  call to make unilaterally.
- No invention of a `desktop_installation(...)`/`agent_connection(...)`
  constructor (or any other public-shape change) on `loki_ipc::business::
  AuthenticatedPrincipal` — construct via struct literal in this node's own
  tests, matching `auth.rs`'s existing convention, exactly as `P1-RUNTIME-
  COMP-001`'s own tests already do for other principal kinds.
- No generic encryption/decryption CLI, no helper binary invoked via
  `child_process` with any secret material passed via argv/env — `ADR-0009`
  forbids both explicitly. The native addon (§6 Deliverable 1) is the only
  sanctioned mechanism; secret bytes cross the Node/Rust boundary only as
  in-process N-API `Buffer` values, never through a spawned process's argv,
  env, stdin, or stdout.
- No plaintext credential written to a log line, error message, test
  snapshot file, `console.log`/`tracing` call, environment variable, or
  configuration file anywhere in this node's diff — grep/review-verifiable.
- No auto-reconnect loop inside the Node client that the Rust reference
  `Client` does not itself have (§3) — do not invent new resiliency
  behavior beyond what `crates/loki-ipc/src/client.rs` actually does; a
  caller-level reconnect wrapper, if ever needed, is a separate, later,
  explicitly-scoped node.
- No Electron main, MCP server, or renderer wiring of any kind
  (`P1-ELECTRON-RUNTIME-001`, `P1-MCP-RUNTIME-001` are later, separate
  nodes) — this node ships a reusable library only, proven against a real
  test-arena server, not against a real desktop/MCP process.
- No attempt to fabricate or default a production `Authority`/
  `authorized_entity_ids` set for either credential kind anywhere in
  `crates/loki-runtime/src/host/credential.rs` or `compose.rs` — the two
  new credentials this node mints carry **no** authority/entity binding
  decision in this node (they are not fed into any live `CredentialResolver`
  at all, per the forbidden-scope item above); inventing one now to make a
  future node's job easier would be exactly the fabricated-authority failure
  mode `AGENTS.md` rule 1 and `P1-RUNTIME-COMP-001`'s own precedent forbid.

## 6. Deliverables

1. **Shared native Windows-DPAPI credential component** —
   `crates/loki-credential-native` (Rust cdylib, napi-rs-bound,
   ABI-stable N-API so one compiled artifact loads unmodified into both
   plain Node.js (MCP server) and Electron's embedded Node (Electron main),
   without a separate `electron-rebuild` step) plus `packages/native-
   credential-store` (the npm-consumable TS wrapper). Exposes exactly:
   - `loadDesktopInstallationCredential(protectedPath: string): Buffer`
   - `loadAgentConnectionCredential(protectedPath: string): Buffer`

   Both are **read-only from Node's perspective — Node never mints a
   credential itself**; minting stays a Rust-runtime-owned operation (§6
   Deliverable 2), a deliberate least-privilege boundary: a compromised or
   buggy Node-side consumer can at most read an existing credential it is
   pointed at, never forge/mint a new one the runtime would accept. Each
   function returns the raw decrypted secret bytes directly to the caller's
   own `Buffer` in-process (never via a temp file, stdout, or env var) and
   throws a specific, typed, distinguishable error
   (`CredentialMissingError`/`CredentialCorruptError`/
   `CredentialKindMismatchError`/`CredentialWindowsError`) mapped 1:1 from
   `loki_storage::credential`'s `SecretError`/kind-mismatch variants — never
   a bare generic `Error`, so a Node caller can fail closed on the *specific*
   reason (§6 Deliverable 6, §7).

   **Why a native addon over a helper-binary/CLI or a JS-side DPAPI
   reimplementation** (recorded rationale, not re-litigated per node):
   - A spawned helper binary (even one invoked without argv secrets) adds a
     process boundary per call, its own build/signing/distribution surface,
     and stdin/stdout plumbing risk; `ADR-0009` already forbids a *generic*
     CLI, and a narrow one-purpose helper binary buys nothing a
     directly-linked addon doesn't already give more simply.
   - A JS-side reimplementation of `CryptProtectData`/`CryptUnprotectData`
     calls (e.g. via a generic FFI package) would duplicate the exact
     entropy/blob/kind-tag convention in two languages, reintroducing the
     very "not assumed compatible by construction" risk `ADR-0009`
     specifically calls out — and would add a third-party dynamic-FFI
     dependency requiring its own `AGENTS.md` rule 11 justification for no
     real benefit over reusing the first-party Rust code that already
     exists.
   - A native addon wrapping the **actual** `crates/loki-storage` code
     (not a reimplementation) makes "what Rust writes, Node reads" true by
     construction of a shared code path, not by two independently-maintained
     implementations happening to agree — still independently verified end-
     to-end (§6 Deliverable 6), never merely assumed.
2. **Purpose-bound, additive credential minting/loading, split across two
   layers**:
   - `crates/loki-storage/src/credential.rs` (new): a `CredentialKind`
     enum (`DesktopInstallation`, `AgentConnection`), each bound to its own
     fixed entropy constant distinct from `secret.rs`'s existing `ENTROPY`,
     plus a small inner envelope (a format-version byte and a kind-tag byte
     prepended to the plaintext secret before `CryptProtectData`, verified
     on `unprotect`) so a credential protected for one kind can **never** be
     silently accepted as the other kind even if a caller pointed the wrong
     path at the wrong loader — the literal "purpose-bound formats... not
     interchangeable/confusable at the format level" property `ADR-0009`
     requires, made a structural property of the format, not a convention a
     caller could bypass. Exposes `mint_or_load_credential(kind, path) ->
     Result<Vec<u8>, SecretError>` (idempotent, Rust-side only — this is the
     one function that may create a fresh secret) and
     `load_credential(kind, path) -> Result<Vec<u8>, SecretError>`
     (read-only, fails closed — `MissingProtectedSecret`/
     `InvalidProtectedSecret`/kind-mismatch — never a default). Zero edits
     to `secret.rs` itself.
   - `crates/loki-runtime/src/host/credential.rs` (new): calls
     `mint_or_load_credential` twice (once per kind) against the two new
     `HostPaths` helpers (§5), returning both plaintext secrets to
     `compose.rs` for immediate, in-memory-only forwarding to
     `store_new_secret`'s call site — never logged, never held longer than
     needed to write the protected file.
   - `ComposedHost::open_with_arena` calls this at startup, alongside (not
     replacing) the existing legacy-token load. Both new credential files
     now exist on disk after a normal `open()`, independent of whether
     anything yet reads them back into the live `Server`.
3. **Envelope-level TypeScript wire contract** —
   `packages/contracts/src/ipcProtocol.ts` (new): zod schemas for
   `WireMessage` (tagged by `type`), `CallOutcome`/`StreamOutcome` (tagged
   by `outcome`), `RpcError` (tagged by `kind`), `HandshakeRejectReason`
   (tagged by `reason`), `ProtocolVersion`, mirroring `crates/loki-ipc/src/
   protocol.rs` field-for-field and tag-for-tag, `.strict()` on every object
   matching Rust's `deny_unknown_fields` posture where present. Exported from
   `packages/contracts/src/index.ts` alongside the existing `businessRpc.ts`
   exports (one new export line).
4. **The Node IPC client** — `packages/ipc-client` (new package):
   - **Frame codec**: `[u32 LE length][JSON payload]`, matching `frame.rs`
     exactly, including `DEFAULT_MAX_FRAME_BYTES` (4 MiB), and typed,
     distinguishable errors for a too-large declared length (rejected
     without allocating/reading the claimed payload — mirrors `frame.rs`'s
     own "oversized length prefix... without allocating" test), a truncated
     length prefix/payload, and malformed/unrecognized JSON — never a bare
     thrown string.
   - **Handshake**: reads `Challenge`, sends `Hello` (protocol version
     `1.1`, capabilities including `loki-ipc.core.v1` and
     `loki-ipc.business.v1`, `client_type`/`client_version`, the echoed
     nonce, and the plaintext token obtained from `packages/native-
     credential-store` for the caller's chosen credential kind), handles
     `HelloAck`/`HelloReject` exactly as `client.rs::connect` does
     (including rejecting a negotiated-capabilities superset it did not
     itself advertise/support, matching `client.rs`'s own defensive check).
     The plaintext token is held only in a `Buffer`, zeroed
     (`buffer.fill(0)`) immediately after the `Hello` frame is sent, with an
     explicit, non-hidden doc comment noting this is best-effort (V8's own
     internal string/JSON-serialization buffers for that one frame are not
     independently wiped by this code, a known, named limitation — not
     silently claimed as a complete guarantee).
   - **Calls**: `ping(payload, timeoutMs)`, `businessCall(method,
     timeoutMs)` (validates the response against `ipcProtocol.ts` +
     `businessRpc.ts` schemas before resolving, matching `client.rs::
     business_call`'s `validate_contract()`/`business_result_matches_method`
     checks — never hands a caller an unvalidated/mismatched result),
     `businessStream(params, timeoutMs)` returning an async-iterable/event
     handle, `cancelCall(correlationId)` (fire-and-forget, matching
     `client.rs::cancel_call`).
   - **Concurrency**: one pending-calls map keyed by `correlation_id`
     (`crypto.randomUUID()` — Node built-in, no new dependency), one reader
     loop demultiplexing frames by `correlation_id`, matching `client.rs`'s
     architecture (reader/writer separation, though Node's single-threaded
     event loop means this is structural clarity, not a literal task-per-
     direction split).
   - **Deadline/grace**: `deadline_unix_millis = Date.now() + timeoutMs`;
     client-side local timeout at `timeoutMs + CLIENT_DEADLINE_GRACE`
     (2000ms, mirrored exactly from `client.rs`), the server remaining the
     primary deadline authority exactly as in Rust.
   - **Reconnect**: `connect()` retries only a busy-pipe connect error (the
     Windows-named-pipe-busy condition Node's `net`/libuv surfaces — the
     implementer must determine and document the exact Node-observed
     error/code empirically against a real busy pipe, not assume it mirrors
     `ERROR_PIPE_BUSY`'s raw value name) up to a configurable
     `connectTimeoutMs`, matching `open_with_retry`'s 20ms poll interval.
     **No automatic reconnect after an established connection is lost** —
     matching `client.rs`'s own behavior exactly (§3, §5 forbidden scope).
   - **Cancellation propagation on client-side timeout**: matching
     `client.rs::ping`/`business_call`'s own behavior, a locally-timed-out
     call removes itself from the pending map (does not leak) and surfaces
     a distinguishable "client timed out" error, distinct from a real
     server-returned `RpcError::DeadlineExceeded`.
5. **Kind-explicit credential loading in the Node client's own connect
   path**: `NodeIpcClient.connect(config)` takes an explicit
   `credentialKind: 'desktop_installation' | 'agent_connection'` and the
   protected-file path, calls the matching `native-credential-store`
   function, and fails closed with a specific, caller-visible error
   (mapped from Deliverable 1's typed errors) before ever attempting to open
   the pipe if the credential is missing, corrupt, or kind-mismatched — the
   literal "fail-closed behavior on missing, corrupt, mismatched, or
   inaccessible credential state" property `ADR-0009` requires, made
   testable end-to-end (§7), not merely asserted.
6. **Real end-to-end round-trip proof (the crux of Option A)**: a real test
   (Rust test harness driving both sides, or a small cross-language test
   script invoked from `pnpm`/`cargo` verification — implementer's choice,
   but it must actually run both the real Rust minting call and the real
   compiled Node native addon in the same test run, not two separately-
   passing unit tests that never touch the same file) that:
   - mints a real `DesktopInstallation` credential via `crates/loki-storage/
     src/credential.rs::mint_or_load_credential` to a real temp file, then
     loads it back via the compiled `native-credential-store` addon, and
     asserts byte-for-byte equality with the plaintext the Rust side
     produced (never comparing hashes/lengths alone).
   - repeats for `AgentConnection`.
   - **negative case**: mints a `DesktopInstallation` credential, then
     attempts to load that same file via `loadAgentConnectionCredential` —
     asserts a `CredentialKindMismatchError`, not a silent decrypt success
     with wrong-kind bytes.
   - **negative case**: attempts to load a credential file that does not
     exist, and one that exists but is corrupted (truncated/random bytes
     written over a real protected file) — asserts `CredentialMissingError`/
     `CredentialCorruptError` respectively, never a default/empty credential.
7. **Real integration tests against a real running `loki-ipc` server** (not
   a mocked pipe), using `ComposedHost::open_with_test_arena_and_
   credential_resolver` with a `CredentialSet` seeded from the real
   DPAPI-decrypted plaintext from Deliverable 6 (so the credential value
   itself, not just its shape, is proven to flow all the way from a real
   Rust-minted, DPAPI-protected file through the real Node addon into a
   real successful handshake against the real `RuntimeBusinessService`):
   - handshake success, `Ping`, one real business read (`RuntimeGetStatus`),
     one real agent-mutation call (`AgentRunRegister` against a real
     registered `AgentConnection` principal, matching `P1-RUNTIME-COMP-001`'s
     own `principal_id == connection_id` convention).
   - deadline: a call whose server-side handler is deliberately slow (reuse
     or mirror `tests/host_lifecycle.rs`'s `SlowBusinessService`-style
     fixture pattern if reusable read-only, else a minimal equivalent) proves
     the Node client surfaces a real `DeadlineExceeded`/client-timeout
     distinction correctly.
   - cancellation: `cancelCall` on an in-flight slow call is observed
     server-side (or at minimum does not hang/leak client-side) — best-
     effort semantics matching `client.rs`'s own documented best-effort
     contract, not over-claimed as guaranteed-synchronous.
   - reconnect-on-busy: a real second connect attempt against a pipe with an
     already-connected client observes the busy-retry path succeed once the
     first client disconnects, within `connectTimeoutMs`.
   - **revocation/fail-closed**: connect succeeds, then the test's own
     `CredentialSet.revoke(principal_id)` is called server-side mid-
     connection (matching `P1-RUNTIME-COMP-001`'s own adversarial-test
     style); the next call from the already-connected Node client receives
     `RpcError::CredentialRevalidationFailed` and the client surfaces it as
     a distinguishable, terminal authentication failure — **not** retried
     silently and **not** misreported as a generic connection error.
   - **independent rotation**: rotating (deleting + re-minting) only the
     `AgentConnection` credential file, then re-running Deliverable 6's
     round-trip for `DesktopInstallation` against its original, untouched
     file, proves the two credentials' storage/lifecycle are genuinely
     independent — not merely asserted by doc comment.
8. Doc comments citing `P1-NODE-IPC-CLIENT-001` and `ADR-0009` on every new
   module, matching existing citation convention.
9. Handoff recording: the exact native-addon mechanism chosen and why (§6
   Deliverable 1's rationale, reproduced/confirmed against what was actually
   built, not just what was planned); confirmation that `ComposedHost::
   open()`'s production credential resolver was **not** touched; the
   Authority-provisioning-policy gap named explicitly as the reason
   production wiring is deferred (§9); every acceptance-criteria scenario in
   §7 mapped to its actual test name.

## 7. Acceptance criteria

- [ ] `cargo build --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` succeed with zero warnings, including the new `loki-credential-native` crate.
- [ ] `cargo test -p loki-storage -p loki-runtime -- --test-threads=1` passes, including new `credential.rs`-related tests; no existing test in either crate changes behavior.
- [ ] `pnpm --filter @lokiai/contracts run build && pnpm --filter @lokiai/contracts run test` passes with the new `ipcProtocol.ts` schemas included; no existing contracts file's exported shape changes.
- [ ] `pnpm --filter @lokiai/native-credential-store run build && ...run test` passes on a real Windows machine, including Deliverable 6's full round-trip and all four negative cases.
- [ ] `pnpm --filter @lokiai/ipc-client run build && ...run test` passes, including every Deliverable 7 scenario, against a real running test-arena `loki-ipc` server — reproduced by real tests, not narrative.
- [ ] A real DPAPI-protected `DesktopInstallation` credential minted by the Rust side is loaded byte-for-byte identically by the compiled Node native addon, and vice-versa is impossible to confuse with an `AgentConnection` credential (kind-mismatch fails closed) — reproduced by a real cross-language test run, not two independently-passing unit tests.
- [ ] The Node client never auto-reconnects after an established connection is lost (grep/review-verifiable: no retry/backoff loop wraps the post-handshake read/write path) — matching `client.rs`'s own behavior exactly.
- [ ] No plaintext credential appears in any log line, thrown-error message, test snapshot, or committed file anywhere in this node's diff (grep/review-verifiable).
- [ ] `ComposedHost::open()`'s production `ServerConfig`/credential-resolver wiring is unchanged (`git diff` on `compose.rs` shows only the additive credential-minting calls named in §6 Deliverable 2, nothing touching `ServerConfig`/`with_credential_resolver`/`with_business_service`).
- [ ] `git diff --stat` shows changes only under the writable paths named in §5.
- [ ] Independent test-reviewer and security-reviewer sign-off with no Critical/High finding, with the security reviewer specifically confirming: no plaintext-to-argv/env/log path exists anywhere in the native-addon boundary; the kind-tag mismatch check is unconditional and precedes any caller-visible success; revocation mid-connection is surfaced as a distinguishable terminal failure, never silently retried.
- [ ] Handoff explicitly names the deferred production-wiring gap and the Authority-provisioning-policy decision need (§9) — not silently omitted.

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static (Rust) | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | command output |
| Static (TS) | `pnpm run lint`, `pnpm run typecheck`, `pnpm run format:check` | exit 0 across all new/changed packages | command output |
| Unit/Integration (Rust) | `cargo test -p loki-storage -p loki-runtime -- --test-threads=1` | new `credential.rs` tests + all existing suites pass | run summary + test names |
| Unit/Integration (native addon) | `pnpm --filter @lokiai/native-credential-store run test` | Deliverable 6 round-trip + 3 negative cases pass on real Windows DPAPI | test names + assertions |
| Cross-language | dedicated round-trip test invoking both the real Rust mint call and the real compiled addon in one run | byte-for-byte plaintext equality; kind-mismatch/missing/corrupt all fail closed with specific typed errors | test name + assertions, run log |
| Integration (Node client) | `pnpm --filter @lokiai/ipc-client run test` against `ComposedHost::open_with_test_arena_and_credential_resolver` | handshake, ping, real read, real agent-mutation, deadline, cancellation, reconnect-on-busy, revocation-mid-connection, independent-rotation all pass over a real named pipe | test names + assertions |
| Adversarial | revoke the connected client's credential server-side mid-connection, then issue a call | `CredentialRevalidationFailed`, surfaced as a distinguishable terminal auth failure, not retried | test name + assertion |
| Adversarial | corrupt one credential's protected file, leave the other untouched | only the corrupted kind fails to load; the untouched kind still loads correctly | test name + assertion |
| Negative | request `AgentConnection` load against a `DesktopInstallation`-protected file | `CredentialKindMismatchError`, never a silent wrong-kind decrypt | test name + assertion |
| Security | independent review: no argv/env/log/file exposure of plaintext anywhere in the native-addon or client boundary; no fabricated Authority/entity binding anywhere in `credential.rs`/`compose.rs`; production `ComposedHost::open()` wiring genuinely untouched | no finding | reviewer note in handoff |
| Desktop/Cloud/Hardware | n/a — no Electron/MCP wiring in this node | n/a | n/a |

## 9. Risks and safety

- Risk level: `high` — this node introduces the first native Node addon, the
  first cross-language secret-handling boundary, and the first two
  production-shaped credentials in this workspace; a mistake here (plaintext
  leak, weak kind-binding, silent fallback on a missing/corrupt credential)
  is a real security defect, not cosmetic.
- Failure modes: (a) a native-addon build/loader mismatch between plain
  Node.js and Electron's embedded Node despite N-API's ABI-stability
  claim — must be verified against both real target runtimes before this
  node is treated as `COMPLETE` for either consumer's later use, not merely
  assumed from N-API's documentation; (b) a kind-tag check that is present
  in code but reachable-bypassable (e.g. checked only on the happy path,
  not before returning any secret bytes) — the exact class of finding
  `P1-RUNTIME-COMP-001`'s own Deliverable 3 kind-check precedent guards
  against, applied here one layer down; (c) a Node-side JSON/V8 string
  buffer retaining an uncleared copy of a plaintext token — a real,
  partially-irreducible limitation of the runtime this node must document
  honestly (§6 Deliverable 4) rather than silently claim doesn't exist;
  (d) silently wiring `ComposedHost::open()`'s production path to the new
  credentials without a real Authority-provisioning-policy decision, which
  this packet explicitly forbids (§5).
- Rollback/recovery: every writable path is new/additive; `compose.rs`'s
  edit is a small additive block, mechanically revertible without touching
  the existing token/business-service wiring.
- Data/secret impact: two new persisted DPAPI-protected credential files
  per host installation; no existing secret's format/location changes.
- Device impact: none.
- **Decision brief — genuinely new, narrower question, not settled by
  `ADR-0009`**: `ADR-0009` settled *how* a Node process obtains/decrypts a
  credential (Option A, the shared native component). It did not settle,
  and this node's own scoping surfaced as a real, separate open question:
  **what `Authority` set and `authorized_entity_ids` policy should a
  freshly-minted, production `DesktopInstallation`/`AgentConnection`
  credential actually carry once `ComposedHost::open()` is wired to
  `CredentialSet`/`CredentialResolver`** (e.g. does every desktop
  installation get every read `Authority` by default, or is there a
  provisioning/enrollment step; does an `AgentConnection` credential's
  `authorized_entity_ids` get populated from `Storage::get_agent_connection`
  at mint time, and if so how does that interact with an agent connection
  that doesn't exist yet at host-startup time)? This is a real product/
  security policy decision (who gets what authority, by default, on a bare
  install), not an implementation detail, and it directly blocks
  `P1-ELECTRON-RUNTIME-001`/`P1-MCP-RUNTIME-001` from safely wiring
  `ComposedHost::open()` to the real multi-credential resolver. Recorded
  here for the orchestrator/human to route to a new `DECISION` node before
  either of those two nodes is dispatched — not resolved unilaterally by
  this packet.

## 10. Human gate

- Gate: none required to *start* this node (ADR-0009 already accepted the
  distribution mechanism; this node's own scope is additive/library-only,
  no production authentication path change).
- Gate required before this node's acceptance is treated as final:
  independent test-reviewer and security-reviewer sign-off with no
  critical/high finding (§7, §8), with the security reviewer specifically
  confirming the plaintext-handling and kind-binding properties named in §9.
- A **separate** human gate is required — named here, not implied by this
  node's own acceptance — before `P1-ELECTRON-RUNTIME-001`/
  `P1-MCP-RUNTIME-001` may wire `ComposedHost::open()`'s production path to
  the new multi-credential resolver, per `AGENTS.md`'s "changing
  authentication... RBAC... policy" approval boundary and the still-open
  Authority-provisioning-policy decision (§9). That gate belongs to
  whichever node proposes that policy, not this one.
- Approver role: human product/security owner, on the independent review's
  findings and (separately) on the §9 decision brief once a policy is
  proposed.
- Approval evidence: independent review record under
  `Docs/graph/evidence/P1/P1-NODE-IPC-CLIENT-001/`.

## 11. Stop conditions

Return `BLOCKED_DEPENDENCY` if, at start, `P1-RPC-CONTRACT-001-R1` or
`P1-RUNTIME-COMP-001` is found to have an outstanding independent-review
finding changing `loki-ipc`'s public shape or `ComposedHost::
open_with_test_arena_and_credential_resolver`'s signature/behavior — confirm
current graph status before starting, do not assume this packet's "accepted"
note is still current.

Return `NEEDS_REPLAN` if N-API/napi-rs is found, during implementation, to
genuinely not load unmodified into both plain Node.js and Electron's
embedded Node on this workspace's actual pinned versions (`package.json`'s
`engines.node`, `electron@43.4.0`) — that would mean this packet's
Deliverable 1 mechanism choice needs reconsideration, a finding to report
rather than work around with a second, divergent build per consumer.

Return `NEEDS_REPLAN` if achieving the wire codec's byte-for-byte
compatibility with `frame.rs`/`protocol.rs` turns out to require any change
to `crates/loki-ipc`'s public shape (e.g. a serde representation that cannot
be mirrored from outside the crate) — that is a contract defect to route
back, not a reason to reinterpret the wire format loosely on the Node side.

Do not wire `ComposedHost::open()`'s production credential resolver to the
two new credentials even if doing so looks locally convenient for testing —
use `open_with_test_arena_and_credential_resolver` exactly as directed (§5,
§6 Deliverable 7). If a worker discovers a genuine, real (non-fabricated) way
to resolve the Authority-provisioning-policy question within already-
accepted contracts, that is a `NEEDS_REPLAN`-worthy discovery to report, not
something to decide silently inside this node's scope.

Do not add `ffi-napi`/`koffi`/any generic dynamic-FFI package, or any
process-spawning helper binary, as an alternate/fallback credential-access
path "in case the native addon doesn't build" — if the native addon
genuinely cannot be made to work, that is a `NEEDS_REPLAN` to report against
`ADR-0009`'s Option A itself, not a silent substitution of a materially
different, unreviewed mechanism.

## 12. Worker and reviewer

- Accountable worker: `loki-implementer` (one bounded node; writable set
  exactly as named in §5). Given the tight coupling between the native
  addon (Deliverable 1), the Rust minting extension (Deliverable 2), and the
  cross-language round-trip proof (Deliverable 6) — none of which can be
  independently verified without the others existing — this node is
  scoped as **one** worker rather than split into parallel sibling nodes;
  splitting was considered and rejected (§13) because the round-trip proof
  requires both halves to exist together, and an early contract freeze
  between two workers would need the same design decisions this packet
  already fixes (§6 Deliverable 1's exact function signatures/error
  taxonomy), making a two-worker split add coordination cost without
  reducing it.
- Independent reviewer: `loki-security-reviewer` (plaintext-handling
  properties, kind-tag-mismatch fail-closed property, no-argv/env/log
  exposure, production-wiring-untouched property, dependency-addition
  justification for napi-rs) and `loki-test-reviewer` (every §7 acceptance
  scenario actually exercised by a real test against a real running server/
  real DPAPI, not merely named; the cross-language round-trip test
  genuinely invokes both the real Rust mint call and the real compiled
  addon in one run).
- Worktree/branch: `node/P1-NODE-IPC-CLIENT-001`.
- Handoff record: `Docs/graph/handoffs/P1-NODE-IPC-CLIENT-001-1.md`.

## 13. Planning provenance

Shaped by `loki-planner` (read-only), 2026-09-01, against
`LokiAI-v1.2`@`41178b7` (post-`ADR-0009` acceptance). Verified by direct
inspection, not narrative alone: read `crates/loki-storage/src/secret.rs` in
full to confirm the exact DPAPI convention (`CryptProtectData`/
`CryptUnprotectData`, `CRYPTPROTECT_UI_FORBIDDEN`, single shared `ENTROPY`
constant, atomic write-temp-then-rename, `wipe()`) and that it has no
per-credential-kind distinction today; read `crates/loki-ipc/src/client.rs`
in full to confirm the Rust reference client's exact reconnect/deadline/
cancellation conventions (`ERROR_PIPE_BUSY` retry only during initial
connect, no auto-reconnect after an established connection is lost,
`CLIENT_DEADLINE_GRACE` = 2s); read `crates/loki-ipc/src/{frame.rs,
protocol.rs,auth.rs,business.rs}` in full to confirm the exact wire codec,
envelope tagging conventions, `CredentialSet`/`CredentialResolver`'s
in-memory-only nature, and `AuthenticatedPrincipal`'s single existing
constructor; read `crates/loki-runtime/src/host/{token.rs,compose.rs}` in
full to confirm exactly one credential is minted today and that
`ComposedHost::open_with_test_arena_and_credential_resolver` already exists
as the sanctioned test seam (`P1-RUNTIME-COMP-001`); read `packages/
contracts/src/businessRpc.ts` in full and confirmed no envelope-level
(`WireMessage`/`CallOutcome`/`HandshakeRejectReason`) TypeScript contract
exists anywhere in `packages/contracts/src/`; confirmed via `pnpm-
workspace.yaml`/`package.json`/`Cargo.lock` that no native-Node-addon
tooling (`napi`/`neon`) exists anywhere in this workspace yet; confirmed via
directory listing that `packages/` contains only `contracts/` and neither
`electron/main` nor `services/mcp-server/src` has any existing runtime-IPC-
client scaffolding to reuse. `Docs/graph/TASK_GRAPH.md`'s existing node-
register row for `P1-NODE-IPC-CLIENT-001` (`depends_on: P1-RPC-CONTRACT-001-
R1`, status `proposed`) and its downstream edges (`P1-ELECTRON-RUNTIME-001`,
`P1-MCP-RUNTIME-001`, `P1-RUNTIME-E2E-001`) were read and are unchanged by
this packet. The Authority-provisioning-policy gap (§9) is a new finding
from this planning pass, not previously recorded anywhere in `TASK_GRAPH.md`
or `ADR-0009` — reported here per the orchestrator's own instruction to
surface a genuinely different, narrower unresolved question distinct from
the now-settled distribution-mechanism question.
