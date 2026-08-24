# Worker Handoff — `BLD-ARCH-ADR-001`

Result: `COMPLETE`  
Worker: `independent containment architecture agent`  
Started from: `29770b5`  
Result revision/patch: `read-only review; no patch`  
Completed at: `2026-08-24T15:05:00Z`

## 1. Outcome

The per-job ancestor-traverse model necessarily mutates the underlying volume
root and cannot satisfy R1. Two local production designs are viable. The
recommended design is one new job-exclusive disposable NTFS VHDX capsule per
build, managed by a narrow signed broker, with every AppContainer-visible path
inside the capsule.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| No per-job existing-root mutation | pass by design | capsule-only ACL boundary |
| Unique job isolation | pass by design | unique AppContainer SID and volume |
| Retain controls | pass by design | AppContainer/Job/Win32k/network/handles/limits unchanged |
| Explicit provisioning/recovery | pass | create/attach/attest/quarantine/detach stages specified |
| Real hostile evidence required | pass | complete L8 and fault matrix specified |

## 3. Changed files

None by the worker.

## 4. Verification performed

Read-only BLD source, handoff, VHDX verification asset, BUILD_EXECUTION,
SECURITY and TESTING review. Existing evidence shows a fresh disposable `X:`
volume reached 23/24 containment and 10/10 stage tests before the then-open UI
gap; that evidence supports feasibility but is not R2 acceptance.

## 5. Contracts, migrations, and compatibility

R2 requires a private typed broker protocol accepting only job identity, sealed
base digest, bounded staged inputs and limits—never arbitrary paths, commands,
executables or URLs.

## 6. Security, privacy, and device safety

No ACL, device, VM or security setting changed. Journal/provision/reconciliation
failure must fail closed. Uncertain capsules and outputs are quarantined.

## 7. Decisions and assumptions

Production uses native signed Windows code/APIs; PowerShell remains verification
only. VMware is not a substitute for the certified local containment boundary.

## 8. Review focus

Broker privilege minimization, volume identity, mount/reparse substitution,
cross-job denial, crash/reboot recovery and exact existing-root DACL hashes.

## 9. Follow-up graph changes proposed

Human acceptance of ADR-0004, then `BLD-001-R2` implementation and a separate
independent security review.
