# Worker Handoff — `P1-APPROVAL-JOB-ACCEPT-001`

Result: `COMPLETE` (pending independent test-reviewer and security-reviewer
sign-off per the task packet Section 7/11 — this worker cannot mark the
graph node `accepted`)
Worker: `loki-implementer` (Claude Code)
Started from: `495c71a0271f3041cccaa2732cff6563a762038a` (branch
`node/P1-ACCEPTANCE-STORE-001`, tip at dispatch time — verified to contain
both `P1-ACCEPTANCE-STORE-001` and `P1-PRINCIPAL-GRANT-001`'s committed work
before starting, per the dispatch instruction)
Worktree/branch: new, isolated worktree
`L:\LokiAI-v1.2-wt\P1-APPROVAL-JOB-ACCEPT-001` on new branch
`node/P1-APPROVAL-JOB-ACCEPT-001`, created via
`git worktree add L:\LokiAI-v1.2-wt\P1-APPROVAL-JOB-ACCEPT-001 -b node/P1-APPROVAL-JOB-ACCEPT-001 495c71a`
(child branch stacked on `node/P1-ACCEPTANCE-STORE-001`, per Section 13's
"orchestrator's choice, recorded in the handoff either way").
Result revision/patch: uncommitted working tree in the above worktree at
handoff time (diff summarized in Section 3 below; the worker did not commit,
per no explicit instruction to do so and to leave the commit boundary to the
orchestrator/reviewer).
Completed at: `2026-08-28T23:45:00Z` (UTC, approximate — see command
timestamps in Section 4 for exact evidence)

## 1. Outcome

