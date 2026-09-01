# ADR-0009 — Node IPC credential distribution (Option A: shared native DPAPI component)

Status: `accepted`
Date: `2026-09-01`
Decision owners: `human product/security owner`

## Context

`P1-RPC-CONTRACT-001-R1` and `P1-RUNTIME-COMP-001` (both accepted, integrated)
built a real, multi-credential-capable business RPC server
(`crates/loki-ipc/src/auth.rs`'s `CredentialSet`/`CredentialResolver`,
`AuthenticatedPrincipal::kind: PrincipalKind::{DesktopInstallation,
AgentConnection, RuntimeService}`), but production `ComposedHost::open()`
still binds a single legacy static shared-secret token
(`crates/loki-runtime/src/host/token.rs`), DPAPI-protected via a hand-rolled
`CryptProtectData`/`CryptUnprotectData` call in `crates/loki-storage/src/
secret.rs` — a Rust-only mechanism unreachable from any Node.js process.

Two structurally different Node processes need their own distinct credential
to reach the runtime: Electron main (has Electron's `safeStorage`, a
DPAPI wrapper with its own blob format) and the standalone MCP server (a
plain Node stdio process with no Electron APIs, hence no `safeStorage`).
Neither can currently obtain, decrypt, or use a credential the Rust side
mints and protects with its own hand-rolled DPAPI call. `P1-NODE-IPC-CLIENT-001`
was blocked on deciding how to close this gap — see the planner's decision
brief (2026-08-31, chat) that first surfaced this as a real architecture
question, not an implementation detail.

## Options considered

**A — One small shared native Windows-DPAPI credential component**, built
once, used identically by the Rust minting side, Electron main, and the MCP
server, so all three speak the exact same DPAPI entropy/blob convention.

**B — Electron uses its own `safeStorage` for its own copy** (obtained once
over an already-trusted channel); the MCP server needs a structurally
different mechanism since it has no `safeStorage`.

**C — Something else** (not specified further; not chosen).

## Decision

**Option A**, human-specified with exact constraints (2026-09-01, chat):

- Build one narrow, shared Windows-native credential component consumed by
  the Node IPC client (used by both Electron main and the MCP server).
- It creates/loads **two distinct random credentials**: `DesktopInstallation`
  and `AgentConnection` — matching `loki-ipc`'s existing `PrincipalKind`
  variants exactly, no new kind invented.
- Protection: current-user Windows DPAPI, strict per-user file permissions,
  purpose-bound formats (i.e. the two credentials are not
  interchangeable/confusable at the format level).
- **Explicitly forbidden**: a generic encryption/decryption CLI (this is a
  narrow, purpose-built component, not a general secrets tool); exposing
  plaintext credentials to the renderer, logs, environment variables,
  command-line arguments, or configuration files anywhere.
- **Required properties**: independent rotation/revocation per credential
  (rotating `AgentConnection` must not require touching
  `DesktopInstallation`, and vice versa); fail-closed behavior on missing,
  corrupt, mismatched, or inaccessible credential state (never fall back to
  a plaintext/default/no-auth path).

## Consequences

- A new, small, focused native/DPAPI-capable component is added as a real
  dependency of `packages/` (Node-consumable) — its exact form (native
  Node addon vs. a small helper binary vs. something else) is
  `P1-NODE-IPC-CLIENT-001`'s implementation detail to work out against this
  decision's constraints, not re-litigated here.
- The Rust minting side (`crates/loki-runtime/src/host/token.rs` and/or a
  new sibling) must mint and protect BOTH credentials using the SAME DPAPI
  entropy/blob convention this new shared component uses, so what Rust
  writes, the shared component can read — this is the crux of Option A and
  must be verified end-to-end (Rust writes, Node reads, byte-for-byte
  round-trip), not assumed compatible by construction.
- `crates/loki-runtime/src/host/compose.rs`'s production `open()` path must
  eventually move from the single legacy token to the real
  `CredentialSet`/`CredentialResolver` machinery `loki-ipc` already supports
  — this ADR authorizes that transition as part of `P1-NODE-IPC-CLIENT-001`
  and/or its natural follow-ups (Electron wiring, MCP wiring), not as a
  separate blocking decision.
- Human explicitly authorized proceeding directly through
  `P1-NODE-IPC-CLIENT-001` → Electron wiring → MCP wiring → runtime E2E
  without further status-only pauses, using separate implementers and
  independent reviewers at each step, per the project's standard graph
  process.
