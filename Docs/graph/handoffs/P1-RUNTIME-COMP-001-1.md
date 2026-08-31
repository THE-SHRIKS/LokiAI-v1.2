# Worker Handoff — `P1-RUNTIME-COMP-001`

Result: `ACCEPTED-pending-review`
Worker: `loki-implementer`
Started from: `LokiAI-v1.2`@`84168b5` (task packet's cited starting revision; branch tip at dispatch)
Rebased onto: `LokiAI-v1.2`@`41178b7` (integrates `P1-IPC-SHUTDOWN-CORR`, the fix for the
shutdown-lifecycle crash this node's own first attempt discovered and reported as
`NEEDS_REPLAN` — see §7 decision 1 and the superseded evidence note below)
Result revision/patch: worktree `node/P1-RUNTIME-COMP-001`, commit `1fea3cd`
(rebase-and-fix commit `1fea3cd` on top of the rebased implementation commit `0317582`)
Completed at: `2026-08-31T18:30:00Z`

**Supersedes** the first attempt's handoff content (previously `NEEDS_REPLAN`,
reported before `P1-IPC-SHUTDOWN-CORR` was integrated). That report's crash
finding was independently confirmed correct and fixed by the orchestrator's
dispatch of `P1-IPC-SHUTDOWN-CORR`. This revision of the handoff documents the
rebase, the reconciliation of this node's own `compose.rs` change against that
fix, two additional (unrelated) test bugs discovered and fixed during
re-verification, and the now-fully-passing evidence.

## 1. Outcome

