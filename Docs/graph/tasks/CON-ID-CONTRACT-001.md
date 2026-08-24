# Task Packet — `CON-ID-CONTRACT-001 Device-derived Connector Identity`

Packet status: `accepted`  
Graph status: `BLOCKED_DEPENDENCY`  
Created by: `Codex orchestrator`  
Created at: `2026-08-24T15:35:00Z`

## 1. Objective

Define device-family-neutral, bounded contracts for transport candidates, probe
results, identity evidence and stable identity derivation so no connector ID,
COM port, USB path or provisional locator can masquerade as device identity.

## 2. Traceability

- Requirements: `FR-DEV-001`, `FR-DEV-002`, `FR-DEV-003`, `FR-CON-002`
- Acceptance scenarios: `AS-003-L`, `AS-004`
- Parent audit: `CON-003-PRE-001`

## 3. Dependencies and inputs

- `depends_on`: `CON-001`, `CON-003-PRE-001`
- Required inputs: `Docs/CONNECTOR_SPEC.md`, existing connector TS/Rust contracts.

## 4. Required reading

- `AGENTS.md`
- `Docs/ARCHITECTURE.md`, `Docs/SECURITY.md`, `Docs/CONNECTOR_SPEC.md`, `Docs/TESTING.md`
- `Docs/graph/handoffs/CON-003-PRE-001-1.md`

## 5. Ownership

Writable:

- `packages/contracts/src/connector.ts`
- focused contract fixtures/tests
- `crates/loki-connectors-sdk/**`
- `Docs/graph/handoffs/CON-ID-CONTRACT-001-1.md`

Read-only: registry and concrete connectors. Root manifest changes return to the orchestrator.

Forbidden: ESP32-specific shared types, raw sensitive locator persistence,
connector-derived identity, arbitrary paths/commands/URLs, mocks claimed as physical evidence.

## 6. Deliverables

1. Bounded transport-candidate and probe-result contracts with explicit unstable/sensitive locator semantics.
2. Typed identity evidence with device-derived fingerprint material/method, confidence, connector/version, UTC time and evidence digest.
3. Stable derived public identity distinct from encrypted raw identity evidence.
4. Exact TypeScript/Rust compatibility fixtures and discriminating invalid-boundary tests.

## 7. Acceptance criteria

- [ ] No provisional transport field can be accepted as stable identity.
- [ ] Every external field is runtime-validated and bounded in both languages.
- [ ] Device-family-specific evidence remains opaque/typed behind the connector boundary.
- [ ] Sensitive raw identity is marked and cannot enter ordinary logs/projections.
- [ ] Unknown/partial/ambiguous/conflicting identity is explicit and fail closed.
- [ ] Cross-language fixtures prove exact serialized compatibility and rejection parity.

## 8. Required verification

Format, warnings-denied Clippy, contract typecheck/tests, Rust tests, fixture parity,
workspace verification, secret/sensitive-log scan and independent contract review.

## 9. Risks and safety

Risk: high identity/security boundary. No device operation is authorized by this packet.

## 10. Human gate

Required before implementation because this node adds a new public
cross-language protocol and compatibility promise:

> I approve the new Connector Identity Protocol v1.0.0: device-neutral
> transient candidate/probe contracts, encrypted-only device-derived identity
> evidence, installation-scoped HMAC-SHA256 public device identity, and
> explicit fail-closed unknown/partial/ambiguous/conflicting outcomes. I accept
> that this is a new public cross-language protocol requiring downstream
> registry migration.

Exact physical SKU remains separately blocked by `OQ-001`.

## 11. Worker and reviewer

Accountable worker: isolated contract worktree. Independent reviewer assigned after implementation.

## 12. Stop conditions

Return `NEEDS_REPLAN` if stable identity cannot be derived without persisting a
raw locator as identity or if the shared contract would require ESP32-specific semantics.
