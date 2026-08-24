# Task Packet — `DAT-002-CORR-001 Fail-safe Encrypted Data Lifecycle`

Packet status: `accepted`  
Graph status: `ready`  
Created by: `Codex orchestrator`  
Created at: `2026-08-24T14:57:00Z`  
Starting revision: `827d2a856af71fbd20e6aaf49fca47d8e9289133`

## 1. Objective

Correct every blocking finding from `DAT-002-REV-001` so encrypted databases,
keys, backups, migrations, rotations, IPC credentials, identity/ACL failures,
and crash recovery always remain recoverable and fail closed without false-empty
success or a generic secret API.

## 2. Traceability

- Requirements: `NFR-DATA-002`, `NFR-SEC-002`
- Acceptance scenarios: `AS-016`
- Architecture: runtime-owned encrypted persistence
- Security: SQLCipher, DPAPI, Windows identity and ACL boundary
- Parent: `DAT-002`

## 3. Dependencies and inputs

- `depends_on`: DAT-002 checkpoint; `DAT-002-REV-001`
- Review: `Docs/graph/evidence/W4/DAT-002/REVIEW-SECURITY-001.md`
- Environment: Windows current-user implementation tests; separate identity is a
  later mandatory reviewer/environment row, never mocked.

## 4. Required reading

- `AGENTS.md`
- `Docs/ARCHITECTURE.md`, `Docs/SECURITY.md`, `Docs/TESTING.md`
- ADR-0001, DAT-002 packet/handoff/evidence, security review 001
- affected storage/runtime token sources and tests

## 5. Ownership

Writable:

- `crates/loki-storage/**`
- `crates/loki-runtime/src/host/token.rs`
- focused tests under the same crates
- `Docs/graph/handoffs/DAT-002-CORR-001-1.md`

Read-only: all other files. Root manifests/locks return to the orchestrator if needed.

Forbidden: mock identity evidence, plaintext fallback, silent empty creation,
generic arbitrary secret persistence, weaker ACLs, unrelated refactors.

## 6. Deliverables

1. Recoverable atomic database/key-pair migration and rotation with retained,
   verified encrypted recovery backup.
2. Exclusive maintenance locking and deterministic crash recovery.
3. Explicit missing/truncated/wrong/malformed/identity-inaccessible recovery states.
4. Purpose-bound DPAPI APIs, SQLCipher configuration verification, and ACL hooks.
5. Unconditional safe legacy-token cleanup and comprehensive fault tests.

## 7. Acceptance criteria

- [ ] Every critical/high review finding has a discriminating regression test.
- [ ] No missing/truncated database can become healthy-looking empty state.
- [ ] Every crash point preserves at least one matching verified database/key pair.
- [ ] Concurrent writers cannot be silently lost during maintenance.
- [ ] Recovery backup/key retention and cleanup policy is explicit and tested.
- [ ] No public arbitrary byte-secret API remains.
- [ ] SQLCipher/cipher integrity/temp-store configuration is verified.
- [ ] Legacy plaintext IPC token is removed even when protected token exists.
- [ ] Failure behavior is explicit; no security boundary is weakened.

## 8. Required verification

Format, warnings-denied Clippy, all storage/runtime tests, new fault-injection and
concurrency tests, complete workspace verification, secret/plaintext sidecar scan,
and independent `DAT-002-REV-002`. The second-identity matrix remains mandatory
before DAT-002 acceptance.

## 9. Risks and safety

Risk: critical. Never delete the last verified database/key pair. Test only in
fresh temporary directories; do not touch real LokiAI user data.

## 10. Human gate

No implementation gate. Separate identity/VM execution is required before final acceptance.

## 11. Worker and reviewer

Accountable worker: isolated DAT correction worker.  
Independent reviewer: new reviewer under `DAT-002-REV-002`.  
Worktree: assigned after packet integration.

## 12. Stop conditions

Return `NEEDS_REPLAN` if atomic recovery requires deleting/overwriting the last
verified pair or widening the secret/ACL boundary. Return `BLOCKED_DEPENDENCY`
only for the real second-identity environment after all code findings are fixed.
