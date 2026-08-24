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
import {
  DeviceLifecycleStateSchema,
  JobStateSchema,
  RuntimeConnectionStateSchema,
} from './states.js';
import { HealthCheckStatusSchema } from './connector.js';

/** Current local business RPC protocol. Version 1.0 remains the bounded transport-only edge. */
export const BUSINESS_RPC_PROTOCOL_VERSION = '1.1' as const;
export const BUSINESS_RPC_CAPABILITY = 'loki-ipc.business.v1' as const;
export const MAX_RPC_PAGE_SIZE = 100;
export const MAX_RPC_TEXT_BYTES = 2_000;
export const MAX_RPC_STREAM_ITEMS = 1_024;

const IdempotencyKeySchema = z
  .string()
  .min(1)
  .max(200)
  .regex(/^[A-Za-z0-9._:-]+$/);
const CursorSchema = z
  .string()
  .min(1)
  .max(512)
  .regex(/^[A-Za-z0-9._~:-]+$/);
const PageSchema = z
  .object({
    limit: z.number().int().min(1).max(MAX_RPC_PAGE_SIZE),
    cursor: CursorSchema.nullable().optional(),
  })
  .strict();
const DeviceSchema = z.object({ device_id: DeviceIdSchema }).strict();
const DevicePageSchema = PageSchema;
const DeviceBoundedReadSchema = DeviceSchema.extend({
  limit: z.number().int().min(1).max(MAX_RPC_PAGE_SIZE),
  cursor: CursorSchema.nullable().optional(),
}).strict();
const OperationBaseSchema = z
  .object({ device_id: DeviceIdSchema, idempotency_key: IdempotencyKeySchema })
  .strict();

const BuildRequestSchema = z
  .object({
    source_snapshot_id: z
      .string()
      .min(1)
      .max(200)
      .regex(/^[A-Za-z0-9._:-]+$/),
    target_id: z
      .string()
      .min(1)
      .max(200)
      .regex(/^[A-Za-z0-9._:-]+$/),
    recipe_id: z
      .string()
      .min(1)
      .max(200)
      .regex(/^[A-Za-z0-9._:-]+$/),
    idempotency_key: IdempotencyKeySchema,
  })
  .strict();
const ArtifactOperationSchema = OperationBaseSchema.extend({
  artifact_id: ArtifactIdSchema,
  artifact_digest: ArtifactDigestSchema,
}).strict();

/** Explicit method table. No string-selected invoke, executable, path, URL, environment, or shell field exists. */
export const BusinessRpcRequestSchema = z.discriminatedUnion('method', [
  z.object({ method: z.literal('runtime_get_status'), params: z.object({}).strict() }).strict(),
  z.object({ method: z.literal('devices_list'), params: DevicePageSchema }).strict(),
  z.object({ method: z.literal('device_get'), params: DeviceSchema }).strict(),
  z.object({ method: z.literal('device_get_health'), params: DeviceBoundedReadSchema }).strict(),
  z.object({ method: z.literal('device_read_logs'), params: DeviceBoundedReadSchema }).strict(),
  z
    .object({
      method: z.literal('device_query_telemetry'),
      params: DeviceBoundedReadSchema.extend({
        signal_ids: z.array(z.string().min(1).max(128)).max(64),
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
          summary: z.string().min(1).max(MAX_RPC_TEXT_BYTES),
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
          idempotency_key: IdempotencyKeySchema,
        })
        .strict(),
    })
    .strict(),
  z.object({ method: z.literal('health_check_request'), params: OperationBaseSchema }).strict(),
  z.object({ method: z.literal('preflight_request'), params: OperationBaseSchema }).strict(),
  z
    .object({
      method: z.literal('job_cancel'),
      params: z.object({ job_id: JobIdSchema, idempotency_key: IdempotencyKeySchema }).strict(),
    })
    .strict(),
  z.object({ method: z.literal('build_request'), params: BuildRequestSchema }).strict(),
  z.object({ method: z.literal('restart_request'), params: OperationBaseSchema }).strict(),
  z.object({ method: z.literal('flash_request'), params: ArtifactOperationSchema }).strict(),
  z
    .object({
      method: z.literal('test_on_device_request'),
      params: OperationBaseSchema.extend({
        test_suite_id: z
          .string()
          .min(1)
          .max(200)
          .regex(/^[A-Za-z0-9._:-]+$/),
      }).strict(),
    })
    .strict(),
  z
    .object({
      method: z.literal('deployment_request'),
      params: ArtifactOperationSchema.extend({
        verification_plan_id: z.string().min(1).max(200),
        rollback_plan_id: z.string().min(1).max(200),
      }).strict(),
    })
    .strict(),
  z
    .object({
      method: z.literal('rollback_request'),
      params: OperationBaseSchema.extend({
        deployment_id: DeploymentIdSchema,
        recovery_policy_id: z.string().min(1).max(200),
      }).strict(),
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
          reason: z.string().min(1).max(MAX_RPC_TEXT_BYTES),
          idempotency_key: IdempotencyKeySchema,
        })
        .strict(),
    })
    .strict(),
]);
export type BusinessRpcRequest = z.infer<typeof BusinessRpcRequestSchema>;

export const PRINCIPAL_KINDS = [
  'desktop_installation',
  'agent_connection',
  'runtime_service',
] as const;
export const AuthenticatedPrincipalSchema = z
  .object({
    kind: z.enum(PRINCIPAL_KINDS),
    principal_id: z.string().min(1).max(200),
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
          'subscribe',
        ]),
      )
      .max(13),
  })
  .strict();

