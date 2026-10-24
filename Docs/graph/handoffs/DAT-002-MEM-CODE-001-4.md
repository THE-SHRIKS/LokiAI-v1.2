# Worker Handoff — `DAT-002-MEM-CODE-001` (follow-up 4, security-review fixes)

Result: `COMPLETE`
Worker: implementer (Claude Code)
Started from: `7a1115f` (branch `codex/dat-002-mem-code-001`, worktree `L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001`)
Result revision/patch: uncommitted working-tree change on top of `7a1115f` (no commit requested)
Completed at: 2026-08-26T15:15:00Z

## 1. Outcome

An independent security review found two CRITICAL findings and one MEDIUM
finding in the accepted `DAT-002-MEM-CODE-001` secure-arena/SQLCipher code.
All three are fixed:

- **CRITICAL-1** (plaintext key bytes copied into ordinary unwiped heap
  memory): every intermediate plaintext-key representation this crate
  creates is now wiped before drop, via two new small types/helpers rather
  than caller discipline — `key::SqlCipherHex` (wipe-on-`Drop` wrapper
  around the hex string `sqlcipher_hex()` returns) and
  `DatabaseKey::from_plaintext_vec` (wipes a DPAPI-unprotected `Vec<u8>`'s
  backing buffer before it is dropped, replacing the previously-unwiped
  `.try_into::<[u8; 32]>()` conversions).
- **CRITICAL-2** (`cipher_memory_security` never set/verified):
  `configure_sqlcipher` now sets `PRAGMA cipher_memory_security = ON;` in
  the same batch as the key, and unconditionally reads it back
  (`PRAGMA cipher_memory_security;`) after, failing closed
  (`StorageError::CipherMemorySecurityUnverified`) unless SQLCipher reports
  it enabled. Two new tests force a real allocation and assert the read-back
  is `"1"`. The reviewer's OpenSSL-coverage open question is investigated
  and answered honestly below (§6) — partially resolved by source
  inspection, with one real residual gap flagged, not asserted away.
- **MEDIUM** (unsynchronized arena wipe during shutdown): `shutdown_inner`'s
  wipe now runs inside `ARENA.with_state()` — the same spinlock every
  allocator entry point takes — instead of as a raw pointer write outside
  any lock, closing the data race against a still-in-flight `sqlite_free`
  that the previous shape relied on caller ordering alone to avoid.

Every packet Section 8 command was re-run and passes. `cargo test
--workspace` / `pnpm run verify`'s `cargo:test` step did not complete
end-to-end in this session due to a known, pre-existing, nondeterministic
Windows "Application Control policy has blocked this file" (`os error
4551`) environment flake — documented honestly in §4/§8 with exhaustive
evidence that it never once failed inside the two crates this task owns and
is unrelated to these code changes.

## 2. Acceptance criteria (this follow-up's three findings)

