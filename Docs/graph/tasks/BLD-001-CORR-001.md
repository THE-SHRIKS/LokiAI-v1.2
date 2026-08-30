# Task Packet — `BLD-001-CORR-001 Production Desktop/UI Isolation and Fail-Closed Correction`

Packet status: `accepted`
Graph status: `ready` — dispatching
Created by: `orchestrator`
Created at: `2026-08-21T00:00:00Z`
Starting revision: `776c946` (branch `LokiAI-v1.2`) plus the uncommitted `BLD-001`
worktree state at `L:\LokiAI-v1.2-wt\BLD-001` (new crate
`crates/loki-build-executor/` untracked, one workspace-members line staged in
root `Cargo.toml`; `Docs/graph/handoffs/BLD-001-1.md` untracked with three
prior dated sections: original S-1–S-6 remediation, the X:\ disposable-volume
human-directed verification pass, and the F-1/F-2 race-condition fix).

## 1. Objective

`BLD-001`'s independent security review (second pass, recorded in
`Docs/graph/handoffs/BLD-001-1.md`) judged the crate's one remaining gap —
sandboxed builds can open the interactive desktop, switch window stations,
create visible windows, and were not adversarially tested for input-hook,
screen-capture, or window-enumeration escapes — an acceptable, non-blocking,
documented follow-up. The human product owner has since required it be closed
now, before any acceptance decision, as a condition of the eventual merge
approval. This node exists solely to close that one gap for real, in the
crate's actual production sandbox launch path (not only test code), verified
in an environment outside this development host's own ambient restrictive
Windows Job Object (which is what prevented the original attempt from being
exercised at all), and to fix the already-known, already-scoped-down `F-2`
residual (`clear_inherit` partial-failure handle leak) in the same pass. This
packet does not reopen or silently modify the accepted `BLD-001` packet or its
existing handoff history; it adds new, dated evidence on top.

## 2. Traceability

- Requirements: `Docs/BUILD_EXECUTION.md` §4 ("headless (no Win32k/UI where
  compatible)") and §7 ("attempts to access ... UI ... fail (tested, not
  asserted from documentation)")
- Acceptance scenarios: `AS-012`/`AS-013` (containment/escape)
- Architecture components: `crates/loki-build-executor` Windows containment
  layer (`windows/{sandbox,job,acl,appcontainer,desktop,journal}.rs`)
- Security controls: AppContainer + Job Object + Window Station/Desktop
  isolation, fail-closed startup
- Parent/feature/incident: `BLD-001`; correction requested directly by the
  human product owner after reviewing `BLD-001`'s second independent security
  review verdict (`accept-with-documented-gap`)

## 3. Dependencies and inputs

- `depends_on`: `BLD-001`'s own dependencies (`JOB-001`, `CON-001`, `RUN-002`,
  all accepted) plus `BLD-001`'s own not-yet-accepted implementation state
  (this node corrects it in place, in the same worktree)
- Consumed contracts and exact versions/revisions: none beyond what `BLD-001`
  already consumes; no contract change
- Required environment/credential/device: a Windows process-launch context
  demonstrably outside this development host's ambient restrictive Job Object
  (clean VM preferred; failing that, an elevated-launch-then-standard-user-token
  external test runner, per Section 6 of this packet's dispatch instructions).
  No hardware device required. No credentials required — any elevation used is
  interactive UAC consent on the operator's own already-logged-in account, not
  a stored credential.
- Known starting conditions:
  - `crates/loki-build-executor/src/windows/desktop.rs` exists, compiles,
    passes `clippy`/`fmt`, and its two unit tests are honestly `#[ignore]`d —
    but it is **not called from any active code path**; `sandbox.rs`'s
    `setup_phase` never sets `STARTUPINFOEXW.lpDesktop`, so sandboxed
    processes currently inherit the launcher's own interactive desktop.
  - The prior attempt to wire it in was reverted specifically because
    `CreateWindowStationW` failed unconditionally on this development host,
    independent of any tool-level sandbox toggle, root-caused to this host's
    shell process tree already running inside an ambient Job Object that
    denies that call for any security descriptor, including a fully default
    one. This is a environment property of the development host used so far,
    not a property of a real production host.
  - `F-2`'s residual gap (the four `clear_inherit(...)?` calls in
    `setup_phase`, `sandbox.rs` ~line 888-891, leaking the two already-created
    pipes plus `job`/`completion_port` on a partial failure) was found and
    explicitly deferred, not fixed, in the prior pass.

