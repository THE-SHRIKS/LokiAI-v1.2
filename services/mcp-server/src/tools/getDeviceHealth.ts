import { z } from 'zod';
import type { CallToolResult, McpServer } from '@modelcontextprotocol/server';
import {
  DeviceIdSchema,
  mcpReadResultSchema,
  type DeviceId,
  type McpReadResult,
} from '@lokiai/contracts';
import { newCorrelationId } from '../correlationId.js';
import { createStructuredLogger, type StructuredLogger } from '../logging.js';
import {
  buildDeviceUnauthorizedError,
  buildDeviceUnavailableError,
  buildInternalError,
} from '../devices/errors.js';
import { clampMaxEntries, MAX_HEALTH_ENTRIES } from '../devices/pagination.js';
import { computeFreshness } from '../devices/freshness.js';
import {
  DeviceHealthSnapshotSchema,
  type DeviceDataSource,
  type DeviceHealthSnapshot,
} from '../devices/types.js';
import type { DeviceAccessScope } from '../devices/scope.js';
import { toolCallResult } from './result.js';

/**
 * `loki_get_device_health` — `Docs/graph/tasks/MCP-002.md` Deliverable 2,
 * `Docs/MCP_SPEC.md` Section 6, `Docs/SPEC.md` AS-005 ("... reads bounded
 * health evidence with correlation and freshness"). Tool name matches
 * `packages/contracts/src/mcp.ts`'s
 * `MCP_TOOL_CATEGORY['loki_get_device_health']` (already `'inspection'`)
 * exactly.
 */
export const LOKI_GET_DEVICE_HEALTH_TOOL_NAME = 'loki_get_device_health';

const GetDeviceHealthInputShape = {
  device_id: DeviceIdSchema.describe('The device to read bounded health evidence for.'),
  max_entries: z
    .number()
    .int()
    .min(1)
    .max(MAX_HEALTH_ENTRIES)
    .describe(`Maximum health evidence entries to return (1-${MAX_HEALTH_ENTRIES}).`)
    .optional(),
};

const GetDeviceHealthOutputSchema = mcpReadResultSchema(DeviceHealthSnapshotSchema);

export interface GetDeviceHealthToolDeps {
  readonly dataSource: DeviceDataSource;
  readonly scope: DeviceAccessScope;
  readonly logger?: StructuredLogger;
}

/**
 * Pure, directly unit-testable handler — see `listDevicesHandler`'s doc
 * comment (`src/tools/listDevices.ts`) for why this is exported separately
 * from `registerGetDeviceHealthTool`.
 *
 * Authorization is checked *before* any data-source call, per
 * `Docs/MCP_SPEC.md` Section 11: "Unauthorized workspace/device access is
 * rejected without data leakage" — an unauthorized caller cannot distinguish
 * "device exists but you can't see it" from "device does not exist" by
 * timing or response shape, because both paths return the exact same
 * `permission_denied` error without ever reaching the data source.
 */
export async function getDeviceHealthHandler(
  args: { readonly device_id: DeviceId; readonly max_entries?: number },
  deps: GetDeviceHealthToolDeps,
): Promise<CallToolResult> {
  const logger = deps.logger ?? createStructuredLogger('mcp-server');

  if (!deps.scope.isDeviceAuthorized(args.device_id)) {
    return toolCallResult(
      buildDeviceUnauthorizedError(LOKI_GET_DEVICE_HEALTH_TOOL_NAME, args.device_id),
      true,
    );
  }

  const maxEntries = clampMaxEntries(args.max_entries);
  const outcome = await deps.dataSource.getDeviceHealth(args.device_id, { maxEntries });

  if (!outcome.available) {
    return toolCallResult(outcome.error, true);
  }

  if (outcome.data === null) {
    return toolCallResult(
      buildDeviceUnavailableError(LOKI_GET_DEVICE_HEALTH_TOOL_NAME, args.device_id),
      true,
    );
  }

  // Deliverable 4's size bound, enforced here regardless of how many entries
  // the data source itself returned — never trust an upstream source to have
  // already honored the requested bound.
  const boundedEntries = outcome.data.entries.slice(0, maxEntries);
  const snapshot: DeviceHealthSnapshot = {
    ...outcome.data,
    entries: boundedEntries,
    truncated: outcome.data.truncated || boundedEntries.length < outcome.data.entries.length,
  };

  const result: McpReadResult<DeviceHealthSnapshot> = {
    data: snapshot,
    source: outcome.source,
    observed_at: outcome.observedAt,
    freshness: computeFreshness(outcome.observedAt),
    correlation_id: newCorrelationId(),
  };

  const validated = GetDeviceHealthOutputSchema.safeParse(result);
  if (!validated.success) {
    logger.error('get-device-health-internal-result-invalid', {
      correlation_id: result.correlation_id,
      issues: validated.error.issues.length,
    });
    return toolCallResult(
      buildInternalError(
        LOKI_GET_DEVICE_HEALTH_TOOL_NAME,
        'internal device health result failed its own schema',
      ),
      true,
    );
  }

  return toolCallResult(validated.data, false);
}

export function registerGetDeviceHealthTool(
  server: McpServer,
  deps: GetDeviceHealthToolDeps,
): void {
  server.registerTool(
    LOKI_GET_DEVICE_HEALTH_TOOL_NAME,
    {
      title: 'Read bounded device health evidence',
      description:
        'Returns bounded health evidence for one authorized device, including correlation ID and ' +
        'freshness/staleness. Never returns an unbounded evidence dump or fabricated evidence.',
      inputSchema: GetDeviceHealthInputShape,
    },
    (args) => getDeviceHealthHandler(args, deps),
  );
}
