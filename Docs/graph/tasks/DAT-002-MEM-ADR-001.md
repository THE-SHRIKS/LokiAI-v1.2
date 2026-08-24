# Task Packet — `DAT-002-MEM-ADR-001 Windows SQLCipher Secure Memory`

Packet status: `accepted`  
Graph status: `ready`  
Created by: `Codex orchestrator`  
Created at: `2026-08-24T16:30:00Z`

## 1. Objective

Determine the smallest production-safe design that keeps SQLCipher memory
security verifiably enabled on supported Windows hosts without `VirtualLock`
quota exhaustion, stack overflow, hidden process-wide resource mutation, or a
false claim that the bundled library defaults memory security on.

## 2. Traceability

- Requirements: `NFR-DATA-002`, `NFR-SEC-002`, `NFR-REL-001`
- Parent: `DAT-002-CORR-001`; review: `DAT-002-REV-002`
- Scenario: `AS-016`

## 3. Dependencies and inputs

- Reproducer: create -> fault after recovery-pair work -> recover -> next rotation.
- Bundled source: `libsqlite3-sys 0.38.2` SQLCipher amalgamation.
- Observed Windows failure: `VirtualLock` error 1453 followed by stack overflow.

## 4. Required reading

- `AGENTS.md`
- `Docs/ARCHITECTURE.md`, `Docs/SECURITY.md`, `Docs/TESTING.md`
- DAT packets, handoffs, reviews 001/002, and affected bundled SQLCipher source
- Official Microsoft process working-set/`VirtualLock` documentation and official SQLCipher guidance/source

## 5. Ownership

Writable:

- `Docs/graph/handoffs/DAT-002-MEM-ADR-001-1.md`
- one proposed decision under `Docs/graph/decisions/`

All code, dependencies, Windows settings, ACLs, databases and devices are read-only.

Forbidden: disabling `cipher_memory_security`, claiming it defaults on when the
bundled source initializes it off, changing host working-set policy, replacing
SQLCipher, or patching vendored source during this audit.

## 6. Deliverables

1. Source-traced root cause, including whether stack overflow follows recursive failure logging, leaked secure allocations, connection churn, or quota policy.
2. At least three viable options with security, privilege, lifecycle, crash, packaging, support and performance trade-offs.
3. Recommended bounded design and exact implementation/test packet.
4. Exact human decision wording for any process-resource, dependency/build, or security-boundary change.

## 7. Acceptance criteria

- [ ] Bundled default/off and enable/query semantics are proven from exact source.
- [ ] Reproducer and Windows error meaning are recorded without fabricated evidence.
- [ ] Recommended design retains verified ON behavior and fails closed.
- [ ] Long-lived runtime, connection churn, crash recovery, packaging and supported-host bounds are addressed.
- [ ] No host or repository mutation occurs during the audit.

## 8. Required verification

Read-only source inspection, official primary-source cross-check, exact file/line
evidence, and independent threat/operations reasoning. No mock result may be
used as proof that Windows memory locking works.

## 9. Risks and safety

This audit authorizes no resource-limit change, dependency change, database
operation, credential access, or device action.

## 10. Human gate

Required after options are produced and before implementation of any selected
resource-policy, SQLCipher-build, or process-isolation change.

## 11. Worker and reviewer

Accountable worker: read-only security/architecture auditor. Independent implementation reviewer assigned after a design is accepted and built.

## 12. Stop conditions

Return `NEEDS_REPLAN` if every viable design requires weakening memory security
or an unbounded host-wide policy. Return `BLOCKED_DEPENDENCY` if exact official
or bundled-build evidence cannot be obtained.
