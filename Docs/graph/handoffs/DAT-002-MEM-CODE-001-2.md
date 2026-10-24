# Worker Handoff — `DAT-002-MEM-CODE-001` (continuation session 2)

Result: `FAILED_VERIFICATION`
Worker: `Claude Code implementer (this session)`
Started from: worktree `L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001` in the real broken
state left by a prior session interrupted by a platform session limit
(uncommitted work already present for both gaps, plus a reported `loki-storage`
build break)
Completed at: `2026-08-26T14:10:00Z`

## 1. Outcome

Both named gaps from `Docs/graph/handoffs/DAT-002-MEM-CODE-001-CONSUMERS-1.md`
§7-9 are fixed and independently verified with real test runs. The reported
`cargo build -p loki-storage` / `E0463: can't find crate for 'tracing'` build
break is fully root-caused: it was **not** a dependency-graph, `Cargo.toml`, or
`Cargo.lock` defect (the self-referential `loki-storage` dev-dependency is
correct and not implicated) — it was a corrupted incremental-build cache in the
long-lived shared `target/` directory, almost certainly left inconsistent by the
prior session being killed mid-build. A full `cargo clean` on that directory
resolved it permanently; no source, config, or lockfile change was needed or made
to fix the build itself.

One genuine, additional, in-scope defect was found and fixed while verifying gap
2 under real default-parallelism execution (a requirement this task added beyond
what CONSUMERS had run): 9 of `loki-storage`'s own inline tests had the exact
order-dependency bug CONSUMERS had already fixed in 9 other crates, just missed
here. Fixed the same way.

One genuine, distinct, **out-of-scope** defect remains and is the reason this
handoff is `FAILED_VERIFICATION` rather than `COMPLETE`: `crates/loki-runtime/
tests/process_behavior.rs` has 2 tests that spawn the real compiled
`loki-runtime` binary and assert it starts successfully end-to-end. Since
production `ComposedHost::open()` correctly fails closed with no accepted bounds
profile embedded (ADR-0005/this packet's own design, `DAT-002-MEM-HOST-VERIFY-001`
still pending), those 2 tests now fail — correctly, but in a way this task was
not authorized to touch (see §7 for why I did not fix it myself).

## 2. Root cause of the `tracing` build break (E0463)

**Not the self-referential dev-dependency.** Verified `crates/loki-storage/
Cargo.toml`'s `tracing = "=0.1.44"` entry, `Cargo.lock`'s single `loki-storage`
`[[package]]` block (one entry, `dependencies` list includes both `tracing` and
the self-referential `loki-storage` line — no duplicate `[[package]]` blocks
anywhere in `Cargo.lock`), and `cargo tree -p loki-storage -e normal` — all
consistent and correct, exactly as the orchestrator's own diagnosis already
established.

**Root cause: a corrupted incremental-build fingerprint/dep-info cache in the
long-lived shared `target/` directory**, evidenced as follows:

1. A brand-new, never-before-used `CARGO_TARGET_DIR` (`target-evidence/`, then
   independently `target-diag-<guid>/`) built `loki-storage` and the full
   workspace cleanly and repeatedly (4+ separate clean builds, no failures) with
   the *exact same* `Cargo.toml`/`Cargo.lock` the shared `target/` directory was
   failing on.
2. The shared `target/` directory (4.8-4.9 GiB, accumulated across many prior
   task sessions before this session started) reproduced `E0463` for `tracing`
   deterministically across 3 separate `cargo build --workspace` invocations in
   that same directory.
3. `cargo clean -p loki-storage` in that directory did **not** fix it — matching
   the orchestrator's own prior finding exactly, and ruling out "just this one
   package's fingerprint is stale."
