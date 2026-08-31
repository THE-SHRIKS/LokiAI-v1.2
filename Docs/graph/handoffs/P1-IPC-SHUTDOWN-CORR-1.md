# Worker Handoff — `P1-IPC-SHUTDOWN-CORR`

Result: `ACCEPTED-pending-review` (implementation complete, all required
verification passes; scoped as a correction to already-accepted,
already-integrated `loki-ipc` code — human/orchestrator integration review
still required per `AGENTS.md`'s graph execution protocol)
Worker: `loki-implementer` (direct dispatch, no separate task packet — scoped
bug-fix correction authorized by the human's message)
Started from: `LokiAI-v1.2`@`84168b5` (branch tip at dispatch)
Result revision/patch: `node/P1-IPC-SHUTDOWN-CORR`@`85fa601`
Completed at: `2026-08-31T20:30:00Z`

## 1. Outcome

`crates/loki-ipc/src/server.rs`'s `Server::serve` no longer returns the
instant its `shutdown` future resolves while spawned per-connection dispatch
tasks (and any `BusinessService`/resource clone they hold) keep running
detached. `serve` now tracks every spawned connection task in a
`tokio::task::JoinSet`, actively asks every still-open connection to close
(via a new `watch`-based signal reaching both the handshake wait and the
post-handshake dispatch loop), and awaits every task to finish — bounded by
a new `CONNECTION_DRAIN_GRACE` (8s), after which any still-running task is
forcibly aborted. This closes the real, reproduced `STATUS_ACCESS_VIOLATION`
use-after-free `P1-RUNTIME-COMP-001`'s implementer found and documented
(`Docs/graph/handoffs/P1-RUNTIME-COMP-001-1.md` §1/§6/§7). I reproduced the
crash myself against the unmodified pre-fix code in that worktree, then
confirmed the fix resolves it there without modifying that worktree's own
committed state. `Connection::handle` (the pre-existing public API every
other test in the crate already calls) is unchanged in behavior — it is now
a thin wrapper over a new `handle_until_shutdown` with a shutdown signal that
never fires.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Fix `crates/loki-ipc/src/server.rs` so a caller of `Server::serve` can reliably wait for every spawned per-connection dispatch task to finish before shutdown | pass | §3, §4 |
| Preserve every existing behavior (auth, framing, deadlines, streaming, business-method dispatch) | pass | `Connection::handle` unchanged in signature/behavior; all 106 pre-existing `loki-ipc` tests pass unmodified — §4 |
| Do not weaken any existing security check | pass | No change to `authorize_method`/`authorize_stream`/`revalidate`/ACL/handshake validation logic — diff is additive (new shutdown-signal plumbing) except two `tokio::select!` additions that only ever *end* a connection early, never grant it anything |
| `cargo fmt --all -- --check` | pass | §4 |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, zero warnings | §4 |
| `cargo test -p loki-ipc -- --test-threads=1` (all existing 106 tests must still pass) | pass, 108/108 (106 pre-existing + 2 new) | §4 |
| New test(s) proving the drain-on-shutdown property | pass | `crates/loki-ipc/tests/shutdown_drain.rs`, 2 tests; both independently confirmed to **fail** against the pre-fix code (§4) |
| Proven against the actual reported crash, not just unit-tested in isolation | pass | §4's cross-worktree reproduction |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-ipc/src/server.rs` | `Server::serve` now owns a `tokio::task::JoinSet<()>` of spawned connection tasks and a `watch::channel(false)` shutdown broadcaster; on its own `shutdown` resolving, it signals every connection then drains the `JoinSet` (bounded by new `CONNECTION_DRAIN_GRACE = 8s`, then `abort_all()`). `Connection::handle` is now a thin wrapper over new `pub(crate) handle_until_shutdown(self, watch::Receiver<bool>)`, which additionally races the handshake's `Hello` wait and `run_dispatch_loop`'s frame wait against the shutdown signal. New `ConnectionError::ServerShutdown` variant (handshake-phase-only; dispatch-phase shutdown ends the connection the same way a clean disconnect does, `Ok(())`). `run_dispatch_loop` gained a `shutdown: &mut watch::Receiver<bool>` parameter and one new `tokio::select!` arm. | yes — this is the exact file/scope the dispatching message named as the correction target |
| `crates/loki-ipc/tests/shutdown_drain.rs` (new) | Two real-pipe integration tests: (1) a client connects, completes the handshake, and never disconnects (the exact shape of the reproduced crash) — asserts `serve()` returns promptly (well under `CONNECTION_DRAIN_GRACE`) and that the only remaining `Arc` reference to the `BusinessService` is the test's own handle (proving the connection task, and every clone of the service it held, actually finished and dropped); (2) a real in-flight `business_call` is still executing (600ms artificial delay) when shutdown fires — asserts `serve()` does not return until that call actually finishes. Both independently confirmed to fail against the pre-fix `server.rs` (§4). | yes — new-file path, additive only |
| `Docs/graph/handoffs/P1-IPC-SHUTDOWN-CORR-1.md` (this file) | Own handoff | yes |

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | Windows 11, workspace toolchain, worktree `L:\LokiAI-v1.2-wt\P1-IPC-SHUTDOWN-CORR` | exit 0 | ran clean after one `cargo fmt --all` pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | same | exit 0, zero warnings | full workspace, ran twice at different points in the change, both clean |
| `cargo build -p loki-ipc` | same | exit 0 | only pre-existing, unrelated `openssl-sys` static-link `LNK4099` PDB linker warnings (present before this fix too — a workspace-wide OpenSSL-vendor build artifact unrelated to this crate) |
| `cargo test -p loki-ipc -- --test-threads=1` | same | **108 passed, 0 failed** (67 lib + 4 auth + 3 business_fixtures + 15 malformed_frames + 4 handshake + 4 deadline + 6 streaming + 2 shutdown_drain(new) + 3 business_rpc + 0 support) | full breakdown captured in session log; re-run twice for stability, identical result both times |
| `cargo test -p loki-ipc --test shutdown_drain -- --test-threads=1` against **pre-fix** `server.rs` (`git stash push -- crates/loki-ipc/src/server.rs`, re-run, `git stash pop`) | same | **both new tests FAIL**: `serve_closes_a_still_open_idle_connection_and_returns_promptly_after_shutdown` fails its `Arc::strong_count` assertion (proving a `BusinessService` clone is still held after `serve()` returns on the old code); `serve_does_not_return_until_an_in_flight_call_actually_finishes` also fails | confirms the new tests actually detect the defect, not just exercise the new code path |
| Cross-worktree crash reproduction: copied this fix's `crates/loki-ipc/src/server.rs` into `L:\LokiAI-v1.2-wt\P1-RUNTIME-COMP-001` (branch `node/P1-RUNTIME-COMP-001`, commit `9149a06`), **not committed there** | same | see below | §1 |
| → `cargo test -p loki-runtime --test host_lifecycle -- --test-threads=1` in that worktree **before** applying the fix (pristine `9149a06`) | same | `startup_accepts_a_real_ipc_connection_and_is_located_as_healthy` crashes the process: `exit code: 0xc0000005, STATUS_ACCESS_VIOLATION`; the other 3 tests in that file pass first | reproduces the exact defect `P1-RUNTIME-COMP-001`'s handoff reported, independently confirmed by me, not merely trusted |
| → same command **after** copying this fix's `server.rs` into that worktree | same | **4/4 passed**, including the previously-crashing test, in `1.60s`–`1.62s` | direct proof the fix resolves the actual reported crash, not just a synthetic reproduction |
| Post-verification cleanup of the other worktree | same | `git checkout -- crates/loki-ipc/src/server.rs` in `P1-RUNTIME-COMP-001`, confirmed `git status --short crates/loki-ipc` empty | that worktree is untouched/uncommitted by this node, per instructions ("without merging the two worktrees' unrelated changes together") |

Mandatory verification not run:

- Independent test-reviewer / security-reviewer sign-off: not requested by
  this dispatch; result is reported `ACCEPTED-pending-review`, not
  `COMPLETE`, pending that gate per `AGENTS.md`'s graph execution protocol
  (a human/orchestrator gate, since this is a correction to
  already-integrated code on `LokiAI-v1.2`'s main line).
- `P1-RUNTIME-COMP-001`'s own `tests/business_service.rs` suite: attempted,
  but its freshly-linked test binary was persistently blocked by a local
  Application Control policy (`os error 4551`) in this environment across
  several retries — a known, already-documented environment flakiness in
  this session (every other freshly-linked binary in this task hit the same
  block once and succeeded on retry; this one binary did not clear even
  after a forced rebuild). `tests/host_lifecycle.rs` — the specific,
  pre-existing, unmodified file the dispatching message named as the
  required reproduction target — was successfully run both before and
  after the fix, with unambiguous results (crash, then pass). Re-running
  `business_service.rs` is `P1-RUNTIME-COMP-001`'s own concern when that
  node re-dispatches against this fix, not required by this node's own
  acceptance criteria.

## 5. Contracts, migrations, and compatibility

- Contracts changed: none at the public-API-shape level.
  `Server::serve<F>(&mut self, shutdown: F)`'s signature is byte-for-byte
  unchanged (both existing call sites —
  `crates/loki-runtime/src/host/compose.rs`'s `ComposedHost::run` and
  `crates/loki-runtime/src/host/discovery.rs`'s test fixture — compile and
  behave correctly with zero changes required on their side, since neither
  is in this node's ownership and neither was touched). `Connection::handle`
  is also unchanged in signature and behavior.
- One additive, non-breaking change: a new `pub(crate)` method
  (`Connection::handle_until_shutdown`) and a new `pub` enum variant
  (`ConnectionError::ServerShutdown`). The variant addition cannot break any
  existing exhaustive `match` outside this crate, because the only
  exhaustive match over `ConnectionError` is this file's own `Display` impl
  (updated in the same commit); every test-suite usage across the crate uses
  `matches!` (non-exhaustive by construction) — verified by `grep` before
  making the change.
- Consumers affected: `crates/loki-runtime`'s `ComposedHost::run` now gets a
  materially safer `serve()` — its existing `IPC_SHUTDOWN_GRACE` (5s) outer
  timeout in `compose.rs` still applies on top of this crate's own new
  internal `CONNECTION_DRAIN_GRACE` (8s), so that outer timeout may now
  occasionally fire *because* `serve()` itself is taking its full internal
  grace period under adversarial/slow-client conditions — this is expected,
  correct, defense-in-depth behavior, not a regression, and does not change
  `ComposedHost::run`'s own code (out of this node's ownership; not
  touched).
- Migration: none; no schema, wire-format, or persisted-state change.
- Backward/rolling compatibility: n/a (local-process lifecycle only, not a
  wire-protocol or persisted-schema change).
- Generated-contract drift: n/a.
- Build recipe/executor/provenance compatibility: n/a.
- Cloud API/offline/notification compatibility: n/a.

## 6. Security, privacy, and device safety

- Trust boundaries touched: the local authenticated IPC transport's
  connection-lifecycle/shutdown path only. No change to the handshake's
  authentication check, per-call authorization (`authorize_method`/
  `authorize_stream`), dynamic credential revalidation
  (`SessionAuthority::revalidate`), ACL/DACL pipe restriction, frame
  size/deadline limits, or business-method dispatch/response-identity
  validation — none of that code was touched.
- The two new `tokio::select!` arms (handshake-phase and dispatch-phase)
  only ever cause a connection to end *earlier* than it otherwise would
  (server-initiated close); they never grant a connection any additional
  capability, bypass any existing check, or change what a connection is
  authorized to do while it remains open.
- **This is itself the fix for a real security/reliability defect**: the
  pre-existing detached-task design meant a `BusinessService` implementation
  holding any resource tied to a caller's teardown lifecycle (e.g.
  `loki-storage`'s secure-memory arena, wiped/unmapped on shutdown per its
  documented security property) had no guarantee that resource's last
  reference had actually dropped before that teardown ran — a real
  use-after-free, not a theoretical one (§4's before/after reproduction).
  This fix restores that guarantee without weakening the wipe-on-shutdown
  property itself (unlike the two workarounds `P1-RUNTIME-COMP-001`'s
  implementer considered and rejected in their handoff §7 decision 2, e.g.
  skipping `secure_memory.shutdown()` — this fix does neither).
- Device impact: none — this crate has no connector/device operation
  surface.
- New dependencies: none. `tokio::sync::watch` and `tokio::task::JoinSet`
  are both already-enabled features of this crate's existing pinned `tokio`
  dependency (`"sync"` already includes `watch`; `JoinSet` is part of
  `tokio::task`, gated on the already-enabled `"rt-multi-thread"` feature) —
  confirmed by successful compilation with no `Cargo.toml`/`Cargo.lock`
  change at all (`git diff --stat` shows only the two files in §3).
- Remaining risk: low. The one residual, pre-existing (not introduced by
  this fix) risk noted for reviewer awareness: per-call dispatch tasks
  (`spawn_unary_call`/`spawn_stream_call` inside `run_dispatch_loop`) are
  still spawned via a bare `tokio::spawn`, not tracked by a `JoinSet`
  themselves. In practice this is not a live use-after-free gap, because
  each such task holds its own clone of the connection's `outgoing_tx`
  `mpsc::Sender`, and `run_dispatch_loop`'s existing (unmodified)
  `writer_task` join — which this fix's own `Connection`-level `JoinSet`
  drain now correctly waits for — only completes once every `outgoing_tx`
  clone (including every in-flight call task's) has been dropped, which
  happens when that call task finishes and sends its final result. A call
  that outlives `WRITER_SHUTDOWN_GRACE` (5s, pre-existing, unmodified) is
  the one case where `writer_task.abort()` fires while the call task itself
  may still be running detached — an existing, narrower, pre-existing
  design tradeoff this node did not expand scope to change, per the
  dispatching message's "narrow lifecycle-correctness fix, not a redesign"
  instruction. Flagged here for the reviewer/orchestrator to judge whether
  a follow-up node should extend the same `JoinSet` tracking one level
  deeper.

## 7. Decisions and assumptions

1. **Graceful shutdown signal via `tokio::sync::watch`, not just a
   `JoinSet` drain.** The dispatching message's minimal framing ("replace
   the bare `tokio::spawn` with a `JoinSet`... with a real method that
   awaits every outstanding task") would, taken completely literally, still
   hang indefinitely for the exact scenario the crash reproduction
   exercises: `Connection::handle`'s dispatch loop only returns when the
   client disconnects, sends a malformed frame, or the connection errors —
   never on its own initiative. A client that stays connected across a host
   shutdown (the ordinary case, and exactly what
   `startup_accepts_a_real_ipc_connection_and_is_located_as_healthy`
   models) would make an unbounded `JoinSet::join_next()` drain hang
   forever. I judged this a necessary, in-scope part of "reliably wait for
   every spawned per-connection dispatch task to finish" — a task that
   never finishes is not a task a caller can reliably wait for — and added
   the `watch`-based signal so connections actually have a way to finish
   when asked, plus the bounded `CONNECTION_DRAIN_GRACE` +
   `abort_all()` fallback as defense-in-depth matching this crate's own
   existing convention (`WRITER_SHUTDOWN_GRACE` + `.abort()`) for exactly
   this "must not hang forever no matter what" property.
2. **`ConnectionError::ServerShutdown` as a new variant, not reuse of
   `ClosedDuringHandshake`.** A server-initiated close during the handshake
   is a materially different, non-error-worthy event from a client
   disconnecting mid-handshake; keeping them distinct preserves this
   enum's existing precision (every other variant already names a specific
   cause) and costs nothing, since (per §5) no external exhaustive match
   exists to break.
3. **Dispatch-phase shutdown ends the connection as `Ok(())`, not a new
   error variant.** This mirrors the existing "clean client disconnect"
   path exactly (`Ok(None)` from `read_frame` also breaks with `Ok(())`) —
   a server-initiated graceful close is not a failure from either side's
   perspective, and `Server::serve`'s own caller
   (`connection.handle_until_shutdown(...)`'s `if let Err(err)` branch)
   only logs at `tracing::debug!` on error, so this also avoids a
   misleading debug-log line for the overwhelmingly common shutdown case.
4. **`CONNECTION_DRAIN_GRACE = 8s`, not reusing `WRITER_SHUTDOWN_GRACE`
   (5s).** `WRITER_SHUTDOWN_GRACE` bounds one connection's own internal
   writer-drain step; `CONNECTION_DRAIN_GRACE` bounds the *outer* wait for
   potentially many connections' tasks, each of which may itself spend up
   to `WRITER_SHUTDOWN_GRACE` internally (concurrently, not sequentially,
   since they're independent tasks) — 8s gives that inner grace period
   room to complete normally before the outer drain gives up and aborts,
   documented in the constant's own doc comment.
5. **Did not extend `JoinSet` tracking to `spawn_unary_call`/
   `spawn_stream_call`.** See §6's "remaining risk" — a real, narrower,
   pre-existing residual gap, but extending tracking to that level was
   judged out of the "narrow lifecycle-correctness fix, not a redesign"
   scope the dispatching message set, especially since the existing
   `writer_task`-join mechanism already provides a real (if narrower and
   time-bounded) backstop for it. Flagged explicitly rather than silently
   left unmentioned.

## 8. Review focus

- Confirm the crash-reproduction methodology in §4 is convincing: the
  before/after `host_lifecycle.rs` run against the *unmodified* target file
  named in the dispatching message, using the *actual* fixed `server.rs`
  copied (not reimplemented) into that worktree.
- Confirm `crates/loki-ipc/tests/shutdown_drain.rs`'s two new tests are
  themselves trustworthy: both were independently re-run against the
  pre-fix `server.rs` (via `git stash`) and both failed, for the reasons
  the test's own assertion messages state.
- Confirm §5's contract-compatibility claim: `Server::serve`'s and
  `Connection::handle`'s public signatures are unchanged; only `Connection`
  gained one new `pub(crate)` method and `ConnectionError` gained one new
  variant, neither of which can break an existing caller.
- §6's "remaining risk" item (per-call task tracking) is a legitimate
  candidate for a narrow follow-up node if the orchestrator judges it
  worth closing now rather than later.

## 9. Follow-up graph changes proposed

- `P1-RUNTIME-COMP-001` (currently `NEEDS_REPLAN`) can be re-dispatched (or
  its accepted portion integrated) now that this correction is available —
  its own handoff §9 named this exact follow-up as the blocking
  prerequisite. That node's own `tests/business_service.rs` suite should be
  re-run against this fix as part of its own re-dispatch (not independently
  re-verified here — see §4's "mandatory verification not run").
- Optional, non-blocking: a narrow follow-up to extend the same `JoinSet`
  tracking this node added at the connection level down to the per-call
  level (`spawn_unary_call`/`spawn_stream_call` inside `run_dispatch_loop`)
  — see §6/§7 decision 5. Not required to close the reproduced crash; noted
  for the orchestrator's own risk judgment.
- Only the orchestrator updates the task graph; this section records
  proposals only.
