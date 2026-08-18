import type { CallToolResult } from '@modelcontextprotocol/server';

/**
 * Shared `CallToolResult` builder for every device tool
 * (`listDevices.ts`/`getDeviceHealth.ts`). `content` always carries a JSON
 * text rendering of `payload` (so a client that only reads `content`, not
 * `structuredContent`, still gets the full result); `structuredContent`
 * carries the same value machine-readable, per the SDK's own
 * `structuredContent`/`content` dual-representation convention
 * (`@modelcontextprotocol/server`'s `McpServer.registerTool` doc examples).
 *
 * `payload` is `Record<string, unknown>` (not `unknown`) so this stays a
 * real JSON object at the type level — `@lokiai/contracts`' `LokiError`,
 * `McpReadResult<T>` inferred types, and every builder in
 * `src/devices/errors.ts` all satisfy that shape.
 */
export function toolCallResult(payload: Record<string, unknown>, isError: boolean): CallToolResult {
  return {
    content: [{ type: 'text', text: JSON.stringify(payload) }],
    structuredContent: payload,
    isError,
  };
}
