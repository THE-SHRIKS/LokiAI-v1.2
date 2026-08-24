# DAT-002 independent security review 001

Result: `FAILED_VERIFICATION`  
Reviewed baseline: `827d2a856af71fbd20e6aaf49fca47d8e9289133`  
Reviewer: independent data-security review agent  
Date: `2026-08-24`

## Blocking findings

Critical:

1. Rotation can promote a new key and strand the rollback database without its
   old key after post-promotion failure (`managed.rs` replacement sequence).
2. An existing protected key with a missing/truncated database can create and
   migrate a healthy-looking empty database instead of entering recovery.

High:

1. No retained independently recoverable encrypted pre-rotation backup/key pair.
2. No exclusive maintenance lock prevents lost concurrent writes during export/swap.
3. No gateway-service identity path or explicit least-privilege ACL validation.
4. Public generic byte-secret APIs exceed DAT-002's purpose-bound scope.
5. Wrong/missing/malformed/identity-inaccessible key states are not distinct.
6. SQLCipher security configuration is set but not verified with cipher-specific checks.
7. A protected IPC token can coexist with an undeleted legacy plaintext token.

Existing focused tests pass but do not exercise these failures. DAT-002 remains
blocked and must not satisfy a descendant dependency.

## Required second-identity evidence

After correction, run a clean Windows VM or separately authorized service
identity with two distinct non-admin SIDs. Prove owner reopen, cross-identity
DPAPI denial independent of ACL denial, explicit ACL denial, restart survival,
distinct malformed/missing/truncated recovery states, unchanged file hashes, and
no replacement/sidecar creation. Record reviewed revision, executable digest,
OS build, SIDs in redacted form, canonical SDDL, commands, classifications, and
before/after hashes.

No VMware installation, service, process, or `vmrun` executable was discoverable
on the current host, so the environment row remains `BLOCKED_DEPENDENCY`.
