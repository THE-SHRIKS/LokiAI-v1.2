import { z } from 'zod';
import {
  ApprovalIdSchema,
  BoundRequestDigestSchema,
  CorrelationIdSchema,
  DeviceIdSchema,
  DeploymentIdSchema,
  IncidentIdSchema,
  JobIdSchema,
  AgentRunIdSchema,
  WorkspaceIdSchema,
} from './ids.js';
import { idSchema } from './brand.js';
import { IsoUtcTimestampSchema } from './timestamp.js';

/**
 * MCP-specific schema shapes per `Docs/MCP_SPEC.md`. Only the shapes
 * `Docs/MCP_SPEC.md` explicitly delegates to this node are covered — Section 6
 * states "Final schemas are owned by CTR-001" for the proposed tool list, and
 * Section 7 gives the two literal result shapes below. Per-tool input/output
 * payload schemas (e.g. the exact fields `loki_request_flash` accepts) are
 * out of scope: no concrete connector exists yet to define them against (the
 * ESP32-S3 reference connector is blocked on hardware confirmation), and
 * inventing them here would be inventing product behavior per `AGENTS.md`.
 */

// --- Pagination cursor (Docs/MCP_SPEC.md Section 4 + Section 7) ---------------------

/** `Docs/MCP_SPEC.md` Section 4: "Pagination uses opaque cursors and bounded limits." */
export const CursorSchema = idSchema('Cursor');
export type Cursor = z.infer<typeof CursorSchema>;

// --- Successful read result (Docs/MCP_SPEC.md Section 7) ----------------------------

/**
 * `Docs/MCP_SPEC.md` Section 7:
 * ```
 * {
 *   data,
 *   source,
 *   observed_at,
 *   freshness,
 *   correlation_id,
 *   next_cursor?
 * }
 * ```
 * `source` and `freshness` have no documented closed value set anywhere in
 * `Docs/` (unlike, say, telemetry sample `quality`, which `Docs/SPEC.md` 3.5
 * pins to `good`/`late`/`gap`/`invalid`). They are intentionally validated as
 * bounded, non-empty strings rather than invented enums; whichever node wires
 * this into the MCP server (`MCP-001`) should pin exact vocabularies there
 * once real read paths exist to observe, per `Docs/graph/handoffs/CTR-001-1.md`.
 */
export function mcpReadResultSchema<DataSchema extends z.ZodTypeAny>(data: DataSchema) {
  return z.object({
    data,
    source: z.string().min(1).max(200),
    observed_at: IsoUtcTimestampSchema,
    freshness: z.string().min(1).max(200),
    correlation_id: CorrelationIdSchema,
    next_cursor: CursorSchema.optional(),
  });
}

export interface McpReadResult<T> {
  data: T;
  source: string;
  observed_at: string;
  freshness: string;
  correlation_id: z.infer<typeof CorrelationIdSchema>;
  next_cursor?: Cursor;
}

// --- Operation request result (Docs/MCP_SPEC.md Section 7) --------------------------

/**
 * `Docs/MCP_SPEC.md` Section 7:
 * ```
 * {
 *   outcome: "rejected" | "approval_required" | "accepted" | "deduplicated",
 *   approval_id?,
 *   job_id?,
 *   bound_request_digest?,
 *   policy_reason,
 *   correlation_id,
 *   status_resource?
 * }
 * ```
 * Transcribed as one flat object exactly as shown — unlike `OperationResult<T>`
 * (`./errors.ts`), the document does not partition fields per `outcome` value
 * here (e.g. `approval_id` is optional regardless of outcome), so this is not
 * a discriminated union.
 */
export const MCP_OPERATION_OUTCOMES = [
  'rejected',
  'approval_required',
  'accepted',
  'deduplicated',
] as const;
/**
 * Standalone validator for the `outcome` discriminant on its own, mirroring
 * `OperationResultOutcomeSchema` in `./errors.ts`. Reused directly by
 * `McpOperationResultSchema` below since — unlike `OperationResult<T>` — this
 * shape is one flat object, not a discriminated union requiring per-branch
 * `z.literal(...)` values.
 */
export const McpOperationOutcomeSchema = z.enum(MCP_OPERATION_OUTCOMES);
export type McpOperationOutcome = z.infer<typeof McpOperationOutcomeSchema>;

export const McpOperationResultSchema = z.object({
  outcome: McpOperationOutcomeSchema,
  approval_id: ApprovalIdSchema.optional(),
  job_id: JobIdSchema.optional(),
  bound_request_digest: BoundRequestDigestSchema.optional(),
  policy_reason: z.string().min(1).max(2000),
  correlation_id: CorrelationIdSchema,
  status_resource: z.string().min(1).max(2000).optional(),
});
export type McpOperationResult = z.infer<typeof McpOperationResultSchema>;

