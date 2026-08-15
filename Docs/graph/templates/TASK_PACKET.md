# Task Packet — `<NODE-ID> <Title>`

Packet status: `draft | accepted | superseded`  
Graph status: `proposed | ready | running | review | blocked | failed | accepted`  
Created by: `<orchestrator>`  
Created at: `<UTC ISO 8601>`  
Starting revision: `<immutable revision>`

## 1. Objective

One paragraph describing the single observable outcome and why it exists.

## 2. Traceability

- Requirements: `<FR-...>`, `<NFR-...>`
- Acceptance scenarios: `<AS-...>`
- Architecture components: `<names/sections>`
- Security controls: `<sections or none>`
- Parent/feature/incident: `<ID or none>`

## 3. Dependencies and inputs

- `depends_on`: `<accepted node IDs>`
- Consumed contracts and exact versions/revisions:
  - `<contract>` — `<version/revision>`
- Required environment/credential/device:
  - `<requirement or none>`
- Known starting conditions:
  - `<fact with file/evidence>`

## 4. Required reading

- `AGENTS.md`
- `<only task-relevant docs and files>`

## 5. Ownership

Writable paths:

- `<exact directory/file/glob>`

Read-only dependency paths:

- `<path>`

Forbidden scope:

- `<specific behavior/files/refactors not authorized>`

No worker may expand ownership without orchestrator acceptance.

## 6. Deliverables

1. `<concrete file/behavior/artifact>`
2. `<tests>`
3. `<documentation/contract update>`
4. `<handoff/evidence>`

## 7. Acceptance criteria

- [ ] `<binary observable criterion>`
- [ ] `<binary observable criterion>`
- [ ] Failure/unavailable behavior is explicit and tested.
- [ ] No prohibited security or architecture boundary is weakened.
- [ ] No unrelated files change.

## 8. Required verification

| Layer | Command/scenario | Expected result | Evidence required |
|---|---|---|---|
| Static | `<existing command>` | exit 0 | command/result |
| Unit | `<existing command>` | required tests pass | run summary |
| Contract | `<existing command or n/a>` | fixtures/compatibility pass | run summary |
| Integration | `<scenario or n/a>` | stated outcome | result reference |
| Desktop | `<scenario or n/a>` | stated outcome | capture/run reference |
| Cloud | `<contract/sync/enrollment/notification scenario or n/a>` | stated outcome | fixture/environment result |
| Containment | `<build/process isolation scenario or n/a>` | denied/allowed behavior matches profile | installed-host evidence |
| Hardware | `<scenario or n/a>` | stated physical outcome | operator record |
| Security | `<check or n/a>` | no blocking finding | review/check result |

If a mandatory row cannot run, the node result cannot be `COMPLETE`.

## 9. Risks and safety

- Risk level: `low | medium | high | critical`
- Failure modes: `<list>`
- Rollback/recovery: `<method>`
- Data/secret impact: `<classification and handling>`
- Executable/toolchain impact: `<none or exact recipe, digest, containment profile>`
- Cloud/offline impact: `<none or contract, retry, freshness, fallback>`
- Device impact: `<none or exact effect>`

## 10. Human gate

- Gate: `<none or gate ID>`
- Exact decision/operation requiring approval: `<scope>`
- Approver role: `<role>`
- Approval evidence: `<filled only after decision>`

## 11. Worker and reviewer

- Accountable worker: `<one identity/role>`
- Independent reviewer: `<identity/role or none>`
- Worktree/branch: `<node-specific location after Git baseline>`
- Handoff record: `<path>`

## 12. Stop conditions

Return `BLOCKED_DEPENDENCY` or `NEEDS_REPLAN` if:

- `<missing decision/contract/environment>`;
- the actual repository contradicts a safety/behavior assumption;
- required work falls outside ownership;
- acceptance cannot be verified without weakening the contract.
