# Task Packet — `DAT-002-MEM-CODE-001 Verified SQLCipher Secure Arena Code`

Packet status: `accepted`  
Graph status: `ready`  
Created by: `Codex orchestrator`  
Created at: `2026-08-24T18:10:00Z`  
Starting revision: `0bd4a7041f3d01c5b419feccc0d48c3198917ac4`

## 1. Objective

Implement ADR-0005's fixed, page-aligned, fail-closed SQLite secure arena and
its repository-controlled SQLCipher build profile. This code node must make
storage unavailable until a measured production bounds profile is integrated;
it cannot claim the separately gated supported-host, packaging, signing, SBOM,
performance, or independent-review evidence.

## 2. Traceability

- Requirements: `NFR-DATA-002`, `NFR-SEC-002`, `NFR-REL-001`; scenario `AS-016`
- Architecture: runtime-owned encrypted persistence and startup/shutdown composition
- Security: SQLCipher, DPAPI, verified page locks, no-fallback allocation
- Parent: DAT-002 correction re-plan; decision: ADR-0005

## 3. Dependencies and inputs

- `depends_on`: `DAT-002-MEM-ADR-001`, `DAT-001`
- Consumes ADR-0005 at `0bd4a7041f3d01c5b419feccc0d48c3198917ac4`
- Failed DAT correction/review artifacts are defect evidence, not predecessors
- Standard-user Windows; worktree, data, `TEMP`, `TMP`, Cargo target, and evidence
  must remain under `L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001`
- Existing pinned Rust/MSVC/SQLCipher toolchain only

## 4. Required reading

- `AGENTS.md`; `Docs/ARCHITECTURE.md`; `Docs/SECURITY.md`; `Docs/TESTING.md`
- ADR-0001, ADR-0005, all DAT-002 packets/handoffs/security reviews
- `crates/loki-storage/**`, runtime composition, pinned `libsqlite3-sys` build source

## 5. Ownership

Writable paths:

- `.cargo/config.toml`, limited to `[env] LIBSQLITE3_FLAGS = { value =
  "-DOMIT_MEMLOCK", force = true }` for every direct Cargo build
- `crates/loki-storage/**`
- `crates/loki-runtime/src/host/compose.rs` and a focused new host secure-memory module
- focused storage/runtime tests
- `Docs/graph/handoffs/DAT-002-MEM-CODE-001-1.md`
- `Docs/graph/evidence/W4/DAT-002-MEM-CODE-001/**`

Read-only: root `Cargo.toml`, `Cargo.lock`, other crates, accepted documents.

Forbidden: vendored SQLCipher patching; disabling `cipher_memory_security`;
fallback/growth/silent degradation; guessed production bounds; real user data;
elevation or VM execution; persistent/global working-set policy; C-drive/system
paths; Windows account/group/UAC/registry/admin/host-policy changes; device access.

## 6. Deliverables

1. Fixed page-aligned arena and explicit lifecycle state machine.
2. Current-process-only bounded/nonpersistent working-set adjustment and restore.
3. `VirtualLock` plus per-page `QueryWorkingSetEx` Locked-bit attestation.
4. Strict SQLite allocator with zero fallback/growth and `SQLITE_NOMEM` exhaustion.
5. Forced repository build config and clean-build evidence for `-DOMIT_MEMLOCK`,
   while `cipher_memory_security=ON` still wipes allocations.
6. Bootstrap before any SQLite/key/DB access and ordered close/shutdown/wipe/unlock/free.
7. A required validated bounds profile with no production default; until the
   later host matrix supplies exact arena/working-set/connection/cache caps,
   production storage must fail closed.
8. Focused tests and a worker handoff.

## 7. Acceptance criteria

- [ ] Arena readiness precedes every SQLite/database/key access.
- [ ] Every committed page reports Locked; failure touches no DB/key/sidecar.
- [ ] Allocator has no fallback/growth; tiny exhaustion returns `SQLITE_NOMEM`
      with transaction integrity and zero fallback count.
