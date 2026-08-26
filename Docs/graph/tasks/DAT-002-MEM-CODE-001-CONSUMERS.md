# Task Packet — `DAT-002-MEM-CODE-001-CONSUMERS Consumer Test-Harness Update`

Packet status: `accepted`
Graph status: `ready`
Created by: `orchestrator (Claude Code)`
Created at: `2026-08-25T00:00:00Z`
Starting revision: worktree `L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001` at its
current uncommitted state (see `Docs/graph/handoffs/DAT-002-MEM-CODE-001-1.md`)

## 1. Objective

`DAT-002-MEM-CODE-001` correctly made `Storage::open_managed` fail closed
without an accepted secure-memory profile (ADR-0005's whole point: every
SQLCipher consumer must refuse to open unprotected storage, not just
`loki-storage`'s own tests). That change broke the test suites of 9 other
crates that also call `Storage::open_managed` in their own tests, because
they were never updated to use the same `loki_storage::secure_memory::
test_support` disposable-arena bootstrap that `DAT-002-MEM-CODE-001` added
for `loki-storage`'s own tests. This node updates only those 9 crates'
**test harnesses** to use that same bootstrap so `pnpm run verify` passes
workspace-wide. It makes **zero production code changes** — the fail-closed
production behavior is correct and must not be weakened, worked around, or
bypassed outside `#[cfg(test)]`.

## 2. Traceability

- Parent: `DAT-002-MEM-CODE-001`, `Docs/graph/handoffs/DAT-002-MEM-CODE-001-1.md`
  §7-9 (this exact gap, flagged by that node's own implementer)
- Requirements: `NFR-DATA-002`, `NFR-SEC-002` (same as parent — this node must
  not weaken them)
- Decision: `ADR-0005`

## 3. Dependencies and inputs

- `depends_on`: `DAT-002-MEM-CODE-001`'s in-worktree state (not yet formally
  accepted — this node works in the same worktree, does not duplicate it)
- Consumes: `loki_storage::secure_memory::test_support` (already implemented
  in this worktree by `DAT-002-MEM-CODE-001`) as the exact pattern to
  replicate — read how `loki-storage`'s own `tests/common/mod.rs` uses it
  before touching any consumer crate.
- Environment: same worktree — `L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001`,
  standard user, no elevation/VM.

## 4. Required reading

- `AGENTS.md`
- `Docs/graph/handoffs/DAT-002-MEM-CODE-001-1.md` (full — especially §5, §7-9)
- `crates/loki-storage/src/secure_memory.rs` (the `test_support` module) and
  `crates/loki-storage/tests/common/mod.rs` (the exact pattern already
  applied for `loki-storage`'s own tests — replicate this, do not invent a
  new pattern)

## 5. Ownership

Writable paths — **test code only**, in each of these 9 crates:

- `crates/loki-monitoring/tests/**` (and any `#[cfg(test)]` test-setup helper
  inside `src/` if that crate's tests are inline rather than in `tests/`)
- `crates/loki-local-notifications/tests/**`
- `crates/loki-local-setup/tests/**`
- `crates/loki-local-authority/tests/**`
- `crates/loki-local-devices/tests/**`
- `crates/loki-incidents/tests/**`
- `crates/loki-approvals/tests/**`
- `crates/loki-agents/tests/**`
- `crates/loki-agent-delivery/tests/**`
- Each crate's `Cargo.toml` — **only** if a `dev-dependencies`/feature line is
  needed to reach `loki-storage`'s `test-support` feature (mirror exactly how
  `loki-storage`'s own `Cargo.toml` exposes it), no other dependency change
- `Docs/graph/handoffs/DAT-002-MEM-CODE-001-CONSUMERS-1.md`

Forbidden:

- Any change to `src/` production code (non-test) in any of the 9 crates,
  or to `crates/loki-storage/**` itself, or to `.cargo/config.toml`
- Any weakening of `Storage::open_managed`'s fail-closed behavior, any
  `#[cfg(not(test))]` bypass, or any change reachable outside `#[cfg(test)]`
- Root `Cargo.toml`/`Cargo.lock` changes beyond what's mechanically required
  by the per-crate `dev-dependencies` addition (flag any such change
  explicitly in the handoff, same discipline the parent node used)
- Elevation, VM, device, real user data

## 6. Deliverables

1. Each of the 9 crates' test suites updated to obtain a test-mode disposable
   secure-memory arena via `loki_storage::secure_memory::test_support`,
   exactly mirroring `loki-storage`'s own `tests/common/mod.rs` pattern,
   before any `Storage::open_managed` call in their tests.
2. `cargo test --workspace` passes (this is the actual acceptance bar — it's
   what `pnpm run verify` depends on).
3. Handoff recording, per crate, exactly what changed and confirming no
   production code path was touched.

## 7. Acceptance criteria

- [ ] `cargo test --workspace` passes.
- [ ] `pnpm run verify` passes (or fails only on the pre-existing, unrelated
      `openssl-sys`/Application-Control-policy environment issue already
      flagged in `BLD-001-R2-CODE`'s handoffs — if so, state that explicitly
      with evidence it's the same pre-existing issue, not a new one).
- [ ] `git diff --stat` shows changes confined to `tests/**` (and the minimal
      necessary `Cargo.toml`/`Cargo.lock` lines) across exactly the 9 named
      crates, plus this node's own handoff — no `src/` production file in any
      crate, and nothing in `crates/loki-storage/**`.
- [ ] No test was weakened, skipped, or deleted to make the suite pass —
      every previously-passing test still exercises the same behavior it did
      before, just via the new test-mode bootstrap instead of failing closed.
- [ ] Handoff explicitly confirms production fail-closed behavior is
      unchanged and unbypassed outside test code.

## 8. Required verification

| Layer | Command | Expected result | Evidence |
|---|---|---|---|
| Unit/Integration | `cargo test -p loki-monitoring -p loki-local-notifications -p loki-local-setup -p loki-local-authority -p loki-local-devices -p loki-incidents -p loki-approvals -p loki-agents -p loki-agent-delivery --all-targets` | all pass | run summary |
| Workspace | `cargo test --workspace` | all pass | run summary |
| Workspace | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | run log |
| Full | `pnpm run verify` | passes, or fails only on the pre-existing unrelated `openssl-sys` environment issue (state which, with evidence) | run log |
| Scope | `git diff --stat` | confined to Section 5's writable paths only | diff output |

## 9. Risks and safety

- Risk: `low` — test-code-only, no production path change, no new
  dependency, no I/O beyond what these crates' existing tests already did.
- Failure mode if done wrong: a test-mode bypass accidentally reachable in
  production would be `critical` — this is why Section 5's forbidden list is
  explicit and why the handoff must state affirmatively that no such path
  exists.
- Rollback: revert the touched test files; no persisted state.

## 10. Human gate

None. Pure test-harness consistency work with an explicit, checkable
non-production-impact constraint. Independent test review still required
before acceptance (Section 11), same as any node.

## 11. Worker and reviewer

- Worker: `loki-implementer`
- Reviewer: `loki-test-reviewer` (primary — verify the fail-closed production
  path is genuinely untouched, not just that tests pass) and
  `loki-security-reviewer` (secondary — confirm no test-mode bypass is
  reachable outside `#[cfg(test)]`)
- Worktree: `L:\LokiAI-v1.2-wt\DAT-002-MEM-CODE-001` (same as parent — do not
  create a separate worktree, this node's changes are additive to it)
- Handoff: `Docs/graph/handoffs/DAT-002-MEM-CODE-001-CONSUMERS-1.md`

## 12. Stop conditions

Return `NEEDS_REPLAN` if any of the 9 crates' tests cannot be fixed via the
test-mode bootstrap pattern alone (e.g. if one has a structurally different
storage-access pattern) — do not invent a different workaround for that one
crate; report it specifically instead.
