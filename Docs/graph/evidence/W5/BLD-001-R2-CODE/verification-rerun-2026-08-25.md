# BLD-001-R2-CODE independent verification rerun

- Packet baseline: `0bd4a7041f3d01c5b419feccc0d48c3198917ac4`
- Worktree baseline: `7a1115f5b291c71c4ffb0e13deee8d1af9654268`
- Worktree: `L:\LokiAI-v1.2-wt\BLD-001-R2-CODE`
- Branch: `codex/bld-001-r2-code`
- Tester: Claude Code, acting as the assigned CODE-node implementer/verifier
- Run time: `2026-08-25T10:35Z` to `2026-08-25T10:50Z` (IST 16:05–16:20)
- Toolchain: `rustc 1.97.1`, `cargo 1.97.1`, Node `v24.19.0`, pnpm `11.21.0` (via
  `corepack pnpm`; bare `pnpm` was not on `PATH` in this session — a temporary
  `pnpm.cmd -> corepack pnpm` shim was placed under a task-owned `.tmp\bin`
  directory and removed after use; nothing outside the worktree was modified)
- Environment: standard user (`lauki`, non-administrator); `TEMP`/`TMP` were
  under the worktree's own `.tmp` directory; Cargo used its default
  `target/` directory inside the worktree (already covered by
  `.gitignore`/`.prettierignore`) after an initial attempt with a custom
  `.cargo-target` directory produced Prettier findings against generated
  Cargo `.json` fingerprints (that custom directory did not match the
  ignore glob) — the custom directory was deleted and the default
  `target/` was used instead; no VHDX, disk, elevation, ACL, account, UAC,
  registry, host-policy, C-drive, VM, device, USB, or serial operation was
  invoked at any point.

## Result summary

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0, no diff |
| `cargo clippy -p loki-build-executor --all-targets -- -D warnings` | exit 0, zero warnings |
| `cargo test -p loki-build-executor --lib -- --test-threads=1` | exit 0; 11 passed, 0 failed/ignored |
| `cargo test -p loki-build-executor --test broker_contract -- --test-threads=1` | exit 0; 8 passed, 0 failed/ignored |
| `cargo test -p loki-build-executor --test broker_dry_run -- --test-threads=1` | exit 0; 2 passed, 0 failed/ignored |
| `cargo test -p loki-build-executor --test native_boundary -- --test-threads=1` | exit 0; 1 passed, 0 failed/ignored |
| `cargo clippy --workspace --all-targets -- -D warnings` | **could not complete** in this session; see below |
| `pnpm run verify` | **could not complete** in this session; see below |
| `git diff --check` | exit 0, no whitespace errors |
| manual scan for `Command::new`/`std::process`/`powershell`/`windows::Win32`/VHDX-API symbols in `crates/loki-build-executor/**` | no matches |
| manual scan for private-key/API-key/secret/password/bearer patterns in `crates/loki-build-executor/**` | no matches |

All commands scoped to `loki-build-executor` (the only commands this node
owns and can guarantee) passed cleanly and reproduce the counts reported by
the prior in-worktree draft handoff.

## Workspace-wide gate: environment blocker, not a crate defect

Both `cargo clippy --workspace --all-targets -- -D warnings` and
`pnpm run verify` (which invokes `cargo build --workspace` /
`cargo test --workspace` internally) failed in this session while compiling
`openssl-sys v0.9.117`, a transitive dependency of pre-existing crates
unrelated to `loki-build-executor` (which depends only on `serde`,
`serde_json`, and `sha2`).

- On the first `cargo clippy --workspace` attempt (custom `CARGO_TARGET_DIR`
  under the worktree), the build failed immediately and explicitly:
  `An Application Control policy has blocked this file. (os error 4551)`
  while trying to execute `openssl-sys`'s build script.
- The identical failure was reproduced by running
  `cargo clippy --workspace --all-targets -- -D warnings` from an unmodified
  checkout of the integration branch at `L:\LokiAI-v1.2` (no
  `loki-build-executor` changes present), confirming this is a pre-existing
  host/sandbox limitation and not something introduced by this node.
- On the `pnpm run verify` attempt (default `target/` directory), Prettier,
  ESLint, TypeScript, Electron unit tests (77/77 passing), and
  `cargo fmt --all --check` all completed successfully; the run then reached
  `cargo:build --workspace` and stalled compiling `openssl-sys` (via Perl's
  `Configure` step) with near-zero CPU usage for over ten minutes before this
  worker terminated the hung `cargo`/child processes. No further log output,
  pass, or failure was produced by the underlying command itself before
  termination.

This worker's ownership is limited to `crates/loki-build-executor/**` and a
narrowly scoped root `Cargo.toml`/`Cargo.lock` registration; it has no
authority to change another crate's TLS/OpenSSL dependency, vendor a
different backend, or alter the host's Application Control policy (the
latter would also be a forbidden system-level/host-policy change under this
packet and under `AGENTS.md`). The workspace-wide required-verification row
therefore cannot be satisfied from inside this node's ownership boundary in
this environment.

## Discrepancy noted against the prior draft evidence

The existing (uncommitted) `Docs/graph/evidence/W5/BLD-001-R2-CODE/verification.md`,
dated `2026-08-24T18:32Z`–`18:33Z`, reports `cargo clippy --workspace` and
`pnpm run verify` both exiting `0` in that session. This worker could not
reproduce that result one day later on the same worktree/branch and the same
pinned toolchain; the failure point (`openssl-sys` build-script execution
blocked by Application Control policy) is identical to the failure reproduced
against the unmodified integration branch. This is recorded as a fact, not an
accusation: either the host's Application Control policy changed between the
two sessions, or the prior session's target/build cache already contained a
successfully-built `openssl-sys` artifact that this session's clean
`target/` did not have cached. The orchestrator should treat the
workspace-wide gate as unverified pending an environment fix, regardless of
which explanation is correct.

## Scope this rerun confirms

- No code in `crates/loki-build-executor/**` invokes a VHDX/disk API, an
  external process, PowerShell, a Windows Win32 API, or any host-mutating
  operation. `#![forbid(unsafe_code)]` is present at the crate root.
- All native/host authority is expressed only as injectable Rust traits
  (`NativeCapsuleApi`, `WindowsDiskApi`); the only production implementation
  in this crate (`HostOperationsDisabled`) always fails closed with
  `ErrorCode::NativeUnavailable` and performs no operation.
- `Cargo.toml`/`Cargo.lock` changes are limited to registering the new
  workspace member and its three already-reused, pinned dependencies
  (`serde`, `serde_json`, `sha2`) plus a dev-only `tempfile`; no new
  third-party dependency was added.
- Diff/ownership scope is limited to `Cargo.toml`, `Cargo.lock`,
  `crates/loki-build-executor/**`, and this node's own evidence/handoff
  files.
