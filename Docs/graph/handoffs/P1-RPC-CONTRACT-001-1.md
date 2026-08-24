# Worker Handoff — `P1-RPC-CONTRACT-001`

Result: `COMPLETE`
Worker: `/root/p1_runtime_audit`
Started from: `29770b5e42c49ba65e9e3df9c60842af9a1edfb8`
Result revision/patch: `74c691e5af7a3332535dc612edbe0aae8e4a194a`
Completed at: `2026-08-24T16:24:43Z`

## 1. Outcome

The TypeScript and Rust packages now share one explicit protocol-v1.1 business RPC vocabulary containing 23 named methods, 10 named bounded streams, closed projections, and per-credential server-resolved workspace/scope principals. Typed unary results and business streams are reachable over the real wire through an injectable asynchronous producer, with method/result, stream/event/summary and correlation coupling enforced by server and client. Input and output are validated before encode and after decode; protocol v1.0 is forcibly core-only even if a hostile client advertises business capability.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Named reads, diagnostics, cancellation, agent-run and work-intent methods | pass | Shared 23-request fixture; TS 312/312 and complete Rust method/result table pass |
| Distinct consequential requests return wire-reachable typed operation state contracts | pass | `MethodResult::Business`, `BusinessService`, typed client API and mismatch rejection |
| Desktop-only approval grant/deny | pass | Real named-pipe agent-denial test plus complete authority-table test |
| Named, bounded, correlated, cancellable, backpressured streams | pass | Real-pipe business stream, 10-stream domain matrix, emitter backpressure and summary coupling |
| Client metadata cannot create authority | pass | Same-server distinct desktop/agent credential projections; revocation/expiry tests |
| Current/previous/incompatible compatibility | pass | Hostile v1.0 business-advertisement is downgraded and denied; major rejection retained |
| TS/Rust input and output parity | pass | Shared request/result/stream/event/principal fixtures; ISO UTC, UTF-8 bytes, null and unique-set tests |
| No generic invoke or unrestricted execution surface | pass | TS and Rust negative tests; final diff inspection |
| No unrelated changes | pass | Final staged diff contained only packet-owned paths |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `packages/contracts/src/businessRpc.ts`, `src/index.ts` | Runtime-validated TS request/result/principal/stream contract and export | yes |
| `packages/contracts/fixtures/business-rpc.json` | Canonical cross-language fixture for every request/result/stream/event and scoped principal | yes |
| `packages/contracts/tests/businessRpc.test.ts` | Positive/negative contract, null, bounds, charset, authority and generic-invoke tests | yes |
| `crates/loki-ipc/src/business.rs` | Rust-aligned business types, enums and semantic validators | yes |
| `crates/loki-ipc/src/auth.rs` | Per-credential resolver, expiry/revocation and minor-gated capability negotiation | yes |
| `crates/loki-ipc/src/protocol.rs`, `frame.rs`, `lib.rs` | Wire-reachable typed results, coupling and validation before encode/after decode | yes |
| `crates/loki-ipc/src/server.rs`, `client.rs` | Injectable producers, typed business APIs, scoped authorization and mismatch rejection | yes |
| `crates/loki-ipc/tests/business_authorization.rs`, `business_fixtures.rs` | Real-pipe security/compatibility and cross-language drift evidence | yes |

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `pnpm --filter @lokiai/contracts typecheck` | Windows, isolated worktree | pass | `tsc --noEmit`, exit 0 |
| `pnpm --filter @lokiai/contracts test` | Windows, unrestricted process spawn | pass | 9 files, 312 tests, 0 failed; business RPC 9/9 |
| `pnpm exec prettier --check packages/contracts/src/businessRpc.ts packages/contracts/tests/businessRpc.test.ts packages/contracts/fixtures/business-rpc.json` | Windows | pass | all matched files formatted |
| `cargo fmt --all -- --check` | Windows | pass | exit 0 |
| `cargo clippy -p loki-ipc --all-targets -- -D warnings` | Windows | pass | exit 0, no warnings |
| `cargo test -p loki-ipc` | Windows unrestricted, real named pipes | pass | 89 tests, 0 failed: 54 unit, 4 auth, 9 business authorization/wire, 5 fixtures, 4 deadline, 4 handshake, 6 malformed, 3 streaming |
| `git diff --cached --check` and staged-path inspection | isolated branch | pass | no whitespace errors; only packet ownership |

Mandatory verification not run:

- Complete repository verification is the orchestrator's post-integration gate, as required by the packet.

## 5. Contracts, migrations, and compatibility

- Contracts changed: local IPC minor version `1.0` to `1.1`; adds `loki-ipc.business.v1`, 23 methods, 10 scoped streams, typed results/events, and workspace/scope principal projection.
- Consumers affected: future Electron main, MCP and Rust service consumers; none implemented by this node.
- Migration: negotiate the business capability before using v1.1 methods; retain v1.0 core-only Ping/echo behavior.
- Backward/rolling compatibility: server intersects capability with the client's minor edge; hostile v1.0 business advertisement cannot negotiate or use business; Ping remains usable.
- Generated-contract drift: shared fixtures parse and validate in TS and Rust, including principal, output, null/omission, charset, ISO UTC and UTF-8 byte parity.
- Build recipe/executor/provenance compatibility: request carries typed snapshot/target/recipe IDs only; no executable/path surface.
- Cloud API/offline/notification compatibility: n/a; local IPC contract only.

## 6. Security, privacy, and device safety

- Trust boundaries touched: credential resolution, named-pipe encode/decode, producer output, client decoding, method and stream authorization.
- Authorization/approval impact: credential resolver binds each token to a distinct workspace/scopes/authorities principal; all 23 methods and all 10 stream domains have table coverage; approval decisions remain desktop-only.
- Secrets/data handling: authentication token behavior unchanged; no secrets or customer/device payloads added.
- Containment profile and denied authorities: agents are denied approval grant/deny even if misconfigured with decision authority; missing capabilities and authorities fail closed.
- Device side effects performed: none.
- New dependencies: none.
- Remaining risk: production composition must supply its OS-protected credential resolver and real `BusinessService`; the safe defaults have no business authority and return typed `UnsupportedMethod` rather than fake output. Independent re-review is required before integration.

## 7. Decisions and assumptions

- Semantic validation occurs before frame encode and immediately after decode for inputs and outputs; producer/client coupling adds method, stream, scope, sequence, summary and correlation checks.
- HelloAck principal comes only from per-credential server resolution; client type and payload fields cannot create or expand authority.
- Protocol v1.0 is server-enforced core-only; business capability is legal only on the v1.1 edge.

## 8. Review focus

- Inspect `authorize_method` and `authorize_stream` against the 23-method and 10-domain matrices, credential workspace/scope binding, and desktop-only approval guards.
- Inspect producer and client coupling, hostile v1.0 downgrade, and pre-encode/post-decode output validation including ISO UTC and UTF-8 byte bounds.

## 9. Follow-up graph changes proposed

- No graph mutation by this worker. Runtime/Electron/MCP composition nodes should depend on this contract and inject real credential and business-service implementations.
