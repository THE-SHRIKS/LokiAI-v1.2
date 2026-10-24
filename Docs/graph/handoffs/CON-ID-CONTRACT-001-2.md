# Worker Handoff — `CON-ID-CONTRACT-001`

Result: `COMPLETE`
Worker: `connector contract worker (security-review remediation pass)`
Started from: implementation state on branch `codex/con-id-contract-001`, on top of `Docs/graph/handoffs/CON-ID-CONTRACT-001-1.md`
Result revision/patch: working tree of `L:\LokiAI-v1.2-wt\CON-ID-CONTRACT-001` (branch `codex/con-id-contract-001`), uncommitted
Completed at: `2026-08-25T00:00:00Z`

## 1. Outcome

Fixed the independent security review's High finding: `SensitiveUnstableLocator`'s
derived `Serialize` (and therefore `TransportCandidate`'s and
`IdentityProbeResult`'s, which embed it) emitted the raw transport locator
verbatim on ordinary JSON serialization, contradicting the packet's own
acceptance criterion ("Sensitive raw identity is marked and cannot enter
ordinary logs/projections"). `SensitiveUnstableLocator` now has a hand-written
`Serialize` impl that redacts `value` to `"[REDACTED]"` by default; a new,
explicitly-named `serialize_for_transport()` method (added to
`SensitiveUnstableLocator`, `TransportCandidate`, and `IdentityProbeResult`)
is the one deliberate opt-in path that still carries the real locator, for the
one legitimate case — a connector genuinely needs the real port/USB
descriptor to open the device. The shared TypeScript contract gets an
equivalent opt-in: `redactSensitiveLocator` / `redactTransportCandidate` /
`redactIdentityProbeResult` in `packages/contracts/src/connector.ts`, since
plain parsed JS objects have no serialization hook to redact automatically
the way Rust's `Serialize` trait does. The wire/fixture shape itself is
**unchanged** — `packages/contracts/fixtures/connector-identity-v1.json` still
represents the legitimate transport-carrying JSON a connector parses to open
a device; only the default *output* paths (Rust default `Serialize`, and a
new TS redaction helper for anything other than the parsed wire object
itself) now redact by default.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Sensitive raw identity is marked and cannot enter ordinary logs/projections | pass (was fail before this fix) | `identity.rs` `default_serialize_redacts_raw_locator` test; `connector.test.ts` `redactIdentityProbeResult ... never leaks it via JSON.stringify` test |
| All other CON-ID-CONTRACT-001 acceptance criteria (unchanged by this fix) | pass, unchanged | see `Docs/graph/handoffs/CON-ID-CONTRACT-001-1.md` history and prior implementation; no other contract shape, validation rule, or fail-closed behavior was touched |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `crates/loki-connectors-sdk/src/identity.rs` | Removed `Serialize` from `SensitiveUnstableLocator`'s derive list; added hand-written `Serialize` that redacts `value`. Added `serialize_for_transport()` to `SensitiveUnstableLocator`, `TransportCandidate`, `IdentityProbeResult` as the explicit unredacted wire path. Updated `shared_fixture_round_trips_exactly` test to use the new explicit transport path (renamed `shared_fixture_round_trips_exactly_via_explicit_transport_path`); added `default_serialize_redacts_raw_locator` and `explicit_transport_path_carries_real_locator` tests. | yes — packet §5 writable: `crates/loki-connectors-sdk/**` |
| `packages/contracts/src/connector.ts` | Added `redactSensitiveLocator`, `redactTransportCandidate`, `redactIdentityProbeResult` helpers (no schema/type changes; wire shape and all existing exports unchanged). | yes — packet §5 writable: `packages/contracts/src/connector.ts` |
| `packages/contracts/tests/connector.test.ts` | Added `describe('sensitive locator redaction', ...)` block covering: parsed wire data still carries the real locator (confirming the legitimate transport path is untouched); each redaction helper strips only `value`; `redactIdentityProbeResult` output never appears in `JSON.stringify`; and a control test proving the *unredacted* parsed object does leak the raw value via plain `JSON.stringify` (documenting exactly why the helpers exist). | yes — packet §5 writable: "focused contract fixtures/tests" |
| `packages/contracts/fixtures/connector-identity-v1.json` | Unchanged. Confirmed still correct: it is the legitimate wire/transport shape, not a log/projection surface. | n/a (no change) |

No changes outside the packet's writable paths. No change to registry, concrete connectors, or the root manifest.

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `corepack pnpm --filter @lokiai/contracts run typecheck` | worktree `L:\LokiAI-v1.2-wt\CON-ID-CONTRACT-001`, Node 24.19.0 via `C:\Program Files\nodejs` | exit 0, no type errors | direct run, this session |
| `corepack pnpm --filter @lokiai/contracts run test` | same | exit 0; 8 files, 318 tests passed (up from prior count; includes 5 new redaction tests) | direct run, this session |
| `cargo clippy -p loki-connectors-sdk --all-targets -- -D warnings` | same worktree, workspace toolchain | exit 0, zero warnings | direct run, this session |
| `cargo test -p loki-connectors-sdk` | same | exit 0; 54 unit tests + 8 conformance tests + doctest all passed (unit tests up from 51 to 54: 3 new identity tests, one renamed) | direct run, this session |

Mandatory verification not run:

- Independent contract review (this fix should go back through the same independent security review that raised the finding, per packet §11).
- Full workspace build/verification beyond `-p loki-connectors-sdk` and `@lokiai/contracts` (out of this fix's scope; no other crate/package touched).

## 5. Contracts, migrations, and compatibility

- Contracts changed: none. The wire/JSON shape validated by `IdentityProbeResultSchema`/`TransportCandidateSchema`/`SensitiveUnstableLocatorSchema` (TS) and `IdentityProbeResult::from_json_str` (Rust `ProbeWire`/`CandidateWire`/`LocatorWire`) is untouched; the fixture still round-trips byte-for-byte through the explicit transport path.
- Consumers affected: none yet exist downstream (`CON-002-R1` is the first dependent, not yet implemented). Its future implementation must call `serialize_for_transport()` (Rust) or pass the raw parsed object directly (TS, only for the one legitimate open-device use) rather than relying on default serialization, and must use the redaction helpers/`Debug` for any logging/evidence path.
- Migration: none required; this is a pre-acceptance fix to an unmerged node.
- Backward/rolling compatibility: n/a, node not yet accepted/released.
- Generated-contract drift: none; fixture unchanged.

## 6. Security, privacy, and device safety

- Trust boundaries touched: none new. This strictly tightens the existing sensitive-data boundary the packet already declared (raw locator is "sensitive"/"unstable").
- Authorization/approval impact: none.
- Secrets/data handling: the raw transport locator (COM port/USB descriptor/transport string) is now redacted by default in every serialization path except the one explicitly-named `serialize_for_transport()` opt-in (Rust) and outside the redaction helpers (TS). `Debug` was already redacted and remains so.
- Containment profile and denied authorities: unchanged; no device I/O in this crate.
- Device side effects performed: none.
- New dependencies: none.
- Remaining risk / open items flagged, not fixed here (per explicit instruction — recorded, not silently decided):
  - **Low** — `IdentityConfidence` (e.g. `Weak`) is never enforced as a minimum threshold before a `PublicDeviceIdentity` is produced by `IdentityProbeResult::verified`. This is a live open design question left for a future node (the packet does not specify a minimum-confidence policy, and inventing one here would be an undocumented product-behavior decision). Flagging for the orchestrator/next design pass.
  - **Informational, load-bearing for `CON-002-R1`** — this contract layer does not itself verify authenticity: `IdentityProbeResult::from_json_str`'s wire-parsing path validates *shape* (schema version, digest format, `fingerprint_digest == evidence_digest`, evidence-count/status invariants) but does not recompute the HMAC from raw material — it trusts the caller-supplied `fingerprint_digest`/`evidence_digest` as already-correct. Any component that constructs an `IdentityProbeResult` from untrusted wire data (rather than deriving it locally via `DeviceIdentityEvidence::new`) is trusting the sender, not independently verifying the HMAC. `CON-002-R1` (real candidate ingestion/identify-by-candidate) must be aware this crate is not a trust boundary by itself; it is a shape/invariant validator over evidence the connector process itself must have produced via `DeviceIdentityEvidence::new` with its own installation key.

## 7. Decisions and assumptions

- Interpreted "default/derived Serialize path must redact" as scoped to `SensitiveUnstableLocator` (and the types that embed it) rather than adding redaction to `DeviceIdentityEvidence`/`PublicDeviceIdentity`/status enums, because those types never carry raw sensitive material — only hashed digests and derived identifiers — so no other field needed redaction.
- Chose not to change the wire/fixture JSON shape: the review explicitly required "do not remove the legitimate transport path entirely, since the fixture/round-trip test needs SOME way to carry the real value across the wire once." The existing fixture already *is* that one legitimate transport-shaped JSON; the fix routes it through an explicitly-named method (`serialize_for_transport`) instead of the default `Serialize`, satisfying both constraints without altering the byte-level contract.
- On the TypeScript side, since parsed zod output is a plain object with no serialization-trait hook (unlike Rust's `Serialize`), implemented the "shared fix" as opt-in redaction helper functions rather than trying to force a "default redacted, explicit exposed" asymmetry that TypeScript's type system does not naturally support. Documented this rationale directly in `connector.ts` so a future consumer understands why the TS and Rust shapes of "default redacted" differ mechanically while being equivalent in intent.

## 8. Review focus

- Confirm `serialize_for_transport()` on `IdentityProbeResult`/`TransportCandidate`/`SensitiveUnstableLocator` is not reachable through any code path other than an explicit, intentional call (grep for accidental `#[serde(flatten)]`/blanket impls that could re-expose it — none currently exist).
- Confirm no other repository code (renderer, main process, future connector implementations) currently calls `serde_json::to_string`/`to_value` on these types expecting the *unredacted* shape by accident; none exist yet since `CON-002-R1` is unimplemented, but this should be re-checked when that node lands.
- Confirm the two flagged lower-severity items (identity-confidence threshold enforcement; no independent HMAC recomputation in the wire-parsing path) are carried forward into `CON-002-R1`'s required reading rather than silently assumed away.

## 9. Follow-up graph changes proposed

- None to the graph structure itself. Recommend the orchestrator re-run the independent security review against this specific diff before accepting `CON-ID-CONTRACT-001`, and ensure `CON-002-R1`'s task packet explicitly carries forward the two flagged open items in section 7 above (minimum identity-confidence threshold; no independent authenticity verification at this layer).
