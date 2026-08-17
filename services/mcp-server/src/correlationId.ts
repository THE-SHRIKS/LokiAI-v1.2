import { randomUUID } from 'node:crypto';
import { CorrelationIdSchema, type CorrelationId } from '@lokiai/contracts';

/**
 * Mints a new, validated correlation ID for one connection/request
 * diagnostic trail.
 *
 * `Docs/MCP_SPEC.md` Section 4: "Every response includes or can resolve a
 * correlation ID and timestamp." `AGENTS.md` rule 5: "Use stable IDs, UTC
 * timestamps, correlation IDs, and idempotency keys for operations that may
 * be retried."
 *
 * `crypto.randomUUID()`'s output always satisfies `CorrelationIdSchema`'s
 * bounds (`packages/contracts/src/brand.ts`: 1-200 chars,
 * `[A-Za-z0-9._:-]+`), but this still runs the real parse rather than an
 * `as CorrelationId` cast — `AGENTS.md` rule 4: "Types alone are not runtime
 * validation."
 */
export function newCorrelationId(): CorrelationId {
  return CorrelationIdSchema.parse(randomUUID());
}
