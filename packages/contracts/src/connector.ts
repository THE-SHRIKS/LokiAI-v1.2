import { z } from 'zod';
import { IsoUtcTimestampSchema } from './timestamp.js';
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

// --- Discovery and device-derived identity (CON-ID-CONTRACT-001) -------------------

const CONNECTOR_VERSION_PATTERN = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;
const SLUG_PATTERN = /^[a-z][a-z0-9_]*$/;
const SHA256_HEX_PATTERN = /^[0-9a-f]{64}$/;

const StrictSlugSchema = z.string().min(1).max(64).regex(SLUG_PATTERN);
const ConnectorVersionSchema = z.string().min(5).max(64).regex(CONNECTOR_VERSION_PATTERN);
const Sha256HexSchema = z.string().length(64).regex(SHA256_HEX_PATTERN);
const BoundedUtcTimestampSchema = IsoUtcTimestampSchema.refine((value) => value.length <= 30, {
  message: 'UTC timestamp exceeds 30 characters',
});

/**
 * An OS/transport locator is discovery input only. The literal fields make
 * its security semantics impossible to omit or silently upgrade on the wire.
 */
export const SensitiveUnstableLocatorSchema = z
  .object({
    value: z.string().min(1).max(512),
    sensitivity: z.literal('sensitive'),
    stability: z.literal('unstable'),
  })
  .strict();
export type SensitiveUnstableLocator = z.infer<typeof SensitiveUnstableLocatorSchema>;

/** Side-effect-free transport candidate. `candidate_id` is explicitly ephemeral. */
export const TransportCandidateSchema = z
  .object({
    schema_version: z.literal('1.0.0'),
    candidate_id: z.string().min(1).max(128),
    candidate_id_stability: z.literal('ephemeral'),
    connector_id: z.string().min(1).max(128),
    connector_version: ConnectorVersionSchema,
    transport_kind: StrictSlugSchema,
    locator: SensitiveUnstableLocatorSchema,
    discovered_at: BoundedUtcTimestampSchema,
  })
  .strict();
export type TransportCandidate = z.infer<typeof TransportCandidateSchema>;

export const IDENTITY_CONFIDENCE_LEVELS = ['weak', 'moderate', 'strong'] as const;
export const IdentityConfidenceSchema = z.enum(IDENTITY_CONFIDENCE_LEVELS);

/**
 * Device-family evidence remains opaque to the shared layer. Raw material is
 * consumed inside the connector SDK and never enters this wire projection.
 *
 * `fingerprint_method` is `hmac_sha256_device_material_v1`, not a bare
 * unkeyed hash: per the approved Connector Identity Protocol v1.0.0
 * (`Docs/graph/tasks/CON-ID-CONTRACT-001.md` Section 10), the public device
 * identity is *installation-scoped* — the connector SDK keys the HMAC with
 * the per-installation secret (`Docs/ARCHITECTURE.md`) so the same physical
 * device yields a different public identity per installation. This wire
 * schema only validates shape; it never computes or carries the key.
 */
export const DeviceIdentityEvidenceSchema = z
  .object({
    evidence_type: StrictSlugSchema,
    fingerprint_method: z.literal('hmac_sha256_device_material_v1'),
    raw_material_handling: z.literal('consumed_not_persisted'),
    fingerprint_digest: Sha256HexSchema,
    confidence: IdentityConfidenceSchema,
    connector_id: z.string().min(1).max(128),
    connector_version: ConnectorVersionSchema,
    observed_at: BoundedUtcTimestampSchema,
    evidence_digest: Sha256HexSchema,
  })
  .strict();
export type DeviceIdentityEvidence = z.infer<typeof DeviceIdentityEvidenceSchema>;

export const IDENTITY_RESOLUTION_STATUSES = [
  'unknown',
  'partial',
  'verified',
  'ambiguous',
  'conflicting',
] as const;
export const IdentityResolutionStatusSchema = z.enum(IDENTITY_RESOLUTION_STATUSES);

/** Safe projection; it contains no raw locator or fingerprint material. */
export const PublicDeviceIdentitySchema = z
  .object({
    scheme: z.literal('lokiai_device_hmac_sha256_v1'),
    device_id: z.string().regex(/^dev1_[0-9a-f]{64}$/),
    fingerprint_digest: Sha256HexSchema,
  })
  .strict()
  .refine((value) => value.device_id === `dev1_${value.fingerprint_digest}`, {
    message: 'device_id must be derived from fingerprint_digest',
  });