The real `RuntimeBusinessService` (Deliverables 1–6, 9) is implemented, wired
into `ComposedHost::open_with_arena` for production `open()`, and proven by a
real `ComposedHost`-level integration suite (Deliverable 7) driven through the
actual `ComposedHost::run` shutdown/drain path — not a second, independently
bound `Server`. The two previously-crashing suites
(`tests/business_service.rs`, this node's own; `tests/host_lifecycle.rs`,
pre-existing and unmodified) both pass consistently (verified across 3
repeated runs). All 8 in-scope methods, the Deliverable-3 kind-check gate
(proven by a real adversarial test), the 3 explicitly-deferred desktop-mutation
methods, and the restart/reconcile scenario are all exercised and pass. The
new black-box test in `tests/process_behavior.rs` (Deliverable 8) remains
`#[ignore]`d, citing `DAT-002-MEM-HOST-VERIFY-001`, matching the two
pre-existing ignored tests exactly.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `cargo build --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` zero warnings | pass | §4 |
| `cargo test -p loki-runtime -- --test-threads=1` passes, including new `tests/business_service.rs` | pass | §4 — 76/76 across lib + all 4 integration-test binaries |
| `cargo test -p loki-ipc -p loki-storage -p loki-mutation-contract -- --test-threads=1` unchanged, still green | pass | §4 |
| Real client receives real results for the 8 in-scope methods over a real pipe against a real `ComposedHost` | pass | `startup_serves_ping_all_four_reads_and_all_three_agent_mutations_over_a_real_pipe` |
| `RestartRequest`/`FlashRequest`/`BuildRequest` return `UnsupportedMethod` | pass | `desktop_mutation_methods_remain_unsupported_even_for_a_fully_authorized_principal` |
| Deliverable-3 security check proven by adversarial test | pass | `desktop_principal_colliding_with_a_real_connection_id_is_denied_not_served` — see §7 decision 3 for a fixture bug found and fixed along the way |
| Restart/reconcile scenario passes | pass | `restart_reconcile_preserves_state_and_serves_a_fresh_mutation_through_the_real_business_rpc_path` — real clean shutdown, real reopen at the same on-disk paths, real replay + fresh mutation, all through the real business-RPC path |
| No `MutationAcceptanceRequest` ever constructed with a fabricated `grant_generation` | pass | Single construction site in `business_service.rs`'s `agent_mutation`; `connection.generation` traces directly to `Storage::get_agent_connection(...).generation` — grep-verifiable, no literal/default |
| `git diff --stat` shows changes only under §5 writable paths | pass | §3 |
| `open_with_arena`'s only behavioral change is installing the real business service | see §7 decision 2 | Necessarily broader than a single call-site swap once reconciled with `P1-IPC-SHUTDOWN-CORR`'s own `business_override` mechanism — recorded as a decision, not silently absorbed |
| Two pre-existing `#[ignore]`d tests in `process_behavior.rs` remain `#[ignore]`d and unmodified | pass | §3 — this node's new third test is additive, `#[ignore]`d the same way |
| Independent reviewer sign-off, no Critical/High finding | not yet run | Requested by this handoff; see §8 review focus |
| Handoff names the 3 deferred methods and follow-up node | pass | §9 |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-runtime/src/host/business_service.rs` (new) | Real `RuntimeBusinessService` implementing `loki_ipc::server::BusinessService`: 4 real reads via `RuntimeReadPort`, 3 agent-mutations via `MutationSubmissionPort` with the mandatory `PrincipalKind::AgentConnection` kind-check gate, explicit `UnsupportedMethod` arms for `RestartRequest`/`FlashRequest`/`BuildRequest`, and a wildcard `UnsupportedMethod` for the 13 permanently-unsupported methods and all streams. Unchanged since the first attempt. | yes — new-file path named in §5 |
| `crates/loki-runtime/src/host.rs` | Added `pub mod business_service;` and a module-map doc-comment entry. Unchanged since the first attempt. | yes |
| `crates/loki-runtime/src/host/compose.rs` | Rebased and reconciled against `P1-IPC-SHUTDOWN-CORR`'s own `compose.rs` changes (removed `IPC_SHUTDOWN_GRACE`, `run()` now directly `.await`s `ipc_task`, and a new `open_with_arena` `business_override: Option<BusinessOverride>` parameter plus `open_with_test_arena_and_business_service`). Introduced a 3-way `BusinessOverride` enum (`None`/`CredentialOnly`/`Full`) so production `open()`/`open_with_test_arena()` install the real `RuntimeBusinessService` (closing the gap where `P1-IPC-SHUTDOWN-CORR`'s own fix had reverted to the fail-closed default, since this node's change was not yet integrated when that fix was written), added `open_with_test_arena_and_credential_resolver` (this node's own new test-only seam: real business service, test-supplied credential resolver), and preserved `open_with_test_arena_and_business_service` unchanged in behavior (still used by `tests/host_lifecycle.rs`'s `SlowBusinessService` fixture). Removed the `storage_handle()`/`services_handle()` accessors the first attempt added — no longer needed now that tests drive real calls through `ComposedHost::run()` itself. See §7 decisions 1–2. | yes — same file, no edit to migration/discovery/token/pipe-bind order/behavior |
| `crates/loki-runtime/tests/business_service.rs` (new) | Rewritten to drive every scenario through the real `ComposedHost::run` (spawn, connect, call, signal shutdown, await to completion) via `open_with_test_arena_and_credential_resolver`, instead of a second `Server` bound outside that lifecycle — matching the testing approach `P1-IPC-SHUTDOWN-CORR`'s own re-verification established. Fixed two real test-fixture bugs found via targeted debugging (§7 decision 3). All 5 tests pass, confirmed stable across 3 repeated runs. | yes — new-file path named in §5 |
| `crates/loki-runtime/tests/process_behavior.rs` | Additive only: one new `#[ignore]`d black-box test, citing `DAT-002-MEM-HOST-VERIFY-001` in the exact same style as the two existing tests, which are unmodified. Unchanged since the first attempt. | yes — additive-only, per §5 |
| `Docs/graph/handoffs/P1-RUNTIME-COMP-001-1.md` (this file) | Own handoff, updated in place after the rebase | yes |

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | Windows 11, workspace toolchain | exit 0 | ran clean after `cargo fmt --all` |
| `cargo clippy --workspace --all-targets -- -D warnings` | same | exit 0, zero warnings | full workspace check, post-rebase |
| `cargo build --workspace` | same | exit 0 | only pre-existing, unrelated `openssl-sys` static-link `LNK4099` PDB warnings |
| `cargo test -p loki-runtime -- --test-threads=1` | same | 76 passed, 0 failed, 3 ignored (pre-existing `DAT-002-MEM-HOST-VERIFY-001`-gated black-box tests, unchanged) | lib (62) + `tests/business_service.rs` (5) + `tests/host_lifecycle.rs` (5) + `tests/process_behavior.rs` (4 run + 3 ignored) + `tests/secure_memory_arena_runtime.rs` (1) |
| `cargo test -p loki-ipc -p loki-storage -p loki-mutation-contract -- --test-threads=1` | same | all green, 0 failed across every suite | regression check — none of these crates' behavior was touched |
| `cargo test -p loki-runtime --test business_service --test host_lifecycle -- --test-threads=1`, repeated 3x | same | 5/5 and 5/5 pass every time, no crash | flake-check specifically targeting the previously-crashing suites |
| `cargo test -p loki-runtime --test host_lifecycle -- --test-threads=1` on the **pristine pre-`P1-RUNTIME-COMP-001` tree** (`git stash`, first attempt's own control) | same | exit 0, 4/4 passed | established in the first attempt as the crash-attribution baseline; not re-run this round since the crash itself is now independently fixed and re-confirmed below |

Mandatory verification not run:

- Independent test-reviewer / security-reviewer sign-off (§7/§8 of the
  packet): requested by this handoff (§8); not yet performed.

## 5. Contracts, migrations, and compatibility

- Contracts changed: none in `loki-ipc`'s `BusinessService`/`ServerConfig`
  public shape (consumed exactly as `P1-IPC-SHUTDOWN-CORR` left it —
  `with_credential_resolver`/`with_business_service` signatures unchanged).
  This node's own `compose.rs` gained two new test-only public methods
  (`open_with_test_arena_and_credential_resolver`), both `#[cfg(any(test,
  feature = "test-support"))]`-gated, zero production reachability.
- Consumers affected: none yet (no caller wiring in this node — out of
  scope per §5's forbidden list).
- Migration: none; no schema change.
- Backward/rolling compatibility: n/a.

## 6. Security, privacy, and device safety

- Trust boundaries touched: the local authenticated IPC transport
  (`loki-ipc`), now serving real storage-backed business RPC results.
- Authorization/approval impact: Deliverable 3's kind-check gate is
  implemented as a hard, unconditional, early return at the top of
  `agent_mutation` in `business_service.rs` — before any storage lookup,
  before any other field of `principal` is consulted. Proven by a real
  adversarial test now genuinely passing (see §7 decision 3 for the fixture
  bug that previously made this untestable — the fix was to the *test's*
  `CredentialSet` construction, not to the production kind-check code, which
  was correct throughout and unchanged since the first attempt).
- No `MutationAcceptanceRequest` is ever constructed with a fabricated
  `grant_generation` — unchanged since the first attempt, re-confirmed by
  passing tests this round.
- `RestartRequest`/`FlashRequest`/`BuildRequest` are explicit, dedicated
  match arms (not a wildcard) returning `RpcError::UnsupportedMethod`, now
  proven by a real, passing test (previously blocked by the crash).
- **The shutdown-lifecycle finding from this node's first attempt is now
  fixed and independently re-verified here**: `tests/host_lifecycle.rs`'s
  `startup_accepts_a_real_ipc_connection_and_is_located_as_healthy` (the
  exact test that reproduced the original `STATUS_ACCESS_VIOLATION`) now
  passes, confirmed stable across 3 repeated runs, alongside this node's own
  `tests/business_service.rs` suite which drives multiple real connections
  and a real restart through the same fixed path.
- Device impact: none — no connector/device operation is reachable through
  the 8 in-scope methods.
- New dependencies: none.
- Remaining risk: none identified beyond the standard need for independent
  review (§8).

## 7. Decisions and assumptions

1. **Rebase reconciliation of `compose.rs`'s `Arc`-wrapping against
   `P1-IPC-SHUTDOWN-CORR`'s own changes to the same file.** `P1-IPC-SHUTDOWN-CORR`
   was written against a `compose.rs` that did not yet have this node's
   Deliverable-1 `Arc`-wrapping (this node was not yet integrated), so its
   own `open_with_arena` still used `RuntimeServices` by value and a
   `business_override: Option<(CredentialResolver, BusinessService)>` tuple
   that, when `None`, left production `open()` using the fail-closed
   `UnsupportedBusinessService` default. Rebasing required reconciling both:
   kept this node's `Arc<RuntimeServices>` field (needed so
   `RuntimeBusinessService` and `ComposedHost` share one composed instance,
   never a second `RuntimeServices::compose` call), and replaced the tuple
   `Option` with a `BusinessOverride` enum (`None`/`CredentialOnly`/`Full`)
   so that `None` (production `open()`, ordinary-test `open_with_test_arena()`)
   now installs the real `RuntimeBusinessService` — restoring this node's
   own Deliverable 6 intent, which `P1-IPC-SHUTDOWN-CORR`'s own commit had
   (correctly, given what it could see at the time) left as a gap.
   `open_with_test_arena_and_business_service` (used by
   `tests/host_lifecycle.rs`'s `SlowBusinessService` fixture, not owned by
   this node) is preserved with an unchanged public signature and identical
   behavior (`BusinessOverride::Full`).
2. **New test-only seam `open_with_test_arena_and_credential_resolver`.**
   This node's own tests need the real `RuntimeBusinessService` (under test)
   with a test-constructed `CredentialSet` (so a real, authorized principal
   can exercise it) — a combination `open_with_test_arena_and_business_service`
   does not offer (it replaces the business service too). Added as a third,
   `test-support`-gated constructor (`BusinessOverride::CredentialOnly`),
   narrowly additive, zero production reachability, following the exact
   precedent `open_with_test_arena_and_business_service` itself set.
   `storage_handle()`/`services_handle()` (added in the first attempt to
   support a since-abandoned "bind a second `Server`" test design) were
   removed as no longer needed — the diff to `compose.rs` is smaller in this
   revision than the first attempt's, not larger, despite the new
   constructor.
3. **Two real test-fixture bugs found and fixed during re-verification, both
   unrelated to `RuntimeBusinessService`'s own (unchanged, correct)
   production logic** — found by targeted `eprintln!` bisection, not
   guessed:
   - The Deliverable-3 adversarial `colliding_desktop_principal` and the
     real `agent_principal` were placed in the *same* `CredentialSet` while
     deliberately sharing one `principal_id` (the entire point of the
     adversarial scenario). But `crate::server::SessionAuthority`
     revalidates every call by `principal_id` alone (never the raw token —
     see that module's own "dynamic session revalidation" doc comment), so
     `CredentialSet::revalidate`'s lookup (a `.find()` over its records) is
     only well-defined when `principal_id` is unique *within one resolver*.
     With two different principals sharing an ID in the same resolver,
     `revalidate()` silently returned the first-registered record (the real
     agent principal), so the call under test was actually served/`Accepted`
     — the assertion failure this produced (before the fix) was proof the
     *test* was checking the wrong thing, not that the kind-check gate had
     failed. Fixed by giving every test its own minimal resolver containing
     only the principal(s) it actually needs, so a `principal_id` shared
     with a real, seeded `connection_id` (the property Deliverable 3 must
     defend against) never also collides with another *resolver record*.
   - The restart/reconcile test issued `DeviceGet` using the agent
     principal, which correctly does not hold `Authority::ReadDevices` (a
     real agent should not have blanket device-read authority) — so
     `authorize_method` correctly denied it, every time, and the test's own
     "must succeed" assertion was simply wrong. Fixed by adding a second,
     desktop-principal connection for that one read.
   - Both bugs caused an assertion panic *before* `stop_host()`'s clean
     shutdown could run, abandoning a live `ComposedHost::run()` task
     (holding the process-global secure-memory arena) when the failing
     test's own per-test Tokio runtime tore down at panic-unwind — a
     second, distinct way to reach a real `STATUS_ACCESS_VIOLATION`, which
     is why these two bugs initially looked like a recurrence of the
     original, already-fixed crash rather than what they actually were:
     ordinary test-fixture defects. Isolated by bisecting with `eprintln!`
     checkpoints (single-call reproduction, tracing exactly which call
     returned an unexpected `Ok`/`Err` before the process died) rather than
     assumed.

## 8. Review focus

- `crates/loki-runtime/src/host/business_service.rs`'s `agent_mutation`
  function: confirm the `PrincipalKind::AgentConnection` gate is truly
  unconditional and precedes the storage lookup (Deliverable 3's core
  property) — unchanged since the first attempt, now backed by a genuinely
  passing adversarial test.
- `crates/loki-runtime/src/host/compose.rs`'s `BusinessOverride` enum and
  `open_with_arena`'s match arms: confirm `None` and `CredentialOnly` both
  install the real `RuntimeBusinessService` (never a stub) and that `Full`'s
  behavior for `tests/host_lifecycle.rs`'s existing `SlowBusinessService`
  fixture is genuinely unchanged (not something this node silently altered).
- The two test-fixture bugs in §7 decision 3: confirm the fix was to test
  construction only, with zero production-code changes in the same diff.
- `tests/business_service.rs`'s `restart_reconcile_preserves_state_and_serves_a_fresh_mutation_through_the_real_business_rpc_path`:
  confirm it genuinely reopens the same on-disk state (same `HostConfig.paths`,
  fresh pipe name only) via a real, clean `ComposedHost::run` shutdown
  followed by a real reopen — not a fresh/different store.
- No `MutationAcceptanceRequest` construction site uses a fabricated/default
  `grant_generation` — grep-verifiable, unchanged since the first attempt.

## 9. Follow-up graph changes proposed

- Previously-named, unaffected follow-up still applies as originally scoped:
  a not-yet-created node (working title `P1-RUNTIME-COMP-DESKTOP-MUTATION-001`)
  for `RestartRequest`/`FlashRequest`/`BuildRequest`, gated on a
  not-yet-opened decision covering how a Hello-authenticated
  `DesktopInstallation` IPC principal is bound to a real `LocalAuthority`
  session/generation. This node's own scope deliberately excludes all three,
  per §1/§9/§11 of the task packet.
- The first attempt's proposed blocking follow-up (working title
  `P1-RUNTIME-COMP-SHUTDOWN-CORR`) is now superseded/closed: the orchestrator
  dispatched and integrated `P1-IPC-SHUTDOWN-CORR`, which fixes exactly the
  gap that finding named. No further action needed on that thread beyond the
  independent review this handoff itself requests.
- Only the orchestrator updates the task graph; this section records
  proposals only.
