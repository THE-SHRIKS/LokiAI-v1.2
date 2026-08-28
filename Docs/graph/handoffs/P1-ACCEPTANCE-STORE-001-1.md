# Worker Handoff — `P1-ACCEPTANCE-STORE-001`

Result: `ACCEPTED-pending-review` (implementation + all required automated
verification pass; independent security review completed 2026-08-28 with
**no Critical/High findings**; independent test review completed 2026-08-28
with all reproducible gates passing — see §2/§8 below for both reviews'
findings folded in.)

**SUPERSEDED-AND-UPDATED NOTICE (2026-08-28, orchestrator):** this handoff
originally described this node in isolation, before `P1-PRINCIPAL-GRANT-001`
was developed on top of it in the same uncommitted worktree with no commit
boundary between the two. Both nodes' work was committed together as
`495c71a` (see commit message) and must be reviewed/integrated as one unit.
This handoff has been updated in place to describe the actual code as
committed — in particular, §3/§7/§9 below now reflect that the
`find_grant_generation_core` provisional shim this handoff originally
described **no longer exists**; it was fully replaced by
`P1-PRINCIPAL-GRANT-001`'s `find_local_generation_core`/
`find_agent_generation_core`. See the companion handoff
`Docs/graph/handoffs/P1-PRINCIPAL-GRANT-001-1.md` for that node's own
deliverables, written separately by the orchestrator from the same
independent-review evidence, since that node's implementer was interrupted
by a session restart before writing its own handoff.
Worker: `loki-implementer` (Claude Code, acting as accountable worker per packet §13)
Started from: `LokiAI-v1.2` branch, tip `a06cd5d` (per packet header)
Result revision/patch: commit `495c71a` on branch `node/P1-ACCEPTANCE-STORE-001`
(worktree `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001`) — combined commit with
`P1-PRINCIPAL-GRANT-001`, see notice above
Completed at: `2026-08-28T15:10:00Z` (approximate; real host clock); handoff
corrected/committed `2026-08-28T20:00:00Z` (approximate)

## 1. Outcome

