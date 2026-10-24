# Worker Handoff — `CON-ID-CONTRACT-001`

Result: `COMPLETE`
Worker: `Claude Code implementer (this dispatch)`
Started from: `2d62bb26818fbe731553badb2a17d684be073006`
Result revision/patch: uncommitted owned worktree patch on `codex/con-id-contract-001`
Completed at: `2026-08-25T19:10:00Z`

## 0. Prior-session state finding (required before proceeding)

The worktree at `L:\LokiAI-v1.2-wt\CON-ID-CONTRACT-001` already contained
substantial uncommitted work from an interrupted prior "Codex" session
(confirmed via `git status`/`git log` before any edit). That prior session had
also left a stale handoff at this same path (previously: "Completed at
2026-08-24T18:41:34Z", result `FAILED_VERIFICATION`, worker "connector
identity contract implementer") claiming the mandatory full Rust workspace
build/test could not be completed due to build-lock contention with
concurrent agents, and recommending the orchestrator perform a mechanical
`Cargo.lock` refresh before acceptance.

I independently re-verified that prior session's code (read `identity.rs` in
full, the TS `connector.ts` additions, the shared fixture, the Cargo
manifest diff, and the `Cargo.lock` diff) rather than trusting its prose,
found the implementation sound and consistent with the accepted Connector
Identity Protocol v1.0.0 and `Docs/CONNECTOR_SPEC.md` Section 5, and then
**reran every required verification command myself from scratch**, including
the exact full Rust workspace command the prior session could not complete.
This handoff replaces the stale one with my own findings, evidence, and
timestamp.

One environment problem was worth fixing before verification: a stray
leftover build directory (`target-con-id-rev/`) and an npm compile cache
(`.tmp-review/`) from the interrupted session were present as untracked
clutter in the worktree and were failing `prettier --check .` (they are not
covered by `.prettierignore`'s literal `target` entry, which does not match
`target-con-id-rev`). I deleted both — they were build/cache artifacts, not
source changes — before running verification. My own working `target-con-id/`
directory used for this dispatch's builds was likewise deleted afterward and
is not part of the diff.

## 1. Outcome

Production, device-family-neutral connector identity contracts now exist in
both TypeScript (`packages/contracts/src/connector.ts`) and Rust
(`crates/loki-connectors-sdk/src/identity.rs`), matching field-for-field via a
shared fixture. A `TransportCandidate`/`SensitiveUnstableLocator` is always
marked `ephemeral`/`sensitive`/`unstable` and can never be validated as a
`PublicDeviceIdentity`. `DeviceIdentityEvidence` carries only a derived
digest — raw fingerprint material is consumed by an HMAC-SHA256 keyed with a
caller-supplied per-installation secret (never generated, stored, or loaded
by this crate) and then discarded; it never reaches the wire or a `Debug`
render. `IdentityProbeResult` only carries a `PublicDeviceIdentity` when
`status == verified`, and constructors/wire parsers reject empty evidence for
`unknown`, fewer than two evidence items for `ambiguous`/`conflicting`,
disagreeing fingerprints for `verified`, forged evidence digests, and unknown
extra fields (`deny_unknown_fields` in Rust, `.strict()` in Zod) — every
unknown/partial/ambiguous/conflicting/forged case fails closed. No ESP32 or
other device-family-specific type appears anywhere in the diff.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| No provisional transport field can be accepted as stable identity | pass | `candidate_id_stability: 'ephemeral'` literal-typed in TS/Rust; `PublicDeviceIdentitySchema` rejects a raw candidate object (test `never accepts a provisional candidate as stable identity`); Rust `SensitiveUnstableLocator` has no path into `PublicDeviceIdentity` |
| Every external field is runtime-validated and bounded in both languages | pass | Zod `.strict()` schemas with explicit bounds in `connector.ts`; Rust `deny_unknown_fields` wire structs + `non_empty_bounded`/`validate_slug`/`validate_version`/`validate_utc`/`validate_sha256` in `identity.rs` |
| Device-family-specific evidence remains opaque/typed behind the connector boundary | pass | `evidence_type` is a generic bounded slug, not an enum of concrete device concepts; no ESP32/vendor-specific field anywhere (`grep -i esp32` over the diff: no matches) |
| Sensitive raw identity is marked and cannot enter ordinary logs/projections | pass | `SensitiveUnstableLocator`'s `Debug` redacts (`sensitive_debug_is_redacted` test); `raw_material_handling: "consumed_not_persisted"`; raw `fingerprint_material`/`installation_key` are consumed by value in the constructor and never stored as a struct field |
| Unknown/partial/ambiguous/conflicting identity is explicit and fail closed | pass | `IdentityResolutionStatus` enum with constructor/superRefine invariants in both languages; Rust tests `unresolved_never_has_public_identity`, `conflicting_verified_evidence_fails_closed`; TS `it.each` tests for unknown/partial/ambiguous/conflicting |
| Cross-language fixtures prove exact serialized compatibility and rejection parity | pass | `packages/contracts/fixtures/connector-identity-v1.json` consumed byte-identically by TS (`IdentityProbeResultSchema.parse(fixture)).toEqual(fixture)`) and Rust (`shared_fixture_round_trips_exactly`, exact `serde_json::Value` equality); both languages independently reject a forged `fingerprint_digest`/extra field derived from the same fixture |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `packages/contracts/src/connector.ts` | Adds `SensitiveUnstableLocatorSchema`, `TransportCandidateSchema`, `DeviceIdentityEvidenceSchema`, `PublicDeviceIdentitySchema`, `IdentityProbeResultSchema` and supporting enums/patterns | yes |
| `packages/contracts/tests/connector.test.ts` | Boundary, fail-closed and fixture-parity tests for the identity contract | yes |
| `packages/contracts/fixtures/connector-identity-v1.json` (new) | Shared synthetic cross-language compatibility fixture; no real device data | yes |
| `crates/loki-connectors-sdk/Cargo.toml` | Adds pinned `serde`, `serde_json`, `sha2`, `hmac` with documented rationale/license | yes |
| `crates/loki-connectors-sdk/src/lib.rs` | Exports the new `identity` module | yes |
| `crates/loki-connectors-sdk/src/identity.rs` (new) | `TransportCandidate`, `SensitiveUnstableLocator`, `DeviceIdentityEvidence`, `PublicDeviceIdentity`, `IdentityProbeResult`, strict wire parser, HMAC-SHA256 derivation, unit tests | yes |
| `Cargo.lock` | Adds `hmac`, `subtle` and records the new `loki-connectors-sdk` dependency edges; mechanical, consistent with the `Cargo.toml` change | yes — verified `cargo test --workspace --locked` accepts it as-is; no separate lock refresh was needed |
| `Docs/graph/handoffs/CON-ID-CONTRACT-001-1.md` | This handoff, replacing the stale prior-session one | yes |

