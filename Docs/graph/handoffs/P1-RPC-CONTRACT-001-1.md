# Worker Handoff — `P1-RPC-CONTRACT-001`

Result: `COMPLETE`
Worker: `/root/p1_runtime_audit`
Started from: `29770b5e42c49ba65e9e3df9c60842af9a1edfb8`
Result revision/patch: `4d26431263eceb10b4a2b7332d9a30133afc5bef`
Completed at: `2026-08-24T15:23:55Z`

## 1. Outcome

The TypeScript and Rust packages now share one explicit protocol-v1.1 business RPC vocabulary containing 23 named methods, 10 named bounded streams, closed projections, and server-owned authenticated principal context. Every decoded request is semantically validated at the frame boundary, authority is selected only by the server, agents cannot decide approvals, and unsupported business consumers fail explicitly instead of returning fabricated data. Protocol v1.0 remains a tested core-only compatibility edge.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Named reads, diagnostics, cancellation, agent-run and work-intent methods | pass | Shared 23-request fixture; TS 311/311 and Rust method-table tests pass |
| Distinct consequential requests return typed operation state contracts | pass | `BusinessRpcRequestSchema`, `BusinessRpcResultSchema`, `BusinessRpcResult`; no command/path/URL/environment field |
| Desktop-only approval grant/deny | pass | Real named-pipe agent-denial test plus complete authority-table test |
| Named, bounded, correlated, cancellable, backpressured streams | pass | 10-stream fixture, max 1024 frame validation, existing cancellation/deadline/slow-consumer suites pass |
| Client metadata cannot create authority | pass | Real named-pipe HelloAck projection test and unknown-field rejection |
| Current/previous/incompatible compatibility | pass | v1.1 capability negotiation, v1.0 core-only test, existing major-version rejection test |
| No generic invoke or unrestricted execution surface | pass | TS and Rust negative tests; final diff inspection |
| No unrelated changes | pass | Final staged diff contained only packet-owned paths |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `packages/contracts/src/businessRpc.ts`, `src/index.ts` | Runtime-validated TS request/result/principal/stream contract and export | yes |
| `packages/contracts/fixtures/business-rpc.json` | Canonical cross-language fixture for every request/result/stream/event | yes |
| `packages/contracts/tests/businessRpc.test.ts` | Positive/negative contract, null, bounds, charset, authority and generic-invoke tests | yes |
| `crates/loki-ipc/src/business.rs` | Rust-aligned business types, enums and semantic validators | yes |
| `crates/loki-ipc/src/protocol.rs`, `frame.rs`, `lib.rs` | v1.1 vocabulary/capability edge and immediate decoded-frame contract validation | yes |
| `crates/loki-ipc/src/server.rs`, `client.rs` | Server-created principal projection, authorization/capability gates and safe client projection | yes |
| `crates/loki-ipc/tests/business_authorization.rs`, `business_fixtures.rs` | Real-pipe security/compatibility and cross-language drift evidence | yes |

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `pnpm --filter @lokiai/contracts typecheck` | Windows, isolated worktree | pass | `tsc --noEmit`, exit 0 |
| `pnpm --filter @lokiai/contracts test` | Windows, unrestricted process spawn | pass | 9 files, 311 tests, 0 failed; business RPC 8/8 |
| `pnpm exec prettier --check packages/contracts/src/businessRpc.ts packages/contracts/tests/businessRpc.test.ts packages/contracts/fixtures/business-rpc.json` | Windows | pass | all matched files formatted |
| `cargo fmt --all -- --check` | Windows | pass | exit 0 |
| `cargo clippy -p loki-ipc --all-targets -- -D warnings` | Windows | pass | exit 0, no warnings |
| `cargo test -p loki-ipc` | Windows unrestricted, real named pipes | pass | 80 tests, 0 failed: 50 unit, 4 auth, 5 business authorization, 4 fixtures, 4 deadline, 4 handshake, 6 malformed, 3 streaming |
| `git diff --cached --check` and staged-path inspection | isolated branch | pass | no whitespace errors; only packet ownership |

Mandatory verification not run:

- Complete repository verification is the orchestrator's post-integration gate, as required by the packet.

## 5. Contracts, migrations, and compatibility

- Contracts changed: local IPC minor version `1.0` to `1.1`; adds `loki-ipc.business.v1`, 23 methods, 10 streams, typed results/events, and safe principal projection.
- Consumers affected: future Electron main, MCP and Rust service consumers; none implemented by this node.
- Migration: negotiate the business capability before using v1.1 methods; retain v1.0 core-only Ping/echo behavior.
- Backward/rolling compatibility: previous v1.0 connects with core only and Ping succeeds; business methods reject without capability; incompatible major remains rejected.
- Generated-contract drift: shared fixture parses in both TS and Rust, including null/omission parity.
- Build recipe/executor/provenance compatibility: request carries typed snapshot/target/recipe IDs only; no executable/path surface.
- Cloud API/offline/notification compatibility: n/a; local IPC contract only.

## 6. Security, privacy, and device safety

- Trust boundaries touched: named-pipe frame decode, handshake projection, method and stream authorization.
- Authorization/approval impact: closed authority enum; server configuration owns principal; all 23 methods have capability and exact-authority table coverage; approval decisions additionally require desktop principal kind.
- Secrets/data handling: authentication token behavior unchanged; no secrets or customer/device payloads added.
- Containment profile and denied authorities: agents are denied approval grant/deny even if misconfigured with decision authority; missing capabilities and authorities fail closed.
- Device side effects performed: none.
- New dependencies: none.
- Remaining risk: business consumers must preserve method-to-result matching and validate producer data when implemented; this node deliberately returns typed `UnsupportedMethod` instead of fake output.

## 7. Decisions and assumptions

- Semantic request validation occurs immediately after JSON frame decode, before authorization or dispatch; this preserves ergonomic Rust types while enforcing the trust boundary.
- HelloAck principal is a server-created safe projection only and cannot grant client authority.
- Protocol v1.0 is intentionally core-only; adding business capability is the compatible v1.1 edge.

## 8. Review focus

- Inspect `authorize_method` against the complete 23-method table, desktop-only approval guard, and immediate `WireMessage::validate_contract` call from `read_frame`.
- Confirm TS/Rust fixture field names, nullable-option behavior, ID/digest character rules, and closed enums remain exact.

## 9. Follow-up graph changes proposed

- No graph mutation by this worker. Consumer nodes should depend on this contract and must add method/result coupling tests when they implement real services.
