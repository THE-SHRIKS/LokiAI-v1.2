import { z } from 'zod';
import {
  AgentRunIdSchema,
  ApprovalIdSchema,
  ArtifactDigestSchema,
  ArtifactIdSchema,
  BoundRequestDigestSchema,
  CorrelationIdSchema,
  DeploymentIdSchema,
  DeviceIdSchema,
  IncidentIdSchema,
  JobIdSchema,
  RuntimeIdSchema,
  WorkIntentIdSchema,
} from './ids.js';
import { IsoUtcTimestampSchema } from './timestamp.js';
import { DeviceLifecycleStateSchema } from './states.js';
import { SupportTierSchema } from './connector.js';

/**
 * Business RPC contract — `P1-RPC-CONTRACT-001-R1`, mirroring
 * `crates/loki-ipc/src/{business.rs,protocol.rs}` field-for-field/
 * variant-for-variant. See `business.rs`'s top doc comment for exactly
 * which 10 of the 23 unary methods produce a real result shape: the other
 * 13 are closed, named, typed, and authorized, but never appear on the
 * right-hand side of `BusinessRpcResultSchema` — no real backing contract
 * exists for them anywhere in this workspace yet.
 *
 * No `workspace_id`/`tenant_id` field exists anywhere in this module
 * (`ADR-0006` Amendment 1). `authorized_entity_ids`/`entity_id` bind to the
 * exact concrete `device_id`/`run_id`/`work_intent_id`/etc. identity, never
 * a placeholder grouping concept.
 */

/** Current local business RPC protocol. Version 1.0 remains the bounded transport-only edge. */
export const BUSINESS_RPC_PROTOCOL_VERSION = '1.1' as const;
export const BUSINESS_RPC_CAPABILITY = 'loki-ipc.business.v1' as const;
export const MAX_RPC_PAGE_SIZE = 100;
export const MAX_RPC_TEXT_BYTES = 2_000;
export const MAX_RPC_STREAM_ITEMS = 1_024;

const utf8Bytes = (value: string) => new TextEncoder().encode(value).byteLength;
const Utf8TextSchema = (maxBytes: number, allowEmpty = false) =>
  z
    .string()
    .refine(
      (value) => (allowEmpty || value.length > 0) && utf8Bytes(value) <= maxBytes,
      `must be ${allowEmpty ? '' : 'non-empty and '}at most ${maxBytes} UTF-8 bytes`,
    );
const SafeIdSchema = z
  .string()
  .min(1)
  .max(200)
  .regex(/^[A-Za-z0-9._:-]+$/);
const IdempotencyKeySchema = SafeIdSchema;
const ParametersDigestSchema = z
  .string()
  .min(16)
  .max(128)
  .regex(/^[A-Za-z0-9_-]+$/);
const CursorSchema = z
  .string()
  .min(1)
  .max(512)
  .regex(/^[A-Za-z0-9._~:-]+$/);
/**
 * Exact wire format `loki_mutation_contract::digest::RequestDigest::try_from_str`
 * accepts: `sha256:` followed by exactly 64 lowercase hex digits. Distinct
 * from the pre-existing, looser `BoundRequestDigestSchema` (`./ids.ts`,
 * used unchanged by `approval_grant`/`approval_deny`'s own
 * `bound_request_digest` field, which has no relationship to
 * `loki-mutation-contract`) — this schema exists specifically for
 * `ApprovalReferenceSchema.bound_request_digest`, the one field that must
 * parse as a real `RequestDigest` on the Rust side.
 */
const RequestDigestSchema = z
  .string()
  .regex(/^sha256:[0-9a-f]{64}$/, 'must be "sha256:" followed by exactly 64 lowercase hex digits');
const PageSchema = z
  .object({
    limit: z.number().int().min(1).max(MAX_RPC_PAGE_SIZE),
    cursor: CursorSchema.nullable().optional(),
  })
  .strict();
/** Mirrors `loki_runtime::services::DevicePageRequest` field-for-field. */
const DeviceListSchema = z
  .object({
    limit: z.number().int().min(1).max(MAX_RPC_PAGE_SIZE),
    after_device_id: DeviceIdSchema.nullable().optional(),
  })
  .strict();
const DeviceSchema = z.object({ device_id: DeviceIdSchema }).strict();
const DeviceBoundedReadSchema = DeviceSchema.extend({
  limit: z.number().int().min(1).max(MAX_RPC_PAGE_SIZE),
  cursor: CursorSchema.nullable().optional(),
}).strict();
const IdempotentDeviceSchema = z
  .object({ device_id: DeviceIdSchema, idempotency_key: IdempotencyKeySchema })
  .strict();

