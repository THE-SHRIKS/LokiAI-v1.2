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
    source_snapshot_id: SafeIdSchema,
    target_id: SafeIdSchema,
    recipe_id: SafeIdSchema,
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
        test_suite_id: SafeIdSchema,
      }).strict(),
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
      params: OperationBaseSchema.extend({
        deployment_id: DeploymentIdSchema,
        recovery_policy_id: Utf8TextSchema(200),
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
          reason: Utf8TextSchema(MAX_RPC_TEXT_BYTES),
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
    principal_id: SafeIdSchema,
    workspace_id: SafeIdSchema,
    scope_ids: z
      .array(SafeIdSchema)
      .min(1)
      .max(128)
      .refine((values) => new Set(values).size === values.length, 'scope_ids must be unique'),
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
    connector_id: Utf8TextSchema(200),
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
    severity: Utf8TextSchema(100),
    updated_at: IsoUtcTimestampSchema,
  })
  .strict();
const JobSummarySchema = z
  .object({
    job_id: JobIdSchema,
    state: JobStateSchema,
    updated_at: IsoUtcTimestampSchema,
    error_code: Utf8TextSchema(100).nullable().optional(),
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
              check_id: Utf8TextSchema(128),
              status: HealthCheckStatusSchema,
              observed_at: IsoUtcTimestampSchema,
              evidence_ref: Utf8TextSchema(512).nullable().optional(),
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
              level: Utf8TextSchema(32),
              message: Utf8TextSchema(MAX_RPC_TEXT_BYTES, true),
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
              signal_id: Utf8TextSchema(128),
              observed_at: IsoUtcTimestampSchema,
              value_text: Utf8TextSchema(512, true),
              unit: Utf8TextSchema(64).nullable().optional(),
              quality: Utf8TextSchema(64),
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
      policy_reason: Utf8TextSchema(MAX_RPC_TEXT_BYTES),
      idempotency_key: IdempotencyKeySchema,
      artifact_digest: ArtifactDigestSchema.nullable(),
      request_reason: Utf8TextSchema(MAX_RPC_TEXT_BYTES).nullable(),
      approval_id: ApprovalIdSchema.nullable().optional(),
      job_id: JobIdSchema.nullable().optional(),
      bound_request_digest: BoundRequestDigestSchema.nullable().optional(),
      scope_ids: z
        .array(SafeIdSchema)
        .min(1)
        .max(16)
        .refine((values) => new Set(values).size === values.length, 'scope_ids must be unique'),
    })
    .strict(),
]);
export type BusinessRpcResult = z.infer<typeof BusinessRpcResultSchema>;

const entityScope = (kind: string, id: string) => `${kind}:${id}`;

export function requiredBusinessRequestScopes(
  request: BusinessRpcRequest,
  workspaceId: string,
): string[] {
  switch (request.method) {
    case 'runtime_get_status':
    case 'devices_list':
    case 'incidents_list':
      return [entityScope('workspace', workspaceId)];
    case 'device_get':
    case 'device_get_health':
    case 'device_read_logs':
    case 'device_query_telemetry':
    case 'health_check_request':
    case 'preflight_request':
    case 'restart_request':
      return [entityScope('device', request.params.device_id)];
    case 'incident_get':
      return [entityScope('incident', request.params.incident_id)];
    case 'job_get':
    case 'job_cancel':
      return [entityScope('job', request.params.job_id)];
    case 'agent_run_register':
      return [
        request.params.parent_run_id == null
          ? entityScope('workspace', workspaceId)
          : entityScope('agent_run', request.params.parent_run_id),
      ];
    case 'agent_run_update':
      return [entityScope('agent_run', request.params.run_id)];
    case 'work_intent_acknowledge':
      return [
        entityScope('work_intent', request.params.work_intent_id),
        entityScope('agent_run', request.params.run_id),
      ];
    case 'build_request':
      return [
        entityScope('source_snapshot', request.params.source_snapshot_id),
        entityScope('target', request.params.target_id),
        entityScope('recipe', request.params.recipe_id),
      ];
    case 'flash_request':
      return [
        entityScope('device', request.params.device_id),
        entityScope('artifact', request.params.artifact_id),
      ];
    case 'test_on_device_request':
      return [
        entityScope('device', request.params.device_id),
        entityScope('test_suite', request.params.test_suite_id),
      ];
    case 'deployment_request':
      return [
        entityScope('device', request.params.device_id),
        entityScope('artifact', request.params.artifact_id),
        entityScope('verification_plan', request.params.verification_plan_id),
        entityScope('rollback_plan', request.params.rollback_plan_id),
      ];
    case 'rollback_request':
      return [
        entityScope('device', request.params.device_id),
        entityScope('deployment', request.params.deployment_id),
        entityScope('recovery_policy', request.params.recovery_policy_id),
      ];
    case 'approval_grant':
    case 'approval_deny':
      return [entityScope('approval', request.params.approval_id)];
  }
}

