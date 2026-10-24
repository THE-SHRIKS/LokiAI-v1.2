# Worker Handoff — `P1-RPC-CONTRACT-001`

Result: `NEEDS_REPLAN`
Worker: `/root/rpc_live_audit` (REV-004 architecture-boundary audit after REV-003 correction)
Started from: existing owned-path correction working tree based on `050e50188264996c22b2b4433d97a703a26796b3`
Result revision/patch: uncommitted owned-path working tree; intentionally not committed by worker
Completed at: `2026-08-24T18:06:37Z`

## 0. REV-004 disposition

REV-004 cannot be completed honestly inside this packet's accepted ownership. `BusinessService::call` is arbitrary in-process domain-service code and may capture database, job-engine or device handles. An IPC-owned flag, lease or resolver callback can revalidate before/after a callback, but cannot prevent that service from mutating before authorization or delaying/mutating after authorization. Holding a resolver lock across an external asynchronous device effect also cannot make credential revocation and the physical effect atomic. Such an API would remain cooperative rather than enforcing the requested adversarial boundary.

The accepted task packet makes runtime and domain-service implementations read-only. `Docs/ARCHITECTURE.md` assigns durable jobs, policy/approval validation, operation leases, persistence, artifact staging and execution to the runtime, and its durable operation pipeline places idempotent job persistence/approval before external execution. The enforceable correction therefore needs an authorized runtime-owned mutation-coordinator node: atomically revalidate the authority version, consume any approval, and idempotently persist/enqueue the durable job in the same runtime transaction; return only typed job/approval state over IPC; execute hardware later through durable checkpoints and defined safe-cancellation semantics. The replan must also define what revocation means after durable job acceptance because an external physical effect cannot generally be rolled back atomically.

No REV-004 implementation code was added. Findings 2-4 are contract-editable, but they overlap the result/read contract whose mutation boundary must first be accepted; changing them before that decision would risk codifying the wrong public contract. The passing REV-003 implementation and evidence below remain present in the uncommitted owned files.

## 1. Outcome

The TypeScript and Rust packages now share one explicit protocol-v1.1 business RPC vocabulary containing 23 named methods, 10 named bounded streams, closed projections, and per-credential server-resolved workspace/scope principals. The REV-002 correction dynamically revalidates the opaque credential identity before every call/subscription and every emitted business event, invalidates a session when its principal projection changes, and retains no presented plaintext credential in session state. Every unary method is authorized against an explicit workspace/resource scope set before producer dispatch; build requests require the referenced source snapshot, target and recipe scopes.