const ApprovalReferenceSchema = z
  .object({
    approval_id: SafeIdSchema,
    bound_request_digest: RequestDigestSchema,
    expires_at_ms: z.number().int().positive(),
  })
  .strict();

const RestartRequestSchema = z
  .object({
    device_id: DeviceIdSchema,
    parameters_digest: ParametersDigestSchema,
    idempotency_key: IdempotencyKeySchema,
    approval_reference: ApprovalReferenceSchema.nullable().optional(),
  })
  .strict();
const FlashRequestSchema = z
  .object({
    device_id: DeviceIdSchema,
    artifact_id: ArtifactIdSchema,
    artifact_digest: ArtifactDigestSchema,
    parameters_digest: ParametersDigestSchema,
    idempotency_key: IdempotencyKeySchema,
    approval_reference: ApprovalReferenceSchema.nullable().optional(),
  })
  .strict();
const BuildRequestSchema = z
  .object({
    source_snapshot_id: SafeIdSchema,
    target_id: SafeIdSchema,
    recipe_id: SafeIdSchema,
    parameters_digest: ParametersDigestSchema,
    idempotency_key: IdempotencyKeySchema,
    approval_reference: ApprovalReferenceSchema.nullable().optional(),
  })
  .strict();
const ArtifactOperationSchema = z
  .object({
    device_id: DeviceIdSchema,
    artifact_id: ArtifactIdSchema,
    artifact_digest: ArtifactDigestSchema,
    idempotency_key: IdempotencyKeySchema,
  })
  .strict();

/** Explicit method table. No string-selected invoke, executable, path, URL, environment, or shell field exists. */
export const BusinessRpcRequestSchema = z.discriminatedUnion('method', [
  z.object({ method: z.literal('runtime_get_status'), params: z.object({}).strict() }).strict(),
  z.object({ method: z.literal('devices_list'), params: DeviceListSchema }).strict(),
  z.object({ method: z.literal('device_get'), params: DeviceSchema }).strict(),
  z.object({ method: z.literal('device_get_health'), params: DeviceSchema }).strict(),
  z.object({ method: z.literal('device_read_logs'), params: DeviceBoundedReadSchema }).strict(),
  z
    .object({
      method: z.literal('device_query_telemetry'),
      params: DeviceBoundedReadSchema.extend({
        signal_ids: z.array(Utf8TextSchema(128)).max(64),
      }).strict(),
    })
    .strict(),
  z.object({ method: z.literal('incidents_list'), params: PageSchema }).strict(),
  z
    .object({
      method: z.literal('incident_get'),
      params: z.object({ incident_id: IncidentIdSchema }).strict(),
    })
    .strict(),
  z
    .object({ method: z.literal('job_get'), params: z.object({ job_id: JobIdSchema }).strict() })
    .strict(),
  z
    .object({
      method: z.literal('agent_run_register'),
      params: z
        .object({
          parent_run_id: AgentRunIdSchema.nullable().optional(),
          parameters_digest: ParametersDigestSchema,
          idempotency_key: IdempotencyKeySchema,
        })
        .strict(),
    })
    .strict(),
  z
    .object({
      method: z.literal('agent_run_update'),
      params: z
        .object({
          run_id: AgentRunIdSchema,
          phase: z.enum(['planning', 'working', 'waiting', 'completed', 'failed']),
          summary: Utf8TextSchema(MAX_RPC_TEXT_BYTES),
          parameters_digest: ParametersDigestSchema,
          idempotency_key: IdempotencyKeySchema,
        })
        .strict(),
    })
    .strict(),
  z
    .object({
      method: z.literal('work_intent_acknowledge'),
      params: z
        .object({
          work_intent_id: WorkIntentIdSchema,
          run_id: AgentRunIdSchema,
          parameters_digest: ParametersDigestSchema,
          idempotency_key: IdempotencyKeySchema,
        })
        .strict(),
    })
    .strict(),
  z.object({ method: z.literal('health_check_request'), params: IdempotentDeviceSchema }).strict(),
  z.object({ method: z.literal('preflight_request'), params: IdempotentDeviceSchema }).strict(),
  z
    .object({
      method: z.literal('job_cancel'),
      params: z.object({ job_id: JobIdSchema, idempotency_key: IdempotencyKeySchema }).strict(),
    })
    .strict(),
  z.object({ method: z.literal('build_request'), params: BuildRequestSchema }).strict(),
  z.object({ method: z.literal('restart_request'), params: RestartRequestSchema }).strict(),
  z.object({ method: z.literal('flash_request'), params: FlashRequestSchema }).strict(),
  z
    .object({
      method: z.literal('test_on_device_request'),
      params: z
        .object({
          device_id: DeviceIdSchema,
          test_suite_id: SafeIdSchema,
          idempotency_key: IdempotencyKeySchema,
        })
        .strict(),
    })
    .strict(),
  z
    .object({
      method: z.literal('deployment_request'),
      params: ArtifactOperationSchema.extend({
        verification_plan_id: Utf8TextSchema(200),
        rollback_plan_id: Utf8TextSchema(200),
      }).strict(),
    })
    .strict(),
  z
    .object({
      method: z.literal('rollback_request'),
      params: z
        .object({
          device_id: DeviceIdSchema,
          deployment_id: DeploymentIdSchema,
          recovery_policy_id: Utf8TextSchema(200),
          idempotency_key: IdempotencyKeySchema,
        })
        .strict(),
    })
    .strict(),
  z
    .object({
      method: z.literal('approval_grant'),
      params: z
        .object({
          approval_id: ApprovalIdSchema,
          bound_request_digest: BoundRequestDigestSchema,
          idempotency_key: IdempotencyKeySchema,
        })
        .strict(),
    })
    .strict(),
  z
    .object({
      method: z.literal('approval_deny'),
      params: z
        .object({
          approval_id: ApprovalIdSchema,
          bound_request_digest: BoundRequestDigestSchema,
          reason: Utf8TextSchema(MAX_RPC_TEXT_BYTES),
          idempotency_key: IdempotencyKeySchema,
        })
        .strict(),
    })
    .strict(),
]);
export type BusinessRpcRequest = z.infer<typeof BusinessRpcRequestSchema>;

