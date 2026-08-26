# Worker Handoff — `DAT-002-MEM-CODE-001-CONSUMERS`

Result: `NEEDS_REPLAN`
Worker: `Claude Code implementer (this session)`
Started from: worktree `L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001` at its state after `DAT-002-MEM-CODE-001`'s uncommitted work (see `Docs/graph/handoffs/DAT-002-MEM-CODE-001-1.md`)
Result revision/patch: uncommitted worktree diff, additive on top of that state
Completed at: `2026-08-25T20:10:00Z`

## 1. Outcome

All 9 named crates (`loki-monitoring`, `loki-local-notifications`, `loki-local-setup`,
`loki-local-authority`, `loki-local-devices`, `loki-incidents`, `loki-approvals`,
`loki-agents`, `loki-agent-delivery`) now bootstrap `loki_storage::secure_memory::
test_support`'s disposable arena in their own tests before any real SqlCipher open,
replicating `loki-storage`'s own `tests/common/mod.rs` pattern exactly. Every one of
these crates' tests passes repeatably and cleanly (`cargo test -p <crate>
--all-targets -- --test-threads=1`, run individually and combined, multiple times).
Zero production (`src/`, non-`#[cfg(test)]`) code was touched in any of the 9 crates.

At the orchestrator's explicit direction I also attempted the identical pattern on a
10th, originally out-of-scope location — `crates/loki-runtime/tests/host_lifecycle.rs`
— because its 2 storage-opening tests fail with the same
`SecureMemory(Profile(Unavailable))` symptom. That attempt **does not work**: I
verified empirically (2 independent real test runs) that `ComposedHost::open`
(`crates/loki-runtime/src/host/compose.rs`) calls `SecureMemoryRuntime::bootstrap()`
directly — the real production entrypoint, which unconditionally calls
`accepted_profile()` first and fails with `Profile(Unavailable)` regardless of
whether `test_support::ensure_test_arena_ready()` already put the process-global
`PHASE` into `READY` — not `Storage::open_managed`'s `require_ready()` check, which is
what the 9 crates and `loki-storage`'s own tests use and what `test_support` is built
to satisfy. This is a structurally different storage-access pattern (compose.rs
*owns* the full arena bootstrap/bind/shutdown lifecycle, storing the returned
`SecureMemoryRuntime` value inside `ComposedHost` for `ComposedHost::run` to shut down
later) that the `test_support::ensure_test_arena_ready()` bootstrap cannot satisfy
without either (a) a production code change to `compose.rs` (explicitly forbidden by
both the original packet and the orchestrator's own follow-up instruction: "no
production code path changes beyond what's already in
`crates/loki-runtime/src/host/compose.rs`"), or (b) a new test-only entrypoint in
`loki-storage`'s `test_support` module shaped like `bootstrap()` (returns an owned
`SecureMemoryRuntime`) rather than like `require_ready()` (checks a flag) — itself a
`crates/loki-storage/**` change, forbidden by this packet's own Section 5. I reverted
the non-working edits to `crates/loki-runtime/tests/host_lifecycle.rs` and
`crates/loki-runtime/Cargo.toml` rather than leave an ineffective patch in place, and
`crates/loki-runtime/**` is back to its exact pre-session state (confirmed via `git
status`/`git diff`). This is the packet's Section 12 stop condition, applied to the
one location where the orchestrator asked me to check it: report it specifically,
don't invent a workaround.

