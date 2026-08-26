# Verification evidence — `BLD-001-R2-CODE` security-review fix pass

Date: `2026-08-25`
Worktree: `L:\LokiAI-v1.2-wt\BLD-001-R2-CODE`
Branch: `codex/bld-001-r2-code`
Environment: `TEMP`/`TMP`/`CARGO_TARGET_DIR` under this worktree; standard, non-administrator user.

This evidence file records the fix pass for one High and one Medium finding
from an independent security review of `BLD-001-R2-CODE`, plus disposition of
a Low/hygiene finding. It supersedes nothing in `verification.md` /
`verification-rerun-2026-08-25.md`; those still apply to unrelated portions of
the crate.

## Findings addressed

1. **High — crash between journaled intent and native completion could be
   silently reported as success.** `CapsuleBroker::new` (`broker.rs`) now
   inspects `entries.last().intent`. If the last journal entry recorded an
   intent that was never followed by a completion/quarantine record (i.e. the
   process crashed between journaling intent and recording the native
   result), reconstruction forces `state = CapsuleState::Quarantined`
   regardless of `last.state`. Because `Quarantined` is a terminal state,
   `handle()`'s existing `is_terminal()` check (`broker.rs`, unchanged)
   rejects every subsequent request outright — the broker can never
   short-circuit to `Ok(complete)` without ever calling `apply()`/the native
   adapter again. This closes the gap for the three actions whose intent and
   complete states coincide (`AuthorizeExecution`, `QuarantineOutput`,
   `Reconcile`).
2. **Medium — `appcontainer_sid_digest` was not bound to the job identity
   invariant.** `CapsuleBroker` now carries `appcontainer_sid_digest: Option<DigestId>`
   alongside `job_id`/`capsule_id`/`base_digest`. `bind_or_verify` binds it on
   the first (`Create`) request and rejects any later request whose
   `appcontainer_sid_digest` differs, with `ErrorCode::IdentityMismatch`, the
   same as the other three identity fields. `JournalEntry`/`JournalEntryDraft`
   now persist `appcontainer_sid_digest`, it is part of the hash-chained
   `calculate_hash()` input, and `journal::validate_next` requires it to match
   the first entry's value across the whole chain (same treatment as
   `job_id`/`capsule_id`/`sealed_base_digest`).
3. **Low/hygiene — MAC-before-schema-validation ordering in
   `Authenticator::verify_and_consume`.** Evaluated and **not changed**. See
   "Decision: MAC-ordering finding not applied" below.

## Commands run (this pass)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy -p loki-build-executor --all-targets -- -D warnings` | exit 0, zero warnings |
| `cargo test -p loki-build-executor --lib -- --test-threads=1` | 11 passed, 0 failed |
| `cargo test -p loki-build-executor --test broker_contract -- --test-threads=1` | 12 passed, 0 failed (8 pre-existing + 4 new) |
| `cargo test -p loki-build-executor --test broker_dry_run -- --test-threads=1` | 2 passed, 0 failed |
| `cargo test -p loki-build-executor --test native_boundary -- --test-threads=1` | 1 passed, 0 failed |

Workspace-wide `cargo clippy --workspace --all-targets -- -D warnings` and
`pnpm run verify` were not rerun in this pass; they were already documented as
blocked by a pre-existing, unrelated `openssl-sys` Application Control policy
failure in `verification-rerun-2026-08-25.md`, reproduced against the
unmodified integration branch. Nothing in this fix pass touches that
dependency chain.

## New tests added (`tests/broker_contract.rs`)

- `crash_between_authorize_execution_intent_and_completion_quarantines_on_reconstruction`
- `crash_between_quarantine_output_intent_and_completion_quarantines_on_reconstruction`
- `crash_between_reconcile_intent_and_completion_quarantines_on_reconstruction`

  Each drives a real `CapsuleBroker`/`MemoryJournal`/`RecordingApi` through the
  legitimate prerequisite actions, then hand-appends one more `JournalEntry`
  with `intent: true` for the target action's intent state directly onto the
  journal (bypassing `handle`, modeling a crash after the intent write but
  before any completion/failure write). A **fresh** `CapsuleBroker` is then
  reconstructed from that journal with a **fresh, empty-ledger** `RecordingApi`.
  The test asserts the retried action (a) returns `Err` with
  `ErrorCode::Quarantined` rather than `Ok`, and (b) the fresh adapter's
  `operations` ledger remains empty — proving the native operation was neither
  assumed to have already succeeded nor blindly re-invoked.

- `appcontainer_sid_digest_substitution_is_rejected`

  Binds identity via `Create` with one `appcontainer_sid_digest`, then signs a
  `PrepareAcl` request with a different (but well-formed) digest and asserts
  `ErrorCode::IdentityMismatch`.

## Decision: MAC-ordering finding not applied

The packet's course-correction explicitly allowed skipping the Low/hygiene
MAC-ordering finding "if cheap and safe... use judgment, don't risk breaking
anything." Investigation found it is not cheap for this file: because the MAC
in `SignedEnvelope` covers the full canonicalized `request` payload, any
reordering that checks the MAC before `envelope.request.validate()` causes a
tampered-but-unresigned request (the exact technique the existing
`deserialization_cannot_bypass_identifier_or_digest_validation` test uses to
prove `BoundedId`/`DigestId` newtype validation catches values that bypass
serde's structural deserialization) to fail with `ErrorCode::Authentication`
instead of `ErrorCode::Malformed` — before ever reaching the schema-validation
code path the test exists to exercise. The underlying fail-closed security
property is unaffected either way (the malformed request is still rejected,
just for a different, arguably stronger, reason), but correctly fixing the
ordering requires re-authoring that acceptance test to authenticate a
still-malformed payload — not achievable using the crate's public API
`Authenticator::sign` (it calls `request.validate()` internally and
`BoundedId`/`DigestId` fields cannot be constructed off-contract by callers
outside the crate) without adding new test-only surface area. Given the
explicit "don't risk breaking anything for this one" instruction, this
finding is left as documented, deferred technical debt for a future
task packet rather than applied speculatively in this fix pass.