/** The exact 23 method names this vocabulary ships (never a generic invoke). */
export const BUSINESS_RPC_METHODS = BusinessRpcRequestSchema.options.map(
  (option) => option.shape.method.value,
);

export const PRINCIPAL_KINDS = [
  'desktop_installation',
  'agent_connection',
  'runtime_service',
] as const;
export const AuthenticatedPrincipalSchema = z
  .object({
    kind: z.enum(PRINCIPAL_KINDS),
    principal_id: SafeIdSchema,
    authorized_entity_ids: z
      .array(SafeIdSchema)
      .max(4096)
      .refine(
        (values) => new Set(values).size === values.length,
        'authorized_entity_ids must be unique',
      ),
    authority: z
      .array(
        z.enum([
          'read_runtime',
          'read_devices',
          'read_telemetry',
          'read_incidents',
          'read_jobs',
          'manage_agent_runs',
          'acknowledge_work_intents',
          'request_diagnostics',
          'cancel_jobs',
          'request_build',
          'request_device_operation',
          'decide_approval',
          'read_approvals',
          'read_notifications',
          'read_work_intents',
          'subscribe',
        ]),
      )
      .max(16)
      .refine((values) => new Set(values).size === values.length, 'authority must be unique'),
  })
  .strict();
export type AuthenticatedPrincipal = z.infer<typeof AuthenticatedPrincipalSchema>;

const ReadMetaSchema = z
  .object({
    source_runtime_id: RuntimeIdSchema,
    observed_at: IsoUtcTimestampSchema,
    correlation_id: CorrelationIdSchema,
  })
  .strict();

/** Mirrors `loki_runtime::services::DeviceSummary` field-for-field. */
const WireDeviceSummarySchema = z
  .object({
    device_id: DeviceIdSchema,
    connector_id: Utf8TextSchema(200),
    connector_version: Utf8TextSchema(100),
    family: Utf8TextSchema(100),
    board_identity: Utf8TextSchema(200).nullable().optional(),
    runtime_id: RuntimeIdSchema,
    support_tier: SupportTierSchema,
    lifecycle_state: DeviceLifecycleStateSchema,
    last_seen_at: IsoUtcTimestampSchema,
  })
  .strict();

const WireStorageReadinessSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('ready') }).strict(),
  z.object({ kind: z.literal('corrupt'), problem_count: z.number().int().nonnegative() }).strict(),
]);
const WireMutationAvailabilitySchema = z.literal('available');

const WireDeviceHealthStateSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('not_observed') }).strict(),
  z
    .object({
      kind: z.literal('persisted'),
      evidence: z
        .object({
          payload_size_bytes: z.number().int().nonnegative(),
          last_seen_at: IsoUtcTimestampSchema,
        })
        .strict(),
    })
    .strict(),
]);

