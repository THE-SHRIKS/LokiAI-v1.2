# P1-RPC-CONTRACT-001 Independent Review 001

Result: `FAILED_VERIFICATION`  
Reviewed revision: `133f615`  
Reviewer: independent Part One integration/security reviewer  
Date: `2026-08-24`

Worker evidence was reproducible: `cargo test -p loki-ipc` passed 80/80 on
real Windows named pipes and the TypeScript contract suite passed 311/311.
Those tests did not discriminate the following five unresolved high findings.

## HIGH-1 — Business success is disconnected from the wire

All 23 business requests can be decoded, but `MethodResult` contains only Ping,
`BusinessRpcResult` is orphaned from `WireMessage::CallResult`, and the server
returns `UnsupportedMethod` for every business method. `BusinessEvents` is also
unsupported and the client treats its typed summary as an internal error.

Required correction: reachable typed unary and stream results, producer
injection, method/result and stream/summary coupling, mismatch rejection, and
representative real-pipe positive tests.

## HIGH-2 — Static pipe-wide principal creates a confused deputy

One static `ServerConfig.principal` applies to every credential/client on the
pipe; the default is an unresolved runtime-service principal. The token/client
identity is not resolved to a distinct scoped principal, and no workspace scope
exists. A shared agent may inherit desktop authority or desktop actions may be
unavailable depending on the one configured principal.

Required correction: fail-closed per-credential principal resolution with
distinct desktop/agent credentials, scoped authority/workspace context, and
real-pipe separation/revocation tests.

## HIGH-3 — Subscribe bypasses resource authorities

All sensitive streams require only a generic `Subscribe` authority. A principal
without telemetry, incident, job, approval, notification, or work-intent read
authority can request those streams once a producer exists.

Required correction: explicit stream-to-authority and scope/filter matrix plus
complete allow/deny tests.

## HIGH-4 — v1.0 can self-advertise the v1.1 capability

Same-major compatibility and plain capability intersection let a malicious v1.0
client advertise and negotiate `loki-ipc.business.v1`. The existing v1.0 test
uses a cooperative capability set and therefore does not prove downgrade safety.

Required correction: server-enforced version/capability gating and a hostile
v1.0-advertises-business test.

## HIGH-5 — Output parity and runtime validation are not exact

Positive fixtures prove only that one payload deserializes. Rust output/result
timestamps accept arbitrary strings where TypeScript requires ISO UTC; Rust
business results lack complete semantic/size validation; and Rust string bounds
count UTF-8 bytes while TypeScript `z.string().max` counts JavaScript code units.
Malformed/oversized producer-output tests are absent.

Required correction: exact input/output validation parity, explicit UTF-8 byte
semantics, validated ISO UTC timestamps, validation before send and after receive,
and malformed/oversized output rejection tests.

## Positive observations

- Request vocabulary is closed and contains no generic invoke, path, URL, shell,
  executable, or environment surface.
- Unary request bounds run immediately after frame decode.
- Core Ping/Echo cancellation, deadlines and backpressure remain proven.

No review edits were made. The node cannot integrate until all five high
findings are corrected and a new independent review passes.
