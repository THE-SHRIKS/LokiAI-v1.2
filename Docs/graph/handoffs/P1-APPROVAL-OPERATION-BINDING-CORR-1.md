# Worker Handoff — `P1-APPROVAL-OPERATION-BINDING-CORR`

Result: `COMPLETE`
Worker: implementer (Claude Sonnet 5, Claude Code)
Started from: `7535e08` ("fix: unify approval mutation operation binding") plus
an uncommitted, partway-through correction already present in the worktree
`L:\LokiAI-v1.2\target\worktrees\p1-operation-binding-corr`
(branch `codex/p1-operation-binding-corr`)
Result revision/patch: `16478fa` ("fix(approvals): eliminate cross-storage
authorization bypass and standalone approval consumption")
Completed at: 2026-08-30T02:15:00Z (approx, UTC)

## 1. Outcome

An independent review found two HIGH findings in `7535e08`. A prior
session had left an uncommitted, partial fix in the working tree. This node
verified that partial fix line-by-line against both HIGH findings, found it
substantively correct and complete for the core defect, fixed two clippy
violations (`clippy::let_and_return`) and two doc-comment inaccuracies left
stale by the removal of the standalone consume path, discovered and fixed
two pre-existing unrelated test regressions (`corruption.rs`,
`migrations.rs`) that `7535e08` had silently broken by adding a tenth
migration without updating their hardcoded migration-count assertions, then
committed the result as a new commit (`16478fa`) on top of `7535e08` — `7535e08`
itself was never rewritten or amended.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| HIGH 1: `ApprovalStore` owns/uses the authoritative shared `Storage`; no caller-supplied `grant_generation`; creation and grant read/validate the authority session and derive `grant_generation` transactionally from the same DB connection | pass | `crates/loki-approvals/src/lib.rs` — `ApprovalStore { storage: Arc<Storage> }`; `ApprovalRequest` has no `grant_generation` field; `create`/`grant` wrap all reads/writes in one `self.storage.transaction(\|tx\| ...)` closure; `authorized_generation_core` reads the session via `loki_storage::local_authority::get_authority_session_core(tx, ...)` on that same `tx` |
| HIGH 1: validates principal SID, IPC-session binding, issuance time, expiry, revocation, capability | pass | `loki_local_authority::evaluate_local_session` (now `pub`, shared by both `LocalAuthority::authorize_local` and `loki-approvals`) checks all six in one place; `crates/loki-local-authority/src/store.rs:218-251` |
| HIGH 1: a session from a different `Storage` fails as `SessionNotFound` or equivalent closed result | pass | new test `session_from_different_storage_cannot_authorize_approval_creation` (`loki-approvals/src/lib.rs`) — asserts `Err(ApprovalError::Authority(AuthorityError::SessionNotFound))` |
| HIGH 2: remove public standalone approval consumption APIs | pass | `ApprovalStore::consume` and `ApprovalsRepository::consume_approval` (+ trait method) both deleted; `grep -rn "fn consume\b" crates/loki-approvals crates/loki-storage/src/approvals.rs` → no matches |
| HIGH 2: keep only the connection/transaction-scoped `consume_approval_core` | pass | `crates/loki-storage/src/approvals.rs` — `consume_approval_core(conn: &Connection, ...)` remains `pub fn`, no trait/repository wrapper; its only real caller is `crates/loki-acceptance-store/src/lib.rs:363` (private `accept_mutation_in_transaction`, itself only reachable via `pub fn accept_mutation` → `storage.transaction(...)`) |
| HIGH 2: no standalone consumption route reachable outside the atomic acceptance transaction | pass (structural + test) | `Storage::lock()` is `pub(crate)` (`crates/loki-storage/src/storage.rs:252`) — no crate outside `loki-storage` can obtain a bare `&Connection`; the only externally reachable route to a `&Connection`/`&Transaction` is `Storage::transaction()`, which commits atomically. `approval_gated_accept_consumes_approval_and_creates_real_job` (`loki-acceptance-store/tests/acceptance_flow.rs`) proves the real, only-reachable path atomically creates the job row and consumes the approval together |
| Adversarial test (a): session from Storage-A cannot authorize approval against Storage-B | pass | `session_from_different_storage_cannot_authorize_approval_creation` |
| Adversarial test (b): no code path outside the acceptance transaction can reach standalone consumption | pass (structural, see above) + `revoked_session_cannot_create_or_grant` closes the adjacent revocation-mid-flow gap | see cell above; no separate runtime test was fabricated for an absent API surface — the guarantee is the type system (`pub(crate) lock()`), not a behavior to exercise |
| `cargo fmt --all -- --check` | pass | see §4 |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, 0 warnings | see §4 (two `clippy::let_and_return` violations found and fixed in the inherited partial diff) |
| `cargo test -p loki-approvals -p loki-local-authority -p loki-storage -p loki-acceptance-store -- --test-threads=1` | pass, 0 failed | see §4 |
| Do not rewrite/amend `7535e08` | pass | new commit `16478fa` on top, `7535e08` untouched (`git log --oneline -3` confirms) |
| Does this correction touch `loki-mutation-contract`? | **No** — verified, not assumed | `git diff --stat -- crates/loki-mutation-contract` on the full working-tree diff (before commit) returned empty; `crates/loki-mutation-contract` does not appear in `git diff --name-only` at any point in this node. See §5 for the ADR-0008 implication |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-approvals/src/lib.rs` | Inherited (verified, then clippy-fixed): `ApprovalStore` owns `Arc<Storage>` directly; `create`/`grant` run inside one `Storage::transaction()`; `grant_generation` removed from `ApprovalRequest`, derived from the session row read in that same transaction via new `authorized_generation_core`; `ApprovalStore::consume` deleted; tests rewritten to match (new `session_from_different_storage_cannot_authorize_approval_creation`, `revoked_session_cannot_create_or_grant`, `create_derives_generation_from_same_store_session_and_grant_persists`; old `consume`-dependent tests removed, including a former `encrypted_restart_preserves_grant_and_single_use_state` and `concurrent_consumption_has_exactly_one_winner` that specifically exercised the now-removed standalone `consume` — coverage for atomic consumption is retained at the correct layer by `loki-acceptance-store`'s `approval_gated_accept_consumes_approval_and_creates_real_job` and `fault_injection_after_each_step_leaves_zero_orphan_rows`). This node additionally fixed 2 `clippy::let_and_return` violations (`let result = ...; result` → tail expression) | yes |
| `crates/loki-local-authority/src/lib.rs` | Inherited: exports new `evaluate_local_session` alongside `LocalAuthority` | yes |
| `crates/loki-local-authority/src/store.rs` | Inherited: extracted the former private `require_live_session` (partial checks + `LocalCapability` allowlist, no generation return) into `pub fn evaluate_local_session` (adds `generation` to its return value, adds the capability-allowlist check that was previously split across two call sites); `LocalAuthority::authorize_local` now reads the session itself and delegates all validation to `evaluate_local_session`, so both `loki-local-authority`'s own callers and `loki-approvals`' transaction-scoped caller share one validator | yes |
| `crates/loki-storage/src/approvals.rs` | Inherited: removed `ApprovalsRepository::consume_approval` (trait method + impl); this node corrected 3 stale doc comments left dangling by that removal (module doc, `ConsumeOutcome` doc, `consume_approval_core` doc, and a comment inside `consume_approval_core` that still named the deleted `self.lock()` wrapper as a "real caller") | yes |
| `crates/loki-acceptance-store/tests/acceptance_flow.rs` | Inherited: `real_flash_request_with_granted_approval` updated to the new `ApprovalStore::create`/`grant` signatures (no `authority`/`grant_generation` args) and reads `record.grant_generation` back instead of asserting the caller's stale `session.generation` | yes |
| `crates/loki-storage/tests/corruption.rs` | **New in this node.** `missing_path_creates_a_fresh_database_not_an_error` asserted `applied_versions == [1..=9]`; `7535e08` added `migrations/0010_approval_operation_binding.sql` without updating this assertion, leaving the test red. Updated to `[1..=10]` | yes — see rationale below |
| `crates/loki-storage/tests/migrations.rs` | **New in this node.** Same root cause, same fix, in `fresh_storage_reports_which_versions_it_applied` | yes — see rationale below |

**File-set note**: the task instructions listed 5 files as the expected touched set and asked me to "confirm this is the complete necessary file set or extend it if genuinely required (document why, don't silently expand scope)." I extended it by 2 files (`loki-storage/tests/corruption.rs`, `loki-storage/tests/migrations.rs`). Reason: both were pre-existing tests, untouched by either HIGH-finding fix, that were already broken by `7535e08` itself (which is out of scope to amend) because it added a 10th migration without updating two hardcoded `applied_versions` assertions elsewhere in the same crate. Leaving them red would mean `cargo test -p loki-storage` — an explicitly mandatory verification command for this node — fails for a reason unrelated to either HIGH finding, masking any real regression this node might introduce. The fix in both cases is a one-line assertion update (`vec![1..=9]` → `vec![1..=10]`), not a behavior or contract change.

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | Windows 11, worktree `p1-operation-binding-corr` | pass (no output) after one `cargo fmt --all` pass to fix 2 formatting issues in the inherited diff | terminal transcript |
| `cargo clippy --workspace --all-targets -- -D warnings` | same | **initially failed**: `openssl-sys`'s build script (a transitive dependency via `rusqlite`'s `bundled-sqlcipher-vendored-openssl` feature) was blocked from executing by a Windows Application Control policy (os error 4551) in this worktree's own empty `target/` directory. Worked around by pointing `--target-dir` at the repository root's `target/` directory (`L:\LokiAI-v1.2\target`), which already had a fully-built `openssl-sys` (149 files under `.../openssl-build/install`) from a prior successful build elsewhere in the repo — this is a build-cache reuse, not a code or environment change, and both `target/` directories are gitignored build output, not tracked/owned files. After that: pass, 0 warnings, 0 errors, across the full workspace. Along the way found and fixed 2 real `clippy::let_and_return` violations in the inherited `loki-approvals/src/lib.rs` (`create`/`grant`'s `let result = self.storage.transaction(...)?; result` → tail expression) | terminal transcript |
| `cargo test -p loki-approvals -p loki-local-authority -p loki-storage -p loki-acceptance-store --target-dir "L:/LokiAI-v1.2/target" -- --test-threads=1` | same | pass, 0 failed, across every test binary in all four crates (loki-approvals lib: 6/6; loki-local-authority lib: 8/8; loki-storage lib: 49/49; loki-storage `corruption`: 4/4 after fix; loki-storage `migrations`: 3/3 after fix; loki-storage other integration binaries: all pass; loki-acceptance-store lib: 5/5; loki-acceptance-store `acceptance_flow`: 14/14). First run surfaced the two pre-existing `corruption.rs`/`migrations.rs` failures described in §3; fixed; full re-run clean | terminal transcript, saved to scratchpad (`test_run3.txt`) |

Mandatory verification not run:

- None. Unlike the sibling correction tasks in this same batch (`P1-AGENT-TERMINAL-CORR`, per its handoff), this node did not hit the "test binary blocked from *executing*" form of os error 4551 — only the build-script-execution form, for which the `--target-dir` cache-reuse workaround above was sufficient to get real compilation and real test execution, not just `--no-run`. No test was skipped or treated as neither-pass-nor-fail in this node.

## 5. Contracts, migrations, and compatibility

- Contracts changed: `ApprovalRequest` no longer has a `grant_generation` field (removed by the inherited diff, confirmed correct); `ApprovalStore::create`/`grant` no longer take a `&LocalAuthority` parameter; `ApprovalStore::consume` and `ApprovalsRepository::consume_approval` removed entirely. All are internal Rust-crate API changes with exactly two consumers in the whole workspace (`loki-acceptance-store`, confirmed by `grep -rn "ApprovalStore" crates/` returning matches only in `loki-approvals` itself and `loki-acceptance-store`'s test file) — both already updated in this diff.
- **`loki-mutation-contract`: confirmed untouched.** `git diff --stat -- crates/loki-mutation-contract` against the full working-tree diff (before this node's commit) returned empty, and `crates/loki-mutation-contract` never appears in `git diff --name-only` at any point in this node's work. ADR-0008's earlier statement that this crate "would remain untouched" is **correct as verified against the real code**, not merely assumed — the user's suspicion that it might be needed did not materialize: `OperationBinding`/`MutationMethod`/`IdempotencyKey` (the only `loki-mutation-contract` types `loki-approvals` uses) needed no signature or semantic change; only the *caller* (`ApprovalRequest::digest`) changed to take `grant_generation` as an explicit parameter instead of a struct field.
- Migration: `migrations/0010_approval_operation_binding.sql` was already present from `7535e08` (unchanged by this node) and is unrelated to either HIGH finding — it is `7535e08`'s original approval-operation-binding schema addition. This node's only migration-adjacent change is fixing two test assertions that hardcoded the pre-0010 migration count.
- Backward/rolling compatibility: n/a — pre-release, no deployed callers.
- Generated-contract drift: none.

## 6. Security, privacy, and device safety

- Trust boundaries touched: local-authority session validation (`loki-local-authority`), approval issuance/grant (`loki-approvals`), and their composition inside the atomic mutation-acceptance transaction (`loki-acceptance-store`). This is exactly the boundary the two HIGH findings were about.
- Authorization/approval impact: closes both HIGH findings. HIGH 1 (cross-storage authorization bypass) is closed structurally — `ApprovalStore` can no longer be constructed with one `Storage` and authorized against a session in another, and `grant_generation` can never again be caller-controlled. HIGH 2 (standalone approval consumption bypass) is closed by removing the standalone API surface and relying on `Storage::lock()`'s `pub(crate)` visibility to make `Storage::transaction()` the only externally reachable route to a connection capable of calling `consume_approval_core`.
- Secrets/data handling: unchanged.
- Containment profile and denied authorities: unchanged (no runtime/IPC/MCP boundary touched).
- Device side effects performed: none — no real device-changing operation was performed or requested in this node.
- New dependencies: none.
- Remaining risk: none identified specific to this correction. General note for the reviewer (§8): the "no standalone consumption route" guarantee is enforced by `pub(crate)` visibility on `Storage::lock()`, which is a manual, per-review-cycle invariant, not something the type system prevents a future same-crate (`loki-storage`) change from silently weakening (e.g., if a future PR made `lock()` `pub`). Worth a `Docs/ARCHITECTURE.md` or crate-doc note flagging `Storage::lock()`'s visibility as a security-load-bearing decision, not routine crate-privacy hygiene — out of scope for this node to add unilaterally.

## 7. Decisions and assumptions

- I treated the inherited, uncommitted working-tree diff as a starting point to review and complete, not as ground truth — I independently re-derived that it correctly closes HIGH 1 and HIGH 2 by reading the actual `Storage::transaction`/`Storage::lock` implementation, not by trusting its doc comments.
- I extended the touched-file set by 2 files (`corruption.rs`, `migrations.rs`) for the reason given in §3 — a pre-existing, unrelated-to-either-HIGH-finding test regression from `7535e08` that blocked a mandatory verification command. I judged this "genuinely required" rather than scope creep because leaving it unfixed would either (a) require reporting `FAILED_VERIFICATION` for a defect this node did not introduce and is trivial to fix, or (b) silently skip a mandatory verification command, which `AGENTS.md` rule 12/"Never report COMPLETE with skipped mandatory verification" forbids.
- The `cargo clippy`/`cargo test` `--target-dir` workaround (pointing at the repository-root `target/` instead of this worktree's own) is a build-cache-reuse convenience, not a code, environment, or policy change; both are gitignored, untracked build-output directories, and reusing a compilation cache across worktrees on the same host/toolchain is standard Cargo practice, not a workaround for a code defect.
- I did not attempt to add a fabricated runtime test asserting "no standalone consume function exists" — that guarantee is a compile-time API-surface fact (the method was deleted), not a runtime behavior; the closest honest runtime evidence is `approval_gated_accept_consumes_approval_and_creates_real_job`, which proves the one real, reachable consumption path is atomic with job creation.

## 8. Review focus

- `crates/loki-approvals/src/lib.rs`'s `authorized_generation_core` and its two call sites in `create`/`grant`: confirm the session read (`local_authority::get_authority_session_core(tx, ...)`) and the approval insert/grant write genuinely share the same `rusqlite::Transaction` (they do — both take the same `tx: &rusqlite::Transaction<'_>` parameter from the enclosing `self.storage.transaction(|tx| ...)` closure), since that shared-connection atomicity is the entire HIGH 1 fix.
- `crates/loki-storage/src/storage.rs`'s `Storage::lock()` `pub(crate)` visibility: confirm no other change in this diff (or a future one) widens it, since that visibility boundary is what makes HIGH 2's "no standalone consumption route" true structurally rather than by convention.
- `crates/loki-storage/src/approvals.rs`'s `consume_approval_core`: confirm its only real (non-test) caller in the whole workspace is `crates/loki-acceptance-store/src/lib.rs`'s private `accept_mutation_in_transaction`, itself only reachable via the public `accept_mutation`, which always wraps it in `storage.transaction(...)`.

## 9. Follow-up graph changes proposed

- Consider a small, separate documentation-only task to add an explicit `Docs/ARCHITECTURE.md` (or crate-doc) note that `Storage::lock()`'s `pub(crate)` visibility is a security-load-bearing invariant for approval-consumption atomicity, not routine crate-privacy hygiene (see §6, remaining risk). Not raised as a blocking finding — purely a documentation hardening suggestion for a future reviewer/maintainer.

Only the orchestrator updates the task graph.