const ReadMetaSchema = z
  .object({
    source_runtime_id: RuntimeIdSchema,
    observed_at: IsoUtcTimestampSchema,
    correlation_id: CorrelationIdSchema,
  })
  .strict();
const DeviceSummarySchema = z
  .object({
    device_id: DeviceIdSchema,
    lifecycle_state: DeviceLifecycleStateSchema,
    connector_id: z.string().min(1).max(200),
    last_seen_at: IsoUtcTimestampSchema,
  })
  .strict();
const IncidentSummarySchema = z
  .object({
    incident_id: IncidentIdSchema,
    state: z.enum([
      'open',
      'investigating',
      'waiting-approval',
      'executing',
      'verifying',
      'resolved',
      'closed',
    ]),
    severity: z.string().min(1).max(100),
    updated_at: IsoUtcTimestampSchema,
  })
  .strict();
const JobSummarySchema = z
  .object({
    job_id: JobIdSchema,
    state: JobStateSchema,
    updated_at: IsoUtcTimestampSchema,
    error_code: z.string().min(1).max(100).nullable().optional(),
  })
  .strict();
const OperationMethodSchema = z.enum([
  'health_check_request',
  'preflight_request',
  'job_cancel',
  'build_request',
  'restart_request',
  'flash_request',
  'test_on_device_request',
  'deployment_request',
  'rollback_request',
  'approval_grant',
  'approval_deny',
]);

export const BusinessRpcResultSchema = z.discriminatedUnion('kind', [
  z
    .object({
      kind: z.literal('runtime_status'),
      meta: ReadMetaSchema,
      state: RuntimeConnectionStateSchema,
      runtime_id: RuntimeIdSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('devices_list'),
      meta: ReadMetaSchema,
      devices: z.array(DeviceSummarySchema).max(MAX_RPC_PAGE_SIZE),
      next_cursor: CursorSchema.nullable().optional(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('device_get'),
      meta: ReadMetaSchema,
      device: DeviceSummarySchema.nullable(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('device_health'),
      meta: ReadMetaSchema,
      device_id: DeviceIdSchema,
      entries: z
        .array(
          z
            .object({
              check_id: z.string().min(1).max(128),
              status: HealthCheckStatusSchema,
              observed_at: IsoUtcTimestampSchema,
              evidence_ref: z.string().min(1).max(512).nullable().optional(),
            })
            .strict(),
        )
        .max(MAX_RPC_PAGE_SIZE),
      next_cursor: CursorSchema.nullable().optional(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('device_logs'),
      meta: ReadMetaSchema,
      device_id: DeviceIdSchema,
      entries: z
        .array(
          z
            .object({
              sequence: z.number().int().nonnegative(),
              observed_at: IsoUtcTimestampSchema,
              level: z.string().min(1).max(32),
              message: z.string().max(MAX_RPC_TEXT_BYTES),
              untrusted: z.boolean(),
            })
            .strict(),
        )
        .max(MAX_RPC_PAGE_SIZE),
      next_cursor: CursorSchema.nullable().optional(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('device_telemetry'),
      meta: ReadMetaSchema,
      device_id: DeviceIdSchema,
      entries: z
        .array(
          z
            .object({
              sequence: z.number().int().nonnegative(),
              signal_id: z.string().min(1).max(128),
              observed_at: IsoUtcTimestampSchema,
              value_text: z.string().max(512),
              unit: z.string().min(1).max(64).nullable().optional(),
              quality: z.string().min(1).max(64),
            })
            .strict(),
        )
        .max(MAX_RPC_PAGE_SIZE),
      next_cursor: CursorSchema.nullable().optional(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('incidents_list'),
      meta: ReadMetaSchema,
      incidents: z.array(IncidentSummarySchema).max(MAX_RPC_PAGE_SIZE),
      next_cursor: CursorSchema.nullable().optional(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('incident_get'),
      meta: ReadMetaSchema,
      incident: IncidentSummarySchema.nullable(),
    })
    .strict(),
  z
    .object({ kind: z.literal('job_get'), meta: ReadMetaSchema, job: JobSummarySchema.nullable() })
    .strict(),
  z
    .object({
      kind: z.literal('agent_run'),
      meta: ReadMetaSchema,
      run_id: AgentRunIdSchema,
      state: z.enum(['planning', 'working', 'waiting', 'completed', 'failed']),
    })
    .strict(),
  z
    .object({
      kind: z.literal('work_intent_acknowledged'),
      meta: ReadMetaSchema,
      work_intent_id: WorkIntentIdSchema,
      run_id: AgentRunIdSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('operation'),
      method: OperationMethodSchema,
      outcome: z.enum(['rejected', 'approval_required', 'accepted', 'deduplicated']),
      correlation_id: CorrelationIdSchema,
      policy_reason: z.string().min(1).max(MAX_RPC_TEXT_BYTES),
      approval_id: ApprovalIdSchema.nullable().optional(),
      job_id: JobIdSchema.nullable().optional(),
      bound_request_digest: BoundRequestDigestSchema.nullable().optional(),
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
export const BusinessStreamRequestSchema = z
  .object({
    stream: z.enum(BUSINESS_STREAM_NAMES),
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
    entity_id: z.string().min(1).max(200),
    state: z.string().min(1).max(100),
    source_runtime_id: RuntimeIdSchema,
  })
  .strict();