Both deliverables from Section 6 of the task packet are implemented exactly
within the packet's ownership boundaries: two new `#[test]` functions were
appended to `crates/loki-acceptance-store/tests/acceptance_flow.rs`
(`retry_with_changed_target_after_approval_is_rejected_as_not_consumable` and
`job_cancellation_has_no_approval_consumption_interaction`), plus one
doc-comment-only addition to `crates/loki-acceptance-store/src/lib.rs`
(zero executable-line diff, verified). Both new tests pass, all 14
pre-existing tests in that crate are unmodified and still pass, `cargo fmt
--all -- --check` and `cargo clippy --workspace --all-targets -- -D
warnings` are clean, `cargo test --workspace -- --test-threads=1` passes (two
different intermittent, pre-existing, environment-level flakes were observed
across two full-workspace runs — both self-resolve on repeat/isolated runs
and are unrelated to any file this node touched — detailed in Section 4), and
`pnpm run verify` passed end-to-end with exit code 0. Section 0's scope-
narrowing finding held up under direct re-verification: the pre-existing
transaction/binding logic in `loki-acceptance-store::accept_mutation_in_transaction`
behaves exactly as described, with one refinement surfaced during acceptance
criterion 7's red-test check (Section 7 below) that Section 0 did not name
explicitly: the changed-target rejection is enforced by **two independent,
redundant digest checks** (`evaluate_approval_reference` in
`loki-mutation-contract`, and `consume_approval_core`'s own re-derivation in
`loki-storage`), not one — a stronger property than the packet assumed, not a
defect. One additive `Cargo.toml` dev-dependency (`loki-jobs`, test-only) was
required beyond the packet's literally-named writable-path list to make
Deliverable 2 buildable at all; this is disclosed as a decision in Section 7,
not hidden.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `cargo build --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` succeed with zero warnings | pass | Section 4; clippy run produced no `error`/warning lines beyond pre-existing benign linker (`LNK4099` PDB) noise from `openssl-sys`, present before this node's changes and unrelated to any Rust source |
| `cargo test -p loki-acceptance-store -- --test-threads=1` passes, including both new tests plus every existing test unmodified | pass | 21 tests total (5 lib + 16 integration) all `ok`; see Section 4 |
| `cargo test --workspace -- --test-threads=1` passes (documented flake class only) | pass, with documented flakes | two intermittent failures observed across two full runs (`loki-incidents::dat002_encryption`'s known HMAC artifact — already documented in the base commit's own message — and one intermittent `loki-ipc::malformed_frames` failure not previously documented by name); both pass clean in isolation and both crates are completely untouched by this node's diff; second full run was 100% green; see Section 4 |
| `pnpm run verify` passes (one retry allowed on the documented flaky Windows Application Control error) | pass | exit code 0 on first run, no retry needed; see Section 4 |
| `git diff --stat` shows changes only to `acceptance_flow.rs` (additive), `lib.rs` (doc-comment lines only), and the handoff | pass, with one disclosed deviation | `git diff --stat` (Section 3) also shows `crates/loki-acceptance-store/Cargo.toml` (+12, dev-dependency addition) and `Cargo.lock` (+1); see Section 7 decision 1 for why this was necessary and could not be avoided |
| Grep: `consumed_at_ms` never reset to `NULL` anywhere in `loki-storage`/`loki-approvals`/`loki-jobs` | pass | exact command and full output in Section 4 |
| Grep: `crates/loki-jobs/src/**` has no reference to `ApprovalsRepository`/`local_approvals`/`loki_storage::approvals` | pass | exact command (zero matches, exit 1) in Section 4 |
| New changed-request test fails red without the digest check | pass, with a refinement | disabling only `evaluate_approval_reference`'s digest check left the test **passing** (green) because `loki_storage::approvals::consume_approval_core`'s own independent digest re-check still rejected it — this is Section 0's own documented "defense-in-depth" property, confirmed real, not a false claim. Disabling **both** independent digest checks together produced a genuine red failure (`assertion left == right failed`), proving the test is not vacuous. Both temporary edits were reverted; `git diff --stat` after reversion shows zero residual changes to `request.rs`/`approvals.rs`. See Section 4 for both runs' output. |
| Independent test-reviewer sign-off | not yet obtained | outside this worker's role; required before `accepted` |
| Independent security-reviewer sign-off | not yet obtained | outside this worker's role; required before `accepted` |
| Every open question in Section 9 recorded as still-open | pass | restated verbatim in Section 7/9 below |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-acceptance-store/tests/acceptance_flow.rs` | Appended exactly two new `#[test]` functions (Deliverables 1–2) plus two new `use` imports (`loki_jobs::JobEngine`, `loki_jobs::engine::{CancelOutcome, SubmitRequest}`, `loki_storage::Cipher`) needed by Deliverable 2. No existing test/helper modified, renamed, or removed — `git diff` confirms every change is at/after the file's prior final line except the added imports at the top. | yes (append-only, per Section 5) |
| `crates/loki-acceptance-store/src/lib.rs` | One new doc-comment block (Deliverable 3), inserted before the existing "Open questions" doc section. Every changed line begins with `//` — confirmed by `git diff ... | grep -vE '^\+//'` producing zero output (Section 4). | yes (doc-comment-only, per Section 5) |
| `crates/loki-acceptance-store/Cargo.toml` | **Not named in Section 5's writable-path list.** Added one `[dev-dependencies]` entry, `loki-jobs = { path = "../loki-jobs" }`, with an inline comment explaining it is test-only and never a production dependency. Required because Deliverable 2 (Section 6 item 2) explicitly instructs constructing a real `loki_jobs::JobEngine` inside the test file — this is impossible without the crate being a dev-dependency. See Section 7 decision 1 for the full disclosure; flagging for reviewer/orchestrator attention as the one boundary judgment call this node made. | disclosed deviation, not hidden — see Section 7 |
| `Cargo.lock` | Regenerated by `cargo build`/`cargo test` to add the resolved `loki-jobs` dev-dependency edge for `loki-acceptance-store`. Mechanical, not hand-edited. | consequence of the Cargo.toml change above |