// --- Tool naming (Docs/MCP_SPEC.md Section 4 + Section 6) ---------------------------

/**
 * `Docs/MCP_SPEC.md` Section 4: "Proposed tool prefix: `loki_`." and "Tool
 * names are stable, action-oriented, and provider-neutral." General shape
 * validator — accepts any conforming name, not just the ones known today, so
 * a future MCP-001 tool addition does not require a CTR-001 change merely to
 * pass shape validation.
 */
const MCP_TOOL_NAME_PATTERN = /^loki_[a-z][a-z0-9_]*$/;
export const McpToolNameSchema = z
  .string()
  .max(64)
  .regex(MCP_TOOL_NAME_PATTERN, 'MCP tool names must match loki_[a-z][a-z0-9_]*')
  .brand<'McpToolName'>();
export type McpToolName = z.infer<typeof McpToolNameSchema>;

/**
 * `Docs/MCP_SPEC.md` Section 6 "Proposed tools" — the full accepted v1.2 list,
 * grouped exactly under the document's three named headings. Section 6 also
 * says "Final schemas are owned by CTR-001; this list defines product intent,"
 * so this closed enum records the accepted set; it is a v1.2 product-intent
 * record, not a hard ceiling MCP-001 can never extend — extending it is a
 * contract change for whichever node owns it next, same as any other export
 * here.
 */
export const MCP_TOOL_CATEGORIES = ['inspection', 'diagnosis_and_build', 'consequential'] as const;
export const McpToolCategorySchema = z.enum(MCP_TOOL_CATEGORIES);
export type McpToolCategory = z.infer<typeof McpToolCategorySchema>;

/**
 * Risk is deliberately NOT collapsed to one `low`/`medium`/`high` value per
 * tool: `Docs/MCP_SPEC.md` Section 6 gives "Inspection — low risk" and
 * "Consequential operations — high risk by default" as fixed defaults, but
 * "Diagnosis and build — low/medium risk by policy" explicitly varies by
 * policy. Encoding a single fixed risk value for that middle category would
 * misrepresent the document. The policy engine (`Docs/SPEC.md` Section 2),
 * not this package, owns the actual risk/approval decision.
 */
export const MCP_TOOL_CATEGORY = {
  loki_list_devices: 'inspection',
  loki_get_device: 'inspection',
  loki_get_device_health: 'inspection',
  loki_read_device_logs: 'inspection',
  loki_query_telemetry: 'inspection',
  loki_list_incidents: 'inspection',
  loki_get_incident: 'inspection',
  loki_get_job: 'inspection',
  loki_list_deployments: 'inspection',
  loki_get_deployment: 'inspection',
  loki_run_health_check: 'diagnosis_and_build',
  loki_run_preflight: 'diagnosis_and_build',
  loki_collect_incident_evidence: 'diagnosis_and_build',
  loki_register_agent_run: 'diagnosis_and_build',
  loki_update_agent_run: 'diagnosis_and_build',
  loki_request_build: 'diagnosis_and_build',
  loki_cancel_job: 'diagnosis_and_build',
  loki_request_restart: 'consequential',
  loki_request_flash: 'consequential',
  loki_request_test_on_device: 'consequential',
  loki_request_deployment: 'consequential',
  loki_request_rollback: 'consequential',
} as const satisfies Record<string, McpToolCategory>;

/**
 * `keyof typeof MCP_TOOL_CATEGORY` — a precise closed literal union of the 22
 * accepted tool names, not merely `string`. This depends on
 * `MCP_TOOL_CATEGORY` being declared with `as const satisfies ...` (preserves
 * literal keys) rather than a widening `: Record<string, McpToolCategory>`
 * annotation (which would silently collapse this to plain `string`).
 */
export type KnownMcpToolName = keyof typeof MCP_TOOL_CATEGORY;

/**
 * `Object.keys` is typed `string[]` regardless of the source object's key
 * literal types (a long-standing TypeScript library-signature limitation),
 * so this cast re-asserts the precision `MCP_TOOL_CATEGORY`'s `as const
 * satisfies` already guarantees at runtime: every key really is a
 * `KnownMcpToolName`.
 */
export const KNOWN_MCP_TOOL_NAMES = Object.keys(MCP_TOOL_CATEGORY) as [
  KnownMcpToolName,
  ...KnownMcpToolName[],
];
export const KnownMcpToolNameSchema = z.enum(KNOWN_MCP_TOOL_NAMES);

// --- Resource URIs (Docs/MCP_SPEC.md Section 5) --------------------------------------

