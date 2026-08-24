# Task Packet — `LNOT-001 Local Notification Intents`

Packet status: `accepted`  
Graph status: `ready`  
Created by: `Codex orchestrator`  
Created at: `2026-08-24T18:45:00+05:30`  
Starting revision: `80146739e8b5bd60e5a609093630d8a35d4ba107`

## 1. Objective

Implement a production local notification-intent service that durably projects
incident work intents and explicit runtime-offline observations into a
deduplicated, privacy-bounded local action center with preferences, truthful
presentation/read/expiry state, and closed safe action references. It performs
no cloud, push, agent, or OS-toast delivery and never claims those deliveries.

## 2. Traceability

- Requirements: `SPEC 3.13`, `SPEC 4.10`, `SPEC 9`, `SPEC 10`
- Acceptance scenarios: local slice of `AS-014` and human-fallback slice of
  `AS-015`; protected state `AS-016`
- Architecture components: runtime notification projection and encrypted SQLite
- Security controls: notification spoofing, sensitive-data bounds, SQLCipher,
  no arbitrary deep link/path/URL
- Parent/feature/incident: `ADR-0002`

## 3. Dependencies and inputs

- `depends_on`: accepted `INC-001`, accepted `DSK-001`, integrated DAT-002
  production checkpoint (DAT-002 itself remains blocked and is not relabeled)
- Consumed contracts:
  - `loki_incidents::work_intent::WorkIntent` at starting revision
  - DAT-002 `open_or_migrate_sqlcipher` at starting revision
- Environment: current Windows desktop-user identity for DPAPI tests
- Starting condition: no local notification service exists; cloud notification
  and remote delivery nodes are deferred by ADR-0002

## 4. Required reading

- `AGENTS.md`
- `Docs/ARCHITECTURE.md` sections 3.1, 4, 8, and 9
- `Docs/SECURITY.md` sections 4, 11, and 13
- `Docs/SPEC.md` sections 3.13, 4.10, 9, 10, and AS-014/AS-015/AS-016
- `Docs/TESTING.md`
- `Docs/graph/INTEGRATION.md`
- `crates/loki-incidents/src/work_intent.rs`

## 5. Ownership

Writable paths:

- `crates/loki-local-notifications/**`
- workspace `Cargo.toml` and `Cargo.lock`
- `Docs/graph/evidence/W6/LNOT-001/**`
- `Docs/graph/handoffs/LNOT-001-1.md`

Read-only dependencies: `crates/loki-incidents/**`, `crates/loki-storage/**`,
desktop/Electron sources.

Forbidden: renderer or preload changes; arbitrary URI/path/action strings;
cloud/push/remote/agent/Windows-toast delivery claims; device actions; secrets;
raw incident evidence or agent text in notification display fields.

## 6. Deliverables

1. Durable encrypted notification/preference store and idempotent schema.
2. Closed category/severity/channel/state/action-reference types with bounded
   redacted display projection.
3. Incident-work-intent and explicit runtime-offline enqueue paths with durable
   deduplication.
4. Transactional presentation, read, expiry, preference, and restart behavior.
5. Focused tests, evidence, and handoff.

## 7. Acceptance criteria

- [ ] One deduplication key produces exactly one notification under retry/race.
- [ ] Work intent projection carries identifiers only and grants no approval or
  capability; runtime-offline input requires explicit observer/last-seen data.
- [ ] Preferences suppress presentation truthfully without deleting the durable
  intent; presentation/read/expiry transitions reject illegal or stale updates.
- [ ] Actions are a closed local enum and cannot encode a URL, filesystem path,
  command, or arbitrary IPC method.
- [ ] Display title/body are bounded, reject controls, and never copy raw evidence
  or delivery-requirement text.
- [ ] Restart preserves exact state and raw database bytes expose no schema or
  marker; missing/wrong key fails closed through DAT-002.
- [ ] Full workspace verification passes with no new lint errors.

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `cargo fmt --all -- --check`; workspace Clippy warnings denied | pass | summary |
| Unit | `cargo test -p loki-local-notifications` | all focused tests pass | counts |
| Contract | work-intent projection tests | IDs preserved; no authority copied | result |
| Integration | encrypted restart/dedup/concurrency | exact durable state | result |
| Desktop | n/a, UI is Part Two | not claimed | recorded n/a |
| Cloud | n/a, deferred by ADR-0002 | no network/cloud state | scope audit |
| Containment | n/a | no executable behavior | recorded n/a |
| Hardware | n/a | no device effect | recorded n/a |
| Security | privacy/action/ciphertext negative tests | no blocking finding | result |

## 9. Risks and safety

- Risk: medium
- Failure modes: duplicate alerts, false delivered state, unsafe action target,
  sensitive display leakage, plaintext metadata, stale offline observation
- Recovery: idempotent migrations and reopen; no destructive cleanup
- Data: sensitive identifiers in SQLCipher; no secrets or raw evidence
- Executable/toolchain: none
- Cloud/offline: local state only; cloud explicitly unavailable
- Device: none

## 10. Human gate

- Gate: none; ADR-0002 already authorizes this local replacement

## 11. Worker and reviewer

- Accountable worker: Codex
- Independent reviewer: orchestrator self-check at integration; independent
  release security review remains Part Two
- Branch: `codex/LNOT-001`
- Handoff: `Docs/graph/handoffs/LNOT-001-1.md`

## 12. Stop conditions

Return `NEEDS_REPLAN` if implementing the node requires renderer/OS-toast/cloud
delivery, a public protocol change, arbitrary actions, or mutation of the
INC-001 work-intent record. Return `BLOCKED_DEPENDENCY` if encrypted managed open
cannot be used without weakening DAT-002.
