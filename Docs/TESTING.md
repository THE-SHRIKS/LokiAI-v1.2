# Verification Strategy — LokiAI Main v1.2

Status: **Normative**  
Updated: **2026-08-15**

## 1. Principle

LokiAI controls physical devices, so success claims require evidence at the layer
where the behavior matters. Unit tests prove logic; they do not prove a board was
flashed. A screenshot proves rendering; it does not prove authentication,
monitoring, or deployment.

Each task packet names its required layers. Release requires all applicable
layers.

## 2. Verification layers

### L0 — Document and graph consistency

- requirement IDs resolve;
- task dependencies are acyclic and every release requirement maps to work/tests;
- links and referenced paths exist;
- locked decisions are not contradicted;
- task packets have ownership and binary acceptance criteria.

### L1 — Static quality

- formatter and lint;
- TypeScript strict type checks;
- Rust formatting and Clippy with agreed warning policy;
- schema validation and generated-contract drift;
- dependency, license, secret, and unsafe-code policy checks.

### L2 — Unit and property tests

- pure state machines and transition guards;
- authorization and approval canonicalization;
- rule windows, debounce, cooldown, and missing-data behavior;
- job idempotency, retry, cancellation, and lease rules;
- parser/encoder boundary cases;
- retention and redaction;
- connector contract logic that does not require a physical target.

Use property-based tests where state/order/input combinations are large and a
single example is weak.

### L3 — Contract tests

- TypeScript ↔ Rust IPC fixtures;
- desktop/runtime version handshake;
- runtime/MCP errors, resources, tools, approval/job results, `2026-07-28`
  per-request metadata/optional discovery, the accepted previous provider
  revision at the edge, and Tasks-extension/polling fallback;
- cloud API client against accepted contract fixtures;
- runtime/device enrollment challenge, ownership, revocation, heartbeat,
  notification, and provider-delivery fixtures;
- connector conformance suite;
- build recipe, containment-profile, artifact-provenance, and quarantine fixtures;
- event schema compatibility and deduplication;
- database migration compatibility.

Malformed, oversized, stale, unauthorized, repeated, and incompatible inputs are
mandatory cases.

### L4 — Process integration

- Electron main/preload/renderer request and subscription behavior;
- runtime start/stop/restart and desktop discovery;
- MCP stdio client → MCP server → runtime path;
- SQLite durability, outbox retry, event projection rebuild;
- cloud disconnect/reconnect and expiry behavior;
- cloud outbox/inbox deduplication, heartbeat expiry, notification fallback, and
  provider work-intent acknowledgement;
- build executor containment startup, process-tree/resource limits, output
  quarantine, crash/timeout teardown, and runtime reconciliation;
- bounded telemetry/log backpressure;
- job interruption/reconciliation without a physical mutation where applicable.

### L5 — Desktop end-to-end

Drive the installed or production-packaged desktop application through:

- first run and browser auth callback;
- logout and relaunch;
- onboarding branches and resume;
- all eight routes and their loading/empty/disconnected/error/permission states;
- approvals and durable-job status;
- device enrollment ownership, contained-build/provenance, runtime stale/offline,
  notification/deep-link, and incident-delivery fallback state;
- crash-safe relaunch and version-incompatible state;
- keyboard, focus, accessibility, scaling, and window/tray behavior.

Renderer-only browser preview is useful during UI work but is not the release E2E
environment.

### L6 — Physical hardware-in-the-loop

Run against the exact confirmed ESP32-S3 board and recorded connector/toolchain:

- discovery and stable identity;
- disconnect/reconnect and wrong-port/target prevention;
- real health measurements and logs;
- build artifact identity and compatibility;
- approved flash and byte/digest verification where supported;
- restart and re-identification;
- post-operation test and observation;
- interrupted transfer/apply recovery at connector-defined safe test points;
- rollback/recovery path;
- incident trigger, evidence capture, correction, verification, and close.

The hardware operator records board SKU/revision, runtime/connector/toolchain
versions, power/transport setup, artifact digest, test timestamps, results, and
unexpected observations. Sensitive identifiers are redacted from committed
evidence.

### L7 — Packaging, update, and recovery

- clean install under supported user/admin conditions;
- component versions and service/background registration;
- launch, auth callback registration, tray/startup behavior;
- repair and uninstall without deleting retained user data unexpectedly;
- signed update happy path;
- tampered/unsigned update rejection;
- interrupted update recovery;
- version compatibility during update;
- database backup/migration and application rollback behavior;
- diagnostic support bundle redaction;
- encrypted database/WAL/backup install, key rotation/recovery, service/user
  identity boundaries, and uninstall data choice;
