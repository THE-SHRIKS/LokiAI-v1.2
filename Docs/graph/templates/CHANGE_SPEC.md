# Change Specification — `<CHANGE-ID> <Title>`

Status: `draft | review | accepted | rejected | superseded`  
Requested by: `<actor>`  
Date: `<YYYY-MM-DD>`

## Problem or opportunity

What observed user, production, security, device, or engineering evidence requires
change? Link incident/metric/feedback where available.

## User-visible outcome

Describe what becomes true without prescribing implementation.

## Scope

### In scope

- `<item>`

### Out of scope

- `<item>`

## Requirement changes

- Add: `<new stable IDs and text>`
- Amend: `<ID, old intent, new intent>`
- Deprecate: `<ID and replacement>`

Accepted requirement IDs are never silently renumbered.

## Acceptance scenarios

```text
Given ...
When ...
Then ...
And failure/permission/retry behavior ...
```

## Architecture and security impact

- Processes/trust boundaries:
- Contracts/data/migrations:
- Authorization/approval/device safety:
- Compatibility:
- ADR required: `yes/no` — reason

## Graph impact

- New/changed nodes:
- Added/removed edges:
- Ownership conflicts:
- Critical-path impact:
- Required human/hardware gates:

## Test and release impact

- Verification layers:
- Regression areas:
- Rollout:
- Rollback:
- Documentation/traceability changes:

## Acceptance

- Product owner:
- Architecture/security owner when applicable:
- Decision and timestamp:

