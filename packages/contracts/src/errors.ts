import { z } from 'zod';
import {
  ApprovalIdSchema,
  BoundRequestDigestSchema,
  CorrelationIdSchema,
  JobIdSchema,
} from './ids.js';
import { JobStateSchema } from './states.js';
import { IsoUtcTimestampSchema } from './timestamp.js';

/**
 * `Docs/SPEC.md` Section 1 "Error" object and "Common operation result"
 * `OperationResult<T>` — the shared success/error discriminated union used by
 * both the local IPC protocol and MCP responses (CTR-001 Deliverable 3).
 * Named `LokiError` here, not `Error`, to avoid shadowing the JavaScript
 * global `Error` type.
 */

// --- Error (Docs/SPEC.md Section 1) --------------------------------------------------

/**
 * `Docs/MCP_SPEC.md` Section 7: "Errors use stable codes such as
 * authentication required, permission denied, workspace mismatch, runtime
 * unavailable, device unavailable, unsupported capability, device busy,
 * approval required/expired, artifact mismatch, invalid state, version
 * incompatible, rate limited, and internal correlation error." The word
 * "such as" makes this an illustrative, non-exhaustive list — `LokiErrorSchema`
 * therefore validates `code` as a bounded slug rather than a closed enum, and
 * `KNOWN_ERROR_CODES` exposes the named examples as a convenience constant,
 * not a closed set new error codes must be added here to use.
 */
export const KNOWN_ERROR_CODES = [
  'authentication_required',
  'permission_denied',
  'workspace_mismatch',
  'runtime_unavailable',
  'device_unavailable',
  'unsupported_capability',
  'device_busy',
  'approval_required',
  'approval_expired',
  'artifact_mismatch',
  'invalid_state',
  'version_incompatible',
  'rate_limited',
  'internal_correlation_error',
] as const;
/**
 * Closed-enum validator for a caller that specifically wants to check
 * membership in the documented example codes (e.g. an exhaustive `switch`
 * with a fallback default for anything else). `LokiErrorSchema.code` itself
 * intentionally uses the open `ERROR_CODE_PATTERN` slug check below, not this
 * schema, so a legitimately new error code is never rejected as malformed.
 */
export const KnownErrorCodeSchema = z.enum(KNOWN_ERROR_CODES);
export type KnownErrorCode = z.infer<typeof KnownErrorCodeSchema>;

const ERROR_CODE_PATTERN = /^[a-z][a-z0-9_]*$/;

/**
 * `Docs/SPEC.md` Section 1:
 * ```
 * Error {
 *   code
 *   category
 *   message
 *   retryable
 *   retry_after_ms?
 *   details?          // redacted, schema-bound
 *   correlation_id
 * }
 * ```
 * `category` has no documented value set anywhere in `Docs/`, so it is
 * validated as a bounded slug only (open vocabulary), matching `code`.
 * `details` is validated as a plain string-keyed record — its *contents* are
 * redacted and schema-bound by the producer per `AGENTS.md` rule 9; this
 * package cannot know a given error's redaction schema in advance.
 */
export const LokiErrorSchema = z.object({
  code: z
    .string()
    .min(1)
    .max(100)
    .regex(ERROR_CODE_PATTERN, 'code must be a lowercase snake_case slug'),
  category: z
    .string()
    .min(1)
    .max(100)
    .regex(ERROR_CODE_PATTERN, 'category must be a lowercase snake_case slug'),
  message: z.string().min(1).max(2000),
  retryable: z.boolean(),
  retry_after_ms: z.number().int().nonnegative().optional(),
  details: z.record(z.string(), z.unknown()).optional(),
  correlation_id: CorrelationIdSchema,
});
export type LokiError = z.infer<typeof LokiErrorSchema>;

// --- OperationResult<T> (Docs/SPEC.md Section 1) -------------------------------------

/**
 * `Docs/SPEC.md` Section 1:
 * ```
 * OperationResult<T> =
 *   | accepted { job_id, state, accepted_at }
 *   | approval_required { approval_id, bound_request_digest, expires_at }
 *   | rejected { error }
 *   | completed { job_id, evidence_refs[], result: T }
 * ```
 * The document shows named variants without an explicit discriminant field;
 * `outcome` is used here as the discriminant key, matching the literal
 * `outcome` field `Docs/MCP_SPEC.md` Section 7 uses for its closely related
 * "Operation request" result shape (see `./mcp.ts`).
 */
export const OPERATION_RESULT_OUTCOMES = [
  'accepted',
  'approval_required',
  'rejected',
  'completed',
] as const;
/**
 * Standalone validator for the `outcome` discriminant on its own (e.g. a
 * caller that only has the tag, not a full envelope, to check). The
 * composite `operationResultSchema(...)` below cannot reuse this directly:
 * `z.discriminatedUnion` requires each branch to declare its own distinct
 * `z.literal(...)` for the discriminant so it can build a dispatch table,
 * not a shared multi-value enum.
 */
export const OperationResultOutcomeSchema = z.enum(OPERATION_RESULT_OUTCOMES);
export type OperationResultOutcome = z.infer<typeof OperationResultOutcomeSchema>;

const AcceptedOperationResultSchema = z.object({
  outcome: z.literal('accepted'),
  job_id: JobIdSchema,
  state: JobStateSchema,
  accepted_at: IsoUtcTimestampSchema,
});

const ApprovalRequiredOperationResultSchema = z.object({
  outcome: z.literal('approval_required'),
  approval_id: ApprovalIdSchema,
  bound_request_digest: BoundRequestDigestSchema,
  expires_at: IsoUtcTimestampSchema,
});

const RejectedOperationResultSchema = z.object({
  outcome: z.literal('rejected'),
  error: LokiErrorSchema,
});

/** Runtime validator factory for `OperationResult<T>` — see `AGENTS.md` rule 4. */
export function operationResultSchema<ResultSchema extends z.ZodTypeAny>(result: ResultSchema) {
  const completed = z.object({
    outcome: z.literal('completed'),
    job_id: JobIdSchema,
    evidence_refs: z.array(z.string().min(1).max(2000)),
    result,
  });

  return z.discriminatedUnion('outcome', [
    AcceptedOperationResultSchema,
    ApprovalRequiredOperationResultSchema,
    RejectedOperationResultSchema,
    completed,
  ]);
}

export type OperationResult<T> =
  | z.infer<typeof AcceptedOperationResultSchema>
  | z.infer<typeof ApprovalRequiredOperationResultSchema>
  | z.infer<typeof RejectedOperationResultSchema>
  | {
      outcome: 'completed';
      job_id: z.infer<typeof JobIdSchema>;
      evidence_refs: string[];
      result: T;
    };
