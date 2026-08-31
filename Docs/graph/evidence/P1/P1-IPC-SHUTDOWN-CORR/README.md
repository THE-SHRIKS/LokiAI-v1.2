# Integration Verification — `P1-IPC-SHUTDOWN-CORR`

Integrated at: `LokiAI-v1.2` merge commit (`--no-ff` merge of
`node/P1-IPC-SHUTDOWN-CORR`@`fef20ad` onto `LokiAI-v1.2`@`84168b5`)
Integrator: `loki-integrator` (Claude Sonnet 5, Claude Code)
Date: `2026-08-31`

## 1. Source

- Worktree: `L:\LokiAI-v1.2-wt\P1-IPC-SHUTDOWN-CORR`
- Branch: `node/P1-IPC-SHUTDOWN-CORR`
- Commits merged (in order): `85fa601` (first-round fix: `Server::serve`
  drain-on-shutdown), `0cc6fde` (handoff), `f3023ab` (second-round fix:
  closes outer/inner shutdown-timeout mismatch and tracks per-call tasks),
  `fef20ad` (handoff §10 addendum documenting the second round).
- Starting revision: `LokiAI-v1.2`@`84168b5` (confirmed ancestor via
  `git merge-base --is-ancestor 84168b5 fef20ad`).
- Merge type: `git merge --no-ff`, zero conflicts. Diff confined to
  `crates/loki-ipc/src/server.rs`, `crates/loki-ipc/tests/shutdown_drain.rs`
  (new file), `crates/loki-runtime/src/host/compose.rs`,
  `crates/loki-runtime/tests/host_lifecycle.rs`, and
  `Docs/graph/handoffs/P1-IPC-SHUTDOWN-CORR-1.md` — exactly the disjoint set
  the dispatching instruction declared, confirmed by
  `git diff --stat 84168b5 fef20ad` before merge and an empty conflict list
  during merge.

## 2. Independent review record (pre-integration, per handoff)

- First round (`85fa601`): fixed `Server::serve` to track every spawned
  per-connection dispatch task in a `tokio::task::JoinSet` and drain them on
  shutdown via a new `watch`-based signal, bounded by a grace period then
  force-abort. Reported `ACCEPTED-pending-review`.
- An independent security review of the first round found it did **not**
  actually close the reported crash end-to-end and named two findings
  (handoff §10):
  - **Critical**: `compose.rs::ComposedHost::run` raced `ipc_task` against
    its own, shorter (5s), non-cancelling outer `IPC_SHUTDOWN_GRACE` timeout
    that proceeded to tear down storage/secure-memory regardless of whether
    `ipc_task` had actually finished — reintroducing the exact
    use-after-free-class hazard for any connection whose drain took 5-8+
    seconds.
  - **High**: `spawn_unary_call`/`spawn_stream_call` inside
    `run_dispatch_loop` were still bare-`tokio::spawn`ed, untracked by any
    `JoinSet`, with no real guarantee they finished before their connection
    was considered done.
- Second round (`f3023ab`) closed both: removed the outer race in
  `compose.rs` entirely (now `.await`s `ipc_task` directly, relying on
  `loki-ipc`'s own now-enforced bounded-then-abort-then-confirm shutdown as
  the single owned bound), and gave `run_dispatch_loop` a connection-local
  `JoinSet<()>` for per-call tasks, drained (`CALL_TASK_DRAIN_GRACE` = 10s)
  then abort-and-confirmed before the connection's own task is considered
  finished. Grace periods recalibrated (`CONNECTION_DRAIN_GRACE` 8s → 20s)
  so the reviewer's named 6-9s in-flight-call window is provably a
  "genuinely finishes" case, not merely a "safely aborted" one.
- Independent security review of the second round: **PASS, no remaining
  Critical/High findings** (per the dispatching instruction). Two narrow
  Medium/Low residual items noted as non-blocking follow-ups (§5 below).

## 3. Integration verification performed (this pass, on the merged baseline)

