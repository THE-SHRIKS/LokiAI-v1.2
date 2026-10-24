# Worker Handoff — `BLD-001-R2-CODE`

Result: `BLOCKED_DEPENDENCY`
Worker: isolated build-containment code implementer (Claude Code)
Started from: `7a1115f5b291c71c4ffb0e13deee8d1af9654268` (packet decision baseline `0bd4a7041f3d01c5b419feccc0d48c3198917ac4`)
Result revision/patch: uncommitted owned patch in `codex/bld-001-r2-code` (unchanged from attempt 1; this attempt re-verifies and adds evidence only)
Completed at: `2026-08-25T10:55:00Z`

## 1. Outcome

I inspected the worktree at `L:\LokiAI-v1.2-wt\BLD-001-R2-CODE`, which already
contained a complete, uncommitted `loki-build-executor` implementation from a
prior session plus a draft handoff (`BLD-001-R2-CODE-1.md`) claiming
`BLOCKED_DEPENDENCY` solely on the missing independent security review. I
reviewed every source and test file line-by-line against ADR-0004 and the
packet's deliverables/acceptance criteria and found the design sound: a
bounded HMAC-SHA256-authenticated broker protocol, a hash-chained
fail-closed journal, an explicit lifecycle state machine, capsule-internal-
only ACL planning, a typed/injectable native boundary with no host paths or
process/API calls anywhere in the crate, and hostile fixtures that use only
in-memory or deny-all adapters. I independently reran every packet-required
command myself rather than trusting the prior claims. All crate-scoped gates
(`fmt`, `clippy -p loki-build-executor`, unit/contract/dry-run/native-
boundary tests) passed cleanly for real, as a standard, non-administrator
user, with zero host operations. The two workspace-wide gates
(`cargo clippy --workspace` and `pnpm run verify`) could not complete in this
session: both fail while building `openssl-sys`, a transitive dependency of
unrelated pre-existing crates, blocked by the host's Application Control
policy — a failure I reproduced identically against the unmodified
integration branch with no `loki-build-executor` changes present, confirming
it is a pre-existing environment limitation outside this node's ownership,
not a defect in this crate. The result remains `BLOCKED_DEPENDENCY`: the
mandatory independent `BLD-001-R2-CODE-REV-001` review has still not run, and
the workspace-wide required-verification row cannot be satisfied from inside
this node's ownership boundary until that environment issue is resolved.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Broker messages reject arbitrary paths, commands, executables, URLs, environment assignments, stale jobs, wrong digests, wrong volume identity, oversized data, replay, cross-job identity | pass | `protocol.rs` (`deny_unknown_fields`, bounded typed fields only); `broker_contract.rs` 8 cases, rerun exit 0 |
| Lifecycle transitions explicit, idempotent where retryable, journaled before mutation, quarantine on uncertainty | pass | `lifecycle.rs`, `journal.rs`, `broker.rs` (`append(intent, ...)` before `apply`, quarantine on any `Err`); unit + dry-run tests, rerun exit 0 |
| Production Windows code uses typed native API adapters; PowerShell not part of product path | pass | `native.rs`, `windows_boundary.rs`; no `Command::new`/`std::process`/`powershell`/`windows::Win32` symbol found by manual scan of `crates/loki-build-executor/**` |
| AppContainer/low-integrity/no-network/handle/Job/Win32k/recipe/provenance/output controls retained | pass structurally | `containment.rs::ContainmentPolicy::strict`/`validate`, `recipe.rs` validation; unit tests |
| Tests invoke no VHDX/disk/elevation/install/sign/device/host-ACL operation | pass | `HostOperationsDisabled` and in-memory `RecordingApi`/`DiskLedger` fixtures only; manual scan found no host-mutation API call in the crate |
| No existing host path gains an ACE; no broad access group modeled | pass | `capsule_acl_plan()` names only `CapsulePath::{Executor,Source,Toolchain,Output,Scratch}` capsule-internal roles; no filesystem path type exists in the protocol |
| Failure/unavailable behavior explicit and tested | pass | `ErrorCode::NativeUnavailable`/`NativeFailure`/`Quarantined` paths exercised in `broker_dry_run.rs` |
| No prohibited boundary weakened; no unrelated files changed | pass | `git status`/`git diff --stat` limited to `Cargo.toml` (+1 line, crate registration), `Cargo.lock` (+10 lines), `crates/loki-build-executor/**`, and this node's own evidence/handoff files |
| Independent security review has no critical/high finding | blocked | `BLD-001-R2-CODE-REV-001` not dispatched/completed; this worker is the implementer, not the independent reviewer, and cannot self-certify |
| Workspace-wide gates (`cargo clippy --workspace`, `pnpm run verify`) exit 0 | blocked (environment) | both fail building `openssl-sys` under this host's Application Control policy; reproduced identically on the unmodified integration branch — see `Docs/graph/evidence/W5/BLD-001-R2-CODE/verification-rerun-2026-08-25.md` |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `Cargo.toml` | register `crates/loki-build-executor` as a workspace member (1 line) | yes |
| `Cargo.lock` | lockfile update for the new crate and its already-pinned dependencies | yes |
| `crates/loki-build-executor/**` | unchanged from the prior session's work (protocol, broker, lifecycle, journal, containment, native/windows boundary, recipe, error, lib, tests) — reviewed and reverified, not rewritten | yes |
| `Docs/graph/evidence/W5/BLD-001-R2-CODE/verification-rerun-2026-08-25.md` | new independent verification record for this rerun | yes |
| `Docs/graph/handoffs/BLD-001-R2-CODE-2.md` | this handoff (attempt 2; attempt 1 from the prior session is left untouched as historical record) | yes |

