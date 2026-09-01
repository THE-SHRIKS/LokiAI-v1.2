# Integration verification — `P1-NODE-IPC-CLIENT-001`

Integrated at commit `631c075` on `LokiAI-v1.2`, merging branch
`node/P1-NODE-IPC-CLIENT-001` (commits `7ecd2ca`, `74307a8`, `9f4e705`
[docs], `ccf89d5`, `b6a6d53`).

**Process note**: the original integrator agent's merge succeeded before its
process was interrupted (harness/session teardown), so the merge commit
landed but the agent's own gate-verification, evidence recording, and
`TASK_GRAPH.md` update did not complete. The orchestrator (Claude Code)
completed those steps directly, re-verifying the merged state rather than
trusting the interrupted agent's unfinished work.

## Verification performed post-merge, on `LokiAI-v1.2`@`631c075`

| Command | Result |
|---|---|
| `cargo build --workspace` | pass. One pre-existing, expected `dead_code` note for `BusinessOverride::{CredentialOnly,Full}` in `compose.rs` under a plain (non-test) build — these variants are test-only and are constructed under `cargo clippy --workspace --all-targets` and `cargo test`, which do compile test targets; not a regression. |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, exit 0, zero warnings |
| `cargo test -p loki-ipc -p loki-storage -p loki-runtime -p loki-credential-native -- --test-threads=1` | pass, all green across every test binary in the four crates most affected by this node (67+15+63+58+... test result: ok blocks, 0 failed) |

Independent review record (both completed before the merge, findings closed
in `ccf89d5`/`b6a6d53` before integration):

- **Security review**: no Critical/High. All ADR-0009 (Option A) constraints
  independently verified: no generic encryption/decryption CLI surface; no
  plaintext credential material through renderer/logs/env/argv/config;
  purpose-bound kind-tag check unconditional and precedes any success
  return; independent rotation/revocation; fail-closed on all four failure
  modes (missing/corrupt/mismatched/inaccessible); DPAPI convention
  compatibility guaranteed by construction (native addon links the same
  compiled `loki_storage::credential` code the Rust minting side uses, not
  an independently-reimplemented format); `crates/loki-storage/src/secret.rs`
  and production `ComposedHost::open()`'s `ServerConfig`/credential-resolver
  wiring both confirmed untouched (deferred scope respected).
- **Test review**: PASS. Independently reproduced 342 (`@lokiai/contracts`)
  + 9 (`@lokiai/native-credential-store` cross-language round-trip,
  byte-for-byte) + 14 (`@lokiai/ipc-client`, real handshake/read/mutation/
  deadline/cancellation/contention/revocation against a real running
  `loki-ipc` server) tests, plus full `loki-storage`/`loki-runtime`
  regression, clean.
- **Shared finding, fixed**: both reviewers independently found
  `packages/ipc-client/src/errors.ts`'s `CredentialRevalidationFailedError`
  was defined but never actually constructed (client always threw the
  generic `RpcCallError`). Fixed in `ccf89d5` — both `ping()`/`businessCall()`
  now route through a shared `toRpcCallError()` that constructs the specific
  subclass on `kind === 'credential_revalidation_failed'`; the revocation
  test now asserts `instanceof CredentialRevalidationFailedError` directly.
- Two handoff-accuracy corrections (an inaccurate "tested against the
  release artifact" claim — actually the debug artifact, no behavioral
  difference since no `cfg`-gated logic differs between profiles; and an
  inaccurate clippy-clean claim for the test-only harness binary, which had
  a real `collapsible_if` lint, since fixed) recorded in `b6a6d53`.

## Deferred, explicitly named follow-ups (not silently absorbed)

1. **Authority-provisioning-policy decision**: what `Authority`/
   `authorized_entity_ids` set a freshly-minted production credential should
   carry once `ComposedHost::open()` is eventually wired to the real
   `CredentialSet`/`CredentialResolver` (this node deliberately left
   production `open()` on the single legacy token — per its own §5 forbidden
   scope). Not yet decided anywhere in the repo. Blocks final production
   wiring in `P1-ELECTRON-RUNTIME-001`/`P1-MCP-RUNTIME-001`, though those
   nodes' own scaffolding/client-side work can proceed without it.
2. **N-API cross-runtime ABI verification against Electron's embedded
   Node**: this node's cross-language round-trip and integration tests ran
   against plain Node.js only (no Electron process was available to the
   implementer/reviewers). `P1-ELECTRON-RUNTIME-001` must independently
   confirm `loki-credential-native`'s compiled `.node` addon loads and
   functions identically inside Electron's main process before relying on
   it there.

Both items are recorded here and in `TASK_GRAPH.md`'s row for this node,
not assumed resolved.
