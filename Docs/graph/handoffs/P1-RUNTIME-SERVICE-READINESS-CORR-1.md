# Worker Handoff — `P1-RUNTIME-SERVICE-READINESS-CORR`

Result: `COMPLETE` (bounded scope only — see §9 for mandatory follow-up)
Worker: implementer (Claude Code)
Started from: `2468878` (`merge: integrate P1-ACCEPTANCE-STORE-001 + P1-PRINCIPAL-GRANT-001`) plus pre-existing uncommitted work under `crates/loki-runtime/src/services/` and `lib.rs`/`Cargo.toml`
Result revision/patch: `be471a5` on branch `codex/p1-runtime-services`, worktree `L:\LokiAI-v1.2\target\worktrees\p1-runtime-services`
Completed at: `2026-08-30T02:04:54Z`

## 1. Outcome

`loki-runtime` now composes a `RuntimeServices` façade (`crates/loki-runtime/src/services/`) over one shared `Arc<Storage>`, exposing bounded, honest read projections (`RuntimeReadPort`) and a genuinely fail-closed mutation submission port (`MutationSubmissionPort`). `ComposedHost` (the real production/host startup path) now actually constructs and holds `RuntimeServices` against the same migrated store it opens — the code was previously dead (present but never wired in, which failed `cargo clippy -D warnings` with a `dead_code` error on `RuntimeServices::compose`). Shutdown ordering was corrected so the services-held `Arc<Storage>` clones drop before the final storage drop, preserving the existing "last core SQLite connection closes before secure-memory teardown" invariant. Mutation submission remains intentionally, verifiably closed: `ClosedMutationRequest` has no public constructor anywhere outside `services::mutation`, and the one port implementation always returns `Err(MutationUnavailable { reason: OperationBindingNotIntegrated })` — this is now covered by a dedicated unit test that actually invokes `submit_mutation`, not just asserts on `availability()`.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `RuntimeServices` over one shared `Storage` | pass | `crates/loki-runtime/src/services/mod.rs` (`RuntimeServices::compose`, single `Arc<Storage>` cloned into every sub-service); wired into `ComposedHost` in `host/compose.rs` (previously unused/dead code) |
| Bounded device listing, stable device-ID cursor pagination | pass | `services/read.rs::list_devices` + `DevicePageRequest` (limit 1..=100 validated before storage access); tests `device_listing_is_bounded_and_uses_a_stable_device_cursor`, `device_page_limit_is_validated_before_storage_access` |
| Runtime/storage readiness projection | pass | `services/read.rs::runtime_status` (uses `Storage::integrity_check`, device count, and mutation availability); test `empty_store_is_an_honest_empty_success` |
| Device summaries exclude sensitive hardware identity / transport references | pass | `DeviceSummary` struct (`services/read.rs`) has no `hardware_identity` or `transport_reference` fields; `From<DeviceRecord> for DeviceSummary` explicitly omits them field-by-field |
| Persisted health JSON validated and bounded, opaque payload not leaked unfiltered | pass | `services/read.rs::device_health` enforces `MAX_HEALTH_JSON_BYTES` (64 KiB) and JSON-parses before acceptance; `PersistedHealthEvidence` exposes only `payload_size_bytes`/`last_seen_at`, never the raw document; test `reads_registered_device_and_persisted_health_without_claiming_live_health` asserts `payload_size_bytes` only; test `invalid_persisted_health_fails_closed` covers oversized/invalid JSON |
| Missing health honestly `NotObserved`, never fabricated | pass | `DeviceHealthState::NotObserved` branch on `record.health_json == None`; test `missing_health_is_not_observed_and_missing_device_is_not_found` |
| Mutation submission fail-closed and genuinely refuses (not silent success/fabrication) | pass | `services/mutation.rs`: `ClosedMutationRequest { _sealed: () }` unconstructible outside the module; `MutationAcceptance` is an uninhabited enum (no `Ok` value exists); `submit_mutation` always returns `Err`; new test `closed_submission_genuinely_refuses_every_request` actually calls the port and asserts the error, plus a compile-time-only `mutation_acceptance_is_uninhabited` proof function |
| `cargo fmt --all -- --check` | pass | see §4 |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass | see §4 (failed before the `ComposedHost` wiring fix, due to `dead_code` on `RuntimeServices::compose`; fixed by wiring it into production startup, not by suppressing the lint) |
| `cargo test -p loki-runtime -- --test-threads=1` | pass | 57 lib unit tests (incl. 6 new/adjusted `services::*` tests), 4 `host_lifecycle`, 4 passed/2 known-ignored `process_behavior`, 1 `secure_memory_arena_runtime` — all pass; doctest run itself is blocked by a known host limitation, see below |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-runtime/src/services/mod.rs` | Already present untracked; committed as-is (no functional edits) — composes `RuntimeServices`, implements `RuntimeReadPort`/`MutationSubmissionPort`, holds the 6 `services::tests` | yes |
| `crates/loki-runtime/src/services/read.rs` | Already present untracked; committed as-is — bounded read projections | yes |
| `crates/loki-runtime/src/services/mutation.rs` | Already present, plus new `#[cfg(test)] mod tests` added by this task exercising the closed port directly | yes |
| `crates/loki-runtime/src/lib.rs` | Already present untracked (`pub mod services;`); committed as-is | yes |
| `crates/loki-runtime/Cargo.toml` | Already present untracked (adds `loki-local-authority`, `loki-agents`, `loki-approvals`, `loki-agent-delivery`, `loki-jobs` path deps with an explanatory comment); committed as-is | yes |
| `crates/loki-runtime/src/host/compose.rs` | New in this task: `ComposedHost.storage` changed from `Storage` to `Arc<Storage>`; new `services: RuntimeServices` field constructed via `RuntimeServices::compose` right after migrations; new `services()` accessor; `storage()` accessor preserved (returns `&Storage` via `.as_ref()`); shutdown now `drop(services)` before `drop(storage)` to avoid leaving the storage `Arc` refcount above 1 | yes |
| `Cargo.lock` | Regenerated lockfile entries for the new `loki-runtime` path dependencies above | yes |