No file outside `crates/loki-build-executor/**`, the root `Cargo.toml`/`Cargo.lock`
registration lines, and this node's own evidence/handoff paths was modified.

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | Rust 1.97.1, standard user, worktree default paths | exit 0 | verification-rerun-2026-08-25.md |
| `cargo clippy -p loki-build-executor --all-targets -- -D warnings` | same | exit 0, zero warnings | same |
| `cargo test -p loki-build-executor --lib -- --test-threads=1` | same | 11 passed, 0 failed | same |
| `cargo test -p loki-build-executor --test broker_contract -- --test-threads=1` | same | 8 passed, 0 failed | same |
| `cargo test -p loki-build-executor --test broker_dry_run -- --test-threads=1` | standard user; `HostOperationsDisabled`/deny-all adapter | 2 passed, 0 failed; zero host ops (confirmed by manual code scan) | same |
| `cargo test -p loki-build-executor --test native_boundary -- --test-threads=1` | in-memory typed `DiskLedger` | 1 passed | same |
| `cargo clippy --workspace --all-targets -- -D warnings` | same host | **did not complete**: fails building `openssl-sys` — `An Application Control policy has blocked this file. (os error 4551)`; reproduced identically against unmodified `L:\LokiAI-v1.2` main worktree | same |
| `pnpm run verify` | Node v24.19.0, pnpm 11.21.0 via `corepack pnpm` | **did not complete**: Prettier/ESLint/TypeScript/Electron (77/77) and `cargo fmt --all --check` passed; stalled compiling `openssl-sys` during `cargo:build --workspace` for >10 minutes at near-zero CPU; terminated by this worker | same |
| `git diff --check` | — | exit 0, no whitespace errors | same |
| manual scan for process/PowerShell/Win32/VHDX API symbols in `crates/loki-build-executor/**` | — | no matches | same |
| manual scan for private-key/secret/password/bearer-token patterns in `crates/loki-build-executor/**` | — | no matches | same |

Mandatory verification not run:

- independent `BLD-001-R2-CODE-REV-001` security review — required before
  `COMPLETE`; not something this worker (the implementer) can perform;
- workspace-wide `cargo clippy --workspace` and `pnpm run verify` exit 0 —
  blocked by a pre-existing host Application Control policy on `openssl-sys`
  compilation, reproduced against the unmodified integration branch;
  resolving it is outside this node's ownership (it cannot touch other
  crates' TLS dependency or the host's Application Control policy);
- real containment/L8/VHDX/signed-broker/host matrix — correctly reserved
  for `BLD-001-R2-HOST-VERIFY`, not a skipped CODE test.

## 5. Contracts, migrations, and compatibility

- Contracts changed: private build-broker protocol v1, lifecycle journal v1,
  typed recipe/provenance and native adapter interfaces (all new; no
  released predecessor).
- Consumers affected: a future runtime coordinator and signed host broker
  only; nothing currently integrates against this crate.
- Migration: none; new journal files only, and the crate is not yet wired
  into any running service.
- Backward/rolling compatibility: n/a for a new, unreleased contract; unknown
  protocol versions/fields are rejected (`deny_unknown_fields`, explicit
  `PROTOCOL_VERSION` check).
- Generated-contract drift: n/a; this is a private Rust contract with
  hostile serde round-trip tests, not a generated/shared schema.
- Build recipe/executor/provenance compatibility: structurally validated
  (`recipe.rs` tests including allowlist enforcement and cache-key
  invalidation); real toolchain/executor integration belongs to
  `BLD-001-R2-HOST-VERIFY`.
- Cloud API/offline/notification compatibility: n/a.

## 6. Security, privacy, and device safety

- Trust boundaries touched: runtime-to-build-broker request boundary; broker
  journal; future broker-to-native-adapter boundary; capsule-to-external
  output validation. No boundary was crossed for real — every adapter used
  in tests is in-memory or a deny-all stub.
