# Task Packet — `BLD-001-R2-CODE Disposable Build Capsule Code`

Packet status: `accepted`  
Graph status: `ready`  
Created by: `Codex orchestrator`  
Created at: `2026-08-24T18:10:00Z`  
Starting revision: `0bd4a7041f3d01c5b419feccc0d48c3198917ac4`

## 1. Objective

Implement the unprivileged source and contracts for ADR-0004's job-exclusive
disposable VHDX build capsule: a private authenticated typed broker protocol,
strict lifecycle journal/state machine, native Windows broker/executor boundary,
and hostile fixtures. This node may compile the native API path but must not
invoke VHDX lifecycle APIs, elevate, install, sign, or mutate host state.

## 2. Traceability

- Requirements: `FR-BLD-001`, `FR-BLD-002`, `NFR-SAFE-001`
- Acceptance scenarios: `AS-012`, `AS-013`
- Architecture: separate contained build executor and signed workspace broker
- Security: AppContainer, Job Object, Win32k disable, no network/device/secret authority
- Parent: rejected `BLD-001-CORR-001-R1` design evidence
- Decision: accepted ADR-0004 / `BLD-ARCH-ADR-001`

## 3. Dependencies and inputs

- `depends_on`: `BLD-ARCH-ADR-001`, `JOB-001`, `CON-001`, `RUN-002`
- Consumed decision: ADR-0004 at starting revision
- The dirty `L:\LokiAI-v1.2-wt\BLD-001` checkout is read-only salvage evidence;
  none of its code, claims, or tests are accepted by inheritance
- Environment: standard-user Windows and existing pinned toolchain; worktree,
  `TEMP`, `TMP`, fixtures, and evidence remain on the assigned `L:` task root
- No signer, VHDX, elevation, VM, device, or production credential is required

## 4. Required reading

- `AGENTS.md`
- `Docs/ARCHITECTURE.md`, `Docs/SECURITY.md`, `Docs/BUILD_EXECUTION.md`
- `Docs/TESTING.md`, ADR-0004, all BLD packets/handoffs/review findings
- accepted job, connector, runtime, and recipe contracts
- old BLD source only as untrusted salvage input

## 5. Ownership

Writable paths:

- `crates/loki-build-executor/**`
- root `Cargo.toml` and `Cargo.lock`, limited to registering that crate and its
  strictly necessary existing/pinned dependencies
- focused build fixtures owned by the crate
- `Docs/graph/handoffs/BLD-001-R2-CODE-1.md`
- `Docs/graph/evidence/W5/BLD-001-R2-CODE/**`

Read-only dependency paths:

- all other crates and product documents
- `L:\LokiAI-v1.2-wt\BLD-001/**`

Forbidden scope:

- invoking create/attach/mount/format/detach VHDX or disk-management APIs
- elevation, UAC prompt, service/task installation, binary signing, signer or
  production credential access, VM execution, or any host mutation
- C-drive ACL/system-path changes; Windows account/group/UAC/registry/admin/
  host-policy changes; ACL changes on any existing volume/repository/profile/toolchain
- arbitrary path, command, executable, URL, environment, or shell inputs
- shared broad access groups, network/device/USB/serial capability, mock success,
  unrestricted fallback, or weakening retained AppContainer/Job/Win32k controls
- registering an artifact before external validation or touching unrelated code

No worker may expand ownership without orchestrator acceptance.

## 6. Deliverables

1. Bounded, versioned, authenticated broker protocol accepting only job ID,
   sealed-base digest, bounded staged-input identities, and resource limits.
2. Deterministic lifecycle state machine and durable journal for create through
   quarantine/reconcile, with stale/wrong-volume/wrong-digest rejection.
3. Native Windows broker and executor code boundaries with injectable low-level
   API adapters so unit/contract tests never perform host operations.
4. Capsule-internal ACL plan for a unique per-job AppContainer SID and immutable
   source/toolchain versus output/scratch rights.
