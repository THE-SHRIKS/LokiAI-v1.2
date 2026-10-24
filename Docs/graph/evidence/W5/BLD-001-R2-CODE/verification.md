# BLD-001-R2-CODE verification evidence

- Packet baseline: `0bd4a7041f3d01c5b419feccc0d48c3198917ac4`
- Worktree baseline: `7a1115f5b291c71c4ffb0e13deee8d1af9654268`
- Worktree: `L:\LokiAI-v1.2-wt\BLD-001-R2-CODE`
- Branch: `codex/bld-001-r2-code`
- Tester: isolated build-containment code implementer
- Final run time: `2026-08-24T18:32Z` to `2026-08-24T18:33Z`
- Toolchain: `rustc 1.97.1`, `cargo 1.97.1`, Node `v24.19.0`, pnpm `11.19.0`
- Environment: standard user; `TEMP`, `TMP`, and `CARGO_TARGET_DIR` were under
  `L:\LokiAI-v1.2-wt\.task-build\BLD-001-R2-CODE`; no VHDX, disk, elevation,
  ACL, account, UAC, registry, host-policy, C-drive, VM, device, USB, or serial
  operation was invoked.

## Final gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy -p loki-build-executor --all-targets -- -D warnings` | exit 0, zero warnings |
| `cargo test -p loki-build-executor --lib -- --test-threads=1` | exit 0; 11 passed, 0 failed/ignored |
| `cargo test -p loki-build-executor --test broker_contract -- --test-threads=1` | exit 0; 8 passed, 0 failed/ignored |
| `cargo test -p loki-build-executor --test broker_dry_run -- --test-threads=1` | exit 0; 2 passed, 0 failed/ignored |
| `cargo test -p loki-build-executor --test native_boundary -- --test-threads=1` | exit 0; 1 passed, 0 failed/ignored |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `pnpm run verify` | exit 0; Prettier, ESLint, TypeScript, Electron, Cargo workspace build/tests passed |
| `git diff --check` | exit 0 |

`pnpm run verify` retained four pre-existing renderer ESLint warnings and zero
errors. Its final run passed 303 contracts tests, 85 MCP-server tests, 77
Electron tests, and the complete Rust workspace test/doc-test graph. The one
ignored connector SDK doc example predates and is unrelated to this node.

An earlier verify attempt placed `.target` inside the source worktree, causing
Prettier to inspect generated Cargo JSON. The implementer validated and removed
only the task-created `.target`/`.tmp` directories, moved build/temp output to
the task-specific external `L:` directory above, and reran the full gate to exit
0. No source result was hidden or reformatted to bypass the gate.

## Host-operation ledger

`broker_dry_run` used the production `HostOperationsDisabled` adapter. The
authenticated create request durably recorded intent, attempted no native host
operation, returned `NativeUnavailable`, and journaled `Quarantined`. The
injected failure case invoked exactly one in-memory adapter method and also
ended `Quarantined`.

`native_boundary` exercised only an in-memory typed low-level ledger. It proved
the fixed order `create, attach, mount, acl, stage, execute, quarantine, detach,
destroy`; the API carries opaque identities/digests/limits and no host path,
command, executable, URL, environment assignment, or broad group.

## Hostile matrix covered in CODE

- malformed JSON, unknown fields, oversized frames and data;
- invalid identifiers/digests after deserialization;
- bad MAC, expired validity window, replay, and saturated replay cache;
- non-create first action, wrong job, cross-capsule/base substitution, wrong or
  missing volume identity, duplicate inputs, and invalid resource bounds;
- invalid lifecycle skips/reuse, invalid journal state sequence, journal tamper,
  stale single-writer lock, and native failure quarantine;
- weakened containment policy/network capability, unallowlisted recipe values,
  output digest mismatch, and provenance/cache invalidation.

## Diff, ownership, and secret inspection

Final tracked/untracked scope is limited to `Cargo.toml`, `Cargo.lock`,
`crates/loki-build-executor/**`, this evidence file, and the owned handoff.
Searches found no private-key, API-key, client-secret, password, or bearer-token
pattern. Searches found no process-launch, VHDX/disk, PowerShell, ACL mutation,
COM/serial, or USB implementation. `#![forbid(unsafe_code)]` applies to the new
crate. SHA-2 reuses the workspace-pinned `0.10.9`; no new third-party package is
introduced beyond registering the new crate.

## Explicit deferral

This is CODE evidence only. It is not real containment evidence and does not
accept `BLD-001`. Signed broker provisioning, real VHDX lifecycle, capsule DACL,
cross-job/L8/fault/reboot evidence, and supported-Windows validation remain
exclusively in separately approved `BLD-001-R2-HOST-VERIFY`.