No file outside this list was modified. `crates/loki-connector-registry/**`
(owns `DeviceId`/`DeviceIdentity` today) was read only, to confirm the exact
current provisional-identity gap this contract is meant to close for the
successor node — not edited.

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `corepack pnpm --filter @lokiai/contracts run typecheck` | Node 24.19.0, corepack pnpm 11.21.0 | pass | exit 0 |
| `corepack pnpm --filter @lokiai/contracts run test` | Vitest 4.1.10 | pass | 313/313, incl. 33 in `connector.test.ts` |
| Full TS workspace `-r --if-present run typecheck` (`pnpm` shim was not on `PATH`; invoked corepack's cached `pnpm.cjs` directly) | 4 workspace projects | pass | `LokiAI Desktop UI`, `packages/contracts`, `services/mcp-server` all "Done" |
| Full TS workspace `-r --if-present run test` | same | pass | `packages/contracts` 313/313; `services/mcp-server` 85/85 |
| `pnpm run format:check` (via corepack) | Prettier 3.9.6 | pass | "All matched files use Prettier code style!" — only after deleting the prior session's stray `target-con-id-rev/`/`.tmp-review/` leftovers, which were not source changes |
| `pnpm run lint` (via corepack) | ESLint | pass | 0 errors; 4 pre-existing unrelated `LokiAI Desktop UI` warnings, untouched by this diff |
| `cargo clippy -p loki-connectors-sdk --all-targets --target-dir target-con-id -- -D warnings` | Rust 1.97.1 (rustc/cargo), isolated `L:` target dir | pass | exit 0, no warnings |
| `cargo test -p loki-connectors-sdk --target-dir target-con-id` | same | pass | 52 unit + 8 conformance + 1 intentionally-`#[ignore]`d doctest |
| `cargo test --workspace --target-dir target-con-id --locked` (mandatory full Rust workspace gate) | PowerShell, Strawberry Perl (`C:\Strawberry\perl\bin\perl.exe`) on `PATH` for the vendored OpenSSL build native to `loki-storage`'s SQLCipher chain | **pass** | Ran to completion in ~15 minutes total (12m01s compile + test run); every crate's unit/integration/doc tests passed, including `loki-connectors-sdk` (52+8), `loki-connector-registry` (26), `loki-ipc` (48 unit + several integration suites), `loki-jobs`, `loki-monitoring` (75 unit + several integration suites), `loki-storage` (SQLCipher fault-injection/backup/restart suites — the `sqlcipher_page_cipher: hmac check failed` stderr lines are expected negative-test output, not failures), `loki-runtime`, `loki-incidents`, `loki-agents`, `loki-approvals`, `loki-local-*`, `loki-agent-delivery`; final process exit code `0` |
| `cargo clippy --workspace --all-targets --target-dir target-con-id -- -D warnings` (extra, run to confirm the dependency/lock change introduced no workspace-wide warning, beyond this node's own owned surface) | same | pass | "Finished `dev` profile" with no warnings across all 17 workspace crates |
| Manual diff/scope scan (`git status --short`, `git diff --stat`, `grep -i esp32` over the diff, secret/credential keyword scan) | worktree | pass | only the 8 owned files above changed; no ESP32-specific code; no real COM port/USB path/credential value in the diff (only doc-comment prose referencing the *concept* of a per-installation secret) |

Mandatory verification not run: none. Independent contract review (a separate
reviewer role per the packet) has not happened yet — that is out of this
worker's scope, not a gap in what I verified.

**Directly resolving the prior session's blocking claim:** the previous
session's build-lock-contention failure on the mandatory full Rust workspace
gate did not reproduce for me. I ran `cargo test --workspace --target-dir
target-con-id --locked` myself, end to end, and it completed with exit code
`0` and zero test failures. No `Cargo.lock` refresh was necessary —
`--locked` accepted the already-present `Cargo.lock` diff as consistent with
the manifest change.

## 5. Contracts, migrations, and compatibility

- Contracts changed: new connector identity schema `1.0.0` (additive; no existing exported symbol was changed or removed).
- Consumers affected: `crates/loki-connector-registry` (currently manufactures `DeviceId` directly from a candidate locator — confirmed by reading `discovery.rs`, read-only, not touched here), future concrete connectors, the runtime.
- Migration: `CON-002-R1` must replace that registry locator-as-identity path with this contract's `IdentityProbeResult`/`PublicDeviceIdentity`.
- Backward/rolling compatibility: purely additive; nothing existing depends on the new exports yet.
- Generated-contract drift: none — the shared fixture is hand-checked-in and round-trips exactly in both languages, proven by the tests above.
- Build recipe/executor/provenance compatibility: n/a.
- Cloud API/offline/notification compatibility: n/a; `PublicDeviceIdentity` is deliberately safe to project later (digest only), raw evidence is not.

## 6. Security, privacy, and device safety

- Trust boundaries touched: connector-SDK-to-consumer contract only (TS/Rust). No IPC, MCP, or renderer surface touched.
- Authorization/approval impact: none; no device operation is exposed or authorized by this packet.
- Secrets/data handling: raw fingerprint material and the installation key are consumed and never stored as struct fields or serialized; `SensitiveUnstableLocator::Debug` is redacted; the fixture is explicitly synthetic (`fixture-locator-not-a-real-device`).
- Containment profile and denied authorities: no shell/path/URL/serial authority added; this is a pure data-contract crate.
- Device side effects performed: none. No COM port was opened, no reset/flash/configuration occurred. (COM5's presence was previously established read-only by `CON-003-PRE-001`; this dispatch did not touch any device or port.)
- New dependencies: `serde 1.0.229`, `serde_json 1.0.151`, `sha2 0.10.9` (already present elsewhere in the workspace lock graph), and `hmac 0.12.1` (new; RustCrypto, MIT OR Apache-2.0, same family/maintainer as `sha2`, pulls in only `subtle 2.6.1` transitively) — each justified in `Cargo.toml` per `AGENTS.md` rule 11.
- Remaining risk: none blocking. Independent contract review (separate reviewer role) and the successor `CON-002-R1` registry migration remain open follow-up work, not defects in this contract.

## 7. Decisions and assumptions

- Stable public identity is `dev1_` plus an **installation-scoped HMAC-SHA256** digest (not a bare unkeyed hash) of connector-supplied device-derived material, generic evidence type, and a caller-supplied per-installation secret — per the human-approved Connector Identity Protocol v1.0.0 (task packet Section 10) and `Docs/ARCHITECTURE.md`'s per-installation secret. This crate never generates, stores, or loads that secret; it only accepts it as opaque keying material (minimum 32 bytes, enforced).
- Connector/version and observed time are evidence provenance, not identity inputs, so a connector upgrade or re-observation does not rotate a device's public ID.
- External wire evidence carries only the derived digest and an explicit `consumed_not_persisted` marker, never raw device material.
- The human gate required by the task packet ("I approve the new Connector Identity Protocol v1.0.0 …") is recorded as accepted in `Docs/graph/TASK_GRAPH.md` (status-update block for `CON-ID-CONTRACT-001`, timestamp `2026-08-24T22:52:06+05:30`, actor "human product/security owner; Codex orchestrator"), with binding conditions (no C-drive/account/admin/host-policy changes; no serial open/reset/flash/configuration under this node) that this dispatch respected — no device or host-privilege operation was performed.

## 8. Review focus

- Confirm `evidence_type` belonging in the identity-hash domain is correct for every planned connector family, and that the "all agreeing evidence reconciles" rule in `IdentityProbeResult::verified` is the intended reconciliation policy.
- Confirm every `crates/loki-connector-registry` call site that currently does `DeviceId::new(candidate.as_str())` (see `discovery.rs`, in the `identify`/discovery-cycle path) is replaced, not merely supplemented, in `CON-002-R1`.
- Note: the prior interrupted session's own handoff prose described the design as a plain "domain-separated SHA-256" digest — that summary text was stale/inaccurate. The actual code I inspected and reran tests against already implements the correct installation-scoped HMAC-SHA256 design; only its own prose was wrong, not the implementation.

## 9. Follow-up graph changes proposed

- Proceed with independent `CON-ID-CONTRACT-001` review, then unblock `CON-002-R1` to replace the registry's locator-as-identity path with this contract.
- No lockfile-refresh or re-run follow-up is needed from the orchestrator — the full workspace `--locked` gate already passed in this dispatch.
