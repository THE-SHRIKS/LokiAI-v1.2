# Worker Handoff — `<NODE-ID>`

Result: `COMPLETE | BLOCKED_DEPENDENCY | FAILED_VERIFICATION | NEEDS_REPLAN | CANCELLED`  
Worker: `<identity/role>`  
Started from: `<revision>`  
Result revision/patch: `<revision>`  
Completed at: `<UTC ISO 8601>`

## 1. Outcome

Two to five sentences describing what is now true. Do not repeat the plan.

## 2. Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| `<criterion>` | `pass/fail/blocked` | `<test/file/result>` |

## 3. Changed files

| Path | Change and reason | Ownership compliant? |
|---|---|---|
| `<path>` | `<summary>` | yes/no |

Generated and large artifacts are referenced by digest/location, not pasted.

## 4. Verification performed

| Command/scenario | Environment | Result | Evidence/reference |
|---|---|---|---|
| `<exact command>` | `<versions/host/device>` | `<exit/result>` | `<reference>` |

Mandatory verification not run:

- `<item and reason; “none” if complete>`

## 5. Contracts, migrations, and compatibility

- Contracts changed: `<none or exact schema/version>`
- Consumers affected: `<list>`
- Migration: `<none or details>`
- Backward/rolling compatibility: `<result>`
- Generated-contract drift: `<result>`
- Build recipe/executor/provenance compatibility: `<n/a or result>`
- Cloud API/offline/notification compatibility: `<n/a or result>`

## 6. Security, privacy, and device safety

- Trust boundaries touched: `<list>`
- Authorization/approval impact: `<none/details>`
- Secrets/data handling: `<none/details>`
- Containment profile and denied authorities: `<none/details/evidence>`
- Device side effects performed: `<none or exact approved operation/evidence>`
- New dependencies: `<none or review record>`
- Remaining risk: `<none or list>`

## 7. Decisions and assumptions

- `<decision/assumption with evidence and impact>`

Do not hide an unresolved material assumption here; return `NEEDS_REPLAN` when it
changes the contract or acceptance criteria.

## 8. Review focus

- `<specific codepath, failure mode, or contract the reviewer should inspect>`

## 9. Follow-up graph changes proposed

- `<new node/edge/risk or none>`

Only the orchestrator updates the task graph.
