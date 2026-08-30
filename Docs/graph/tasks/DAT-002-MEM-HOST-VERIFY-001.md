# Task Packet — `DAT-002-MEM-HOST-VERIFY-001 Real-Host Secure-Arena Bounds and Second-Identity Evidence`

Packet status: `accepted`
Graph status: `ready`
Created by: `orchestrator (Claude Code), shaped by loki-planner`
Created at: `2026-08-27T00:00:00Z`
Starting revision: `1d6ed5b` (DAT-002-MEM-CODE-001 integrated)

Human gate: approved in chat, 2026-08-27 — explicit go-ahead for real
fault-injection/lock/quota-breaking testing on the already-provisioned VM,
scoped exactly as this packet describes (no elevation beyond the VM's own
already-provisioned admin account, no host-wide policy change, no C-drive/
account/UAC/registry change, fully contained to the VM).

## 1. Objective

Produce the real Windows 11 measured evidence ADR-0005 requires before any
production secure-memory bounds profile can be proposed for acceptance:
(a) real VirtualLock/Locked-bit, quota-failure, exhaustion, fault/recover/
rotate, churn and soak evidence on the real Windows 11 VM; (b) an exact,
justified arena/working-set/connection/cache candidate bounds recommendation
derived from that measurement, not guessed; (c) the second-Windows-identity
DPAPI cross-user isolation matrix this VM specifically unblocks. This node
produces evidence and a recommended profile only — it does not embed an
accepted digest into `secure_profile.rs` (that is `DAT-002-MEM-IMPL-001`) and
does not claim signing/SBOM/full packaging evidence this environment cannot
produce (see Section 9 — named, not silently dropped).

## 2. Traceability

- Requirements: `NFR-DATA-002`, `NFR-SEC-002`, `NFR-REL-001`; `AS-016`
- Architecture: runtime-owned encrypted persistence, secure-arena lifecycle
- Security: SQLCipher, DPAPI, verified page locks, no-fallback allocation
- Parent: DAT-002 correction re-plan; decisions: ADR-0005
- Unblocks (on acceptance): `DAT-002-MEM-REV-001`; `DAT-002-MEM-IMPL-001`;
  downstream Lane A RPC/runtime chain (`P1-ACCEPTANCE-STORE-001` onward)

## 3. Dependencies and inputs

- `depends_on`: `DAT-002-MEM-CODE-001` (accepted, integrated)
- Consumes: `crates/loki-storage/src/secure_memory.rs`,
  `crates/loki-storage/src/secure_profile.rs` at the integrated revision
- Required environment: the existing Windows 11 VirtualBox VM `LokiAI`
  (single-CPU, Guest Additions installed, guest-control automation working,
  Rust+MSVC+Perl confirmed working — `loki-storage` builds clean, ~18min due
  to single-CPU), second local Windows account present — already provisioned
- Known starting conditions: `ACCEPTED_PROFILE = None` in `secure_profile.rs`
  (fail-closed, unchanged by this node); no SBOM tool exists anywhere in this
  repo; no code-signing certificate exists in this repo/environment

## 4. Required reading

- `AGENTS.md`, `Docs/TESTING.md`, `Docs/SECURITY.md`
- `Docs/graph/decisions/ADR-0005-verified-sqlcipher-secure-arena.md`
- `Docs/graph/tasks/DAT-002-MEM-CODE-001.md` and handoffs `-1` through `-5`
- `Docs/graph/evidence/W4/DAT-002/REVIEW-SECURITY-001.md` (second-identity
  procedure this node executes)
- `crates/loki-storage/src/secure_memory.rs`, `secure_profile.rs`, `key.rs`,
  `secret.rs`, `managed.rs`, `storage.rs`
- `crates/loki-storage/tests/secure_arena_profile.rs`,
  `crates/loki-runtime/tests/{process_behavior,secure_memory_arena_runtime,host_lifecycle}.rs`

## 5. Ownership

Writable paths:
- `crates/loki-storage/src/secure_memory.rs` — additive `test-support`-gated
  parameterizable candidate-profile bootstrap entry point ONLY (no change to
  `bootstrap()`, `prepare()`, `shutdown_inner()`, allocator, or any production
  code path — this file already went through 2 rounds of independent security
  review for exactly this class of risk; treat any new code here with the
  same scrutiny)
- `crates/loki-storage/tests/secure_arena_host_bounds.rs` (new) — lock/quota/
  exhaustion/fault/recover/rotate/churn/soak scenarios