5. Retained suspended-child attestation, pre-create Win32k, no-network, explicit
   handles, Job Object limits, typed recipe, provenance, and output validation.
6. Hostile protocol/state/path/substitution/concurrency/fault fixtures and handoff.

## 7. Acceptance criteria

- [ ] Broker messages reject arbitrary paths, commands, executables, URLs,
      environment assignments, stale jobs, wrong digests, wrong volume identity,
      oversized data, replay, and cross-job identity.
- [ ] Lifecycle transitions are explicit, idempotent where retryable, journaled
      before mutation, and quarantine uncertain state without reuse.
- [ ] Production Windows code uses typed native API adapters; PowerShell is not
      part of the product path.
- [ ] All retained AppContainer, low-integrity, no-network, handle, Job Object,
      Win32k, recipe, provenance, and external-output validation controls remain.
- [ ] Tests prove they invoke no VHDX/disk, elevation, installation, signing,
      device, or host-ACL operation.
- [ ] No existing host path gains an ACE and no broad access group is modeled.
- [ ] Failure/unavailable behavior is explicit and tested.
- [ ] No prohibited security or architecture boundary is weakened.
- [ ] No unrelated files change.

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `cargo fmt --all -- --check` | exit 0 | exact revision/command |
| Static | `cargo clippy -p loki-build-executor --all-targets -- -D warnings` | exit 0 | run log |
| Unit | `cargo test -p loki-build-executor --lib -- --test-threads=1` | protocol, bounds, journal, state, recipes pass | count/summary |
| Contract | `cargo test -p loki-build-executor --test broker_contract -- --test-threads=1` | malformed/stale/oversized/replay/substitution matrix fails closed | fixture matrix |
| Integration | `cargo test -p loki-build-executor --test broker_dry_run -- --test-threads=1` as standard user with `TEMP`, `TMP`, and `CARGO_TARGET_DIR` under the assigned `L:` worktree | deny-all adapter ledger proves no host operation attempted | ledger and run log |
| Containment | n/a in CODE node | reserved for HOST-VERIFY | explicit deferral |
| Security | independent code review | no critical/high finding | review record |
| Workspace | `pnpm run verify` and `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | integrated run |
| Hardware | n/a | no device access | recorded n/a |

Real VHDX, signed-broker, L8, cross-job, fault, reboot, and DACL evidence belongs
only to `BLD-001-R2-HOST-VERIFY`. CODE cannot make BLD-001 production-accepted.

## 9. Risks and safety

- Risk level: `critical`
- Failure modes: privileged confused deputy, replay/substitution, cross-job leak,
  unsafe recovery/reuse, escaped child, false artifact success
- Recovery: code models kill/quarantine-on-uncertainty; no host recovery is run
- Data/secret impact: synthetic bounded fixtures only; no production secrets
- Executable/toolchain impact: existing pinned toolchain; no install/update/signing
- Cloud/offline impact: none
- Device impact: none

## 10. Human gate

- Gate: ADR-0004 implementation acceptance is recorded at the starting revision
- This packet authorizes code/fixture work only
- Every elevation, VHDX lifecycle call, signed-broker provisioning, VM run, host
  ACL mutation, or device action needs a separate exact operation approval
- Approver role: human architecture/security owner
- Approval evidence: ADR-0004 and graph acceptance record

## 11. Worker and reviewer

- Accountable worker: isolated build-containment code implementer
- Independent reviewer: `BLD-001-R2-CODE-REV-001` security reviewer
- Worktree/branch: `L:\LokiAI-v1.2-wt\BLD-001-R2-CODE` / `codex/bld-001-r2-code`
- Handoff: `Docs/graph/handoffs/BLD-001-R2-CODE-1.md`

## 12. Stop conditions

Return `BLOCKED_DEPENDENCY` or `NEEDS_REPLAN` if safe code requires any host
operation, prohibited system change, arbitrary broker authority, control
weakening, unsupported dependency, or if the old implementation cannot be
salvaged without importing its rejected existing-volume ACL design.