const OPERATION_METHODS = new Set([
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

export function businessResultMatchesRequest(
  requestInput: unknown,
  resultInput: unknown,
  workspaceId: string,
): boolean {
  const parsedRequest = BusinessRpcRequestSchema.safeParse(requestInput);
  const parsedResult = BusinessRpcResultSchema.safeParse(resultInput);
  if (!parsedRequest.success || !parsedResult.success) return false;
  const request = parsedRequest.data;
  const result = parsedResult.data;

  if (OPERATION_METHODS.has(request.method)) {
    if (result.kind !== 'operation' || result.method !== request.method) return false;
    const operationParams = request.params as { idempotency_key: string };
    const expectedScopes = new Set(requiredBusinessRequestScopes(request, workspaceId));
    if (
      result.scope_ids.length !== expectedScopes.size ||
      result.scope_ids.some((scope) => !expectedScopes.has(scope)) ||
      result.idempotency_key !== operationParams.idempotency_key
    ) {
      return false;
    }
    const expectedArtifact =
      request.method === 'flash_request' || request.method === 'deployment_request'
        ? request.params.artifact_digest
        : null;
    if (result.artifact_digest !== expectedArtifact) return false;
    const expectedReason = request.method === 'approval_deny' ? request.params.reason : null;
    if (result.request_reason !== expectedReason) return false;
    if (
      (request.method === 'approval_grant' || request.method === 'approval_deny') &&
      result.bound_request_digest !== request.params.bound_request_digest
    ) {
      return false;
    }
    return true;
  }

  switch (request.method) {
    case 'runtime_get_status':
      return result.kind === 'runtime_status';
    case 'devices_list':
      return result.kind === 'devices_list';
    case 'device_get':
      return (
        result.kind === 'device_get' &&
        (result.device === null || result.device.device_id === request.params.device_id)
      );
    case 'device_get_health':
      return result.kind === 'device_health' && result.device_id === request.params.device_id;
    case 'device_read_logs':
      return result.kind === 'device_logs' && result.device_id === request.params.device_id;
    case 'device_query_telemetry':
      return result.kind === 'device_telemetry' && result.device_id === request.params.device_id;
    case 'incidents_list':
      return result.kind === 'incidents_list';
    case 'incident_get':
      return (
        result.kind === 'incident_get' &&
        (result.incident === null || result.incident.incident_id === request.params.incident_id)
      );
    case 'job_get':
      return (
        result.kind === 'job_get' &&
        (result.job === null || result.job.job_id === request.params.job_id)
      );
    case 'agent_run_register':
      return result.kind === 'agent_run';
    case 'agent_run_update':
      return result.kind === 'agent_run' && result.run_id === request.params.run_id;
    case 'work_intent_acknowledge':
      return (
        result.kind === 'work_intent_acknowledged' &&
        result.work_intent_id === request.params.work_intent_id &&
        result.run_id === request.params.run_id
      );
  }
  return false;
}

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
    workspace_id: SafeIdSchema,
    scope_id: SafeIdSchema,
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
