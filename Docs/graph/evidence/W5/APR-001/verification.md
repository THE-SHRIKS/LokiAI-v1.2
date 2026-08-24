# APR-001 verification — 2026-08-24

- Implementation revision: `7cbd62f`; worker/host: Codex, Windows 11
- `cargo test -p loki-approvals`: pass, 6/6
- focused and full-workspace warnings-denied Clippy: pass
- `pnpm run verify`: pass across TypeScript, Electron, and Rust workspace
- Device/build/cloud effects: none

Verified closed/default-deny policy, length-prefixed canonical SHA-256 binding,
artifact-absence distinction, grant-before-consume, exact-request mutation
denial, expiry, policy denial, single-use replay denial, and concurrent consume
with exactly one winner. Records are metadata, not bearer secrets. Existing
renderer lint output remains four warnings and zero errors.
