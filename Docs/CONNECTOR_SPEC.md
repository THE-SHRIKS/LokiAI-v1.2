# Device Connector Specification — LokiAI Main v1.2

Status: **Normative proposed contract**  
Updated: **2026-08-15**

## 1. Objective

The connector system lets LokiAI support many micro-device families without
placing family-specific behavior in shared runtime, MCP, cloud, or UI logic. A
connector translates a declared LokiAI operation into verified device-family and
transport behavior; it does not receive unrestricted platform authority.

## 2. Support levels

| Level | Meaning | UI/agent promise |
|---|---|---|
| Certified | LokiAI release tests the exact board/module, connector, toolchain, and required physical scenarios | All certified capabilities may be offered under policy |
| Supported | Connector recognizes a family/variant and passes contract tests, but not every physical revision is release-certified | Only declared supported capabilities; compatibility caveats visible |
| Custom | User/team supplies a connector or transport integration | Isolated and labeled; no LokiAI certification claim |
| Unsupported | Candidate transport observed but no compatible connector/identity | Inspectable reason; no device-changing actions |

The first v1.2 Certified target is the exact ESP32-S3 board/module confirmed by
`OQ-001`.

## 3. Connector package manifest

Required metadata:

- connector ID and semantic version;
- publisher and integrity/signature/provenance;
- runtime protocol range;
- supported OS/architectures;
- device families, board/module identifiers, and match rules;
- transports and required host permissions/drivers;
- capabilities and risk classification;
- external toolchain names, pinned versions, integrity source, and licenses;
- health checks, telemetry/log schemas, and retention/redaction hints;
- operation input/output schemas, timeouts, cancellation points, verification, and
  recovery guarantees;
- physical certification matrix and evidence references.

## 4. Connector boundary

Connector receives:

- a least-privilege transport handle or approved tool adapter;
- stable job context, deadline/cancellation, bounded diagnostic sink;
- validated device/operation/artifact inputs;
- approved capability and operation plan.

Connector may return:

- candidate and identity evidence;
- capability/health/telemetry/log results;
- stage progress and bounded diagnostics;
- verification/reconciliation/recovery evidence;
- structured errors.

Connector must not:

- access OAuth or agent credentials;
- decide authorization/approval;
- write the domain database or activity stream directly;
- accept arbitrary shell fragments from renderer/agent input;
- change undeclared targets or operations;
- report deployment/incident completion;
- continue a device-changing stage after losing the owning job lease.

## 5. Discovery and identity

Discovery is two-step:

1. `probe` observes candidate transports with evidence and confidence.
2. `identify` performs bounded family-specific interrogation and returns stable
   identity or a structured unsupported/permission/busy/error result.

Rules:

- COM port, IP address, BLE address, or probe index is not stable identity.
- Identity evidence records connector/version, method, time, and confidence.
- Duplicate observations of the same stable device are reconciled.
- Before every device-changing operation, connector re-identifies target and
  runtime compares it to the approved target snapshot.
- Discovery never flashes, resets, or changes configuration unless the user
  explicitly starts a separate authorized operation.
- Discovery creates a candidate, not workspace ownership. Consequential
  capabilities remain disabled until candidate evidence is bound through the
  one-time enrollment contract in `CLOUD_CONTRACT.md`.

## 6. Capability model

Capabilities are granular and parameterized. Initial vocabulary may include:

- identify;
- read health;
- stream/query declared telemetry;
- read bounded logs;
- run diagnostics/preflight;
- build target artifact;
- transfer/flash artifact;
- restart;
- run named on-device test suite;
- update declared configuration;
- deploy declared edge-model artifact;
- verify current artifact/config/model;
- rollback/recover.

An advertised capability includes schema, risk, required state, exclusive-access
mode, expected interruption, timeout, verification, and recovery contract.

## 7. Health checks

Health report entries include:

- check ID/version and category;
- status: passed, warning, failed, unavailable, interrupted;
- observed value/unit and threshold/expectation where meaningful;
- observed and received timestamp;
- evidence/reference and redaction class;
- suggested next safe action.

A connector cannot infer voltage, thermal, memory, firmware, or bus health when it
has no evidence source. It returns unavailable with reason.

## 8. Device operation contract

Every operation declares:

```text
preconditions
required capability and host permission
stable target identity fields
canonical inputs
risk and expected downtime
exclusive lease mode
preflight checks
execution stages
safe cancellation points
progress semantics
post-operation re-identification
verification plan
recoverable interruption points
rollback/recovery plan
terminal structured errors
```

The runtime persists the plan before invoking connector execution.

## 9. External toolchains

Some connectors use vendor or open toolchains. LokiAI wraps them through fixed
argument construction and parsed structured results.

- Pin and verify exact compatible versions.
- Never concatenate user/agent input into a shell command.
- Resolve executables from an approved manifest/path, not ambient `PATH` alone.
- Execute with bounded environment, working directory, output, deadline, and
  cancellation.
- Capture version, arguments with secrets redacted, exit status, and output digest.
- Treat exit success as stage completion only; connector verification still runs.

Build-capable connectors also publish the typed recipe required by
[`BUILD_EXECUTION.md`](BUILD_EXECUTION.md). Toolchains and repository scripts run
only in the contained build executor. Flash, serial, USB, credential, signing,
and authenticated runtime-IPC access is never granted to that executor.

## 10. ESP32-S3 reference connector

Implementation is blocked until the human confirms:

- exact module marking and development-board name/revision;
- flash capacity and PSRAM configuration implied by the actual SKU;
- USB interface/bridge and boot/reset wiring;
- permitted operating voltage/power setup;
- intended framework/build system and partition/bootloader ownership;
- recovery access and known-good artifact policy.

After confirmation, certification must cover:

- discovery and stable chip/board evidence;
- transport permission/busy/disconnect behavior;
- firmware/build metadata;
- real health/log sources supported by the board/firmware;
- image/partition compatibility validation;
- boot/reset/flash sequence and digest/readback verification where available;
- post-flash restart/re-identification and test observation;
- interruption and recovery at safe, controlled test points;
- rollback/recovery policy and physical operator record.

No shared interface may be named or shaped around ESP32-only concepts.

## 11. Connector conformance suite

Every connector passes:

- manifest/schema/version compatibility;
- discovery with no side effects;
- stable identity and duplicate reconciliation;
- enrollment challenge replay, wrong-runtime binding, and ownership isolation;
- capability honesty and unsupported-operation rejection;
- malformed/oversized input and output bounds;
- deadline/cancellation behavior;
- lease loss and concurrent-operation rejection;
- structured error and redaction;
- deterministic canonical operation plan;
- interruption/reconciliation/recovery contract;
- telemetry/log quality and backpressure;
- target re-identification before mutation;
- build recipe schema, containment profile, toolchain digest, and quarantined
  output validation where build is advertised;
- physical evidence required for its claimed support level.

## 12. Third-party/custom connectors

The v1.2 architecture preserves a future isolation boundary. A custom connector
must be explicitly enabled, labeled unverified, scoped to approved workspaces, and
unable to expand runtime authority. Loading arbitrary native code into the trusted
runtime requires a separate ADR; an out-of-process connector host is preferred for
untrusted third-party packages.