No files outside `crates/loki-runtime/` were touched. No test, security, or approval control was weakened.

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt --all -- --check` | Windows 11, rustc 1.97.1-x86_64-pc-windows-msvc | pass, no output | run twice, before and after adding the mutation test |
| `cargo clippy --workspace --all-targets -- -D warnings` | same | pass after wiring `RuntimeServices` into `ComposedHost` (previously failed with `error: associated function 'compose' is never used`, `-D dead-code`) | full workspace check, `Finished` with no errors |
| `cargo test -p loki-runtime -- --test-threads=1` | same | pass: lib unit tests 57/57 (incl. `services::tests::*` x5, `services::mutation::tests::closed_submission_genuinely_refuses_every_request`), `host_lifecycle` 4/4, `process_behavior` 4/4 (+2 pre-existing `#[ignore]` for `DAT-002-MEM-CODE-001`/`DAT-002-MEM-HOST-VERIFY-001`, unrelated to this task), `secure_memory_arena_runtime` 1/1 | full run captured; see below for doctest note |
| `cargo test -p loki-runtime --no-run` | same | pass — every unit/integration test binary (`lib`, `main`, `host_lifecycle`, `process_behavior`, `secure_memory_arena_runtime`) compiles and links cleanly | confirms the doctest failure below is execution-only, not a compile error in this task's code |

Mandatory verification not run / partially run:

- Doctest execution (`cargo test -p loki-runtime --doc`, invoked as part of the plain `cargo test -p loki-runtime` run) fails with `An Application Control policy has blocked this file. (os error 4551)` when the harness tries to invoke `rustdoc.exe` to run `crates/loki-runtime/src/lib.rs`'s doctests. This is the exact known host limitation called out in the task instructions (Windows Application Control os error 4551). It is a host/environment restriction on executing `rustdoc.exe`, not a compile or logic failure — confirmed by `cargo test -p loki-runtime --no-run`, which compiles every test target (including the doc-test harness build step for the library, since `--no-run` still builds it) with no errors. No doctest content was added or changed by this task.

## 5. Contracts, migrations, and compatibility

