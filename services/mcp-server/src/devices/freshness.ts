/**
 * `Docs/MCP_SPEC.md` Section 7's successful-read result carries a `freshness`
 * field; `packages/contracts/src/mcp.ts`'s module doc explicitly defers
 * pinning a concrete vocabulary for it: "whichever node wires this into the
 * MCP server (`MCP-001`) should pin exact vocabularies there once real read
 * paths exist to observe." This node is that wiring point for the device
 * tools, so this module pins one — scoped locally (not promoted into
 * `@lokiai/contracts`, outside this node's writable scope) until a future
 * node needs to share it more broadly.
 *
 * Only exercised when a `DeviceDataSource` actually returns data
 * (`available: true`) — at this milestone that is exclusively test fixtures
 * (`src/devices/runtimeUnavailableDataSource.ts` is the only production
 * implementation and never reaches this code), so the exact threshold below
 * is a placeholder default a future real-backend node should revisit against
 * real observed staleness, not a tuned production value.
 */

export const FRESHNESS_STALE_THRESHOLD_MS = 30_000;

export const FRESHNESS_VALUES = ['fresh', 'stale'] as const;
export type Freshness = (typeof FRESHNESS_VALUES)[number];

/**
 * `observedAtIso` is trusted input from a `DeviceDataSource` implementation
 * (not untrusted client input), but this still never throws on an
 * unparseable timestamp — it reports `stale` rather than crashing the tool
 * call, per `AGENTS.md` rule 1's "expose an honest ... error state" over an
 * unhandled exception.
 */
export function computeFreshness(observedAtIso: string, nowMs: number = Date.now()): Freshness {
  const observedMs = Date.parse(observedAtIso);
  if (Number.isNaN(observedMs)) {
    return 'stale';
  }
  return nowMs - observedMs <= FRESHNESS_STALE_THRESHOLD_MS ? 'fresh' : 'stale';
}
