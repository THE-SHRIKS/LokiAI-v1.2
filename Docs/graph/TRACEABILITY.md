# Requirements Traceability — LokiAI Main v1.2

Status: **Operational; update with every accepted behavior change**  
Updated: **2026-08-15**

## 1. Rules

- Every PRD requirement maps to at least one architecture component, task node,
  and verification item before release.
- A task may serve multiple requirements; every relationship remains explicit.
- `planned` is not passing evidence.
- Release blockers have no `unmapped` or `not tested` state at G6.
- Evidence links identify an immutable revision/run/artifact where possible.

## 2. Initial matrix

| Requirement(s)                                 | Architecture owner                                               | Task nodes                                                       | Primary acceptance/evidence                                                       | State                       |
| ---------------------------------------------- | ---------------------------------------------------------------- | ---------------------------------------------------------------- | --------------------------------------------------------------------------------- | --------------------------- |
| FR-AUTH-001, FR-AUTH-002, FR-AUTH-003          | Electron main, secret store, cloud auth client                   | DSK-002, DAT-001, AUTH-001, SEC-001                              | AS-001, AS-002; L3–L5/L7                                                          | blocked OQ-002              |
| FR-ONB-001, FR-WS-001, FR-WS-002               | Desktop application service, cloud client, runtime projection    | CLD-001, AUTH-001, ONB-001, UI-001, UI-002                       | onboarding resume/branch, role expiry/revocation E2E                              | blocked OQ-002              |
| FR-DEV-001, FR-DEV-002, FR-DEV-003             | Runtime device registry, connectors                              | CON-001, CON-002, UI-002                                         | AS-003, AS-004; connector conformance                                             | planned                     |
| FR-DEV-004                                     | Enrollment/cloud authority, runtime device registry              | CLD-001, ENR-001, UI-002                                         | AS-011 replay/wrong-workspace/runtime tests                                       | blocked OQ-002              |
| FR-CON-001                                     | Connector SDK                                                    | CTR-001, CON-001                                                 | conformance and boundary tests                                                    | planned                     |
| FR-CON-002                                     | ESP32-S3 connector                                               | CON-003, OPS-001, HIL-001                                        | physical reference-board suite                                                    | blocked OQ-001              |
| FR-CON-003                                     | Capability projection/policy/UI                                  | CON-001, CON-002, UI-002                                         | unsupported/capability negative cases                                             | planned                     |
| FR-AGT-001, FR-AGT-002                         | Agent registry, MCP adapter, Agents UI                           | MCP-001, MCP-002, AGT-001, UI-003                                | client configuration and run lifecycle                                            | planned                     |
| FR-AGT-003                                     | Work-intent delivery coordinator/provider adapters               | AGT-001, AGT-002, NOT-001, UI-003                                | AS-015 acknowledgement and user-action fallback                                   | planned                     |
| FR-MCP-001, FR-MCP-002, FR-MCP-003, FR-MCP-004 | MCP server, local IPC, runtime services                          | CTR-001, MCP-001, MCP-002, MCP-003, SEC-002                      | AS-005, AS-006, AS-007; protocol/security tests                                   | planned                     |
| FR-MON-001, FR-MON-002, FR-MON-003             | Runtime event pipeline/rules, Monitoring UI                      | MON-001, MON-002, UI-004                                         | loss/freshness/rule/runtime-state tests                                           | planned                     |
| FR-INC-001, FR-INC-002, FR-INC-003             | Incident/evidence domain                                         | INC-001, DEP-001, UI-004                                         | AS-008; verification-window tests                                                 | planned                     |
| FR-NOT-001                                     | Runtime/cloud notification service, Electron main, action center | CLD-002, NOT-001, UI-003, UI-004                                 | AS-014/AS-015; dedup/privacy/deep-link tests                                      | planned                     |
| FR-CLD-001, FR-CLD-002, FR-CLD-003             | Cloud contract/client, runtime sync and heartbeat                | CLD-001, CLD-002, DAT-001, RUN-003, UI-004, UI-005               | AS-014; contract/revocation/reconnect/dedup tests                                 | blocked OQ-002              |
| FR-BLD-001, FR-BLD-002                         | Contained build executor, job engine, artifact validation        | BLD-001, ART-001, MCP-003, UI-005, SEC-002                       | AS-012/AS-013; L8 containment/provenance                                          | planned                     |
| FR-ART-001                                     | Artifact registry, connector capability, deployment domain       | ART-001, CON-001, OPS-001, DEP-001, UI-005, HIL-001              | type/bundle/resource/compatibility/substitution and physical apply/rollback tests | planned                     |
| FR-JOB-001                                     | Durable job engine/storage                                       | DAT-001, JOB-001, RUN-003, UI-005                                | AS-009/AS-010; crash/reconcile tests                                              | planned                     |
| FR-APR-001, FR-APR-002                         | Policy/approval domain                                           | APR-001, MCP-003, UI-003, UI-005                                 | AS-006/AS-007; binding/race tests                                                 | planned                     |
| FR-DEP-001, FR-DEP-002, FR-DEP-003             | Artifact/deployment/connector jobs                               | OPS-001, DEP-001, UI-005, HIL-001                                | AS-006, AS-008, AS-009                                                            | planned                     |
| FR-AUD-001                                     | Activity event store/cloud sync/UI                               | DAT-001, AGT-001, JOB-001, UI-005                                | append/dedup/redaction/activity E2E                                               | planned                     |
| FR-SET-001                                     | Settings projections and policy clients                          | AUTH-001, AGT-001, MON-002, UI-005                               | role/permission route E2E                                                         | planned                     |
| FR-UI-001, FR-UI-002                           | React renderer and typed preload clients                         | DSK-002, ONB-001, UI-001, UI-002, UI-003, UI-004, UI-005, UI-006 | all-route packaged desktop E2E                                                    | planned                     |
| NFR-SEC-001                                    | Electron main/preload                                            | DSK-001, DSK-002, SEC-001                                        | Electron security matrix                                                          | planned                     |
| NFR-SEC-002                                    | Secret store/auth/runtime                                        | AUTH-001, RUN-002, SEC-001                                       | redaction/storage/rotation tests                                                  | planned                     |
| NFR-SAFE-001                                   | Policy, job, connector, deployment                               | CON-001, JOB-001, APR-001, OPS-001, SEC-002, HIL-001             | wrong-target/interruption/approval evidence                                       | planned                     |
| NFR-REL-001, NFR-REL-002                       | Runtime lifecycle/job engine                                     | RUN-003, JOB-001, P1-INT-001, INT-001                            | AS-009/AS-010; retry/idempotency                                                  | planned                     |
| NFR-DATA-001                                   | SQLite/migrations                                                | DAT-001, PKG-002                                                 | migration/recovery/update tests                                                   | planned                     |
| NFR-DATA-002                                   | SQLCipher storage and OS-protected key service                   | DAT-002, SEC-001, PKG-001, PKG-002                               | AS-016; wrong-key/rotation/backup/service-user tests                              | blocked OQ-007              |
| NFR-OBS-001                                    | Contracts/runtime/activity                                       | CTR-001, RUN-001, DAT-001, P1-INT-001, INT-001                   | correlation/redaction/diagnostics                                                 | planned                     |
| NFR-PERF-001, NFR-PERF-002                     | Desktop/runtime streams                                          | DSK-001, MON-001, UI-001, UI-004, UI-006, P1-INT-001, INT-001    | startup/backpressure/soak results                                                 | planned                     |
| NFR-COMP-001                                   | Shared contracts/release updater                                 | CTR-001, RUN-002, MCP-001, PKG-002                               | compatibility matrix                                                              | planned                     |
| NFR-PORT-001                                   | Domain/runtime/connector boundaries                              | CTR-001, RUN-001, CON-001                                        | architecture and platform-neutral tests                                           | planned                     |
| NFR-PRIV-001                                   | Evidence/retention/redaction                                     | MON-001, INC-001, DAT-001, SEC-002                               | privacy/retention/export tests                                                    | blocked OQ-005 for defaults |
| NFR-UX-001                                     | Existing UI/design system                                        | DSK-001, UI-001, UI-002, UI-003, UI-004, UI-005, UI-006          | visual/interaction comparison and accessibility                                   | planned                     |
| NFR-PLAT-001                                   | Foundation, packaging, release manifest                          | FND-002, PKG-001, PKG-002, REL-001                               | supported Windows/toolchain/install/update matrix                                 | planned                     |
| NFR-SUP-001                                    | Runtime diagnostics, desktop export, redaction                   | RUN-001, DAT-002, UI-005, UI-006, PKG-002                        | consent preview/redaction/corruption/key-state support bundle tests               | planned                     |

## 3. Evidence update format

Replace `planned` with a concise evidence reference, not prose:

```text
accepted @ <revision>
tests: <run IDs or protected links>
review: <handoff/review record>
hardware: <operator record and artifact digest when required>
```

If an accepted change adds a requirement, add its traceability row in the same
change before implementation dispatch.
