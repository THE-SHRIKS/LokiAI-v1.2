# Worker Handoff — `P1-MUTATION-CONTRACT-001` (attempt 2)

Result: `COMPLETE`
Worker: `loki-implementer` (Claude Code, one bounded node, security-review remediation pass)
Started from: working tree as left by attempt 1 (`Docs/graph/handoffs/P1-MUTATION-CONTRACT-001-1.md`), itself on top of `7a1115f5b291c71c4ffb0e13deee8d1af9654268`
Result revision/patch: uncommitted working tree; only `crates/loki-mutation-contract/src/{request.rs,result.rs,lib.rs}` changed relative to attempt 1
Completed at: `2026-08-25T00:00:00Z`

## 1. Outcome

Both Medium findings from the independent security review of attempt 1 are
fixed, scoped entirely inside `crates/loki-mutation-contract/src/**` — no
other crate, no root `Cargo.toml`, no task/graph document was touched by this
attempt. Finding 1 (`AcceptanceResult::Accepted`/`ReplayedIdenticalAccepted`
structurally bypassable via struct-literal construction) is closed by
introducing `AcceptedRecord` (private fields, private constructor, public
accessors only) and changing both variants to carry it instead of three
loose public fields — the only way to obtain an `AcceptedRecord` outside
`result.rs` is now `construct_accepted` or `replayed_result`. Finding 2
(approval expiry never validated; `MutationRejection::ApprovalNotConsumable`
unreachable) is closed by adding `evaluate_approval_reference` (analogous to
`grant.rs`'s `evaluate_grant`) and having `construct_accepted` call it,
routing a present-but-mismatched-or-expired `ApprovalReference` through
`Ok(AcceptanceResult::Rejected(MutationRejection::ApprovalNotConsumable))`
instead of a raw `MutationContractError::InvalidInput`, while a genuinely
*missing* `ApprovalReference` on an approval-gated method remains a
construction-time `InvalidInput` (caller/usage error, not a runtime
rejection). `cargo build`, `cargo fmt -- --check`, `cargo clippy -D
warnings`, and `cargo test --lib` all pass for `-p loki-mutation-contract`
(29/29 tests, up from 23; every new test is named and asserted below). The
five Section 9 open questions from the accepted packet remain honestly
recorded as unresolved — see §7; neither fix touches or silently resolves
any of them.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Finding 1 fixed: `Accepted`/`ReplayedIdenticalAccepted` no longer constructible via bare struct-literal bypass | pass | `crates/loki-mutation-contract/src/result.rs`: `AcceptedRecord` has private fields and a private (module-scoped) `new`; `AcceptanceResult::Accepted(AcceptedRecord)` / `AcceptanceResult::ReplayedIdenticalAccepted(AcceptedRecord)`; only `job_id()`/`correlation_id()`/`accepted_at_ms()` accessors are public. Grep-verifiable: no `pub fn new` on `AcceptedRecord`, no public fields. |
| Finding 2 fixed: approval expiry validated; `ApprovalNotConsumable` reachable | pass | `crates/loki-mutation-contract/src/request.rs`: `evaluate_approval_reference(reference, expected_digest, now_ms) -> Result<(), MutationRejection>`, checks digest mismatch and `now_ms >= expires_at_ms`, both routed to `ApprovalNotConsumable`. `construct_accepted` (`result.rs`) calls it and returns `Ok(AcceptanceResult::Rejected(...))` for a present reference that fails; a missing reference stays `Err(MutationContractError::InvalidInput)`. |
| New test: present, digest-matched, expired `ApprovalReference` rejected via `ApprovalNotConsumable` | pass | `result::tests::expired_but_digest_matched_approval_reference_is_rejected_as_approval_not_consumable_via_construct_accepted` |
| New test: `ApprovalNotConsumable` reachable from a real call path, not just isolated | pass | same test above plus `result::tests::mismatched_approval_digest_is_rejected_as_approval_not_consumable_via_construct_accepted` — both call `construct_accepted`, not `evaluate_approval_reference` directly |
| `cargo build -p loki-mutation-contract` succeeds, zero warnings | pass | §4 |
| `cargo fmt -p loki-mutation-contract -- --check` | pass (after one `cargo fmt` pass) | §4 |
| `cargo clippy -p loki-mutation-contract -- -D warnings` | pass | §4 |
| `cargo test -p loki-mutation-contract --lib` | pass, 29/29 | §4 |
| Scope confined to `crates/loki-mutation-contract/**` only | pass | §3; `git status --porcelain crates/loki-mutation-contract` shows only the untracked crate directory (still uncommitted from attempt 1 — nothing outside it changed); no edit to root `Cargo.toml`, `Cargo.lock`, or any `Docs/graph` task/graph file in this attempt |
| Five Section 9 open questions still recorded as unresolved | pass | §7 — explicitly re-confirmed none were touched by either fix |
| No prohibited security/architecture boundary weakened | pass | §6 — this attempt only *strengthens* two boundaries (construction-time bypass, expiry validation); nothing was loosened |
| No unrelated files change | pass | §3 |

## 3. Changed files (relative to attempt 1's state)

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-mutation-contract/src/result.rs` | Added `AcceptedRecord` (private fields/constructor, public accessors); changed `AcceptanceResult::Accepted`/`ReplayedIdenticalAccepted` to carry `AcceptedRecord`; `construct_accepted` now calls `evaluate_approval_reference` and returns `Ok(Rejected(ApprovalNotConsumable))` for a present-but-invalid reference instead of `Err(InvalidInput)`; `replayed_result` updated to build `AcceptedRecord`; updated/added unit tests | yes — within owned crate |
| `crates/loki-mutation-contract/src/request.rs` | Added `evaluate_approval_reference` pure function next to `ApprovalReference`, mirroring `grant.rs`'s `evaluate_grant` pattern; added unit tests for match/mismatch/expiry | yes — within owned crate |
| `crates/loki-mutation-contract/src/lib.rs` | Re-exported `AcceptedRecord` and `evaluate_approval_reference` from the crate root, consistent with existing re-export convention | yes — within owned crate |

No other file was touched by this attempt. `Docs/graph/tasks/README.md`,
`Docs/graph/TASK_GRAPH.md`, root `Cargo.toml`/`Cargo.lock`, and every other
file already present/modified in the working tree before this attempt
started were left exactly as attempt 1 left them.

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo build -p loki-mutation-contract` | Windows 11, workspace `rust-version = 1.97.1`, MSVC toolchain | exit 0, no warnings | `Compiling loki-mutation-contract v1.2.0 ... Finished` |
| `cargo fmt -p loki-mutation-contract -- --check` | same | exit 0 (one `cargo fmt` pass applied rustfmt's canonical formatting to two `if let`/one-line call-site reflows in `result.rs`, then check passed clean) | rerun `EXIT:0` |
| `cargo clippy -p loki-mutation-contract -- -D warnings` | same | exit 0, zero diagnostics | `Finished` |
| `cargo test -p loki-mutation-contract --lib` | same | 29/29 passed, 0 failed | full test name list below |
| `git status --porcelain` / scoped `git status --porcelain crates/loki-mutation-contract` | same | only the (still-untracked, since attempt 1) `crates/loki-mutation-contract/` directory; no root `Cargo.toml`/`Cargo.lock`/`Docs/graph` change from this attempt | command output reviewed directly |

Full passing test list (`cargo test -p loki-mutation-contract --lib`):

```
test error::tests::display_does_not_panic ... ok
test grant::tests::malformed_grant_generation_is_rejected ... ok
test digest::tests::digest_changes_with_grant_generation ... ok
test request::tests::approval_reference_with_matching_digest_and_unexpired_time_is_consumable ... ok
test digest::tests::idempotency_key_rejects_malformed_input ... ok
test grant::tests::revoked_grant_before_evaluation_time_denies_acceptance ... ok
test digest::tests::digest_distinguishes_field_boundaries_and_artifact_absence ... ok
test grant::tests::valid_grant_within_window_is_accepted ... ok
test method::tests::device_operation_methods_require_approval_and_agent_methods_do_not ... ok
test request::tests::approval_reference_may_be_bound_to_the_computed_digest ... ok
test method::tests::every_variant_round_trips_through_as_str_and_parse ... ok
test digest::tests::request_digest_try_from_str_validates_shape ... ok
test request::tests::approval_reference_with_mismatched_digest_is_not_consumable ... ok
test request::tests::expired_approval_reference_is_not_consumable_even_with_matching_digest ... ok
test grant::tests::expired_grant_denies_acceptance ... ok
test method::tests::unknown_mutation_method_string_is_not_constructible ... ok
test request::tests::request_digest_is_always_derived_not_caller_supplied ... ok
test result::tests::acceptance_result_variants_carry_no_contradictory_fields ... ok
test result::tests::accepted_record_exposes_fields_only_through_accessor_methods ... ok
test result::tests::agent_method_constructs_accepted_without_any_approval_reference ... ok
test result::tests::approval_required_method_cannot_construct_accepted_without_approval_reference ... ok
test result::tests::expired_but_digest_matched_approval_reference_is_rejected_as_approval_not_consumable_via_construct_accepted ... ok
test result::tests::mismatched_approval_digest_is_rejected_as_approval_not_consumable_via_construct_accepted ... ok
test result::tests::mutation_rejection_is_closed_with_exhaustive_match ... ok
test result::tests::same_key_different_digest_is_rejected_as_digest_conflict ... ok
test revocation::tests::post_commit_revocation_requires_reconciliation ... ok
test result::tests::same_key_same_digest_replays_original_accepted_result ... ok
test revocation::tests::pre_commit_revocation_denies_acceptance ... ok
test revocation::tests::revocation_disposition_is_closed_with_exhaustive_match ... ok

test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

6 new tests relative to attempt 1's 23: `request::tests::approval_reference_with_matching_digest_and_unexpired_time_is_consumable`,
`request::tests::approval_reference_with_mismatched_digest_is_not_consumable`,
`request::tests::expired_approval_reference_is_not_consumable_even_with_matching_digest`,
`result::tests::mismatched_approval_digest_is_rejected_as_approval_not_consumable_via_construct_accepted`,
`result::tests::expired_but_digest_matched_approval_reference_is_rejected_as_approval_not_consumable_via_construct_accepted`,
`result::tests::accepted_record_exposes_fields_only_through_accessor_methods`.
All 8 names required by the original packet's §8 table are still present and
passing (unchanged from attempt 1).

Environment note (not a defect, matches attempt 1's already-recorded
observation): the test binary link step emits a large number of pre-existing
`LNK4099` PDB-not-found warnings from `loki-storage`'s bundled OpenSSL
(`libopenssl_sys-*.rlib`). These are present for every crate in this
workspace that transitively links OpenSSL and are not introduced or affected
by this attempt's changes; they are linker-messages warnings, not compiler
warnings, and do not fail `cargo clippy -D warnings` or `cargo build`.

Mandatory verification not run: none.

## 5. Contracts, migrations, and compatibility

- Contracts changed: the crate's own internal Rust API shape changed within
  this attempt (relative to attempt 1, which was never accepted/merged, so
  there is no external consumer to break): `AcceptanceResult::Accepted`/
  `ReplayedIdenticalAccepted` now carry `AcceptedRecord` instead of three
  loose fields; `construct_accepted`'s behavior for a present-but-invalid
  `ApprovalReference` changed from `Err(MutationContractError::InvalidInput)`
  to `Ok(AcceptanceResult::Rejected(MutationRejection::ApprovalNotConsumable))`.
  Since attempt 1 was never accepted and no crate depends on
  `loki-mutation-contract` yet, this is not a breaking change to any real
  consumer — it is a correction before first acceptance.
- Consumers affected: none yet (unchanged from attempt 1 — `P1-ACCEPTANCE-
  STORE-001` is still the intended first consumer and has not started).
- Migration: none — no persisted schema.
- Backward/rolling compatibility: n/a — pre-acceptance revision.
- Generated-contract drift: n/a.
- Build recipe/executor/provenance compatibility: n/a.
- Cloud API/offline/notification compatibility: n/a directly; the still-open
  Section 9 item 5 tension is unchanged (see §7).

## 6. Security, privacy, and device safety

- Trust boundaries touched: none at runtime (still true — this crate is not
  wired into any process/IPC boundary). This attempt strengthens two
  compile-time/construction-time guarantees the crate's *shape* encodes for
  whatever future code calls into it:
  1. `AcceptanceResult::Accepted`/`ReplayedIdenticalAccepted` can no longer
     be produced by any code path other than `construct_accepted`/
     `replayed_result` inside this module — closing the struct-literal
     bypass a downstream crate could otherwise use to fabricate an
     "accepted" result without ever calling the approval/digest checks.
  2. `construct_accepted` now evaluates approval expiry, not just presence
     and digest match, before it will produce `Accepted` for an
     approval-gated method — closing the gap where an expired-but-present
     approval reference would have been accepted as valid.
- Authorization/approval impact: as above — this is a type-level/
  construction-time guarantee only; this crate still performs no I/O, does
  not itself consume a real approval, and does not read a real clock
  (`accepted_at_ms`/`now_ms` remain caller-supplied values, consistent with
  `evaluate_grant`'s existing `now_ms` parameter convention).
- Secrets/data handling: unchanged — none.
- Containment profile and denied authorities: n/a — unchanged.
- Device side effects performed: none.
- New dependencies: none added by this attempt.
- Remaining risk: unchanged from attempt 1's assessment — this crate's
  standalone risk is low (no I/O, no callback surface); its downstream risk
  (an incorrect closed-set/enum shape becoming load-bearing for
  `P1-ACCEPTANCE-STORE-001` and later nodes) is unchanged by this attempt,
  which only fixes two structural gaps the reviewer found, it does not
  change the overall shape's scope.

## 7. Decisions and assumptions

Per the original packet's own acceptance criterion (still binding — this is
a remediation attempt on the same node, not a new packet), all five Section
9 "open questions — do not resolve silently" are re-confirmed here as
**still unresolved**. Neither Finding 1 nor Finding 2's fix touches any of
them; this is stated explicitly per this attempt's instructions rather than
left implicit:

1. **"Declared safe point" for cooperative post-commit cancellation.**
   Unresolved. Not touched by either fix — `revocation.rs` is unmodified in
   this attempt. Still blocks `P1-ACCEPTANCE-STORE-001`/`P1-MUTATION-REV-001`.
2. **`outcome_unknown` detection contract.** Unresolved. `revocation.rs`
   unmodified. Still blocks `P1-ACCEPTANCE-STORE-001`/`P1-MUTATION-REV-001`.
3. **`GrantGeneration` vs. `LocalSession`.** Unresolved by design. `grant.rs`
   unmodified in this attempt (only `request.rs`/`result.rs`/`lib.rs`
   changed). Still blocks `P1-PRINCIPAL-GRANT-001`.
4. **Cross-crate single-transaction feasibility.** Unresolved as a
   feasibility question; only direction was previously recorded (packet
   §14). This attempt adds no `Connection`/`Transaction` type and does not
   touch the connection-topology question — `evaluate_approval_reference`
   and `AcceptedRecord` remain pure data/functions with no I/O, exactly like
   every other function in this crate. Still blocks `P1-ACCEPTANCE-STORE-001`
   pending the human/security owner routing named in the packet's §11.
5. **"Binds workspace" with no local workspace concept in Part One.**
   Unresolved as an ADR-0006 amendment. Neither fix adds, removes, or
   references a workspace/tenant field; `MutationAcceptanceRequest`'s shape
   (no `workspace`, no `scopes` field) is unchanged by this attempt. The
   `request.rs` module doc comment recording this tension is unmodified.
   Still blocks `P1-ACCEPTANCE-STORE-001` pending the ADR-0006 amendment
   named in packet §14.

One additional note specific to this attempt, not one of the five named
questions: `construct_accepted`'s `accepted_at_ms` parameter now does double
duty as the approval-expiry evaluation time (`now_ms` passed to
`evaluate_approval_reference`). This was a deliberate choice to avoid adding
a new parameter to `construct_accepted`'s public signature for this fix —
the acceptance transaction's own commit timestamp is the correct "now" for
evaluating whether an approval consumed *at that moment* has expired. This
is a design choice within this attempt's scope, not a new open question; it
is recorded here for reviewer visibility since it was not explicit in the
original packet's function-signature sketch for `construct_accepted`.

## 8. Review focus

- Whether `evaluate_approval_reference`'s two failure conditions (digest
  mismatch, `now_ms >= expires_at_ms`) collapsing to the single
  `ApprovalNotConsumable` variant (rather than distinguishing "wrong digest"
  from "expired" as separate rejection reasons) is acceptable for
  `P1-ACCEPTANCE-STORE-001`'s eventual caller — the packet's original §6
  field list names `ApprovalNotConsumable` as one closed rejection reason
  without sub-classification, so this attempt did not add one; flagging in
  case a reviewer wants more granular telemetry/audit-trail detail later
  (would be an additive enum-payload change, not a breaking one, if needed).
- Whether reusing `accepted_at_ms` as the approval-expiry evaluation time
  (see §7's design note) is the correct binding, versus a caller someday
  needing to evaluate approval consumability at a time distinct from the
  job's `accepted_at_ms` (this crate has no evidence such a case exists yet,
  but the coupling is worth an explicit sign-off).
- Confirm `AcceptedRecord`'s private-constructor pattern is what was
  intended by the finding (accessor-only public surface, `PartialEq`/`Eq`/
  `Clone`/`Debug` derived so equality-based tests still work without needing
  a public constructor) rather than some other mitigation shape (e.g. a
  sealed trait) the reviewer may have had in mind.

## 9. Follow-up graph changes proposed

- None beyond what attempt 1 already proposed (routing Section 9 items 4 and
  5 to the human/security owner before `P1-ACCEPTANCE-STORE-001` is
  dispatched; the still-unwritten ADR-0006 workspace-binding amendment).
  This attempt does not add a new follow-up.

Only the orchestrator updates the task graph.
