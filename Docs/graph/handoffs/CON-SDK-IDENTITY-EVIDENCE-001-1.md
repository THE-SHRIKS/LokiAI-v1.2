# Worker Handoff — `CON-SDK-IDENTITY-EVIDENCE-001`

Result: `COMPLETE`
Worker: `loki-implementer`
Started from: `5c58938` (`LokiAI-v1.2`)
Result revision/patch: `L:\LokiAI-v1.2-wt\CON-SDK-IDENTITY-EVIDENCE-001` (branch `codex/con-sdk-identity-evidence-001`)
Completed at: `2026-08-26T18:55:00Z`

## 1. Outcome

`StageEvidence` (`crates/loki-connectors-sdk/src/outcome.rs`) now carries an
optional `identity_evidence: Option<DeviceIdentityEvidence>` field, populated
via a new `with_identity_evidence` builder method, mirroring the existing
`with_evidence_ref` pattern. `identity.rs` and the `Connector` trait
(`connector.rs`) were not touched — no trait method signature changed. The
field wraps `identity::DeviceIdentityEvidence`, the already-validated,
digest-only type `DeviceIdentityEvidence::new` produces (not the raw
`DeviceIdentityEvidenceInput`, which borrows a secret installation key and
raw fingerprint material and carries a lifetime parameter — see §7 for why
that type was rejected). `StageEvidence::new` initializes the field to `None`
and every existing construction site across the workspace uses `::new` (no
struct-literal construction site exists outside `outcome.rs` itself), so no
other file needed to change. `cargo build --workspace` and
`cargo test --workspace` both succeed unmodified elsewhere, confirming
backward compatibility.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `cargo build --workspace` succeeds with zero changes required to any connector that doesn't use the new field | `pass` | §4; only `crates/loki-connectors-sdk/src/outcome.rs` changed (`git diff --stat`), build succeeded |
| `cargo test -p loki-connectors-sdk --all-targets` passes | `pass` | §4; 54 unit + 8 conformance tests, 0 failed |
| `cargo test --workspace` passes | `pass` | §4; every crate's `test result: ok`, exit code `0` |
| `cargo clippy -p loki-connectors-sdk --all-targets -- -D warnings` passes | `pass` | §4; clean compile, zero warnings |
| New field optional, defaults to absent/`None` | `pass` | `StageEvidence::new` sets `identity_evidence: None`; field type is `Option<DeviceIdentityEvidence>` |
| No `Connector` trait method signature changed | `pass` | `connector.rs` untouched (`git diff --stat` shows only `outcome.rs`) |
| No unrelated files change | `pass` | `git diff --stat`: 1 file, 37 insertions, 0 deletions |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-connectors-sdk/src/outcome.rs` | Added `identity_evidence: Option<DeviceIdentityEvidence>` field to `StageEvidence`, a `with_identity_evidence` builder method, and a doc comment explaining purpose/rationale, citing this packet and `CON-002-R1` | yes — in writable-paths list |
| `Docs/graph/handoffs/CON-SDK-IDENTITY-EVIDENCE-001-1.md` | This handoff | yes — in writable-paths list |

`crates/loki-connectors-sdk/src/lib.rs` was not changed: `identity` was
already `pub mod identity;` and `DeviceIdentityEvidence` was already `pub`,
so no re-export line was needed.

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `cargo fmt -p loki-connectors-sdk -- --check` | worktree, git-bash | exit 0, no output | direct run, this session |
| `cargo clippy -p loki-connectors-sdk --all-targets -- -D warnings` | worktree, git-bash | exit 0, `Finished` with zero warnings | direct run, this session |
| `cargo test -p loki-connectors-sdk --all-targets` | worktree, git-bash | 54 unit tests + 8 conformance integration tests, all `ok`, 0 failed | direct run, this session |
| `cargo build --workspace` | worktree, PowerShell (needed for real `perl` on `PATH` — see §7) | `Finished dev profile ... in 7m 52s`, exit 0 | direct run, this session |
| `cargo test --workspace` | worktree, PowerShell | every crate's `test result: ok`, `EXITCODE:0` (captured explicitly via `$LASTEXITCODE`); `sqlcipher_page_cipher: hmac check failed` stderr lines are expected negative-test output (documented pattern, e.g. `Docs/graph/handoffs/CON-ID-CONTRACT-001-1.md`), not failures | direct run, this session, first attempt — no retry needed |

Mandatory verification not run: none — all five required-verification rows
from the task packet's Section 8 were run for real, on the first attempt.

## 5. Contracts, migrations, and compatibility

- Contracts changed: `StageEvidence` (public struct in `loki-connectors-sdk`)
  gains one new optional field and one new builder method. This is a
  strictly additive change to the type; any code constructing
  `StageEvidence` via `StageEvidence::new(...)` (the only public constructor,
  and the only pattern used anywhere in the workspace) is unaffected.
- Consumers affected: none required to change. `loki-connector-registry` and
  its tests, `lifecycle.rs`, and both `tests/support/mod.rs` fixture files
  all construct `StageEvidence` via `::new` and compile/pass unmodified.
- Migration: none — no persisted state, no wire format change (this field is
  in-process Rust only; no serialization derive exists on `StageEvidence`).
- Backward/rolling compatibility: confirmed via `cargo build --workspace`
  and `cargo test --workspace` both passing with zero other files touched.
- Generated-contract drift: none — `StageEvidence` is not part of any
  generated/schema-derived contract (`packages/contracts/**` untouched).

## 6. Security, privacy, and device safety

- Trust boundaries touched: none. This node adds a payload-shape hook only;
  it performs no validation, parsing, or use of the new field itself (that
  remains `CON-002-R1`'s job, per the packet's Section 9 risk statement).
- Authorization/approval impact: none.
- Secrets/data handling: the new field's type (`DeviceIdentityEvidence`)
  never carries raw fingerprint material or the installation secret — only
  an already-computed HMAC digest, per `identity.rs`'s own design
  (`raw_material_handling: "consumed_not_persisted"`). This node deliberately
  chose that type over the raw `DeviceIdentityEvidenceInput` for exactly this
  reason (see §7).
- Containment profile and denied authorities: unchanged.
- Device side effects performed: none.
- New dependencies: none — `identity.rs` was already a sibling module in the
  same crate; only an intra-crate `use` was added.
- Remaining risk: none introduced by this node. The packet's own risk
  assessment (`low`) is confirmed by the verification results.

## 7. Decisions and assumptions

- **Used `DeviceIdentityEvidence`, not `DeviceIdentityEvidenceInput`, as the
  field's type.** The packet's Deliverable 1 names
  `DeviceIdentityEvidenceInput`-shaped data (`evidence_type`,
  `fingerprint_material`, `IdentityConfidence`) as the target shape but
  explicitly says "use your judgment on the exact shape given `identity.rs`'s
  real types." `DeviceIdentityEvidenceInput<'a>` (a) borrows the caller's raw
  installation secret and raw fingerprint material — both of which
  `identity.rs`'s own doc comments say must never be persisted — and (b)
  carries a lifetime parameter, which would force a lifetime parameter onto
  `StageEvidence`, and transitively onto `StageOutcome`/`StageFailure`/
  `LifecycleOutcome`, a much larger and genuinely breaking ripple this
  additive node must not make. `DeviceIdentityEvidence` (the type
  `DeviceIdentityEvidence::new` returns after validating an `Input`) is
  owned, has no lifetime parameter, already derives `Debug`/`Clone`/
  `PartialEq`/`Eq`/`Serialize` (matching `StageEvidence`'s own derive list
  except `Serialize`, which `StageEvidence` does not currently have), and
  carries only the validated `evidence_type`, `fingerprint_digest`,
  `confidence`, `connector_id`, `connector_version`, and `observed_at` — the
  same semantic content the packet asked for, safely. This choice is also
  directly consistent with `CON-002-R1`'s own handoff
  (`Docs/graph/handoffs/CON-002-R1-1.md` §9, follow-up item), which floated
  "an already-hashed digest, if raw material must never leave the connector
  process even in memory" as an open design question — this node resolves
  that question in the safer direction by construction.
- A connector wishing to populate this field still calls
  `DeviceIdentityEvidence::new(DeviceIdentityEvidenceInput { .. })` itself
  (inside its own `identify`/`verify` implementation, where it has the
  installation key and raw material in scope) and passes only the resulting
  validated, digest-only value into
  `StageEvidence::with_identity_evidence`. No raw secret or raw fingerprint
  material ever needs to reach `outcome.rs`.
- No `serde` derive was added to `StageEvidence` or to this field — none
  existed before, and adding one was out of scope (would be a new public
  wire-contract decision, not a payload-shape addition).
- `identity.rs` and `crates/loki-connector-registry/**` were read-only, per
  the packet's ownership section; neither was modified.

## 8. Review focus

- Confirm `DeviceIdentityEvidence` (not `DeviceIdentityEvidenceInput`) was
  the correct type choice per §7's reasoning — this is the one place this
  node exercised judgment beyond the packet's literal suggested shape.
- Confirm the new `use crate::identity::DeviceIdentityEvidence;` import in
  `outcome.rs` does not create a circular or otherwise undesirable
  intra-crate module dependency (`identity.rs` does not import from
  `outcome.rs`, so this is one-directional).
- Confirm no other crate needed edits: `git diff --stat` from `5c58938`
  shows only `crates/loki-connectors-sdk/src/outcome.rs` changed.
- `CON-002-R1` is now unblocked to re-attempt Deliverables 2/3 (interpreting
  a connector's `identify` result into a real `IdentityProbeResult`) using
  this new field as the typed channel it was missing.

## 9. Follow-up graph changes proposed

- Re-issue/re-attempt `CON-002-R1` now that the typed identity-evidence
  channel exists in `loki-connectors-sdk`. It still owns Deliverables 2/3
  (interpreting `StageEvidence.identity_evidence` into a real
  `IdentityProbeResult`, and the reconciliation/minimum-confidence-threshold
  policy decision flagged in its own handoff §9 item 1) — this node
  deliberately did not decide that policy question, consistent with this
  packet's Section 9/§3 scope boundary (payload shape only, not usage
  policy).

Only the orchestrator updates the task graph.