/** Mirrors `loki_mutation_contract::MutationRejection` variant-for-variant. */
export const WireMutationRejectionSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('digest_conflict') }).strict(),
  z
    .object({ kind: z.literal('grant_invalid'), reason: Utf8TextSchema(MAX_RPC_TEXT_BYTES) })
    .strict(),
  z.object({ kind: z.literal('grant_expired') }).strict(),
  z.object({ kind: z.literal('grant_revoked') }).strict(),
  z.object({ kind: z.literal('policy_denied') }).strict(),
  z.object({ kind: z.literal('approval_not_consumable') }).strict(),
  z
    .object({
      kind: z.literal('invalid_input'),
      field: Utf8TextSchema(200),
      reason: Utf8TextSchema(MAX_RPC_TEXT_BYTES),
    })
    .strict(),
]);

/** Mirrors `loki_mutation_contract::AcceptanceResult` variant-for-variant. */
export const WireMutationOutcomeSchema = z.discriminatedUnion('outcome', [
  z
    .object({
      outcome: z.literal('accepted'),
      job_id: JobIdSchema,
      correlation_id: CorrelationIdSchema,
      accepted_at_ms: z.number().int(),
    })
    .strict(),
  z
    .object({
      outcome: z.literal('approval_required'),
      approval_id: ApprovalIdSchema,
      bound_request_digest: RequestDigestSchema,
      expires_at_ms: z.number().int(),
    })
    .strict(),
  z
    .object({
      outcome: z.literal('replayed_identical_accepted'),
      job_id: JobIdSchema,
      correlation_id: CorrelationIdSchema,
      accepted_at_ms: z.number().int(),
    })
    .strict(),
  z.object({ outcome: z.literal('rejected'), rejection: WireMutationRejectionSchema }).strict(),
]);

/** The six mutation-backed methods' `MutationMethod` identity on the wire. */
export const OPERATION_METHODS = [
  'restart',
  'flash',
  'contained_build',
  'agent_run_register',
  'agent_run_update',
  'work_intent_acknowledge',
] as const;
const OperationMethodSchema = z.enum(OPERATION_METHODS);

/**
 * Only 10 of the 23 unary methods produce a real result shape — see this
 * module's top doc comment. `runtime_status`/`devices_list`/`device_get`/
 * `device_get_health` mirror `loki_runtime::services::{RuntimeStatus,
 * DevicePage, DeviceHealthProjection}` field-for-field; `mutation` is the
 * unified result shape for all six mutation-backed methods, produced only
 * via a real `AcceptanceResult` mapping (`crates/loki-ipc/src/
 * mutation_mapping.rs`).
 */
export const BusinessRpcResultSchema = z.discriminatedUnion('kind', [
  z
    .object({
      kind: z.literal('runtime_status'),
      meta: ReadMetaSchema,
      storage: WireStorageReadinessSchema,
      registered_device_count: z.number().int().nonnegative(),
      mutations: WireMutationAvailabilitySchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('devices_list'),
      meta: ReadMetaSchema,
      devices: z.array(WireDeviceSummarySchema).max(MAX_RPC_PAGE_SIZE),
      next_after_device_id: DeviceIdSchema.nullable().optional(),
      total_registered_count: z.number().int().nonnegative(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('device_get'),
      meta: ReadMetaSchema,
      device: WireDeviceSummarySchema.nullable(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('device_health'),
      meta: ReadMetaSchema,
      device_id: DeviceIdSchema,
      lifecycle_state: DeviceLifecycleStateSchema,
      health: WireDeviceHealthStateSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('mutation'),
      method: OperationMethodSchema,
      correlation_id: CorrelationIdSchema,
      target_id: SafeIdSchema,
      outcome: WireMutationOutcomeSchema,
    })
    .strict(),
]);
export type BusinessRpcResult = z.infer<typeof BusinessRpcResultSchema>;

export const BUSINESS_STREAM_NAMES = [
  'runtime_status',
  'device_projection',
  'device_health',
  'telemetry',
  'logs',
  'incidents',
  'jobs',
  'approvals',
  'notifications',
  'work_intents',
] as const;
/** `entity_id` binds the stream to the exact entity id it is scoped to — never a workspace/tenant concept. */
export const BusinessStreamRequestSchema = z
  .object({
    stream: z.enum(BUSINESS_STREAM_NAMES),
    entity_id: SafeIdSchema,
    after_sequence: z.number().int().nonnegative().nullable().optional(),
    max_items: z.number().int().min(1).max(MAX_RPC_STREAM_ITEMS),
  })
  .strict();
export const BusinessEventSchema = z
  .object({
    stream: z.enum(BUSINESS_STREAM_NAMES),
    sequence: z.number().int().nonnegative(),
    correlation_id: CorrelationIdSchema,
    occurred_at: IsoUtcTimestampSchema,
    entity_id: SafeIdSchema,
    state: Utf8TextSchema(100),
    source_runtime_id: RuntimeIdSchema,
  })
  .strict();
