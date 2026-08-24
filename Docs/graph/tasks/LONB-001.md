# Task Packet — `LONB-001`

Status: `accepted for implementation`  
Graph node: `LONB-001`  
Part/wave: `Part One / W5`  
Issued: `2026-08-24`

## Objective

Implement a durable local setup state machine for the installation owner,
registered agent, and existing-bound-device or project-only branch. Never create
or imply a cloud account, workspace, role, or enrollment.

## Dependencies

Accepted `LOC-001`, `LDEV-001`, `MCP-002`, and ADR-0002. Read `AGENTS.md`,
`Docs/SPEC.md` onboarding/state sections, `Docs/SECURITY.md`, `Docs/TESTING.md`.

## Ownership

New `crates/loki-local-setup/**`, workspace manifest/lockfile, and node evidence.
No renderer, cloud, device mutation, agent credential creation, or fabricated
completion.

## Acceptance criteria

1. Explicit states/transitions cover owner → agent → choice → bound device or
   project → complete, plus back and recoverable failure.
2. Advancing requires validated real reference IDs supplied by accepted producer
   services; absence is an error, never a generated fake reference.
3. Every transition requires a live LOC-001 session, is transactional, records
   correlation/time, and rejects stale expected-state races.
4. Restart/resume preserves state; back clears invalid descendant references;
   complete cannot be reached before its selected branch is satisfied.
5. Malformed input, illegal transitions, stale state, restart, failure/retry,
   branch, and concurrency tests pass; full workspace gates pass.
