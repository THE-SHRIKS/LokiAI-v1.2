# Worker Handoff — `BLD-001-R2-CODE`

Result: `BLOCKED_DEPENDENCY`  
Worker: `isolated build-containment code implementer`  
Started from: `7a1115f5b291c71c4ffb0e13deee8d1af9654268` (packet decision baseline `0bd4a7041f3d01c5b419feccc0d48c3198917ac4`)  
Result revision/patch: `uncommitted owned patch in codex/bld-001-r2-code`  
Completed at: `2026-08-24T18:33:00Z`

## 1. Outcome

The unprivileged CODE implementation is ready: it provides a bounded HMAC-SHA256
broker envelope, fail-closed replay/expiry/scope validation, hash-chained durable
lifecycle journal, capsule-internal ACL plan, strict retained containment
policy, typed recipe/provenance/output validation, and an injectable Windows
native boundary. The production CODE adapter performs no host operation and
fails closed until HOST-VERIFY supplies a separately reviewed implementation.
Every worker-run CODE gate passes. The result remains `BLOCKED_DEPENDENCY`
solely because the packet-mandated independent `BLD-001-R2-CODE-REV-001`
security review has not yet run; this worker cannot self-certify independence.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Bounded authenticated protocol rejects arbitrary authority, malformed/stale/oversized/replay/substitution | pass | `protocol.rs`; 8 `broker_contract` cases |
| Explicit, retry-aware lifecycle and durable journal; uncertainty quarantines | pass | `lifecycle.rs`, `journal.rs`, `broker.rs`; 11 unit + 2 dry-run cases |
| Typed native broker/executor boundary; no PowerShell product path | pass | `native.rs`, `windows_boundary.rs`; 1 native-boundary test |
| AppContainer/low-integrity/no-network/handles/Job/Win32k/recipe/provenance/output controls retained | pass structurally | `containment.rs`, `recipe.rs`; policy/recipe tests |
| Tests invoke no VHDX/disk/elevation/install/sign/device/host-ACL operation | pass | deny-all adapter ledger; forbidden-operation scan |
| No existing host path ACE or broad access group modeled | pass | capsule-role-only ACL plan; no host path in protocol/native traits |
| Failure/unavailable behavior explicit | pass | `NativeUnavailable`, journal/replay saturation/identity/native failure tests |
| No prohibited boundary weakened/unrelated files changed | pass | ownership/diff scan; workspace gates |
| Independent security review has no critical/high finding | blocked | `BLD-001-R2-CODE-REV-001` not yet dispatched/completed |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `Cargo.toml`, `Cargo.lock` | register the new crate; reuse existing pinned dependencies | yes |
| `crates/loki-build-executor/Cargo.toml` | crate manifest | yes |
| `crates/loki-build-executor/src/{protocol,broker,lifecycle,journal}.rs` | authenticated protocol and fail-closed journaled state machine | yes |
| `crates/loki-build-executor/src/{containment,native,windows_boundary}.rs` | retained policy, capsule ACL roles, typed injectable native boundary | yes |
| `crates/loki-build-executor/src/{recipe,error,lib}.rs` | typed recipe/provenance/output and public error/module surface | yes |
| `crates/loki-build-executor/tests/**` | hostile contract, deny-all dry-run, native-order fixtures | yes |
| `Docs/graph/evidence/W5/BLD-001-R2-CODE/verification.md` | exact gate and safety evidence | yes |
| `Docs/graph/handoffs/BLD-001-R2-CODE-1.md` | this handoff | yes |

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | Rust 1.97.1; L: temp/target | exit 0 | verification evidence |
| `cargo clippy -p loki-build-executor --all-targets -- -D warnings` | same | exit 0 | verification evidence |
| `cargo test -p loki-build-executor --lib -- --test-threads=1` | same | 11 passed | verification evidence |
| `cargo test -p loki-build-executor --test broker_contract -- --test-threads=1` | same | 8 passed | verification evidence |
| `cargo test -p loki-build-executor --test broker_dry_run -- --test-threads=1` | standard user; deny-all adapter | 2 passed; zero host ops | verification evidence |
| `cargo test -p loki-build-executor --test native_boundary -- --test-threads=1` | in-memory typed ledger | 1 passed | verification evidence |
| `cargo clippy --workspace --all-targets -- -D warnings` | L: temp/target | exit 0 | verification evidence |
| `pnpm run verify` | Node 24.19.0; pnpm 11.19.0; L: temp/target | exit 0 | verification evidence |
| diff/secret/forbidden-operation/ownership review | dirty isolated worktree | clean | verification evidence |

Mandatory verification not run:

- independent `BLD-001-R2-CODE-REV-001` review; required before `COMPLETE`;
- real containment/L8/VHDX/signed-broker/host matrix; correctly reserved for
  `BLD-001-R2-HOST-VERIFY`, not a skipped CODE test.

## 5. Contracts, migrations, and compatibility

- Contracts changed: private build-broker protocol v1, lifecycle journal v1,
  typed recipe/provenance and native adapter interfaces.
- Consumers affected: future runtime coordinator and signed host broker only.
- Migration: none; new journal files only.
- Backward/rolling compatibility: no released predecessor; unknown versions and
  fields are rejected.
- Generated-contract drift: n/a; private Rust contract has hostile serde tests.
- Build recipe/executor/provenance compatibility: structurally validated and
  cache-key invalidation tested; real toolchain/executor belongs to HOST-VERIFY.
- Cloud API/offline/notification compatibility: n/a.

## 6. Security, privacy, and device safety

- Trust boundaries touched: runtime-to-build-broker request; broker journal;
  future broker-to-native adapter; capsule-to-external output validation.
- Authorization/approval impact: no policy change; protocol authenticates with a
  minimum 256-bit key, bounded 60-second lifetime and non-evicting live replay cache.
- Secrets/data handling: synthetic values only; MAC key is not serializable or
  logged; no credentials captured.
- Containment profile and denied authorities: AppContainer, low integrity, empty
  network capabilities, pre-create Win32k disable, suspended-child attestation,
  explicit handles, Job kill-on-close/no-breakaway and resource bounds retained.
- Device side effects performed: none; no device/USB/serial handle was opened.
- New dependencies: no new third-party package; existing pinned serde,
  serde_json, sha2 0.10.9 and tempfile are reused.
- Remaining risk: concrete signed Win32/VHDX implementation and real adversarial
  containment remain unimplemented/unverified and must not be inferred here.

## 7. Decisions and assumptions

- The CODE node carries no host path. A separately installed trusted broker owns
  mapping opaque sealed identities to broker-owned capsule resources.
- Replay-cache saturation rejects new work instead of evicting a still-live nonce.
- Journal lock residue, invalid state/hash, and native uncertainty fail closed and
  require reconciliation; no capsule is reused.
- The dirty R1 worktree supplied conceptual evidence only; its existing-volume
  ACL implementation and verification scripts were not copied.

## 8. Review focus

- HMAC canonicalization/constant-time comparison and replay-cache lifecycle.
- Journal write-before-mutate ordering, hash/state-chain rules, stale-lock and
  crash semantics.
- Cross-job/base/volume binding and native handle retention on failure.
- Absence of path/command/executable/URL/environment authority.
- Recipe parameter/output/provenance validation and containment-policy invariants.

## 9. Follow-up graph changes proposed

- Dispatch independent `BLD-001-R2-CODE-REV-001` against this exact patch.
- Keep `BLD-001-R2-HOST-VERIFY` blocked pending its separate exact human
  approval, signed-broker/provisioning environment, and full real Windows matrix.

Only the orchestrator updates the task graph.