/**
 * `Docs/MCP_SPEC.md` Section 5 "Proposed resources" table, one family per row.
 * Parsed structurally so each family's `{id}` segment is validated and typed
 * against the correct branded ID (a device resource's `{id}` is a `DeviceId`,
 * an incident resource's is an `IncidentId`, etc.) instead of a generic
 * string.
 */
export type LokiResourceUri =
  | { family: 'workspace_summary'; workspace_id: z.infer<typeof WorkspaceIdSchema> }
  | { family: 'device'; device_id: z.infer<typeof DeviceIdSchema> }
  | { family: 'device_logs'; device_id: z.infer<typeof DeviceIdSchema> }
  | { family: 'device_telemetry'; device_id: z.infer<typeof DeviceIdSchema> }
  | { family: 'incident'; incident_id: z.infer<typeof IncidentIdSchema> }
  | { family: 'incident_evidence'; incident_id: z.infer<typeof IncidentIdSchema> }
  | { family: 'deployment'; deployment_id: z.infer<typeof DeploymentIdSchema> }
  | { family: 'job'; job_id: z.infer<typeof JobIdSchema> }
  | { family: 'agent_run'; agent_run_id: z.infer<typeof AgentRunIdSchema> };

type ResourceUriRule = {
  pattern: RegExp;
  build: (id: string) => LokiResourceUri | null;
};

const RESOURCE_URI_RULES: readonly ResourceUriRule[] = [
  {
    pattern: /^loki:\/\/workspaces\/([^/]+)\/summary$/,
    build: (id) => {
      const parsed = WorkspaceIdSchema.safeParse(id);
      return parsed.success ? { family: 'workspace_summary', workspace_id: parsed.data } : null;
    },
  },
  {
    pattern: /^loki:\/\/devices\/([^/]+)\/logs$/,
    build: (id) => {
      const parsed = DeviceIdSchema.safeParse(id);
      return parsed.success ? { family: 'device_logs', device_id: parsed.data } : null;
    },
  },
  {
    pattern: /^loki:\/\/devices\/([^/]+)\/telemetry$/,
    build: (id) => {
      const parsed = DeviceIdSchema.safeParse(id);
      return parsed.success ? { family: 'device_telemetry', device_id: parsed.data } : null;
    },
  },
  {
    pattern: /^loki:\/\/devices\/([^/]+)$/,
    build: (id) => {
      const parsed = DeviceIdSchema.safeParse(id);
      return parsed.success ? { family: 'device', device_id: parsed.data } : null;
    },
  },
  {
    pattern: /^loki:\/\/incidents\/([^/]+)\/evidence$/,
    build: (id) => {
      const parsed = IncidentIdSchema.safeParse(id);
      return parsed.success ? { family: 'incident_evidence', incident_id: parsed.data } : null;
    },
  },
  {
    pattern: /^loki:\/\/incidents\/([^/]+)$/,
    build: (id) => {
      const parsed = IncidentIdSchema.safeParse(id);
      return parsed.success ? { family: 'incident', incident_id: parsed.data } : null;
    },
  },
  {
    pattern: /^loki:\/\/deployments\/([^/]+)$/,
    build: (id) => {
      const parsed = DeploymentIdSchema.safeParse(id);
      return parsed.success ? { family: 'deployment', deployment_id: parsed.data } : null;
    },
  },
  {
    pattern: /^loki:\/\/jobs\/([^/]+)$/,
    build: (id) => {
      const parsed = JobIdSchema.safeParse(id);
      return parsed.success ? { family: 'job', job_id: parsed.data } : null;
    },
  },
  {
    pattern: /^loki:\/\/agents\/runs\/([^/]+)$/,
    build: (id) => {
      const parsed = AgentRunIdSchema.safeParse(id);
      return parsed.success ? { family: 'agent_run', agent_run_id: parsed.data } : null;
    },
  },
];

/**
 * Parses a `loki://` resource URI into its typed family + branded ID, or
 * returns `null` for anything that does not match a known family or whose
 * `{id}` segment fails that family's ID validation (empty, oversized, or
 * unsupported characters). Never throws — this is the runtime validator
 * `AGENTS.md` rule 4 requires alongside the `LokiResourceUri` type above.
 */
export function parseLokiResourceUri(uri: unknown): LokiResourceUri | null {
  if (typeof uri !== 'string') {
    return null;
  }
  for (const rule of RESOURCE_URI_RULES) {
    const match = rule.pattern.exec(uri);
    if (match?.[1] !== undefined) {
      const parsed = rule.build(match[1]);
      if (parsed) {
        return parsed;
      }
    }
  }
  return null;
}
