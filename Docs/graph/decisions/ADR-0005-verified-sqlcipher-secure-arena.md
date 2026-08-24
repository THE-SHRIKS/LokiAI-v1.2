# ADR-0005 — Verified SQLCipher Secure Arena on Windows

Status: `accepted for implementation; operational validation separately gated`
Date: `2026-08-24`  
Decision owner: human product/security owner

## Context

The exact bundled SQLCipher initializes memory security OFF. Enabling it installs
a process-global SQLite allocator wrapper whose Windows lock failures are
non-fatal: `VirtualLock` may fail with `ERROR_WORKING_SET_QUOTA` while allocation
continues and the PRAGMA still reports enabled. The required recover-then-rotate
fault lifecycle reproduced error 1453 followed by `STATUS_STACK_OVERFLOW`.

Microsoft documents that `VirtualLock` is bounded by the process minimum working
set minus overhead and that `QueryWorkingSetEx` exposes the Locked bit. The
bundled failure path also provides a high-confidence, not debugger-proven,
recursive warning hypothesis because Windows stderr formatting re-enters the
SQLCipher allocator after a failed lock.

## Decision

Use one fixed, page-aligned SQLite secure arena owned by the LokiAI runtime:

1. Before any SQLite symbol use, reserve/commit a fixed arena and apply only a
   bounded, non-persistent current-process working-set policy.
2. Lock the arena once with `VirtualLock` and attest every page Locked using
   `QueryWorkingSetEx`.
3. Register a strict `sqlite3_mem_methods` allocator constrained to that arena,
   with no growth or system-heap fallback.
4. Build the same pinned SQLCipher reproducibly with `OMIT_MEMLOCK` to remove
   its unsafe duplicate per-allocation lock/unlock calls.
5. Keep `PRAGMA cipher_memory_security=ON` so SQLCipher still wipes all SQLite
   allocations. Readiness requires both PRAGMA=1 after a forced allocation and
   locked-arena attestation with fallback/overflow count zero.
6. Arena exhaustion returns `SQLITE_NOMEM` and fails the operation closed.
7. Shutdown closes every connection, calls `sqlite3_shutdown`, wipes the arena,
   unlocks it, frees it, and ends the process-scoped working-set policy.

Exact arena, working-set, connection and cache bounds are acceptance evidence
derived from real supported-host measurement; they are not guessed in this ADR.

## Alternatives rejected for Part One

- Dedicated storage-host process: stronger isolation but a new locked IPC/process boundary and broad persistence refactor.
- Downstream SQLCipher memlock patch: viable but creates permanent C fork/CVE/rebase burden.
- Vendor build: conditionally viable but no current community build was verified to fail closed on Windows locks.
- PRAGMA-only checks, logging suppression, connection recycling, VM-only containment, disabling memory security or a simple unverified working-set increase: insufficient.

## Required acceptance evidence

- Standard-user Windows 10/11 real lock/Locked-bit evidence before key/DB open.
- Real quota failure refuses startup without recursion or database/key access.
- Tiny-arena exhaustion produces `SQLITE_NOMEM`, rollback and zero fallback.
- Full recover/rotate fault matrix, churn/soak and all workspace SQLite consumers together.
- Reproducible source/build digest, `OMIT_MEMLOCK` proof, signature/SBOM, packaging and performance budgets.
- Independent `DAT-002-MEM-REV-001`; then full DAT review and second-identity matrix.

## Human decision

Accepted by reference by the human product/security owner on 2026-08-24 after
the exact decision from `Docs/graph/handoffs/DAT-002-MEM-ADR-001-1.md` had been
presented. The later approval added the binding execution conditions below.

Binding execution conditions: no changes to `C:\` ACLs or system paths; no
Windows-account, group, UAC, registry, administrator-level, or host-wide policy
changes; worktrees and temporary storage remain on `L:\`; and any elevated or
VM operation requires a separate exact-operation approval. A working-set change
is permitted only inside the current LokiAI process as already bounded by this
ADR; it must be non-persistent and must not change host-wide policy.
