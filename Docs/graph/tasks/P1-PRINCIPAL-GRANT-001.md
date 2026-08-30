# Task Packet — `P1-PRINCIPAL-GRANT-001` Durable Grant-Generation Counters (ADR-0007 Option B)

Packet status: `proposed` (ready for orchestrator review/dispatch)
Graph status: `ready` — ADR-0007 accepted (Option B, human decision 2026-08-28)
Created by: `orchestrator (Claude Code), shaped by loki-planner`
Created at: `2026-08-28T00:00:00Z`
Starting revision: **stacked on branch `node/P1-ACCEPTANCE-STORE-001`'s current
uncommitted tip** in worktree `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001` — **not**
`LokiAI-v1.2`'s tip. See Section 3's dependency note for why.

## 1. Objective

Implement ADR-0007 Option B: give `crates/loki-local-authority` (`LOC-001`) and
`crates/loki-agents` (`AGT-001`) each an **additive, durable `generation`
counter**, with **no change to any existing accepted method's behavior**, and
replace `P1-ACCEPTANCE-STORE-001`'s explicitly-provisional
`loki_storage::local_authority::find_grant_generation_core` shim (which maps
`generation == issued_at_ms`, with a documented Low-severity ambiguity on
duplicate `issued_at_ms` values for one principal) with a real,
per-identity-model generation read that has no such collision by construction.

Per ADR-0007's explicit scope clarification, this node also adds two **new**
`AgentCredential` operations that do not exist today — `rotate` and
`change_scopes` — as additive capability, not a redesign of `AGT-001`'s
existing accepted `register`/`revoke` behavior.

**This node does NOT implement "one kill switch covers both humans and
agents."** ADR-0007 explicitly declined that property (that would have been
Option A). Revoking a Windows-principal's `LocalSession` generation never
cascades to revoke any `AgentCredential`, and vice versa. State this
limitation plainly in every doc comment that touches grant generations so no
later node assumes a stronger guarantee than what was actually decided.

### 1.1 The core design: two independent, non-merged generation models

ADR-0007 Option B requires "no shared storage/issuance-layer merge between the
two models." Direct inspection of the already-accepted, unmodifiable
`crates/loki-mutation-contract::method::MutationMethod::requires_approval()`
(equivalently `policy_tier()`) shows it is already the exact, exhaustive,
1:1 discriminant this node needs: the four `ApprovalRequired` methods
(`Restart`, `Flash`, `ApplyConfig`, `ContainedBuild`) are always Windows-SID
(`LocalSession`) principals; the three `NotRequired` methods
(`AgentRunRegister`, `AgentRunUpdate`, `WorkIntentAcknowledge`) are always
agent-connection (`AgentCredential`) principals. This is not a new
convention this node invents — it is already latent in ADR-0006's own
"Consequences" text and confirmed by direct code reading. **No edit to
`crates/loki-mutation-contract` is needed or permitted** to make this
determination; `loki-acceptance-store` branches on
`request.method.requires_approval()` alone.

**Known, inherited naming overload (not introduced by this node, must not be
silently "fixed"):** `MutationAcceptanceRequest.principal_sid: String` is an
already-accepted field name from `loki-mutation-contract` that, for agent
methods, actually carries an `AgentCredential::connection_id`, not a Windows
SID. This packet documents that overload precisely at every call site; it
does not rename the field (out of scope — would require editing the pure
contract crate).

Two independent designs follow directly from each crate's own already-accepted
row lifecycle shape (confirmed by direct inspection, not assumed identical):

- **`LOC-001` (`local_authority_sessions`): generation is per-SESSION, assigned
  once, immutable.** Each session is its own row, many concurrent sessions
  per principal, never mutated in place except `revoked_at_ms`. So
  `generation` is a monotonically increasing integer scoped to
  `principal_sid`, assigned atomically at `issue_session` time (the
  `MAX(generation)+1` computation and the `INSERT` execute inside the same
  `Storage::lock()`-held `MutexGuard`, which already serializes all writers —
  this is what actually eliminates the old shim's collision risk, not merely
  swapping the column name). Revocation never mints a new generation: it
  updates `revoked_at_ms` on the same row/same generation number that was
  already issued (LOC-001 has no rotate operation today, and this node does
  not add one — only `AGT-001` gets new operations, per ADR-0007's scope
  clause).