- [ ] A fresh direct Cargo build automatically compiles pinned SQLCipher with
      `-DOMIT_MEMLOCK`; `force = true` prevents ambient override and no
      wrapper-only environment is required.
- [ ] `cipher_memory_security=ON` is verified after forced allocation.
- [ ] Shutdown closes connections, calls `sqlite3_shutdown`, wipes all arena
      bytes, unlocks/frees memory, and restores current-process adjustment.
- [ ] Connection/cache/arena/working-set values have no guessed production
      default and cannot become ready without an immutable, digest-bound,
      human-accepted measured profile.
- [ ] Failure/unavailable behavior is explicit and tested; no boundary weakens.
- [ ] No unrelated files change.

## 8. Required verification

Precondition for every command, run once in a standard-user PowerShell:
`New-Item -ItemType Directory -Force -Path 'L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001\.tmp','L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001\target-evidence' | Out-Null; $env:TEMP='L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001\.tmp'; $env:TMP=$env:TEMP; $env:CARGO_TARGET_DIR='L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001\target-evidence'`.

| Layer | Exact command/scenario | Expected result | Evidence |
|---|---|---|---|
| Static | `cargo fmt --all -- --check` | exit 0 | command/revision |
| Static | `cargo clippy -p loki-storage -p loki-runtime --all-targets -- -D warnings` | exit 0 | run log |
| Unit | `cargo test -p loki-storage --all-targets -- --test-threads=1` | all pass | count/summary |
| Integration | `cargo test -p loki-runtime --test secure_memory_arena_runtime -- --test-threads=1` | lifecycle/fail-closed tests pass | child-process evidence |
| Build | `$datBuildRun = [guid]::NewGuid().ToString('N'); $env:CARGO_TARGET_DIR = "L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001\target-omit-memlock-$datBuildRun"; cargo build -vv --locked -p loki-storage` | fresh verbose compile contains `-DOMIT_MEMLOCK` | log plus source/build digest |
| Contract | `cargo test -p loki-storage --test secure_arena_profile -- --test-threads=1` | missing/tampered/oversized profile rejected | run log |
| Workspace | `cargo clippy --workspace --all-targets -- -D warnings` and `pnpm run verify` | exit 0 | integrated run |
| Hardware | n/a | no device action | recorded n/a |

CODE acceptance does not satisfy ADR-0005. `DAT-002-MEM-HOST-VERIFY-001` must
still prove exact arena/working-set/connection/cache bounds; all SQLite consumers
together; Win10/11 lock/quota/churn/recovery/rotation; source/build digest;
signature/SBOM; packaging/performance budgets. `DAT-002-MEM-REV-001` and final
`DAT-002-MEM-IMPL-001` acceptance remain blocked until that evidence exists.

## 9. Risks and safety

- Risk: `critical`; failures include unlocked secrets, recursion, premature init,
  fallback, corruption, use-after-free, and resource leakage
- Recovery: refuse readiness; use fresh disposable redacted `L:` test data only
- Toolchain: pinned existing toolchain, no installation/update
- Cloud/device impact: none

## 10. Human gate

ADR-0005 authorizes this standard-user code work only. Elevation, VM, signing
credential/provisioning, account/UAC/registry/host-policy/C-drive changes,
dependency replacement, or vendored patching require a new exact approval.

## 11. Worker and reviewer

- Worker: isolated secure-memory code implementer
- Reviewer: independent `DAT-002-MEM-CODE-REV-001`
- Worktree: `L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001`
- Branch: `codex/dat-002-mem-code-001`
- Handoff: `Docs/graph/handoffs/DAT-002-MEM-CODE-001-1.md`

## 12. Stop conditions

Return `BLOCKED_DEPENDENCY` or `NEEDS_REPLAN` if the build flag cannot be made
reproducible without a prohibited change, SQLite initializes before arena
readiness, allocator ownership cannot cover every consumer, elevation is needed,
or verification would touch real user data.