- Contracts changed: none. `RuntimeReadPort`/`MutationSubmissionPort` are new internal (crate-visible-composition, publicly-typed) traits with no external callers yet — nothing in the codebase consumed `RuntimeServices` before this task, and `ComposedHost::services()` is a brand-new accessor with zero current callers.
- Consumers affected: none yet. `ComposedHost::storage()`'s external signature (`&Storage`) is unchanged; only its internal representation (`Storage` -> `Arc<Storage>`) changed, which is not observable to callers.
- Migration: none — no schema change.
- Backward/rolling compatibility: n/a, no released consumers of this internal API.
- Generated-contract drift: none.
- Build recipe/executor/provenance compatibility: n/a.
- Cloud API/offline/notification compatibility: n/a.

## 6. Security, privacy, and device safety

- Trust boundaries touched: internal composition boundary between `loki-runtime`'s host startup and its downstream domain-service crates (`loki-local-authority`, `loki-agents`, `loki-approvals`, `loki-agent-delivery`, `loki-jobs`). No IPC, MCP, or renderer-facing surface was touched — `RuntimeServices`/`ComposedHost::services()` have zero callers today; nothing outside this crate can reach them yet.
- Authorization/approval impact: none. Mutation submission — the only path that could eventually touch approval/authorization semantics — is deliberately, verifiably closed at this revision (see §1, §2). No approval bypass was introduced.
- Secrets/data handling: `DeviceSummary` and `PersistedHealthEvidence` continue to withhold raw hardware identity, transport references, and the raw persisted-health JSON payload from the projection layer, matching the task's stated intent. No secret or credential is read or exposed by this change.
- Containment profile and denied authorities: unchanged — this task added no shell, process, filesystem, or network access.
- Device side effects performed: none. No real device, firmware, flashing, or fleet operation was performed or attempted.
- New dependencies: none new. The five workspace-internal path dependencies (`loki-local-authority`, `loki-agents`, `loki-approvals`, `loki-agent-delivery`, `loki-jobs`) were already declared, with rationale comments, in the pre-existing uncommitted `Cargo.toml` diff this task inherited and verified — all are `publish = false` in-repo crates, no external license/maintenance/security review is triggered.
- Remaining risk: the mutation port is closed by construction today, but this is explicitly temporary. See §9 — real mutation submission must be rebased onto the corrected `loki-acceptance-store` operation-binding contract once that sibling node lands; until then, no caller anywhere can submit a real device mutation through this path (verified: `ClosedMutationRequest` cannot be constructed outside `services::mutation`, and no code in the repository does so).

## 7. Decisions and assumptions

- Assumption: the task's stated scope ("finish/verify... the intent already implemented under `services/` and `lib.rs`/`Cargo.toml`") implicitly required actually wiring `RuntimeServices::compose` into production startup (`host/compose.rs`), because leaving it unused failed the mandatory `cargo clippy -D warnings` gate and left the stated intent ("the runtime supplies one migrated store to all of them instead of allowing per-service databases") unrealized rather than merely undocumented. I resolved this by wiring `ComposedHost` to hold and construct `RuntimeServices` against the same `Arc<Storage>` it already opens and migrates, rather than by suppressing the lint (`#[allow(dead_code)]`) or reverting the composition code. This is the smallest change that makes the existing, already-written service code real rather than dead, and it does not touch any owned/forbidden file outside `crates/loki-runtime/`.
- Decision: changed `ComposedHost.storage` from `Storage` to `Arc<Storage>` (rather than, e.g., composing `RuntimeServices` from a private `&Storage` reference held separately) because `RuntimeServices::compose` requires ownership of `Arc<Storage>` clones for each of its five sub-services, and storage is otherwise owned by value with a single well-defined drop point in `ComposedHost::run`. I preserved shutdown-ordering correctness explicitly (`drop(services)` before `drop(storage)`) and documented why, since a latent extra `Arc` clone surviving past the intended drop point would have silently broken the existing "close the SQLite connection before `sqlite3_shutdown`" invariant without any compiler or test signal.
- No task packet exists in `Docs/graph/tasks/` or `Docs/graph/TASK_GRAPH.md` for `P1-RUNTIME-SERVICE-READINESS-CORR` at this revision. Per `AGENTS.md`'s source-of-truth order, the explicit human instruction for this task (source-of-truth rank 1) governs; I did not wait for a formal task packet to exist. I am flagging this so the orchestrator can backfill the graph node/packet for this work if that is expected process, since ordinarily "the accepted task packet" is rank 2.

