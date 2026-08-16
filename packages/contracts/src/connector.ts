import { z } from 'zod';
import { DeviceLifecycleStateSchema } from './states.js';

/**
 * Connector capability schema shapes per `Docs/CONNECTOR_SPEC.md`.
 *
 * Scope note: `Docs/graph/TASK_GRAPH.md` assigns the full connector trait,
 * package manifest, health-report, and per-operation contract to `CON-001`
 * ("Device-neutral connector trait, manifest, capability, health, operation,
 * verification, recovery contracts", `depends_on: CTR-001`). This module
 * therefore provides the shared *vocabulary* CON-001 builds on — support
 * tiers, capability risk, health-check status, and the capability descriptor
 * shape literally enumerated in `Docs/CONNECTOR_SPEC.md` Section 6 — not the
 * full manifest (Section 3) or per-operation contract (Section 8), which are
 * CON-001's explicit scope. No ESP32-specific concept appears here per
 * `AGENTS.md`'s device-family-neutral contract rule; the ESP32-S3 reference
 * connector itself remains blocked on hardware confirmation
 * (`Docs/CONNECTOR_SPEC.md` Section 10).
 */

// --- Support levels (Docs/CONNECTOR_SPEC.md Section 2) ------------------------------

/**
 * `Docs/CONNECTOR_SPEC.md` Section 2 "Support levels" table. Also used by
 * `Docs/SPEC.md` 3.3 Device "support tier". Lowercased from the table's
 * Title-Case column headers to match this package's wire-value convention —
 * see `Docs/SPEC.md` 3.5's own lowercase `good`/`late`/`gap`/`invalid` quality
 * values for the established precedent; the table headers are prose styling,
 * not a demonstrated distinct wire casing.
 */
export const SUPPORT_TIERS = ['certified', 'supported', 'custom', 'unsupported'] as const;
export const SupportTierSchema = z.enum(SUPPORT_TIERS);
export type SupportTier = z.infer<typeof SupportTierSchema>;

// --- Capability risk (Docs/CONNECTOR_SPEC.md Section 6 + Docs/MCP_SPEC.md Section 6) --

/**
 * `Docs/CONNECTOR_SPEC.md` Section 6 requires every capability to declare a
 * `risk`, but does not enumerate a fixed value set on its own. The three
 * values below are lifted verbatim from `Docs/MCP_SPEC.md` Section 6's tool
 * risk headings ("Inspection — low risk", "... — low/medium risk by policy",
 * "Consequential operations — high risk by default"), which is the only place
 * either document uses concrete risk-level words. This is a capability's own
 * *declared* classification (an input to policy), not a policy decision —
 * `Docs/SPEC.md` Section 2 reserves allow/approval-required/deny decisions to
 * the policy engine, which this package does not model.
 */
export const CAPABILITY_RISK_LEVELS = ['low', 'medium', 'high'] as const;
export const CapabilityRiskLevelSchema = z.enum(CAPABILITY_RISK_LEVELS);
export type CapabilityRiskLevel = z.infer<typeof CapabilityRiskLevelSchema>;

// --- Health check status (Docs/CONNECTOR_SPEC.md Section 7) -------------------------

/**
 * `Docs/CONNECTOR_SPEC.md` Section 7 "status: passed, warning, failed,
 * unavailable, interrupted" — given as a definitive (not "may include") list.
 */
export const HEALTH_CHECK_STATUSES = [
  'passed',
  'warning',
  'failed',
  'unavailable',
  'interrupted',
] as const;
export const HealthCheckStatusSchema = z.enum(HEALTH_CHECK_STATUSES);
export type HealthCheckStatus = z.infer<typeof HealthCheckStatusSchema>;

// --- Capability identifiers (Docs/CONNECTOR_SPEC.md Section 6) ----------------------