The REV-003 correction additionally binds every consequential result to the exact request method, resource scope set, idempotency key, artifact digest and denial reason, and requires approval grant/deny results to repeat the exact non-null request digest. Shared Rust/TypeScript negatives cover changed-request bindings and missing required nullable entity fields. Consequential services must cross a dynamically revalidated commit boundary immediately before side effects; the server rejects a successful response if the boundary was skipped and revalidates again before returning it. Per-connection unary/stream dispatch is bounded, duplicate active correlation IDs are rejected without replacing the original canceller, cancellation retains the reservation until producer teardown, and capacity recovers deterministically. Typed results and streams remain reachable over real Windows named pipes, with protocol-v1.0 forced core-only.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Named reads, diagnostics, cancellation, agent-run and work-intent methods | pass | Shared 23-request fixture; TS 313/313 and complete Rust method/result table pass |
| Distinct consequential requests return wire-reachable typed operation state contracts | pass | `MethodResult::Business`, `BusinessService`, typed client API and mismatch rejection |
| Desktop-only approval grant/deny | pass | Real named-pipe agent-denial test plus complete authority-table test |
| Named, bounded, correlated, cancellable, backpressured streams | pass | Real-pipe business stream, 10-stream domain matrix, emitter backpressure and summary coupling |
| Client metadata cannot create authority | pass | Same-server distinct desktop/agent credential projections; revocation/expiry tests |
| Current/previous/incompatible compatibility | pass | Hostile v1.0 business-advertisement is downgraded and denied; major rejection retained |
| Live credential revocation/expiry and principal replacement | pass | Real-pipe post-handshake call/subscription/reconnect tests plus active-stream invalidation; opaque credential IDs are unique and ambiguous IDs/tokens fail closed |
| Complete unary scope and result-identity coupling | pass | All 23 methods lose access before producer dispatch when a required scope is removed; all 23 have a coupled result; entity/operation mismatches fail closed |
| Consequential changed-request and approval-digest binding | pass | Shared positive cases plus 6 shared Rust/TS mutations for idempotency key, artifact digest, exact scopes, bound request digest and denial reason |
| Enforceable mid-flight authority/commit atomicity | needs replan | REV-003 tests prove a cooperative service stops at the boundary after revocation, expiry or principal replacement, but arbitrary service code can mutate before/after it; this is not the REV-004 adversarial guarantee |
| Bounded and unambiguous dispatch | pass | Real-pipe duplicate-correlation test preserves the original canceller; unary/stream capacity exhaustion returns a typed deterministic error and recovers after teardown |
| TS/Rust input and output parity | pass | Shared request/result/stream/event/principal fixtures, 3 shared strict unknown-field negatives and 3 shared missing-required-nullable-field negatives; ISO UTC, UTF-8 bytes, null and unique-set tests |
| Agent-run/work-intent durable mutation classification and binding | pending replan | REV-004 finding 2 intentionally not codified before the runtime mutation boundary is accepted |
| Exact operation outcome invariants | pending replan | REV-004 finding 3 intentionally not codified before the runtime result contract is accepted |
| Full read request/projection binding | pending replan | REV-004 finding 4 intentionally not codified before the boundary-aligned contract revision |
| No generic invoke or unrestricted execution surface | pass | TS and Rust negative tests; final diff inspection |
| No unrelated changes | pass | Final working-tree diff contains only packet-owned paths |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `packages/contracts/src/businessRpc.ts` | Runtime-validated TS request/result/principal/stream contract and semantic request/result matcher | yes |
| `packages/contracts/fixtures/business-rpc.json` | Canonical cross-language fixture for every request/result/stream/event/principal, exact operation identity fields, 6 strict-result negatives and 6 changed-binding negatives | yes |
| `packages/contracts/tests/businessRpc.test.ts` | Positive/negative contract, semantic request/result binding, shared strict-result, null, bounds, charset, authority and generic-invoke tests | yes |
| `crates/loki-ipc/src/business.rs` | Rust-aligned types, required-nullable presence decoding, strict projections, operation identity fields, semantic validators and comparable stream summary | yes |
| `crates/loki-ipc/src/auth.rs` | Opaque credential identity, dynamic revalidation, expiry/revocation, ambiguity denial and capability negotiation | yes |
| `crates/loki-ipc/src/protocol.rs` | Typed results/errors, per-method scope sets, exact consequential request/result identity coupling and validation | yes |
| `crates/loki-ipc/src/server.rs`, `client.rs` | Coherent session context, injectable producers, typed APIs, scoped authorization, explicit commit boundary, bounded dispatch and mismatch rejection | yes |
| `crates/loki-ipc/tests/business_authorization.rs`, `business_fixtures.rs` | Real-pipe revoke/expiry/scope/full-matrix/dispatch evidence and shared cross-language strict/coupling evidence | yes |

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `pnpm --filter @lokiai/contracts typecheck` | Windows, isolated worktree | pass | `tsc --noEmit`, exit 0 |
| `pnpm --filter @lokiai/contracts test` | Windows, unrestricted process spawn | pass | 9 files, 314 tests, 0 failed; business RPC 11/11 |
| `pnpm exec prettier --check packages/contracts/src/businessRpc.ts packages/contracts/tests/businessRpc.test.ts packages/contracts/fixtures/business-rpc.json` | Windows | pass | all matched files formatted |
| `cargo fmt --all -- --check` | Windows | pass | exit 0 |
| `cargo clippy -p loki-ipc --all-targets -- -D warnings` | Windows | pass | exit 0, no warnings |
| `cargo test -p loki-ipc` | Windows unrestricted, real named pipes | pass | 109 tests, 0 failed: 57 unit, 4 auth, 22 business authorization/wire, 9 fixtures, 4 deadline, 4 handshake, 6 malformed, 3 streaming |
| `git diff --check`, owned-path and secret-pattern inspection | isolated branch | pass | no whitespace errors, no suspicious production secret material, only packet-owned paths |