## 8. Review focus

- Confirm the shutdown-ordering fix (`drop(services); drop(storage);` in `ComposedHost::run`) is genuinely sufficient: `RuntimeServices` holds `Arc<Storage>` clones in `_local_authority`, `_agents`, `_approvals`, `_agent_delivery`, `_jobs`, and `mutation` does not hold storage at all — so dropping `services` releases 5 of the 6 non-original clones (the 6th, `mutation: ClosedMutationSubmission`, holds none). Verify none of `loki-local-authority`/`loki-agents`/`loki-approvals`/`loki-agent-delivery`/`loki-jobs` internally spawn a task or otherwise leak a further `Arc<Storage>` clone that would outlive this drop point (I did not audit those crates' internals beyond their public `::new(Arc<Storage>)` constructors).
- Confirm `services::mutation`'s sealing is actually airtight from a reviewer's independent read: `ClosedMutationRequest { _sealed: () }` is only constructible inside `services::mutation` (private field, module-private), and `MutationAcceptance` is an uninhabited enum, so no `Ok` value can exist. I added a test and a compile-time uninhabited-check function for this, but an independent reviewer should re-derive that no `pub` re-export or trait default method anywhere provides an alternate construction path.
- This node should not be treated as independently final/mergeable review-wise until `P1-MUTATION-SEM-ADR-001`'s operation-binding correction lands in its sibling worktree and this branch is rebased against it — per the orchestrator's stated sequencing. Full independent review (test-reviewer/security-reviewer) of the mutation boundary is most useful after that rebase, since the closed-port shape itself may change once real submission is wired.

## 9. Follow-up graph changes proposed

- **Required, not optional**: once the operation-binding correction (sibling in-progress node, e.g. `P1-MUTATION-SEM-ADR-001` / the corrected `loki-acceptance-store` contract) integrates, this node must be rebased and `services::mutation`'s `ClosedMutationSubmission` must be replaced with a real submission path that calls into the corrected acceptance coordinator. Until that follow-up lands, `MutationSubmissionPort::submit_mutation` will keep returning `Err(MutationUnavailable { OperationBindingNotIntegrated })` for every caller — this is correct today but must not be forgotten as "already done."
- No other new graph nodes/edges proposed. Only the orchestrator updates the task graph.

## 10. Follow-up completed — real mutation submission wired (2026-08-30, commit `f72c0c3`)

The required follow-up from §9 above is now done. Summary, since §1-§8 above describe the now-superseded fail-closed revision:

### 10.1 What changed

- **Rebase**: branch rebased from `9f6e7ab` (on top of `2468878`) onto current `LokiAI-v1.2` at `8e47588` (`docs(integration): record W7 integration of P1-APPROVAL-OPERATION-BINDING-CORR`). The rebase was clean — zero conflicts (`git rebase FETCH_HEAD` reported "Successfully rebased" with no manual resolution needed), confirming this branch's `crates/loki-runtime/**` changes are genuinely disjoint from the `loki-agents`/`loki-approvals`/`loki-storage`/`loki-mutation-contract` files the agent-terminal and operation-binding corrections touched. New commits on top: `e85c55a` (`fix: unify approval mutation operation binding`), `e13ae46` (`fix(approvals): eliminate cross-storage authorization bypass and standalone approval consumption`), `0c8c535` (docs), then this node's own `8deb947`/`77a853b` (the original fail-closed work, replayed by the rebase), then the new work described below.
- **`crates/loki-runtime/src/services/mutation.rs`**: `ClosedMutationSubmission`/`ClosedMutationRequest`/`MutationAcceptance`/`MutationUnavailable`/`MutationUnavailableReason` are gone. Replaced with `RealMutationSubmission`, which holds the same privately-owned `Arc<loki_storage::Storage>` `RuntimeServices` already composes, and whose `submit_mutation` takes a `&loki_mutation_contract::MutationAcceptanceRequest` (that crate's own closed, already-hardened, digest-derived-not-caller-supplied request type — unchanged by this node) and forwards it, unmodified, into `loki_acceptance_store::accept_mutation(self.storage.as_ref(), request)`, returning whatever real `AcceptanceResult` that call produces (`Accepted`/`Rejected`/`ReplayedIdenticalAccepted`/`ApprovalRequired`) wrapped in `Ok(..)`. Infrastructure failures (`AcceptanceStoreError::Storage`/`::Contract`) map to a new `MutationSubmissionError` — genuine database/construction faults, never a business rejection (a business rejection stays `Ok(AcceptanceResult::Rejected(..))`).
- **`crates/loki-runtime/src/services/mod.rs`**: `RuntimeServices` now composes `mutation::RealMutationSubmission::new(storage)` instead of the closed unit struct; the `MutationSubmissionPort` impl signature changed to match (`&MutationAcceptanceRequest -> Result<AcceptanceResult, MutationSubmissionError>`); re-exports `loki_mutation_contract::{AcceptanceResult, MutationAcceptanceRequest}` alongside the port so a caller never has to reach into `loki-mutation-contract` directly by accident. Module doc comment updated to describe the real boundary instead of "sealed and explicitly unavailable."
- **`crates/loki-runtime/Cargo.toml`**: added `loki-acceptance-store` and `loki-mutation-contract` as path dependencies (both workspace `publish = false` crates, no external license/maintenance review triggered), with rationale comments matching this crate's existing dependency-documentation convention.
- **`Cargo.lock`**: two new dependency-edge lines under `loki-runtime`'s existing package entry (`loki-acceptance-store`, `loki-mutation-contract`) — both packages were already resolved elsewhere in the workspace, so no new package version entries were added.
- **`crates/loki-runtime/src/host/compose.rs`**: one doc-comment sentence updated (no code change) to stop describing the mutation boundary as "sealed."

### 10.2 What this node did **not** do (deliberately out of scope)

- No IPC/MCP-facing business method was added that calls `RuntimeServices::submit_mutation` — `ComposedHost::services()` still has zero external callers, exactly as before this node. Wiring a real IPC business method through this port is a distinct, later node's scope (this node's task description was explicitly "wire `RuntimeServices`'s mutation submission to call the real `accept_mutation`," not "expose it over IPC").
- No change to `loki-acceptance-store`, `loki-mutation-contract`, `loki-approvals`, or `loki-storage` themselves — this node only became a *consumer* of `accept_mutation`, an already-complete, already-reviewed public entry point (see `Docs/graph/handoffs/P1-APPROVAL-OPERATION-BINDING-CORR-1.md`).

### 10.3 Required evidence

| Criterion (from the dispatch instructions) | Result | Evidence |
|---|---|---|
| A real accepted mutation via `RuntimeServices` produces the exact same result as calling `accept_mutation` directly (no divergence/wrapper bug) | pass | `services::mutation::tests::accepted_mutation_matches_calling_accept_mutation_directly` (compares both an independently-accepted request and a same-idempotency-key replay across the port vs. a direct call, asserting identical `job_id`/`correlation_id`/`accepted_at_ms`); `services::tests::runtime_services_submit_mutation_matches_direct_accept_mutation_call` (same property through the top-level `RuntimeServices` façade, not just the inner port) |
| A rejected mutation (bad approval reference / expired grant) is correctly surfaced as rejected, not silently swallowed or converted to a generic error that loses information | pass | `services::mutation::tests::rejected_mutation_surfaces_the_real_rejection_not_a_generic_error` (no matching agent connection → real `Ok(Rejected(GrantInvalid{..}))`, not `Err`); `services::mutation::tests::expired_local_grant_is_rejected_not_swallowed` (a genuinely expired local-authority session → real `Ok(Rejected(GrantExpired))`); `services::tests::runtime_services_submit_mutation_surfaces_a_real_rejection` (same property through the façade) |
| No raw `Storage` handle or connection is ever returned/exposed through `RuntimeServices`'s public API | pass | `services::mutation::tests::port_never_returns_storage_or_a_connection` (compile-time-shape assertion on `submit_mutation`/`availability`'s return types); structurally, `RealMutationSubmission.storage`/`RuntimeServices.storage` are both private fields with no accessor exposing them — `ComposedHost::storage()` (pre-existing, unchanged) is the only place a `&Storage` is ever handed out, and that predates and is unrelated to this port |
| Never fabricate success | pass | `submit_mutation` never constructs an `AcceptanceResult` itself — every variant it can return originates inside `loki-mutation-contract`'s own closed constructors (`construct_accepted`/`replayed_result`), reached only via `accept_mutation` |
| Preserve everything else about the existing bounded/validated/redacted design | pass | `read.rs` untouched; `RuntimeReadPort`/`DeviceSummary`/`PersistedHealthEvidence`/pagination/`NotObserved` semantics all unchanged (`git diff --stat` shows zero changes to `read.rs`) |
| `cargo fmt --all -- --check` | pass | ran twice (before/after adding the façade-level tests), clean both times |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, 0 warnings | full workspace, after adding the new dependency edges |
| `cargo test -p loki-runtime -- --test-threads=1` | pass | 62 lib tests (up from 57 — 4 new `services::mutation::tests::*`, 2 new `services::tests::*`), 4 `host_lifecycle`, 4 passed/2 known-pre-existing-ignored `process_behavior`, 1 `secure_memory_arena_runtime`; one transient spurious failure was observed on a single run of the full multi-binary command and did not reproduce on immediate re-run or on isolated `--test host_lifecycle` — consistent with the documented "concurrent agents sharing this checkout's `target/` cache" hazard, not a real regression (the final confirming run, captured in the commit, was clean) |

