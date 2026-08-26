# Task Packet — `CON-SDK-IDENTITY-EVIDENCE-001 Typed Identity Evidence Channel`

Packet status: `accepted`
Graph status: `ready`
Created by: `orchestrator (Claude Code)`
Created at: `2026-08-26T00:00:00Z`
Starting revision: `5c58938`

## 1. Objective

Add a small, additive, backward-compatible way for a `Connector`'s `identify`
capability to report structured identity evidence
(`loki_connectors_sdk::identity::DeviceIdentityEvidenceInput`-shaped data —
`evidence_type`, `fingerprint_material`, `IdentityConfidence`) instead of only
a free-text `StageEvidence.summary`. This unblocks `CON-002-R1` (currently
`NEEDS_REPLAN` for exactly this reason — see
`Docs/graph/handoffs/CON-002-R1-1.md`), without breaking any existing
connector or consumer.

## 2. Traceability

- Parent: `CON-002-R1` (`NEEDS_REPLAN`), `CON-ID-CONTRACT-001` (accepted,
  integrated at `1a00a38`)
- Human approval: given in chat, 2026-08-26 — "Approve the small addition":
  an optional, backward-compatible field carrying structured identity
  evidence; existing connectors keep working unchanged.

## 3. Dependencies and inputs

- `depends_on`: `CON-ID-CONTRACT-001` (accepted, integrated)
- Consumed types: `loki_connectors_sdk::identity::{DeviceIdentityEvidenceInput,
  IdentityConfidence}` (read-only reference for shape — this node does not
  modify `identity.rs`)

## 4. Required reading

- `AGENTS.md`
- `crates/loki-connectors-sdk/src/outcome.rs` (`StageOutcome`/`StageEvidence`
  — what's being extended)
- `crates/loki-connectors-sdk/src/connector.rs` (the `Connector` trait —
  confirm no signature change is needed, only the evidence payload shape)
- `crates/loki-connectors-sdk/src/identity.rs` (the shape being referenced)
- `Docs/graph/handoffs/CON-002-R1-1.md` (the exact gap this closes)

## 5. Ownership

Writable paths:

- `crates/loki-connectors-sdk/src/outcome.rs`
- `crates/loki-connectors-sdk/src/lib.rs` (only if a re-export line is needed)
- `Docs/graph/handoffs/CON-SDK-IDENTITY-EVIDENCE-001-1.md`

Read-only: `crates/loki-connectors-sdk/src/identity.rs`,
`crates/loki-connector-registry/**` (do not touch — that's `CON-002-R1`'s job,
picking this up after).

Forbidden:

- Any change to the `Connector` trait's method signatures (this must be a
  payload-shape addition, not a breaking API change).
- Any change to `crates/loki-connectors-sdk/src/identity.rs` itself.
- Any change outside `loki-connectors-sdk`.

## 6. Deliverables

1. `StageEvidence` gets a new field, e.g.
   `identity_evidence: Option<DeviceIdentityEvidenceInput>` (or an equivalent
   small typed structure carrying `evidence_type`/`fingerprint_material`/
   `IdentityConfidence` — use your judgment on the exact shape given
   `identity.rs`'s real types, but it must be optional/`None`-by-default so
   every existing `StageEvidence` construction site in the whole workspace
   keeps compiling unchanged).
2. Confirm (and if necessary fix, but only mechanically — e.g. a struct-update
   `..Default::default()` if that pattern already exists, or explicit `None`
   at each site) that every existing `StageEvidence` construction site across
   the workspace still compiles with the new field, with no behavior change
   for any connector that doesn't set it.
3. Doc comment on the new field explaining its purpose and citing this
   packet + `CON-002-R1`.

## 7. Acceptance criteria

- [ ] `cargo build --workspace` succeeds with zero changes required to any
      connector that doesn't use the new field (backward compatible).
- [ ] `cargo test -p loki-connectors-sdk --all-targets` passes.
- [ ] `cargo test --workspace` passes (proves no other crate broke).
- [ ] `cargo clippy -p loki-connectors-sdk --all-targets -- -D warnings`
      passes.
- [ ] The new field is optional and defaults to absent/`None` — no existing
      caller is forced to populate it.
- [ ] No `Connector` trait method signature changed.
- [ ] No unrelated files change.

## 8. Required verification

| Layer | Command | Expected |
|---|---|---|
| Static | `cargo fmt -p loki-connectors-sdk -- --check` | exit 0 |
| Static | `cargo clippy -p loki-connectors-sdk --all-targets -- -D warnings` | exit 0 |
| Unit | `cargo test -p loki-connectors-sdk --all-targets` | all pass |
| Workspace | `cargo build --workspace` | exit 0, no other crate needs edits |
| Workspace | `cargo test --workspace` | all pass (retry once on the known flaky Windows Application Control error, don't retry indefinitely) |

## 9. Risks and safety

- Risk: `low` — additive, optional field, no trait signature change, no I/O,
  no security semantics change (this node does not itself validate or use the
  evidence — that remains `CON-002-R1`'s job).
- Rollback: revert the field addition; no persisted state.

## 10. Human gate

Already satisfied — see Section 2. No further approval needed for this
specific, scoped addition.

## 11. Worker and reviewer

- Worker: `loki-implementer`
- Reviewer: `loki-test-reviewer` (confirm genuinely backward-compatible,
  no other crate needed changes)
- Handoff: `Docs/graph/handoffs/CON-SDK-IDENTITY-EVIDENCE-001-1.md`

## 12. Stop conditions

Return `NEEDS_REPLAN` if achieving this requires changing the `Connector`
trait's method signatures, or if any existing connector/consumer would need
to change to keep compiling.
