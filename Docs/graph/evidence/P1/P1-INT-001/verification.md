# P1-INT-001 revised local Part One gate

Result: `INTEGRATED_PASS` / task result `COMPLETE`  
Tested baseline: `333209ea7d674d1cd397164ebafd66ef5c63ef36`  
Completed: `2026-08-24T19:14:22+05:30`

## Scope verdict

The non-UI, local-only Part One baseline authorized by ADR-0002 passes its
runnable integration gate. This is not `SEC-001`, `SEC-002`, packaging, release,
cloud, contained-build, exact-board, flash, device-operation, or deployment
certification. Part Two has not started.

## Dependency audit

Accepted and integrated gate dependencies: `LOC-001`, `LDEV-001`, `LONB-001`,
`AGT-001`, `APR-001`, `LNOT-001`, and `AGT-002`, plus their accepted local
predecessors.

`DAT-002` remains `BLOCKED_DEPENDENCY`, not accepted. Its production current-user
SQLCipher/DPAPI implementation and every local Part One store integration pass,
but the mandatory second Windows/service identity DPAPI environment and an
independent data-security reviewer remain unavailable.

## Verification results

- Final `pnpm run verify` on the merged baseline: exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0.
- Contracts: 303/303 tests pass.
- MCP server: 85/85 tests pass, including current protocol, bounded previous
  adapter, handshake, capabilities, malformed requests, scopes, and tools.
- Electron main/preload: 77/77 tests pass, including sender checks, CSP,
  navigation/permission denial, bounded API validation, and subscription cleanup.
- Complete Cargo workspace build/tests and doctests: pass.
- Existing renderer lint baseline: 4 warnings, 0 errors; no renderer file changed.

The focused local-services command initially received Windows `Access is denied`
for three real named-pipe cases under the sandbox. The exact
`cargo test -p loki-ipc --test auth` rerun outside the sandbox passed 4/4, and
the final complete elevated repository verify also passed the entire IPC suite.

## Cross-service matrix

| Scenario | Evidence | Result |
|---|---|---|
| Authority/session expiry and revocation | LOC-001 plus complete workspace rerun | fail closed |
| Device bind/rebind/revoke and restart | LDEV-001 tests | pass |
| Setup resume/back/failure/device/project branches | LONB-001 tests | pass |
| Approval request mutation/expiry/reuse/race/restart | APR-001 tests | pass |
| Agent secret/scope/revocation and owned run | AGT-001/AGT-002 tests | pass |
| Incident intent to agent acknowledgement | AGT-002 real local registries/run | pass |
| Unavailable agent to deduplicated human fallback | AGT-002 plus LNOT-001 stores | pass |
| Notification preference/read/expiry/restart | LNOT-001 tests | pass |
| Encrypted open/restart/wrong key/backup/rotation | DAT-002 and local-store tests | current-user path pass |
| Runtime/IPC current-user ACL/auth/replay/version | real Windows named-pipe tests | pass |

## Contract compatibility matrix

| Producer/consumer | Tested compatibility | Result |
|---|---|---|
| TypeScript shared contracts | current fixtures and negative validation | 303/303 |
| MCP client/server | current plus one supported previous edge adapter | 85/85 |
| Electron renderer/preload/main | current typed methods and malformed inputs | 77/77 |
| Rust runtime/local IPC | version/capability/auth/replay/framing matrix | pass |
| Build executor, cloud, deployment | excluded and unavailable | no fallback claim |

## Security and dependency review

- High-confidence credential scan of tracked sources: 0 hits.
- Cargo offline inventory: 83 external packages, 0 missing licence metadata.
- Installed Node dependency roots: 607 unique name/version records, 0 missing
  licence fields. Licence families were inventoried; CC attribution-bearing
  packages are `caniuse-lite` and `spdx-exceptions`, retained for the later
  packaging/notices gate.
- Cargo duplicate review found only `syn` 2.x/3.x, an existing procedural-macro
  toolchain split; AGT-002 introduced no new version split.
- `pnpm licenses list --json` could not read one pnpm cache index entry, so the
  installed root package manifests were inspected directly. This did not affect
  installation, build, or tests.
- The temporary repository-local `.pnpm-store` created during that inspection
  was path-verified and removed; no user source or project folder was deleted.

## Explicit deferred and blocked register

- Cloud/auth: `CLD-001`, `AUTH-001`, `ONB-001`, `ENR-001`, `CLD-002`, `NOT-001`.
- Contained build chain: blocked `BLD-001`; proposed/dependent `ART-001`,
  `OPS-001`, `MCP-003`, and `DEP-001`.
- Exact physical board: `CON-003` remains blocked on `OQ-001`; no flash or
  hardware mutation was performed.
- Data assurance: DAT-002 second-identity DPAPI evidence and independent review.
- UI, independent security integration, packaging, HIL, and release are Part Two.

These items are retained in the graph, remain fail-closed, and were not
relabelled or silently weakened to pass this gate.
