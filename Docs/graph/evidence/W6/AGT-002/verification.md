# AGT-002 verification evidence

- Worker revision: `6c90d72`
- Verified: `2026-08-24T19:08:00+05:30`
- Scope: local-only incident work-intent delivery; remote/cloud delivery absent

## Results

- `cargo test -p loki-agent-delivery`: 7 passed, 0 failed.
- `cargo clippy -p loki-agent-delivery --all-targets -- -D warnings`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- `pnpm run verify`: pass after the exact command was rerun outside the sandbox
  because the first attempt received Windows `spawn EPERM` while pnpm created
  TypeScript workers. The elevated rerun completed every stage.
- Existing renderer lint baseline: 4 warnings, 0 errors; no renderer file changed.

## Security and state evidence

- Claim requires an active AGT-001 secret and explicit
  `ReceiveWorkIntents` scope; wrong secret, absent scope, and revocation deny.
- Concurrent claims produce one winner and exactly one delivery attempt.
- Acknowledgement links an existing run owned by the claiming connection;
  cross-connection acknowledgement is denied.
- Queued and delivered expiry is fail-closed; failed, acknowledged,
  user-action-required, and expired records do not regress.
- Human fallback creates exactly one deduplicated LNOT-001 intent and validates
  the complete original work-intent projection before doing so.
- SQLCipher/DPAPI restart preserves terminal state; raw database bytes do not
  contain the schema marker; a different protected key fails to open it.
- Dependency review: the new crate reuses workspace-pinned `rusqlite 0.40.2`,
  `serde_json 1.0.151`, and existing internal crates; no network/provider/device
  dependency was introduced.

Intentional SQLCipher HMAC diagnostics during wrong-key negative tests are the
expected fail-closed evidence, not test failures. Vendored OpenSSL PDB linker
messages are toolchain debug-symbol warnings and do not change the passing gate.
