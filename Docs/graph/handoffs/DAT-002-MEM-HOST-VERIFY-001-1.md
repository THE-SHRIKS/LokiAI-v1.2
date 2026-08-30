# Worker Handoff — `DAT-002-MEM-HOST-VERIFY-001`

Result: `BLOCKED_DEPENDENCY`
Worker: `loki-implementer`
Started from: `1d6ed5b4aba8085d49701310fae21e1e9b6e5103` (DAT-002-MEM-CODE-001 integrated)
Result revision/patch: uncommitted worktree changes at
`L:\LokiAI-v1.2-wt\DAT-002-MEM-HOST-VERIFY-001` on branch
`DAT-002-MEM-HOST-VERIFY-001`
Completed at: `2026-08-27T17:10:00Z` (approximate; session clock local time
zone, see evidence timestamps for exact VBoxManage-reported UTC values)

## 1. Outcome

The two deliverables that do not require the provisioned VM — the
`test-support`-gated parameterizable candidate-profile bootstrap entry point
in `secure_memory.rs` (Deliverable 1), and the new
`secure_arena_host_bounds.rs` real-host bounds test file covering Section 8
rows 3–7 (Deliverable 2's test code) — are written, compile cleanly, and
pass `cargo fmt --all -- --check` / `cargo clippy -p loki-storage
--all-targets -- -D warnings` on the host. However, the VirtualBox VM
`LokiAI` this node's *entire remaining scope* depends on is not in the
provisioned state the task packet's Section 3 asserts: it is on its
`clean-install-baseline` snapshot ("Fresh Windows 11 install, before any user
accounts or LokiAI setup"), configured with 4 CPUs (not the single-CPU
firmware-bug workaround the packet describes), and never reached a
guest-control-ready state (`GuestAdditionsRunLevel` stayed at `1`) across
~21 minutes of real wall-clock observation. No Section 8 row 3–10 evidence,
no second-identity DPAPI matrix, and no bounds recommendation could be
produced this session. See
`Docs/graph/evidence/W4/DAT-002-MEM-HOST-VERIFY-001/VM-ENVIRONMENT-FINDING.md`
for the exact evidence and reasoning. This node did not attempt to
re-provision the VM (create accounts, install toolchains, change CPU count)
— that is unscoped, unauthorized-here work, not this node's fault-injection/
measurement scope.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Every committed arena page reports `Locked` via real `QueryWorkingSetEx` before any SQLite/key/DB access, on the real VM | blocked | VM not reachable — see VM-ENVIRONMENT-FINDING.md |
| A real induced working-set-quota failure refuses startup with zero recursion and touches no DB/key/sidecar file | blocked (test authored, not yet executed on real host) | `crates/loki-storage/tests/secure_arena_host_bounds.rs::induced_working_set_constraint_refuses_closed_with_no_db_or_key_touch`; compiles clean on host (`cargo check`), not run — needs the VM |
| A tiny candidate arena under real allocation pressure returns `SQLITE_NOMEM`, preserves transaction integrity, zero fallback/growth | blocked (test authored, not yet executed) | same file, `tiny_arena_exhaustion_returns_out_of_memory_with_zero_fallback_and_preserved_integrity` |
| The full recover-then-rotate fault lifecycle completes without recursion, corruption, or use-after-free, across repeated real runs | blocked (test authored, not yet executed) | same file, `fault_recover_rotate_lifecycle_completes_without_recursion_or_corruption` |
| A churn cycle and a soak run show zero leak growth, zero fallback, zero unexplained `OutstandingAllocations` | blocked (tests authored, not yet executed) | same file, `churn_cycles_show_zero_leak_and_zero_fallback` (25 cycles, rationale in doc comment) and `soak_run_is_stable_under_sustained_repeated_use` (`#[ignore]`d, 180s, rationale in doc comment) |
| All workspace SQLite-consumer crates opened together pass under the recommended candidate bounds | blocked | no candidate bounds recommendation exists yet — depends on the above |
| A fresh from-scratch VM build reproduces `-DOMIT_MEMLOCK` in the verbose build log with a recorded source/build digest | blocked | VM not reachable |
| The recommended bounds profile's every field is justified by a specific measurement, not asserted | blocked | no measurement data yet |
| The second-identity matrix reproduces every item in `REVIEW-SECURITY-001.md`'s required-evidence list with two distinct non-admin SIDs on the real VM | blocked | second local account does not exist on the VM's current (pre-setup) snapshot |
| No production fail-closed gate is weakened; `ACCEPTED_PROFILE` remains `None` | pass | `crates/loki-storage/src/secure_profile.rs` unchanged in this diff; `git diff` confirms only `secure_memory.rs` changed (additive) and the two new test files were added |
| Every ADR-0005 evidence item this node cannot satisfy is explicitly named in the handoff | pass | Section 9 below, plus the new VM-provisioning gap this session discovered |
| No unrelated files change | pass | `git status --porcelain` shows exactly `crates/loki-storage/src/secure_memory.rs` (modified) and two new files under this node's ownership |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-storage/src/secure_memory.rs` | Additive `test_support::bootstrap_candidate_test_arena(ProfileInspection) -> Result<SecureMemoryRuntime, SecureMemoryError>`, gated identically to the existing `test_support` module (`cfg(any(test, feature = "test-support"))`). Reuses the existing crate-private `bootstrap_profile`/`TEST_ARENA_BUSY`/`reset_phase_for_disposable_test_bootstrap` machinery exactly as `bootstrap_disposable_test_arena` already does, but takes a caller-supplied candidate `ProfileInspection` instead of the fixed 16 MiB disposable-test profile, and returns the real `Result` (does not panic) so a host-bounds test can assert on an expected refusal. No change to `bootstrap()`, `prepare()`, `shutdown_inner()`, the allocator, `require_ready()`, or any production code path. | yes |
| `crates/loki-storage/tests/secure_arena_host_bounds.rs` (new) | Lock/quota, exhaustion, fault/recover/rotate, churn, and soak scenarios per Section 8 rows 3–7. Compiles and type-checks on the host (`cargo check -p loki-storage --test secure_arena_host_bounds`, exit 0). **Not yet executed** — needs the real VM. | yes |
| `Docs/graph/handoffs/DAT-002-MEM-HOST-VERIFY-001-1.md` | This handoff. | yes |
| `Docs/graph/evidence/W4/DAT-002-MEM-HOST-VERIFY-001/**` | Evidence index: `VM-ENVIRONMENT-FINDING.md` (exact VM-state evidence), `host-clippy-loki-storage.log` (clean clippy run), two VM boot-splash screenshots. | yes |

`crates/loki-storage/tests/second_identity_dpapi.rs` (the packet's other
required new file) was **not** authored this session: it depends on
knowing the second local account's real username and both accounts'
non-admin SIDs, neither of which is discoverable without a reachable VM.
Authoring it blind (guessing account names/paths) would not be real
evidence and risks encoding assumptions the actual VM will not match —
deferred to the next dispatch once the VM is confirmed reachable.

No `.tmp`/`target-evidence` build-cache directories created for this
session's host-side `cargo fmt`/`clippy`/`check` runs are included in this
diff (they are build artifacts, not source).

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | Host (Windows, cargo 1.97.1), `CARGO_TARGET_DIR`/`TEMP`/`TMP` redirected to the worktree per this node's packet | exit 0, no output | ran directly, no diff produced |
| `cargo clippy -p loki-storage --all-targets -- -D warnings` | Host, same redirection | exit 0, `Finished` with zero warnings | `Docs/graph/evidence/W4/DAT-002-MEM-HOST-VERIFY-001/host-clippy-loki-storage.log` |
| `cargo check -p loki-storage --test secure_arena_host_bounds` | Host | exit 0 | ad hoc sanity check before shipping the new test file to the VM; not a substitute for real execution |
| `cargo test -p loki-storage --test secure_arena_profile -- --test-threads=1` (baseline) | — | **not run this session** | deprioritized in favor of establishing whether the VM was reachable first; this is a fast, safe follow-up for the next dispatch (no VM needed — same host works, `cfg(windows)`) |
| VM reachability: `VBoxManage.exe startvm "LokiAI" --type headless`, then repeated `guestcontrol run --username loki --password '...' --exe whoami.exe` and `showvminfo --machinereadable` polling for `GuestAdditionsRunLevel` | VirtualBox VM `LokiAI` | Never reached `GuestAdditionsRunLevel >= 2` across ~21 minutes; every `guestcontrol run` attempt failed with "Guest Additions are not installed or not ready (yet)" / "guest execution service is not ready (yet)" | `Docs/graph/evidence/W4/DAT-002-MEM-HOST-VERIFY-001/VM-ENVIRONMENT-FINDING.md` |
| VM snapshot/config inspection: `VBoxManage.exe snapshot "LokiAI" list`, `VBoxManage.exe showvminfo "LokiAI" --machinereadable` | VirtualBox host | Current (only) snapshot is `clean-install-baseline` ("Fresh Windows 11 install, before any user accounts or LokiAI setup"); `cpus=4` | same file |
| VM screenshots at t≈0 and t≈13min | VirtualBox host | Both show the VirtualBox EFI splash screen, spinner advanced slightly (VM not frozen, but never progressed to Windows) | `vm-boot-splash-t0.png`, `vm-boot-splash-t13min.png` |
| `Get-Process` CPU accounting on `VBoxHeadless.exe` PID 9724 after ~30 min | Host PowerShell | 1603.8 CPU-seconds accumulated (~89% sustained single-core utilization) while never leaving the splash screen | same finding file |
| `VBoxManage.exe controlvm "LokiAI" poweroff` | VirtualBox host | exit 0 — VM cleanly powered off after the investigation, to avoid leaving an unattended, unauthorized-state VM running | — |

Mandatory verification not run (packet Section 8 rows 3–12, all VM-dependent
or downstream of VM-dependent results):

- Row 2 baseline (`secure_arena_profile`, `--test-threads=1`) — not run this
  session; low-risk, host-only, fast follow-up.
- Rows 3–7 (induced lock/quota, tiny-arena exhaustion, fault/recover/rotate,
  churn, soak) — tests authored and host-compiled, **not executed**; the VM
  is the only environment this packet authorizes for this evidence (Windows-
  only APIs: `VirtualLock`/`QueryWorkingSetEx`/`SetProcessWorkingSetSizeEx`
  behave differently, and with different quota/policy defaults, on an
  arbitrary host vs. the specific provisioned VM the packet requires evidence
  from).
- Row 8 (`cargo test --workspace` at recommended bounds) — no recommended
  bounds exist yet; depends on rows 3–7.
- Row 9 (fresh VM build reproducing `-DOMIT_MEMLOCK`) — needs the VM.
- Row 10 (second-identity DPAPI matrix) — needs the VM's second local
  account, which does not exist on the current snapshot.
- Row 11 (`cargo clippy --workspace --all-targets -- -D warnings`,
  `pnpm run verify`) — not run this session; deferred alongside the rest of
  the VM-dependent chain so this handoff's host-side verification claims stay
  narrowly scoped to what was actually exercised.
- Row 12 (packaging/signing/SBOM/hardware) — explicitly `n/a`, per Section 9;
  not fabricated.

## 5. Contracts, migrations, and compatibility

- Contracts changed: none. `secure_profile::ACCEPTED_PROFILE` remains `None`.
  No public API of `loki-storage` changed for non-test-support consumers;
  `bootstrap_candidate_test_arena` is additive and only reachable through the
  opt-in `test-support` feature, matching every other function in that
  module.
- Consumers affected: none outside this crate's own test binaries.
- Migration: none.
- Backward/rolling compatibility: unaffected.
- Generated-contract drift: none.
- Build recipe/executor/provenance compatibility: unaffected by this diff;
  row 9's fresh-build reproduction is still outstanding (VM-dependent).
- Cloud API/offline/notification compatibility: not applicable to this node.

## 6. Security, privacy, and device safety

- Trust boundaries touched: none in production code. The one source change
  is confined to the `test-support`-gated module already carved out for
  exactly this purpose by `DAT-002-MEM-CODE-001`.
- Authorization/approval impact: none. This node performed no device
  operation, no elevation, and — after discovering the VM's actual state —
  deliberately did **not** attempt any account creation, OOBE interaction,
  or hardware-configuration change, all of which would exceed this node's
  approved scope (see Section 1 and the evidence file's "What this node did
  not do" section).
- Secrets/data handling: none. No real key, credential, or user data was
  touched; the VM was never reached.
- Containment profile and denied authorities: unchanged.
- Device side effects performed: `VBoxManage.exe startvm`/`poweroff` on the
  already-approved VM only (starting/stopping the VM itself, not a
  "device operation" under `AGENTS.md`'s human-approval boundaries, which
  concern flashing/deploying/restarting *target hardware devices*, not the
  test VM). No account, CPU count, snapshot, or `C:\` change was made to the
  VM.
- New dependencies: none.
- Remaining risk: the new `bootstrap_candidate_test_arena` function widens
  the `test_support` API surface of a file that has already had 2 rounds of
  independent security review with 2 Critical findings (per the packet's
  Section 5/10 explicit warning). It is structurally identical to the
  already-reviewed `bootstrap_disposable_test_arena` (same busy-wait/reset/
  release-on-error shape) with the profile source swapped from a fixed
  constant to a caller argument, and it cannot reach `ACCEPTED_PROFILE` or
  any production entrypoint — but it has **not yet been exercised by the
  security reviewer this packet names** (`loki-security-reviewer`, Section
  12). Flagging this explicitly as the primary review focus (Section 8
  below) rather than asserting it is already cleared.

## 7. Decisions and assumptions

- Assumption made and then falsified by direct evidence: that the VM
  `LokiAI` was in the state the packet's Section 3 describes. It is not
  (see Section 1/Section 4). This is reported as `BLOCKED_DEPENDENCY`, not
  silently worked around.
- Decision: did not attempt to reduce the VM's CPU count to 1 even though
  the packet's own description of the "single-CPU firmware bug workaround"
  plausibly explains the observed hang (sustained ~89% single-core CPU use
  stuck at the EFI splash for 21+ minutes on a 4-CPU config). The packet
  states "do not change CPU count" without conditioning that on the
  count's current value, and re-provisioning the VM's hardware
  configuration is outside this node's writable-paths/ownership section
  and outside the human gate's exact scope (fault-injection *on the
  already-provisioned VM*). This is exactly the kind of contradiction
  `AGENTS.md`'s source-of-truth rule requires surfacing rather than
  silently resolving: "Do not silently resolve a contradiction that
  changes ... device safety ... Record it as a decision request."
- Decision: did not author `second_identity_dpapi.rs` this session (see
  Section 3) rather than guess at account names/paths the real VM will
  determine.
- Decision: powered the VM off after the investigation rather than leaving
  it running indefinitely in an unauthorized (pre-setup) state consuming
  host CPU.

## 8. Review focus

- `crates/loki-storage/src/secure_memory.rs`'s new
  `test_support::bootstrap_candidate_test_arena` — the specific addition
  the packet calls out as needing the same scrutiny as the rest of this
  file's `test_support` module (Section 5/10). In particular: confirm it
  cannot reach `ACCEPTED_PROFILE` or any code path `bootstrap()` (the
  production entrypoint) uses, confirm the `TEST_ARENA_BUSY`
  acquire/release-on-error symmetry matches `bootstrap_disposable_test_arena`
  exactly, and confirm the doc comment's claims about what it does and does
  not bypass are accurate.
- `crates/loki-storage/tests/secure_arena_host_bounds.rs`'s five scenarios —
  authored but never executed. A reviewer should sanity-check the *design*
  (candidate sizes, the induced-quota-failure mechanism, the churn-cycle
  count and soak-duration rationale) even before real VM execution, since a
  design flaw found only after burning VM time would be expensive to
  iterate on.
- The `VM-ENVIRONMENT-FINDING.md` evidence itself — an orchestrator/human
  should confirm whether the VM needs to be re-provisioned from scratch (a
  new, separate node) or whether a different, already-provisioned snapshot
  exists that this session simply failed to select.

## 9. Follow-up graph changes proposed

- A VM re-provisioning step (or a corrected snapshot selection) is needed
  before this node's remaining Section 8 rows 3–10 can run. This is
  environment/infrastructure work, not a code change to this repository,
  and is outside this node's ownership — the orchestrator should decide
  whether that is a new small node, a correction to this node's Section 3,
  or a human action outside the graph entirely.
- Once the VM is confirmed reachable, the next dispatch of this same node
  should: (1) run the Section 8 row 2 baseline to confirm nothing regressed;
  (2) run the new `secure_arena_host_bounds.rs` scenarios for real and
  record actual measured numbers (replacing the design rationale in this
  handoff with real evidence, including revising candidate sizes if reality
  disagrees with this session's estimates); (3) author and run
  `second_identity_dpapi.rs` now that the second account's real identity is
  discoverable; (4) complete rows 8–11.
- Section 9 (known gaps this VM-only node cannot close) from the task
  packet remains fully open and unaddressed this session, in addition to
  the new VM-provisioning gap above:
  1. Code signing — no certificate exists anywhere in this repo/environment.
  2. SBOM — no tool selected/introduced.
  3. Full installer/update packaging — out of this node's scope.
  4. Performance budgets as pass/fail thresholds — no numeric SLA exists yet;
     this node's soak/churn timing (once actually run) will produce the
     first real numbers, not clear a pre-existing bar.
  5. Full "Win10+11" coverage — the VM is Windows 11 only; no Windows 10 host
     available. Partial-coverage gap.
  6. Single-CPU VM constraint — this session's own investigation reinforces
     this: the VM's *actual* current configuration is 4 CPUs, which is
     itself evidence that "single-CPU" cannot yet be taken as a settled,
     verified property of this environment going into the next dispatch —
     that should be explicitly re-confirmed once the VM is reachable, not
     assumed from the packet text alone.
