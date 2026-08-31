# Task Packet — `P1-RPC-CONTRACT-001-R1` Corrected Business RPC Contract Over the Real RuntimeServices Boundary

Packet status: `proposed` (ready for orchestrator review/dispatch)
Graph status: `proposed`
Created by: `loki-planner` (read-only)
Created at: `2026-08-30T00:00:00Z`
Starting revision: current tip of branch `LokiAI-v1.2` (post `80bff05`) for the
target crate layout; salvage source is the unmerged branch
`codex/p1-rpc-contract-001` at `74c691e5af7a3332535dc612edbe0aae8e4a194a`.

## 1. Objective

Replace `P1-RPC-CONTRACT-001`'s IPC-owns-mutation-atomicity design with a
corrected authenticated, versioned business RPC contract that sits strictly
ON TOP of the now-real `RuntimeServices` internal service boundary
(`crates/loki-runtime/src/services/**`, accepted as of
`P1-SERVICE-READINESS-001-R1`). This node salvages the safe bounded-transport
work already reviewed twice (frame codec, handshake, deadlines, streaming
backpressure, closed 23-method typed vocabulary, `BusinessService` producer
seam) from `codex/p1-rpc-contract-001`, and replaces every mutation/read
result type with one that is structurally derived from — never a
reinterpretation of — `loki_mutation_contract::AcceptanceResult`/
`MutationRejection` and `loki_runtime::services::{RuntimeStatus, DevicePage,
DeviceHealthProjection}`. It also closes the three still-open REVIEW-002 HIGH
findings (dynamic session revalidation, per-method target/entity-identity
result coupling, exact Rust/TS output parity) and the two HIGH findings from
REVIEW-001 not yet independently re-confirmed at this revision (per-resource
stream authority matrix, hostile-downgrade capability gating).

