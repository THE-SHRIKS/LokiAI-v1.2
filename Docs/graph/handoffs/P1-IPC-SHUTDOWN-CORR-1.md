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

## 10. Follow-up correction — independent-review findings closed

Result: `ACCEPTED-pending-review` (both findings below are fixed, re-verified
through the real `ComposedHost::run` path, and all required verification
passes; still pending human/orchestrator integration review, same as §"Result"
above).
Started from: `node/P1-IPC-SHUTDOWN-CORR`@`0cc6fde` (this handoff's own tip).
New commit(s): see this file's own commit and the accompanying code commit on
`node/P1-IPC-SHUTDOWN-CORR`.
Completed at: `2026-08-31T23:59:00Z` (approximate).

An independent security review found that §1-§9 above did **not** actually
close the reported crash end-to-end. Both findings are addressed below.

### 10.1 Critical finding — timeout mismatch between `loki-ipc` and its real caller

**The defect.** `crates/loki-runtime/src/host/compose.rs::ComposedHost::run`
wrapped `server.serve()` in `tokio::time::timeout(IPC_SHUTDOWN_GRACE, ipc_task)`
with `IPC_SHUTDOWN_GRACE = 5s` — **shorter** than `loki-ipc`'s own internal
`CONNECTION_DRAIN_GRACE = 8s`. When the outer timeout fired, it did **not**
cancel or abort `ipc_task`; it only stopped *waiting* and fell through to
`drop(storage); secure_memory.shutdown()` regardless, while the detached
`serve()` task (and everything inside it) kept running. Any connection whose
drain took 5-8+ seconds reintroduced the exact use-after-free-class crash
this whole node exists to close.

**The fix.** `ComposedHost::run` no longer races `ipc_task` against any
independent outer timeout at all — it `.await`s it directly:

```rust
let _ = ipc_stop_tx.send(());
if let Err(err) = ipc_task.await {
    tracing::error!(error = %err, "ipc accept loop task ended abnormally ...");
}
```

This is option (a) from the dispatching message, chosen over (b) for two
reasons specific to this codebase:

1. After §10.2's fix, `server.serve()` (what `ipc_task` runs) is **itself**
   a genuine, enforced upper bound: every still-open connection is asked to
   close, every per-call task it spawned is drained-or-aborted
   (`CALL_TASK_DRAIN_GRACE`), the writer task is drained-or-aborted
   (`WRITER_SHUTDOWN_GRACE`), and if a connection still somehow does not
   finish, the whole per-connection `JoinSet` is force-aborted
   (`CONNECTION_DRAIN_GRACE`) — and `tokio::task::JoinSet`'s documented
   `Drop` behavior (aborts every task it still owns) means that force-abort
   transitively aborts anything still outstanding, including a connection's
   own nested per-call `JoinSet`. `ipc_task` is therefore already guaranteed
   to resolve in bounded time; racing it against a second, shorter,
   independent timeout adds no safety and, as this exact bug proved, can
   actively remove it if the two bounds are not perfectly synchronized
   forever (which the original fix already failed to do once).
2. This directly follows the pattern this codebase already uses at every
   other layer of this same shutdown path (`WRITER_SHUTDOWN_GRACE` +
   `.abort()` inside `run_dispatch_loop`; `CONNECTION_DRAIN_GRACE` +
   `abort_all()` inside `Server::serve`): bound the wait **at the point
   that owns the resource being waited on**, then abort-and-confirm, never
   "give up and continue" from a level that cannot itself cancel anything.
   `compose.rs` cannot cancel `server.serve()`'s internals; `loki-ipc` can.
   Moving the one enforced bound into `loki-ipc` (already true before this
   fix, just miscalibrated against `compose.rs`'s separate, shorter one) and
   removing `compose.rs`'s redundant, unsynchronized copy is what actually
   fixes the class of bug, not just this instance of it.

The unused `IPC_SHUTDOWN_GRACE` constant and its now-dangling doc references
were removed from `compose.rs`.

### 10.2 High finding — per-call dispatch tasks were untracked and unbounded

**The defect.** `spawn_unary_call`/`spawn_stream_call` inside
`run_dispatch_loop` were still bare-`tokio::spawn`ed, never added to any
`JoinSet`. The only accidental backstop was `run_dispatch_loop`'s existing
`writer_task` join (bounded by `WRITER_SHUTDOWN_GRACE`), which only
indirectly waits for per-call tasks because they each hold a clone of
`outgoing_tx` — and that backstop breaks the moment `writer_task` is
force-aborted at its own grace-period boundary: the abort drops the
*receiver* half, so the still-running call task's next `outgoing_tx.send()`
errors out and it unwinds quickly in the common case, but there was still no
actual guarantee this always happens promptly, and — critically — nothing
server-side bounds how long a call can run before that point, since
`deadline_unix_millis` is entirely client-supplied with no server-side
maximum.

**The fix.** `run_dispatch_loop` now owns a connection-local
`call_tasks: JoinSet<()>`. `spawn_unary_call`/`spawn_stream_call` were
changed from `tokio::spawn(...)` to `call_tasks.spawn(...)` (both now take
`&mut JoinSet<()>` as their first parameter). After the dispatch loop ends
(client disconnect, protocol error, or server-initiated shutdown) — and
*before* dropping `outgoing_tx` or touching the writer task — the loop now
drains `call_tasks`, bounded by a new `CALL_TASK_DRAIN_GRACE` (10s), and
force-aborts (then confirms via `join_next()`) anything still running past
that grace. Only after every per-call task this connection ever spawned is
confirmed finished does the function proceed to the (unchanged) writer-task
drain.

This closes the gap completely, not just narrows it: once
`Server::serve`'s outer per-connection `JoinSet` reports a connection task
"done," every call it ever spawned is now provably also done — either it
finished on its own, was aborted-and-confirmed by `CALL_TASK_DRAIN_GRACE`,
or (in the pathological case where the whole connection is force-aborted by
the outer `CONNECTION_DRAIN_GRACE`) was transitively aborted by
`JoinSet::drop`, per §10.1 point 1.

### 10.3 Grace-period recalibration

Closing both findings together required recalibrating the grace-period
nesting so a genuinely slow-but-legitimate in-flight call (the reviewer's
named 6-9s reproduction window) has room to be observed to actually finish,
not merely be safely aborted, before its connection's drain gives up:

| Constant | Location | Old | New | Why |
|---|---|---|---|---|
| `CALL_TASK_DRAIN_GRACE` | `loki-ipc/src/server.rs` (new) | n/a | 10s | Comfortably above the reviewer's named 6-9s window, so that scenario is observed to finish, not merely get aborted. |
| `CONNECTION_DRAIN_GRACE` | `loki-ipc/src/server.rs` | 8s | 20s | Must exceed `CALL_TASK_DRAIN_GRACE` (10s) + `WRITER_SHUTDOWN_GRACE` (5s) = 15s with real margin, since those two now run sequentially inside one connection's own teardown before this outer bound would ever need to force-abort it. |
| `IPC_SHUTDOWN_GRACE` | `loki-runtime/host/compose.rs` | 5s | removed | No longer raced against anything — see §10.1. |
| `WRITER_SHUTDOWN_GRACE` | `loki-ipc/src/server.rs` | 5s | 5s (unchanged) | Not part of either finding. |

### 10.4 Required re-verification: the actual 5-9s-window reproduction

Both prior verifications (the original §4 and any narrower unit test) were
insufficient per the review because they exercised `loki_ipc::Server::serve`
in isolation, never the real `crate::host::compose::ComposedHost::run` path
where the actual bug lived. This re-verification closes that gap directly.

**New test-support surface** (both additive, `cfg(any(test, feature =
"test-support"))`-gated, matching the existing `open_with_test_arena`
pattern — zero reachability from production `open()`):

- `ComposedHost::open_with_test_arena_and_business_service(config,
  credential_resolver, business_service)` — lets a test install a real,
  controllable `BusinessService` (and a credential resolver whose principal
  is actually authorized for it) into the real `Server`/`ServerConfig`
  `ComposedHost::open`/`run` compose, instead of the crate's permanent
  fail-closed default (no business method is wired into production
  `open()` yet — see `compose.rs`'s own top doc comment). This was
  necessary because there was no other way to drive a real, timed
  `business_call` through `ComposedHost::run`'s actual wiring.

**New test**:
`crates/loki-runtime/tests/host_lifecycle.rs::slow_business_call::
run_does_not_return_until_a_slow_in_flight_business_call_actually_finishes`.

- A `SlowBusinessService` fixture (same shape as
  `loki-ipc/tests/shutdown_drain.rs`'s own, duplicated rather than shared —
  this crate has no dev-dependency on `loki-ipc`'s test-support module)
  notifies a `Notify` the instant its `call()` starts, sleeps a real
  `Duration::from_secs(7)` (inside the reviewer's named 6-9s window), then
  flips an `AtomicBool` and returns a real `BusinessRpcResult::RuntimeStatus`.
- The test starts a real `ComposedHost` via
  `open_with_test_arena_and_business_service`, spawns `host.run(...)` as a
  task, connects a real `Client` over the real named pipe, and fires a real
  `business_call(Method::RuntimeGetStatus, ...)` without awaiting it.
- Once the fixture confirms the call has actually started (deterministic,
  not timing-guessed), the test signals `run()`'s shutdown — so the call is
  genuinely still in flight (about to sleep for ~6.9 more seconds) when the
  primary shutdown/heartbeat race inside `run()` ends and it moves to tear
  down the IPC task.
- The test asserts, in order: (1) the call has *not* completed yet at the
  moment shutdown is signalled (sanity check on its own timing assumption);
  (2) by the time `run()`'s returned task actually resolves,
  `call_completed` **must already be `true`** — the exact property
  `compose.rs`'s pre-fix outer-timeout race did not guarantee; (3) the
  outcome is a clean `RunOutcome::Shutdown`; (4) the client's own
  `business_call` future (awaited separately, after `run()`'s task) either
  observes the real `RuntimeStatus` result or, acceptably,
  `CallError::ConnectionClosed` if the transport was torn down immediately
  after delivery — `call_completed` (set by the service itself, independent
  of whether the client observed the reply before its transport went away)
  is the property this test actually needs.

**Before/after reproduction** (exactly what the dispatching message asked
for — proving the fix through the *real* `ComposedHost::run` path, not
`server.serve()` in isolation):

1. Built the fix described in §10.1/§10.2/§10.3, confirmed
   `run_does_not_return_until_a_slow_in_flight_business_call_actually_finishes`
   passes against it (`cargo test -p loki-runtime --test host_lifecycle
   slow_business_call -- --test-threads=1`) — pass, `ComposedHost::run`
   correctly waits ~7s for the slow call before returning.
2. **Without touching the new test file**, surgically reverted only the two
   defect-causing code shapes back to their pre-fix behavior *in place* (not
   via `git stash`, since the new test-support hook the test needs did not
   exist in the originally-committed `0cc6fde` code and a literal stash
   would fail to compile):
   - `compose.rs`: restored the `tokio::time::timeout(5s, ipc_task)` race
     that proceeds regardless of whether `ipc_task` finished.
   - `server.rs`: restored `spawn_unary_call`/`spawn_stream_call` to bare,
     untracked `tokio::spawn(...)` (ignoring the new `call_tasks` parameter).
   Re-ran the identical, unmodified test:
   `cargo test -p loki-runtime --test host_lifecycle slow_business_call --
   --test-threads=1` → **FAILED** in 5.43s:
   ```
   thread 'slow_business_call::run_does_not_return_until_a_slow_in_flight_business_call_actually_finishes'
   panicked at crates\loki-runtime\tests\host_lifecycle.rs:469:9:
   ComposedHost::run() returned before the in-flight business call actually
   finished — the exact use-after-free-class hazard this re-verification targets
   ```
   This is the reproduced hazard: `ComposedHost::run()`'s task resolved at
   ~5.4s (matching the old, reintroduced `IPC_SHUTDOWN_GRACE` = 5s) while the
   business call — and therefore the connection task, and therefore
   everything it still held — was still genuinely running, 1.5+ seconds
   before it would naturally finish. Exactly the scenario named: a call in
   the 5-8s window that the old, shorter outer timeout gave up on first.
3. Restored both files exactly to their post-fix state (byte-for-byte,
   verified via `git diff --stat` matching the pre-reversion diff shape).
   Re-ran the full required verification (below) — all green.

### 10.5 Verification performed (this correction)

| Command/scenario | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0, zero warnings (full workspace) |
| `cargo test -p loki-ipc -- --test-threads=1` | **108 passed, 0 failed** (unchanged from §4 — no existing test needed modification) |
| `cargo test -p loki-runtime -- --test-threads=1` | **72 passed, 0 failed, 2 ignored** (62 lib + 5 `host_lifecycle`, incl. the new test + 4 `process_behavior`, 2 pre-existing ignores unrelated to this node — `DAT-002-MEM-CODE-001`'s fail-closed secure-memory gate — + 1 `secure_memory_arena_runtime`) |
| `cargo test -p loki-runtime --test host_lifecycle slow_business_call -- --test-threads=1` against the deliberately-reverted pre-fix code shapes (§10.4 step 2) | **FAILED**, as expected — proves the new test actually detects both defects, not just exercises the new code paths |
| Same command against the restored, fully-fixed code | **passed**, `run()` genuinely waits ~9s total (7s call + connect/handshake/drain overhead) before returning, confirmed via the test's own assertions |

### 10.6 Contracts, migrations, and compatibility (this correction)

- `crates/loki-ipc`: `spawn_unary_call`/`spawn_stream_call` gained a new
  first parameter (`&mut JoinSet<()>`) — both are module-private (`fn`, not
  `pub fn`), so this is not a public API change. `Server::serve`'s and
  `Connection::handle`'s public signatures are unchanged (same claim as §5,
  still true). Two new `pub` items: `CALL_TASK_DRAIN_GRACE` (const) and
  `CONNECTION_DRAIN_GRACE`'s value changed (8s → 20s) — both are
  implementation-detail constants, not part of any wire contract.
- `crates/loki-runtime`: `ComposedHost::open_with_test_arena` gained no
  signature change. Two new `pub` methods, both `cfg(any(test, feature =
  "test-support"))`-gated (zero production reachability, matching the
  existing `open_with_test_arena` guarantee):
  `open_with_test_arena_and_business_service`. The private
  `open_with_arena` helper's signature changed (now takes an
  `Option<(credential_resolver, business_service)>` instead of nothing) —
  private, not a public contract.
- No wire-protocol, persisted-schema, or cross-process contract changed.
- `IPC_SHUTDOWN_GRACE` (private `compose.rs` constant) removed entirely —
  not part of any public contract.

### 10.7 Security, privacy, and device safety (this correction)

- This correction closes a real, reproduced (§10.4) use-after-free-class
  hazard that §1-§9's original fix did not actually close end-to-end,
  despite believing it had. The new `open_with_test_arena_and_business_service`
  test-support hook does not weaken any production security control: it is
  compiled only under `cfg(any(test, feature = "test-support"))`, the same
  guarantee `open_with_test_arena` already relied on, and production
  `open()` never calls it.
- No change to authentication, authorization, ACL, frame validation, or
  business-method dispatch logic — the diff is additive shutdown-sequencing
  and task-tracking plumbing only.
- Grace-period lengthening (§10.3) means a background host's clean shutdown
  can now legitimately take up to ~20s in the worst case (a genuinely wedged
  connection) instead of the old, unsafe-but-faster ~8s. This is a
  deliberate safety-over-speed tradeoff — see §10.1 point 2 — and does not
  affect any device-changing operation's own safety envelope (business
  methods that submit device mutations already go through the
  accept-then-track pattern in `loki_acceptance_store`, not a
  blocking-for-the-operation's-full-duration RPC call).

### 10.8 Decisions and assumptions (this correction)

1. Chose compose.rs option (a) (`.await` `ipc_task` directly, no outer
   timeout) over option (b) (keep an outer timeout but abort-and-confirm on
   it) — see §10.1's two reasons. `loki-ipc` already had to implement the
   "bound-then-abort-then-confirm" pattern internally regardless (that is
   what actually terminates a genuinely wedged connection); duplicating it a
   second time at the `compose.rs` layer, now pointed at a task `compose.rs`
   itself cannot inspect or partially cancel (only the whole `JoinHandle`),
   would add complexity without adding safety.
2. Recalibrated `CONNECTION_DRAIN_GRACE`/added `CALL_TASK_DRAIN_GRACE`
   generously (10s/20s) rather than tightly, so the specific 6-9s
   reproduction window is provably a "genuinely finishes" case, not merely
   a "safely aborted" case — matching the dispatching message's
   phrasing ("shutdown genuinely waits for the slow call to finish"). A
   tighter pair of constants (e.g. 5s/8s, unchanged from the original fix)
   would still have been *safe* (no UAF — abort-then-confirm is unconditionally
   safe regardless of duration), but would not have satisfied that specific,
   explicit re-verification requirement for a 6-9s call.
3. Added test-support-only compose.rs hooks rather than reproducing the
   call solely via `loki_ipc::Server::serve` again, because the review's
   central point was that testing `server.serve()` alone previously missed
   a bug that only existed in `compose.rs`'s own caller-side code. A
   re-verification that again avoided exercising `compose.rs::run` would not
   have actually addressed the review's core objection.
4. Did not attempt to add a server-side maximum on client-supplied
   `deadline_unix_millis` (the reviewer's parenthetical aside). §10.2's fix
   makes this unnecessary for the specific UAF-class hazard in scope: any
   call, however long its client-requested deadline, is now provably
   drained-or-aborted-and-confirmed before its connection is considered
   done. A server-side deadline cap would be a distinct, separate
   design decision (denial-of-service/resource-fairness policy, not a
   memory-safety fix) and was judged out of this correction's scope.

### 10.9 Follow-up graph changes proposed (this correction)

- No change to §9's proposals; both still stand.
- New, optional: if a future node wants a stricter server-side maximum
  business-call deadline (independent of this correction — see §10.8 point
  4), that is a policy decision for the orchestrator/product, not a
  safety-fix follow-up this node's scope covers.
