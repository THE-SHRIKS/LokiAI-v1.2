import { z } from 'zod';
import type { CallToolResult, McpServer } from '@modelcontextprotocol/server';
import { CursorSchema, mcpReadResultSchema, type McpReadResult } from '@lokiai/contracts';
import { newCorrelationId } from '../correlationId.js';
import { createStructuredLogger, type StructuredLogger } from '../logging.js';
import { buildInternalError, buildInvalidCursorError } from '../devices/errors.js';
import { clampLimit, decodeCursor, encodeCursor, MAX_PAGE_SIZE } from '../devices/pagination.js';
import { computeFreshness } from '../devices/freshness.js';
import {
  DeviceSummarySchema,
  type DeviceDataSource,
  type DeviceSummary,
} from '../devices/types.js';
import type { DeviceAccessScope } from '../devices/scope.js';
import { toolCallResult } from './result.js';

/**
 * `loki_list_devices` — `Docs/graph/tasks/MCP-002.md` Deliverable 1,
 * `Docs/MCP_SPEC.md` Section 6's "Inspection — low risk" tool of the same
 * name, `Docs/SPEC.md` AS-005 ("connected MCP client lists authorized
 * devices ..."). Tool name matches
 * `packages/contracts/src/mcp.ts`'s `MCP_TOOL_CATEGORY['loki_list_devices']`
 * (already `'inspection'`) exactly — not a new/invented name.
 */
export const LOKI_LIST_DEVICES_TOOL_NAME = 'loki_list_devices';

const ListDevicesInputShape = {
  limit: z
    .number()
    .int()
    .min(1)
    .max(MAX_PAGE_SIZE)
    .describe(`Maximum devices to return in this page (1-${MAX_PAGE_SIZE}).`)
    .optional(),
  cursor: CursorSchema.describe(
    'Opaque page cursor from a previous response next_cursor.',
  ).optional(),
};

const ListDevicesOutputSchema = mcpReadResultSchema(z.array(DeviceSummarySchema));

export interface ListDevicesToolDeps {
  readonly dataSource: DeviceDataSource;
  readonly scope: DeviceAccessScope;
  readonly logger?: StructuredLogger;
}

/**
 * Pure, directly unit-testable handler (`tests/tools/listDevices.test.ts`) —
 * exported separately from `registerListDevicesTool` so pagination/scope/
 * size-bound/honest-unavailable behavior can be exercised with fake
 * `DeviceDataSource`/scope implementations without going through the full
 * MCP protocol stack, matching this package's existing injectable-module
 * discipline (`src/bootstrap.ts`, `src/logging.ts`).
 */
export async function listDevicesHandler(
  args: { readonly limit?: number; readonly cursor?: string },
  deps: ListDevicesToolDeps,
): Promise<CallToolResult> {
  const logger = deps.logger ?? createStructuredLogger('mcp-server');
  const limit = clampLimit(args.limit);
  const decoded = decodeCursor(args.cursor);
  if (decoded === null) {
    return toolCallResult(buildInvalidCursorError(LOKI_LIST_DEVICES_TOOL_NAME), true);
  }

  const outcome = await deps.dataSource.listDevices({
    limit,
    offset: decoded.offset,
    isDeviceAuthorized: (deviceId) => deps.scope.isDeviceAuthorized(deviceId),
  });

  if (!outcome.available) {
    return toolCallResult(outcome.error, true);
  }

  const nextOffset = decoded.offset + outcome.data.devices.length;
  const hasMore = nextOffset < outcome.data.totalAuthorizedCount;

  const result: McpReadResult<DeviceSummary[]> = {
    data: [...outcome.data.devices],
    source: outcome.source,
    observed_at: outcome.observedAt,
    freshness: computeFreshness(outcome.observedAt),
    correlation_id: newCorrelationId(),
    next_cursor: hasMore ? encodeCursor(nextOffset) : undefined,
  };

  const validated = ListDevicesOutputSchema.safeParse(result);
  if (!validated.success) {
    logger.error('list-devices-internal-result-invalid', {
      correlation_id: result.correlation_id,
      issues: validated.error.issues.length,
    });
    return toolCallResult(
      buildInternalError(
        LOKI_LIST_DEVICES_TOOL_NAME,
        'internal device list result failed its own schema',
      ),
      true,
    );
  }

  return toolCallResult(validated.data, false);
}

export function registerListDevicesTool(server: McpServer, deps: ListDevicesToolDeps): void {
  server.registerTool(
    LOKI_LIST_DEVICES_TOOL_NAME,
    {
      title: 'List authorized devices',
      description:
        'Lists devices the connected agent is authorized to see, in bounded pages. ' +
        'Never returns an unbounded or unscoped device list.',
      inputSchema: ListDevicesInputShape,
    },
    (args) => listDevicesHandler(args, deps),
  );
}