This node does not compose `loki-ipc` into the actual runtime process
(`P1-RUNTIME-COMP-001`'s scope) and does not implement a real
`BusinessService` backed by a live `RuntimeServices` instance — it proves the
contract end-to-end against a test double that reproduces `RuntimeServices`'
real return-type shapes exactly.

## 2. Traceability

- Requirements/architecture: `CTR-001`, `RUN-002`, `ADR-0003` (as named in
  `TASK_GRAPH.md`'s row), plus `ADR-0006` (transactional runtime mutation
  acceptance — the corrected design this node must conform to) and its
  Amendment 1 (no workspace/scopes field).
- Security: `Docs/SECURITY.md` IPC/local-client-impersonation section;
  `AGENTS.md` locked boundary — "Desktop-to-runtime communication uses an
  authenticated, versioned local IPC contract... No generic `invoke`, raw
  command, or arbitrary path bridge is allowed."
- Parent/supersedes: `P1-RPC-CONTRACT-001` (`NEEDS_REPLAN` at REV-004/ADR-0006).
- Review history consumed: `Docs/graph/evidence/P1/P1-RPC-CONTRACT-001/REVIEW-001.md`,
  `REVIEW-002.md`.

## 3. Dependencies and inputs

- `depends_on`: `P1-SERVICE-READINESS-001-R1` — substantively satisfied at
  code level per this packet's cover report (§1 above); the orchestrator
  should confirm/record its graph status before dispatch. If its own pending
  independent review (handoff §8/§11, "ACCEPTED-pending-review") later
  surfaces a finding that changes `RuntimeReadPort`/`MutationSubmissionPort`'s
  public shape, that finding routes back through this node before its own
  acceptance is treated as final.
- Consumed contracts/exact revisions:
  - `loki_runtime::services::{RuntimeReadPort, MutationSubmissionPort,
    RuntimeStatus, DevicePage, DevicePageRequest, DeviceSummary,
    DeviceHealthProjection, DeviceHealthState, PersistedHealthEvidence,
    StorageReadiness, MutationAvailability, MutationSubmissionError}` —
    read-only dependency, current working-tree shape (`80bff05`).
  - `loki_mutation_contract::{AcceptanceResult, MutationRejection,
    AcceptedRecord, MutationMethod, MutationAcceptanceRequest}` — read-only
    dependency, accepted revision (no `Serialize`/`Deserialize` derive by
    design; this node must not add one to that crate).
  - Salvage source: `codex/p1-rpc-contract-001`@`74c691e5af7a3332535dc612edbe0aae8e4a194a`
    (`crates/loki-ipc/src/{business.rs,protocol.rs,server.rs,client.rs,auth.rs,frame.rs}`,
    `packages/contracts/{src/businessRpc.ts,fixtures/business-rpc.json,tests/businessRpc.test.ts}`).
    This branch is NOT merged into `LokiAI-v1.2` — confirmed by
    `git merge-base --is-ancestor 74c691e HEAD` returning false. The
    implementer must cherry-pick/rebase the salvageable files onto current
    `LokiAI-v1.2`, not assume they are already present.
- Required environment/credential/device: real Windows named pipes (existing
  `loki-ipc` test convention — `tests/{auth,deadline,handshake,
  malformed_frames,streaming}.rs`); no device/hardware.
- Known starting conditions (confirmed by direct inspection):
  - Current `LokiAI-v1.2`'s `crates/loki-ipc` has no `business.rs` module and
    no business methods — only the RUN-002 Ping/EchoStream baseline.
  - `crates/loki-ipc`'s own doc comment (`lib.rs`) currently states it
    depends on `loki-runtime` for nothing and vice versa. This node may add a
    read-only dependency from `loki-ipc` on `loki-mutation-contract` (a pure,
    already-accepted, I/O-free contract crate — safe, no cycle) to reuse its
    method/result vocabulary types for structural fidelity. This node must
    NOT add a dependency from `loki-ipc` on `loki-runtime` or
    `loki-acceptance-store` — that would let transport code reach for
    storage/acceptance logic directly, which is exactly the coupling
    `ADR-0006` forbids at this layer. The actual `BusinessService`
    implementation that calls into a live `RuntimeServices` is
    `P1-RUNTIME-COMP-001`'s job (an `INTEGRATION` node with a different,
    disjoint ownership set), not this node's.
  - `server.rs`'s existing `pub trait BusinessService: Send + Sync` (salvage
    branch) with its `UnsupportedBusinessService` fail-safe default is the
    correct seam for that later composition and should be preserved/reused,
    not redesigned, unless this node's own findings require a signature
    change (record any such change explicitly in the handoff).

## 4. Required reading

- `AGENTS.md`, `Docs/ARCHITECTURE.md`, `Docs/SECURITY.md` (IPC section),
  `Docs/TESTING.md`
- `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`
  (including Amendment 1)
