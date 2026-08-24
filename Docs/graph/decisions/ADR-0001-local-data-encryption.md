# ADR-0001 — Local data encryption and bounded-context database topology

Status: `accepted`  
Date: `2026-08-24`  
Decision owners: `human product owner; orchestrator`  
Supersedes: `none`  
Superseded by: `none`

## Context

`DAT-001`, `MON-001`, and `INC-001` established three runtime-owned SQLite
database files: core storage, monitoring, and incidents. They are plaintext on
the accepted baseline. `Docs/ARCHITECTURE.md` §8 and `Docs/SECURITY.md` §13
require full-database encryption, including WAL, backups, and temporary files,
with a key held by the Windows OS protection boundary. `OQ-007` required an
edition and support decision before `DAT-002` could begin.

The human product owner selected SQLCipher Community Edition for the local-only
v1.2 implementation on 2026-08-24. The topology question recorded by MON-001
must also be settled without silently merging independent databases or changing
their consistency boundaries.

## Decision drivers

- Database, WAL, backup, and temporary-file confidentiality after local file theft.
- A Windows-native key protection boundary for both desktop-user and gateway-service identities.
- Reproducible, source-built dependency provenance without a proprietary runtime requirement.
- Preserve accepted bounded contexts and avoid a risky, unrelated data-topology migration.
- Honest recovery: a missing or wrong key must never create a replacement empty database.

## Considered options

### Option A — SQLCipher Community Edition; keep three encrypted bounded-context files

- How it works: build the pinned `rusqlite` dependency with its SQLCipher
  feature, provide each existing database connection the same in-memory key
  retrieved through Windows DPAPI, and encrypt each file's SQLite sidecars and
  generated backup. Keep core storage, monitoring, and incidents as separate
  database files.
- Benefits: meets the accepted full-database-encryption boundary, preserves
  working bounded contexts, avoids a data merge, and carries no licence fee.
- Costs/risks: Community Edition has no commercial support or FIPS validation;
  attribution is mandatory; native SQLCipher compilation and Windows key
  lifecycle require careful release verification.
- Migration/rollback: create a verified encrypted replacement with a backup,
  atomically switch only after validation, and retain the known-good backup for
  the documented recovery window. Do not roll back over an irreversible
  migration without restoring the validated backup.

### Option B — Commercial SQLCipher distribution

- How it works: package a supported commercial SQLCipher build and follow its
  support/licensing terms.
- Benefits: commercial support options and potentially validated build options.
- Costs/risks: licence and procurement dependency; no security benefit is
  assumed without specific contract and validation evidence.
- Migration/rollback: same data-migration and recovery obligations as Option A.

### Option C — Merge every bounded context into one database before encryption

- How it works: move monitoring and incident tables into `loki-storage` and
  encrypt the new shared database.
- Benefits: one file and one migration owner.
- Costs/risks: changes accepted ownership and consistency boundaries, expands
  migration blast radius, and is not required for full-database encryption.
- Migration/rollback: requires a separate accepted data-topology ADR and a
  dedicated data-migration node; not part of `DAT-002`.

## Decision

Use SQLCipher Community Edition for all local v1.2 runtime databases and ship
the required attribution. Preserve the existing three bounded-context database
files; `DAT-002` encrypts each file rather than merging them. A single random
database-key value is protected with Windows DPAPI for the identity that runs
the runtime: the current user in `desktop-user` mode and the dedicated service
identity in `gateway-service` mode. The plaintext key exists only in process
memory long enough to open or rotate a database and is never logged, stored in a
configuration file, or exposed to renderer, MCP, build-executor, device, or
cloud code.

## Consequences

### Positive

- Core, telemetry/rules, and incidents/evidence receive the same at-rest protection.
- No accepted data store is silently excluded because it lives outside `loki-storage`.
- No database merge changes current behavior or threatens existing data.

### Negative/tradeoffs

- Three encrypted files must each be migrated, backed up, recovered, and tested.
- SQLCipher Community Edition has attribution and self-support obligations.
- The product is not FIPS-validated by this choice.

### Required controls

- Use the exact pinned SQLCipher-capable build and record its licence, source provenance, maintenance posture, and removal path.
- Use Windows DPAPI with an application-specific entropy value; protect database-key metadata and the existing runtime IPC token through the same reviewed OS-secret abstraction.
- Apply SQLCipher keying before schema access; set and verify cipher compatibility/security pragmas before data access.
- Encrypt WAL, rollback journals, temporary stores, and backups; reject an unencrypted destination.
- Treat missing, malformed, or wrong key material as explicit recovery states; never call a create-on-open API against an existing encrypted database after such a failure.
- Rotate atomically per database with pre-rotation backup, integrity check, reopen verification, durable recovery marker, and an explicit interrupted-rotation path.
- Ship required SQLCipher Community attribution in release notices and SBOM/licence material.

## Compatibility and migration

- Affected components/contracts: `loki-storage`, `loki-monitoring`,
  `loki-incidents`, runtime host secret/token handling, packaging notices, and
  diagnostics. Renderer and MCP receive no key-management contract.
- Data/protocol migration: support a controlled plaintext-to-encrypted
  migration for each existing database. Existing deployments are not assumed
  to be disposable. The migration must make an encrypted backup before an
  atomic replacement and record recoverable progress without recording a key.
- Rolling compatibility: older plaintext-only binaries must not open an
  encrypted database. Installer/update planning must pair the schema/key change
  with compatible runtime binaries and provide the verified backup recovery path.
- Rollback/reversal plan: restore the verified pre-migration backup only through
  an explicit recovery operation; do not silently downgrade or overwrite data.

## Verification

- All three database contexts reject wrong/missing keys without replacing data.
- On-disk database, WAL/journal, temporary store, and backup do not expose a
  known plaintext test marker.
- Restart, migration, backup, and rotation preserve rows and integrity.
- Desktop-user and dedicated-service identity boundary tests prove a protected
  key cannot be unprotected by the other identity.
- Full workspace build, tests, formatting, warnings-denied Clippy, dependency/
  licence review, and an independent data-security review pass.

## Human acceptance

- Decision: `accepted`
- Actor: `human product owner`
- Timestamp: `2026-08-24`
- Conditions: `SQLCipher Community Edition with required attribution; local-only v1.2; retain all other containment and security controls. No cloud, hardware, installer, release, or destructive data operation is approved by this decision.`
