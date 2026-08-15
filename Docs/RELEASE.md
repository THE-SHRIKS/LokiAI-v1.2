# Release, Packaging, and Rollback — LokiAI Main v1.2

Status: **Normative draft**  
Updated: **2026-08-15**

## 1. Release unit

A LokiAI Main release is a compatible, signed set:

- Electron desktop application;
- Rust runtime in the selected host mode(s);
- contained build-executor binary and accepted sandbox profiles;
- LokiAI MCP server and provider configuration helpers;
- connector packages/tool manifests included in the release;
- database migrations;
- protocol/schema versions;
- third-party notices and SBOM;
- installer/update metadata;
- release notes and known-risk record.

Versioning one UI binary while silently changing the runtime or connector is not a
valid release.

## 2. Version policy

- Product releases use semantic versioning.
- Protocols, database schema, connectors, MCP tools, and artifact formats expose
  their own compatibility versions.
- Release manifests map every component version and digest.
- A release candidate is immutable. Any changed byte creates a new candidate.
- Pre-release channels are isolated from production channels and signing policy.

## 3. Windows packaging requirements

Final installer technology is decided by `OQ-004`, but every option must:

- be code-signed with the LokiAI publisher identity;
- install exact desktop/runtime/MCP component digests;
- register protocol/callback and startup/service behavior explicitly;
- request elevation only for actions that require it;
- configure least-privilege ACLs for data, named pipes, and executables;
- support repair and safe uninstall;
- preserve or remove user data according to an explicit user choice/policy;
- write diagnosable, redacted installer logs;
- verify prerequisites and report actionable failures;
- resist path hijacking and untrusted binary replacement.

For the direct-installer baseline, Electron Forge packages the signed desktop and
WiX Burn installs the compatible desktop/runtime/MCP/build-executor component set.
The production path does not depend on Electron Forge's experimental Vite plugin.
If OQ-004 selects Store-only or dual distribution, an ADR records how service,
protocol callback, updates, and component atomicity remain satisfied.

## 4. Update requirements

- Update manifest and payload signatures are verified before apply.
- Updates use HTTPS but do not rely on transport security alone.
- Desktop/runtime/MCP/connector compatibility is checked before apply.
- Device-changing jobs block update at unsafe stages; safe queued jobs are paused
  or drained according to policy.
- Database backup and migration plan are created before irreversible update.
- Failed or interrupted update recovers to the last coherent component set.
- Runtime reports update state to desktop and activity log.
- The user sees version, channel, expected restart/downtime, and release notes.

## 5. Product rollout

v1.2 progression:

1. internal development builds;
2. physical reference-board validation;
3. internal signed release candidate;
4. controlled friends/device cohort with explicit diagnostics consent;
5. correction and repeated release verification;
6. human production release decision.

Promotion reuses the exact tested digests. Rebuilding for promotion requires
equivalent provenance and a new candidate record.

## 6. Rollback

### Application rollback

- Determine whether database/schema state remains compatible.
- Do not roll back binaries across an irreversible migration without a validated
  restore path.
- Preserve audit and diagnostic evidence.
- Revoke a compromised update through control-plane policy.
- Record actor, reason, versions/digests, affected installations, and result.

### Device deployment rollback

- Rollback plan is target/connector/artifact-specific and approved before apply.
- Known-good artifact digest and compatibility are verified.
- Automatic rollback occurs only at predefined policy conditions.
- A failed rollback enters explicit recovery/quarantine; it never loops forever.
- Incident remains open until recovery verification passes.

## 7. Release checklist

### Product and traceability

- [ ] G0 product/spec decisions accepted.
- [ ] All v1.2 release requirements map to accepted tasks and passing evidence.
- [ ] Open questions are resolved or explicitly out of release scope.
- [ ] UI route and onboarding acceptance pass.

### Build and supply chain

- [ ] Clean reproducible build from release revision.
- [ ] Lockfiles and pinned toolchains present.
- [ ] Dependency/license/security scans pass.
- [ ] SBOM and component manifest generated.
- [ ] Artifacts/installers/update metadata signed and verified.
- [ ] No secrets or customer data in package/evidence.
- [ ] Exact supported Electron/React/Vite/TypeScript/Node/Rust/MCP/SQLite and
  connector-toolchain matrix is recorded; no production prerelease or unsupported
  build line is present.
- [ ] Contained build executor denies secret/IPC/network/device/unrelated-file
  access and validates quarantined output before registration.

### Security and safety

- [ ] Electron security baseline passes.
- [ ] Auth, credentials, local IPC, cloud client, and cross-workspace tests pass.
- [ ] Device enrollment, runtime heartbeat, cloud sync/deduplication,
  notifications, and incident work-intent delivery/fallback pass.
- [ ] MCP scope/side-effect/prompt-injection tests pass.
- [ ] Approval binding and device lease/reconciliation tests pass.
- [ ] Independent security reviews pass.
- [ ] Residual risks have named human acceptance.

### Reliability and data

- [ ] Database migration/backup/integrity/recovery pass.
- [ ] Database, WAL, backups, temporary data, key rotation/loss, and service/user
  identity behavior pass data-at-rest tests.
- [ ] Runtime restart, window closure, offline/reconnect, and backpressure pass.
- [ ] Installer, repair, uninstall, update, failed-update recovery pass.
- [ ] Support bundle is usable and redacted.

### Hardware

- [ ] Exact reference board identity is recorded.
- [ ] Connector conformance passes.
- [ ] Discovery/health/logs pass on physical board.
- [ ] Approved build/flash/restart/test/observe passes.
- [ ] Wrong-target and interrupted-operation recovery pass.
- [ ] Rollback/recovery and incident closure pass.

### Human gate

- [ ] Product owner accepts behavior/scope.
- [ ] Security owner accepts security evidence/residual risk.
- [ ] Hardware operator accepts physical evidence.
- [ ] Release owner approves exact manifest and digests.

## 8. Release decision record

```text
release:
revision:
manifest_digest:
installer_digest:
component_versions:
evidence_index:
known_issues:
residual_risks:
rollback_plan:
product_approval:
security_approval:
hardware_approval:
release_approval:
decision_timestamp:
```

Publishing or deployment happens only after this record is complete and the human
release owner explicitly authorizes it.
