# Worker Handoff — `P1-AGENT-TERMINAL-CORR`

Result: `COMPLETE (pending independent review)`
Worker: `implementer (single bounded node)`
Started from: `2468878` (worktree `p1-agent-terminal-corr`, branch `codex/p1-agent-terminal-corr`, with the pre-existing uncommitted `crates/loki-agents/src/lib.rs` changes already present in the worktree)
Result revision/patch: `588446b`
Completed at: `2026-08-30T00:00:00Z`

## 1. Outcome

`AgentRegistry::finish_run` now performs authentication, revocation checking,
scope checking, run-ownership validation, and the terminal state transition
inside a single `Storage::transaction`, using connection-parameterized core
functions so it never re-locks the shared `Mutex<Connection>` from inside
itself. The guarded `UPDATE ... WHERE state='running'` statement's
rows-affected result is checked, not discarded: on `0` rows it re-reads the
row inside the same transaction to distinguish an idempotent replay of the
same terminal state (returns `Ok(())`, original `completed_at_ms`
untouched) from a genuinely conflicting terminal result (returns
`AgentError::TerminalRun`). This is proven by a real-thread concurrency test
against a file-backed encrypted `Storage`/`AgentRegistry` where two threads
race a conflicting terminal transition, exactly one commits, and the
committed result survives a real close/reopen of the same on-disk database
file.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `finish_run` runs auth + revocation + scope + ownership + terminal transition in one `Storage` transaction | pass | `crates/loki-agents/src/lib.rs` `finish_run` body: single `self.storage.transaction(\|tx\| { ... })` call using `get_agent_connection_core`, `authenticate_connection`, `get_agent_run_core`, `finish_agent_run_core` on the same `&Transaction`, never a second `self.storage.*` call |
| Identical terminal retry replays without rewriting `completed_at_ms` | pass | `tests::invalid_transition_is_denied` (retry assertion added), `tests::file_backed_concurrent_terminal_attempts_commit_exactly_one_result` |
| Conflicting terminal result returns `AgentError::TerminalRun` | pass | `tests::invalid_transition_is_denied` (conflicting-result assertion), `tests::file_backed_concurrent_terminal_attempts_commit_exactly_one_result` (loser thread) |
| Concurrent conflicting terminal transitions commit exactly one result (real thread test) | pass | `tests::file_backed_concurrent_terminal_attempts_commit_exactly_one_result` — two real `std::thread::spawn` threads, `Barrier`-synchronized, racing `finish_run(Succeeded)` vs `finish_run(Failed)` over one `Arc<AgentRegistry>` backed by a real on-disk encrypted file; asserts exactly one `Ok`, exactly one `TerminalRun`, and the committed row matches the winner |
| Guarded SQL update's rows-affected is checked, not ignored | pass | `loki-storage/src/agents.rs::finish_agent_run_core` returns `Ok(changed == 1)`; `lib.rs::finish_run` branches on that boolean (`if finish_agent_run_core(...)? { ... } else { re-read and compare }`) instead of discarding it |
| File-backed restart-durability test: terminal state survives a real process restart (fresh `Storage`/`AgentRegistry` reopened over the same on-disk file) | pass | `tests::file_backed_concurrent_terminal_attempts_commit_exactly_one_result`'s final section: `drop(registry); let reopened = AgentRegistry::open(&p, &key).unwrap();` then asserts the reopened registry's read of the run matches the pre-drop committed result, over the same tempdir-backed encrypted file |
| No existing check weakened to make a test pass | pass | Diff only adds transactional structure and tests; `authenticate_connection` (revocation/secret/scope logic) is unchanged, only factored into a free function reusable from both `authenticate()` and `finish_run`'s transaction closure |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-agents/src/lib.rs` | Rewrote `finish_run` to run inside `Storage::transaction`; extracted `authenticate_connection` as a free function shared by `authenticate()` and the transaction closure; added/extended tests (`invalid_transition_is_denied` retry/conflict assertions, `finish_revalidates_revocation_and_run_ownership_before_transition`, `file_backed_concurrent_terminal_attempts_commit_exactly_one_result`) | yes — sole owned path for this node |

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | Windows 11, worktree `p1-agent-terminal-corr` | pass (no output) | terminal transcript |
| `cargo clippy --workspace --all-targets -- -D warnings` | same | pass, 0 warnings | terminal transcript |
| `cargo test -p loki-agents -- --test-threads=1` | same | 15/15 unit tests pass; doctest step failed with `An Application Control policy has blocked this file. (os error 4551)` | terminal transcript — this is the known host limitation called out in the task instructions, not a code failure; the doctest binary never executed |
| `cargo test -p loki-acceptance-store -- --test-threads=1` | same | test binary blocked from executing by the same host Application Control policy (`os error 4551`), before running any test | terminal transcript |
| `cargo test -p loki-acceptance-store --no-run` | same | compiles clean (0 errors), producing both the lib-test and `acceptance_flow` integration-test executables | terminal transcript — confirms `P1-AGENT-MUTATION-IDEMP-001`'s composition tests against `finish_run` still type-check against the new signature/behavior; execution itself is blocked by the host policy above, consistent with the task's "compile with `--no-run` and record separately" instruction |

Mandatory verification not run:

