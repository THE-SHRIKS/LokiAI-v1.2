# Verification — `P1-RUNTIME-SERVICE-READINESS-CORR` — W7 integration

Integrator: Claude Code (integrator role), operating on integration
branch/worktree `L:\LokiAI-v1.2` (branch `LokiAI-v1.2`).

Node: correction/completion of `RUN-002`/`DAT-001`'s original runtime
service-readiness scope. Composes `RuntimeServices` (bounded read
projections + mutation submission) over one shared `Arc<loki_storage::Storage>`
inside `ComposedHost`, and — in this specific increment — replaces the prior
deliberate fail-closed mutation-submission stub with real wiring to
`loki_acceptance_store::accept_mutation`. Zero external reachability yet: no
IPC/MCP caller invokes `RuntimeServices::submit_mutation` at this revision.

## 1. Source

- Worktree: `L:\LokiAI-v1.2\target\worktrees\p1-runtime-services`, branch
  `codex/p1-runtime-services`.
- Commits (in order): `8deb947` (compose `RuntimeServices` over shared
  storage with fail-closed mutation port), `77a853b` (handoff doc for the
  fail-closed revision), `f72c0c3` (replace fail-closed mutation port with
  real `loki_acceptance_store::accept_mutation` submission), `a91bca2`
  (handoff doc recording the real-mutation-submission follow-up).
- Branch was rebased by the worker onto `LokiAI-v1.2`@`8e47588`
  (`docs(integration): record W7 integration of
  P1-APPROVAL-OPERATION-BINDING-CORR`) — the exact tip this integration
  started from. Rebase was reported clean (zero conflicts) by the worker and
  independently confirmed here: the merge below was a clean fast-forward.
- Handoff: `Docs/graph/handoffs/P1-RUNTIME-SERVICE-READINESS-CORR-1.md`
  (§1-§9 describe the original fail-closed revision; §10-§11, added by
  `a91bca2`, describe the real-mutation-submission follow-up and its
  acceptance-criteria/verification evidence).
- Independent reviews: reported by the dispatching orchestrator message as
  complete — security review PASS (no Critical/High; one Low finding, see
  §5 below) and test review PASS, independently corroborating the exact same
  Low finding. No separate `Docs/graph/evidence/**/REVIEW-*.md` artifact
  exists in the repository for this specific correction at integration time
  — consistent with the same traceability gap already recorded for the two
  sibling corrections integrated earlier in this wave
  (`Docs/graph/evidence/W7/P1-AGENT-TERMINAL-CORR/verification.md`,
  `P1-APPROVAL-OPERATION-BINDING-CORR`'s equivalent record). Per `AGENTS.md`'s
  source-of-truth order this integrator treated the explicit dispatch
  instruction as authoritative and independently re-verified the correction's
  behavior and full-workspace compatibility directly, rather than relying on
  the report alone.

## 2. Integration mechanics

- Starting revision confirmed: `LokiAI-v1.2` at `8e47588`, clean working
  tree except two pre-existing untracked files unrelated to this task
  (`Docs.zip`, a garbled-name artifact file) and one untracked copy of this
  node's handoff document (a prefix subset of the fetched version — removed
  before merge so the fetch could bring in the complete file; content
  verified identical for the overlapping lines via `diff`, no information
  lost).
- `git fetch <worktree p1-runtime-services> codex/p1-runtime-services` then
  `git merge --ff-only FETCH_HEAD` — clean fast-forward, zero conflicts,
  8 files changed (`Cargo.lock`, the handoff doc,
  `crates/loki-runtime/Cargo.toml`, `crates/loki-runtime/src/host/compose.rs`,
  `crates/loki-runtime/src/lib.rs`, and three new files under
  `crates/loki-runtime/src/services/`). No merge-conflict resolution of any
  kind was needed (mechanical or behavioral) — the branch's changes are
  disjoint from every other node integrated so far in this wave, confirmed
  by the fast-forward itself.
- Result revision: `LokiAI-v1.2`@`a91bca2` (same commit hash as the worktree
  tip — a genuine fast-forward, not a rewrite).

## 3. Verification performed (integration checkout, post-merge)

All commands run directly against the integrated `LokiAI-v1.2` checkout at
`a91bca2`.