/**
 * `Docs/CONNECTOR_SPEC.md` Section 6 "Initial vocabulary may include" —
 * explicitly non-exhaustive. `KNOWN_CONNECTOR_CAPABILITY_IDS` is the literal
 * bullet list decomposed into slug form (slash-combined bullets such as
 * "stream/query declared telemetry" become two capability IDs); it is a
 * convenience/reference list, not a closed set. `ConnectorCapabilityIdSchema`
 * validates the general slug shape so a connector can declare a capability ID
 * outside this list without failing validation, per the spec's own "may
 * include" language.
 */
export const KNOWN_CONNECTOR_CAPABILITY_IDS = [
  'identify',
  'read_health',
  'stream_telemetry',
  'query_telemetry',
  'read_logs',
  'run_diagnostics',
  'run_preflight',
  'build_artifact',
  'transfer_artifact',
  'flash_artifact',
  'restart',
  'run_test_suite',
  'update_configuration',
  'deploy_model',
  'verify_artifact',
  'rollback',
] as const;
/**
 * Closed-enum runtime validator for the known/reference slugs above — for a
 * caller that specifically wants to check membership in the documented
 * vocabulary rather than merely the general slug shape. Most callers should
 * use `ConnectorCapabilityIdSchema` (below) instead: this closed schema will
 * reject a legitimately new capability ID a connector declares, which is by
 * design not an error per Section 6's "may include" language.
 */
export const KnownConnectorCapabilityIdSchema = z.enum(KNOWN_CONNECTOR_CAPABILITY_IDS);
export type KnownConnectorCapabilityId = z.infer<typeof KnownConnectorCapabilityIdSchema>;

const CAPABILITY_ID_PATTERN = /^[a-z][a-z0-9_]*$/;

export const ConnectorCapabilityIdSchema = z
  .string()
  .min(1)
  .max(64)
  .regex(CAPABILITY_ID_PATTERN, 'capability_id must be a lowercase snake_case slug');
export type ConnectorCapabilityId = z.infer<typeof ConnectorCapabilityIdSchema>;

// --- Capability descriptor (Docs/CONNECTOR_SPEC.md Section 6) -----------------------

/**
 * `Docs/CONNECTOR_SPEC.md` Section 6: "An advertised capability includes
 * schema, risk, required state, exclusive-access mode, expected interruption,
 * timeout, verification, and recovery contract." Field-for-field:
 *
 * - schema               -> `schema_ref` + `schema_version`
 * - risk                 -> `risk`
 * - required state       -> `required_state`
 * - exclusive-access mode -> `exclusive_access`
 * - expected interruption -> `expected_interruption`
 * - timeout              -> `timeout_ms`
 * - verification contract -> `verification_required`
 * - recovery contract    -> `recovery_supported`
 *
 * `schema_ref` intentionally points at a named/versioned input-output schema
 * elsewhere rather than embedding a full JSON Schema inline: the CTR-001 task
 * packet directs "hand-written TypeScript types + zod ... runtime validators
 * ... at this scale; do not add a schema-compiler toolchain merely for
 * convenience," and no concrete connector exists yet to author real per-
 * operation input/output schemas against (the ESP32-S3 reference connector is
 * blocked on hardware confirmation, `Docs/CONNECTOR_SPEC.md` Section 10).
 */
export const ConnectorCapabilityDescriptorSchema = z.object({
  capability_id: ConnectorCapabilityIdSchema,
  schema_ref: z.string().min(1).max(200),
  schema_version: z.string().min(1).max(50),
  risk: CapabilityRiskLevelSchema,
  required_state: z.array(DeviceLifecycleStateSchema).min(1),
  exclusive_access: z.boolean(),
  expected_interruption: z.boolean(),
  timeout_ms: z.number().int().positive(),
  verification_required: z.boolean(),
  recovery_supported: z.boolean(),
});
export type ConnectorCapabilityDescriptor = z.infer<typeof ConnectorCapabilityDescriptorSchema>;
