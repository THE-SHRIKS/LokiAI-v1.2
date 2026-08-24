# LDEV-001 verification — 2026-08-24

- Implementation revision: `ac18835`
- Worker/host: Codex on Windows 11
- Device effects: none; no serial command, configuration, restart, or flash

| Gate | Result |
| --- | --- |
| `cargo test -p loki-local-devices` | pass; 6/6 |
| focused warnings-denied Clippy | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `pnpm run verify` | pass; full TypeScript/Electron/Rust workspace |

Verified: LOC-001 authorization, nonce replay denial, idempotent duplicate,
active-binding overwrite denial, exact consequential identity match, durable
revocation, fresh-nonce rebind, restart persistence, and expired-session denial.
The schema contains connector-qualified stable identity and no transport/COM-port
identity field. Existing renderer lint output remains four warnings, zero errors.
