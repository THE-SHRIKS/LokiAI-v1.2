import { z } from 'zod';
import {
  CausationIdSchema,
  CorrelationIdSchema,
  MessageIdSchema,
  WorkspaceIdSchema,
} from './ids.js';
import { IsoUtcTimestampSchema } from './timestamp.js';

/**
 * `Docs/SPEC.md` Section 1 "Common envelope" — the shared cross-process
 * message wrapper both the local IPC protocol and MCP responses build on
 * (`Docs/ARCHITECTURE.md` Section 5: "Contract definitions live in
 * packages/contracts and generate or validate both TypeScript and Rust
 * representations"). This module defines the *logical* envelope shape only;
 * physical wire framing (length-prefixing, handshake mechanics) is a
 * transport concern owned by `RUN-002`, not this package.
 */

// --- Actor (Docs/SPEC.md Section 2 "Actors and authority") --------------------------

/**
 * `Docs/SPEC.md` Section 2 table rows, in table order. "Reviewer/release
 * owner" is slugged to `reviewer` for a stable identifier.
 */
export const ACTOR_TYPES = [
  'human_user',
  'coding_agent',
  'desktop_main',
  'runtime',
  'connector',
  'policy_engine',
  'cloud_control_plane',
  'reviewer',
] as const;
export const ActorTypeSchema = z.enum(ACTOR_TYPES);
export type ActorType = z.infer<typeof ActorTypeSchema>;

/**
 * `Docs/SPEC.md` Section 2: "An actor's display name is not an authorization
 * identity. Authorization uses stable IDs, signed/scoped credentials, and
 * current policy." `actor_id` is intentionally a generic bounded opaque
 * string rather than one specific branded ID type: which ID space it lives in
 * (a human account ID, a runtime installation ID, a connector ID, ...)
 * depends on `actor_type`, and forcing one brand here would misrepresent that.
 * `display_name` is explicitly documented as non-authoritative so a consumer
 * cannot mistake it for an authorization check.
 */
export const ActorSchema = z.object({
  actor_type: ActorTypeSchema,
  actor_id: z.string().min(1).max(200),
  /** Not an authorization identity — see `Docs/SPEC.md` Section 2. */
  display_name: z.string().min(1).max(200).optional(),
});
export type Actor = z.infer<typeof ActorSchema>;

// --- Envelope<T> (Docs/SPEC.md Section 1) --------------------------------------------

/**
 * `Docs/SPEC.md` Section 1:
 * ```
 * Envelope<T> {
 *   protocol_version
 *   message_id
 *   correlation_id
 *   causation_id?
 *   workspace_id
 *   occurred_at
 *   actor
 *   payload: T
 * }
 * ```
 * `protocol_version` is validated as a bounded non-empty string rather than a
 * specific format (semver, date-based like MCP's `2026-07-28`, ...) because
 * `Docs/SPEC.md` does not pin one scheme for this cross-cutting envelope.
 */
export interface Envelope<T> {
  protocol_version: string;
  message_id: z.infer<typeof MessageIdSchema>;
  correlation_id: z.infer<typeof CorrelationIdSchema>;
  causation_id?: z.infer<typeof CausationIdSchema>;
  workspace_id: z.infer<typeof WorkspaceIdSchema>;
  occurred_at: string;
  actor: Actor;
  payload: T;
}

/** Runtime validator factory for `Envelope<T>` — see `AGENTS.md` rule 4. */
export function envelopeSchema<PayloadSchema extends z.ZodTypeAny>(payload: PayloadSchema) {
  return z.object({
    protocol_version: z.string().min(1).max(50),
    message_id: MessageIdSchema,
    correlation_id: CorrelationIdSchema,
    causation_id: CausationIdSchema.optional(),
    workspace_id: WorkspaceIdSchema,
    occurred_at: IsoUtcTimestampSchema,
    actor: ActorSchema,
    payload,
  });
}