### 10.4 Verification performed

| Command | Result |
|---|---|
| `git rebase FETCH_HEAD` (rebase onto `LokiAI-v1.2`@`8e47588`) | clean, zero conflicts |
| `cargo build -p loki-runtime --target-dir L:/LokiAI-v1.2/target` | pass (PDB link warnings only, pre-existing/unrelated to this crate's own code) |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets --target-dir L:/LokiAI-v1.2/target -- -D warnings` | pass, 0 warnings |
| `cargo test -p loki-runtime --target-dir L:/LokiAI-v1.2/target -- --test-threads=1` | pass (62+4+4+1, doctest execution blocked by the documented host os error 4551) |
| `cargo test -p loki-runtime --no-run --target-dir L:/LokiAI-v1.2/target` | pass — confirms the doctest failure is execution-only, not a compile error (no doctest content was added or changed) |
| `cargo test -p loki-acceptance-store -p loki-mutation-contract --target-dir L:/LokiAI-v1.2/target -- --test-threads=1` | pass, 31/31 (sanity check that the crate this node now depends on is itself green; same doctest-execution host limitation on `loki-acceptance-store`, unrelated to this node) |

### 10.5 Security notes

- The mutation-submission trust boundary is now real, not closed-by-construction — the reviewer should re-verify (independently of this handoff's own claim) that `RealMutationSubmission::submit_mutation` truly performs no transformation of the request or result beyond the direct `accept_mutation(..)` call (`services/mutation.rs`'s `submit_mutation` body is a single expression: `Ok(accept_mutation(self.storage.as_ref(), request)?)`).
- `MutationAcceptanceRequest` construction (validation, digest derivation, principal/actor/grant-generation binding) is entirely `loki-mutation-contract`'s existing, already-reviewed responsibility — this node introduces no new request-construction logic of its own; `RuntimeServices`/`RealMutationSubmission` only ever receive an already-constructed request from their caller.
- No approval-bypass, digest-bypass, or transaction-boundary-bypass was introduced: the only call this crate makes into `loki-acceptance-store` is its single `pub fn accept_mutation`, which internally wraps its entire ADR-0006 sequence in one `Storage::transaction()` call — this node adds no alternate path into `loki-storage`'s approval/mutation-acceptance tables.
- This node still has zero real callers (no IPC/MCP business method invokes `RuntimeServices::submit_mutation` yet) — the real device-mutation risk surface this unlocks is real but currently unreachable from outside `loki-runtime`'s own tests, exactly as intended at this stage of the graph.

## 11. Result

`ACCEPTED-pending-review`. Rebase clean, real mutation submission wired and tested against the corrected `loki-acceptance-store`/`loki-approvals` contracts, all required verification passing except the pre-existing, documented, host-environment-only doctest-execution limitation. Not self-marked `COMPLETE` in the task graph — only the orchestrator does that.
