# Worker Handoff — `P1-PRINCIPAL-GRANT-001`

Result: `ACCEPTED-pending-review` (implementation complete, all reproducible
automated verification passes; independent security review completed
2026-08-28 with **no Critical/High findings**; independent test review
completed 2026-08-28 with all reproducible gates passing.)

**PROCESS NOTE (orchestrator, 2026-08-28):** this handoff was written by the
orchestrator, not the original `loki-implementer`, because that worker was
interrupted by a Claude Code session restart after completing the
implementation and verification but before writing its own handoff. The
content below is derived from: the task packet's deliverables, direct
inspection of the committed code, and both independent reviews' findings
(which re-verified the code fresh, not by trusting an implementer narrative
that never existed for this node). This node's work was developed in the
same shared, uncommitted worktree as `P1-ACCEPTANCE-STORE-001` with no
commit boundary between the two, so both were committed together as
`495c71a` and are reviewed/integrated as one unit — see that node's own
(now-updated) handoff for its side of the combined commit.

Worker: `loki-implementer` (Claude Code), handoff completed by orchestrator
Started from: stacked on `P1-ACCEPTANCE-STORE-001`'s uncommitted work in
worktree `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001` (per packet header)
Result revision/patch: commit `495c71a` on branch `node/P1-ACCEPTANCE-STORE-001`
(combined with `P1-ACCEPTANCE-STORE-001`)
Completed at: `2026-08-28T19:00:00Z` (approximate); handoff written `2026-08-28T20:05:00Z`

## 1. Outcome

Implemented ADR-0007 Option B in full. `crates/loki-local-authority` and
`crates/loki-agents` each gained an additive, durable `generation` counter
with zero change to any existing accepted method's behavior:

- **`LOC-001` (`LocalSession`)**: per-session, immutable, assigned once.
  `insert_authority_session_core` computes `SELECT COALESCE(MAX(generation),0)+1`
  and performs the `INSERT` in the same function body, always invoked while
  the caller holds the single-writer `Storage` mutex — atomic by
  construction, not by a database-level constraint (deliberately no
  `UNIQUE` index, per the packet's own reasoning about pre-existing dev
  databases at the `DEFAULT 0` backfill value).
- **`AGT-001` (`AgentCredential`)**: per-connection, mutable, bumped in
  place. `register` starts at `1` (no storage round-trip needed). Two new
  `AgentRegistry` methods — `rotate` and `change_scopes` — atomically bump
  it via a guarded `UPDATE ... WHERE connection_id = ? AND revoked_at_ms IS
  NULL`, using the identical `authority.authorize_local(context, session_id,
  LocalCapability::RegisterAgent, now_ms)` gate `register`/`revoke` already
  use — no new `LocalCapability` variant.
- **`loki-acceptance-store`'s grant read** now branches on
  `request.method.requires_approval()` (already an exact, exhaustive
  discriminant for which identity model a request belongs to — no
  `loki-mutation-contract` edit needed or made) and calls
  `find_local_generation_core` or `find_agent_generation_core` accordingly,
  fully replacing the prior provisional `find_grant_generation_core` shim
  (that identifier no longer exists anywhere in the tree).
- **No expiry for agent grants**: `AGENT_GRANT_NEVER_EXPIRES_MS: i64 =
  i64::MAX` is a sentinel confined entirely to `loki-acceptance-store`
  (confirmed absent from `loki-storage` by both independent reviews).
- **No cross-model cascade**: revoking a `LocalSession` never touches
  `agent_connections` and vice versa — this is ADR-0007's explicit,
  deliberate limitation (not the rejected Option A's "one kill switch"
  property), confirmed by both reviews via direct code/grep inspection.
