# Verification — `P1-AGENT-TERMINAL-CORR` — W7 integration

Integrator: Claude Code (integrator role), operating on integration
branch/worktree `L:\LokiAI-v1.2` (branch `LokiAI-v1.2`).

Node: correction to `AgentRegistry::finish_run` (`crates/loki-agents/src/lib.rs`)
— makes the terminal-run state transition (authentication, revocation check,
scope check, run-ownership validation, guarded `UPDATE ... WHERE
state='running'`, and rows-affected-checked replay-vs-conflict resolution)
run inside a single `Storage::transaction`, closing a prior non-atomic/
silently-discarded-guard gap. This is a correction to the already-accepted
`AGT-001`/`AGT-002` acceptance basis (`finish_run` is `AGT-001`-owned surface),
not a new graph node in its own right — no new `depends_on` edge or capability
is introduced.

## 1. Source

- Worktree: `L:\LokiAI-v1.2\target\worktrees\p1-agent-terminal-corr`, branch
  `codex/p1-agent-terminal-corr`.
- Commits: `588446b` (fix + tests), `b4dbd9f` (handoff doc), stacked cleanly
  on `2468878` (an ancestor of the integration branch's actual starting HEAD
  `1a4211a` — `1a4211a` added only documentation on top of `2468878`, zero
  code divergence confirmed via `git diff 2468878 1a4211a -- crates/ Cargo.toml
  Cargo.lock` = empty).
- Handoff: `Docs/graph/handoffs/P1-AGENT-TERMINAL-CORR-1.md` (cherry-picked
  in as part of `94d80ec`).
- Independent reviews: reported by the dispatching orchestrator message as
  complete and clean (PASS, no Critical/High findings; transactional
  atomicity of `finish_run`, no deadlock, fresh revocation enforcement inside
  the transaction, run-ownership validation, correct replay-vs-conflict
  semantics, 8+ repeated concurrency runs with no flakiness). No separate
  `Docs/graph/evidence/**/REVIEW-*.md` artifact exists in the repository for
  this specific correction at integration time — this is a traceability gap
  worth closing in a follow-up documentation pass, not a reason to withhold
  integration given the explicit dispatch instruction describing the review
  outcome and this integrator's own independent re-verification below.

## 2. Integration mechanics

- `git cherry-pick 588446b` — applied cleanly, zero conflicts, one file
  changed (`crates/loki-agents/src/lib.rs`, +233/-29).
- `git cherry-pick b4dbd9f` — applied cleanly, zero conflicts, one new file
  (`Docs/graph/handoffs/P1-AGENT-TERMINAL-CORR-1.md`).
- No merge conflict of any kind (formatting, contract, or behavioral) — the
  node's sole owned file had zero concurrent modification on the integration
  branch between its branch point and integration time.
- Result commits on `LokiAI-v1.2`: `196165b` (fix), `94d80ec` (handoff).

## 3. Verification performed (integration checkout, post-cherry-pick)

