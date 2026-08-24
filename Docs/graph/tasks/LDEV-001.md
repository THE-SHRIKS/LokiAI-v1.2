# Task Packet — `LDEV-001`

Status: `accepted for implementation`  
Graph node: `LDEV-001`  
Part/wave: `Part One / W5`  
Issued: `2026-08-24`

## Objective

Implement installation-scoped local device binding without inventing cloud
ownership. Bind only stable connector-qualified hardware identity with evidence;
reject replay, wrong runtime/installation, silent identity changes, and use after
revocation.

## Dependencies and reading

- Accepted: `CON-002`, `LOC-001`, `ADR-0002`
- Read: `AGENTS.md`, `Docs/CONNECTOR_SPEC.md`, `Docs/SECURITY.md`,
  `Docs/SPEC.md`, `Docs/TESTING.md`, and ADR-0002.

## Ownership

- Allowed: new `crates/loki-local-devices/**`, workspace manifest/lockfile,
  packet/evidence/handoff.
- Forbidden: cloud enrollment/workspace semantics, connector-specific flashing,
  port-as-identity, device mutation, renderer changes, or authority bypass.

## Acceptance criteria

1. Binding requires a live LOC-001 session with `BindLocalDevice` capability.
2. Connector ID, stable hardware identity, runtime ID, evidence reference, and a
   unique bounded request nonce are persisted transactionally.
3. Replayed nonce is rejected; duplicate identical binding is idempotent; changed
   identity/runtime cannot overwrite an active binding.
4. Revocation is explicit, durable, and single-transition; rebind requires a new
   nonce after revocation.
5. Consequential eligibility requires an active binding and exact current
   connector/hardware/runtime match; COM/port references never establish identity.
6. Restart, replay, mismatch, revocation, malformed/oversized input, and race
   tests pass deterministically.

## Verification

Run focused tests and Clippy, then full `pnpm run verify` and workspace Clippy
with warnings denied. No physical device mutation is authorized.
