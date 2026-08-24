# Worker Handoff — `DAT-002-MEM-ADR-001`

Result: `COMPLETE`  
Worker: `read-only storage security/architecture auditor`  
Started from: `f34d19f`  
Result revision/patch: `orchestrator documentation integration`  
Completed at: `2026-08-24T17:00:00Z`

## 1. Outcome

The read-only audit proved that the bundled SQLCipher memory-security PRAGMA is
not an OS-lock attestation and that Windows lock failure can silently degrade
while allocation continues. Four viable designs were compared. ADR-0005
proposes the smallest coherent production correction: one fixed, verified,
no-fallback secure arena inside the existing runtime. No code, dependency,
setting, ACL, database or device changed.

## 2. Key evidence

- Bundled `sqlite3.c` initializes `sqlcipher_mem_security_on=0`; the query is
  `on && executed`, not a `VirtualLock` success check.
- Bundled Windows mlock returns void and allocation continues after lock failure.
- Microsoft documents `ERROR_WORKING_SET_QUOTA`, `VirtualLock` working-set bounds,
  and the `QueryWorkingSetEx` Locked bit.
- Cargo feature unification makes the bundled SQLCipher allocator process-global
  across all rusqlite consumers; no repository `sqlite3_shutdown` owner exists.
- The observed 1453 then stack-overflow sequence is real; recursive logging is a
  high-confidence source-derived hypothesis pending debugger confirmation.

Primary references:

- https://www.zetetic.net/sqlcipher/sqlcipher-api/#pragma-cipher_memory_security
- https://learn.microsoft.com/windows/win32/api/memoryapi/nf-memoryapi-virtuallock
- https://learn.microsoft.com/windows/win32/api/psapi/ns-psapi-psapi_working_set_ex_block
- https://learn.microsoft.com/windows/win32/api/memoryapi/nf-memoryapi-setprocessworkingsetsize
- https://sqlite.org/c3ref/config.html

## 3. Recommended implementation node

`DAT-002-MEM-IMPL-001` followed by independent `DAT-002-MEM-REV-001`, with the
state machine and real Windows matrix specified in ADR-0005. Existing partial
H1-H8 DAT corrections remain isolated and must be rebased onto the accepted
secure-memory implementation; none is accepted by this decision audit.

## 4. Exact human decision wording

> I approve DAT-002-MEM Design A for implementation and real Windows/VM
> validation: LokiAI may replace SQLCipher's unsafe per-allocation Windows
> memlock calls with one fixed, page-aligned SQLite secure arena that LokiAI
> locks and verifies before any database or key is opened, while keeping
> `PRAGMA cipher_memory_security = ON` for full SQLCipher allocation wiping.
> The arena must have no heap fallback or runtime growth; quota, lock,
> attestation, or arena-exhaustion failure must make storage fail closed. Any
> working-set adjustment is limited to the current LokiAI runtime process, must
> be explicit, bounded, observable, and non-persistent. This approval permits
> the repository-controlled `OMIT_MEMLOCK` SQLCipher build flag and
> secure-allocator bootstrap, but does not approve disabling memory security,
> replacing SQLCipher, changing database format, weakening DPAPI/ACL controls,
> or changing host-wide policy. Before integration, return the exact arena
> bytes, working-set min/max bytes, connection/cache caps, supported-Windows
> matrix, source/build digest, real VirtualLock/QueryWorkingSetEx evidence,
> fault/churn/soak results, and an independent DAT-002-MEM-REV-001 security
> review for final acceptance.