| Command | Result |
|---|---|
| `cargo build --workspace` | pass (PDB-link warnings only, pre-existing/unrelated to this crate's own code) |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test -p loki-runtime -- --test-threads=1` | 62/62 lib tests pass (incl. `services::mutation::tests::*` and `services::tests::*` covering: accepted-mutation parity with a direct `accept_mutation` call, real-rejection surfacing (not swallowed/generic), the `port_never_returns_storage_or_a_connection` shape check, and the pre-existing bounded-read-projection tests), 4/4 `host_lifecycle`, 4/4 `process_behavior` (+2 pre-existing, unrelated `#[ignore]` for `DAT-002-MEM-HOST-VERIFY-001`), 1/1 `secure_memory_arena_runtime`. Doctest execution blocked by the documented host Application Control policy (`os error 4551`) — execution-only, not a compile/logic failure (see §4). |
| `cargo test --workspace --no-fail-fast -- --test-threads=1` | full run across all 23 workspace crates; every real (non-doctest) test target passed. Four test binaries (`loki-jobs --test lifecycle`, `loki-monitoring --test rule_evaluation`, `loki-runtime --test process_behavior`, `loki-storage --test transactions`) were transiently blocked with `os error 4551` on this one run and all four passed cleanly on an immediate individual retry (`loki-jobs`'s lifecycle suite needed a second retry, then passed 19/19). All 19 doctest targets across the workspace hit the same host-wide `os error 4551` block on `rustdoc.exe` itself — systemic, not specific to this node or any single crate. |
| `pnpm run verify` | `format:check`, `lint` (0 errors, 4 pre-existing unrelated warnings), `typecheck` (3 TS projects), `test` (`packages/contracts` + `services/mcp-server`, all pass), `electron:typecheck`, `electron:test` (77/77 Node test-runner cases pass), `cargo:fmt:check`, `cargo:build` (via `tools/scripts/cargo-verify-guard.mjs`) all passed cleanly. The composite `cargo:test` stage stopped at its first doctest target (`loki-acceptance-store --doc`) with the same `os error 4551` host block — independently confirmed above (§3, `cargo test --workspace --no-fail-fast`) to be the systemic rustdoc-execution hazard, not a regression; every real test target in the workspace was separately confirmed passing. |

## 4. Known host hazard — doctest/fresh-binary Application Control block (`os error 4551`)

Consistent with the task's stated known hazard and the pattern already
recorded in `Docs/graph/evidence/W7/P1-AGENT-TERMINAL-CORR/verification.md`
§4 and `Docs/graph/evidence/W6/INTEGRATION-2026-08-26/integration-record.md`
§4: this host's Application Control policy intermittently blocks execution
of freshly-linked binaries (test binaries and, universally, `rustdoc.exe`
itself for every crate's doctests) with `os error 4551`. Every occurrence in
this integration session was resolved by a single immediate retry with no
source, test-expectation, or verification-scope change. No doctest content
was added or changed by this node's commits. This is a host/environment
limitation, not a defect in the integrated code.

## 5. Security note — tracked, non-blocking follow-up

The independent security and test reviews both identified the same Low
finding: `crates/loki-runtime/src/services/mutation.rs`'s
`port_never_returns_storage_or_a_connection` test only asserts `T: 'static`
on `MutationAvailability` and `Result<AcceptanceResult,
MutationSubmissionError>` — it does not structurally assert the *absence* of
a `Storage`/connection type from those return types. The property it claims
to enforce (no raw `Storage` handle or connection is ever returned/exposed
through `RuntimeServices`'s public API) is true today by direct structural
inspection (`RealMutationSubmission.storage`/`RuntimeServices.storage` are
both private fields with no accessor; `ComposedHost::storage()` — pre-existing,
unrelated to this port — is the only place a `&Storage` is ever handed out),
but the test itself does not enforce it against a future regression.

This integrator confirmed the finding by direct inspection of the test body
(`crates/loki-runtime/src/services/mutation.rs:289-298`) and concurs with
both independent reviews: Low severity, non-blocking for this integration.
Recorded here as a tracked follow-up rather than silently accepted:

- **Recommended follow-up task**: strengthen
  `port_never_returns_storage_or_a_connection` with a genuine negative
  trait-bound / structural check (e.g. a compile-time assertion that
  `MutationAvailability` and the `submit_mutation` result type do not
  implement/contain any trait or field associated with `loki_storage::Storage`
  or a raw connection handle, rather than only `'static`). Small, isolated,
  test-only change scoped to `crates/loki-runtime/src/services/mutation.rs`.
  Not release-blocking; the underlying property holds today.

## 6. Contracts, migrations, and compatibility

- No public/cross-process contract changed. `RuntimeReadPort`/
  `MutationSubmissionPort` remain internal composition traits with zero
  external callers (`ComposedHost::services()` has zero current callers,
  confirmed unchanged from the prior fail-closed revision).
- `RealMutationSubmission::submit_mutation` forwards, unmodified, into the
  already-integrated, already-reviewed `loki_acceptance_store::accept_mutation`
  (see `Docs/graph/handoffs/P1-APPROVAL-OPERATION-BINDING-CORR-1.md`) — this
  node introduces no new request-construction, approval, digest, or
  transaction-boundary logic of its own.
- No schema/migration change. `Cargo.lock` gained two dependency-edge lines
  under `loki-runtime`'s existing package entry (`loki-acceptance-store`,
  `loki-mutation-contract`); both were already resolved elsewhere in the
  workspace, so no new package-version entries were added.
- No device operation sequencing, authorization, or approval control was
  weakened. No real device, firmware, or fleet operation is reachable
  through this path at this revision (no IPC/MCP caller exists yet).

## 7. Result

`INTEGRATED_PASS`. All four commits (`8deb947`, `77a853b`, `f72c0c3`,
`a91bca2`) are present on `LokiAI-v1.2` via a clean fast-forward merge to
`a91bca2`, zero conflicts of any kind. `cargo build --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, and the full
`cargo test --workspace` suite (verified with retries against the documented
host doctest/fresh-binary hazard) all pass, as does `pnpm run verify` up to
and including every check other than the same systemic doctest-execution
hazard. No behavioral, contract, migration, dependency, security, or
device-safety conflict was found or needed to be returned to the worker. The
Low security/test finding on `port_never_returns_storage_or_a_connection` is
recorded above as a tracked, non-blocking follow-up, not silently accepted.
No commit was pushed to any remote.