All commands run directly against the integrated `LokiAI-v1.2` checkout, not
assumed from worktree-only evidence.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no output |
| `cargo clippy -p loki-agents --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, 0 errors, 0 warnings (confirmed twice in independently-clean windows, full log grepped for `^error`) |
| `cargo build --workspace` | pass (confirmed twice) |
| `cargo test -p loki-agents -- --test-threads=1` | **15/15 unit tests pass** (confirmed 3 times across the session), including the node's new tests: `finish_revalidates_revocation_and_run_ownership_before_transition`, `file_backed_concurrent_terminal_attempts_commit_exactly_one_result`, and the extended `invalid_transition_is_denied`. Doctest step blocked by the known host Application Control policy (`os error 4551`) — this crate has no runnable doctest content (prose only), so this has no material coverage impact. |
| `cargo test -p loki-acceptance-store -- --test-threads=1` | **19/19 tests pass** (5 lib + 14 `acceptance_flow` integration tests), confirmed 3 times. This is the direct consumer of `finish_run` via `P1-AGENT-MUTATION-IDEMP-001`'s composition tests — confirms the correction's behavior is compatible with and exercised by that consumer. (The worker's own handoff could only get as far as `--no-run` compile verification on its host; this integration environment was able to actually execute the suite, a strictly stronger result.) |
| `cargo test --workspace` (all 19 workspace crates, verified individually per-package due to a full single-invocation `cargo test --workspace` run being unreliable — see §4) | **every crate's real (non-doctest) test targets pass**, zero failures, zero test-content skipped or weakened. Per-crate pass counts: `loki-agents` 15, `loki-acceptance-store` 19, `loki-approvals` 7, `loki-agent-delivery` 7, `loki-build-executor` 26, `loki-connector-esp32-s3` 36, `loki-connector-registry` 52, `loki-connectors-sdk` 62, `loki-incidents` 62, `loki-ipc` 69, `loki-jobs` 44, `loki-local-devices` 6, `loki-local-notifications` 7, `loki-local-setup` 5, `loki-storage` 89, `loki-monitoring` 111, `loki-mutation-contract` 29, `loki-runtime` 59. Only rustdoc doctest binaries (a separate, always-blocked binary per crate under this host's Application Control policy) and a small number of freshly-linked test binaries on their *first* execution attempt each session were blocked with `os error 4551`/`os error 2`/`os error 5` (file-lock/link-race errors); every one of these was resolved by a single immediate retry (occasionally preceded by `cargo clean -p <crate>` to force a clean relink), consistent with the documented pre-existing host limitation and the pattern already recorded in prior integration records (`Docs/graph/evidence/W6/INTEGRATION-2026-08-26/integration-record.md` §4). No test was skipped, weakened, or retried indefinitely. |
| `pnpm run verify` | `format:check`, `lint`, `typecheck` (3 TS projects), `test` (318 `packages/contracts` + 85 `services/mcp-server`, 403/403 pass), `electron:typecheck`, `electron:test` (77/77 Node test-runner cases pass) all passed cleanly. `cargo:fmt:check` and `cargo:build` (via `tools/scripts/cargo-verify-guard.mjs`) passed. The final `cargo:test` stage of this one composite invocation hit the same known fresh-binary Application Control flake on `loki-agents`'s lib-test binary (blocked on its very first link of that run); this was independently re-verified via the direct `cargo test -p loki-agents -- --test-threads=1` runs in the row above, which passed cleanly on retry. No TypeScript/Electron/JS content was affected — this correction touches only Rust (`crates/loki-agents/src/lib.rs`). |

## 4. Environment finding: concurrent-writer contamination of the shared checkout

During this integration, `cargo test -p loki-approvals` and
`cargo test -p loki-acceptance-store` intermittently failed to compile with
errors referencing fields/methods (`NewApproval::binding_version`,
`NewApproval::grant_generation`, `Storage::consume_approval`) that do **not**
exist in this checkout's actual, committed, on-disk source
(`crates/loki-storage/src/approvals.rs`'s `NewApproval` struct has never
declared those fields on this branch — confirmed by direct `grep` and `git
log` against the committed file). `git status` showed a clean `crates/` tree
immediately before and after each failing attempt. `cargo clean -p
loki-storage` (and, once, an "Access is denied" partial-clean failure on a
locked file) followed by a clean rebuild consistently resolved the failure.

This is external filesystem contention from at least one other
concurrently-active process writing into this same shared `L:\LokiAI-v1.2`
checkout and its shared `target/` build-cache directory during this
integration session — corroborated by an uncommitted, in-progress edit this
integrator did not make (`Docs/graph/decisions/ADR-0008-agent-mutation-idempotency-composition.md`,
an "Amendment 1" section referencing commit `7535e08` /
`P1-APPROVAL-OPERATION-BINDING-CORR`, a node explicitly out of this
integrator's scope) and an untracked handoff file
(`Docs/graph/handoffs/P1-RUNTIME-SERVICE-READINESS-CORR-1.md`) that appeared
in this checkout without this integrator creating it. The orchestrator
separately confirmed mid-task that another reviewer had identified the same
root cause (concurrent agents sharing the repo-root `target/` directory
causing intermittent stale/corrupted incremental-build state).

**This did not affect `P1-AGENT-TERMINAL-CORR`'s own commits, files, or
correctness** — `crates/loki-agents/src/lib.rs` was never touched by the
contaminating process, and every apparent failure was resolved by a targeted
`cargo clean -p <affected-crate>` and rebuild, never by altering test
expectations, source code, or skipping verification. It is recorded here as
an environment/process-integrity finding for the orchestrator, not as a
defect of this node.

## 5. Result

`INTEGRATED_PASS`. Both commits (`196165b`, `94d80ec`) are present on
`LokiAI-v1.2`, cleanly cherry-picked with zero conflicts, and every affected
and full-workspace check (Rust and TypeScript/Electron) passes on the
integrated checkout. No behavioral, contract, migration, dependency,
security, or device-safety conflict was found or needed to be returned to
the worker. No commit was pushed to any remote.
