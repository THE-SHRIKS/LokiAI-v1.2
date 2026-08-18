import { CursorSchema, type Cursor } from '@lokiai/contracts';

/**
 * `Docs/MCP_SPEC.md` Section 4: "Pagination uses opaque cursors and bounded
 * limits." `Docs/graph/tasks/MCP-002.md` Deliverable 3/4: listing tools
 * support a bounded page size and no single tool response can be unbounded.
 *
 * Cursors here encode a simple `{ offset }` page marker, base64url-encoded so
 * the wire value is opaque to a client (it should not construct or interpret
 * one itself) while still satisfying `packages/contracts/src/mcp.ts`'s
 * `CursorSchema` (`idSchema('Cursor')`, `[A-Za-z0-9._:-]+`) — base64url's
 * alphabet (`A-Za-z0-9-_`, no `=` padding) is a strict subset of that
 * charset, so no separate encoding scheme is needed to satisfy both.
 */

export const DEFAULT_PAGE_SIZE = 20;
export const MAX_PAGE_SIZE = 100;

/** Deliverable 4's size bound for `loki_get_device_health`'s evidence list. */
export const DEFAULT_MAX_HEALTH_ENTRIES = 20;
export const MAX_HEALTH_ENTRIES = 200;

/**
 * Clamps a caller-requested page size into `[1, MAX_PAGE_SIZE]`, defaulting
 * to `DEFAULT_PAGE_SIZE` when omitted. The MCP tool's own zod input schema
 * already rejects an out-of-range `limit` at the protocol boundary (a
 * malformed-request case, `Docs/graph/tasks/MCP-002.md` Deliverable 6); this
 * clamp is a defensive second bound so no code path — including a future
 * caller that bypasses the zod schema — can ever request an unbounded page.
 */
export function clampLimit(requested: number | undefined): number {
  if (requested === undefined) {
    return DEFAULT_PAGE_SIZE;
  }
  return Math.min(Math.max(Math.trunc(requested), 1), MAX_PAGE_SIZE);
}

/** Same defense-in-depth bound as `clampLimit`, for `max_entries`. */
export function clampMaxEntries(requested: number | undefined): number {
  if (requested === undefined) {
    return DEFAULT_MAX_HEALTH_ENTRIES;
  }
  return Math.min(Math.max(Math.trunc(requested), 1), MAX_HEALTH_ENTRIES);
}

export interface DecodedCursor {
  readonly offset: number;
}

function isDecodedCursorShape(value: unknown): value is DecodedCursor {
  if (typeof value !== 'object' || value === null || !('offset' in value)) {
    return false;
  }
  const offset: unknown = value.offset;
  return typeof offset === 'number' && Number.isInteger(offset) && offset >= 0;
}

/**
 * Decodes an opaque page cursor. Returns `{ offset: 0 }` (the first page)
 * when `cursor` is `undefined` (no cursor supplied — a fresh list request),
 * and `null` when `cursor` is present but not a token this server actually
 * issued (garbage/tampered/foreign cursor) — a domain-level malformed-input
 * case distinct from the protocol-level schema check `CursorSchema` already
 * performs on the raw string shape (`Docs/graph/tasks/MCP-002.md`
 * Deliverable 6's malformed-request test coverage).
 */
export function decodeCursor(cursor: string | undefined): DecodedCursor | null {
  if (cursor === undefined) {
    return { offset: 0 };
  }
  let parsed: unknown;
  try {
    const json = Buffer.from(cursor, 'base64url').toString('utf8');
    parsed = JSON.parse(json) as unknown;
  } catch {
    return null;
  }
  return isDecodedCursorShape(parsed) ? parsed : null;
}

/** Encodes a page offset into the opaque cursor wire format `decodeCursor` reads back. */
export function encodeCursor(offset: number): Cursor {
  const raw = Buffer.from(JSON.stringify({ offset }), 'utf8').toString('base64url');
  return CursorSchema.parse(raw);
}