- `crates/loki-storage/tests/second_identity_dpapi.rs` (new, VM-only)
- `Docs/graph/handoffs/DAT-002-MEM-HOST-VERIFY-001-1.md`
- `Docs/graph/evidence/W4/DAT-002-MEM-HOST-VERIFY-001/**`

Read-only: everything else.

Forbidden: embedding a real digest into `secure_profile.rs::ACCEPTED_PROFILE`;
weakening `require_ready()`/allocator fail-closed behavior; any elevation,
account/UAC/registry/host-policy change beyond using the pre-existing second
local account; touching real user data; introducing an SBOM/signing tool.

## 6. Deliverables

1. Parameterizable `test-support`-gated candidate-profile bootstrap entry
   point in `secure_memory.rs` (reuse existing `bootstrap_profile`).
2. Real VM evidence: page-lock attestation before key/DB access; induced
   working-set-quota refusal with no recursion/DB/key access; tiny-arena
   `SQLITE_NOMEM` exhaustion with zero fallback; recover-then-rotate fault
   matrix; churn (repeated bootstrap/shutdown cycles); soak (sustained run) —
   across all workspace SQLite-consumer crates together.

   **Important framing** (flagged by the planner, repeated here so it isn't
   lost): the new fixed-arena design makes a lock failure FATAL/fail-closed
   by construction. You cannot literally reproduce ADR-0005's original bug
   (a non-fatal `VirtualLock` failure that let allocation continue) against
   this code — that would be a regression, not evidence. Instead, INDUCE an
   artificial working-set constraint (e.g. temporarily lower the process
   working-set ceiling before bootstrap) and prove the new code refuses
   cleanly. Do not waste a round trying to recreate the old stack overflow.
3. Justified, exact candidate arena/working-set/connection/cache bounds
   recommendation with real measurement behind each number.
4. Reproducible source/build digest evidence and `-DOMIT_MEMLOCK` proof on
   the VM (second independent real host, matching `DAT-002-MEM-CODE-001`'s
   own evidence shape).
5. Second-Windows-identity DPAPI/ACL matrix per `REVIEW-SECURITY-001.md`'s
   exact list: owner reopen, cross-identity DPAPI denial independent of ACL
   denial, explicit ACL denial, restart survival, distinct malformed/missing/
   truncated recovery states, unchanged file hashes, no replacement/sidecar
   creation — with reviewed revision, executable digest, OS build, redacted
   SIDs, canonical SDDL, exact commands, classifications, before/after
   hashes recorded.
6. Worker handoff and evidence index under
   `Docs/graph/evidence/W4/DAT-002-MEM-HOST-VERIFY-001/`.
7. Explicit named list of ADR-0005 evidence items this node cannot satisfy
   from the VM alone (Section 9) — reported, not silently dropped.

## 7. Acceptance criteria

- [ ] Every committed arena page reports `Locked` via real `QueryWorkingSetEx`
      before any SQLite/key/DB access, on the real VM.
- [ ] A real induced working-set-quota failure refuses startup with zero
      recursion and touches no DB/key/sidecar file.
- [ ] A tiny candidate arena under real allocation pressure returns
      `SQLITE_NOMEM`, preserves transaction integrity, zero fallback/growth.
- [ ] The full recover-then-rotate fault lifecycle completes without
      recursion, corruption, or use-after-free, across repeated real runs.
- [ ] A churn cycle and a soak run show zero leak growth, zero fallback, zero
      unexplained `OutstandingAllocations`.
- [ ] All workspace SQLite-consumer crates opened together pass under the
      recommended candidate bounds.
- [ ] A fresh from-scratch VM build reproduces `-DOMIT_MEMLOCK` in the
      verbose build log with a recorded source/build digest.
- [ ] The recommended bounds profile's every field is justified by a
      specific measurement, not asserted.
- [ ] The second-identity matrix reproduces every item in
      `REVIEW-SECURITY-001.md`'s required-evidence list with two distinct
      non-admin SIDs on the real VM.
- [ ] No production fail-closed gate is weakened; `ACCEPTED_PROFILE` remains
      `None` in the delivered diff.
- [ ] Every ADR-0005 evidence item this node cannot satisfy is explicitly
      named in the handoff.
- [ ] No unrelated files change.

## 8. Required verification (ordered)