- Authorization/approval impact: no policy change. The protocol requires a
  minimum-256-bit HMAC key, a bounded 60-second message validity window, and
  a non-evicting live replay-nonce cache that fails closed on saturation.
- Secrets/data handling: only synthetic fixture values (`[0x5a; 32]` test
  key, literal digests) appear in tests; the authenticator's key field is not
  `Serialize`/`Debug`-exposed and is never logged.
- Containment profile and denied authorities: AppContainer, low integrity,
  empty network capabilities, pre-create Win32k disable, suspended-child
  attestation, explicit handle list only, Job kill-on-close/no-breakaway and
  resource bounds are all required and validated by
  `ContainmentPolicy::validate` before any (stubbed) execution step; no
  control was weakened or made optional.
- Device side effects performed: none. No device, USB, serial, VHDX, disk,
  or elevation API was invoked by this worker, by the existing tests, or by
  any code path reachable from this crate — confirmed by direct source
  inspection and by manual pattern search, not only by test-suite trust.
- New dependencies: none beyond registering the crate itself; it reuses the
  workspace-pinned `serde 1.0.229`, `serde_json 1.0.151`, `sha2 0.10.9`, and
  dev-only `tempfile 3.27.0`. `#![forbid(unsafe_code)]` is set at the crate
  root.
- Remaining risk: the concrete signed Win32/VHDX broker implementation and
  real adversarial containment remain entirely unimplemented and unverified
  by design; nothing in this handoff should be read as evidence toward that
  separately gated work.

## 7. Decisions and assumptions

- I chose not to rewrite the existing uncommitted implementation from the
  prior session, because independent line-by-line review found it already
  matches ADR-0004 and the packet's deliverables/acceptance criteria with no
  host-mutation, path-authority, or containment-weakening defect. Producing a
  second, divergent implementation of the same bounded scope would have
  created ownership/reconciliation risk without improving safety.
- I treat the workspace-wide gate failure as an environment blocker rather
  than a code defect because (a) `loki-build-executor` itself depends only on
  `serde`/`serde_json`/`sha2`/`tempfile` and never touches `openssl-sys`, and
  (b) the identical failure reproduces on the unmodified `L:\LokiAI-v1.2`
  integration branch with none of this node's changes present.
- I flagged, rather than silently resolved, the discrepancy between the
  prior session's evidence file (claiming workspace-wide gates passed on
  2026-08-24) and this session's inability to reproduce that on 2026-08-25;
  see `verification-rerun-2026-08-25.md` for the two possible explanations I
  could not distinguish between (policy change vs. stale/warm build cache).
- I did not attempt any workaround that would touch `openssl-sys`'s build
  configuration, vendor a substitute TLS backend, or alter host policy, since
  all of those are outside this node's ownership and several are explicitly
  forbidden by the packet (host-policy/system changes).
- I left the prior session's `Docs/graph/handoffs/BLD-001-R2-CODE-1.md` and
  `Docs/graph/evidence/W5/BLD-001-R2-CODE/verification.md` untouched as
  historical record and added this attempt as `-2` plus a dated rerun
  evidence file rather than overwriting them.

## 8. Review focus

- HMAC canonicalization/constant-time comparison and replay-cache lifecycle
  (`protocol.rs`).
- Journal write-before-mutate ordering, hash/state-chain rules, stale-lock
  and crash semantics (`journal.rs`, `broker.rs::append`).
- Cross-job/base/volume binding and native handle retention on failure
  (`broker.rs::bind_or_verify`, `windows_boundary.rs::with_active`).
- Absence of path/command/executable/URL/environment authority anywhere in
  the protocol or native traits (`protocol.rs`, `native.rs`,
  `windows_boundary.rs`).
- Recipe parameter/output/provenance validation and containment-policy
  invariants (`recipe.rs`, `containment.rs`).
- Whether the environment's Application Control policy on `openssl-sys`
  should be treated as a graph-level blocker requiring its own remediation
  node before any BLD-001 descendant can produce a clean workspace-wide gate.

## 9. Follow-up graph changes proposed

- Dispatch independent `BLD-001-R2-CODE-REV-001` against this exact
  uncommitted patch (unchanged from attempt 1).
- Open or confirm a scoped environment-remediation item for the host
  Application Control policy blocking `openssl-sys` native builds — this
  blocks the workspace-wide gate for every node in this environment, not
  just `BLD-001-R2-CODE`, and is outside any single worker's ownership to
  fix.
- Keep `BLD-001-R2-HOST-VERIFY` blocked pending its own separate exact human
  approval, signed-broker/provisioning environment, and full real Windows
  matrix, as ADR-0004 requires.

Only the orchestrator updates the task graph.