Mandatory verification not run:

- Complete repository verification is the orchestrator's post-integration gate, as required by the packet.

## 5. Contracts, migrations, and compatibility

- Contracts changed: local IPC minor version `1.0` to `1.1`; adds `loki-ipc.business.v1`, 23 methods, 10 scoped streams, typed results/events, workspace/resource principal scopes and exact operation `scope_ids` result coupling.
- Consumers affected: future Electron main, MCP and Rust service consumers; none implemented by this node.
- Migration: negotiate the business capability before using v1.1 methods; retain v1.0 core-only Ping/echo behavior.
- Backward/rolling compatibility: server intersects capability with the client's minor edge; hostile v1.0 business advertisement cannot negotiate or use business; Ping remains usable.
- Generated-contract drift: shared fixtures parse and validate in TS and Rust, including strict unknown-field negatives, principal, output, null/omission, charset, ISO UTC and UTF-8 byte parity.
- Build recipe/executor/provenance compatibility: request carries typed snapshot/target/recipe IDs only, and authorization requires each referenced resource scope; no executable/path surface.
- Cloud API/offline/notification compatibility: n/a; local IPC contract only.

## 6. Security, privacy, and device safety

- Trust boundaries touched: credential resolution, named-pipe encode/decode, producer output, client decoding, method and stream authorization.
- Authorization/approval impact: credential resolver binds each token to a distinct opaque credential identity and workspace/scopes/authorities principal; established sessions are revalidated dynamically; consequential services must reauthorize at their commit boundary; all 23 methods and all 10 stream domains have allow/deny coverage; approval decisions remain desktop-only and exact-digest-bound.
- Secrets/data handling: the presented authentication token exists only during Hello; established sessions retain an opaque generated credential ID. No secrets or customer/device payloads were added.
- Containment profile and denied authorities: agents are denied approval grant/deny even if misconfigured with decision authority; missing capabilities and authorities fail closed.
- Device side effects performed: none.
- New dependencies: none.
- Remaining risk/blocker: REV-003's `authorize_commit()` is cooperative and does not satisfy REV-004's enforceable atomicity requirement. Production composition needs an accepted runtime-owned mutation coordinator and transactional authority/approval/job boundary; the current API must not be treated as proof that arbitrary service code cannot mutate outside it. The safe defaults still have no business authority and return typed `UnsupportedMethod` rather than fake output.

## 7. Decisions and assumptions

- Semantic validation occurs before frame encode and immediately after decode for inputs and outputs; producer/client coupling adds method, stream, scope, sequence, summary and correlation checks.
- HelloAck principal comes only from per-credential server resolution; client type and payload fields cannot create or expand authority.
- A changed, revoked, expired, duplicated or ambiguously resolved credential/principal fails the established session closed instead of silently reducing or expanding authority.
- Protocol v1.0 is server-enforced core-only; business capability is legal only on the v1.1 edge.
- Active correlation IDs reserve one bounded per-connection slot through producer teardown, including after cancellation; duplicates never replace the original cancellation channel.
- Result schemas distinguish an absent field from a required field whose value is null for `device`, `incident`, `job`, `artifact_digest` and `request_reason`.

## 8. Review focus

- Inspect opaque credential-ID uniqueness/ambiguity handling, live session revalidation, and the unchanged-principal requirement.
- Inspect `required_method_scopes`, especially build snapshot/target/recipe binding, the 23-method missing-scope real-pipe matrix, and desktop-only approval guards.
- Inspect exact consequential request/result identity coupling, especially approval digest and artifact digest mutations, and shared missing-field/unknown-field fixtures.
- Treat the REV-003 commit-boundary tests as cooperative regression evidence only; review the required runtime-owned atomic mutation design after replanning.
- Inspect duplicate-correlation and capacity-exhaustion recovery, hostile v1.0 downgrade, and pre-encode/post-decode output validation.

## 9. Follow-up graph changes proposed

- No graph mutation by this worker. Re-plan a cross-owner runtime mutation-coordinator node covering authority-version revalidation, atomic approval consumption, idempotent durable job persistence/enqueue and post-acceptance revocation semantics. Keep runtime/Electron/MCP composition blocked; after that node is accepted, return this RPC contract for REV-004 findings 2-4 and boundary-aligned result/read schemas.