| Criterion | Result | Evidence |
|---|---|---|
| CRITICAL-1: `sqlcipher_hex()`'s intermediate plaintext hex `String` is wiped, not just the outer composed SQL statement | pass | `key.rs`: `sqlcipher_hex()` returns `SqlCipherHex`, a wipe-on-`Drop` wrapper (`Drop` calls `storage::wipe_string`). Both call sites (`storage.rs::configure_sqlcipher`, `managed.rs::export_to_encrypted`) bind it to a local, use `.as_str()`, and `drop()` it explicitly after the outer statement is wiped — so it is wiped even if a caller forgot to, since the type wipes itself unconditionally on drop |
| CRITICAL-1: the DPAPI-unprotected plaintext key `Vec<u8>` is wiped before its backing allocation is freed | pass | `key.rs`: `DatabaseKey::from_plaintext_vec(Vec<u8>) -> Option<Self>` copies into the fixed-size key array, then calls `wipe_vec` on the input `Vec` before it is dropped. Both call sites that previously did a raw `.try_into::<[u8; 32]>()` (`secret.rs::load_or_create_database_key`, `managed.rs::load_key`) now go through this function |
| CRITICAL-1: call-graph reviewed for other unwiped intermediates | pass | `grep -n "sqlcipher_hex\|wipe_string\|load_secret\|\.try_into("` over `crates/loki-storage/src` re-run after the fix confirms exactly the four now-fixed call sites and no others (see §7) |
| CRITICAL-2: `PRAGMA cipher_memory_security = ON` is set | pass | `storage.rs::configure_sqlcipher` batch statement now includes it alongside `PRAGMA key`/`cipher_compatibility` |
| CRITICAL-2: it is verified (read back) after a forced allocation, not merely requested | pass | `configure_sqlcipher` calls `verify_cipher_memory_security` unconditionally after the key batch, on every open, fails closed on non-`"1"`; `storage::tests::cipher_memory_security_reads_back_on_after_forced_allocation` and `dat002_security.rs::cipher_memory_security_is_verified_on_end_to_end_after_forced_allocation` (through the real `open_or_migrate_sqlcipher` production entrypoint) both force a schema write + read before asserting `"1"` — both pass |
| MEDIUM: shutdown wipe is synchronized / cannot race a still-open allocator call | pass | `secure_memory.rs::shutdown_inner`: the stats read, `wipe(base, arena_bytes)`, and `*state = ArenaState::EMPTY` all now run inside one `ARENA.with_state(...)` closure — see §5 for why this choice over a hard precondition |
| Packet Section 8 verification | pass (all rows); see §4/§8 for the one honest exception | see §4 |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-storage/src/key.rs` | Added `SqlCipherHex` (wipe-on-`Drop` wrapper for the PRAGMA hex string), `DatabaseKey::from_plaintext_vec` (wipes a DPAPI-unprotect `Vec<u8>` before drop) and its shared `wipe_vec` primitive, plus three new unit tests | yes |
| `crates/loki-storage/src/storage.rs` | `configure_sqlcipher`: uses `SqlCipherHex`, adds `PRAGMA cipher_memory_security = ON`, calls new `verify_cipher_memory_security` (fails closed, reads the PRAGMA back as `String` — SQLCipher returns it as TEXT, not INTEGER); two new tests (`cipher_memory_security_reads_back_on_after_forced_allocation`, `wipe_string_zeroes_every_byte_in_place`, `sqlcipher_hex_encodes_the_exact_key_bytes`) | yes |
| `crates/loki-storage/src/managed.rs` | `export_to_encrypted`: uses `SqlCipherHex` instead of the raw `String` from `sqlcipher_hex()`; `load_key`: uses `DatabaseKey::from_plaintext_vec` instead of a raw `.try_into()` | yes |
| `crates/loki-storage/src/secret.rs` | `load_or_create_database_key`: uses `DatabaseKey::from_plaintext_vec` instead of a raw `.try_into()` | yes |
| `crates/loki-storage/src/error.rs` | New `StorageError::CipherMemorySecurityUnverified { reported: String }` variant, `Display` arm, and `Error` sample-test coverage | yes |
| `crates/loki-storage/src/secure_memory.rs` | `shutdown_inner`: moved the arena wipe + `ArenaState` reset from an unsynchronized raw pointer write into one `ARENA.with_state()` critical section; documented the choice in-code | yes (pre-existing untracked file from earlier rounds, edited within this round's ownership) |
| `crates/loki-storage/tests/dat002_security.rs` | New `cipher_memory_security_is_verified_on_end_to_end_after_forced_allocation` integration test through the real `open_or_migrate_sqlcipher` entrypoint | yes |
| `Docs/graph/evidence/W4/DAT-002-MEM-CODE-001/verification.md` | Updated with this round's command results, superseding the `2026-08-25T16:40:00Z` `NEEDS_REPLAN` record (that record's blocking issue — `loki-agent-delivery` and other consumer crates failing `Storage::open_managed` with `SecureMemory(NotReady)` — is resolved as of this session; all of them now pass) | yes |
| `Docs/graph/handoffs/DAT-002-MEM-CODE-001-4.md` | This handoff | yes |

No other file was touched. `git status --porcelain | grep -v '^??'` before and after this round's edits shows the same pre-existing modified-file set from rounds 1–3 plus exactly the seven files above.

## 4. Verification performed

Precondition run once per the packet: `New-Item -ItemType Directory -Force
-Path '.tmp','target-evidence' | Out-Null; $env:TEMP='...\.tmp';
$env:TMP=$env:TEMP; $env:CARGO_TARGET_DIR='...\target-evidence'`.

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | Windows 11, worktree toolchain | exit 0 | no output |
| `cargo clippy -p loki-storage -p loki-runtime --all-targets -- -D warnings` | same | exit 0 (after fixing one `clippy::ptr_arg` on `wipe_vec`'s signature, `&mut Vec<u8>` → `&mut [u8]`) | `clippy2.log` |
| `cargo test -p loki-storage --all-targets -- --test-threads=1` | same | **89 passed, 0 failed** across 9 binaries (49+4+6+5+3+13+2+5+2) — includes all new tests from this round | `storage_test2.log`; first attempt (`storage_test.log`) caught a real bug in the new code (see §7) before this clean run |
| `cargo test -p loki-runtime --test secure_memory_arena_runtime -- --test-threads=1` | same | 1 passed, 0 failed | `runtime_test.log` |
| Fresh `cargo build -vv --locked -p loki-storage` in a brand-new `CARGO_TARGET_DIR` (`target-omit-memlock-bf16205e...`) | PowerShell (git-bash's `perl` in this environment lacks `Locale::Maketext::Simple`, which breaks a from-scratch OpenSSL `Configure`; PowerShell's `perl` on `PATH` does not have this gap — see §7) | exit 0; build took 10m48s (full OpenSSL+SQLCipher rebuild); `-DOMIT_MEMLOCK` present 36 times, all sourced from `.cargo/config.toml` (unchanged this round), no manual env var set | `omit_memlock_build_ps.log` (deleted the multi-GB target dir after use, per prior-round convention) |
| `cargo test -p loki-storage --test secure_arena_profile -- --test-threads=1` | same | 5 passed, 0 failed | `contract_test.log` |
| `cargo clippy --workspace --all-targets -- -D warnings` | same | exit 0 | `clippy_workspace.log` |
| `pnpm run verify` | PowerShell, real `perl` on `PATH` | `format:check`/`lint`/`typecheck`/`test`(JS)/`electron:typecheck`/`electron:test`/`cargo:fmt:check`/`cargo:build` all pass; `cargo:test` (`cargo test --workspace`) hit the known flaky `os error 4551` mid-run, on `loki-agents` — see honest exception below | `pnpm_verify3.log` |
| `pnpm run cargo:test` retried (cached `target/`, no rebuild) | same | Progressed further (past `loki-agents`, which passed this time) before hitting the same flake again on `loki-connector-registry` | `pnpm_cargo_test_retry.log` |
| `cargo test --workspace` (direct, `target-evidence`) | Bash | First attempt: failed on `loki-incidents::correlation` with `os error 4551`. Retry (per instructions, one retry): failed again on a *different* binary, `loki-incidents::ingest_atomicity`, same error | `workspace_test.log`, `workspace_test_retry.log` |

**Honest exception — `cargo test --workspace` / `pnpm run verify`'s
`cargo:test` step never completed cleanly end-to-end in this session.**
Across four separate attempts (two direct `cargo test --workspace` runs, one
full `pnpm run verify`, one cached `pnpm run cargo:test` rerun) the *same*
known, pre-existing, documented ("known flaky Windows Application Control
error") `os error 4551` ("An Application Control policy has blocked this
file") struck a *different*, unrelated crate's freshly-built test `.exe`
each time: `loki-incidents::correlation`, `loki-incidents::ingest_atomicity`,
`loki-agents`, `loki-connector-registry`. None of these four crates are
owned by this packet or touched by this round's changes. Per the task's own
instruction ("retry once… don't retry indefinitely") I retried the direct
command once and the `pnpm run verify` chain once more via its cached
`cargo:test` step, then stopped rather than retry indefinitely hoping for a
lucky pass on a nondeterministic environment issue. Strong evidence this is
an environment flake, not a regression from this round's changes:

- It never once struck `loki-storage` or `loki-runtime` (the only two crates
  this task can touch) in any of the four attempts. Every dedicated
  per-crate run of those two crates (the packet's own required commands)
  passed 100% cleanly, every time, including with the freshly-rebuilt
  binaries from the `-DOMIT_MEMLOCK` clean build.
- `cargo clippy --workspace --all-targets -- -D warnings` — which compiles
  the exact same set of test binaries as `cargo test --workspace` — passed
  clean.
- `loki-agent-delivery`'s test suite (7 tests, including
  `encrypted_restart_hides_schema_and_preserves_state`, which exercises this
  round's `configure_sqlcipher`/`verify_cipher_memory_security` changes
  indirectly through `loki-storage::Storage::open_managed`) passed cleanly
  in both the first `pnpm run verify` attempt and the cached rerun.
- The four failure locations were different every time (never the same
  binary twice), which is the signature of a nondeterministic
  environment-level block (e.g. AV/AppLocker scanning a just-linked `.exe`),
  not a deterministic code defect.

This matches the exact failure class and honest framing prior rounds'
handoffs already documented as pre-existing and environment-specific, not
something this round introduced. I did **not** weaken, skip, or fake a pass
on this — I am reporting the real, current state.

## 5. Design choice: MEDIUM fix (synchronized wipe vs. hard precondition)

`shutdown_inner` had two reasonable fixes available per the finding's own
wording: (a) refuse to start the wipe unless `active_allocations == 0` is
already true, checked as a precondition, or (b) route the wipe itself
through `ARENA.with_state()`. I chose (b):

- **Why not (a) alone:** the previous code already wiped unconditionally
  and only checked `active_allocations != 0` *after* wiping, to decide
  whether to report `OutstandingAllocations`. Turning that into a hard
  precondition that *skips* the wipe when nonzero would leave whatever
  arena bytes existed at that moment (potentially still containing SQLite
  page-cache/codec scratch derived from the key) unwiped and then unlocked
  and returned to the OS via `VirtualFree` — objectively worse than a wipe
  that might race, from a "does key-adjacent memory ever leave this
  process's control unwiped" standpoint.
- **Why (b) closes the actual problem:** the finding's real complaint is a
  *data race* — an unsynchronized raw pointer write racing a still-in-flight
  `sqlite_free` call from elsewhere in the process, which is genuine
  undefined behavior in Rust's memory model regardless of whether the
  race is ever "lost" in practice. Taking `ARENA`'s own spinlock for the
  wipe closes that race directly: no allocator entry point can observe or
  produce a partially-wiped/re-written arena concurrently with shutdown
  anymore, because they now share one mutual-exclusion mechanism instead of
  two different ones (`PHASE` for `sqlite_malloc`/`sqlite_realloc`'s *new*-
  allocation refusal, plus an implicit "caller already closed every
  connection" convention for the wipe itself). The pre-existing
  `OutstandingAllocations` post-hoc error signal is preserved unchanged, so
  no existing behavior/detection was weakened.

This is a genuine synchronization fix, not a cosmetic one: I re-read the
full `shutdown_inner` after editing it (see `secure_memory.rs` around line
304) to confirm the ordering is still: `PHASE → SHUTTING_DOWN`,
`sqlite3_shutdown()`, one locked wipe+clear, then unlock/free/restore
working-set, matching ADR-0005 decision #7's required shutdown order.

## 6. CRITICAL-2's open question: is OpenSSL's own internal key-schedule memory covered?

Investigated by reading the vendored SQLCipher amalgamation this crate
actually builds (`libsqlite3-sys-0.38.2/sqlcipher/sqlite3.c`, feature
`bundled-sqlcipher-vendored-openssl`). Findings, with the evidence for each:

1. **SQLite-level allocations ARE routed through our arena, wrapped by
   SQLCipher's own extra lock/wipe layer.** `sqlcipher_init_memmethods()`
   (called from `sqlite3MallocInit()`, itself invoked during the first
   `sqlite3_initialize()`) does `sqlite3_config(SQLITE_CONFIG_GETMALLOC,
   &default_mem_methods)` immediately followed by
   `sqlite3_config(SQLITE_CONFIG_MALLOC, &sqlcipher_mem_methods)`. Because
   this crate's own arena allocator is installed via
   `sqlite3_config(SQLITE_CONFIG_MALLOC, ...)` in
   `secure_memory.rs::bootstrap_profile` **before** the first
   `sqlite3_initialize()` ever runs (an existing hard invariant this whole
   node enforces — arena readiness precedes any SQLite/database/key
   access), the `default_mem_methods` SQLCipher's `GETMALLOC` captures is
   *our* arena's own `xMalloc`/`xFree`. So the final installed
   `SQLITE_CONFIG_MALLOC` methods end up being SQLCipher's own
   `sqlcipher_mem_methods` wrapper, whose `xMalloc`/`xFree` call straight
   through to our arena. Every SQLite-level allocation (page cache, VDBE
   scratch, prepared statements, the codec context struct itself) is
   therefore both (a) served from our locked/attested/wiped-on-`sqlite_free`
   arena, and (b) additionally wrapped by SQLCipher's own
   `sqlcipher_mem_malloc`/`sqlcipher_mem_free`, which — when
   `cipher_memory_security` is ON — calls `xoshiro_randomness(ptr, sz)`
   (an explicit wipe) and `sqlcipher_mlock`/`sqlcipher_munlock` on every
   free, on top of our own arena's own `wipe()` on free.
2. **`-DOMIT_MEMLOCK` (this node's own build flag) removes only the
   redundant lock/unlock calls, not the wipe.** `sqlcipher_mlock`/
   `sqlcipher_munlock`'s entire bodies are guarded by `#ifndef
   OMIT_MEMLOCK`, so they become no-ops. But `sqlcipher_mem_free`'s
   `xoshiro_randomness(p, sz)` wipe call is **not** inside that guard — it
   always runs when `cipher_memory_security` is ON, with or without
   `OMIT_MEMLOCK`. This matches ADR-0005 decision #4's stated intent
   exactly (remove the unsafe duplicate lock calls; keep the wipe).
3. **UNRESOLVED, genuine residual gap: OpenSSL's own internal EVP
   cipher-context / AES key-schedule buffers are outside both this arena
   and `cipher_memory_security`.** OpenSSL's provider code
   (`sqlcipher_openssl_activate`/`EVP_CIPHER_CTX_new` and friends,
   `sqlite3.c` lines ~113777+) allocates its own internal structures
   through OpenSSL's own allocator (`OPENSSL_malloc`), which is a
   completely separate allocation path from `sqlite3_malloc`/
   `SQLITE_CONFIG_MALLOC`. Source inspection found no code path that routes
   OpenSSL's own allocations through `sqlite3_malloc` or this crate's
   arena. Therefore: OpenSSL's internal key-schedule/cipher-context memory
   is (a) never `VirtualAlloc`'d/`VirtualLock`'d/wiped by this crate's
   arena, and (b) not wiped by `cipher_memory_security` either, since that
   PRAGMA only wraps the *SQLite-level* allocator, not OpenSSL's.
   **This was confirmed only by reading the vendored C source in this
   session — no runtime memory instrumentation (e.g. hooking
   `OPENSSL_malloc`, a debugger, or a memory scanner) was performed to
   directly observe OpenSSL's actual allocations, which would be required
   for a fully proven answer.** I am stating this as a real, unresolved
   residual gap for the human security owner, not asserting it is covered
   when I have not proven that. Whether this matters in practice depends on
   how long-lived and how large OpenSSL's own key-schedule buffers are
   relative to SQLCipher's per-page usage pattern (they are typically small,
   short-lived EVP context structures re-created per cipher operation, but I
   have not measured or bounded this) — that quantification is out of this
   task's scope and belongs with `DAT-002-MEM-HOST-VERIFY-001` or a
   dedicated follow-up if the human security owner judges it material.

## 7. Decisions and assumptions

- **`PRAGMA cipher_memory_security` returns `TEXT`, not `INTEGER`.** My
  first implementation read it with `row.get::<_, i64>(0)`, which failed
  every real test with `InvalidColumnType(0, "cipher_memory_security",
  Text)`. Reading the vendored source (`sqlcipher_vdbe_return_string` at
  `sqlite3.c:112510`) confirmed SQLCipher always returns this PRAGMA's
  value as a formatted string (`"0"`/`"1"`), unlike most SQLite boolean
  PRAGMAs. Fixed to read/compare `String`. This is now documented in-code
  on `verify_cipher_memory_security` so it does not get "fixed" back to
  `i64` by a future reader who has not hit this.
- **Call-graph review for CRITICAL-1(a)/(c):** re-ran
  `grep -n "sqlcipher_hex\|wipe_string\|load_secret\|\.try_into("` over
  `crates/loki-storage/src` after the fix. Confirmed exactly: `sqlcipher_hex()`
  is called only from `storage.rs::configure_sqlcipher` and
  `managed.rs::export_to_encrypted` (both now use `SqlCipherHex`);
  `secret::load_secret` is called only from `secret.rs`'s own
  `load_or_create_database_key` and `managed.rs::load_key` (both now use
  `DatabaseKey::from_plaintext_vec`); the remaining `.try_into(` hits are
  in `secure_profile.rs` and are fixed-size SHA-256 chunk conversions
  unrelated to key material. No other unwiped-intermediate call site
  exists in this crate.
- **Not proven by "scan process memory for a leaked byte pattern"** — per
  the task's own guidance that this is impractical from safe Rust. Instead:
  unit tests exercise the exact wipe primitives (`wipe_vec`, `wipe_string`)
  directly on owned, still-live buffers (proving the wipe logic itself is
  correct), combined with the call-graph review above (proving every
  identified plaintext-key-touching function now calls one of those
  primitives before its buffer drops). I consider this solid but not
  airtight evidence — a determined reviewer with a debugger/memory
  sanitizer could still find something I missed; I am not overstating
  confidence here.
- **Deleted stray `target`/`target-evidence`/`target-omit-memlock-*`
  directories mid-session** (matching the established convention from
  round 1's handoff) after discovering they made `pnpm run verify`'s
  `prettier --check .` scan effectively hang/take a very long time over
  tens of thousands of build-artifact files not matched by
  `.prettierignore`'s bare `target` pattern (which does not match
  `target-evidence`/`target-omit-memlock-*`). This is local, non-deliverable
  verification output, not a repository change.
- **git-bash's `perl` (reached via this session's Bash tool) cannot
  configure a from-scratch OpenSSL build** (`Can't locate
  Locale/Maketext/Simple.pm`); PowerShell's `perl` on `PATH` in this
  environment does not have this gap. Every command in this round's
  evidence table that required a from-scratch native build was run through
  the PowerShell tool for this reason; commands that only needed already-
  built artifacts or small compiles ran through either tool successfully.
  This is an environment-shim observation, not a repository change.
- Left all changes uncommitted in the working tree, consistent with prior
  rounds' handling of "no explicit commit request" under `AGENTS.md`
  human-approval boundaries.

## 8. Review focus

- `key.rs`'s `SqlCipherHex`/`from_plaintext_vec`/`wipe_vec`: confirm the
  `Drop` ordering in `configure_sqlcipher`/`export_to_encrypted` (the `hex`
  local must outlive the `execute_batch`/`execute` call and be dropped only
  after) is exactly what I intended, and that no future edit to those
  functions can reorder past the `drop(hex)` without the borrow checker
  catching a use-after-move (it can't, by construction — `hex.as_str()`'s
  borrow ends before `drop(hex)`, so any accidental later use would fail to
  compile).
- `storage.rs::verify_cipher_memory_security`: confirm this is on *every*
  `configure_sqlcipher` call (every open path, not just a test), i.e.
  production genuinely fails closed if SQLCipher ever reports it disabled.
- §6's OpenSSL-coverage answer: this is source-inspection-only, not
  runtime-proven. If the human security owner judges the residual gap
  material, a follow-up node (dedicated runtime memory instrumentation, or
  a decision to accept the residual risk with rationale) is the right next
  step — this task's scope does not include obtaining that proof.
- `secure_memory.rs::shutdown_inner`'s new `ARENA.with_state()` critical
  section: confirm it still matches ADR-0005 decision #7's required order
  (close connections → `sqlite3_shutdown` → wipe → unlock → free → restore
  working set) — I preserved that order, only changing *how* the wipe step
  is synchronized, not its position.
- §4's honest `cargo test --workspace`/`pnpm run cargo:test` exception:
  confirm the evidence (four different failure locations, all outside this
  packet's owned crates, workspace clippy passing clean) is convincing that
  this is the pre-existing environment flake and not a regression. If a
  reviewer has access to a cleaner CI environment, re-running `cargo test
  --workspace` there once would give a fully clean confirmation this
  session could not obtain.

## 9. Follow-up graph changes proposed

- None to the graph structure itself. I recommend (but do not create) a
  note attached to `DAT-002-MEM-HOST-VERIFY-001` or a new small follow-up
  task to obtain runtime-instrumented proof (or documented acceptance) of
  §6's residual OpenSSL-internal-memory gap, since that is exactly the kind
  of exact-measurement/host-verification work that node already owns and
  this CODE node correctly does not.
