# Worker Handoff — `P1-AGENT-MUTATION-IDEMP-001`

Result: `ACCEPTED-pending-review` (implementation complete, all required
automated verification passes; independent test-reviewer and
security-reviewer sign-off per the packet's own acceptance criteria are
still outstanding — this handoff is the implementer's own submission, not a
substitute for either review)

Worker: `implementer (Claude Code)`
Started from: `495c71a` (`node/P1-ACCEPTANCE-STORE-001`, containing
`P1-ACCEPTANCE-STORE-001` + `P1-PRINCIPAL-GRANT-001` combined) — a real,
existing commit, confirmed by direct `git log` inspection before creating
the worktree, per the dispatch instruction (not the packet's own Section 0,
which described an uncommitted stacking point that had since been
committed by the orchestrator).
Worktree: `L:\LokiAI-v1.2-wt\P1-AGENT-MUTATION-IDEMP-001`
Branch: `node/P1-AGENT-MUTATION-IDEMP-001`
Result revision/patch: `988bff0`
Completed at: `2026-08-28T23:50:00Z`

## 1. Outcome

`AgentRegistry::finish_run` now correctly replays `Ok(())` for a retry of
an already-completed, identical terminal transition instead of returning
`Err(AgentError::TerminalRun)`, while a retry requesting a *different*
terminal state is still denied. A new integration test file,
`crates/loki-acceptance-store/tests/agent_mutation_composition.rs`, proves
— using real, on-disk `Storage` reopened over the same file/key after
dropping every prior in-process handle (a genuine simulated crash/restart,
not a sequential-only assertion) — that the two-call composition of
`loki_acceptance_store::accept_mutation` followed by the real
`AgentRegistry`/`WorkIntentDeliveryStore` business call is crash-safe for
all three agent `MutationMethod`s. `loki-acceptance-store/src/lib.rs` has
zero diff; the crate's `Cargo.toml` gained only `[dev-dependencies]`
entries.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `cargo build --workspace` succeeds | pass | see §4 |
| `cargo clippy --workspace --all-targets -- -D warnings` zero warnings | pass | see §4 |
| `cargo test -p loki-agents -- --test-threads=1` passes incl. updated/new tests | pass | 14/14 passed, see §4 |
| `cargo test -p loki-acceptance-store -- --test-threads=1` passes incl. all 4 new composition tests | pass | 23/23 passed (5 lib + 14 `acceptance_flow` + 4 `agent_mutation_composition`), see §4 |
| `cargo test --workspace -- --test-threads=1` passes (documented flake class only) | pass on retry | see §4 — one instance of the documented Windows Application Control `os error 4551` spawn flake (`loki-build-executor::native_boundary`, then on retry `loki-runtime::process_behavior::health_check_mode_starts_up_logs_json_and_exits_zero`), both unrelated to this node's code and passing in isolation on immediate retry |
| `pnpm run verify` passes | pass | exit code 0, see §4 |
| `git diff --stat` limited to the named files | pass | `Cargo.lock`, `crates/loki-acceptance-store/Cargo.toml`, `crates/loki-agents/src/lib.rs`, new `crates/loki-acceptance-store/tests/agent_mutation_composition.rs`, this handoff |
| No new `[dependencies]` entry in `loki-acceptance-store/Cargo.toml` | pass | `git diff` shows the addition is entirely under the existing `[dev-dependencies]` section |
| `loki-acceptance-store/src/lib.rs` has zero diff | pass | `git diff crates/loki-acceptance-store/src/lib.rs \| wc -l` → `0` |
| Identical-retry `finish_run` returns `Ok(())`, no write | pass | `finish_run_retry_of_identical_transition_is_idempotent` |
| Different-state retry still `Err(TerminalRun)` | pass | updated `invalid_transition_is_denied` + `finish_run_conflicting_retry_after_composition_is_still_rejected` |
| Each composition test proves Accepted→ReplayedIdenticalAccepted with identical `job_id`/`correlation_id`/`accepted_at_ms`, real business call succeeds identically on both attempts, exactly one durable row/state change | pass | all 3 composition tests, see §4/§8 |
| `agent_run_update_composition_...` fails red if Deliverable 1 reverted | pass, verified locally | see §4 — reverted the fix, reran, observed `panicked ... TerminalRun`, then restored the fix and reran green; the reverted state was never committed |
| Test-reviewer sign-off | **outstanding** | not this implementer's role |
| Security-reviewer sign-off | **outstanding** | not this implementer's role |
| Section 9 open questions recorded as still-open | pass | see §7 |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-agents/src/lib.rs` | `finish_run` fix (Section 5/6.1): identical-terminal-state retry now returns `Ok(())` without writing; different-terminal-state retry still `Err(TerminalRun)`. Doc comment added citing this packet and ADR-0008. Updated `invalid_transition_is_denied` (identical retry now asserts `Ok(())`, added a different-state-retry assertion). Added `finish_run_retry_of_identical_transition_is_idempotent`. No signature change; no other function touched. | yes |
| `crates/loki-acceptance-store/Cargo.toml` | Added `[dev-dependencies]` only: `loki-agents`, `loki-agent-delivery` (as Section 5 named), plus `loki-local-authority`, `loki-incidents`, `loki-connectors-sdk` (not literally named in Section 5, but necessary — see §7 assumption below) — each with a comment explaining why it is test-only. No `[dependencies]` entry added. | yes, with one documented, justified extension beyond Section 5's literal two-crate list (see §7) |
| `crates/loki-acceptance-store/tests/agent_mutation_composition.rs` (new) | Deliverable 2: four crash-safety composition tests (Section 6.2 items 1–4), each real-file-backed, each dropping every handle and reopening `Storage`/`AgentRegistry`/`WorkIntentDeliveryStore` before the measured business call. No production glue code. | yes |
| `Cargo.lock` | Dependency-graph update from the new dev-dependencies above. | yes |
| `Docs/graph/handoffs/P1-AGENT-MUTATION-IDEMP-001-1.md` (this file) | Own handoff. | yes |

No edit to `crates/loki-mutation-contract/**`, `crates/loki-acceptance-store/src/lib.rs` (executable lines), `crates/loki-acceptance-store/tests/acceptance_flow.rs`, `crates/loki-storage/**`, or `crates/loki-agent-delivery/src/lib.rs`. No schema/migration change. No RPC/wire work.

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` (after `cargo fmt --all`) | Windows, worktree | exit 0 | direct run |
| `cargo clippy --workspace --all-targets -- -D warnings` | Windows, worktree | exit 0, zero warnings | direct run |
| `cargo test -p loki-agents -- --test-threads=1` | Windows, worktree | `14 passed; 0 failed` | includes `finish_run_retry_of_identical_transition_is_idempotent`, updated `invalid_transition_is_denied`, and every `P1-PRINCIPAL-GRANT-001` `rotate`/`change_scopes`/generation test unmodified |
| `cargo test -p loki-acceptance-store -- --test-threads=1` | Windows, worktree | `5 + 14 + 4 = 23 passed; 0 failed` | lib tests, unmodified `acceptance_flow.rs` (14/14), new `agent_mutation_composition.rs` (4/4) |
| `cargo test --workspace -- --test-threads=1` | Windows, worktree | pass on retry | first run: `loki-build-executor::native_boundary` failed to spawn (`os error 4551`, documented Windows Application Control spawn-block flake, unrelated crate); retry of the full suite: all green except one further instance of the same flake class in `loki-runtime::process_behavior::health_check_mode_starts_up_logs_json_and_exits_zero` (same `os error 4551`), confirmed to pass in isolation on immediate re-run (`cargo test -p loki-runtime --test process_behavior`) |
| `pnpm run verify` | Windows, worktree | exit code 0 | `format:check`, `lint` (pre-existing, unrelated React-hook warnings only — 0 errors), `typecheck`, `test`, `electron:typecheck`, `electron:test`, `cargo:fmt:check`, `cargo:build`, `cargo:test` all completed; full log captured |
| Manual red/green check | Windows, worktree | red without fix, green with fix | temporarily reverted the `finish_run` fix, ran `cargo test -p loki-acceptance-store --test agent_mutation_composition -- --test-threads=1`: `agent_run_update_composition_survives_restart_between_accept_and_apply` failed with `panicked ... TerminalRun`, the other 3 tests still passed; restored the fix, reran, all 4 green. The reverted state was never committed. |
| `git diff --stat` / targeted `git diff` on `loki-acceptance-store/src/lib.rs` / grep for `[dependencies]` in the crate's `Cargo.toml` | — | all match the acceptance criteria exactly | see §2 |

Mandatory verification not run: none — every item in Section 8's table was run.

## 5. Contracts, migrations, and compatibility

- Contracts changed: none. `loki-mutation-contract` untouched (grep/diff-verifiable — no changes in `crates/loki-mutation-contract/**`).
- Consumers affected: none — no public signature changed on `AgentRegistry::finish_run` (only internal logic), and no other public API in `loki-agents`/`loki-agent-delivery`/`loki-acceptance-store` changed.
- Migration: none — no schema/migration file touched.
- Backward/rolling compatibility: unaffected — `finish_run`'s new behavior is strictly a bug fix (a legitimate identical-transition retry now succeeds instead of erroring); no caller that depended on the old, incorrect `Err(TerminalRun)`-on-identical-retry behavior exists in this repository (grepped for `finish_run` call sites — only test call sites and `crates/loki-agent-delivery` documentation reference it, and `loki-agent-delivery` never calls `finish_run` itself).
- Generated-contract drift: n/a.
- Build recipe/executor/provenance compatibility: n/a — no build-executor surface touched.
- Cloud API/offline/notification compatibility: n/a.

## 6. Security, privacy, and device safety

- Trust boundaries touched: none newly. `finish_run`'s own authentication (`authenticate(connection_id, secret, AgentScope::RegisterRuns)`) and connection-ownership check (`run.connection_id != connection_id`) run unchanged and *before* the new identical/conflicting-state branch — the fix only changes what happens once both checks have already passed and the run is confirmed already terminal.
- Authorization/approval impact: none. No new capability, scope, or approval path.
- Secrets/data handling: the new test file authenticates against a real, freshly minted `AgentSecret` (via `AgentRegistry::register`) exactly as production code would; the secret is never logged, only compared via the existing `authenticate` path. No secret or device identifier is committed.
- Containment profile and denied authorities: n/a — no build-executor/containment code touched.
- Device side effects performed: none. No physical device/connector effect anywhere in this node (confirmed — see §7, the `outcome_unknown` open question this node explicitly does not reach).
- New dependencies: five new `[dev-dependencies]` entries in `crates/loki-acceptance-store/Cargo.toml` — all path-local, in-workspace crates already used elsewhere in the workspace (`loki-agents`, `loki-agent-delivery`, `loki-local-authority`, `loki-incidents`, `loki-connectors-sdk`), none pulling in a new external crate, none added to `[dependencies]`. Justification for each is inline in the `Cargo.toml` comment and in §7 below.
- Remaining risk: the security-critical property this fix protects — "a conflicting retry (different terminal state) must still be denied" — is proven by two independent tests (`invalid_transition_is_denied` in `loki-agents`, and `finish_run_conflicting_retry_after_composition_is_still_rejected` in the cross-crate composed context). No other risk identified.

## 7. Decisions and assumptions

- **Extended the `Cargo.toml` dev-dependency list beyond Section 5's literal two-crate list.** Section 5 names only `loki-agents` and `loki-agent-delivery` as the crates to add. Building the required tests (Section 6.2, which mandates registering a real agent connection via `AgentRegistry::register` and, for the `WorkIntentAcknowledge` test, a real `WorkIntent`) is not possible without also depending on `loki-local-authority` (for the `LocalAuthority`/`LocalContext` types `AgentRegistry::register` takes by reference and that `loki-agents` does not re-export), `loki-incidents` (for `WorkIntent`/`DeliveryRequirements`/`IncidentId`), and `loki-connectors-sdk` (for `CapabilityId`). This exactly mirrors `loki-agent-delivery`'s own `[dev-dependencies]` list, which already carries the identical three-crate pattern for its own equivalent tests. This is a `[dev-dependencies]`-only addition (never `[dependencies]`), does not violate any Forbidden Scope item, and does not change the `git diff --stat` acceptance criterion's file list (still just `Cargo.toml`, dev-dependencies only). Flagging this explicitly rather than treating it as silently implied by Section 5's wording, since Section 5's list was not literally complete for the packet's own mandated test design.
- No other deviation from the packet. The `finish_run` fix, the four composition tests, and the doc comments match Section 5/6 exactly.
- Section 9 (packet-referenced open questions, restated here as still open, per the packet's own instruction not to resolve them): (1) the `outcome_unknown` trigger contract (`P1-MUTATION-CONTRACT-001` §9 item 2) — not reached by this node, since none of this node's tests run a physical device/connector effect; (2) `MutationAcceptanceRequest.target_id`'s meaning is not reconciled across the three agent methods — this node's tests use the run_id as `target_id` for `AgentRunUpdate` and the work_intent_id for `WorkIntentAcknowledge`, an asymmetry recorded, not fixed.

## 8. Review focus

- `crates/loki-agents/src/lib.rs`'s `finish_run` (lines ~486–524): confirm the identical-state/different-state branch is exhaustive and cannot be reached in a way that silently overwrites an already-recorded terminal outcome with a different one.
- `crates/loki-acceptance-store/tests/agent_mutation_composition.rs`: confirm each of the four tests genuinely drops every `Arc<Storage>`/`AgentRegistry`/`WorkIntentDeliveryStore` handle (block-scoped, returned only as plain owned data — `String`/`u64`/`AgentRun`/`DeliveryRecord` fields, never a handle) before reopening a fresh `Storage::open_managed` over the same on-disk path; confirm the "exactly one durable row" assertions (raw `SELECT COUNT(*)` via `storage.transaction(...)` for the two `AgentRun*` tests, `WorkIntentDeliveryStore::get(...)` for the acknowledge test) are real repository reads, not incidental.
- The manually-verified red/green check for `agent_run_update_composition_survives_restart_between_accept_and_apply` (§4) — reviewer may wish to reproduce this locally rather than trust the implementer's report alone, since the reverted state was never committed.
- `crates/loki-acceptance-store/Cargo.toml`'s three additional dev-dependencies beyond Section 5's literal list (§7) — confirm this reviewer agrees they are necessary and correctly scoped to `[dev-dependencies]` only.

## 9. Follow-up graph changes proposed

- None. This node's own forbidden-scope items (no new composition glue code, no `loki-mutation-contract` change) stand as designed; any future real RPC/IPC caller that needs to combine both calls behind one wire-level request is a distinct future node's decision, not proposed here.

Only the orchestrator updates the task graph.