4. `cargo clean -p tracing` in that same directory did not fix it either — it
   moved the *identical class* of error one level down `tracing`'s own
   dependency chain, from `can't find crate for 'tracing'` (in `loki-storage`) to
   `can't find crate for 'tracing_attributes'` (in `tracing` itself, `tracing`
   0.1.44's own proc-macro dependency). This is strong evidence the corruption
   was systemic to that directory's fingerprint/dep-info database, not isolated
   to any one crate or dependency edge — consistent with a build process having
   been hard-killed mid-write (a `.fingerprint/*.json` or `deps/*.d` file
   partially written, or accounting for a `.rlib` that was since removed/never
   completed), which is exactly what "interrupted by a platform session limit"
   during an active `cargo build`/`cargo test` would produce.
5. A full, unscoped `cargo clean` on the shared `target/` directory (removed
   10,786 files / 4.9 GiB) fully and permanently resolved it: every subsequent
   `cargo build --workspace`, `pnpm run verify`'s `cargo:build` step, and every
   `cargo test --workspace` run in this session (many, both threading modes)
   compiled `loki-storage`/`tracing` successfully, with zero recurrence.

**Fix applied:** `cargo clean` (full, unscoped) on `L:\LokiAI-v1.2-wt\DAT-002-
MEM-CODE-001\target`. No source, `Cargo.toml`, `Cargo.lock`, or `.cargo/
config.toml` change. This is a purely local, disposable build-cache repair —
not a persistent state, not a security-relevant change, fully reversible by
rebuilding.

**Note for whoever next touches this crate's self-referential dev-dependency
shape:** the shape itself is fine and not the culprit. What *is* worth carrying
forward: a `target/` directory that has lived through an interrupted/killed
build should be treated as suspect before spending time re-diagnosing "why does
this dependency I can see in `Cargo.lock` not resolve" — try a full `cargo
clean` (not a package-scoped one) before assuming a manifest/lockfile defect.

## 3. Gap 1 — `loki-runtime` test-only secure-arena entrypoint

**Fixed and verified.** Present already at session start (from the interrupted
prior session's work); I reviewed it in full and confirmed it is correct and
complete rather than redoing it:

- `crates/loki-runtime/src/host/compose.rs`: production `ComposedHost::open()`
  is **unchanged** — still calls the real `SecureMemoryRuntime::bootstrap()`
  first (unconditionally fails closed with `Profile(Unavailable)` absent an
  accepted profile), still the only entrypoint `main`/real startup ever calls.
  A new `#[cfg(any(test, feature = "test-support"))]`-gated
  `ComposedHost::open_with_test_arena()` was added, sharing the rest of the
  startup/shutdown body (`open_with_arena`) with `open()` but bootstrapping
  `loki_storage::secure_memory::test_support::bootstrap_disposable_test_arena()`
  instead — an **owned**, shutdown-able `SecureMemoryRuntime` (not a
  `require_ready()` flag check), matching the shape `ComposedHost` needs since
  it stores and shuts down its own arena on `run()` exit.
- `crates/loki-storage/src/secure_memory.rs`'s `test_support` module gained
  `bootstrap_disposable_test_arena()` (owned, `TEST_ARENA_BUSY`-guarded,
  panics rather than silently degrading on failure) alongside the pre-existing
  `ensure_test_arena_ready()` (shared, leaked, flag-style).
- `crates/loki-runtime/Cargo.toml` gained a `test-support` feature and a
  self-referential `[dev-dependencies] loki-runtime = { path = ".", features =
  ["test-support"] }` entry (mirroring `loki-storage`'s own pattern exactly) so
  `tests/host_lifecycle.rs`, a separate-process integration-test binary, can
  reach `open_with_test_arena`.
- `crates/loki-runtime/tests/host_lifecycle.rs`'s 2 previously-failing tests
  (`startup_accepts_a_real_ipc_connection_and_is_located_as_healthy`,
  `simulated_crash_and_restart_preserves_persisted_device_state`) now call
  `ComposedHost::open_with_test_arena` instead of `open`.
- `crates/loki-runtime/tests/secure_memory_arena_runtime.rs` (new): a
  black-box test asserting `ComposedHost::open()` (the real, unmodified
  production path) still fails closed with `SecureMemory(Profile(Unavailable))`
  and touches no base dir/database/key/token/discovery file — proving the
  test-only bypass does not weaken the production gate.

**Verification (real runs, this session):**
- `cargo test -p loki-runtime --test host_lifecycle` → **4/4 pass**.
- `cargo test -p loki-runtime --test secure_memory_arena_runtime` → **1/1
  pass**.
- Both also pass cleanly as part of every full-workspace run in this session
  (see §5).

## 4. Gap 2 — `loki-storage` test-mode arena capacity under real concurrency

**Fixed and verified.** Present already at session start: `test_support`'s
`DISPOSABLE_TEST_ARENA_BYTES` was raised from 1 MiB to **16 MiB**, with a
doc-comment in `crates/loki-storage/src/secure_memory.rs` explaining it was
sized from measuring real concurrent `cargo test --workspace` runs (default
thread-per-core harness) across all 11 crates that now bootstrap this arena,
after 1 MiB reproducibly hit `Sqlite(SqliteFailure(OutOfMemory))`. This is a
fixed, page-aligned, zero-fallback test-only constant — same design shape as
production, just a larger fixed number chosen empirically rather than guessed.
I did not choose `--test-threads=1` as the fix (the alternative the task
offered) because forcing serial execution workspace-wide would mask real
concurrency bugs (see the order-dependency bug below, which serial execution
would have hidden) and would slow every future contributor's default `cargo
test` run; a correctly-sized fixed arena with real concurrent verification is
the better fit for this crate's own no-fallback design philosophy.

**Verification (real runs, this session):** ran the full workspace test suite
multiple times under both `--test-threads=1` and default parallelism (`--no-
fail-fast` for full visibility) after the arena-size code was already in
place — **zero** `OutOfMemory`/`SqliteFailure` occurrences anywhere across the
whole workspace in any run (see §5 logs).

### Additional in-scope defect found and fixed: `loki-storage`'s own inline-test order dependency

While verifying gap 2 under real default-parallelism execution (a mode
CONSUMERS's own required verification never exercised — it always used
`--test-threads=1`), `loki-storage --lib` failed nondeterministically with:

```
thread 'managed::tests::interrupted_pre_swap_restores_source_then_retries_migration' panicked at
crates\loki-storage\src\secure_memory.rs:993:18:
disposable test-only secure arena bootstrap: SqliteConfiguration { code: 21 }
```
(`SQLITE_MISUSE`), plus a second test failing with `Once instance has previously
been poisoned` (a downstream symptom of the first panic).

Root cause: `crate::secure_memory::test_support::ensure_test_arena_ready()`
calls `sqlite3_config(SQLITE_CONFIG_MALLOC)`, which can only succeed *before*
SQLite's own process-global lazy default-allocator init. `storage.rs` had 4
tests and `migrations.rs` had 5 tests that call `Storage::open_in_memory
(Cipher::None)` / `Connection::open_in_memory()` directly — bypassing
`secure_memory::require_ready()` entirely (correct, since `Cipher::None` isn't
SQLCipher) but still triggering that same process-global SQLite init. All of
these share one test binary (`loki-storage --lib`) with `managed.rs`'s 2 tests
that *do* call `ensure_test_arena_ready()`. Under Rust's default multi-threaded
test harness, if one of the 9 `Cipher::None`/raw-`Connection` tests happened to
run first, it silently claimed that one-shot `sqlite3_config` window, and
`managed.rs`'s later call failed with `SQLITE_MISUSE`.

This is exactly the same order-dependency class CONSUMERS already diagnosed and
fixed across 9 other crates (`Docs/graph/handoffs/DAT-002-MEM-CODE-001-
CONSUMERS-1.md` §7) — it had simply been missed inside `loki-storage`'s own
tests, because every required-verification command that ever ran them used
`--test-threads=1` (which never exposes intra-binary ordering bugs). This
task's own instruction to run default-parallelism `cargo test --workspace` "for
real" is what surfaced it.

**Fix:** added `crate::secure_memory::test_support::ensure_test_arena_ready();`
as the first statement of all 9 affected tests (4 in `crates/loki-storage/src/
storage.rs`'s `#[cfg(test)] mod tests`, 5 in `crates/loki-storage/src/
migrations.rs`'s `#[cfg(test)] mod tests`), with a doc comment explaining why.
Test-only change; no production code touched.

**Verification:** `cargo test -p loki-storage --lib` run 5 consecutive times at
default parallelism (no `--test-threads=1`) — **43/43 pass every time**, zero
flakes. Confirmed absent in every subsequent full-workspace run.

## 5. Required verification — final results

Precondition run each time per the packet: isolated `TEMP`/`TMP` under this
worktree; `CARGO_TARGET_DIR` either an isolated evidence directory (direct
`cargo` commands) or the workspace's own `target/` (for `pnpm run verify`,
since its script chain does not accept an override and a non-`target`-named
directory pollutes `prettier`'s scan — see §7).

| Command | Result | Notes |
|---|---|---|
| `cargo fmt --all -- --check` | **exit 0** | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | **exit 0** | clean, whole workspace |
| `cargo build --workspace --locked` (fresh isolated target dir) | **exit 0** | proves the `E0463` break is gone independent of any target-dir corruption |
| `cargo test -p loki-runtime --test host_lifecycle` | **PASS 4/4** | |
| `cargo test -p loki-runtime --test secure_memory_arena_runtime` | **PASS 1/1** | |
| `cargo test --workspace -- --test-threads=1` (`--no-fail-fast` for full visibility) | **67 binaries `ok`, 1 `FAILED`** | the 1 failure is `loki-runtime --test process_behavior` (2/6 tests) — §6. One incidental Application-Control transient block hit `loki-storage --test repositories`; retried once, passed. |
| `cargo test --workspace` (default parallelism, `--no-fail-fast` for full visibility) | **68 binaries `ok`, 1 `FAILED`** (2 independent full runs) | same single failure, same 2 tests, both runs; zero `OutOfMemory`/order-dependency/App-Control failures either run |
| `pnpm run verify` | **fails** | `format:check`/`lint`/`typecheck`/JS `test`/`electron:typecheck`/`electron:test`/`cargo:fmt:check`/`cargo:build` all pass; fails at `cargo:test` (`cargo test --workspace`, no `--no-fail-fast`, stops at the first failing target, alphabetically `loki-runtime --test process_behavior`, same 2 genuine failures as above). Two attempts (pre- and post- the `target/` clean) additionally hit a transient Application-Control block on a freshly-linked binary each; each cleared on one retry, consistent with the project's already-documented flaky-execution-block class (`Docs/graph/evidence/W4/DAT-002/local-part-one-integration.md`, `BLD-001-R2-CODE-2.md`). |

Full logs retained under `L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001\.tmp\` for this
session (`test-single-thread-2.log`, `test-default-parallel-2.log`, `clippy.log`,
`pnpm-verify-7.log` through `-9.log`) — `.tmp/` is gitignored, not a deliverable,
kept only as local evidence for this handoff.

## 6. The one remaining, distinct, out-of-scope failure: `process_behavior.rs`

`crates/loki-runtime/tests/process_behavior.rs` (last touched in commit
`073af82`, before ADR-0005's arena work existed in this worktree — confirmed via
`git log`) has 2 tests, `ctrl_break_triggers_graceful_shutdown` and
`run_mode_composes_a_real_ipc_listener_that_accepts_a_connection_then_shuts_down_cleanly`,
that spawn the **real compiled `loki-runtime` binary** (not a test-arena-aware
test harness) and assert it logs `"ipc listener bound and serving"` within 10s
— i.e., that production `Run` mode starts successfully end to end. Since
production `ComposedHost::open()` correctly fails closed (`SecureMemory error
starting background host: no digest-bound human-accepted bounds profile is
embedded` — the exact, intended ADR-0005/this-packet behavior), the spawned
binary now exits with an error instead of ever logging that line, so both tests
fail deterministically (not flaky — reproduced identically across every run
this session).

This is real, reproducible, and **not caused by anything in this session's two
gaps** — it is a direct, structural consequence of the primary
`DAT-002-MEM-CODE-001` deliverable (production storage/runtime must fail closed
until `DAT-002-MEM-HOST-VERIFY-001` supplies an accepted profile) landing against
`RUN-001`/`RUN-003`'s own accepted acceptance criteria, which specify a real,
successful end-to-end spawned-process proof of graceful startup/shutdown. I did
**not** fix this myself:

- Giving the compiled binary an env/flag-gated way to launch with a disposable
  test arena would require a production code change to `crates/loki-runtime`'s
  `main`/`app` composition beyond `compose.rs` — explicitly forbidden by this
  task's own framing ("no production code path changes beyond what's already in
  compose.rs").
- Rewriting the test's assertions to instead expect the fail-closed error would
  be a real, product-relevant change to `RUN-001`/`RUN-003`'s accepted
  acceptance criteria (from "proves a real successful startup" to "proves a real
  fail-closed refusal") — exactly the kind of contradiction AGENTS.md says to
  record as a decision request rather than silently resolve ("Do not silently
  resolve a contradiction that changes product behavior... Record it as a
  decision request and stop only the affected graph branch").

This was not previously documented in any handoff I could find (`DAT-002-MEM-
CODE-001-1.md`, `-CONSUMERS-1.md`) — it is a new finding from this session's
required default-parallelism/full-suite verification.

## 7. Changed files (this session, beyond what existed at session start)

| Path | Change |
|---|---|
| `crates/loki-storage/src/storage.rs` | added `ensure_test_arena_ready()` to 4 `Cipher::None` inline tests (order-dependency fix, §4) |
| `crates/loki-storage/src/migrations.rs` | added `ensure_test_arena_ready()` to 5 inline tests (same fix) |
| `target-diag`, `target-diag-<guid>` | created then deleted (diagnostic scratch, not deliverables) |
| `target-evidence` | created then deleted (superseded by using the shared `target/` after the clean; not a deliverable, was blocking `prettier`'s scan under `pnpm run verify` since it doesn't match `.prettierignore`'s literal `target` pattern) |
| `target/` (shared, default Cargo target dir) | fully cleaned (`cargo clean`, no scope) to repair the corrupted incremental cache — see §2; this directory itself is gitignored (`**/target/`), not tracked |
| `Docs/graph/handoffs/DAT-002-MEM-CODE-001-2.md` | this handoff |

Everything else present in `git status` (the 9-consumer-crate test fixes,
`loki-runtime`'s test-arena entrypoint, `loki-storage`'s secure-memory module,
arena-size constant, etc.) was already present at session start from the prior
interrupted session and was reviewed, not re-authored, by me — see §3/§4 for
exactly what I verified versus what I changed.

No production (`src/`, non-`#[cfg(test)]`, non-`test-support`-feature-gated)
code was touched by me this session. No `Cargo.lock`/`Cargo.toml`/`.cargo/
config.toml` change was made by me this session (the ones already in the diff
from the prior session were reviewed and are consistent with what §3/§4
describe).

## 8. Contracts, security, and safety

- No production trust boundary changed. `ComposedHost::open()` and
  `Storage::open`/`open_in_memory`/`configure_sqlcipher`/
  `open_or_migrate_sqlcipher`'s fail-closed gates are byte-identical to
  `DAT-002-MEM-CODE-001`'s original accepted design; every test-arena bypass
  remains behind `#[cfg(any(test, feature = "test-support"))]`, itself only
  reachable via each crate's own self-referential `[dev-dependencies]` entry —
  never a default feature, never reachable from a release build.
  `secure_memory_arena_runtime.rs`'s new test explicitly proves the production
  path is still untouched.
- No real user data, elevation, VM, or device action was used or required.
- The `target/` clean is a purely local, disposable build-cache operation —
  no persistent host state, no security-relevant change, fully reversible by
  rebuilding.

## 9. Decisions and assumptions

- Chose a correctly-measured fixed 16 MiB test-only arena over forcing
  `--test-threads=1` globally for gap 2 (§4) — a design choice within the task's
  own stated discretion ("your call, document reasoning").
- Did not attempt to fix `process_behavior.rs` (§6) — treated as a genuine
  packet-Section-12-style stop condition ("structurally different... discovered
  empirically") for that one file, reported rather than worked around.
- Deleted non-deliverable scratch directories (`target-diag*`, the interim
  `target-evidence`) that were blocking `prettier`'s scan; kept `.tmp/*.log`
  evidence (gitignored, not a deliverable) for this handoff's citations.

## 10. Follow-up graph changes proposed

- A new decision node (or an accepted amendment) to resolve the `RUN-001`/
  `RUN-003` vs. `ADR-0005`/`DAT-002-MEM-CODE-001` contradiction in
  `process_behavior.rs` (§6) — needs an explicit choice between (a) giving the
  real binary a test-support-gated startup path (a production code change,
  needs its own review), (b) rewriting the 2 tests' asserted behavior to match
  the now-correct fail-closed reality (a test-contract change against
  `RUN-001`/`RUN-003`'s own accepted criteria), or (c) accepting this as a known
  gap until `DAT-002-MEM-HOST-VERIFY-001` lands a real profile, at which point
  the real binary would start successfully in every environment without any
  test-mode change at all. This blocks `cargo test --workspace` (any threading
  mode) and `pnpm run verify` from a fully clean pass today.

Only the orchestrator updates the task graph.
