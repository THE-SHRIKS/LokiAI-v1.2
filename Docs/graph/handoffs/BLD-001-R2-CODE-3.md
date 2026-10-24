# Worker Handoff — `BLD-001-R2-CODE`

Result: `BLOCKED_DEPENDENCY`
Worker: isolated build-containment code implementer (Claude Code)
Started from: `7a1115f5b291c71c4ffb0e13deee8d1af9654268` (packet decision baseline `0bd4a7041f3d01c5b419feccc0d48c3198917ac4`)
Result revision/patch: uncommitted owned patch in `codex/bld-001-r2-code`, building on the unchanged attempt-1/attempt-2 implementation plus the security fixes described here
Completed at: `2026-08-25T00:00:00Z`

## 1. Outcome

I fixed both findings from the independent security review of this exact
patch. High: `CapsuleBroker::new` now inspects the last journal entry's
`intent` flag on reconstruction and forces the in-memory state to
`Quarantined` whenever an intent was journaled without a matching
completion/failure record, so a crash between journaling an action's intent
and recording its native result can never be replayed into a silent
`Ok(complete)` for the three actions whose intent and complete states
coincide (`AuthorizeExecution`, `QuarantineOutput`, `Reconcile`). Medium:
`appcontainer_sid_digest` is now part of the `bind_or_verify` identity
invariant (bound at `Create`, rejected on mismatch exactly like
`job_id`/`capsule_id`/`sealed_base_digest`) and is now persisted and
hash-chained in every `JournalEntry`. I evaluated the Low/hygiene
MAC-ordering finding in `protocol.rs::Authenticator::verify_and_consume` and
deliberately did not apply it — see section 7 — per the explicit "use
judgment, don't risk breaking anything for this one" instruction. All six
packet-mandated crate-scoped commands pass for real, as a standard user, with
four new tests proving both fixes. The result stays `BLOCKED_DEPENDENCY`: it
still requires the independent reviewer to confirm no critical/high finding
remains, and the workspace-wide gates (`cargo clippy --workspace`,
`pnpm run verify`) remain blocked by the pre-existing, unrelated `openssl-sys`
Application Control policy failure already documented in
`verification-rerun-2026-08-25.md` and unaffected by this change.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Lifecycle transitions explicit, idempotent where retryable, journaled before mutation, quarantine on uncertainty (now including reconstruction-time uncertainty) | pass | `broker.rs::CapsuleBroker::new` forces `Quarantined` on dangling intent; 3 new `broker_contract` tests |
| Broker messages reject cross-job/capsule/base/AppContainer-SID substitution | pass | `broker.rs::bind_or_verify`; `journal.rs::validate_next`; `appcontainer_sid_digest_substitution_is_rejected` test |
| Tests invoke no VHDX/disk/elevation/install/sign/device/host-ACL operation | pass | new tests use only `MemoryJournal`/`RecordingApi`/`HostOperationsDisabled`; no host API added |
| No prohibited boundary weakened; no unrelated files changed | pass | diff confined to `crates/loki-build-executor/**` and this node's own evidence/handoff paths (see section 3) |
| Independent security review has no critical/high finding | blocked | this pass responds to the review's two findings; a fresh independent pass to confirm remains outstanding |
| Workspace-wide gates (`cargo clippy --workspace`, `pnpm run verify`) exit 0 | blocked (environment, pre-existing, unrelated) | unchanged from `verification-rerun-2026-08-25.md`; not rerun this pass since nothing touches that dependency chain |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-build-executor/src/broker.rs` | High fix: reconstruct-time dangling-intent detection forces `Quarantined`; Medium fix: `appcontainer_sid_digest` added to the bound-identity struct, `bind_or_verify`, and `append` | yes |
| `crates/loki-build-executor/src/journal.rs` | Medium fix: `appcontainer_sid_digest` added to `JournalEntry`/`JournalEntryDraft`, `calculate_hash`, and `validate_next`'s cross-entry identity check; existing unit-test fixtures updated for the new required field | yes |
| `crates/loki-build-executor/tests/broker_contract.rs` | 4 new hostile tests: 3 crash/reconstruction cases (one per same-state action) and 1 AppContainer-SID substitution case | yes |
| `Docs/graph/evidence/W5/BLD-001-R2-CODE/security-fix-2026-08-25.md` | new evidence record for this fix pass | yes |
| `Docs/graph/handoffs/BLD-001-R2-CODE-3.md` | this handoff | yes |

No file outside `crates/loki-build-executor/**` and this node's own
evidence/handoff paths was modified. `Cargo.toml`/`Cargo.lock` are unchanged
from the prior attempt (no new dependency was needed for either fix).
`protocol.rs` is unchanged (see section 7 for why the Low finding was not
applied there).

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | standard user, worktree `TEMP`/`TMP`/`CARGO_TARGET_DIR` | exit 0 | `security-fix-2026-08-25.md` |
| `cargo clippy -p loki-build-executor --all-targets -- -D warnings` | same | exit 0, zero warnings | same |
| `cargo test -p loki-build-executor --lib -- --test-threads=1` | same | 11 passed, 0 failed | same |
| `cargo test -p loki-build-executor --test broker_contract -- --test-threads=1` | same | 12 passed, 0 failed (8 pre-existing + 4 new) | same |
| `cargo test -p loki-build-executor --test broker_dry_run -- --test-threads=1` | same; `HostOperationsDisabled`/deny-all adapter | 2 passed, 0 failed | same |
| `cargo test -p loki-build-executor --test native_boundary -- --test-threads=1` | same; in-memory typed ledger | 1 passed, 0 failed | same |

Mandatory verification not run:

- independent `BLD-001-R2-CODE-REV-001` re-review of this exact fix — required
  before `COMPLETE`; not something this worker (the implementer) can perform;
- `cargo clippy --workspace --all-targets -- -D warnings` and `pnpm run verify`
  — not rerun this pass; both were already confirmed blocked by a pre-existing
  `openssl-sys` Application Control policy failure unrelated to this crate or
  this fix (see `verification-rerun-2026-08-25.md`); nothing in this pass
  touches TLS/`openssl-sys` or any other crate;
- real containment/L8/VHDX/signed-broker/host matrix — correctly reserved for
  `BLD-001-R2-HOST-VERIFY`, not a skipped CODE test.

## 5. Contracts, migrations, and compatibility

- Contracts changed: the private build-broker journal record now includes a
  required `appcontainer_sid_digest: DigestId` field. This is a breaking
  change to the journal record shape, but there is no released predecessor
  and no running consumer of this unreleased contract; `deny_unknown_fields`
  already rejected unrecognized/missing-field journal records before this
  change, so any hand-authored fixture journal without the new field would
  already have failed deserialization the same way it does now (field
  presence is enforced by serde's default "missing field" error either way).
- Consumers affected: none currently; a future runtime coordinator and signed
  host broker only.
- Migration: none; no journal files exist outside this crate's own tests and
  fixtures.
- Backward/rolling compatibility: n/a, unreleased contract.
- Generated-contract drift: n/a; private Rust contract with hostile
  round-trip tests, not a generated/shared schema.
- Build recipe/executor/provenance compatibility: unaffected; `recipe.rs` was
  not touched.
- Cloud API/offline/notification compatibility: n/a.

## 6. Security, privacy, and device safety

- Trust boundaries touched: the runtime-to-build-broker request boundary and
  the durable journal record shape. No new boundary was crossed; both fixes
  make an existing boundary strictly more restrictive (more identity fields
  bound; a previously silent success path now fails closed).
- Authorization/approval impact: none. No policy, MAC key, or authentication
  requirement changed.
- Secrets/data handling: unchanged; only synthetic fixture digests
  (`digest('9')`, etc.) appear in new tests.
- Containment profile and denied authorities: unchanged; `containment.rs` was
  not touched by this fix pass.
- Device side effects performed: none. No device, USB, serial, VHDX, disk, or
  elevation API was invoked by this worker or by any new/changed test — all
  new tests use `MemoryJournal`, `RecordingApi`, or `HostOperationsDisabled`.
- New dependencies: none.
- Remaining risk: the concrete signed Win32/VHDX broker implementation and
  real adversarial containment remain entirely unimplemented/unverified by
  design; nothing in this handoff should be read as evidence toward that
  separately gated work. The Low/hygiene MAC-ordering finding in
  `protocol.rs` remains open as documented technical debt (see section 7);
  it does not weaken the current fail-closed behavior, only its failure-mode
  attribution.

## 7. Decisions and assumptions

- **High fix design choice**: on detecting a dangling intent at
  reconstruction, I force `state = CapsuleState::Quarantined` rather than
  attempting to "replay" the interrupted `apply()`/native call automatically.
  `Quarantined` is already `is_terminal()` in this file, so this uses the
  exact same fail-closed mechanism the file already applies to synchronous
  native failures (`broker.rs` lines ~87-99, unchanged), rather than
  introducing a second, divergent "resume" code path. This necessarily also
  blocks a subsequent `Reconcile` from proceeding through `handle()` (because
  `is_terminal()` is checked before the action-specific logic), which mirrors
  the pre-existing behavior for a capsule quarantined by a synchronous native
  failure — `Reconcile` was already unreachable through `handle()` once a
  capsule reaches `Quarantined` before this fix, so this is a consistency
  fix, not a new restriction. I flag this pre-existing `Reconcile`-through-
  `handle()` dead path as a candidate follow-up (see section 9); it is
  out of scope for this fix pass since it existed identically before either
  finding.
- **Medium fix scope**: I bound `appcontainer_sid_digest` using the exact
  same first-request-wins / mismatch-rejects pattern already used for
  `job_id`, `capsule_id`, and `sealed_base_digest`, rather than inventing a
  new verification mechanism, to keep the change minimal and consistent with
  the file's existing design.
- **MAC-ordering (Low) finding — not applied.** See
  `Docs/graph/evidence/W5/BLD-001-R2-CODE/security-fix-2026-08-25.md` for the
  full reasoning: reordering the MAC check ahead of
  `envelope.request.validate()` necessarily changes the error code returned
  for the existing `deserialization_cannot_bypass_identifier_or_digest_validation`
  test's tampered-but-unresigned-payload technique (from `Malformed` to
  `Authentication`), because the MAC covers the full request payload. The
  underlying fail-closed guarantee is not weakened either way. Correctly
  fixing the ordering without weakening that existing acceptance test would
  require adding new test-only surface area to resign a still-malformed
  payload, which is a materially larger, riskier change than "cheap and
  safe." Given the explicit instruction to use judgment and not risk breaking
  anything for this specific item, I left it unapplied and documented it as
  deferred technical debt rather than silently resolving the tension.
- I did not touch `Cargo.toml`/`Cargo.lock`; neither fix required a new or
  different dependency.

## 8. Review focus

- `broker.rs::CapsuleBroker::new` — confirm the dangling-intent-to-`Quarantined`
  reconstruction logic is correct for all nine `BrokerAction` variants, not
  only the three explicitly named in the finding (actions whose intent/complete
  states differ, e.g. `Create`→`Creating`/`Created`, are unaffected by this
  change since their normal `handle()` path already re-attempts `apply()` on
  reconstruction from a non-terminal intent state — verify this reasoning).
- `broker.rs::bind_or_verify` and `journal.rs::validate_next` — confirm
  `appcontainer_sid_digest` is now checked/persisted everywhere the other
  three identity fields are, with no gap.
- The three new crash/reconstruction tests in `tests/broker_contract.rs` —
  confirm the hand-appended `JournalEntry` fixtures are a faithful model of
  "process crashed after `append(intent, ...)`, before `apply()`/its
  completion record" and not a different, easier-to-pass scenario.
- The documented decision not to apply the MAC-ordering (Low) finding —
  confirm the reasoning holds and decide whether a follow-up task packet
  should own a proper fix (see section 9).
- The pre-existing `Reconcile`-through-`handle()` reachability question noted
  in section 7 — confirm whether it needs its own follow-up node.

## 9. Follow-up graph changes proposed

- Dispatch a fresh independent `BLD-001-R2-CODE-REV-001` pass against this
  exact patch (attempt 1/2 implementation plus this fix) to confirm no
  critical/high finding remains.
- Consider a small follow-up task to properly re-order MAC verification ahead
  of schema validation in `protocol.rs::Authenticator::verify_and_consume`,
  including the necessary test rework, rather than leaving it open-ended.
- Consider a small follow-up task (or fold into the above) to review whether
  `BrokerAction::Reconcile` needs an explicit path that is reachable even
  when `CapsuleState` is already `Quarantined`, since `is_terminal()`
  currently blocks it identically to every other action once a capsule is
  quarantined by any means (pre-existing behavior, not introduced by this
  fix).
- Keep the workspace-wide `openssl-sys` Application Control policy blocker
  and `BLD-001-R2-HOST-VERIFY`'s separate human-approval/signed-broker
  requirements as already recorded in `verification-rerun-2026-08-25.md` and
  earlier handoffs.

Only the orchestrator updates the task graph.
