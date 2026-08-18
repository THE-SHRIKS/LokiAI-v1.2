import { z } from 'zod';
import {
  DeviceIdSchema,
  DeviceLifecycleStateSchema,
  type DeviceId,
  type DeviceLifecycleState,
  type HealthCheckStatus,
  HealthCheckStatusSchema,
  type LokiError,
} from '@lokiai/contracts';

/**
 * MCP-002 — shared device/health domain types this node's tools read from,
 * plus the pluggable `DeviceDataSource` boundary a future real backend
 * implements.
 *
 * `Docs/graph/tasks/MCP-002.md` Section 3 names
 * `crates/loki-connector-registry/src/{registry,health,projection}.rs` as
 * the Rust counterpart data shapes to mirror. Two shapes below are direct,
 * deliberate mirrors of that crate (not invented here):
 *
 * - `AggregatedHealthStatus` mirrors `loki-connector-registry`'s own
 *   `AggregatedHealthStatus` enum (`health.rs`) verbatim (lowercased for this
 *   package's wire-value convention, matching `Docs/CONNECTOR_SPEC.md`
 *   Section 2's precedent for this repo's casing style).
 * - `DeviceHealthEvidenceEntry` mirrors `loki-connectors-sdk`'s
 *   `HealthCheckEntry` (`crates/loki-connectors-sdk/src/health.rs`) field for
 *   field, since that struct is `Docs/CONNECTOR_SPEC.md` Section 7's "Health
 *   report entries include ..." list.
 *
 * Neither is promoted into `@lokiai/contracts` by this node: that package is
 * outside this node's writable scope (`Docs/graph/tasks/MCP-002.md` Section
 * 2), and `packages/contracts/src/mcp.ts`'s own module doc already defers
 * "final schemas" for exactly this kind of per-tool payload to "whichever
 * node wires this into the MCP server" — this is that node, and the
 * vocabulary is scoped locally until a future contracts-owning node decides
 * to share it more broadly.
 *
 * `DeviceSummary` deliberately projects only `device_id` + `lifecycle_state`
 * — the same minimal, honest projection `DSK-002`'s own `DeviceSummaryData`
 * uses (`electron/preload/contract.cts`), for the same reason stated there:
 * no backend exists yet to observe a fuller device projection from, and
 * inventing plausible-looking additional fields (connector identity,
 * capability set, support tier, health projection — `Docs/MCP_SPEC.md`
 * Section 5's `loki://devices/{id}` resource contents) would be exactly the
 * fabrication `AGENTS.md` rule 1 forbids.
 */

// --- Device summary (Docs/SPEC.md 3.3, minimal honest projection) -------------------

export const DeviceSummarySchema = z.object({
  device_id: DeviceIdSchema,
  lifecycle_state: DeviceLifecycleStateSchema,
});
export type DeviceSummary = z.infer<typeof DeviceSummarySchema>;

// --- Aggregated health status (mirrors loki-connector-registry health.rs) -----------

export const AGGREGATED_HEALTH_STATUSES = [
  'unhealthy',
  'degraded',
  'healthy',
  'unavailable',
] as const;
export const AggregatedHealthStatusSchema = z.enum(AGGREGATED_HEALTH_STATUSES);
export type AggregatedHealthStatus = z.infer<typeof AggregatedHealthStatusSchema>;

// --- One health evidence entry (mirrors loki-connectors-sdk health.rs HealthCheckEntry) --

export const DeviceHealthEvidenceEntrySchema = z.object({
  check_id: z.string().min(1).max(128),
  check_version: z.string().min(1).max(50),
  category: z.string().min(1).max(64),
  status: HealthCheckStatusSchema,
  detail: z.string().min(1).max(2000).optional(),
  observed_at: z.iso.datetime(),
  received_at: z.iso.datetime(),
  evidence_ref: z.string().min(1).max(2000).optional(),
  suggested_next_action: z.string().min(1).max(2000).optional(),
});
export type DeviceHealthEvidenceEntry = z.infer<typeof DeviceHealthEvidenceEntrySchema>;

// --- Bounded health snapshot (Docs/graph/tasks/MCP-002.md Deliverable 2/4) ----------

/**
 * `truncated`/`total_entry_count` make Deliverable 4's size bound
 * ("a device with a huge evidence history must be truncated/paginated, not
 * dumped whole") an observable, honest fact in the response rather than a
 * silent server-side cutoff a client cannot detect.
 */
export const DeviceHealthSnapshotSchema = z.object({
  device_id: DeviceIdSchema,
  aggregated_status: AggregatedHealthStatusSchema,
  checked_at: z.iso.datetime(),
  entries: z.array(DeviceHealthEvidenceEntrySchema),
  truncated: z.boolean(),
  total_entry_count: z.number().int().nonnegative(),
});
export type DeviceHealthSnapshot = z.infer<typeof DeviceHealthSnapshotSchema>;

// --- Data source boundary -----------------------------------------------------------

/**
 * Honest success/unavailable split for every data-source call — the same
 * discipline `electron/preload/contract.cts`'s `FailableResult<T>` and
 * `electron/main/ipc.cts`'s `runtime_unavailable` stub already established
 * for the identical real-world constraint (`Docs/graph/handoffs/DSK-002-1.md`).
 * `available: false` is never collapsed into an empty-but-`available: true`
 * result — `Docs/graph/tasks/MCP-002.md` Deliverable 5: "no state silently
 * defaults to an empty-but-successful response when the underlying call
 * actually failed/is unavailable."
 */
export type DataSourceOutcome<T> =
  | {
      readonly available: true;
      readonly data: T;
      readonly source: string;
      readonly observedAt: string;
    }
  | { readonly available: false; readonly error: LokiError };

export interface DeviceListPage {
  readonly devices: readonly DeviceSummary[];
  /**
   * The total number of devices the caller is authorized to see (after
   * authorization filtering, before this page's `limit`/`offset` slice) —
   * lets the tool compute an honest `next_cursor` without a second round
   * trip. A real backend implementation authorizes *before* counting; see
   * `ListDevicesParams.isDeviceAuthorized` below.
   */
  readonly totalAuthorizedCount: number;
}

export interface ListDevicesParams {
  readonly limit: number;
  readonly offset: number;
  /**
   * The scope-filtering predicate (`src/devices/scope.ts`'s
   * `DeviceAccessScope.isDeviceAuthorized`), passed through to the data
   * source rather than applied by the caller after the fact, so pagination
   * and authorization stay consistent (a page is always `limit` *authorized*
   * devices, never `limit` raw devices some of which are then dropped).
   */
  readonly isDeviceAuthorized: (deviceId: DeviceId) => boolean;
}

export interface GetDeviceHealthParams {
  readonly maxEntries: number;
}

export interface DeviceDataSource {
  listDevices(params: ListDevicesParams): Promise<DataSourceOutcome<DeviceListPage>>;
  /**
   * `data: null` on an `available: true` outcome means the query itself
   * succeeded but no such device exists — distinct from `available: false`
   * (the query could not be made at all). Never conflate the two:
   * `Docs/graph/tasks/MCP-002.md` Deliverable 5's honesty requirement
   * applies to this per-device case exactly as it does to `listDevices`.
   */
  getDeviceHealth(
    deviceId: DeviceId,
    params: GetDeviceHealthParams,
  ): Promise<DataSourceOutcome<DeviceHealthSnapshot | null>>;
}

export type { DeviceId, DeviceLifecycleState, HealthCheckStatus };