| Command | Result | Notes |
|---|---|---|
| `cargo build --workspace` | pass | `Finished` dev profile; only pre-existing, unrelated `openssl-sys` PDB `LNK4099` linker notices |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass | zero warnings |
| `cargo test -p loki-ipc -- --test-threads=1` | pass | 108/108 (106 pre-existing + 2 new in `tests/shutdown_drain.rs`) |
| `cargo test -p loki-runtime -- --test-threads=1` | pass | 72/72, including the new `host_lifecycle::slow_business_call::run_does_not_return_until_a_slow_in_flight_business_call_actually_finishes` test |
| `cargo test --workspace -- --test-threads=1` | pass after several documented-flake retries | Windows Application Control (`os error 4551`) transiently blocked freshly-linked test binaries across several unrelated crates during this integration session (`loki-agent-delivery`, `loki-agents`, `loki-ipc --test business_rpc`, `loki-ipc --test deadline`, `loki-local-authority`), each in a different crate on a different attempt — the documented, known, host-level, random-crate hazard (not a code defect, always resolves on retry/rebuild). Each blocked binary was individually deleted and rebuilt (forcing a fresh hash) and confirmed passing; the workspace run then completed with 86/86 `test result: ok` blocks and zero failures. |
| `pnpm run verify` | pass, exit 0 | full pipeline: `format:check`, `lint`, `typecheck`, `test`, `electron:typecheck`, `electron:test`, `cargo:fmt:check`, `cargo:build`, `cargo:test` — all stages exit 0 |

## 4. Acceptance criteria cross-check

All criteria from the handoff (`Docs/graph/handoffs/P1-IPC-SHUTDOWN-CORR-1.md`
§2 for the first round, §10.5 for the second) were already verified by the
worker and re-confirmed here on the integrated baseline:

- `Server::serve` reliably waits for every spawned per-connection dispatch
  task before returning: confirmed — `crates/loki-ipc/tests/shutdown_drain.rs`
  passes (2/2), both tests independently confirmed to fail against the
  pre-fix code per the handoff.
- Every existing behavior (auth, framing, deadlines, streaming,
  business-method dispatch) preserved: confirmed — `Connection::handle`'s
  public signature is byte-for-byte unchanged; all 106 pre-existing
  `loki-ipc` tests pass unmodified.
- No security check weakened: confirmed — diff is additive shutdown-sequencing
  and task-tracking plumbing only; `authorize_method`/`authorize_stream`/
  `revalidate`/ACL/handshake logic untouched.
- Proven against the actual reported crash through the real
  `ComposedHost::run` path, not `Server::serve` in isolation: confirmed —
  the new `host_lifecycle::slow_business_call` test drives a real 7s
  `business_call` through the actual composed host, fails against
  deliberately-reverted pre-fix code shapes (`run()` returned at ~5.4s while
  the call was still in flight), and passes against the restored fix
  (`run()` waits the full ~7s).
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D
  warnings`: confirmed clean on the integrated baseline (§3 above).
- `git diff --stat` confined to the five declared files: confirmed at merge
  time (§1 above).

## 5. Follow-ups (tracked, not silently dropped)

Per the dispatching instruction, both residual items from the second-round
review are recorded here as tracked follow-ups, not accepted silently:

1. **Medium/Low — secondary nested-abort-without-confirm edge case.** Only
   reachable if a connection is already catastrophically stuck beyond the
   normal 20s `CONNECTION_DRAIN_GRACE` bound. Not required to close the
   reproduced crash (the outer `JoinSet::abort_all()` + `Drop` behavior
   still transitively aborts everything outstanding in that pathological
   case — see handoff §10.2's closing paragraph). Candidate for a future
   narrow follow-up node if the orchestrator judges it worth closing
   proactively.
2. **Low — doc-comment overclaim.** Cosmetic; does not affect behavior,
   security, or test evidence. Candidate for a trivial future documentation
   fix, not release-blocking.

Also carried forward from the first-round handoff (§6/§7 decision 5) and
superseded by the second round's fix: the originally-flagged "per-call
dispatch tasks untracked" risk is now closed (§2/§10.2 above), not merely
mitigated.

## 6. Result

`INTEGRATED_PASS`. No conflicts required return to the worker. No contract,
migration, dependency, security, or device-safety issue found on the
integrated baseline beyond the two explicit, tracked, non-blocking
follow-ups in §5. Unblocks `P1-RUNTIME-COMP-001` (still `NEEDS_REPLAN` in
worktree `L:\LokiAI-v1.2-wt\P1-RUNTIME-COMP-001`, commit `9149a06`; not
touched by this integration) to re-dispatch against this fix.
