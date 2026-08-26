# Worker Handoff — `P1-MUTATION-CONTRACT-001`

Result: `COMPLETE`
Worker: `loki-implementer` (Claude Code, one bounded node)
Started from: `7a1115f5b291c71c4ffb0e13deee8d1af9654268`
Result revision/patch: uncommitted working tree on top of `7a1115f5b291c71c4ffb0e13deee8d1af9654268` (new crate `crates/loki-mutation-contract/**`, one added line in root `Cargo.toml`, this handoff)
Completed at: `2026-08-25T00:00:00Z`

## 1. Outcome

New crate `crates/loki-mutation-contract` exists, is registered as a workspace
member, and defines the closed value types and pure functions ADR-0006's
transactional runtime mutation acceptance decision requires: `GrantGeneration`
+ `evaluate_grant`, `RequestDigest`/`IdempotencyKey` + `compute_request_digest`,
the closed `MutationMethod` enum, `MutationAcceptanceRequest` +
`ApprovalReference`, the closed `AcceptanceResult`/`MutationRejection`
discriminated states + `evaluate_replay`/`construct_accepted`, and
`RevocationDisposition` + `evaluate_revocation_disposition`. The crate has zero
I/O, zero async, and zero callback/trait-object/mutex-as-authorization
pattern; every closed enum has at least one exhaustive `match` with no
wildcard arm. `cargo fmt`, `cargo clippy -D warnings`, and `cargo test` all
pass for `-p loki-mutation-contract` (23 unit tests, all named adversarial
cases present). No file outside this crate, the one `Cargo.toml` line, and
this handoff was modified. The five Section 9 open questions are recorded
below as still unresolved, per the packet's own requirement — this crate
documents each as an extension point, it does not decide any of them.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `cargo build -p loki-mutation-contract` succeeds, zero warnings | pass | §4 below; no warnings emitted by the compiler for this crate (only pre-existing unrelated linker `LNK4099` PDB-not-found noise from `loki-storage`'s bundled OpenSSL, present for every crate in this workspace, not introduced here) |
| `cargo test -p loki-mutation-contract` passes, incl. every named adversarial case | pass | §4 below; all 8 explicitly-named tests present and passing, plus 15 supporting tests |
| New crate's `Cargo.toml` has no I/O crate; depends on `loki-storage` only for `ids`/`states`/error-pattern reuse | pass | `crates/loki-mutation-contract/Cargo.toml` lists only `loki-storage` (path dep) and `sha2`; source only imports `loki_storage::ids::{JobId, CorrelationId}` (in `result.rs`) — no `loki_storage::storage`/`managed`/`key`/`secret` import anywhere |
| No `dyn Fn`/`Box<dyn ...>`-callback/`Arc<Mutex<...>>`-authorization pattern (grep-verifiable) | pass | `Grep` for `dyn Fn|Box<dyn|Arc<Mutex|Mutex<` under `crates/loki-mutation-contract/src` matches only this crate's own doc-comment sentences describing what is absent, in `lib.rs` |
| Every closed enum has ≥1 exhaustiveness-proof `match`, no wildcard arm | pass | `MutationMethod::as_str`/`policy_tier` (method.rs); `MutationRejection`/`AcceptanceResult` exhaustive test matches (result.rs); `RevocationDisposition` exhaustive test match (revocation.rs) |
| `git diff --stat` shows changes only under `crates/loki-mutation-contract/`, one added `Cargo.toml` line, and this handoff | pass | §3 below; `Cargo.lock` also changed as an unavoidable side effect of adding a workspace member (see §5) |
| Handoff records all five Section 9 open questions as unresolved | pass | §7 below |
| Failure/unavailable behavior explicit and tested | pass | `MutationContractError::InvalidInput` covers every construction-time validation failure; `MutationRejection` covers every runtime rejection; both are exercised in tests, no panics on malformed input |
| No prohibited security/architecture boundary weakened | pass | see §6 |
| No unrelated files change | pass | §3 below |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-mutation-contract/Cargo.toml` | new crate manifest | yes |
| `crates/loki-mutation-contract/src/lib.rs` | crate root doc + re-exports | yes |
| `crates/loki-mutation-contract/src/error.rs` | `MutationContractError` | yes |
| `crates/loki-mutation-contract/src/grant.rs` | `GrantGeneration`, `evaluate_grant` | yes |
| `crates/loki-mutation-contract/src/digest.rs` | `RequestDigest`, `IdempotencyKey`, `compute_request_digest` | yes |
| `crates/loki-mutation-contract/src/method.rs` | `MutationMethod`, `PolicyTier` | yes |
| `crates/loki-mutation-contract/src/request.rs` | `MutationAcceptanceRequest`, `ApprovalReference` | yes |
| `crates/loki-mutation-contract/src/result.rs` | `AcceptanceResult`, `MutationRejection`, `AcceptanceRecordView`, `ReplayDecision`, `evaluate_replay`, `replayed_result`, `construct_accepted` | yes |
| `crates/loki-mutation-contract/src/revocation.rs` | `RevocationDisposition`, `evaluate_revocation_disposition` | yes |
| `Cargo.toml` (root) | exactly one added line: `"crates/loki-mutation-contract",` in `[workspace] members` | yes |
| `Cargo.lock` | regenerated by `cargo build`/`cargo test` to add the new crate's own lockfile entries (`loki-mutation-contract`, `sha2` already present via sibling crates) — not hand-edited, no other package's pinned version changed | yes (generated artifact, unavoidable side effect of the one authorized `Cargo.toml` line, not a separate scope violation) |
| `Docs/graph/handoffs/P1-MUTATION-CONTRACT-001-1.md` | this handoff | yes |

No other file was modified by this worker. `Docs/graph/TASK_GRAPH.md`,
`Docs/graph/tasks/README.md`, `Docs/graph/decisions/ADR-0006-...`,
`Docs/graph/tasks/P1-MUTATION-SEM-ADR-001.md`,
`Docs/graph/handoffs/P1-MUTATION-SEM-ADR-001-1.md`,
`Docs/graph/tasks/BLD-001-CORR-001*.md`, and `Docs/graph/tasks/P1-MUTATION-
CONTRACT-001.md` itself were already present/modified in the working tree
before this node started (visible in the starting `git status`) and were not
touched by this worker.

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo build -p loki-mutation-contract` | Windows 11, `rustc`/`cargo` per workspace `rust-version = 1.97.1`, MSVC toolchain | exit 0, no warnings for this crate | `Compiling loki-mutation-contract v1.2.0 ... Finished` |
| `cargo fmt -p loki-mutation-contract -- --check` | same | exit 0 (after one `cargo fmt` pass to apply rustfmt's canonical formatting) | empty diff on rerun |
| `cargo clippy -p loki-mutation-contract -- -D warnings` | same | exit 0 | `Finished` with zero clippy diagnostics (one `collapsible_if` finding was fixed by using a let-chain, per Rust 2024 edition) |
| `cargo test -p loki-mutation-contract --lib` | same | 23/23 passed | full test name list captured below |
| `cargo test -p loki-mutation-contract --doc` | same | 0 doctests (none written; no doc-comment code blocks) | `test result: ok. 0 passed` |
| `git diff --stat` / `git status --porcelain` | same | matches §3 exactly | command output reviewed directly |
| Manual grep security review: no `dyn Fn`/`Box<dyn ...>`-callback/`Arc<Mutex<...>>`-authorization pattern | same | no finding (only doc-comment prose mentions the excluded patterns) | `Grep` tool, pattern `dyn Fn|Box<dyn|Arc<Mutex|Mutex<`, path `crates/loki-mutation-contract/src` |
| Manual grep: no `rusqlite`/`tokio`/`async fn`/`std::net`/`std::fs` in this crate | same | no finding | `Grep` tool over `crates/loki-mutation-contract` |

Test names from the passing `cargo test -p loki-mutation-contract --lib` run
(one flaky rerun was needed — see note below):

```
test grant::tests::malformed_grant_generation_is_rejected ... ok
test method::tests::device_operation_methods_require_approval_and_agent_methods_do_not ... ok
test digest::tests::digest_changes_with_grant_generation ... ok
test error::tests::display_does_not_panic ... ok
test result::tests::acceptance_result_variants_carry_no_contradictory_fields ... ok
test grant::tests::revoked_grant_before_evaluation_time_denies_acceptance ... ok
test grant::tests::valid_grant_within_window_is_accepted ... ok
test method::tests::every_variant_round_trips_through_as_str_and_parse ... ok
test digest::tests::digest_distinguishes_field_boundaries_and_artifact_absence ... ok
test request::tests::request_digest_is_always_derived_not_caller_supplied ... ok
test request::tests::approval_reference_may_be_bound_to_the_computed_digest ... ok
test digest::tests::idempotency_key_rejects_malformed_input ... ok
test result::tests::mutation_rejection_is_closed_with_exhaustive_match ... ok
test digest::tests::request_digest_try_from_str_validates_shape ... ok
test method::tests::unknown_mutation_method_string_is_not_constructible ... ok
test result::tests::agent_method_constructs_accepted_without_any_approval_reference ... ok
test grant::tests::expired_grant_denies_acceptance ... ok
test result::tests::approval_required_method_cannot_construct_accepted_without_approval_reference ... ok
test revocation::tests::post_commit_revocation_requires_reconciliation ... ok
test revocation::tests::revocation_disposition_is_closed_with_exhaustive_match ... ok
test result::tests::same_key_different_digest_is_rejected_as_digest_conflict ... ok
test revocation::tests::pre_commit_revocation_denies_acceptance ... ok
test result::tests::same_key_same_digest_replays_original_accepted_result ... ok

test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

All 8 names the packet's §8 table requires are present exactly:
`same_key_same_digest_replays_original_accepted_result`,
`same_key_different_digest_is_rejected_as_digest_conflict`,
`malformed_grant_generation_is_rejected`,
`revoked_grant_before_evaluation_time_denies_acceptance`,
`expired_grant_denies_acceptance`,
`approval_required_method_cannot_construct_accepted_without_approval_reference`,
`unknown_mutation_method_string_is_not_constructible`,
`acceptance_result_variants_carry_no_contradictory_fields`.

Mandatory verification not run:

- none.

Environment note (not a defect in this crate): the first
`cargo test -p loki-mutation-contract --lib` invocation failed with `An
Application Control policy has blocked this file. (os error 4551)` before the
freshly linked test binary ever executed. This is a known, pre-existing,
transient Windows Application Control behavior in this environment — the same
failure reproduces immediately for `cargo test -p loki-approvals --lib`
(an untouched sibling crate, checked directly in this session for
comparison) and is already recorded for other nodes
(`Docs/graph/evidence/W4/DAT-002/local-part-one-integration.md`,
`Docs/graph/handoffs/P1-DAT-INTEGRATION-1.md`: "first full Rust test attempt
was interrupted... The exact package then passed"). An immediate rerun of the
exact same command passed cleanly (23/23) with no code change — the results
in this section are from that passing rerun.

## 5. Contracts, migrations, and compatibility

- Contracts changed: none — this is a new internal Rust crate with no wire
  schema. `P1-RPC-CONTRACT-001-R1` (not yet dispatched) is the node that
  would eventually expose any of this at the RPC boundary; that is out of
  this node's scope and was not touched.
- Consumers affected: none yet — no crate depends on
  `loki-mutation-contract` today. `P1-ACCEPTANCE-STORE-001` is the intended
  first consumer.
- Migration: none — no persisted schema exists in this crate.
- Backward/rolling compatibility: n/a — first version of a new crate.
- Generated-contract drift: n/a — `Cargo.lock` is the only generated file
  touched, and only to add this crate's own entries; see §3.
- Build recipe/executor/provenance compatibility: n/a — no build-recipe code
  in this crate.
- Cloud API/offline/notification compatibility: n/a directly, but see §7
  item 5 for the live ADR-0006/ADR-0002 tension this crate deliberately does
  not resolve.

## 6. Security, privacy, and device safety

- Trust boundaries touched: none at runtime — this crate is not wired into
  any process, IPC boundary, or execution path yet. It defines the *shape*
  of a future trust boundary (`P1-ACCEPTANCE-STORE-001`'s transaction) but
  performs no authorization, validation-at-a-live-boundary, or I/O itself.
- Authorization/approval impact: none directly executed. The type shapes
  (`ApprovalReference`, `construct_accepted`'s refusal to build `Accepted`
  for an approval-gated method without a matching bound approval reference)
  encode ADR-0006's approval-consumption requirement as a compile-time/
  construction-time guarantee for whatever code calls into this crate later;
  they do not themselves consume or validate a real approval (no I/O in this
  crate).
- Secrets/data handling: none — no secret material, no database, no
  logging/telemetry sink in this crate.
- Containment profile and denied authorities: n/a — no process/executor
  interaction.
- Device side effects performed: none.
- New dependencies: `sha2 = "=0.10.9"` — already a pinned, reviewed
  dependency of `loki-approvals`/`loki-agents` (RustCrypto, MIT/Apache-2.0,
  actively maintained, same exact version reused so the workspace lockfile
  resolves one copy); `loki-storage` — internal workspace crate, path
  dependency, used only for its `ids`/`states` modules (see the Cargo.toml
  justification comment). No new external dependency was introduced beyond
  what sibling crates already carry.
- Remaining risk: the crate itself is inert (no I/O, no callback surface),
  so its standalone risk is low. Its *downstream* risk is what the packet's
  §10 already flags: an incorrect closed-set/enum shape here becomes
  load-bearing for every mutation-boundary node built on it
  (`P1-ACCEPTANCE-STORE-001` through `P1-AGENT-MUTATION-IDEMP-001` and
  `P1-MUTATION-REV-001`). This handoff does not claim those downstream nodes
  are safe to build against this contract without independent review — see
  §8.

## 7. Decisions and assumptions

Per the packet's own acceptance criterion, all five Section 9 "open
questions — do not resolve silently" are recorded here as still unresolved,
each with the exact type/doc-comment location this crate left as an
extension point rather than a decision:

1. **"Declared safe point" for cooperative post-commit cancellation.**
   Unresolved. `RevocationDisposition` (`crates/loki-mutation-contract/src/
   revocation.rs`) exposes only the coarse pre-commit/post-commit
   distinction (`DeniesAcceptance` / `RequiresPostCommitReconciliation`) and
   documents, by citation to `loki-jobs`'s `request_cancel`/
   `is_cancel_requested`/`acknowledge_cancelled` (`crates/loki-jobs/src/
   engine.rs`), that a generic "safe point" mechanism is not invented here.
   Blocks: `P1-ACCEPTANCE-STORE-001`, `P1-MUTATION-REV-001` (whichever
   defines the actual cooperative-cancellation call sites).
2. **`outcome_unknown` detection contract.** Unresolved. The same
   `revocation.rs` doc comment states this module reuses
   `loki_storage::states::JobState::OutcomeUnknown`/`Cancelled` as the
   closed *state* only, and explicitly does not define the trigger contract
   for what counts as "uncertain" vs. "clearly failed." Blocks:
   `P1-ACCEPTANCE-STORE-001`, `P1-MUTATION-REV-001`.
3. **`GrantGeneration` vs. `LocalSession`.** Unresolved by design.
   `crates/loki-mutation-contract/src/grant.rs`'s module doc comment
   documents the distinction (per-IPC-session token vs. durable
   cross-session credential epoch) and explicitly states this crate does
   not assume how — or whether — the two will later relate. Blocks:
   `P1-PRINCIPAL-GRANT-001` (named in the packet as owning this
   reconciliation).
4. **Cross-crate single-transaction feasibility.** Unresolved as a
   feasibility question; only *direction* was recorded (packet §14): the
   human confirmed the target architecture is one shared local database,
   not distributed, but the actual consolidation of `loki-approvals`,
   `loki-agents`, `loki-agent-delivery`, and `loki-local-authority` onto one
   shared `loki-storage`-style connection is unbuilt. This crate's types
   remain deliberately connection-agnostic (pure data/functions, no
   `Connection`/`Transaction` type appears anywhere in this crate) so they
   do not assume a topology that does not exist yet. Blocks:
   `P1-ACCEPTANCE-STORE-001` — per the packet, this feasibility question
   must reach the human/security owner before that node is dispatched. This
   worker did not route it there; that remains the orchestrator's
   responsibility per the packet's §11.
5. **"Binds workspace" with no local workspace concept in Part One.**
   Unresolved as an ADR-0006 amendment, though the packet's own §14 records
   the human's direction on this node's scope: drop the `workspace` field
   entirely from `MutationAcceptanceRequest`/`compute_request_digest` for
   Part One (no placeholder). This crate implements that direction — see
   `crates/loki-mutation-contract/src/request.rs`'s module doc comment
   ("# No `workspace` field (Part One)") and `digest.rs`'s top doc comment.
   The struct also has no `scopes` field, because the packet's own §6
   deliverable field list for `MutationAcceptanceRequest` does not name one
   (unlike ADR-0006's decision text, which names both "workspace" and
   "scopes") — this worker followed the packet's explicit field list
   literally rather than independently reconciling it against the ADR text,
   and flags that literal reading here rather than silently picking either
   interpretation. What remains genuinely unresolved: the short ADR-0006
   "Decision" step 2 amendment the packet's §14 says "should be reflected...
   before `P1-ACCEPTANCE-STORE-001` treats it as settled" has not been
   written — ADR-0006's decision text still says "bind workspace." Blocks:
   `P1-ACCEPTANCE-STORE-001` (per packet §14, that node should not proceed
   treating this as settled without the ADR amendment landing first).

Additional assumption made by this worker (not one of the five named open
questions, but worth recording): `MutationAcceptanceRequest::new` always
*derives* `request_digest` internally via `compute_request_digest` rather
than accepting it as an independently caller-supplied field, even though the
packet's §6 prose lists "full canonical request digest" as one of the
envelope's bound fields alongside the others. This was a deliberate design
choice to make "the digest always matches its own bound fields" a
construction-time guarantee rather than a runtime invariant a caller could
violate; it does not change the field's presence or shape in the public
struct (`request_digest: RequestDigest` is still a public field, just never
independently settable at construction).

## 8. Review focus

- Whether `MutationRejection` is the right closed set for
  `P1-ACCEPTANCE-STORE-001`'s real transaction — in particular whether
  `GrantInvalid { reason: String }` needs to become more structured once a
  real grant-generation lookup exists (currently unused by any pure function
  in this crate; only `GrantExpired`/`GrantRevoked` are actually produced by
  `evaluate_grant`).
- Whether `construct_accepted`'s bound-digest equality check on
  `ApprovalReference` is the correct enforcement point, or whether
  `P1-ACCEPTANCE-STORE-001` needs an additional pure function here once its
  real approval-consumption call shape is known (this crate cannot predict
  that shape without inventing one, so it stopped at the narrowest
  type-level guarantee the packet's acceptance criterion names).
- Whether dropping `scopes` (not just `workspace`) from
  `MutationAcceptanceRequest` — a reading of the packet's §6 field list the
  ADR text itself does not fully support — needs an explicit orchestrator
  ruling before `P1-ACCEPTANCE-STORE-001` starts, alongside the already-
  flagged workspace omission.
- Confirm the `sha2`-based digest construction in `digest.rs` really is
  order-stable/collision-resistant against the exact field set
  `P1-ACCEPTANCE-STORE-001` ends up needing (this node fixed six fields;
  a seventh added later is a breaking change to `compute_request_digest`'s
  signature, not an additive one, given no serialized/versioned digest
  format exists here beyond the `"lokiai-mutation-v1"` prefix byte string).

## 9. Follow-up graph changes proposed

- Orchestrator should route Section 9 items 4 and 5 (transaction-topology
  feasibility; workspace/scopes-binding tension with ADR-0002) to the human
  product/security owner before dispatching `P1-ACCEPTANCE-STORE-001`, per
  the packet's own §11 instruction — this was not this worker's action to
  take.
- A short ADR-0006 amendment (packet §14) replacing "bind workspace" in
  "Decision" step 2 with language matching the approved Part One deferral
  should land before `P1-ACCEPTANCE-STORE-001` treats the workspace/scopes
  omission as settled.
- Consider filing `Docs/graph/evidence/W6/P1-MUTATION-CONTRACT-001/
  planning-report.md` (named in the packet's §15 as not yet done) so this
  node's planning provenance does not depend on chat-transcript retention.

Only the orchestrator updates the task graph.