- standalone MCP and build-executor launch without global Node/toolchain leakage.

### L8 — Adversarial build containment

Run controlled hostile repositories/toolchains against the installed build
executor and prove denial of:

- LokiAI database, secrets, local IPC, signing material, unrelated source, user
  profile, clipboard/UI, registry, network, USB/serial, and other build sandboxes;
- escaped child processes, resource-limit bypass, inherited privileged handles,
  output path traversal, symlink/reparse escape, and executable/argument injection;
- artifact registration after timeout, crash, invalid format, wrong target,
  oversize output, or provenance mismatch.

Containment tests run on every supported Windows build and against each certified
toolchain profile. A unit test of recipe parsing is not containment evidence.

## 3. Acceptance mapping

| Scenario | Minimum layers |
|---|---|
| AS-001/AS-002 auth/logout | L1–L5, L7 |
| AS-003 existing device | L1–L6 |
| AS-004 unsupported target | L1–L5; L6 when using observed candidate |
| AS-005 agent read | L1–L5 |
| AS-006 approved flash | L1–L7, including physical evidence |
| AS-007 changed request invalidates approval | L1–L5 |
| AS-008 incident recovery | L1–L6 |
| AS-009 interrupted operation | L1–L7, physical interruption evidence for release |
| AS-010 window closure | L1–L5, L7 |
| AS-011 device enrollment | L1–L6; cloud contract environment required |
| AS-012 isolated build | L1–L5, L7–L8 |
| AS-013 build escape attempt | L1–L4, L7–L8 |
| AS-014 runtime offline | L1–L5, L7 |
| AS-015 incident delivery | L1–L5; real supported-client acknowledgement or recorded fallback |
| AS-016 protected local state | L1–L5, L7 |

## 4. Test determinism and clocks

- Inject clock, randomness, ID generation, and retry policy into domain tests.
- Use fixed event sequences and expected projections.
- Never use sleep as the primary synchronization mechanism; wait on observable
  state with bounded deadlines.
- Record test seeds for property-based failures.
- Device tests declare environmental tolerances and distinguish expected ranges
  from exact values.

## 5. Negative and failure-path requirements

Every privileged feature includes tests for:

- unauthenticated and unauthorized caller;
- wrong workspace/target;
- invalid, stale, duplicate, and oversized input;
- timeout, cancellation, process crash, and reconnect;
- missing capability or incompatible version;
- policy deny and approval expiry/denial/reuse;
- artifact mismatch and wrong target;
- unsupported firmware/model/config/bundle type, member-digest/order/resource
  mismatch, partial bundle apply, and artifact rollback incompatibility;
- enrollment replay, wrong runtime/workspace, expired challenge, and revoked
  ownership;
- build sandbox escape, toolchain substitution, output traversal, and artifact
  mislabeling;
- stale role/policy, heartbeat expiry, duplicate cloud event/notification, and
  unsupported agent wake/resume;
- storage full/busy/corrupt where safely reproducible;
- redaction and safe error content;
- no false success after partial failure.

## 6. Quality thresholds

Release-blocking:

- zero failing required tests;
- zero unresolved critical/high security findings;
- zero uncovered release-blocking PRD requirements;
- zero TypeScript/Rust contract drift;
- zero known false-success state in device-changing workflows;
- all physical release scenarios pass on the confirmed reference board;
- installer and updater signatures verify;
- rollback/recovery evidence exists for defined failure points.

Coverage percentages may be tracked, but a global percentage never replaces
state-transition, authorization, and safety-path coverage.

## 7. Standard command contract

`FND-002` will implement and pin root commands with these semantic names:

```text
check            format/lint/type/static checks
test:unit        TypeScript and Rust unit/property tests
test:contract    IPC/MCP/cloud/connector contract tests
test:integration cross-process and persistence tests
test:desktop     packaged Electron end-to-end tests
test:hardware    operator-controlled physical hardware suite
test:security    configured security checks
test:containment adversarial build-executor isolation suite
package          unsigned local packaging for verification
release:verify   complete non-publishing release gate
```

Exact scripts are not considered available until `FND-002` accepts them. Task
packets must use commands that exist at their starting revision.

## 8. Evidence standard

A passing statement includes:

- exact revision;
- command/scenario;
- environment and relevant versions;
- start/end or run ID;
- exit/result summary;
- artifact/evidence references;
- skipped items and reason;
- tester identity.

Mandatory skipped verification means `BLOCKED_DEPENDENCY`, not `COMPLETE`.