## 4. Required reading

- `AGENTS.md` (all 13 rules; rule 1 and the containment/authority-boundary
  rules apply hardest)
- `Docs/BUILD_EXECUTION.md` in full, §4 and §7 specifically
- `Docs/SECURITY.md`
- `Docs/graph/tasks/BLD-001.md` (read-only — do not modify)
- `Docs/graph/handoffs/BLD-001-1.md` in full, all existing dated sections
  (read-only history — new work adds a new dated section, does not rewrite
  prior ones)
- `crates/loki-build-executor/src/windows/desktop.rs`,
  `windows/sandbox.rs`, `windows/job.rs`, `windows/acl.rs`,
  `windows/appcontainer.rs`, `bin/loki-build-executor.rs`,
  `bin/loki-test-fixture-toolchain.rs`,
  `tests/containment_windows.rs`, `tests/support/mod.rs`

## 5. Ownership

Writable paths:

- `crates/loki-build-executor/**`
- `Docs/graph/tasks/BLD-001-CORR-001.md` (this file)
- `Docs/graph/handoffs/BLD-001-CORR-001-1.md`
- a new dated section appended to `Docs/graph/handoffs/BLD-001-1.md` (append
  only — do not edit or remove any existing dated section)

Read-only dependency paths:

- all other workspace files, including `Docs/graph/tasks/BLD-001.md` (frozen)

Forbidden scope:

- weakening any already-verified containment property (AppContainer,
  Job Object, filesystem/ACL, network, named-pipe, device, process-tree,
  resource-limit, or fail-closed startup denial) to make desktop/UI isolation
  pass;
- any mock, simulated Win32 behavior, documentation-only assertion, fake
  process, or boolean/return-value-only test standing in for a real
  adversarial check;
- any test-only hook, `cfg(test)` shortcut, or environment variable that can
  alter production sandbox behavior (test-only path *selection*, e.g. the
  existing scratch-root/toolchain-path overrides already in
  `tests/support/mod.rs`, is fine and precedented; a hook that changes what
  the *production* code path actually enforces is not);
- modifying ACLs on `L:\`, `C:\`, `S:\`, either repository root, or any
  existing worktree other than this task's own disposable test infrastructure;
- modifying the accepted `Docs/graph/tasks/BLD-001.md` packet or rewriting any
  existing dated section of `Docs/graph/handoffs/BLD-001-1.md`;
- merging this worktree's branch into `LokiAI-v1.2` or marking any node
  `accepted` — that decision is reserved to the human product owner for this
  node, explicitly, regardless of how this node's own verification concludes.

No worker may expand ownership without orchestrator acceptance.

## 6. Deliverables

1. Desktop/Window-Station isolation wired into the real production sandbox
   launch path (`windows/sandbox.rs`'s `setup_phase`/`run_with_attestation`),
   not only test code — every build launched through
   `loki_build_executor::windows::sandbox::run` gets a per-job, non-interactive
   Window Station + Desktop, or the build does not start.
2. Fail-closed behavior: if the Window Station/Desktop cannot be created, the
   build must not reach `ResumeThread`; it must return a bounded, honest
   `OutcomeUnknown`/failure result (matching this crate's existing
   `SetupTimeout`/`ContainmentSetupFailed` pattern), with the same
   `SetupSuccessGuard`-style unconditional teardown discipline already
   established for the `F-1` fix — no leaked process, handle, ACL entry,
   AppContainer profile, desktop, window station, job, pipe, journal entry, or
   temporary file on any failure path.
3. `F-2`'s residual `clear_inherit` partial-failure gap fixed, matching the
   `create_four_inheritable_pipes`/`close_pipe` cleanup discipline already
   used elsewhere in the same function.
4. A real, separately-compiled adversarial fixture (extending the existing
   `loki-test-fixture-toolchain` pattern — real Win32 calls, no mocks)
   genuinely attempting, from inside the production sandbox path, and proven
   denied: opening the interactive user desktop; opening/switching window
   stations; creating a visible window; enumerating/messaging unrelated user
   windows; reading/writing the clipboard; installing input hooks or sending
   simulated input; capturing screen/window pixels; escaping through a child
   or grandchild process.
5. A deliberate fault-injection test proving fail-closed behavior when
   Window Station/Desktop creation is forced to fail before process resume:
   no process starts, the job reaches a bounded honest outcome, and every
   resource is confirmed torn down.
6. A discriminating regression test for the `F-2` fix (fails with the fix
   disabled, passes with it restored — same rigor as the existing `F-1`
   regression test), with the deliberate fault removed before the final run.
7. Verification executed in an environment independently confirmed to be
   outside this development host's ambient restrictive Job Object, with that
   confirmation itself captured as evidence (not asserted).
8. A new dated section appended to `Docs/graph/handoffs/BLD-001-1.md`, plus
   this node's own handoff at `Docs/graph/handoffs/BLD-001-CORR-001-1.md`,
   with full command output, before/after discrimination evidence for every
   new test, and an honest accounting of any remaining gap.
9. A follow-up independent `loki-security-reviewer` pass against the actual
   production wiring (not the test-only path), confirming no fail-open path
   and no test-only hook affecting production behavior.

## 7. Acceptance criteria

- [ ] Every build launched through the real production sandbox entry point
      gets enforced, fail-closed desktop/window-station isolation — verified
      by a real adversarial executable, not a mock or a boolean check.
- [ ] If isolation cannot be created, the build does not start, returns a
      bounded honest failure, and leaves no process/handle/ACL/profile/
      desktop/window-station/job/pipe/journal/temp-file behind — proven by a
      deliberate fault-injection test, not asserted.
- [ ] `F-2`'s `clear_inherit` gap is fixed with a discriminating regression
      test.
- [ ] No previously-verified containment property regresses (filesystem/ACL,
      network, named-pipe, device, process-tree, resource-limit, fail-closed
      startup, the three drive-canary tests).
- [ ] Every new security-relevant test is proven discriminating: fails with
      the protection deliberately disabled, passes with it restored, and the
      deliberate fault is removed before the final reported run.
- [ ] Verification ran in an environment independently confirmed outside this
      host's ambient Job Object, with that confirmation captured as evidence.
- [ ] Full production verification suite passes with no mandatory check
      ignored, skipped, mocked, or reported as passing indirectly: workspace
      build, `fmt`, `clippy -D warnings`, all unit/recipe-contract/
      containment/end-to-end tests, all three drive-canary tests, process/
      handle/ACL cleanup checks, the full desktop/UI adversarial suite.
- [ ] An independent security reviewer inspects the actual production wiring,
      independently reruns the complete suite, checks for fail-open paths,
      and confirms no test-only hook affects production behavior.
- [ ] Disposable test infrastructure is cleaned up after evidence is
      preserved; no process, ACL entry, AppContainer profile, VHDX, canary
      file, or temporary security object remains.
- [ ] This worktree's branch is **not** merged into `LokiAI-v1.2` and no node
      is marked `accepted` by this node's own work — the final report goes to
      the human product owner for an explicit merge decision.
- [ ] No unrelated files change; no existing dated handoff section is
      rewritten; `Docs/graph/tasks/BLD-001.md` is untouched.

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `cargo build --workspace` | exit 0 | command/result |
| Static | `cargo fmt -p loki-build-executor -- --check` | exit 0 | command/result |
| Static | `cargo clippy -p loki-build-executor --all-targets -- -D warnings` | exit 0, no warnings | command/result |
| Unit | `cargo test -p loki-build-executor --lib -- --test-threads=1` | all pass | counts/timing |
| Contract | `cargo test -p loki-build-executor --test recipe_contract -- --test-threads=1` | all pass | counts/timing |
| Containment | `cargo test -p loki-build-executor --test containment_windows -- --test-threads=1` | **every** test passes, including the new desktop/UI adversarial suite and the fault-injection test — zero ignored | counts/timing, real stdout per sub-check |
| Integration | `cargo test -p loki-build-executor --test stages_end_to_end -- --test-threads=1` | all pass | counts/timing |
| Containment | the three drive-canary tests (`L:`/`C:`/`S:`) | all pass, zero ACL mutation on the drives themselves | counts, before/after ACL diff |
| Desktop | real adversarial fixture: desktop open, window-station switch, window creation, window enumeration/messaging, clipboard, input hook/`SendInput`, screen/window capture, child/grandchild escape | every sub-check denied | real stdout per sub-check, not a boolean |
| Fail-closed | deliberate Window Station/Desktop creation failure forced before `ResumeThread` | build never starts; bounded honest outcome; zero leaked resources | before/after process/handle/ACL enumeration |
| Regression discrimination | each new test run with its protection deliberately disabled, then restored | fails disabled, passes restored | paired command output for each test |
| Environment | proof of execution outside this host's ambient Job Object | `IsProcessInJob`-equivalent check (or `Get-Process`/job query) at the point of test execution | captured check output |
| Cleanup | post-run process/ACL/AppContainer-profile/VHDX/canary/temp-object enumeration | zero residue | enumeration output |
| Security | independent `loki-security-reviewer` pass against production wiring | no fail-open path; no test-only hook affects production; explicit verdict | written review |

If a mandatory row cannot run, the node result cannot be `COMPLETE`.

## 9. Risks and safety

- Risk level: `high` (Windows security-object creation, fail-closed startup
  correctness, and an escape from this development host's own tool sandbox
  are all safety-relevant; a mistake here could either leave real containment
  weaker or leave dangling elevated/standard-user processes on the test host)
- Failure modes: desktop/window-station isolation cannot be exercised in any
  available environment (report honestly, do not simulate); the environment-
  escape method itself leaves an orphaned elevated or standard-user process;
  a discriminating test is later found not to actually discriminate (as
  already happened once in this task and was self-caught — apply the same
  rigor here from the start); a fix weakens an already-verified property to
  make a new one pass.
- Rollback/recovery: this is an uncommitted worktree; a bad attempt can be
  discarded without touching any accepted baseline. No merge occurs regardless
  of outcome — reserved to the human product owner.
- Data/secret impact: none. No credential is created, stored, or transmitted;
  any elevation is interactive UAC consent on the operator's existing session.
- Executable/toolchain impact: one new adversarial fixture binary/mode set,
  test-only, matching the existing `loki-test-fixture-toolchain` pattern —
  `Docs/RELEASE.md` packaging must still exclude it (unchanged existing note).
- Cloud/offline impact: none.
- Device impact: none. No real device operation occurs.

## 10. Human gate

- Gate: `merge/accept decision`
- Exact decision/operation requiring approval: merging this worktree's branch
  into `LokiAI-v1.2` and marking `BLD-001`/`BLD-001-CORR-001` `accepted`.
- Approver role: human product owner (explicit instruction for this node).
- Approval evidence: to be recorded only after the human reviews this node's
  final report and explicitly approves — not before, and not implied by a
  clean verification result on its own.

## 11. Worker and reviewer

- Accountable worker: `loki-implementer`, same worktree
  `L:\LokiAI-v1.2-wt\BLD-001`, branch `BLD-001` (uncommitted correction on top
  of the existing uncommitted `BLD-001` implementation state)
- Independent reviewer: `loki-security-reviewer`, dispatched after
  implementation, against the actual production wiring
- Worktree/branch: `L:\LokiAI-v1.2-wt\BLD-001` / `BLD-001`
- Handoff record: `Docs/graph/handoffs/BLD-001-CORR-001-1.md` plus a new dated
  section in `Docs/graph/handoffs/BLD-001-1.md`

## 12. Stop conditions

Return `BLOCKED_DEPENDENCY` or `NEEDS_REPLAN` if:

- no environment can be obtained that is independently confirmed outside this
  host's ambient Job Object (report exactly what was tried and what a real
  production host would need, rather than simulating the result);
- closing the desktop/UI gap is only achievable by weakening an
  already-verified containment property;
- the fail-closed teardown guarantee cannot be proven without leaving a real
  leaked resource in at least one tested failure path;
- verification requires expanding scope outside `crates/loki-build-executor/**`.
