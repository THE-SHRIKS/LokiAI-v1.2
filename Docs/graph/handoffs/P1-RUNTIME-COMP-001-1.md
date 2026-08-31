# Worker Handoff — `P1-RUNTIME-COMP-001`

Result: `NEEDS_REPLAN`
Worker: `loki-implementer`
Started from: `LokiAI-v1.2`@`84168b5` (task packet's cited starting revision; branch tip at dispatch)
Result revision/patch: worktree `node/P1-RUNTIME-COMP-001` (uncommitted at time of writing this handoff; committed alongside it — see final commit hash in the PR/integration record)
Completed at: `2026-08-31T18:00:00Z`

## 1. Outcome

The real `RuntimeBusinessService` (Deliverables 1–6, 9) is implemented,
compiles clean, and passes `cargo clippy --workspace --all-targets -- -D
warnings` and every unaffected regression suite. However, wiring it into
`ComposedHost::open_with_arena` exposes a **real, reproducible
`STATUS_ACCESS_VIOLATION` crash** in the compiled test binary whenever a real
IPC connection referencing the new storage-backed `BusinessService` is still
alive when `ComposedHost::run()` tears down `SecureMemoryRuntime` — a genuine
use-after-free class defect, not a test-authoring mistake, caused by an
already-accepted `loki-ipc` `Server` design gap (`crates/loki-ipc/src/
server.rs`'s per-connection tasks are spawned detached, with no way for a
caller to know when they have all finished) that this node is the first to
expose, because it is the first node to install a `BusinessService` that
holds any resource tied to the secure-memory arena. Fixing this correctly
requires editing `crates/loki-ipc/src/server.rs` (forbidden scope, an
already-accepted contract this node must not edit) to track and let a caller
drain/await outstanding per-connection tasks before secure-memory teardown.
This is reported as `NEEDS_REPLAN`, with full reproduction evidence below,
rather than worked around.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `cargo build --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` zero warnings | pass | §4 |
| `cargo test -p loki-runtime -- --test-threads=1` passes, including new `tests/business_service.rs` | **fail** | §4 — `tests/business_service.rs` and the pre-existing (unmodified) `tests/host_lifecycle.rs` both crash the test process with `STATUS_ACCESS_VIOLATION` once the real `BusinessService` is installed |
| `cargo test -p loki-ipc -p loki-storage -p loki-mutation-contract -- --test-threads=1` unchanged, still green | pass | §4 |
| Real client receives real results for the 8 in-scope methods over a real pipe against `open_with_test_arena` | **blocked** | The tests exist and are logically correct (see §7), but the suite crashes before all assertions can be proven to hold reliably across every run — see §1 |
| `RestartRequest`/`FlashRequest`/`BuildRequest` return `UnsupportedMethod` | implemented, evidence blocked | Test written (`desktop_mutation_methods_remain_unsupported_even_for_a_fully_authorized_principal`); suite-level crash prevents a clean full-suite pass, though this specific test's logic is sound and unrelated to the crash's storage-touching code path |
| Deliverable-3 security check proven by adversarial test | implemented, evidence blocked | Test written (`desktop_principal_colliding_with_a_real_connection_id_is_denied_not_served`); same suite-crash caveat |
| Restart/reconcile scenario passes | implemented, evidence blocked | Test written (`restart_reconcile_preserves_state_and_serves_a_fresh_mutation_through_the_real_business_rpc_path`), reopens the same on-disk `HostConfig`/paths via `ComposedHost::open_with_test_arena` twice, same suite-crash caveat |
| No `MutationAcceptanceRequest` ever constructed with a fabricated `grant_generation` | pass | Single construction site in `business_service.rs`'s `agent_mutation`; `connection.generation` traces directly to `Storage::get_agent_connection(...).generation` — grep-verifiable, no literal/default |
| `git diff --stat` shows changes only under §5 writable paths | pass | §3 |
| `open_with_arena`'s only behavioral change is `.with_business_service(...)` | **partially exceeded** | Also had to `Arc`-wrap `services`/`storage` handles and add `storage_handle()`/`services_handle()` accessors so `RuntimeBusinessService` and the host itself can share the same composed instances (per Deliverable 1's own "shared handle... never opened a second time" requirement) — recorded as a decision in §7, not silently absorbed |
| Two pre-existing `#[ignore]`d tests in `process_behavior.rs` remain unmodified | pass | §3 |
| Independent reviewer sign-off, no Critical/High finding | not reached | Node is not ready for review under `COMPLETE`; the crash itself is the finding this handoff surfaces first |
| Handoff names the 3 deferred methods and follow-up node | pass | §9 |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-runtime/src/host/business_service.rs` (new) | Real `RuntimeBusinessService` implementing `loki_ipc::server::BusinessService`: 4 real reads via `RuntimeReadPort`, 3 agent-mutations via `MutationSubmissionPort` with the mandatory `PrincipalKind::AgentConnection` kind-check gate, explicit `UnsupportedMethod` arms for `RestartRequest`/`FlashRequest`/`BuildRequest`, and a wildcard `UnsupportedMethod` for the 13 permanently-unsupported methods and all streams | yes — new-file path named in §5 |
| `crates/loki-runtime/src/host.rs` | Added `pub mod business_service;` and a module-map doc-comment entry | yes — per-existing convention this file already documents ("register it as a module from `host.rs`") |
| `crates/loki-runtime/src/host/compose.rs` | `open_with_arena`: `.with_business_service(Arc::new(RuntimeBusinessService::new(...)))` on the existing `ServerConfig::new(...)` call. Also `Arc`-wrapped `services` (was `RuntimeServices`, now `Arc<RuntimeServices>`) so the exact same composed instance is shared with the new `BusinessService`, and added `storage_handle()`/`services_handle()` accessors (cheap `Arc::clone`) alongside the existing `storage()`/`services()` accessors, so this crate's own tests can compose a second `BusinessService`-backed `Server` over the real host's real composed state without a second database open — see §7 decision 1 | yes — same file, no edit to migration/discovery/token/pipe-bind order/behavior |
| `crates/loki-runtime/tests/business_service.rs` (new) | `ComposedHost`-level integration suite (Deliverable 7): startup + all 4 reads + all 3 agent-mutations round trip; Deliverable-3 adversarial collision test; deferred-method `UnsupportedMethod` test; unauthorized-principal-denied test; restart/reconcile test. **Currently crashes the test process** — see §1/§4 | yes — new-file path named in §5 |
| `crates/loki-runtime/tests/process_behavior.rs` | Additive only: one new `#[ignore]`d black-box test (`run_mode_serves_a_real_business_rpc_round_trip_that_completes_with_a_definitive_authorization_outcome`), citing `DAT-002-MEM-HOST-VERIFY-001` in the exact same style as the two existing tests, which are unmodified. This new test asserts a real, definitive `PermissionDenied` round trip (not `Ok`) because the production single legacy credential (`ServerConfig::new`'s empty-authority principal) denies every business method before `BusinessService::call` is ever reached — a limitation this packet's §3 records as `P1-NODE-IPC-CLIENT-001`'s scope, not this node's; documented in the test's own doc comment | yes — additive-only, per §5 |
| `Docs/graph/handoffs/P1-RUNTIME-COMP-001-1.md` (this file) | Own handoff | yes |

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | Windows 11, rustc/cargo from workspace toolchain | exit 0 | ran clean after `cargo fmt --all` |
| `cargo clippy --workspace --all-targets -- -D warnings` | same | exit 0, zero warnings | full workspace check |
| `cargo build --workspace` | same | exit 0 | only pre-existing, unrelated `openssl-sys` static-link `LNK4099` PDB warnings (present before this node's changes too) |
| `cargo test -p loki-runtime --lib -- --test-threads=1` | same | 62 passed, 0 failed | includes every pre-existing `src/**` unit test, unmodified by this node, all still green |
| `cargo test -p loki-ipc -p loki-storage -p loki-mutation-contract -- --test-threads=1` | same | 201 passed across all suites, 0 failed | regression check per §7 acceptance criterion — none of these crates' behavior was touched |
| `cargo test -p loki-runtime --test host_lifecycle -- --test-threads=1` **before** this node's changes (`git stash`, re-run on pristine `84168b5`) | same | exit 0, 4/4 passed | baseline control — confirms the crash below is caused by this node's change, not a pre-existing environment defect |
| `cargo test -p loki-runtime --test host_lifecycle -- --test-threads=1` **after** this node's changes | same | `test startup_accepts_a_real_ipc_connection_and_is_located_as_healthy ...` then process exits `0xc0000005 STATUS_ACCESS_VIOLATION`; the other 3 tests in that file pass first | `tests/host_lifecycle.rs` is **unmodified** by this node — proves the crash is caused by `compose.rs`'s new `.with_business_service(...)` wiring alone, not by anything in the new test file |
| `cargo test -p loki-runtime --test business_service -- --test-threads=1` | same | `running 5 tests` then process exits `0xc0000005 STATUS_ACCESS_VIOLATION` on the very first (alphabetically-first) test | reproduced identically on this node's own new suite; §7 decision 2 traces the mechanism |
| `cargo test -p loki-runtime --test process_behavior` | not run (all `Run`-mode-spawning tests are `#[ignore]`d, gated on `DAT-002-MEM-HOST-VERIFY-001`, per design) | n/a | the 4 non-`Run`-mode tests in that file are unaffected by this node and were not re-verified individually beyond the workspace build |

Mandatory verification not run:

- Independent test-reviewer / security-reviewer sign-off (§7/§8 of the
  packet): not requested, since the crash above means the node is not ready
  for that gate.
- A clean, full `cargo test -p loki-runtime -- --test-threads=1` pass: not
  achievable in the current state — see §1.

## 5. Contracts, migrations, and compatibility

- Contracts changed: none. `loki_ipc::server::BusinessService`,
  `loki_runtime::services::{RuntimeReadPort, MutationSubmissionPort}`,
  `loki_mutation_contract::MutationAcceptanceRequest`, and
  `loki_storage::agents::AgentsRepository` are all consumed exactly as
  designed, unedited.
- Consumers affected: none yet (no caller wiring in this node — out of
  scope per §5's forbidden list).
- Migration: none; no schema change.
- Backward/rolling compatibility: n/a.
- Generated-contract drift: n/a.
- Build recipe/executor/provenance compatibility: n/a.
- Cloud API/offline/notification compatibility: n/a.

## 6. Security, privacy, and device safety

- Trust boundaries touched: the local authenticated IPC transport
  (`loki-ipc`), now serving real storage-backed business RPC results for the
  first time.
- Authorization/approval impact: Deliverable 3's kind-check gate
  (`PrincipalKind::AgentConnection` required before `principal_id` is ever
  treated as a `connection_id`) is implemented as a hard, unconditional,
  early return at the top of `agent_mutation` in `business_service.rs` —
  before any storage lookup, before any other field of `principal` is
  consulted. Proven by a real adversarial test
  (`desktop_principal_colliding_with_a_real_connection_id_is_denied_not_served`)
  granting the colliding `DesktopInstallation` principal the exact
  `ManageAgentRuns`/`AcknowledgeWorkIntents` authorities a real agent
  principal would have, so only the missing kind check could explain a
  denial. (This test's *logic* is correct by inspection and by the crate's
  own unit-level authorization tests, which are unaffected by the crash; its
  reliable pass/fail status in the crashing suite could not be independently
  reconfirmed run-over-run — see §1.)
- No `MutationAcceptanceRequest` is ever constructed with a fabricated
  `grant_generation` — the single construction site's `grant_generation`
  argument is `connection.generation`, read from
  `Storage::get_agent_connection(connection_id).generation`, never a literal
  or default.
- `RestartRequest`/`FlashRequest`/`BuildRequest` are explicit,
  dedicated match arms (not a wildcard) returning `RpcError::UnsupportedMethod`,
  each with a doc comment citing this packet and the follow-up node.
- **New finding, this node's own discovery (§1, §7 decision 2)**: installing
  any `BusinessService` that holds a resource tied to
  `loki_storage::secure_memory`'s process-global arena (this node's
  `Arc<Storage>`, the very first such `BusinessService` ever composed)
  creates a real use-after-free hazard at `ComposedHost::run()` shutdown,
  because `crates/loki-ipc/src/server.rs`'s `Server::serve` accept loop
  spawns each per-connection dispatch task fully detached (`tokio::spawn`
  with no retained `JoinHandle`), so nothing in `crate::host::compose` can
  know when a still-open connection's last `Arc<Storage>` clone has actually
  dropped before `SecureMemoryRuntime::shutdown()` (called explicitly, and
  again via `Drop` regardless) unmaps/wipes the arena a live connection's
  SQLite object may still reference. This is a genuine, reproduced
  `STATUS_ACCESS_VIOLATION`, not a theoretical concern — see §4.
- Device impact: none — no connector/device operation is reachable through
  the 8 in-scope methods.
- New dependencies: none.
- Remaining risk: **high, unresolved** — the crash above must be fixed
  before this node (or its integration) can be treated as safe to run
  against any real client connection that outlives the accept loop, which
  is the ordinary case (a client typically stays connected across a host
  shutdown/restart, exactly like `tests/host_lifecycle.rs`'s own pre-existing
  `startup_accepts_a_real_ipc_connection_and_is_located_as_healthy` test
  models).

## 7. Decisions and assumptions

1. **`RuntimeServices` `Arc`-wrapping in `compose.rs`.** Deliverable 1
   requires `RuntimeBusinessService` to hold "a shared handle to
   `RuntimeServices`... passed in at construction, never opened a second
   time." `ComposedHost` previously held `services: RuntimeServices` by
   value with no way to share it. I changed the field to
   `Arc<RuntimeServices>`, kept `services()`/`storage()`'s existing
   `&RuntimeServices`/`&Storage` return types unchanged (so no existing
   caller signature broke — there were none outside this crate's own tests
   at time of writing, confirmed by `grep`), and added
   `services_handle()`/`storage_handle()` returning cheap `Arc::clone`s for
   this crate's own tests to reuse the exact same composed instances. This
   is a narrower behavioral footprint than it might look: no change to
   startup order, migration, discovery-file, token-loading, or pipe-bind
   behavior — only how the already-composed instances are subsequently
   shared. Flagged here per §5's "no other behavior change to this file"
   instruction, since it is a real (if narrow and necessary) addition beyond
   the literal single call-site swap the packet's prose describes.
2. **The crash's root cause and why it is out of this node's fixable
   scope**, in more detail than §1/§6: before this node, `ServerConfig`'s
   `business_service` field only ever held `UnsupportedBusinessService`
   (zero state) or, in tests, `TestBusinessService`-style doubles wired
   independently per test (never sharing lifetime with a real
   `ComposedHost`'s own `SecureMemoryRuntime`). This node is the first to
   install a `BusinessService` holding `Arc<Storage>`/`Arc<RuntimeServices>`
   *and* to have that same instance's `SecureMemoryRuntime` torn down by
   `ComposedHost::run()`/`Drop` in the same process. `crate::server::Server`
   in `loki-ipc` (`P1-RPC-CONTRACT-001-R1`, accepted, forbidden to edit)
   spawns each accepted connection's dispatch loop via a bare
   `tokio::spawn`, discarding the `JoinHandle` — there is no way for
   `ComposedHost` (or this suite's own second `Server`, bound the same way
   any consumer of the public `BusinessService`/`ServerConfig` API would) to
   wait for outstanding connections to finish before safely tearing down a
   resource those connections' business-service clone still references. I
   considered and rejected two workarounds as unsafe/dishonest rather than
   real fixes: (a) polling `Arc::strong_count` in `compose.rs` and skipping
   `secure_memory.shutdown()` if a clone is still outstanding — rejected
   because `SecureMemoryRuntime`'s own `Drop` impl calls
   `shutdown_inner()` unconditionally regardless of whether `.shutdown()`
   was explicitly called, so this could not actually be avoided without
   leaking the arena forever (itself weakening the documented
   wipe-on-shutdown security property, which `AGENTS.md`/this packet's own
   §11 forbid weakening); (b) not sharing `Arc<Storage>` into the business
   service at all and instead re-opening a second `Storage` handle per call
   — rejected because it is exactly the "second database open" Deliverable
   1 and §5 forbid, and would not fix the underlying teardown-ordering
   hazard for the *existing* shared handles other services in
   `RuntimeServices` already hold today for the host's own lifetime. A real
   fix requires `crates/loki-ipc/src/server.rs` to track spawned connection
   tasks (e.g. a `tokio::task::JoinSet`) and expose a way for a caller to
   drain/await them as part of (or before) `Server::serve`'s shutdown
   signal resolving — an edit to an already-accepted, forbidden-to-edit
   contract, hence `NEEDS_REPLAN` rather than a silent in-scope patch.
3. **`ReadMeta.source_runtime_id`**: this crate has no per-process "runtime
   instance identity" distinct from the per-device `RuntimeId` newtype, so I
   used a stable literal (`"loki-runtime"`) rather than inventing a
   per-process identifier nothing else in this workspace persists —
   documented in `business_service.rs`'s own doc comment on the constant.
4. **`DeviceGet`'s "lookup"**: `RuntimeReadPort` has no single-device read
   (only `list_devices`/`device_health`/`runtime_status`), and widening that
   already-accepted, deliberately bounded port is forbidden (§5). `DeviceGet`
   is implemented as a bounded scan through `list_devices`'s own real
   pagination (`find_device` in `business_service.rs`) until the target is
   found or the registry is exhausted — real, non-fabricated, but O(n) in
   registered device count. Flagged for reviewer awareness; not something I
   judged worth a contract-widening request given "Custom" support-tier
   registries in this product are expected to be small, but a future node
   revisiting `RuntimeReadPort` should consider a real single-device read.
5. **`AgentRunRegister`'s `target_id`**: this method has no request-scoped
   target entity (the run doesn't exist yet); I mirrored the exact sentinel
   (`"agent-run:root"`, used only when `parent_run_id` is absent) the
   already-accepted `crates/loki-ipc/tests/support/business.rs`
   `TestBusinessService` test double established, rather than inventing an
   independent convention.

Per this handoff's own result, none of the above decisions are hidden as
silently resolved — decision 2 in particular is the material one this
`NEEDS_REPLAN` rests on.

## 8. Review focus

- The crash reproduction itself (§4): re-run
  `cargo test -p loki-runtime --test host_lifecycle -- --test-threads=1`
  (unmodified file) both before and after this node's diff to confirm
  attribution.
- `crates/loki-runtime/src/host/business_service.rs`'s `agent_mutation`
  function: confirm the `PrincipalKind::AgentConnection` gate is truly
  unconditional and precedes the storage lookup (Deliverable 3's core
  property) — this part of the implementation is not in question, only the
  lifecycle/shutdown-safety issue is.
- Whether `crates/loki-ipc/src/server.rs`'s detached-connection-task design
  should itself become the subject of a new, explicit node (rather than
  reopening `P1-RPC-CONTRACT-001-R1`) — see §9.
- Whether decision 1 in §7 (the `Arc`-wrapping of `services` in `compose.rs`)
  is itself in-scope or should be routed back as a separate, narrower
  finding independent of the crash.

## 9. Follow-up graph changes proposed

- **Blocking, new node** (working title `P1-RUNTIME-COMP-SHUTDOWN-CORR`
  or similar): extend `crates/loki-ipc/src/server.rs`'s `Server`/`Server::serve`
  so a caller can track and drain/await all spawned per-connection dispatch
  tasks as part of (or strictly before) shutdown completing — the real fix
  for the `STATUS_ACCESS_VIOLATION` this handoff reproduces. This node
  (`P1-RUNTIME-COMP-001`) should be re-dispatched (or its accepted portion
  merged) only once that correction lands, since the two are tightly
  coupled: `RuntimeBusinessService` is only unsafe to install *because* of
  the gap that new node would close.
- Previously-named, unaffected follow-up still applies as originally
  scoped: a not-yet-created node (working title
  `P1-RUNTIME-COMP-DESKTOP-MUTATION-001`) for `RestartRequest`/
  `FlashRequest`/`BuildRequest`, gated on a not-yet-opened decision covering
  how a Hello-authenticated `DesktopInstallation` IPC principal is bound to
  a real `LocalAuthority` session/generation. This node's own scope
  deliberately excludes all three, per §1/§9/§11 of the task packet, and
  that exclusion is unaffected by the `NEEDS_REPLAN` above.
- Only the orchestrator updates the task graph; this section records
  proposals only.