| # | Command/scenario | Expected result |
|---|---|---|
| 1 | `cargo fmt --all -- --check`, `cargo clippy -p loki-storage --all-targets -- -D warnings` | exit 0 |
| 2 | `cargo test -p loki-storage --test secure_arena_profile -- --test-threads=1` (baseline, before host work) | existing 5 pass, unaffected |
| 3 | New host test — induced lock/quota failure | fail-closed, no DB/key touch |
| 4 | New host test — tiny-arena exhaustion | `SQLITE_NOMEM`, zero fallback |
| 5 | New host test — fault/recover/rotate | no recursion/corruption |
| 6 | New host test — churn (N cycles) | zero leak, zero fallback |
| 7 | Soak (sustained duration — use your judgment on duration given single-CPU VM constraints, document what you chose and why) | stable stats, no degradation |
| 8 | `cargo test --workspace` at recommended bounds | pass, or only the already-known environment-flake class |
| 9 | Fresh `cargo build -vv --locked -p loki-storage` in a new `CARGO_TARGET_DIR` on the VM | `-DOMIT_MEMLOCK` present, digest recorded |
| 10 | Second-identity DPAPI matrix, run once as primary VM account (`loki`), once as the second local account | full matrix from `REVIEW-SECURITY-001.md` passes |
| 11 | `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm run verify` | exit 0 (one retry allowed on the known flaky Windows Application Control error) |
| 12 | Packaging/signing/SBOM/hardware | n/a — record explicitly with reason, do not fabricate |

## 9. Known gaps this VM-only node cannot close — do not silently drop

1. **Code signing** — no certificate exists anywhere in this repo/environment. Separate future resource/decision needed.
2. **SBOM** — no tool selected/introduced anywhere in this repo. Introducing one is its own small decision (AGENTS.md rule 11), out of scope here.
3. **Full installer/update packaging** — that's `Docs/RELEASE.md`/Part Two/`PKG-*` scope. This node reproduces only the native build-flag proof (Section 8 row 9), not MSI/installer packaging.
4. **Performance budgets as pass/fail thresholds** — no numeric SLA exists anywhere in `Docs/`. This node's soak/churn/lock timing measurement PRODUCES the first real numbers; it does not clear a pre-existing bar.
5. **Full "Win10+11" coverage** — the VM is Windows 11 only. A genuine dual-OS claim needs a second, separate Windows 10 host — not available. State this as a partial-coverage gap, do not narrow the acceptance wording to hide it.
6. **Single-CPU VM constraint** — materially limits how representative churn/soak/concurrency evidence is of real multi-core consumer hardware. State this plainly in the handoff.

## 10. Risks and safety

- Risk: `critical` — false bounds evidence could let production silently accept an unsafe profile later.
- Recovery: refuse readiness on any failed scenario; do not lower a candidate bound to make a test pass without re-justifying the number from the failure.
- `test_support` API surface growth in `secure_memory.rs` (Deliverable 1) touches the same file that took 2 rounds of independent security review with 2 Critical findings — treat any addition here with equal scrutiny, not as "test-only, low risk."

## 11. Human gate

Satisfied — see header. This exact scope (fault-injection on the already-provisioned VM, no elevation beyond the pre-existing accounts, no host-policy change) is approved.

## 12. Worker and reviewer

- Worker: `loki-implementer`, operating via the confirmed working guest-control automation into the VM `LokiAI`
- Independent reviewer: `loki-security-reviewer` (the `test_support` addition to `secure_memory.rs` specifically) and `loki-test-reviewer`
- Worktree: `L:\LokiAI-v1.2-wt\DAT-002-MEM-HOST-VERIFY-001` on the host, with `TEMP`/`TMP`/`CARGO_TARGET_DIR` redirected exactly as `DAT-002-MEM-CODE-001`'s packet required, PLUS actual test execution happens inside the VM via guest-control
- Handoff: `Docs/graph/handoffs/DAT-002-MEM-HOST-VERIFY-001-1.md`

## 13. Stop conditions

Return `BLOCKED_DEPENDENCY` if the VM cannot actually exercise a scenario for
environment reasons (name the exact reason). Return `NEEDS_REPLAN` if any
scenario reveals the current arena/allocator design cannot be made safe at
any bounds. Do not report `COMPLETE` for signature/SBOM/full packaging
evidence — report exactly which items this node cannot supply (Section 9),
route them as a named follow-up, not a silent gap.
