# `@lokiai/contracts`

LokiAI Main v1.2's shared, versioned, cross-language contracts: opaque/branded
IDs, the common envelope and error/operation-result shapes, `Docs/SPEC.md`
Section 4 state enums, and the MCP/connector schema shapes those documents
delegate to this package. Every exported type has a paired runtime (`zod`)
validator — this package is the single TypeScript-side source of truth the
renderer, Electron preload/main, and MCP server validate against, per
`Docs/ARCHITECTURE.md` Section 5.

## What this package is (and is not)

This package defines **contract shapes**, not behavior:

- versioned, branded/nominal ID types for every core entity in
  `Docs/SPEC.md` Section 3, plus the envelope's cross-cutting IDs
  (`message_id`, `correlation_id`, `causation_id`) and content digests
  (artifact/bound-request/toolchain/output);
- the shared `Envelope<T>` and `Error`/`OperationResult<T>` shapes from
  `Docs/SPEC.md` Section 1;
- the state enums `Docs/SPEC.md` Section 4 defines for runtime connection,
  durable jobs, incidents, deployments, and onboarding — plus device
  lifecycle, needed to type a connector capability's `required_state`;
- connector support-tier, capability-risk, health-check-status, and
  capability-descriptor shapes per `Docs/CONNECTOR_SPEC.md`;
- MCP-specific shapes `Docs/MCP_SPEC.md` explicitly delegates to CTR-001:
  the successful-read and operation-request result shapes, tool-name
  validation, the accepted v1.2 tool list with its three risk categories,
  and `loki://` resource URI parsing.

It does **not** contain:

- business logic, persistence, transport/framing, or policy decisions —
  those live in the runtime, MCP server, and preload/main layers that
  consume this package;
- a Rust crate or codegen tooling yet. Per the accepted `CTR-001` task
  packet, this node ships TypeScript types + `zod` runtime validators only;
  a Rust codegen/crate consumer is an explicit fast-follow once `RUN-001`'s
  Cargo crate exists. Until then, the Rust side must not hand-roll a
  divergent copy of these shapes — treat this package's doc comments (each
  citing the exact `Docs/` section it transcribes) as the source to port
  from;
- the full connector package manifest (`Docs/CONNECTOR_SPEC.md` Section 3)
  or per-operation contract (Section 8). Those are `CON-001`'s explicit
  scope ("Device-neutral connector trait, manifest, capability, health,
  operation, verification, recovery contracts"); this package provides the
  shared vocabulary (support tier, risk, capability descriptor) `CON-001`
  builds on, not the full trait;
- per-tool MCP input/output payload schemas (e.g. the exact fields
  `loki_request_flash` accepts). No concrete connector exists yet to define
  them against — the ESP32-S3 reference connector remains blocked on
  hardware confirmation (`Docs/CONNECTOR_SPEC.md` Section 10) — and
  inventing them here would be inventing product behavior.

## Versioning policy

- `package.json`'s `version` is this package's own semantic version and the
  source of truth; `CONTRACTS_PACKAGE_VERSION` (exported from `src/index.ts`)
  is kept in sync with it by convention for consumers that cannot reliably
  read `package.json` at runtime (e.g. a bundled Electron main/MCP build).
- This is a **pre-1.0, workspace-internal, `private: true`** package. Before
  `1.0.0`, a breaking shape change may land as a minor bump; consumers pin
  through the pnpm workspace lockfile, not a registry range.
- A breaking change to any exported schema (removing/renaming a field,
  narrowing a type, tightening a validator in a way that rejects previously
  valid input) is a **contract change** per `AGENTS.md` — it requires the
  same graph-level visibility as any other public contract change, not a
  silent patch. Additive, backward-compatible changes (a new optional field,
  a new enum member appended for a genuinely new documented state, a new
  export) are the normal path for fast-follow nodes.
- Every non-obvious shape decision is documented inline, next to the export,
  citing the exact `Docs/` file and section it was derived from — including
  cases where this package deliberately left a value **open** (a bounded
  string instead of a closed enum) because the cited document does not pin
  an exact vocabulary. Search for "not invented"/"open vocabulary" in
  `src/**` before assuming a gap is an oversight rather than a documented
  choice.

## How consumers import it

This is an unpublished, workspace-only package resolved through the pnpm
workspace (`pnpm-workspace.yaml` already declares `packages/*`). A consumer
package adds it as a normal dependency:

```jsonc
// some other workspace package's package.json
{
  "dependencies": {
    "@lokiai/contracts": "workspace:*",
  },
}
```

```ts
import {
  DeviceIdSchema,
  type DeviceId,
  envelopeSchema,
  JobStateSchema,
  type OperationResult,
} from '@lokiai/contracts';

const deviceId: DeviceId = DeviceIdSchema.parse(rawValue); // throws on malformed input
const result = DeviceIdSchema.safeParse(rawValue); // { success, data | error }
```

`"exports"` points directly at `src/index.ts` (TypeScript source, not a
compiled `dist/`), consistent with the repo's `"moduleResolution": "bundler"`
baseline (`tsconfig.base.json`) — every current/expected consumer (Vite for
the renderer, and whichever bundler/loader `DSK-001`/`MCP-001` select for
Electron main and the standalone MCP server) resolves `.ts` sources directly.
A compiled `dist/` output is intentionally not part of this node — no
consumer exists yet to require one, and adding a build step speculatively
would be exactly the kind of premature packaging decision `AGENTS.md` rule
11 warns against. Whichever node first wires a non-Vite consumer should
revisit this if plain TS-source resolution turns out to be insufficient for
that consumer's build.

## Layout

| Module             | Contents                                                                                             |
| ------------------ | ---------------------------------------------------------------------------------------------------- |
| `src/brand.ts`     | Shared branding/validation primitives (`idSchema`, `digestSchema`, `opaqueReferenceSchema`)          |
| `src/ids.ts`       | Every branded ID/digest schema + type                                                                |
| `src/timestamp.ts` | `IsoUtcTimestampSchema` (`Docs/SPEC.md` "All timestamps are UTC ISO 8601")                           |
| `src/states.ts`    | `Docs/SPEC.md` Section 4 state enums (+ device lifecycle) and documented transition maps             |
| `src/connector.ts` | `Docs/CONNECTOR_SPEC.md`-derived support tier, capability risk, health status, capability descriptor |
| `src/envelope.ts`  | `Actor`/`ActorType` and the generic `Envelope<T>`                                                    |
| `src/errors.ts`    | `LokiError` and the generic `OperationResult<T>`                                                     |
| `src/mcp.ts`       | MCP read/operation result shapes, tool naming, and `loki://` resource URI parsing                    |

## Testing

`pnpm --filter @lokiai/contracts run test` runs the full suite (`vitest`).
Every exported schema has both valid-input and malformed-input
(missing-field, wrong-type, unknown-enum-value, oversized-payload) coverage —
see `tests/`, one file per `src/` module.