`Storage::transaction()` is now `pub` and is the real ADR-0006 seven-step
one-transaction mutation-acceptance boundary. `loki-storage` gained four new
consolidated repository modules (`approvals`, `agents`, `agent_delivery`,
`local_authority`) plus the new `mutation_acceptance` repository and
`migrations/0004`–`0007`, so `loki-approvals`/`loki-agents`/
`loki-agent-delivery`/`loki-local-authority` now share one physical
`Storage`/`Mutex<Connection>` instead of four independent SQLite
connections. `jobs.rs`/`outbox.rs` were refactored (no behavior change,
proven by their unmodified test suites passing) into connection-parameterized
cores callable from inside a transaction closure without deadlocking. The new
`crates/loki-acceptance-store` crate implements the real seven-step
transaction against `loki-mutation-contract`'s pure types, with a real
fault-injection/concurrency/race test suite against a real file-backed,
DPAPI-managed SQLCipher `Storage` (not in-memory-only). `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo build
--workspace`, `cargo test --workspace -- --test-threads=1`, and `pnpm run
verify` all pass with zero failures at the time of this handoff.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `cargo build --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` succeed with zero warnings | pass | §4 below; both ran clean, zero warnings, zero errors |
| `cargo test -p loki-storage -p loki-approvals -p loki-agents -p loki-agent-delivery -p loki-local-authority -p loki-acceptance-store -- --test-threads=1` passes, including every existing test in the four consolidated crates unmodified in intent | pass | §4; 49+7+6+7+7+16 = 92 tests, 0 failed. Existing tests' assertions unchanged (same behavior asserted); only construction paths and two disk-marker strings corrected (see §7) |
| `pnpm run verify` passes (one retry allowed on documented flaky Windows Application Control error) | pass | §4; passed on first attempt, exit code 0 |
| No existing public business method signature changed in the four consolidated crates except each store's constructor | pass | Every business method (`create`/`grant`/`consume`, `register`/`revoke`/`start_run`/`finish_run`/`append_event`/etc., `enqueue`/`claim`/`acknowledge`/etc., `initialize_current`/`issue_session`/`authorize_local`/`revoke_session`) kept its exact signature; only `open`/`open_in_memory` (kept, unchanged, back-compat) plus a new `new(Arc<Storage>)` constructor were touched — see §7 deviation note |
| `Storage::transaction()` is the only path `loki-acceptance-store` uses to reach the database — grep-verifiable | pass | `grep -rn "Connection::open\|open_or_migrate_sqlcipher" crates/loki-acceptance-store/src/*.rs` → no matches |
| Fault injection after each of the seven ADR-0006 steps leaves zero orphan rows, reproduced ≥2 times | pass | `fault_injection_after_each_step_leaves_zero_orphan_rows` in `crates/loki-acceptance-store/tests/acceptance_flow.rs`; real panics, real file-backed `Storage`, 2 full runs × 7 steps = 14 injected panics, all verified zero-orphan, connection remains usable after every one, plus a final real successful commit proving end-to-end health |
| Concurrent submission of the same idempotency key from two real OS threads yields exactly one committed row, one `Accepted`/one `ReplayedIdenticalAccepted`, never two `Accepted`s, never a panic | pass | `concurrent_submission_of_the_same_idempotency_key_yields_exactly_one_accepted`; real `std::thread::spawn`, real `Barrier`; re-run 5× consecutively with identical results (deterministic under `Storage`'s single-mutex design — see §7) |
| Approval expiring between request-receipt and lock-acquisition is rejected | pass | `approval_already_expired_by_the_time_the_transaction_runs_is_rejected` |
| Same-key/different-digest replay is rejected as `DigestConflict` | pass | `same_key_different_digest_rejected` |
| Revocation racing acceptance yields exactly one legal `RevocationDisposition` outcome under real thread interleaving | pass | `revocation_racing_acceptance_yields_exactly_one_legal_outcome`; real threads, real `Storage::revoke_authority_session` racing real `accept_mutation`; re-run 5× |
| No code path outside `loki-mutation-contract`'s own constructors can produce `Accepted`/`ReplayedIdenticalAccepted` | pass | `grep -rn "AcceptedRecord *{" crates/loki-acceptance-store/src crates/loki-storage/src` → no matches; `loki-acceptance-store` only ever calls `construct_accepted`/`replayed_result` |
| `git diff --stat` shows changes only under Section 5's writable paths, one added `Cargo.toml` line, and the handoff | pass, with 3 named, justified additions beyond the packet's literal list | See §7 — `crates/loki-storage/src/migrations.rs` and two pre-existing test files (`crates/loki-storage/tests/{corruption,migrations}.rs`) needed a mechanical count update (3→7) as a direct, unavoidable consequence of Deliverable 1 |
| Independent test-reviewer and security-reviewer sign-off with no Critical/High finding | pass | Security review (2026-08-28): no Critical/High; two Low/informational notes (a `u64::try_from(...).unwrap_or(0)` coercion on generation read, and confirmation open questions are correctly deferred, not dropped). Test review (2026-08-28): all reproducible technical gates pass (build/clippy/fmt/full workspace tests/5x-repeated concurrency tests/pnpm verify modulo the documented pre-existing `loki-incidents` flake); the review's process finding — that the handoff was not part of the original commit and this file was stale relative to `P1-PRINCIPAL-GRANT-001`'s changes — is what this update resolves |
| Every open question in §9 recorded as still-open | pass | §9 below, and cited verbatim in-code (`crates/loki-acceptance-store/src/lib.rs` top doc comment, `clock.rs`, `local_authority.rs`) |
| No prohibited security/architecture boundary weakened | pass | No edit to `secure_memory.rs`/`secure_profile.rs`/`managed.rs`/`loki-monitoring`/`loki-incidents`; `require_ready()` gates untouched; ADR-0001 three-file boundary untouched (this consolidation only touches the "core storage" file, exactly as scoped) |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-storage/migrations/0004_approvals.sql` | new — mechanical mirror of `loki-approvals`'s prior schema | yes |
| `crates/loki-storage/migrations/0005_agents.sql` | new — mechanical mirror of `loki-agents`'s prior schema | yes |
| `crates/loki-storage/migrations/0006_agent_delivery_and_local_authority.sql` | new — mechanical mirror of `loki-agent-delivery`'s and `loki-local-authority`'s prior schemas | yes |
| `crates/loki-storage/migrations/0007_mutation_acceptances.sql` | new — the `mutation_acceptances` table | yes |
| `crates/loki-storage/src/approvals.rs` | new — `ApprovalsRepository` trait + connection-parameterized cores | yes |
| `crates/loki-storage/src/agents.rs` | new — `AgentsRepository` trait + cores | yes |
| `crates/loki-storage/src/agent_delivery.rs` | new — `AgentDeliveryRepository` trait + cores | yes |
| `crates/loki-storage/src/local_authority.rs` | new — `LocalAuthorityRepository` trait + cores. **Updated by `P1-PRINCIPAL-GRANT-001` in this same commit**: the original `find_grant_generation_core` provisional shim described below in §7/§9 no longer exists — replaced by `find_local_generation_core` (real per-principal atomic generation counter) as that node's own Deliverable 4. | yes |
| `crates/loki-storage/src/mutation_acceptance.rs` | new — `MutationAcceptanceRepository` trait + cores | yes |
| `crates/loki-storage/src/jobs.rs` | narrow, additive-only — extracted `*_core` functions; no SQL/behavior change | yes |
| `crates/loki-storage/src/outbox.rs` | narrow, additive-only — same extraction | yes |
| `crates/loki-storage/src/storage.rs` | `transaction()` made `pub`, doc comment rewritten naming this node and its concurrency contract; no other change | yes |
| `crates/loki-storage/src/lib.rs` | added `pub mod`s for the five new modules; top doc comment updated | yes |
| `crates/loki-storage/src/migrations.rs` | **not literally named in Section 5** — required to register the four new migrations in `MIGRATIONS` (the vehicle Deliverable 1 itself demands) and to update its own colocated unit tests' hardcoded version-count assertions (3→7). See §7. | deviation, justified — see §7 |
| `crates/loki-storage/tests/corruption.rs`, `crates/loki-storage/tests/migrations.rs` | **not named in Section 5** — two pre-existing integration tests hardcoded `vec![1, 2, 3]`/table counts that mechanically had to become `vec![1, 2, 3, 4, 5, 6, 7]` once migrations `0004`–`0007` exist; otherwise `cargo test --workspace` (an explicit acceptance criterion) fails. See §7. | deviation, justified — see §7 |
| `crates/loki-approvals/src/lib.rs` | constructor/internal storage-access change only, per §5; `open`/`open_in_memory` kept as back-compat wrappers; new `new(Arc<Storage>)` added | yes |
| `crates/loki-agents/src/lib.rs` | same pattern | yes |
| `crates/loki-agent-delivery/src/lib.rs` | same pattern; multi-statement operations moved from private `transaction_with_behavior` to `Storage::transaction` | yes |
| `crates/loki-local-authority/src/store.rs` | same pattern | yes |
| `crates/loki-acceptance-store/Cargo.toml` | new crate manifest | yes |
| `crates/loki-acceptance-store/src/lib.rs` | new — the ADR-0006 transaction engine | yes |
| `crates/loki-acceptance-store/src/clock.rs` | new — local Unix-epoch-ms clock + ISO-8601 formatter (§9 item 1) | yes |
| `crates/loki-acceptance-store/tests/acceptance_flow.rs` | new — real file-backed integration/fault-injection/concurrency tests | yes |
| `Cargo.toml` (root) | one added line: `"crates/loki-acceptance-store",` | yes |
| `Cargo.lock` | updated by `cargo build` for the new crate + `serde_json` dependency | yes (mechanical, not hand-edited) |
| `Docs/graph/handoffs/P1-ACCEPTANCE-STORE-001-1.md` | this handoff | yes |

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | Windows, `CARGO_TARGET_DIR` pointed at the shared workspace target dir to reuse the cached `openssl-sys`/`bundled-sqlcipher-vendored-openssl` native build (the git-bash `perl` on `PATH` cannot configure OpenSSL; this is a pre-existing environment condition, not a code defect — see §7) | exit 0, no diff | ran twice (once found real diffs from my own uncorrected formatting, fixed via `cargo fmt --all`, then re-checked clean) |
| `cargo clippy --workspace --all-targets -- -D warnings` | same | exit 0, zero warnings | full transcript captured; two `large_enum_variant` findings (my own `ConsumeOutcome`/`EnqueueOutcome`/`FailureOutcome` variants) fixed by boxing |
| `cargo test -p loki-storage --all-targets -- --test-threads=1` | same | 49 lib + 4+6+5+3+13+2+5+2 integration = all pass, 0 failed | full transcript captured |
| `cargo test -p loki-approvals -p loki-agents -p loki-agent-delivery -p loki-local-authority -- --test-threads=1` | same | 7+6+7+7 = 27 tests, 0 failed | full transcript captured |
| `cargo test -p loki-acceptance-store` | same | 5 unit + 11 integration = 16 tests, 0 failed, including every named test (`same_key_same_digest_replays`, `same_key_different_digest_rejected`, `expired_grant_denies_acceptance`, `revoked_grant_denies_acceptance`, `approval_required_without_reference_never_reaches_construct_accepted_bypassed`, `agent_method_accepts_without_jobs_row`) | full transcript captured |
| Fault injection: real file-backed `Storage`, panic after each of 7 steps | same | 0 orphan rows every time, 2 runs × 7 steps = 14/14 | `fault_injection_after_each_step_leaves_zero_orphan_rows`, run individually 5× in a loop with identical results |
| Concurrency/race: two real OS threads, same idempotency key | same | exactly one committed acceptance, no panic, no deadlock, re-run 5× | `concurrent_submission_of_the_same_idempotency_key_yields_exactly_one_accepted` |
| `cargo test --workspace -- --test-threads=1` | same | 83 test binaries, 0 failures (includes every crate in the workspace, not only the six named) | full transcript, `grep -c "test result: FAILED"` → 0 |
| `pnpm run verify` | same, `pnpm install --frozen-lockfile` run first (fast — reused the existing content-addressable store) | exit 0 on first attempt (no retry needed) | full transcript captured; 0 errors (4 pre-existing, unrelated ESLint warnings in `LokiAI Desktop UI`, not touched by this node) |
| Security grep: `dyn Fn`/`Box<dyn ...>`-callback/`Arc<Mutex<...>>`-as-authorization, `AcceptedRecord` struct-literal construction, `Connection::open`/`open_or_migrate_sqlcipher` | same | zero matches in `crates/loki-acceptance-store/src` (and, for the `AcceptedRecord` check, `crates/loki-storage/src` too) | commands and outputs in this session's transcript |

Mandatory verification not run:

- Independent test-reviewer and security-reviewer review itself (Section 11's
  human gate) — this handoff is the request for that review, not a
  substitute for it.

## 5. Contracts, migrations, and compatibility

- Contracts changed: none in `loki-mutation-contract` (not edited, per forbidden
  scope). New `loki-storage` migrations `0004`–`0007` (additive only; `0001`–
  `0003` untouched byte-for-byte).
- Consumers affected: `loki-approvals`, `loki-agents`, `loki-agent-delivery`,
  `loki-local-authority` (constructor/internal-storage change only, business
  API preserved); two out-of-scope crates that call these four crates' test
  constructors in their own tests (`loki-local-devices`, `loki-local-setup`)
  are **not** affected because `open`/`open_in_memory` were kept, unchanged —
  see §7.
- Migration: new tables only (`local_approvals`, `agent_connections`,
  `agent_runs`, `agent_run_events`, `work_intent_deliveries`,
  `local_installation_authority`, `local_authority_sessions`,
  `mutation_acceptances`), applied via the existing idempotent migration
  runner. No existing table/column/index changed.
- Backward/rolling compatibility: a database that already has the four
  consolidated crates' schema as independent files is unaffected (those
  files are untouched by this node — old per-crate files are simply
  superseded going forward, not migrated in-place; that migration, if ever
  needed for an already-shipped install, is out of this node's scope since
  Part One has not shipped yet).
- Generated-contract drift: n/a (no `packages/contracts` change).
- Build recipe/executor/provenance compatibility: n/a.
- Cloud API/offline/notification compatibility: n/a (no workspace/cloud
  binding, per ADR-0006 Amendment 1, unchanged).

## 6. Security, privacy, and device safety

- Trust boundaries touched: local persistence (`crates/loki-storage`) and the
  four consolidated local-authority-adjacent crates. No IPC, no MCP, no cloud,
  no device/connector/build-executor surface touched anywhere in this node.
- Authorization/approval impact: approval consumption is now atomic with
  job/outbox/mutation-acceptance writes inside one transaction (this is the
  core ADR-0006 guarantee this node exists to deliver). No policy/approval
  *rule* changed — `loki_approvals::evaluate_operation`'s closed policy table
  is untouched; only how the resulting DB rows are read/written.
- Secrets/data handling: no new secret class. `AgentSecret` redaction,
  DPAPI-protected keys, and SQLCipher-at-rest protection are all unchanged
  (`configure_sqlcipher`/`secure_memory`/`managed.rs` not edited).
- Containment profile and denied authorities: n/a.
- Device side effects performed: none — no connector/device/build/deploy
  effect exists anywhere in `loki-acceptance-store` or inside any
  `Storage::transaction()` closure (grep-verified, §4).
- New dependencies: `serde_json` added to `loki-acceptance-store`
  (already a pinned workspace dependency elsewhere — used to safely encode
  the outbox notification payload rather than hand-building JSON with
  injection risk; justification recorded in that crate's `Cargo.toml`).
  `rusqlite`/`uuid` added to `loki-acceptance-store` per the packet's own
  Section 5 requirement (transaction closure operates on
  `&rusqlite::Transaction` directly).
- Remaining risk: the deadlock-avoidance refactor (Deliverable 3) and the
  no-bypass property (Deliverable/criterion re: `AcceptedRecord`) are exactly
  what Section 11's independent security review must specifically re-check —
  this worker's own review is not a substitute for that independent gate.

## 7. Decisions and assumptions

- **Deviation — `crates/loki-storage/src/migrations.rs` touched, not named in
  Section 5.** Deliverable 1 requires the four new migration files be
  "registered in `migrations::MIGRATIONS` in that order after `0003`" — that
  registry lives in `src/migrations.rs`, so fulfilling the named deliverable
  mechanically requires editing this file (append-only: four new `Migration`
  entries added, nothing else touched). Its own colocated unit tests
  hardcoded `vec![1, 2, 3]`/table counts that then had to become
  `vec![1, 2, 3, 4, 5, 6, 7]`/13, or `cargo test -p loki-storage` (an
  explicit acceptance criterion) would fail. Treated as an implied-necessary
  vehicle for an explicitly named deliverable, not scope creep — flagged
  here for orchestrator visibility rather than silently done.
- **Deviation — two pre-existing integration tests touched
  (`crates/loki-storage/tests/{corruption,migrations}.rs`), not named in
  Section 5.** Same root cause: `Storage::run_migrations` return value is a
  public, tested contract, and two integration tests elsewhere hardcoded the
  old three-migration count. Fixed mechanically (count update only, no
  assertion intent changed) because `cargo test --workspace` is itself an
  acceptance criterion.
- **Deviation — `loki-acceptance-store`'s actual dependency list omits
  `loki-approvals`/`loki-agents`/`loki-agent-delivery`/`loki-local-authority`**,
  which Section 5 names. Recorded in detail, with rationale, in that crate's
  own `Cargo.toml` comment and reproduced here: this crate's real
  implementation (Deliverable 6's own wording: "via the consolidated
  approvals.rs repository," "through the connection-parameterized core
  functions") never needs those four crates' business APIs — only
  `loki-storage`'s consolidated repository modules directly. Declaring them
  as unused dependencies would violate `AGENTS.md` rule 11. If the
  orchestrator/reviewer determines those four crates' APIs are needed for a
  reason this worker did not anticipate, that is a legitimate reason to
  revise this decision, not evidence of an oversight — the reasoning is
  laid out for exactly that check.
- **`open`/`open_in_memory` kept unchanged (not replaced) in all four
  consolidated crates**, as thin wrappers that build a private, fully
  migrated `Storage` and delegate to the new `new(Arc<Storage>)`
  constructor. The packet's literal wording ("replace ... with a
  constructor taking `&loki_storage::Storage`") could be read as requiring
  their removal; direct inspection found two crates outside this node's
  writable/forbidden-scope lists (`loki-local-devices`, `loki-local-setup`)
  call `LocalAuthority::open_in_memory()`/`AgentRegistry::open_in_memory()`
  in their own tests. Removing those constructors would break
  `cargo build --workspace`/`cargo test --workspace` for crates this node
  has no authorization to touch. Keeping them (while still removing every
  independent `Connection::open_in_memory()`/`open_or_migrate_sqlcipher`/
  ad hoc `CREATE TABLE` from all four crates' internals, which was the
  substantive goal) satisfies the acceptance criterion "no existing public
  business method signature changed ... except each store's constructor"
  more conservatively than required, and avoids an unauthorized cross-crate
  break. Flagged as a decision, not hidden.
- **Two disk-plaintext-marker test assertions strengthened, not just
  preserved.** `loki-local-authority`'s and `loki-agents`' restart tests
  originally checked for byte-string markers (`"local_authority_state"`,
  `"local_agents"`) that never matched any real table name in either
  crate's actual prior schema (`local_authority_sessions`/
  `local_installation_authority`; `agent_connections`) — the assertions were
  trivially true regardless of real encryption. Corrected to check the real
  table names, which is a strictly stronger, more honest test of the exact
  same claimed behavior ("schema marker must not be plaintext on disk"), not
  a changed claim.
- **`GrantGeneration` read (Section 9 item 2) — SUPERSEDED, resolved by
  `P1-PRINCIPAL-GRANT-001` in this same commit.** This node originally
  implemented a narrow, explicitly-provisional shim
  (`find_grant_generation_core`, mapping `generation == issued_at_ms`). That
  shim has been fully replaced, not extended, by `P1-PRINCIPAL-GRANT-001`
  per ADR-0007 Option B: `find_local_generation_core` (real atomic
  per-principal counter for `LocalSession`) and `find_agent_generation_core`
  (real per-connection mutable counter for `AgentCredential`), with
  `loki-acceptance-store`'s single call site now branching on
  `request.method.requires_approval()` to pick the correct identity model's
  read. See `Docs/graph/handoffs/P1-PRINCIPAL-GRANT-001-1.md` for that
  node's own deliverables and decisions.
- **Agent-method `job_id` minting scheme**: `mjob_<uuid-v4-simple>` (see
  `synthesize_agent_job_id` in `crates/loki-acceptance-store/src/lib.rs`),
  consistent with the existing per-crate prefix convention this
  consolidation absorbed (`apr_`, `ac_`/`ar_`/`las_`, `li_`/`ls_`). Real
  jobs use `job_<uuid-v4-simple>`, so the two ID spaces are visually
  distinguishable by prefix without needing a lookup.
- **Clock read timing**: `accepted_at_ms`/`now_ms` are read exactly once, at
  the top of the transaction closure, and reused for both the grant-expiry
  check and `construct_accepted`'s `accepted_at_ms` argument — reasoning
  documented in `accept_mutation_in_transaction`'s doc comment: both checks
  execute inside the same continuously-held `Storage` mutex guard with no
  intervening I/O, so one read is the honest "now" for the whole atomic
  sequence.
- **Digest reconciliation between `loki_approvals::ApprovalRequest::digest()`
  and `loki_mutation_contract::compute_request_digest`** remains unresolved
  (as it must — `P1-MUTATION-CONTRACT-001` §9 item 5's own scope, not this
  node's). This node's tests construct `local_approvals.request_digest`
  values directly (bypassing `loki-approvals`'s own digest method) so the
  positive test paths exercise `evaluate_approval_reference`'s real
  digest-match logic against a value known to match — this is a real,
  deliberate test-construction choice recorded here, not evidence the two
  algorithms were secretly unified.

## 8. Review focus

- **Deadlock-avoidance refactor (Deliverable 3)**: confirm no
  `self.lock()`-based repository trait method is ever called from inside a
  `Storage::transaction()` closure anywhere in this diff (`grep -rn
  "self.lock()" crates/loki-storage/src/{jobs,outbox,approvals,agents,
  agent_delivery,local_authority,mutation_acceptance}.rs` should show it only
  inside each `impl X for Storage` block, never inside a `..._core`
  function) — and that `crates/loki-acceptance-store/src/lib.rs` and
  `crates/loki-agent-delivery/src/lib.rs`'s new `Storage::transaction` call
  sites only ever call `..._core` functions with the given
  `&rusqlite::Transaction<'_>`, never a `self.storage.*` trait method.
- **No bypass of `AcceptedRecord`/`AcceptanceResult::Accepted`/
  `ReplayedIdenticalAccepted` construction**: confirm
  `crates/loki-acceptance-store/src/lib.rs` never struct-literal-constructs
  either, and that the only two call sites producing them are
  `construct_accepted`/`replayed_result`.
- **`consume_approval_core`'s single-use guarantee under real concurrency**:
  re-run `concurrent_submission_of_the_same_idempotency_key_yields_exactly_one_accepted`
  and `revocation_racing_acceptance_yields_exactly_one_legal_outcome` under
  `--test-threads=1` and independently (e.g. with `cargo nextest` or a
  stress-repeat loop) to build additional confidence beyond this worker's
  own 5x manual repetition.
- **The three named deviations in §7** (dependency list, `migrations.rs`/two
  test files touched, `open`/`open_in_memory` kept) — confirm they are each
  within the spirit of Section 5 and do not require a packet revision before
  this node is treated as accepted.

## 9. Follow-up graph changes proposed

Every item below is carried forward from `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md`
§9, verbatim in substance, per that section's explicit "do not resolve
silently" instruction:

1. **Clock source deviates from `claudeplanner.md`'s recommendation, by
   necessity.** `crates/loki-acceptance-store/src/clock.rs` is a small,
   locally-owned Unix-epoch-millis clock read, not a `loki-runtime` import —
   because `loki-runtime` will depend on this crate once
   `P1-RUNTIME-COMP-001` lands, and importing `loki-runtime`'s clock here
   would create a real dependency cycle at that point. This is not a defect
   to fix; it is the correct shape and should not be "cleaned up" into a
   shared clock crate without re-solving the cycle problem it avoids.
2. **`GrantGeneration` source of truth — RESOLVED by `P1-PRINCIPAL-GRANT-001`
   in this same commit**, per ADR-0007 Option B (accepted 2026-08-28): two
   independent, non-merged generation models (real atomic counters for both
   `LocalSession` and `AgentCredential`), not the original provisional
   timestamp-mapping shim. No cross-model "kill switch" was established
   (an explicit, documented limitation of Option B, not an oversight) —
   revoking a `LocalSession` never revokes an `AgentCredential` or vice
   versa. See `Docs/graph/handoffs/P1-PRINCIPAL-GRANT-001-1.md`.
3. **`outcome_unknown` trigger contract remains undefined.** Confirmed (not
   merely asserted) during implementation: nothing in this node's seven-step
   sequence runs a physical/irreversible effect — every step is a database
   read/write inside one transaction, and the transaction closure never
   calls into any connector/device/build/deploy code path (grep-verified,
   §4). This node's scope genuinely never reaches the "uncertain vs. clearly
   failed" decision point `P1-MUTATION-CONTRACT-001` §9 item 2 names; that
   remains open for whichever node first runs a physical effect after this
   node's commit point.
4. **Lock-contention/throughput risk** (`claudeplanner.md` §3.4): recorded,
   not fixed. This node's own concurrency tests exercise 2 concurrent
   threads on one `Storage`; they do not measure throughput or contention
   under realistic multi-caller load. `P1-RUNTIME-COMP-001`/
   `P1-MUTATION-REV-001` should verify this at realistic load once the
   runtime actually composes these crates end to end.
5. **Agent-method `job_id` minting scheme is `mjob_<uuid-v4-simple>`**
   (§7 above) — `P1-AGENT-MUTATION-IDEMP-001` should treat this as the
   already-decided, stable scheme rather than re-deriving it.

No new graph node/edge is proposed by this worker beyond what the packet
already anticipates (`P1-PRINCIPAL-GRANT-001`, `P1-AGENT-MUTATION-IDEMP-001`,
`P1-RUNTIME-COMP-001`, `P1-MUTATION-REV-001`, and the independent security/
test review this handoff requests under Section 11's human gate).