export type PublicDeviceIdentity = z.infer<typeof PublicDeviceIdentitySchema>;

/**
 * Fail-closed probe result. Only `verified` may carry a public stable identity;
 * all verified evidence must agree on the same device-derived fingerprint.
 */
export const IdentityProbeResultSchema = z
  .object({
    schema_version: z.literal('1.0.0'),
    candidate: TransportCandidateSchema,
    status: IdentityResolutionStatusSchema,
    evidence: z.array(DeviceIdentityEvidenceSchema).max(16),
    public_identity: PublicDeviceIdentitySchema.nullable(),
    reason_code: StrictSlugSchema,
  })
  .strict()
  .superRefine((value, ctx) => {
    for (const item of value.evidence) {
      if (item.evidence_digest !== item.fingerprint_digest) {
        ctx.addIssue({
          code: 'custom',
          path: ['evidence'],
          message: 'evidence_digest must match the canonical device fingerprint evidence',
        });
        break;
      }
    }
    const verified = value.status === 'verified';
    if (verified !== (value.public_identity !== null)) {
      ctx.addIssue({
        code: 'custom',
        path: ['public_identity'],
        message: 'only verified results carry identity',
      });
    }
    if (verified && value.evidence.length === 0) {
      ctx.addIssue({
        code: 'custom',
        path: ['evidence'],
        message: 'verified identity requires evidence',
      });
    }
    if (
      (value.status === 'ambiguous' || value.status === 'conflicting') &&
      value.evidence.length < 2
    ) {
      ctx.addIssue({
        code: 'custom',
        path: ['evidence'],
        message: 'ambiguous/conflicting requires multiple evidence items',
      });
    }
    if (value.status === 'unknown' && value.evidence.length !== 0) {
      ctx.addIssue({
        code: 'custom',
        path: ['evidence'],
        message: 'unknown must not carry evidence',
      });
    }
    if (verified && value.public_identity) {
      for (const item of value.evidence) {
        if (item.fingerprint_digest !== value.public_identity.fingerprint_digest) {
          ctx.addIssue({
            code: 'custom',
            path: ['evidence'],
            message: 'verified evidence fingerprints must agree',
          });
          break;
        }
      }
    }
  });
export type IdentityProbeResult = z.infer<typeof IdentityProbeResultSchema>;

// --- Sensitive-locator redaction (security review follow-up, CON-ID-CONTRACT-001) --
//
// `SensitiveUnstableLocatorSchema` intentionally validates the *wire* shape,
// which legitimately carries the real locator once: a connector genuinely
// needs the real COM port / USB descriptor / transport locator to open the
// device. TypeScript objects returned by `.parse()` are plain data with no
// serialization hook to redact automatically (unlike the Rust SDK's
// `Serialize` impl in `crates/loki-connectors-sdk/src/identity.rs`, which
// redacts by default and exposes an explicit `serialize_for_transport()` for
// this one legitimate case). These helpers are the TypeScript-side
// equivalent of that explicit, opt-in boundary: any code that logs, persists
// as evidence, or otherwise projects a candidate/probe result for anything
// other than opening the device MUST route it through one of these before
// it leaves connector-owning code, rather than passing the parsed wire
// object straight into `JSON.stringify`/logging.

const REDACTED_LOCATOR_VALUE = '[REDACTED]';

/** Redacts the raw `value` of a single locator. */
export function redactSensitiveLocator(
  locator: SensitiveUnstableLocator,
): SensitiveUnstableLocator {
  return { ...locator, value: REDACTED_LOCATOR_VALUE };
}

/** Redacts the raw locator embedded in a transport candidate. */
export function redactTransportCandidate(candidate: TransportCandidate): TransportCandidate {
  return { ...candidate, locator: redactSensitiveLocator(candidate.locator) };
}

/** Redacts the raw locator embedded in a full identity probe result. */
export function redactIdentityProbeResult(result: IdentityProbeResult): IdentityProbeResult {
  return { ...result, candidate: redactTransportCandidate(result.candidate) };
}