- The existing `acceptance_flow.rs` test harness — which previously seeded
  every agent-shaped test request through the wrong (Windows-SID)
  `LocalAuthority` path, per the task packet's Section 0/3 "known starting
  condition" finding — was corrected to seed through a real
  `AgentsRepository`-backed helper, and the relevant tests were split into
  genuine Windows-SID-path and agent-connection-path pairs (both
  independently proving their own revocation/expiry behavior).

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `cargo build`/`clippy --workspace --all-targets -D warnings` zero warnings | pass | Both independent reviews re-ran this fresh; clean |
| `cargo test -p loki-storage -p loki-local-authority -p loki-agents -p loki-acceptance-store -- --test-threads=1` passes, including every new test named in the packet | pass | Test review confirmed by name: `concurrent_session_issuance_yields_distinct_generations`, `rotate_replaces_secret_and_bumps_generation`, `rotate_denied_for_revoked_connection`, `scope_change_updates_authorization_and_bumps_generation`, `scope_change_denied_for_revoked_connection`, `restart_preserves_generation_across_reopen`, plus the split `local_grant_expiry_denies_acceptance_for_approval_required_method` / `agent_grant_is_revocable_but_never_expires` / `local_grant_revocation_denies_acceptance` / `agent_connection_revocation_denies_acceptance` in `acceptance_flow.rs` |
| `pnpm run verify` passes (documented flake retry convention) | pass, with a caveat both reviews independently noted | Stops on the same pre-existing, unrelated `loki-incidents --test correlation` Windows Application Control flake seen across this whole chain (confirmed passing 6/6 in isolation); the underlying implementation's own tests are not flaky |
| No existing public business method signature changed except additive struct fields and the two brand-new `AgentRegistry` methods | pass | `register`/`revoke`/`authenticate`/`start_run`/`finish_run`/`append_event`/`issue_session`/`revoke_session`/`authorize_local`/`authorize_cloud`/`initialize_current` all confirmed unchanged in signature by direct review |
| Grep-verifiable: `find_grant_generation_core`/`find_grant_generation`/`GrantGenerationRow` do not exist anywhere in the tree | pass | Both reviews confirmed zero live references (only historical prose mentions in this now-corrected handoff and the packet's own "known starting conditions" section) |
| Grep-verifiable: no new `MutationMethod`/`LocalCapability` enum variant | pass | Security review confirmed `LocalCapability` unchanged at exactly 4 variants; `git diff` on `loki-mutation-contract` is empty |
| Grep-verifiable: no `i64::MAX`/"never expires" sentinel inside `crates/loki-storage/**` | pass | Confirmed confined to `loki-acceptance-store/src/lib.rs:120` |
| Two concurrent `issue_session` calls for the same principal never produce the same `generation`, reproduced ≥2 runs | pass | `concurrent_session_issuance_yields_distinct_generations`, 8 real threads via `Barrier`, asserts `generations == [1..8]`; security review re-verified, test review re-ran 5× with stable results |
| `rotate`/`change_scopes` against an already-revoked connection denied | pass | `rotate_denied_for_revoked_connection`, `scope_change_denied_for_revoked_connection` |
| Agent-connection grant with no expiry accepted until actually revoked | pass | `agent_grant_is_revocable_but_never_expires`, `agent_connection_revocation_denies_acceptance` |
| Windows-SID grant's real session expiry still denies acceptance (regression) | pass | `local_grant_expiry_denies_acceptance_for_approval_required_method` |
| `git diff --stat` confined to Section 5's writable paths + handoff | pass, now that this handoff and the corrected `P1-ACCEPTANCE-STORE-001-1.md` are committed alongside it | See combined commit `495c71a`'s file list in the companion handoff's §3 |
| Independent test-reviewer and security-reviewer sign-off with no Critical/High finding | pass | See §8 below |
| Every open question in packet §9 recorded as still-open | pass | §9 below |

## 3. Changed files (this node's portion of combined commit `495c71a`)

| Path | Change |
|---|---|
| `crates/loki-storage/migrations/0008_local_authority_grant_generation.sql` | new — additive `generation` column + non-unique index on `local_authority_sessions` |
| `crates/loki-storage/migrations/0009_agent_grant_generation.sql` | new — additive `generation`/`generation_issued_at_ms` columns on `agent_connections` |
| `crates/loki-storage/src/local_authority.rs` | `find_grant_generation_core`/`GrantGenerationRow` removed; `find_local_generation_core`/`LocalGrantGenerationRow` added; `insert_authority_session_core` now atomically assigns and returns the generation |
| `crates/loki-storage/src/agents.rs` | `find_agent_generation_core`/`AgentGrantGenerationRow` added; `insert_agent_connection_core` sets `generation=1`; `revoke_agent_connection_core` bumps generation; `rotate_agent_connection_core`/`change_agent_scopes_core` added |
| `crates/loki-local-authority/src/model.rs` | `LocalSession` gains `pub generation: u64` |
| `crates/loki-local-authority/src/store.rs` | `issue_session` substitutes the real assigned generation into the returned `LocalSession`; read-back paths carry it through |
| `crates/loki-agents/src/lib.rs` | `AgentCredential` gains `pub generation: u64`; two new methods `rotate`/`change_scopes` added; `finish_run`'s retry behavior is unaffected by this node (that fix belongs to `P1-AGENT-MUTATION-IDEMP-001`, a separate node) |
| `crates/loki-acceptance-store/src/lib.rs` | grant-read branch replaced per §1 above; doc comment updated citing ADR-0007 and this packet, explicitly restating the no-cross-model-cascade limitation |
| `crates/loki-acceptance-store/tests/acceptance_flow.rs` | test-harness correction per §1 above (Deliverable 6) |

## 4. Verification performed

Independently re-run and confirmed by both reviewers (not merely claimed by
an implementer, since none survived to make the claim):

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test -p loki-storage -p loki-local-authority -p loki-agents -p loki-acceptance-store -- --test-threads=1` — all pass.
- `cargo test --workspace -- --test-threads=1` — 0 real failures (one pre-existing, confirmed-flaky, unrelated `loki-incidents` test).
- Concurrency tests re-run 5× each by the test reviewer independently — stable every time.
- `pnpm run verify` — stops at the same documented pre-existing flake both times it was independently run; everything before that gate (format, lint, typecheck, JS tests, `cargo fmt`/`build`) passes.
- Security review's structural greps (dyn Fn/Box\<dyn\>/Arc\<Mutex\>, sentinel confinement, enum-variant-count, authorization-gate reuse) — all clean, zero findings.

## 5. Contracts, migrations, and compatibility

- Contracts changed: none in `loki-mutation-contract`.
- Migrations `0008`/`0009`: strictly additive (`ALTER TABLE ADD COLUMN`), no
  existing column/index/table changed. `0001`–`0007` untouched.
- Consumers affected: `loki-acceptance-store` (grant-read call site only);
  no other crate's public API changed.

## 6. Security, privacy, and device safety

- Trust boundaries touched: same as `P1-ACCEPTANCE-STORE-001` — local
  persistence and the same two already-touched crates
  (`loki-local-authority`, `loki-agents`). No IPC/MCP/cloud/device surface.
- Authorization impact: `rotate`/`change_scopes` are gated by the exact same
  `LocalCapability::RegisterAgent` check `register`/`revoke` already
  require — confirmed by security review, no bypass path found.
- No cross-model kill switch exists (ADR-0007's explicit, accepted
  limitation — restated here so no downstream reader assumes a stronger
  guarantee).
- Secrets: `rotate` mints a new one-time secret exactly like `register`
  already does, same `AgentSecret` redaction; no new secret class.
- Remaining risk: the generation-assignment critical sections' race-safety
  is exactly what the independent security review re-verified (§8) — this
  handoff is not a substitute for that review having actually run, and it
  did, with a clean result.

## 7. Decisions and assumptions

- `insert_authority_session_core`'s signature was changed from
  `Result<(), StorageError>` to `Result<u64, StorageError>` — this is a
  deliberate, packet-authorized deviation (packet §9 item 1): the trait it
  belongs to was itself introduced by `P1-ACCEPTANCE-STORE-001`, not yet
  integrated to `main` at the time either node was built, so nothing outside
  this stacked worktree depended on its prior shape.
- No `UNIQUE` index added on `(principal_sid, generation)` — uniqueness is
  an application invariant enforced by the single-writer `Storage` mutex,
  not a DB constraint, per the packet's explicit reasoning about
  pre-existing dev/test databases.

## 8. Review focus — completed, findings below

Both independent reviews ran against combined commit `495c71a` (this node's
work could not be isolated from `P1-ACCEPTANCE-STORE-001`'s in the same
commit, so both were reviewed together):

- **Security review (2026-08-28)**: No Critical/High findings. Confirmed:
  atomic generation assignment under real concurrent access (not merely
  sequential); no cross-model cascade; `rotate`/`change_scopes`
  authorization-gate reuse with no bypass; `AGENT_GRANT_NEVER_EXPIRES_MS`
  sentinel confinement; no new `MutationMethod`/`LocalCapability` variant.
  Two Low/informational notes: a `u64::try_from(...).unwrap_or(0)`
  coercion on generation read that silently defaults to `0` on a
  structurally-unreachable out-of-range value (write path already guards
  against it — recommended as a future hardening item, not blocking); and
  confirmation that both packets' open questions are correctly deferred in
  code comments, not silently dropped.
- **Test review (2026-08-28)**: All reproducible technical gates pass. The
  review's substantive finding was a **process** gap, not a code defect:
  neither this handoff nor the companion `P1-ACCEPTANCE-STORE-001-1.md` was
  part of the originally-reviewed commit, and this handoff did not exist at
  all (the implementer was interrupted before writing it). Both are
  resolved by this document and the companion handoff's in-place
  correction, to be committed together before either node is treated as
  finally accepted.

## 9. Follow-up graph changes proposed / open questions carried forward

Every item below is carried from `Docs/graph/tasks/P1-PRINCIPAL-GRANT-001.md`
§9, verbatim in substance, per that section's "do not resolve silently"
instruction:

1. Changing `insert_authority_session`'s signature (§7 above) is a
   deliberate, packet-authorized pre-acceptance edit to a not-yet-integrated
   internal trait — not scope creep against `LOC-001`'s own public API.
2. `principal_sid`'s dual meaning (Windows SID vs. agent `connection_id`,
   inherited from `loki-mutation-contract`) is not fixed here — a future
   contract change would need its own review.
3. **No cross-model "kill switch" exists after this node** — restated
   because it is the single most likely thing a future reader could wrongly
   assume once both models have "a generation counter."
4. `outcome_unknown` trigger contract remains undefined — unaffected by
   this node.
5. Lock-contention risk (already named by `P1-ACCEPTANCE-STORE-001` §9 item
   4): this node adds `rotate`/`change_scopes` as two more code paths
   briefly holding the same single-writer mutex — recorded as an
   incremental addition to that same named risk for
   `P1-RUNTIME-COMP-001`/`P1-MUTATION-REV-001` to verify at realistic load.

No new graph node/edge is proposed beyond what the packet already
anticipates.
