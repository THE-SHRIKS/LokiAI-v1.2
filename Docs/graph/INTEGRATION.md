# Integration Protocol — LokiAI Main v1.2

Status: **Normative operational process**  
Updated: **2026-08-15**

## 1. Objective

Integration converts independently verified task nodes into one verified system.
It is not an opportunity to redesign worker output or hide failing contracts.

## 2. Preconditions

A node may enter integration only when its handoff contains:

- node ID and exact starting revision;
- result revision or patch identity;
- changed-file list and confirmation of ownership compliance;
- acceptance-criterion results;
- commands run with exit results;
- contract/schema changes and compatibility impact;
- migrations and rollback/recovery notes;
- security/privacy/device-safety impact;
- known limitations and follow-up nodes;
- independent review result when required.

Incomplete handoffs return to the worker.

## 3. Integration order

Within a wave, integrate in topological order:

1. accepted decisions and ADRs;
2. schema/contract producers;
3. domain and storage primitives;
4. adapters/services;
5. UI/client consumers;
6. tests and independent verification not already paired;
7. packaging/release metadata.

Do not integrate a consumer before the exact contract revision it consumed.

## 4. Contract ownership

Public and cross-process contracts have one active writer. Consumers may propose a
change but must not silently fork or locally reinterpret the contract.

Contract changes require:

- schema version and compatibility classification;
- affected consumer list;
- positive and negative contract fixtures;
- generated-type drift check where generation is used;
- rolling-version behavior or atomic update decision;
- traceability update.

Breaking changes create a new graph branch with migration nodes. They are not
merged as incidental fixes.

## 5. Merge-conflict policy

The integrator may resolve:

- formatting-only conflicts;
- adjacent import ordering;
- generated manifest changes whose source and result are deterministic;
- documentation links/status references with no semantic impact.

The integrator must return conflicts involving:

- behavior, state transitions, authorization, validation, or error semantics;
- public DTO/schema/trait/tool signatures;
- database migrations;
- dependency versions or lockfile meaning;
- security controls;
- device operation sequencing;
- test expectation differences.

## 6. Integration checks

After each node:

- inspect the integrated diff against the task packet;
- run contract/generated drift checks if contracts changed;
- run affected format, lint, type, unit, and package tests;
- run affected cross-process tests;
- confirm no unrelated files or secrets entered;
- update evidence link and ready frontier.

After each wave:

- clean checkout/install/build;
- full required TypeScript and Rust checks for the wave;
- desktop/runtime handshake plus MCP per-request version/client-capability and
  optional-discovery compatibility;
- runtime/build-executor/recipe and runtime/cloud contract compatibility;
- database migration from the last accepted baseline;
- security checks affected by the wave;
- desktop E2E for affected user journeys;
- `Docs/graph/TRACEABILITY.md` coverage review.

## 7. Cross-process compatibility matrix

Maintain tested combinations during implementation:

| Desktop | Runtime | MCP server | Build executor | Cloud API | Expected result |
|---|---|---|---|---|---|
| current | current | current | current | current | full capabilities |
| current | previous supported | current | compatible | supported | negotiated supported subset or guided atomic update |
| previous supported | current | previous supported | current | supported | compatible subset or guided update |
| any | current | any | incompatible | any | build unavailable; no unrestricted fallback |
| any | current | any | any | previous supported | explicit accepted API subset or guided update |
| incompatible | any | any | any | any | explicit `version-incompatible`; no privileged fallback |

The release plan decides whether the supported rolling window is required or the
installer updates all local components atomically. Until that decision, protocols
still identify versions and capabilities.

## 8. Database migration integration

- Runtime is the only migration owner.
- Desktop/MCP wait for runtime readiness; neither runs schema migration.
- Migration is tested from every supported previous release database.
- Interrupted migration recovery is tested.
- Encryption key availability/version is checked before migration; wrong or
  missing key never causes creation of a replacement empty database.
- WAL, backup, restore, and rekey behavior use the same accepted encryption
  boundary and are tested across the supported service/user identity mode.
- Irreversible migration requires a backup and release rollback implications.
- Application code that requires a new column/table is not integrated before its
  migration and compatibility behavior.

## 9. Evidence locations

Until CI infrastructure defines immutable run URLs, store concise records under a
future `evidence/<release-or-wave>/<node-id>/` hierarchy. Do not commit large raw
device logs, secrets, binaries, databases, or user data. Store hashes and protected
artifact references for large/sensitive evidence.

## 10. Integration result

Use one of:

- `INTEGRATED_PASS` — node and affected gates pass on integration baseline;
- `RETURN_TO_WORKER` — node defect or incomplete handoff;
- `CONTRACT_REPLAN` — contract invalidates consumer assumptions;
- `BLOCKED_ENVIRONMENT` — required environment/device/credential unavailable;
- `SECURITY_GATE` — named security requirement failed;
- `HUMAN_DECISION` — conflict requires product/architecture/risk authority.