- **`AGT-001` (`agent_connections`): generation is per-CONNECTION, mutable,
  bumped in place.** A connection is a single row with exactly one live
  `secret_hash` at a time. `generation` starts at `1` on `register`
  (constant, no computation needed — no race possible), and increments by
  exactly `1` on `rotate`, `change_scopes`, and (defensively, matching
  ADR-0007's literal "issue/rotate/revoke" wording, though not itself a new
  security property since `revoked_at_ms` already fail-closes independently)
  `revoke` — each via one atomic guarded `UPDATE ... WHERE connection_id = ?
  AND revoked_at_ms IS NULL` under the same `Storage::lock()` pattern
  `revoke_agent_connection_core` already uses.

**`AGT-001` has no expiry concept today and this node does not add one** (per
ADR-0007's context: "no expiry"). `loki_mutation_contract::GrantGeneration::new`
requires `expires_at_ms > issued_at_ms` unconditionally. The single, explicit,
documented resolution: `loki-acceptance-store` (never `loki-storage`) supplies
a local `const AGENT_GRANT_NEVER_EXPIRES_MS: i64 = i64::MAX;` sentinel only at
the point it constructs a `GrantGeneration` for an agent-connection read — this
sentinel must never appear in `loki-storage`'s row types, SQL, or schema. This
is a boundary rule, not a design option; do not let it drift into
`loki-storage`.

## 2. Traceability

- Requirements: `NFR-SEC-002`, `NFR-SAFE-001`, `NFR-REL-001`
- Acceptance scenarios: `AS-006`–`AS-010`, `AS-015-L`
- Architecture: `Docs/ARCHITECTURE.md` §8 Persistence; ADR-0001 "core storage"
  boundary (unchanged — this node stays inside the same core-storage file)
- Security: credential/session revalidation, fail-closed revocation,
  additive-only capability expansion (`AGENTS.md` human-approval boundary:
  "changing authentication ... RBAC ... policy" — already satisfied by
  ADR-0007's human acceptance; this packet implements exactly what was
  decided, nothing more)
- Parent/decisions: `Docs/graph/decisions/ADR-0007-grant-generation-identity-unification.md`
  (accepted, Option B); `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
  (accepted); `P1-MUTATION-CONTRACT-001` §9 item 3 (the open question this
  node closes)

## 3. Dependencies and inputs

- `depends_on` (graph-recorded): `P1-ACCEPTANCE-STORE-001`, `LOC-001`,
  `AGT-001`, `ADR-0007` (accepted).
- **Start-vs-integration distinction (explicit, not silent):**
  `P1-ACCEPTANCE-STORE-001` is implemented and independent-security-review
  clean (no Critical/High), with independent test-review in progress
  (verification only, not expected to reshape code). Every file this node
  touches exists only in worktree `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001`
  (branch `node/P1-ACCEPTANCE-STORE-001`), not on `LokiAI-v1.2`'s tip. This
  node **starts** stacked on that exact branch/worktree now. This node
  **cannot be marked `accepted` or integrated to `main`** before
  `P1-ACCEPTANCE-STORE-001` itself reaches `accepted` and integrates first
  (`AGENTS.md` graph protocol point 8: integration is one node at a time in
  dependency order). If `P1-ACCEPTANCE-STORE-001`'s test-review later forces
  `NEEDS_REPLAN`, this node returns to rebase against the corrected shape
  before its own acceptance is final — this is a stacking/sequencing
  decision, not a design waiver, and is recorded here for audit exactly like
  Section 3's waiver pattern in `P1-ACCEPTANCE-STORE-001.md`.
- Consumed contracts/exact revisions (read-only, unmodified by this node):
  - `crates/loki-mutation-contract::{grant::{GrantGeneration, evaluate_grant},
    method::MutationMethod::requires_approval}` — consumed exactly as
    designed.
  - `loki_storage::{storage::Storage, ids, error::StorageError}` at the
    `P1-ACCEPTANCE-STORE-001` worktree revision.
- Known starting conditions (confirmed by direct inspection of the worktree,
  not assumed):
  - `loki_storage::local_authority::find_grant_generation_core` (and its
    trait method `LocalAuthorityRepository::find_grant_generation`) is the
    exact, sole, three-call-site provisional shim this node replaces —
    call sites: `crates/loki-acceptance-store/src/lib.rs`,
    `crates/loki-storage/src/local_authority.rs` (its own definition), and
    `crates/loki-acceptance-store/tests/acceptance_flow.rs` (confirmed via
    repo-wide grep — no other caller exists).
  - `crates/loki-acceptance-store/tests/acceptance_flow.rs`'s
    `agent_request()`-based tests (`same_key_same_digest_replays`,
    `same_key_different_digest_rejected`, `expired_grant_denies_acceptance`,
    `revoked_grant_denies_acceptance`, `agent_method_accepts_without_jobs_row`)
    currently seed their grant through `loki_storage::local_authority`
    (`seed_grant`/`seed_valid_grant`) even though the request method is
    `AgentRunRegister` — i.e. today's harness accidentally exercises only the
    Windows-SID read path for agent-shaped requests, never a real
    agent-connection path (because none existed). This is exactly the gap
    ADR-0007 found; Section 6 Deliverable 6 requires correcting this, not
    preserving it.
  - `crates/loki-local-authority/src/model.rs`'s `LocalSession` and
    `crates/loki-agents/src/lib.rs`'s `AgentCredential` have no `generation`
    field today — both are additive-only struct-field targets, not existing
    fields being changed.
- Required environment/credential/device: none beyond normal build/test
  toolchain (same as `P1-ACCEPTANCE-STORE-001`).

## 4. Required reading

- `AGENTS.md`, `Docs/ARCHITECTURE.md` §8
- `Docs/graph/decisions/ADR-0007-grant-generation-identity-unification.md`
  (in full — this packet operationalizes it exactly, Option B only)
- `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
- `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md` §9 items 2 and 5
- `Docs/graph/handoffs/P1-ACCEPTANCE-STORE-001-1.md` §7–§9 (the exact
  provisional-shim shape and the two Low/informational notes recorded for
  this node's awareness)
- In the `node/P1-ACCEPTANCE-STORE-001` worktree: `crates/loki-mutation-contract/src/{grant.rs,method.rs,request.rs}`
  (consume, do not edit); `crates/loki-storage/src/{local_authority.rs,agents.rs,storage.rs}`;
  `crates/loki-local-authority/src/{model.rs,store.rs}`;
  `crates/loki-agents/src/lib.rs`; `crates/loki-acceptance-store/src/lib.rs`
  and `tests/acceptance_flow.rs`

## 5. Ownership

Writable paths (all inside worktree `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001`,
new commits on top of branch `node/P1-ACCEPTANCE-STORE-001`, or a child branch
`node/P1-PRINCIPAL-GRANT-001` stacked on it — orchestrator's choice, recorded
in the handoff either way):

- `crates/loki-storage/migrations/0008_local_authority_grant_generation.sql`
  (new, additive-only): `ALTER TABLE local_authority_sessions ADD COLUMN
  generation INTEGER NOT NULL DEFAULT 0;` plus a **non-unique** index
  `idx_local_sessions_principal_generation(principal_sid, generation)` for
  lookup performance only — uniqueness per `(principal_sid, generation)` is
  an **application invariant** enforced by the single-writer `Storage`
  mutex (Section 1.1), not a DB constraint; do not add a `UNIQUE` index (a
  pre-existing dev/test database with rows already at the `DEFAULT 0`
  backfill value could violate one, and Part One has no shipped
  installations to migrate cleanly in the first place).
- `crates/loki-storage/migrations/0009_agent_grant_generation.sql` (new,
  additive-only): `ALTER TABLE agent_connections ADD COLUMN generation
  INTEGER NOT NULL DEFAULT 1;` and `ALTER TABLE agent_connections ADD COLUMN
  generation_issued_at_ms INTEGER NOT NULL DEFAULT 0;` (the second column
  tracks "when did this connection's *current* generation become current" —
  distinct from `created_at_ms`, which stays "when was this connection
  first registered," so a rotated/scope-changed connection's generation
  timestamp is accurate, not silently stuck at original registration time).
- `crates/loki-storage/src/local_authority.rs`: remove
  `GrantGenerationRow`/`find_grant_generation_core`/
  `LocalAuthorityRepository::find_grant_generation`; add
  `LocalGrantGenerationRow { principal_sid, generation, issued_at_ms,
  expires_at_ms, revoked_at_ms }` and
  `LocalAuthorityRepository::find_local_generation`/`find_local_generation_core`;
  split the existing insert input into `NewAuthoritySessionRow` (no
  `generation` field — caller-supplied fields only, mirrors `jobs.rs`'s
  existing `NewJob`/`JobRecord` split convention already used in this
  crate) vs. `AuthoritySessionRow` (read-side, gains `pub generation: u64`);
  change `LocalAuthorityRepository::insert_authority_session`/
  `insert_authority_session_core`'s signature from `Result<(), StorageError>`
  to `Result<u64, StorageError>` (returns the atomically assigned
  generation) — see Section 9 item 1 for why changing this specific,
  not-yet-accepted trait's signature is in scope here, unlike `LOC-001`'s
  own public API.
- `crates/loki-storage/src/agents.rs`: add `AgentGrantGenerationRow
  { connection_id, generation, issued_at_ms, revoked_at_ms }` (no
  `expires_at_ms` field — see Section 1.1) and
  `AgentsRepository::find_agent_generation`/`find_agent_generation_core`;
  extend `AgentConnectionRow` with `pub generation: u64`,
  `pub generation_issued_at_ms: i64`; extend `insert_agent_connection_core`'s
  SQL to set `generation=1, generation_issued_at_ms=created_at_ms` (no Rust
  signature change — both values are already-known constants derived from
  existing parameters); extend `revoke_agent_connection_core`'s existing
  `UPDATE` to also `SET generation = generation + 1` (no signature change);
  add `AgentsRepository::rotate_agent_connection`/`rotate_agent_connection_core`
  (`(connection_id, new_secret_hash, now_ms) -> Result<Option<u64>,
  StorageError>`, atomic guarded `UPDATE ... WHERE connection_id=? AND
  revoked_at_ms IS NULL`, returns the new generation or `None` if not found
  or already revoked) and
  `AgentsRepository::change_agent_scopes`/`change_agent_scopes_core`
  (`(connection_id, new_scopes_wire, now_ms) -> Result<Option<u64>,
  StorageError>`, same atomicity/guard shape).
- `crates/loki-local-authority/src/model.rs`: add `pub generation: u64` to
  `LocalSession` (additive field only; `LocalSession::issue` sets a
  placeholder `0`, documented as "always overwritten by the storage layer's
  atomic assignment before this value reaches any caller — never trust this
  placeholder directly").
- `crates/loki-local-authority/src/store.rs`: `issue_session` builds the
  `NewAuthoritySessionRow`, calls
  `self.storage.insert_authority_session(&row)?` for the real assigned
  generation, then returns the final `LocalSession` with that generation
  substituted in (via `..` struct-update syntax or equivalent); update
  `row_to_session`/`require_live_session` to carry `AuthoritySessionRow`'s
  new `generation` field into `LocalSession.generation`. No change to any
  existing method's public signature (`issue_session`/`revoke_session`/
  `authorize_local`/`authorize_cloud`/`initialize_current` all keep their
  exact current signatures — only `LocalSession`'s own struct shape gains a
  field).
- `crates/loki-agents/src/lib.rs`: add `pub generation: u64` to
  `AgentCredential`; `register` sets it to the known constant `1` (no
  storage round-trip needed for this value — see above); add two **new**
  public methods:
  - `AgentRegistry::rotate(&self, authority: &LocalAuthority, context:
    &LocalContext, session_id: &str, connection_id: &str, now_ms: i64) ->
    Result<AgentCredential, AgentError>` — same
    `authority.authorize_local(context, session_id,
    LocalCapability::RegisterAgent, now_ms)?` gate `register`/`revoke`
    already use (no new `LocalCapability` variant — additive reuse of the
    existing one); mints a new `las_`-prefixed secret exactly like
    `register`; maps a `None` storage result to `AgentError::NotFound` if
    the connection never existed or `AgentError::Revoked` if it exists but
    is already revoked (mirror `revoke`'s existing exists-vs-revoked
    disambiguation at lines 293–300 of the current file).
  - `AgentRegistry::change_scopes(&self, authority: &LocalAuthority,
    context: &LocalContext, session_id: &str, connection_id: &str,
    new_scopes: &[AgentScope], now_ms: i64) -> Result<u64, AgentError>` —
    same authorization gate; same non-empty/sort/dedup validation
    `register` already applies to scopes; returns the bare new generation
    number (no secret is involved, so no `AgentCredential` return —
    returning a fabricated/reused secret here would be dishonest per
    `AGENTS.md` rule 1); same `NotFound`/`Revoked` error mapping as
    `rotate`.
  - No change to `register`/`revoke`/`authenticate`/`start_run`/
    `finish_run`/`append_event`'s existing public signatures.
- `crates/loki-acceptance-store/src/lib.rs`: replace step 1's single
  `local_authority::find_grant_generation_core` call with the
  `request.method.requires_approval()` branch described in Section 1.1,
  calling `local_authority::find_local_generation_core` (Windows-SID path,
  real `expires_at_ms`) or `agents::find_agent_generation_core` (agent path,
  `AGENT_GRANT_NEVER_EXPIRES_MS` sentinel applied only at this call site);
  update the top-of-file doc comment (currently says the shim is "pending
  `P1-PRINCIPAL-GRANT-001`") to cite this packet as the resolution, and
  document the "no shared kill switch" limitation from Section 1 explicitly
  in that same comment.
- `crates/loki-acceptance-store/tests/acceptance_flow.rs`: add
  `seed_agent_connection_grant(storage, connection_id, ...) -> u64`
  (inserts directly via `loki_storage::agents::AgentsRepository`, mirroring
  `seed_grant`'s existing direct-repository-seeding style); re-point every
  `agent_request(...)`-based test at this new helper instead of
  `seed_grant`/`seed_valid_grant`; split `expired_grant_denies_acceptance`
  into (a) a Flash/approval-required-method version proving real session
  expiry still denies acceptance (keep the existing LocalAuthority seeding
  for this one) and (b) a new
  `agent_grant_is_revocable_but_never_expires` test proving an
  agent-connection grant, seeded far in the "past" by every other clock
  measure, is still accepted (no expiry exists for this identity model) and
  is denied only once actually revoked; split
  `revoked_grant_denies_acceptance` similarly into a Windows-SID version
  (unchanged, LocalAuthority-seeded) and a new agent-connection version
  (seeded via the new helper, revoked via
  `AgentsRepository::revoke_agent_connection`) — both identity models must
  each independently prove their own revocation path is read and enforced,
  since that is the exact property this node's per-identity-model read
  boundary exists to guarantee.
- `Docs/graph/handoffs/P1-PRINCIPAL-GRANT-001-1.md` (own handoff only).

Read-only dependency paths:

- `crates/loki-mutation-contract/src/**` (consume; do not edit)
- `Docs/graph/decisions/**`

Forbidden scope:

- No edit to `crates/loki-mutation-contract/**` — no new `MutationMethod`
  variant for `rotate`/`change_scopes`. Those are direct, locally-authorized
  `AgentRegistry` business methods (same call pattern as today's
  `register`/`revoke`), never routed through `loki-acceptance-store`'s
  ADR-0006 mutation-acceptance transaction. Adding a wire-level mutation
  method for them is out of this node's scope entirely.
- No new `LocalCapability` variant in `crates/loki-local-authority` — reuse
  `LocalCapability::RegisterAgent` for `rotate`/`change_scopes`, exactly as
  `register`/`revoke` already do.
- No edit to `crates/loki-storage/migrations/0001`–`0007` (append-only:
  `0008`, `0009` only).
- No "one kill switch" cross-model cascade anywhere (Section 1 — this is the
  literal scope boundary ADR-0007 drew; implementing it would silently
  reopen the rejected Option A).
- No change to `LOC-001`'s or `AGT-001`'s **existing** public business method
  signatures (`issue_session`, `revoke_session`, `authorize_local`,
  `authorize_cloud`, `initialize_current`, `register`, `revoke`,
  `authorize_scope`, `require_run_for_connection`, `start_run`,
  `finish_run`, `append_event`) — only their return **struct shapes**
  (`LocalSession`, `AgentCredential`) gain additive fields, and the
  internal `loki-storage` repository trait introduced by
  `P1-ACCEPTANCE-STORE-001` (not yet accepted/integrated) may have its
  signature adjusted where Section 5 says so.
- No `AGENT_GRANT_NEVER_EXPIRES_MS`-shaped sentinel or any other
  `GrantGeneration`-specific concept leaking into `loki-storage`'s row types
  or SQL (Section 1.1) — that mapping belongs solely to
  `loki-acceptance-store`.
- No RPC/wire schema work.
- Do not resolve the `outcome_unknown` trigger contract (still
  `P1-MUTATION-CONTRACT-001` §9 item 2's open scope, untouched by this
  node — this node's changes are all still pure database reads/writes, no
  physical effect).

## 6. Deliverables

1. **Additive migrations `0008`/`0009`** exactly as specified in Section 5,
   registered in `migrations::MIGRATIONS` in order after `0007`.
2. **`LOC-001` per-principal session-generation counter**: atomic
   `MAX(generation)+1`-then-`INSERT` under one `Storage::lock()` critical
   section (Section 1.1); `LocalSession.generation` correctly populated on
   every `issue_session` call and every subsequent `require_live_session`/
   `authorize_local` read-back; `revoke_session` behavior and signature
   completely unchanged.
3. **`AGT-001` per-connection mutable generation counter**: `register`
   starts at `1`; `rotate` and `change_scopes` (new methods) each atomically
   bump it by exactly `1` under a guarded `UPDATE ... WHERE revoked_at_ms IS
   NULL`; `revoke` also bumps it (defensive, not itself a new fail-closed
   property — `revoked_at_ms` already independently fail-closes).
4. **`find_local_generation_core`/`find_agent_generation_core`** replacing
   `find_grant_generation_core` exactly as specified in Section 5, each
   incapable of the old shim's duplicate-match ambiguity by construction
   (unique-by-invariant per `(principal_sid, generation)` or
   `(connection_id, generation)`).
5. **`loki-acceptance-store`'s per-identity-model branch** (Section 1.1),
   replacing the single provisional call with the
   `request.method.requires_approval()`-gated read, each branch converting
   its own row type into `loki_mutation_contract::GrantGeneration` at its
   own call site (no shared conversion helper across the two branches — see
   ADR-0007's "no shared merge" requirement).
6. **Test-harness correction** in `crates/loki-acceptance-store/tests/acceptance_flow.rs`
   exactly as specified in Section 5's ownership entry for that file — this
   is required, not optional, because the existing harness currently never
   exercises a real agent-connection read at all (Section 3's "known
   starting conditions").
7. **New unit tests** in `crates/loki-local-authority` and
   `crates/loki-agents` proving: LOC-001 — `concurrent_session_issuance_yields_distinct_generations`
   (two threads issuing sessions for the same principal simultaneously never
   produce a duplicate `generation`, mirroring the existing
   `concurrent_revocation_has_exactly_one_winner` test's real-thread style);
   AGT-001 — `rotate_replaces_secret_and_bumps_generation` (old secret
   authentication-fails after rotate; new secret authenticates; generation
   increased by exactly 1), `rotate_denied_for_revoked_connection`,
   `scope_change_updates_authorization_and_bumps_generation` (old scope set
   denied post-change, new set allowed), `scope_change_denied_for_revoked_connection`,
   `restart_preserves_generation_across_reopen` (mirrors the existing
   `restart_preserves_revocation` test's real on-disk-DB style).
8. Doc comments citing ADR-0007 (by option letter) and this packet at every
   new/changed function, matching sibling-crate citation convention, and
   explicitly restating the "no cross-model kill switch" limitation
   wherever a reader might otherwise assume one.
9. Handoff recording: the exact old-shim replacement (interface signature,
   before/after); the two new `AgentRegistry` methods' exact signatures for
   `P1-AGENT-MUTATION-IDEMP-001`'s benefit; confirmation that no
   `MutationMethod`/`LocalCapability` enum changed; every still-open
   question this node does not resolve (Section 9 below).

## 7. Acceptance criteria

- [ ] `cargo build --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` succeed with zero warnings.
- [ ] `cargo test -p loki-storage -p loki-local-authority -p loki-agents -p loki-acceptance-store -- --test-threads=1` passes, including every test named in Section 6 Deliverable 7 and every existing test in these four crates (updated only where Section 5 requires it — same assertions elsewhere).
- [ ] `pnpm run verify` passes (one retry allowed on the documented flaky Windows Application Control error).
- [ ] No existing public business method signature changed in `crates/loki-local-authority` or `crates/loki-agents` except `LocalSession`/`AgentCredential`'s additive struct fields and the two brand-new `AgentRegistry` methods.
- [ ] Grep-verifiable: `find_grant_generation_core`/`find_grant_generation`/`GrantGenerationRow` do not exist anywhere in the tree after this node (fully replaced, not left dead alongside the new functions).
- [ ] Grep-verifiable: no new `MutationMethod` or `LocalCapability` enum variant was added anywhere.
- [ ] Grep-verifiable: no sentinel/expiry-related constant referencing "never expires"/`i64::MAX` for agent grants exists inside `crates/loki-storage/**` — only inside `crates/loki-acceptance-store/**`.
- [ ] Two concurrent `issue_session` calls for the same principal (real OS threads) never produce the same `generation` value, reproduced ≥2 runs.
- [ ] `rotate`/`change_scopes` against an already-revoked connection are denied (`AgentError::Revoked`), never silently succeed.
- [ ] An agent-connection grant that would have been "expired" under the old shim's timestamp semantics is still accepted (no expiry exists for this identity model) and is denied only once actually revoked — proven by the new split tests in Section 5.
- [ ] A Windows-SID (approval-required-method) grant's real session expiry still denies acceptance exactly as before this node's change (regression-proof for the unmodified path).
- [ ] `git diff --stat` shows changes only under the writable paths named in Section 5, plus the handoff.
- [ ] Independent test-reviewer and security-reviewer sign-off with no Critical/High finding, with explicit focus (Section 11) on: (a) the atomic generation-assignment critical sections genuinely race-safe under real concurrency, not merely sequential-test-passing; (b) no cross-model cascade was accidentally introduced; (c) `rotate`/`change_scopes` are gated by the same `LocalCapability::RegisterAgent` authorization `register`/`revoke` already require, with no bypass path.
- [ ] Every open question in Section 9 is recorded as still-open in the handoff.

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `cargo fmt --all -- --check` | exit 0 | command output |
| Static | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | command output |
| Unit | `cargo test -p loki-storage --all-targets -- --test-threads=1` | all existing plus new generation-read tests pass | run summary |
| Unit | `cargo test -p loki-local-authority -- --test-threads=1` | all existing tests pass plus `concurrent_session_issuance_yields_distinct_generations` | run summary |
| Unit | `cargo test -p loki-agents -- --test-threads=1` | all existing tests pass plus the four new rotate/scope-change tests plus `restart_preserves_generation_across_reopen` | run summary |
| Unit | `cargo test -p loki-acceptance-store` | all pass, including the corrected/split agent-vs-local grant tests named in Section 5/6 | run summary / test names |
| Concurrency/race | two real OS threads issuing sessions for one principal | no duplicate `generation`, no panic, no deadlock, reproduced ≥2 runs | test output |
| Integration | `cargo test --workspace -- --test-threads=1` | pass (documented flake class only) | run summary |
| Security | independent review: confirm no `MutationMethod`/`LocalCapability` enum change; confirm `i64::MAX` sentinel confined to `loki-acceptance-store`; confirm `rotate`/`change_scopes` share `register`/`revoke`'s exact authorization gate; confirm no cross-model cascade | no finding | reviewer note in handoff |
| Desktop/Cloud/Hardware/Containment | n/a — no IPC, no cloud, no device, no build-executor surface in this node | n/a | n/a |

## 9. Known open questions — do not resolve silently

1. **Changing `LocalAuthorityRepository::insert_authority_session`'s
   signature is a deliberate, justified pre-acceptance edit, not a
   violation of "no existing accepted method changes."** That trait was
   itself introduced by `P1-ACCEPTANCE-STORE-001`, which is not yet
   `accepted`/integrated — nothing outside this stacked worktree depends on
   its current shape. `AGENTS.md`'s protection targets already-accepted
   public crate APIs (`LocalAuthority`/`AgentRegistry`'s own methods, which
   this node does **not** change), not an internal repository trait two
   nodes deep in the same in-flight feature chain. Flagging this explicitly
   so a reviewer does not mistake it for scope creep.
2. **The `principal_sid` field's dual meaning (Windows SID vs. agent
   `connection_id`) is inherited, not introduced, and is not fixed here.**
   A future node that gives `MutationAcceptanceRequest` a real principal-type
   discriminant field (rather than deriving it from `method.requires_approval()`)
   would be a `loki-mutation-contract` contract change requiring its own
   review — explicitly out of this node's scope, recorded for
   `P1-AGENT-MUTATION-IDEMP-001`'s or a later hardening node's awareness.
3. **No cross-model "kill switch" exists after this node** (Section 1,
   ADR-0007's explicit limitation) — restated here because it is the single
   most likely thing a future reader could wrongly assume once both models
   have "a generation counter."
4. **`outcome_unknown` trigger contract remains undefined** — unaffected by
   this node; still open for whichever future node runs a physical effect.
5. **Lock-contention risk** (already named in `P1-ACCEPTANCE-STORE-001` §9
   item 4): this node adds two more code paths (`rotate`/`change_scopes`)
   that briefly hold the same single-writer `Storage` mutex — recorded as an
   incremental addition to that same named risk for `P1-RUNTIME-COMP-001`/
   `P1-MUTATION-REV-001` to verify at realistic load, not fixed here.

## 10. Risks and safety

- Risk level: `high` — this node changes how every future mutation
  acceptance validates "is the caller's authority still current," across
  both identity models; a mistake here fails either open (accepting a
  stale/revoked grant) or falsely closed (denying legitimate agents/humans).
- Failure modes: (d) unbound identity in state-machine invariants — the
  exact class ADR-0007 exists to close; (g) unsynchronized shared state —
  the generation-assignment critical sections must be genuinely race-safe,
  not merely passing under `--test-threads=1`.
- Rollback/recovery: purely additive schema (`ALTER TABLE ADD COLUMN`, new
  migrations only) — no destructive migration, no data loss risk to any
  existing row.
- Data/secret impact: `rotate` mints a new one-time secret exactly like
  `register` already does (same redaction/`Debug` handling via
  `AgentSecret`); no new secret class.
- Concurrency impact: real, incremental — see Section 9 item 5.
- Cloud/offline impact: none.
- Device impact: none.

## 11. Human gate

- Gate: none required to *start* this node — ADR-0007 already carries the
  human product/security-owner acceptance this node implements.
- Gate required before this node's acceptance is treated as final: an
  independent security review focused specifically on (a) the
  generation-assignment race-safety claims (Section 7/8), (b) confirmation
  that no cross-model cascade was introduced (Section 1's explicit
  limitation), and (c) `rotate`/`change_scopes`'s authorization gate has no
  bypass — mirroring the review-focus pattern already used for
  `P1-ACCEPTANCE-STORE-001`.
- Approver role: human product/security owner, on the independent review's
  findings.
- Approval evidence: independent review record under
  `Docs/graph/evidence/W6/P1-PRINCIPAL-GRANT-001/`.

## 12. Stop conditions

Return `BLOCKED_DEPENDENCY` if `P1-ACCEPTANCE-STORE-001`'s worktree is
abandoned/reset before this node starts (this node has nothing to stack on
otherwise). Return `NEEDS_REPLAN` if `P1-ACCEPTANCE-STORE-001`'s in-progress
test-review forces a code-shape change to `loki-storage::local_authority`/
`agents`/`loki-acceptance-store` after this node has already built on the
pre-change shape — rebase, do not silently patch around the divergence.
Return `NEEDS_REPLAN` rather than inventing a cross-model reconciliation if,
during implementation, a real product requirement for "one kill switch"
surfaces — that is a distinct future decision (ADR-0007's own Consequences
section says so explicitly), not something to absorb here.

## 13. Worker and reviewer

- Accountable worker: `loki-implementer` (one bounded node, ownership set
  named exactly in Section 5).
- Independent reviewer: `loki-security-reviewer` (authorization-gate and
  cross-model-boundary focus per Section 11) and `loki-test-reviewer`
  (concurrency/race matrix actually exercised, not merely named).
- Worktree/branch: continue in `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001`
  on branch `node/P1-ACCEPTANCE-STORE-001`, or a child branch
  `node/P1-PRINCIPAL-GRANT-001` stacked on it — orchestrator's choice,
  recorded in the handoff either way (Section 3).
- Handoff record: `Docs/graph/handoffs/P1-PRINCIPAL-GRANT-001-1.md`.

## 14. Human direction and waivers on record for this node (2026-08-28)

- ADR-0007 accepted, Option B, by the human product/security owner
  (2026-08-28) — this packet implements exactly that decision, including its
  explicit scope clarification that `rotate`/`change_scopes` are in-scope
  additive `AgentCredential` capability.
- No new human waiver required to start (unlike `P1-ACCEPTANCE-STORE-001`'s
  `DAT-002` waiver) — the only dependency-timing judgment this packet makes
  (Section 3) is a sequencing/stacking call within the planner's normal
  scheduling authority, not a security/policy waiver.

## 15. Planning provenance

Shaped by `loki-planner` (read-only), 2026-08-28, against the uncommitted
worktree at `L:\LokiAI-v1.2-wt\P1-ACCEPTANCE-STORE-001` (branch
`node/P1-ACCEPTANCE-STORE-001`), by direct inspection of:
`crates/loki-mutation-contract/src/{grant.rs,method.rs,request.rs}`;
`crates/loki-storage/src/{local_authority.rs,agents.rs,storage.rs}` and their
migrations `0004`–`0007`; `crates/loki-local-authority/src/{model.rs,store.rs}`;
`crates/loki-agents/src/lib.rs`; `crates/loki-acceptance-store/src/lib.rs`
and `tests/acceptance_flow.rs`; and `Docs/graph/handoffs/P1-ACCEPTANCE-STORE-001-1.md`.
The key finding not previously recorded anywhere: `MutationMethod::requires_approval()`
is already a complete, exhaustive, 1:1 discriminant for "which identity
model does this request's `principal_sid` belong to," which is what makes
Option B implementable without any `loki-mutation-contract` change or any
further ADR — this was verified by reading the enum's exhaustive match
arms, not inferred from documentation.
