# DAT-002-MEM-CODE-001 verification

Recorded at: `2026-08-26T15:15:00Z`
Worktree: `L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001`
Result: `COMPLETE`

This replaces the prior `2026-08-25T16:40:00Z` `NEEDS_REPLAN` record (kept
below for history). See `Docs/graph/handoffs/DAT-002-MEM-CODE-001-4.md` for
full detail; this file is a compact index of commands actually run this
session, which fixed three independent-security-review findings
(CRITICAL-1: unwiped plaintext-key intermediates; CRITICAL-2:
`cipher_memory_security` never set/verified; MEDIUM: unsynchronized
shutdown wipe).

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy -p loki-storage -p loki-runtime --all-targets -- -D warnings` | exit 0 |
| `cargo test -p loki-storage --all-targets -- --test-threads=1` | 89 passed, 0 failed, across 9 binaries |
| `cargo test -p loki-runtime --test secure_memory_arena_runtime -- --test-threads=1` | 1 passed |
| fresh `cargo build -vv --locked -p loki-storage` (new `CARGO_TARGET_DIR`) | exit 0; `-DOMIT_MEMLOCK` confirmed 36 times, from `.cargo/config.toml` only |
| `cargo test -p loki-storage --test secure_arena_profile -- --test-threads=1` | 5 passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `pnpm run verify` | `format:check`/`lint`/`typecheck`/`test`/`electron:typecheck`/`electron:test`/`cargo:fmt:check`/`cargo:build` all pass; `cargo:test` step hit the known flaky Windows `os error 4551` — see `DAT-002-MEM-CODE-001-4.md` §4 for full honest evidence that this is a pre-existing environment flake unrelated to this task's crates |

The `NotReady`/consumer-crate blocker this file previously recorded as the
reason for `NEEDS_REPLAN` is resolved: `loki-agent-delivery` and every other
previously-broken consumer crate now pass their real `Storage::open_managed`
test paths (confirmed in this session's `pnpm run verify` and
`pnpm run cargo:test` logs).

---

## Prior record (`2026-08-25T16:40:00Z`, `NEEDS_REPLAN` — superseded)

This replaces the prior session's `2026-08-24T18:35:58Z` record. See
`Docs/graph/handoffs/DAT-002-MEM-CODE-001-1.md` for full detail; this file is
a compact index of commands actually run this session.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy -p loki-storage -p loki-runtime --all-targets -- -D warnings` | exit 0 |
| `cargo test -p loki-storage --all-targets -- --test-threads=1` | 82 passed, 0 failed, across 9 binaries |
| `cargo test -p loki-runtime --test secure_memory_arena_runtime -- --test-threads=1` | 1 passed |
| fresh `cargo build -vv --locked -p loki-storage` (new `CARGO_TARGET_DIR`) | exit 0; `-DOMIT_MEMLOCK` confirmed from `.cargo/config.toml` only |
| `cargo test -p loki-storage --test secure_arena_profile -- --test-threads=1` | 5 passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `pnpm run verify` | **exit 101** — fails in `loki-agent-delivery`'s test suite (`Storage(SecureMemory(NotReady))`), a crate outside this packet's ownership that also calls `Storage::open_managed` |

### Why this was `NEEDS_REPLAN`, not `COMPLETE`, at the time

The `require_ready()` fail-closed gate (ADR-0005, correctly implemented) makes
every SqlCipher-backed `Storage::open*` fail until a measured profile is
accepted — by design. But at least 9 other already-existing crates
(`loki-monitoring`, `loki-local-notifications`, `loki-local-setup`,
`loki-local-authority`, `loki-local-devices`, `loki-incidents`,
`loki-approvals`, `loki-agents`, `loki-agent-delivery`) call
`Storage::open_managed`/`Cipher::SqlCipher` directly and are broken by the
same change, and are outside this packet's writable ownership
(`crates/loki-storage/**` and `crates/loki-runtime/src/host/compose.rs`
only). `pnpm run verify` — this packet's own mandatory Section 8 row — runs
`cargo test --workspace` and therefore cannot pass until those crates are
also updated or the workspace-wide verification scope for this node is
explicitly narrowed by the orchestrator.

One additional flagged item: `Cargo.lock` gained one mechanical line
(`loki-storage` listed as its own dev-dependency), an automatic consequence
of the `test-support` Cargo feature added to fix pre-existing storage tests
that the new gate broke. The packet lists `Cargo.lock` as read-only; this is
called out explicitly in the handoff for orchestrator decision rather than
silently accepted.