- `Docs/graph/evidence/P1/P1-RPC-CONTRACT-001/REVIEW-001.md`, `REVIEW-002.md`
  (every named finding is this node's required-correction checklist)
- `crates/loki-runtime/src/services/{mod.rs,read.rs,mutation.rs}` (consume
  exactly as designed — do not edit)
- `crates/loki-mutation-contract/src/{result.rs,request.rs,method.rs}`
  (consume exactly as designed — do not edit)
- Salvage source on `codex/p1-rpc-contract-001`:
  `crates/loki-ipc/src/{business.rs,protocol.rs,server.rs,client.rs,auth.rs}`,
  `packages/contracts/src/businessRpc.ts`
- Current `LokiAI-v1.2` baseline: `crates/loki-ipc/src/**` (all files),
  `packages/contracts/src/**` (all files)

## 5. Ownership

Writable paths:

- `crates/loki-ipc/src/business.rs` (replace with a corrected version —
  salvage the safe vocabulary/param types, rebuild every mutation/read
  result type per Deliverable 2 below).
- `crates/loki-ipc/src/{protocol.rs,server.rs,client.rs,auth.rs,frame.rs}` —
  narrow corrections only: version/capability downgrade gating (HIGH-4),
  per-credential principal resolution and dynamic session revalidation
  (HIGH-2 + REVIEW-002 finding 1), stream-to-authority matrix (HIGH-3). No
  unrelated refactor of the RUN-002 Ping/EchoStream proof-of-life path.
- `crates/loki-ipc/Cargo.toml` — add exactly one new dependency edge:
  `loki-mutation-contract` (path dep, `publish = false`, rationale comment
  matching existing convention). No other dependency change.
- `crates/loki-ipc/tests/{business_authorization.rs,business_fixtures.rs}`
  (new/replace) plus any new real-pipe adapter test file this node needs.
- `packages/contracts/src/businessRpc.ts`, `packages/contracts/fixtures/business-rpc.json`,
  `packages/contracts/tests/businessRpc.test.ts` (mirror every Rust wire-type
  correction; strict schema, `deny_unknown_fields`-equivalent parity).
- `Docs/graph/handoffs/P1-RPC-CONTRACT-001-R1-1.md` (own handoff only).

Read-only dependency paths:

- `crates/loki-runtime/src/services/**` (consume `RuntimeReadPort`/
  `MutationSubmissionPort`'s public types for structural mirroring in test
  doubles; do not edit; do not import `loki-runtime` from `loki-ipc`'s
  production code — only from this node's own test crate if genuinely
  needed, and only as a dev-dependency, never a production dependency).
- `crates/loki-mutation-contract/src/**` (consume; do not edit).
- `crates/loki-acceptance-store/src/**` (read for understanding
  `AcceptanceResult` semantics only; no dependency, no edit).

Forbidden scope:

- No re-implementation of atomic mutation-acceptance, approval-consumption,
  idempotency, or replay logic anywhere in `loki-ipc` or `packages/contracts`
  — every mutation result this node's wire types can represent must be a
  1:1 explicit mapping (`From`/`TryFrom`) of a value `loki-acceptance-store`'s
  `accept_mutation` already produced, never independently derived, guessed,
  or partially reconstructed at the transport layer. This is the exact
  boundary `ADR-0006`/REV-004 drew.
- No `workspace`/`tenant`/placeholder-scope field invented anywhere
  (`ADR-0006` Amendment 1 is settled — do not reopen it). Per-method
  target/entity-identity coupling (REVIEW-002 finding 2's remaining live
  scope) must bind to `device_id`/`job_id`/`incident_id`/`run_id`/
  `approval_id`/`work_intent_id` as applicable — never a workspace concept.
- No generic `invoke`, raw command string, arbitrary path, shell, serial,
  USB, database-handle, or environment-variable surface anywhere in the
  wire vocabulary or its handlers (`AGENTS.md` rule 13; locked architecture
  boundary). The method vocabulary stays closed and named — no new
  `Invoke(name, params: Value)`-shaped escape hatch.
- No dependency from `loki-ipc` production code on `loki-runtime` or
  `loki-acceptance-store` (see §3/§5 above) — that composition is
  `P1-RUNTIME-COMP-001`'s job, a separate `INTEGRATION` node.
- No edit to `crates/loki-runtime/src/services/**` or
  `crates/loki-mutation-contract/src/**` — if either's current public shape
  is insufficient for a clean wire mapping, that is `NEEDS_REPLAN`
  (a contract defect in an already-accepted crate), not a silent patch here.
- No edit to `crates/loki-acceptance-store/**`.
- No change to the RUN-002 Ping/EchoStream proof-of-life method/result
  shapes beyond what version/capability-gating corrections require.
- No IPC/MCP/Electron caller wiring (out of this node's scope; later nodes:
  `P1-NODE-IPC-CLIENT-001`, `P1-ELECTRON-RUNTIME-001`, `P1-MCP-RUNTIME-001`).

## 6. Deliverables

1. **Corrected `crates/loki-ipc/src/business.rs`**, salvaged from
   `codex/p1-rpc-contract-001`@`74c691e5` where safe (principal/authority
   vocabulary, bounded/`deny_unknown_fields` param types, read-result
   summary types), with:
   - `BusinessRpcResult`'s mutation variant(s) replaced: discriminated
     states with no contradictory nullable combinations, each variant
     mapped 1:1 (via explicit `From`/`TryFrom`, in a dedicated mapping
     module/functions this node owns) from
     `loki_mutation_contract::AcceptanceResult`'s four real variants
     (`Accepted`, `ApprovalRequired`, `ReplayedIdenticalAccepted`,
     `Rejected(MutationRejection)` — every `MutationRejection` variant
     surfaced distinctly on the wire, never collapsed to a generic
     `policy_reason: String`).
   - Read-result variants re-derived to structurally match
     `loki_runtime::services::{RuntimeStatus, DevicePage,
     DeviceHealthProjection}` field-for-field (never inventing a field
     `RuntimeServices` doesn't actually produce, e.g. no fabricated
     `hardware_identity`/`transport_reference` on the wire, matching
     `DeviceSummary`'s existing redaction).
2. **Fail-closed per-credential principal resolution + dynamic session
   revalidation** (closes HIGH-2 and REVIEW-002 finding 1): distinct
   desktop/agent credentials resolve to distinct scoped principals (no
   shared static `ServerConfig.principal`); every call/subscription
   re-checks current revocation/expiry state before dispatch, not only at
   `Hello` — no plaintext credential retained across the revalidation.
3. **Explicit stream-to-authority/scope matrix** (closes HIGH-3): every one
   of the 10 business streams requires its specific resource-read
   authority (telemetry/incident/job/approval/notification/work-intent),
   not a generic `Subscribe`.
4. **Server-enforced version/capability downgrade gating** (closes HIGH-4):
   a v1.0-only client cannot advertise/negotiate
   `loki-ipc.business.v1`; hostile-downgrade test proves it.
5. **Exact Rust/TypeScript output parity** (closes HIGH-5 and REVIEW-002
   finding 3): explicit UTF-8-byte string-bound semantics matching TS's
   JS-code-unit `z.string().max` exactly where they must align; validated
   ISO-UTC timestamps on both sides; Rust result/param types reject unknown
   top-level fields exactly as the TS strict schema does; malformed/
   oversized producer-output rejected on both sides with matching negative
   fixtures.
6. **Per-method target/entity-identity result coupling** (closes the
   non-workspace part of REVIEW-002 finding 2): a `DeviceGet`/`DeviceHealth`/
   `JobGet`/`IncidentGet`/etc. result is bound to and verifiably about the
   exact requested identity — request/result mismatch is a rejection, not a
   silently-returned unrelated record.
7. **Preserved `BusinessService` producer-injection seam** (`server.rs`),
   with its safe `UnsupportedBusinessService` default retained, so
   `P1-RUNTIME-COMP-001` has a stable point to install a real implementation
   backed by `RuntimeServices` later.
8. **Test double proving contract fidelity**: an in-crate (test-only)
   `BusinessService` implementation whose return values are constructed
   using the *same* `loki_mutation_contract::AcceptanceResult`/
   `MutationRejection` construction paths `loki-acceptance-store` itself
   uses (not hand-rolled fixtures that could drift from real shapes), so
   the wire-mapping tests exercise the real coupling, not an approximation.
9. **Real-pipe adapter/fixture tests** (Rust + TypeScript), not type-level
   contract tests only: all 23 unary variants, all 10 streams, business
   cancellation/deadline/backpressure, hostile downgrade, post-handshake
   revoke/expiry, cross-scope/cross-identity rejection, extra-field
   negative fixtures shared between Rust and TS.
10. Doc comments citing `ADR-0006` (by consequence/step) and this packet,
    matching existing citation convention (`crates/loki-ipc`'s existing
    `RUN-002`-citation style).
11. Handoff recording exactly which REVIEW-001/002 findings were closed,
    with evidence, and any newly discovered gap as still-open (not silently
    resolved).

## 7. Acceptance criteria

- [ ] `cargo build --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` succeed with zero warnings.
- [ ] `cargo test -p loki-ipc -- --test-threads=1` passes: real named-pipe tests for all 23 unary methods, all 10 streams, cancellation, deadlines, backpressure.
- [ ] `pnpm --filter @lokiai/contracts test` (or repo-equivalent) passes: strict TS schema fixtures for every business method/result, including negative extra-field fixtures shared with Rust.
- [ ] `pnpm run verify` passes (one retry allowed on the documented flaky Windows Application Control error, per existing project convention).
- [ ] Every mutation-result wire variant is produced only via an explicit, reviewable mapping function from a real `loki_mutation_contract::AcceptanceResult`/`MutationRejection` value — grep/review-verifiable that no wire code independently constructs an "accepted"/"approval required" outcome.
- [ ] No `workspace`/tenant/placeholder-scope field exists anywhere in the new/changed wire types (grep-verifiable).
- [ ] No generic `invoke`/arbitrary-command/path/shell/serial/USB/environment surface exists in the method vocabulary (grep-verifiable — closed, named enum only).
- [ ] `loki-ipc` has zero production (non-dev, non-test) dependency edge on `loki-runtime` or `loki-acceptance-store` (grep/`Cargo.toml`-verifiable).
- [ ] A hostile v1.0 client cannot negotiate/advertise `loki-ipc.business.v1` — reproduced by a real adversarial test, not a cooperative fixture.
- [ ] A privileged session's authority is re-checked after revocation/expiry for both a new call and an existing subscription — reproduced by a real post-handshake revoke/expiry test, not only a Hello-time test.
- [ ] Every one of the 10 business streams is denied to a principal lacking its specific resource-read authority (full allow/deny matrix), not just the generic `Subscribe` marker.
- [ ] Rust and TypeScript reject the same malformed/oversized/extra-field payloads (shared negative fixture set, both sides run against it).
- [ ] `git diff --stat` shows changes only under the writable paths named in Section 5.
- [ ] Independent test-reviewer and security-reviewer sign-off with no Critical/High finding (mirrors `P1-RPC-CONTRACT-REV-005`'s stated exit bar in `TASK_GRAPH.md`).
- [ ] No prohibited security or architecture boundary weakened (OS-ACL + nonce/token connection-level auth from RUN-002 remains intact and untouched in intent).

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `cargo fmt --all -- --check` | exit 0 | command output |
| Static | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | command output |
| Unit | `cargo test -p loki-ipc -- --test-threads=1` | all 23 unary + 10 stream methods pass on real named pipes | run summary + test names |
| Unit | `cargo test -p loki-mutation-contract -p loki-acceptance-store -- --test-threads=1` | unchanged, still green (sanity that this node did not touch them) | run summary |
| Contract | TS strict schema/fixture suite (`packages/contracts`) | 100% pass incl. shared negative fixtures | run summary |
| Contract | Rust/TS shared malformed/oversized/extra-field fixture set | both sides reject identically | fixture diff / test names |
| Adversarial | hostile v1.0-advertises-v1.1-business test | negotiation rejected | test name + assertion |
| Adversarial | post-handshake revoke/expiry mid-session (call + subscription) | call/stream denied after revalidation, not merely at next Hello | test name + timing log |
| Adversarial | cross-identity/cross-scope request (e.g. device A's session requesting device B's health) | rejected, not silently served | test name |
| Security | independent review: grep for any wire-side reconstruction of `Accepted`/`ApprovalRequired`/`ReplayedIdenticalAccepted`; confirm zero `loki-runtime`/`loki-acceptance-store` production dependency edge; confirm no generic invoke/path/shell surface | no finding | reviewer note in handoff |
| Desktop/Cloud/Hardware | n/a — no Electron/MCP/device wiring in this node | n/a | n/a |

## 9. Risks and safety

- Risk level: `critical` — this is the node that reintroduces the exact
  business-mutation wire surface a prior review chain found fundamentally
  unsafe twice; `ADR-0006`'s corrected boundary must be proven, not assumed.
- Failure modes: (a) reintroducing a generic/collapsed mutation-result shape
  that hides which real `AcceptanceResult` variant occurred; (b) a wire-side
  reconstruction of acceptance logic that can diverge from
  `loki-acceptance-store`'s real transaction outcome; (c) stale/cached
  authority surviving revocation; (d) capability-negotiation downgrade.
- Rollback/recovery: this node has no persisted state of its own — a failed
  gate returns to this node, no data migration involved.
- Data/secret impact: none beyond what RUN-002's existing token/nonce auth
  already handles; no new secret class.
- Device impact: none — no connector/device effect reachable from this
  layer at this revision (no live producer wired to `RuntimeServices` yet).

## 10. Human gate

- Gate: none required to *start* this node.
- Gate required before this node's acceptance is treated as final:
  independent test-reviewer and security-reviewer sign-off with no
  critical/high finding — mirrors the already-planned
  `P1-RPC-CONTRACT-REV-005` node in `TASK_GRAPH.md` and `ADR-0006`'s own
  "Required evidence": "Independent mutation-boundary and RPC reviews have
  no critical/high finding."
- Approver role: human product/security owner, on the independent review's
  findings.
- Approval evidence: independent review record under
  `Docs/graph/evidence/P1/P1-RPC-CONTRACT-001-R1/`.

## 11. Stop conditions

Return `BLOCKED_DEPENDENCY` if `P1-SERVICE-READINESS-001-R1`'s pending
independent review (handoff §8/§11) later finds a defect that changes
`RuntimeReadPort`/`MutationSubmissionPort`'s public shape before this node
starts. Return `NEEDS_REPLAN` if a clean 1:1 mapping from
`loki_mutation_contract::AcceptanceResult`/`MutationRejection` to a
non-contradictory wire result cannot be built without editing that crate —
that means the contract crate's shape is insufficient, which is a defect in
an already-accepted node, not something to patch around here. Return
`NEEDS_REPLAN` rather than adding a `loki-runtime`/`loki-acceptance-store`
production dependency to `loki-ipc` if the implementer finds no other way to
prove real coupling — that would mean the transport/service boundary itself
needs re-architecture, which is exactly the class of finding this node
exists to prevent from recurring silently.

## 12. Worker and reviewer

- Accountable worker: `loki-implementer` (one bounded node; writable set
  exactly as named in Section 5).
- Independent reviewer: `loki-security-reviewer` (mutation-result-mapping
  fidelity, principal/session-revalidation, and no-generic-invoke focus)
  and `loki-test-reviewer` (all 23 unary + 10 stream real-pipe adapter tests
  actually exercised, not merely named; adversarial matrix genuinely run).
- Worktree/branch: `node/P1-RPC-CONTRACT-001-R1`.
- Handoff record: `Docs/graph/handoffs/P1-RPC-CONTRACT-001-R1-1.md`.

## 13. Planning provenance

Shaped by `loki-planner` (read-only), 2026-08-30, against `LokiAI-v1.2`@`80bff05`
and salvage branch `codex/p1-rpc-contract-001`@`74c691e5af7a3332535dc612edbe0aae8e4a194a`.
Verified by direct inspection, not narrative alone: confirmed via
`git merge-base --is-ancestor` that the salvage branch is unmerged; read
`crates/loki-runtime/src/services/{mod.rs,read.rs,mutation.rs}` directly to
confirm `P1-SERVICE-READINESS-001-R1`'s real shape; read
`crates/loki-mutation-contract/src/result.rs` directly to confirm
`AcceptanceResult` has no serde derive (forcing an explicit mapping layer);
read `crates/loki-ipc/src/business.rs` on the salvage branch directly to
confirm the exact generic `BusinessRpcResult::Operation` shape that must be
replaced, and `server.rs`'s `BusinessService` trait seam that should be
preserved.