- `cargo test -p loki-acceptance-store -- --test-threads=1` (actual execution): blocked by host Application Control (`os error 4551`), a pre-existing host limitation named explicitly in the task instructions as expected and not to be looped on. Compilation was verified instead (`--no-run`, passes).
- `cargo test -p loki-agents --doc`: blocked by the same host Application Control policy; the crate has no meaningful runnable doctests (its doc comments are prose, not `rust` code fences with assertions), so this gap has no material coverage impact.

## 5. Contracts, migrations, and compatibility

- Contracts changed: none. `finish_run`'s public signature and `AgentError` variants are unchanged; `RunState`/wire encoding unchanged.
- Consumers affected: `loki-acceptance-store`'s composition tests from `P1-AGENT-MUTATION-IDEMP-001` call into `loki-agents`; verified to still compile against the new `finish_run` (see §4). Behavior for well-formed callers (correct connection/secret/scope/ownership) is unchanged — only the previously-missing atomicity and the previously-silent-on-conflict path are corrected.
- Migration: none — no schema change; `crates/loki-storage/src/agents.rs`'s `finish_agent_run_core` SQL (`UPDATE ... WHERE state='running'`) was already present and unchanged by this node.
- Backward/rolling compatibility: unaffected.
- Generated-contract drift: n/a.
- Build recipe/executor/provenance compatibility: n/a.
- Cloud API/offline/notification compatibility: n/a.

## 6. Security, privacy, and device safety

- Trust boundaries touched: none newly crossed. This is entirely within the existing `loki-agents`/`loki-storage` local-authority-gated agent-run boundary already documented in this module's top-of-file doc comment.
- Authorization/approval impact: none — `finish_run` still requires the exact same `AgentScope::RegisterRuns` grant and active (non-revoked) connection it always did; this node only closes a TOCTOU-shaped gap where authentication/scope/ownership could be validated in one moment and the terminal write applied in a later, unguarded moment, and makes revocation checking happen at the actual instant of the state transition rather than only earlier in the call.
- Secrets/data handling: unchanged — `authenticate_connection` still does constant-time secret comparison against the stored hash; no new secret material introduced.
- Containment profile and denied authorities: n/a to this node.
- Device side effects performed: none. No real device-changing operation was performed or requested.
- New dependencies: none.
- Remaining risk: `Storage` is single-connection/`Mutex`-guarded by design (documented in `crates/loki-storage/src/storage.rs`'s `Storage` doc comment as this crate's deliberately bounded scope, deferring connection pooling to `RUN-002`). This means "concurrency" here is real-OS-thread contention for that one mutex plus a real SQLite transaction, not multi-connection database-level locking — the added test exercises exactly that real contention path and is an honest test of the actual deployed concurrency model, not a simulation of a model this crate doesn't implement.

## 7. Decisions and assumptions

- Interpreted "restart-durability test" as satisfied by the drop/reopen section appended to the same concurrency test (`file_backed_concurrent_terminal_attempts_commit_exactly_one_result`) rather than a second, fully separate test function, since it exercises the exact required scenario (fresh `Storage`/`AgentRegistry::open` reopened over the same on-disk file, asserting the terminal result survived) using the more interesting of the two committed values (the actual concurrency-race winner) rather than a trivial single-writer case already covered elsewhere by `restart_preserves_revocation`/`restart_preserves_generation_across_reopen`. If the orchestrator wants a fully independent, single-purpose test for readability, that is a small, low-risk follow-up split, not a behavior gap.
- `cargo test -p loki-acceptance-store -- --test-threads=1` could not actually execute on this host (Application Control `os error 4551`), matching the task instructions' explicitly named known limitation; verified via `--no-run` compilation instead and did not attempt workarounds (e.g., disabling Application Control) since that is outside this node's scope and the instructions say not to loop on it.

## 8. Review focus

- The transaction closure in `finish_run` (`crates/loki-agents/src/lib.rs`): confirm every internal call inside the closure uses a `_core` function taking `&Transaction`/`&Connection` directly, never a `self.storage.*`/trait-repository method — any such call would deadlock the single `Mutex<Connection>` `Storage` wraps, since `Storage::transaction` already holds that mutex for the closure's entire duration.
- The rows-affected branch: `finish_agent_run_core` returns `false` both when the run no longer exists (shouldn't happen here, already checked) and when its state is no longer `'running'` (the real race case) — confirm the re-read-and-compare fallback correctly treats "same state as requested" as success and any other state as `TerminalRun`, with no way to observe or commit a torn/partial write in between (the whole branch runs inside the same still-open transaction as the original guarded `UPDATE`).
- `authenticate_connection`'s extraction into a free function: confirm it is byte-for-byte the same authorization logic previously inlined in `authenticate()` (revocation check first, then constant-time secret comparison, then scope membership) — no check was reordered, weakened, or dropped in the extraction.
- The concurrency test's real-thread structure (`Barrier`, `mpsc::channel`, `Arc<AgentRegistry>` over a real tempdir-backed encrypted file): confirm it is a genuine race (both threads released simultaneously via the 3-party barrier) against a real file-backed `Storage`, not a sequential call disguised as a thread test.

## 9. Follow-up graph changes proposed

- None. This is a correction to an already-accepted node's implementation; no new node/edge is proposed.

Only the orchestrator updates the task graph.