`cargo test --workspace` and `pnpm run verify` therefore still do not pass cleanly —
not because of any defect in the 9 crates' fixes, but because of (a) the confirmed,
reproducible `loki-runtime` gap above, and (b) a newly discovered, distinct capacity
limitation in `loki-storage`'s own `test_support` arena (1 MiB, fixed, zero-fallback
by design) that only surfaces when tests run without `--test-threads=1` — exactly
what `pnpm run verify`'s `cargo:test` step and a bare `cargo test --workspace` do.
Both are detailed in §4 with exact evidence.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `cargo test --workspace` passes | fail | `crates/loki-runtime/tests/host_lifecycle.rs` fails 2/4 with genuine `SecureMemory(Profile(Unavailable))` (not flaky — reproduced twice, same line numbers, same message); a bare run also hit the known transient `os error 4551` on the unrelated, untouched `loki-connectors-sdk` crate, persisting through one retry (§4) |
| `pnpm run verify` passes, or fails only on the documented pre-existing `openssl-sys`/Application-Control issue | fail, **new** failure category | fails at the `cargo:test` step in `loki-agent-delivery`'s lib tests with `Sqlite(SqliteFailure(OutOfMemory))`, not `openssl-sys`/Application-Control — see §4 for why this is a distinct, newly discovered gap outside this node's ownership |
| `git diff --stat` confined to `tests/**`/minimal `Cargo.toml` across exactly the 9 named crates + this handoff | pass | see §3; `crates/loki-runtime/**` reverted to original, `crates/loki-storage/**` untouched by this session |
| No test weakened, skipped, or deleted | pass | every previously-passing test in the 9 crates still exercises the same behavior; nothing deleted or marked `#[ignore]` |
| No production fail-closed path bypassed outside test code | pass | see §6 |
| Each of the 9 crates' own dedicated test command passes | pass | `cargo test -p loki-monitoring -p loki-local-notifications -p loki-local-setup -p loki-local-authority -p loki-local-devices -p loki-incidents -p loki-approvals -p loki-agents -p loki-agent-delivery --all-targets -- --test-threads=1`: every binary in every crate passed cleanly, multiple independent runs |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-monitoring/src/store.rs` | inline `#[cfg(test)] mod tests`: `ensure_test_arena_ready()` call added to `migrations_are_idempotent_across_repeated_opens_on_the_same_file` (real SqlCipher open) and to all 3 `open_in_memory`-based tests (needed because this binary's `pipeline`/`rule_store`/`store` modules share one process, and `sqlite3_config` can only run before SQLite's *first* implicit init from *any* test, in-memory or not — see §7) | yes, test-only |
| `crates/loki-monitoring/src/rule_store.rs` | same pattern: 2 real-open tests + all `open_in_memory` tests | yes, test-only |
| `crates/loki-monitoring/src/pipeline.rs` | same pattern: the one `open_in_memory` test in this module | yes, test-only |
| `crates/loki-monitoring/tests/common/mod.rs` | **new**, shared `ensure_ready()` helper for this crate's separate-process `tests/*.rs` binaries, mirroring `loki-storage/tests/common/mod.rs` | yes, test-only |
| `crates/loki-monitoring/tests/dat002_encryption.rs`, `tests/restart.rs`, `tests/rule_restart.rs` | `mod common;` + `common::ensure_ready()` at the top of every test that opens real SqlCipher storage | yes, test-only |
| `crates/loki-monitoring/Cargo.toml` | added `[dev-dependencies] loki-storage = { path = "../loki-storage", features = ["test-support"] }`, mirroring `loki-storage`'s own self-referential pattern | yes |
| `crates/loki-local-notifications/src/lib.rs` | inline tests: `ensure_test_arena_ready()` added to the 1 real-open test and all `open_in_memory` tests in the same binary | yes, test-only |
| `crates/loki-local-notifications/Cargo.toml` | same dev-dependency addition | yes |
| `crates/loki-local-setup/src/lib.rs` | `ensure_test_arena_ready()` added to the shared `fixture()` helper (used by every in-memory test) and to `restart_resumes_exact_state` (real open) | yes, test-only |
| `crates/loki-local-setup/Cargo.toml` | same dev-dependency addition | yes |
| `crates/loki-local-authority/src/store.rs` | added to shared `initialized()` helper, `concurrent_revocation_has_exactly_one_winner`, and `restart_preserves_installation_and_revocation_metadata` | yes, test-only |
| `crates/loki-local-authority/Cargo.toml` | same dev-dependency addition | yes |
| `crates/loki-local-devices/src/lib.rs` | added to shared `setup()` helper and `restart_preserves_binding_and_revocation` | yes, test-only |
| `crates/loki-local-devices/Cargo.toml` | same dev-dependency addition | yes |
| `crates/loki-incidents/src/store.rs` | added to `migrations_are_idempotent_across_repeated_opens_on_the_same_file` and all `open_in_memory` tests in this binary | yes, test-only |
| `crates/loki-incidents/tests/support/mod.rs` | **new** shared `ensure_ready()` helper for this crate's `tests/*.rs` binaries | yes, test-only |
| `crates/loki-incidents/tests/dat002_encryption.rs`, `tests/ingest_atomicity.rs` | call `ensure_ready()` at the top of the 3 tests that open real SqlCipher storage | yes, test-only |
| `crates/loki-incidents/Cargo.toml` | same dev-dependency addition | yes |
| `crates/loki-approvals/src/lib.rs` | added to shared `setup()` helper (covers all in-memory tests) and `encrypted_restart_preserves_grant_and_single_use_state` | yes, test-only |
| `crates/loki-approvals/Cargo.toml` | same dev-dependency addition | yes |
| `crates/loki-agents/src/lib.rs` | added to shared `setup()` helper and `restart_preserves_revocation` | yes, test-only |
| `crates/loki-agents/Cargo.toml` | same dev-dependency addition | yes |
| `crates/loki-agent-delivery/src/lib.rs` | added a private `ensure_ready()` helper and called it as the first statement of all 7 tests in this module (every test in this binary touches SQLite, directly or via `agent()`) | yes, test-only |
| `crates/loki-agent-delivery/Cargo.toml` | same dev-dependency addition | yes |
| `Cargo.lock` | **unchanged by this session** (still carries only the single pre-existing line from `DAT-002-MEM-CODE-001`'s own `loki-storage` self-dependency) — the 9 new `dev-dependencies` entries above did not require any new lockfile line since each crate already depended on `loki-storage` from the same path/version | n/a — no lockfile change made by this node |
| `Docs/graph/handoffs/DAT-002-MEM-CODE-001-CONSUMERS-1.md` | this handoff | yes |

`crates/loki-runtime/**` and `crates/loki-storage/**`: **not changed by this session**.
I edited `crates/loki-runtime/tests/host_lifecycle.rs` and `crates/loki-runtime/
Cargo.toml` experimentally per the orchestrator's request, confirmed the fix does not
work (§1, §4), and reverted both with `git checkout --` before finishing — `git
status`/`git diff` confirm they are byte-identical to the session's starting state.

## 4. Verification performed

All commands ran as a standard user, no elevation/VM, from
`L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001`, using PowerShell (not the bash tool's
Cygwin `perl`, which breaks `openssl-src`'s build — bash's `perl` is a different,
incompatible interpreter from the `C:\Strawberry\perl` PowerShell resolves, and using
it produces a *different* `openssl-sys` failure than the real one this project has
already documented).

| Command/scenario | Result | Evidence/reference |
|---|---|---|
| `cargo test -p loki-monitoring -p loki-local-notifications -p loki-local-setup -p loki-local-authority -p loki-local-devices -p loki-incidents -p loki-approvals -p loki-agents -p loki-agent-delivery --all-targets -- --test-threads=1` | **all pass** | run multiple times; final clean run: every one of 28 test binaries across the 9 crates reported `test result: ok`, 0 failed |
| Same command + `-p loki-runtime` | 9 crates all pass; `loki-runtime --test host_lifecycle` fails 2/4 | `simulated_crash_and_restart_preserves_persisted_device_state` and `startup_accepts_a_real_ipc_connection_and_is_located_as_healthy` both panic `host must start: SecureMemory(Profile(Unavailable))` — reproduced twice, identical message/line numbers both times (not the transient execution-block issue) |
| `cargo test --workspace -- --test-threads=1` | fails | first attempt: transient `os error 4551` (Application Control policy) on the unrelated, untouched `loki-connectors-sdk --lib` binary; retried once immediately per the orchestrator's instruction — **same binary blocked again** on the retry, so I report this as a persisting-through-one-retry instance of the documented flaky issue, not a new one (identical error text/mechanism as `Docs/graph/evidence/W4/DAT-002/local-part-one-integration.md` and `BLD-001-R2-CODE-2.md`); a separate clean-workspace-minus-loki-runtime run confirms every other crate passes, so this is not caused by this node's changes |
| `cargo clippy --workspace --all-targets -- -D warnings` | **exit 0** | clean, zero warnings, whole workspace |
| `pnpm run verify` | fails, exit 101 | prettier/eslint/typecheck/JS test/electron/`cargo:fmt:check`/`cargo:build` steps all pass; fails inside `cargo:test` (`cargo test --workspace`, invoked via `tools/scripts/cargo-verify-guard.mjs` with **no** `--test-threads=1`) at `loki-agent-delivery`'s lib tests: `tests::scoped_claim_and_owned_run_acknowledgement` and `tests::cross_connection_run_cannot_acknowledge` fail with `Sqlite(SqliteFailure(Error { code: OutOfMemory, extended_code: 7 }, ...))`, plus a real `sqlcipher_page_cipher: hmac check failed` log line from a different concurrently-running test's connection hitting a wiped/reused arena block — **this is not the pre-existing `openssl-sys` issue** (the `openssl-sys` build itself completed without error in this run; the failure is purely at `cargo test`'s own execution stage) and **is not the Application-Control transient-block issue** (the binary executed; its tests ran and produced a real, deterministic-under-contention SQLite error). See §7 for root cause: `loki-storage`'s `test_support` arena is a fixed 1 MiB, zero-fallback allocator (its own accepted design — DAT-002-MEM-CODE-001's ADR-0005 acceptance criterion "no fallback"), and `pnpm run verify`'s `cargo:test` step runs this binary's 7 tests with Rust's *default* multi-threaded test harness (no `--test-threads=1` anywhere in `tools/scripts/cargo-verify-guard.mjs` or `package.json`), so several tests now legitimately compete for that same 1 MiB arena at once and can exhaust it |

Mandatory verification not run to a fully passing state:

- `cargo test --workspace`: blocked by (a) the confirmed `loki-runtime` gap (§1) and (b) one persisting instance of the documented transient Application-Control block on an unrelated crate.
- `pnpm run verify`: blocked by the newly discovered `test_support` arena-capacity/parallelism gap in §4/§7, which is outside this node's ownership (`crates/loki-storage/**` is forbidden).

## 5. Contracts, migrations, and compatibility

- Contracts changed: none. `Storage::open_managed`/`open_or_migrate_sqlcipher`'s
  fail-closed `require_ready()` gate (added by `DAT-002-MEM-CODE-001`) is unchanged
  and unbypassed in production.
- Consumers affected: the 9 named crates' own test suites only — no other crate's
  production behavior changed.
- Migration: none; no real database touched by any change in this node.
- Backward/rolling compatibility: unaffected.
- Generated-contract drift: none.
- Build recipe/executor/provenance: unaffected; no `.cargo/config.toml` change.

## 6. Security, privacy, and device safety

- Trust boundaries touched: none in production. Every change is inside
  `#[cfg(test)]` blocks (confirmed per file — see the file-by-file `#[cfg(test)]`
  boundary check in §8) or inside `tests/**` integration-test binaries, gated behind
  the `test-support` Cargo feature that is only ever turned on by each crate's own
  `[dev-dependencies]` self-reference to `loki-storage` (never a default feature,
  never reachable from a release build or from any other consumer's `[dependencies]`
  entry).
- Authorization/approval impact: none.
- Secrets/data handling: no real key, credential, or user database touched; every
  test uses a disposable temp directory or `:memory:` connection, and the disposable
  test arena's "profile" is a hardcoded, obviously-non-production placeholder
  (`source_sha256`/`build_sha256`/`manifest_sha256` all zeroed) that never touches
  `accepted_profile()`.
- Device side effects: none.
- New dependencies: none — every `Cargo.toml` change only adds a `features =
  ["test-support"]` qualifier to an already-existing `loki-storage` path dependency,
  duplicated into `[dev-dependencies]` exactly as `loki-storage`'s own `Cargo.toml`
  already does for itself. No new crate enters any lockfile.
- Remaining risk: **critical framing, not weakened by this node** — the fail-closed
  production gate is untouched everywhere, including in `loki-runtime`, which is
  exactly why its tests still correctly fail today. The residual risk is entirely
  about test-suite completeness/CI-greenness, not about any production security
  boundary.

## 7. Decisions and assumptions

- **Order-dependency discovery (why the diff is larger than "one line per real-open
  test")**: my first pass only added `ensure_test_arena_ready()` immediately before
  each test that calls the real (SqlCipher) `open`. That was insufficient: several of
  these crates share one test *binary* per crate (inline `#[cfg(test)] mod tests` in
  `src/`, or multiple `tests/*.rs` files that share a helper module compiled into one
  binary), and Rust's default test harness runs all tests in one binary in one
  process. `sqlite3_config(SQLITE_CONFIG_MALLOC)` — which `SecureMemoryRuntime::
  bootstrap_profile` calls — can only succeed *before* SQLite's very first implicit
  initialization in that process. Several `open_in_memory()`-based tests (which never
  go through `loki_storage` at all — they call `rusqlite::Connection::open_in_memory()`
  directly) alphabetically run *before* the real-open test in the same binary,
  silently initializing SQLite's default allocator first; when the real-open test's
  `ensure_test_arena_ready()` then runs, `sqlite3_config` fails with `SQLITE_MISUSE`
  (I hit and diagnosed this exact failure — `SqliteConfiguration { code: 21 }` — while
  fixing `loki-monitoring`). The fix is to make *every* SQLite-touching test in a
  shared binary call `ensure_test_arena_ready()` first (it's a cheap, idempotent
  `Once`), not just the ones that need `require_ready()` to succeed. I applied this
  systematically to every crate, not only where I happened to reproduce the failure,
  by auditing every `open_in_memory`/`::open(` call site per shared test binary.
- **`loki-runtime` extension attempt and revert**: the orchestrator's follow-up
  message asked me to apply "the same `test_support`/disposable-arena bootstrap
  pattern" to `crates/loki-runtime/tests/host_lifecycle.rs`. I did so, ran the tests
  for real, found it does not fix the failure, root-caused why (§1), and reverted
  rather than leave a non-functional patch masquerading as a fix. I did not invent an
  alternative (e.g. touching `compose.rs`) because both the original packet and the
  orchestrator's own message explicitly forbid production changes there, and a
  `loki-storage`-side test-support addition is outside this packet's Section 5
  ownership. This is a genuine "structurally different storage-access pattern" per
  the packet's own Section 12 stop-condition language, discovered empirically rather
  than assumed.
- **Application-Control transient-execution-block handling**: I treated `os error
  4551` (`An Application Control policy has blocked this file`) as the same
  pre-existing, documented, non-deterministic issue this project has already recorded
  (`Docs/graph/evidence/W4/DAT-002/local-part-one-integration.md`,
  `Docs/graph/handoffs/BLD-001-R2-CODE-2.md`) whenever it blocked a *freshly linked*
  binary that later ran clean on retry with no source change — I reproduced this
  exact pattern repeatedly across many different, unrelated binaries (never the same
  binary content failing twice after a successful retry, with one exception: one
  particular `loki-approvals` build fingerprint from a specific multi-`-p` invocation
  stayed blocked across 3 retries before finally clearing on the 4th attempt after I
  deleted the stale `.exe` and forced a relink — still consistent with "an
  unrecognized binary hash needs to clear a host security check", just an unusually
  slow instance of the same mechanism, not a different one).
- **`pnpm run verify`'s `cargo:test` OutOfMemory failure is a new, distinct finding,
  not the openssl-sys issue**: I explicitly checked build output for `perl`/
  `openssl-src` errors (none present — the build completed) and for `os error 4551`
  (not present in this failure) before concluding this is a different, previously
  undocumented gap: `loki-storage`'s `test_support` arena is fixed at 1 MiB with
  explicit zero-fallback semantics (an accepted `DAT-002-MEM-CODE-001`/ADR-0005
  design point, not a bug), and `pnpm run verify` never passes `--test-threads=1`
  anywhere in its script chain, so any test binary with enough concurrently-running
  SqlCipher-backed tests sharing that one process-global arena can legitimately
  exhaust it under real parallel execution. I could not fix this within my ownership
  (the arena's size/fallback policy lives in `crates/loki-storage/src/secure_memory.rs`,
  forbidden to me) or by touching `pnpm`/`cargo-verify-guard.mjs` (outside this
  packet's scope and not something I was asked to change).
- I left `.tmp/corepack-shims` (a local pnpm shim, not a deliverable) and deleted
  several stray, non-gitignored `target-consumers/`, `target-evidence/`, and
  `.tmp-*.log` directories/files this session created or found left over from a prior
  session's own verification runs — none are deliverables, all were blocking
  `pnpm run verify`'s prettier/eslint steps exactly as the parent node's handoff
  already documented as the correct thing to do with such artifacts.

## 8. Review focus

- Confirm, file by file, that every change in §3 is genuinely inside a `#[cfg(test)]`
  block (for the crates with inline tests) or under `tests/**` (for the crates with
  separate integration-test binaries) — I verified this myself with `awk`/`grep`
  against every touched `src/*.rs` file and found the first `#[cfg(test)]` boundary
  precedes every edit in all 11 files, but an independent check is still warranted
  given the number of files.
- Whether `loki-runtime/tests/host_lifecycle.rs`'s gap should become its own graph
  node (needs a `loki-storage`-side test-support extension shaped like `bootstrap()`,
  or a `compose.rs` refactor to accept an already-bootstrapped `SecureMemoryRuntime`)
  or be accepted as a known, documented gap until `DAT-002-MEM-HOST-VERIFY-001` lands
  a real accepted profile (at which point `ComposedHost::open` would succeed in every
  environment, test or production, without any test-mode bootstrap at all).
- Whether `test_support`'s 1 MiB fixed arena size should be revisited (increased, or
  given a test-only larger disposable profile) now that 9-10 crates' worth of tests
  legitimately want to open concurrent SqlCipher connections against it under
  `pnpm run verify`'s real (parallel) execution mode — this is `loki-storage`-owned
  scope, not something I could decide or implement here.
- The unusually persistent (3-retry) `loki-approvals` execution-block instance in
  §7 — worth flagging to whoever owns the host's Application Control policy, even
  though it did eventually clear.

## 9. Follow-up graph changes proposed

- A new node (or an accepted amendment to this one) to resolve
  `crates/loki-runtime/tests/host_lifecycle.rs`'s gap — needs an explicit design
  decision (test-only `bootstrap()`-shaped entrypoint in `loki-storage`'s
  `test_support`, vs. a `compose.rs` refactor, vs. accepting the gap until
  `DAT-002-MEM-HOST-VERIFY-001`) before any worker attempts it, since I've now shown
  the naive `require_ready()`-style pattern does not apply here.
- A decision on whether `test_support`'s 1 MiB arena size is adequate for
  workspace-wide parallel `cargo test`/`pnpm run verify` execution, or whether a
  larger test-only profile (or a documented `--test-threads=1` requirement enforced
  in `tools/scripts/cargo-verify-guard.mjs`) is needed — this blocks `pnpm run
  verify` from ever passing cleanly workspace-wide as more crates adopt the same
  test-support pattern, independent of this node's own 9-crate scope.
- Confirmation of whether the Application Control transient-execution-block issue
  (§4/§7) merits its own environment-remediation graph item, as `BLD-001-R2-CODE`'s
  own handoff already proposed for the build-time variant of the same class of
  issue.

Only the orchestrator updates the task graph.