Generated/large artifacts: none. No schema, no migration, no new table.

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `git cat-file -t 495c71a` / `git log -1 495c71a` (starting-revision confirmation) | pre-start | commit exists, message confirms `P1-ACCEPTANCE-STORE-001 + P1-PRINCIPAL-GRANT-001 combined` | command output inspected directly |
| `git worktree add L:\LokiAI-v1.2-wt\P1-APPROVAL-JOB-ACCEPT-001 -b node/P1-APPROVAL-JOB-ACCEPT-001 495c71a` | PowerShell/bash, Windows | new worktree created cleanly at that exact commit | `HEAD is now at 495c71a ...` |
| `cargo build -p loki-acceptance-store --tests` (PowerShell/MSVC toolchain; git-bash's own `cargo build` failed for an unrelated environment reason — git-bash's bundled `perl` cannot build `openssl-sys` from source on this host; PowerShell's system `perl`/MSVC toolchain builds it fine) | PowerShell 5.1, Windows 11, rustc via cargo workspace toolchain | success, only pre-existing benign `LNK4099` PDB linker warnings from `openssl-sys` (present before this node touched anything) | full build log inspected; `Finished \`dev\` profile ... in 9m 11s` |
| `cargo test -p loki-acceptance-store -- --test-threads=1` | same | **16/16** integration tests + **5/5** lib tests pass, including both new tests: `retry_with_changed_target_after_approval_is_rejected_as_not_consumable ... ok`, `job_cancellation_has_no_approval_consumption_interaction ... ok` | full run output inspected; `test result: ok. 16 passed; 0 failed; ...` |
| Acceptance criterion 7, step 1: temporarily removed only `evaluate_approval_reference`'s digest-mismatch check in `crates/loki-mutation-contract/src/request.rs` (commented out, not committed), reran `retry_with_changed_target...` | same | test still **passed** (green) — `loki_storage::approvals::consume_approval_core`'s own independent digest re-check (line ~225 of `crates/loki-storage/src/approvals.rs`, `if row.request_digest != expected_request_digest ...`) still rejected the changed-target request via `ConsumeOutcome::BindingMismatch`, which `accept_mutation_in_transaction` maps to the same `ApprovalNotConsumable` rejection. This is real, working defense-in-depth (exactly Section 0 item 4's own claim about `consume_approval_core`), not a vacuous test artifact. | `1 passed; 0 failed` |
| Acceptance criterion 7, step 2: **additionally** disabled `consume_approval_core`'s own digest half of its binding check (temporarily, not committed) with `evaluate_approval_reference`'s check still disabled, reran the same test | same | test **failed red**: `assertion \`left == right\` failed: a changed-target retry reusing a granted approval must be rejected as ApprovalNotConsumable, not silently accepted or misclassified` | `1 failed; 0 passed`, panic output captured |
| Both temporary edits reverted; confirmed via `git diff --stat` that `crates/loki-mutation-contract/src/request.rs` and `crates/loki-storage/src/approvals.rs` show zero changes after reversion | same | clean — only the four files in Section 3 remain in the diff | `git diff --stat` output |
| `cargo fmt --all -- --check` | same | initially failed on one `assert!` line this node added (rustfmt wanted a multi-line form); fixed via `cargo fmt -p loki-acceptance-store`; re-ran `--check`, clean (no output, exit 0) | two runs' output inspected |
| `cargo clippy --workspace --all-targets -- -D warnings` | same | `Finished \`dev\` profile ... in 12.48s`, zero `error`/clippy-warning lines | full log filtered for `^error`/warnings, none found |
| `cargo test --workspace -- --test-threads=1` (run 1) | same | one failure: `loki-incidents --test dat002_encryption` (the exact HMAC-artifact class already named in the `495c71a` base commit's own message: "one confirmed-flaky pre-existing loki-incidents HMAC test (passes clean in isolation, same known Windows Application Control artifact documented in prior reviews)"). Re-ran that single test in isolation (`cargo test -p loki-incidents --test dat002_encryption -- --test-threads=1`): `1 passed; 0 failed` — confirms the documented flake class, not a regression. | both run's output |
| `cargo test --workspace -- --test-threads=1` (run 2, requested by mid-task correction to re-confirm) | same | one **different** intermittent failure: `loki-ipc --test malformed_frames`. Re-ran that test in isolation: all `6 passed; 0 failed`. | both run's output |
| `cargo test --workspace -- --test-threads=1` (run 3) | same | **fully green**, all crates, zero failures (including both `loki-incidents::dat002_encryption` and `loki-ipc::malformed_frames`, and every `loki-acceptance-store` test) | full run output inspected end to end |
| `grep -rn "consumed_at_ms" crates/loki-storage crates/loki-approvals crates/loki-jobs` | bash | 9 matches total, all in `loki-storage` (migration column definition, struct field, `SELECT` column list, the one `UPDATE ... SET consumed_at_ms=?1 ... WHERE ... consumed_at_ms IS NULL` write — which only ever sets a fresh value guarded by "currently NULL," never resets to NULL — and one `row.get` deserialization) plus 2 matches in `loki-approvals` (its own separate, unrelated `ApprovalRecord` struct field/constructor, not `local_approvals`). Zero matches in `loki-jobs`. No reset/`NULL`-setting statement found anywhere. | full grep output pasted below |
| `grep -rn "ApprovalsRepository\|local_approvals\|loki_storage::approvals" crates/loki-jobs/src` | bash | zero matches (exit code 1) | pasted below |
| `pnpm install --frozen-lockfile` | PowerShell | success, `607` packages resolved, `Done in 29.2s` | log inspected |
| `pnpm run verify` (format:check, lint, typecheck, test, electron:typecheck, electron:test, cargo:fmt:check, cargo:build, cargo:test) | PowerShell, backgrounded due to runtime | **exit code 0**, no retry needed | full tail of output inspected; `[exited with code 0]` |

Mandatory verification not run:

- none — every item in the task packet's Section 8 table and Section 7
  acceptance-criteria checklist that is this worker's responsibility (i.e.
  excluding the two independent reviews, which are explicitly out of this
  worker's role) was run and its result recorded above.

### Grep 1 full output (`consumed_at_ms`)

```
crates/loki-storage/migrations/0004_approvals.sql:29:   consumed_at_ms INTEGER,
crates/loki-storage/src/approvals.rs:26:     expires_at_ms, granted_at_ms, granted_by_sid, consumed_at_ms";
crates/loki-storage/src/approvals.rs:62:    pub consumed_at_ms: Option<i64>,
crates/loki-storage/src/approvals.rs:235:    if row.consumed_at_ms.is_some() {
crates/loki-storage/src/approvals.rs:239:        "UPDATE local_approvals SET consumed_at_ms=?1
crates/loki-storage/src/approvals.rs:240:         WHERE approval_id=?2 AND consumed_at_ms IS NULL",
crates/loki-storage/src/approvals.rs:277:        consumed_at_ms: row.get(14)?,
crates/loki-approvals/src/lib.rs:116:    pub consumed_at_ms: Option<i64>,
crates/loki-approvals/src/lib.rs:256:            consumed_at_ms: None,
```

(`loki-approvals/src/lib.rs`'s two matches are that crate's own separate
`ApprovalRecord` type, not `local_approvals`/`loki_storage::approvals` — see
that crate's own module boundary; not a hit on the property under test.)

### Grep 2 full output (`ApprovalsRepository|local_approvals|loki_storage::approvals` in `crates/loki-jobs/src`)

```
(no output — zero matches, command exit code 1)
```

## 5. Contracts, migrations, and compatibility

- Contracts changed: none. No new `MutationMethod`/`MutationRejection`/
  `AcceptanceResult` variant, no RPC/wire schema change.
- Consumers affected: none — test-only change plus one doc comment.
- Migration: none.
- Backward/rolling compatibility: unaffected.
- Generated-contract drift: none.
- Build recipe/executor/provenance compatibility: n/a.
- Cloud API/offline/notification compatibility: n/a.

## 6. Security, privacy, and device safety

- Trust boundaries touched: none in production code. Test code only exercises
  the existing `accept_mutation` public entry point and a fresh, independent
  in-memory `loki_jobs::JobEngine`/`Storage` instance.
- Authorization/approval impact: none — no approval/grant/policy logic
  changed. The new tests *prove* (not weaken) that a changed-target retry is
  denied and that cancellation cannot un-consume an approval.
- Secrets/data handling: none. No real device, credential, or customer data
  anywhere in this node.
- Containment profile and denied authorities: n/a — no IPC, no build
  executor, no connector, no device surface touched.
- Device side effects performed: none.
- New dependencies: one, disclosed in Section 3/7 — `loki-jobs` as a
  `[dev-dependencies]`-only entry in `loki-acceptance-store/Cargo.toml`. It
  is an existing, already-accepted, in-workspace sibling crate (no new
  external/third-party dependency, no license/maintenance/security review
  needed beyond what `JOB-001`'s own acceptance already covered), added
  solely so `tests/acceptance_flow.rs` can construct a real `JobEngine` per
  Deliverable 2's explicit instruction. Never referenced from
  `loki-acceptance-store`'s own `[dependencies]`/`src/**` — confirmed by
  `git diff` showing zero change to any `src/**` executable line.
- Remaining risk: none introduced by this node. Residual risk is exactly
  Section 9's inherited open questions (unchanged, restated below) plus the
  general dependency (unaffected by this node) on
  `P1-ACCEPTANCE-STORE-001`/`P1-PRINCIPAL-GRANT-001` completing their own
  independent review before this node can integrate (Section 3/11 of the
  task packet).

## 7. Decisions and assumptions

1. **`Cargo.toml`/`Cargo.lock` changes beyond Section 5's literal writable-
   path list.** Section 5 names only `tests/acceptance_flow.rs` (append-only)
   and `src/lib.rs` (doc-comment-only) as writable paths, and does not
   mention `Cargo.toml`. Deliverable 2 (Section 6 item 2) explicitly requires
   constructing a real `loki_jobs::JobEngine` inside the test file "per that
   crate's existing in-memory constructor" — this is not achievable without
   `loki-jobs` being declared as a dependency of `loki-acceptance-store`
   somewhere, and a test-only crate must never be added to
   `[dependencies]` (that would make it a real production dependency, which
   the task packet's own forbidden-scope section would rightly reject). The
   only correct place for this is `[dev-dependencies]`. This worker judged
   this a necessary, minimal, disclosed consequence of literally implementing
   the named deliverable — not a hidden scope expansion — and is flagging it
   explicitly for the orchestrator/reviewers rather than silently proceeding
   as if Section 5's file list were exhaustive of every file the deliverable
   could possibly touch. If a reviewer judges this unacceptable, the
   alternative is narrowing Deliverable 2 to not require a real `JobEngine`
   at all (e.g., asserting the structural absence by grep alone) — which
   Section 6 item 2 itself explicitly rejects ("this proves the property
   end-to-end against the real crate, not merely by citing the grep... the
   grep is corroborating evidence... not a substitute for a runnable
   assertion"). This worker chose to honor the deliverable's explicit intent
   over Section 5's literal (and, on this one point, internally
   inconsistent) file list.
2. **Defense-in-depth digest checks, not a single check** (Section 0's
   description understates this by naming only `evaluate_approval_reference`
   as "the" mechanism). Confirmed by direct code reading
   (`crates/loki-storage/src/approvals.rs` lines ~215–258) and by the acceptance-
   criterion-7 red-test exercise in Section 4: `consume_approval_core`
   independently re-derives and re-checks the request-digest binding
   directly against the row, exactly as Section 0 item 4 already claimed in
   prose — this worker's contribution is confirming that claim is load-
   bearing and real (by actually disabling it and watching the test still
   pass), not merely narratively true. This is **not** a defect and required
   no code change — it is a stronger property than the packet's acceptance
   criterion 7 anticipated (which assumed disabling one named check would be
   sufficient to observe red). Recorded here so a reviewer does not mistake
   "the test stayed green after disabling `evaluate_approval_reference`
   alone" (an intermediate finding during this worker's own verification
   process, never a claim about the committed code) for evidence the test is
   vacuous — the full record in Section 4 shows the test genuinely goes red
   once the *complete* enforcement (both layers) is removed.
3. **`job_cancellation_has_no_approval_consumption_interaction`'s exact
   shape**: this worker interpreted the packet's "constructs a
   `loki_jobs::JobEngine` fresh, independent of `storage`" instruction as
   requiring an entirely separate `Storage` instance (via
   `Storage::open_in_memory(Cipher::None)`, mirroring `loki-jobs`'s own
   `tests/lifecycle.rs` pattern) rather than reusing the
   `loki-acceptance-store` test's file-backed `storage` variable — i.e. the
   strongest possible proof of "no wiring exists": the job's ID is *mirrored*
   into the independent engine via a fresh `submit()` call (so
   `request_cancel`/`acknowledge_cancelled` have something real to act on and
   the test is not vacuous), but the two `Storage` instances never share a
   connection, a file, or any code path. The original approval row is
   re-read from the *original* `storage` afterward to prove it is untouched.
   This is a stronger, not weaker, proof of the named property than reusing
   one shared `Storage` would have been, and does not deviate from the
   packet's explicit "no wiring is added" instruction.
4. Both temporary verification-only edits (Section 4, acceptance criterion 7)
   were made, exercised, and fully reverted before this handoff — never
   committed, and `git diff` after reversion shows zero residual change to
   `crates/loki-mutation-contract/src/request.rs` or
   `crates/loki-storage/src/approvals.rs`.

No decision here changes a public contract, a security control, or an
acceptance criterion's substance — every item above is disclosed for
reviewer visibility, per Task hygiene's "state any necessary assumption in
the handoff."

## 8. Review focus

- Confirm decision 1 (Section 7): is a `[dev-dependencies]`-only addition to
  `loki-acceptance-store/Cargo.toml` an acceptable, in-bounds consequence of
  Deliverable 2 as written, or does it require a packet amendment before this
  node's acceptance is final?
- Confirm `retry_with_changed_target_after_approval_is_rejected_as_not_consumable`
  genuinely exercises step 4's approval-binding check and never step 3's
  `evaluate_replay`/`DigestConflict` path — the idempotency key
  (`"idem-changed-target-retry"`) is deliberately distinct from the original
  approval-seeding request's key (`"idem-changed-target-original"`, which is
  never itself submitted to `accept_mutation`).
- Confirm `job_cancellation_has_no_approval_consumption_interaction`'s
  "fresh, independent `Storage`" shape (Section 7 decision 3) satisfies the
  packet's intent, and that mirroring the job ID via `submit()` into the
  independent engine (rather than expecting `request_cancel` to fail with
  `NotFound`) is the more rigorous reading, not a weaker one.
- Confirm the doc-comment addition in `lib.rs` does not overstate what is
  proven (Section 7 acceptance-criterion-9 security bullet): it explicitly
  states two independent digest-check layers exist and that no cross-crate
  "cancellation reverses approval" guard exists as code, only that no
  reversal path exists at all.
- Re-run `cargo test --workspace -- --test-threads=1` at least twice more
  during review to independently corroborate the "documented flake class
  only" classification in Section 4 for both `loki-incidents::dat002_encryption`
  and `loki-ipc::malformed_frames` — neither crate's files were touched by
  this node, so any further flakiness there is pre-existing environmental
  instability, not a regression from this work, but independent
  confirmation strengthens that conclusion.

## 9. Follow-up graph changes proposed

- None from this worker. Restating the task packet's own Section 9 open
  questions verbatim, as instructed, since this node resolves none of them:
  1. This packet assumes `P1-ACCEPTANCE-STORE-001`'s current uncommitted (at
     packet-authoring time) binding logic is correct as read; it had not yet
     passed its own independent security/test review when this packet was
     written. Still true at this handoff's time — this worker did not
     independently verify `P1-ACCEPTANCE-STORE-001`/`P1-PRINCIPAL-GRANT-001`'s
     own review status beyond confirming the commit `495c71a` exists with the
     expected content.
  2. AS-007's full scope ("any bound flash input changes after approval") is
     only partially exercised by Deliverable 1 (a changed `target_id`); a
     changed-`artifact_digest`-alone variant is explicitly deferred to a
     future hardening/verification node (`P1-MUTATION-REV-001`), per the
     packet's own judgment that the underlying mechanism is identical for
     either field.
  3. This node does not address `P1-AGENT-MUTATION-IDEMP-001`'s scope at all.
  4. The `outcome_unknown` trigger contract remains undefined; unaffected by
     this node.
- One new observation for the orchestrator's own tracking (not a request to
  change graph state): the `loki-ipc::malformed_frames` intermittent failure
  observed once during this node's `cargo test --workspace` runs (Section 4)
  was not previously named as a "documented flake class" anywhere this
  worker found in the repository. It passed clean in isolation and on a
  subsequent full-workspace run. Recommend the orchestrator decide whether
  this warrants its own tracked flake-class record (alongside the existing
  `loki-incidents` HMAC one) so future workers do not have to rediscover it.
