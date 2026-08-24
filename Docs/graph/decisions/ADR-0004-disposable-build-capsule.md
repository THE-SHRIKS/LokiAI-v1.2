# ADR-0004 — Disposable per-job VHDX build capsule

Status: `accepted for implementation`
Date: `2026-08-24`  
Decision owners: `human architecture/security owner; orchestrator`  
Supersedes: rejected `BLD-001-CORR-001-R1` workspace ACL design

## Context

The current executor grants each unique AppContainer SID traverse access on
every ancestor of source, toolchain and output paths, including the existing
volume root. Real attempts on `L:\` take 40–121+ seconds and the hostile fixture
never reaches child launch. R1 also prohibits modifying existing-drive roots.
Timeout tuning cannot resolve this architecture contradiction.

## Decision

Each build receives a newly created, job-exclusive disposable NTFS VHDX capsule
managed by a narrowly privileged signed LokiAI Build Workspace Broker. All
AppContainer-visible executor, immutable source, digest-verified toolchain,
scratch and quarantined output paths reside inside the capsule.

Only capsule-internal ACLs may be changed. No per-job ACE may be added to or
removed from an existing host-volume root, repository, user profile or installed
toolchain. Each job uses a unique AppContainer SID; no `ALL APPLICATION
PACKAGES` or shared broad group receives access.

The broker protocol is private, versioned, authenticated and typed. It accepts
only job identity, an approved sealed-base digest, bounded staged input and
resource limits. It accepts no arbitrary host path, command, executable, URL or
environment assignment.

AppContainer, low integrity, empty network capabilities, explicit inherited
handles, Job Object kill-on-close/no-breakaway/resource limits, pre-creation
Win32k disable, suspended-child attestation, typed recipes, immutable inputs,
external artifact validation, provenance, fail-closed startup, crash recovery
and teardown remain mandatory. There is no unrestricted, mock, simulated or VM
fallback.

## Required recovery behavior

Persist capsule/job/base/volume/SID/mount/lease identity before mutation. On any
uncertain liveness or cleanup result, kill/quarantine rather than reuse. A pool
or capsule is never shared concurrently. Journal creation/reconciliation failure
fails closed. Production provisioning uses native signed Windows APIs.

## Verification

`BLD-001-R2-HOST-VERIFY` must pass real existing-root before/after DACL hashes; concurrent
cross-job denial; source/toolchain read-only and output-only write; host secret,
database, pipe, registry, signing, USB/serial and arbitrary-file denial; path/
reparse/mount/volume substitution; socket/DNS denial; hostile Win32k/UI calls;
Job/handle/resource limits; provisioning fault injection; crash/reboot recovery;
zero residue; external artifact validation; supported-Windows L8 matrix; and an
independent security review with no critical/high finding.

## Human acceptance

- Decision: `accepted by human architecture/security owner on 2026-08-24`
- Required approval phrase: `I approve BLD-ARCH-ADR-001 Design A disposable
  per-job VHDX capsule with all retained containment controls and no
  VM/unrestricted fallback.`
- Binding execution conditions: no changes to `C:\` ACLs or system paths; no
  Windows-account, group, UAC, registry, administrator-level, or host-policy
  changes; worktrees and temporary storage remain on `L:\`; and every elevated
  operation or VHDX create/attach/mount/detach action requires a separate,
  exact-operation human approval before execution. These conditions narrow the
  accepted design and do not weaken any containment control above.
