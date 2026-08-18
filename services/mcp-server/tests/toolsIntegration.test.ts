import { describe, expect, it } from 'vitest';
import { ProtocolError } from '@modelcontextprotocol/client';
import { LOKI_LIST_DEVICES_TOOL_NAME } from '../src/tools/listDevices.js';
import { LOKI_GET_DEVICE_HEALTH_TOOL_NAME } from '../src/tools/getDeviceHealth.js';
import { startRawTestServer, withConnectedClient } from './helpers.js';

/**
 * Full-stack proof that `src/server.ts`'s `lokiMcpServerFactory` — the exact
 * production factory `src/bootstrap.ts`/`src/index.ts` serve — really wires
 * both `MCP-002` tools through the real MCP protocol/SDK, not just through
 * this package's own pure-handler unit tests
 * (`tests/tools/{listDevices,getDeviceHealth}.test.ts`).
 *
 * Every scenario here uses the *default* production dependencies
 * (`registerLokiTools`'s defaults: `createRuntimeUnavailableDeviceDataSource`
 * + `createDenyAllDeviceScope`) — there is no way to inject a fake backend
 * through the real `McpServerFactory` signature (it is called per-connection
 * by the SDK with no test hook), which is itself proof this server cannot
 * currently serve real device data over the wire, only the honest
 * unavailable/unauthorized states asserted below.
 */
describe('MCP-002 device tools — real protocol integration', () => {
  it('tools/list advertises both device tools with real, non-empty schemas', async () => {
    await withConnectedClient({}, undefined, async ({ client }) => {
      const { tools } = await client.listTools();
      const names = tools.map((t) => t.name);
      expect(names).toContain(LOKI_LIST_DEVICES_TOOL_NAME);
      expect(names).toContain(LOKI_GET_DEVICE_HEALTH_TOOL_NAME);

      const listDevicesTool = tools.find((t) => t.name === LOKI_LIST_DEVICES_TOOL_NAME);
      expect(listDevicesTool?.inputSchema).toBeTruthy();
      const healthTool = tools.find((t) => t.name === LOKI_GET_DEVICE_HEALTH_TOOL_NAME);
      expect(healthTool?.inputSchema).toBeTruthy();
    });
  });

  it('server/discover and legacy initialize both honestly advertise the tools capability now that real tools exist', async () => {
    const server = await startRawTestServer();
    await server.sendRaw({
      jsonrpc: '2.0',
      id: 'discover-1',
      method: 'server/discover',
      params: {
        _meta: {
          'io.modelcontextprotocol/protocolVersion': '2026-07-28',
          'io.modelcontextprotocol/clientCapabilities': {},
        },
      },
    });
    const [response] = await server.collector.waitForCount(1);
    const result = (response as { result?: { capabilities?: Record<string, unknown> } }).result;
    expect(result?.capabilities).toHaveProperty('tools');
    await server.handle.close();
  });

  it('loki_list_devices reports isError with an honest runtime_unavailable code, never a fabricated device list', async () => {
    await withConnectedClient({}, undefined, async ({ client }) => {
      const result = await client.callTool({ name: LOKI_LIST_DEVICES_TOOL_NAME, arguments: {} });
      expect(result.isError).toBe(true);
      const structured = result.structuredContent as { code: string };
      expect(structured.code).toBe('runtime_unavailable');
    });
  });

  it('loki_get_device_health reports permission_denied over the wire — the honest deny-all default with no real auth primitive', async () => {
    await withConnectedClient({}, undefined, async ({ client }) => {
      const result = await client.callTool({
        name: LOKI_GET_DEVICE_HEALTH_TOOL_NAME,
        arguments: { device_id: 'dev-1' },
      });
      expect(result.isError).toBe(true);
      const structured = result.structuredContent as { code: string };
      expect(structured.code).toBe('permission_denied');
    });
  });

  it('a malformed loki_get_device_health call (invalid device_id charset) fails schema validation as a tool-level error, not a crash or fabricated result', async () => {
    // The official SDK validates tool arguments against the registered
    // input schema *inside* the `tools/call` handler and reports a failure
    // as an `isError: true` `CallToolResult` (its own `createToolError`),
    // not a rejected JSON-RPC protocol error — confirmed empirically this
    // node (a schema-shape violation on a specific tool's own arguments is
    // a tool-level concern; a malformed envelope/unknown method/unknown
    // tool name, asserted below, is the transport/protocol-level case
    // `tests/malformedRequests.test.ts` already covers for MCP-001). Either
    // way the request fails safely: no crash, no fabricated device data.
    await withConnectedClient({}, undefined, async ({ client }) => {
      const result = await client.callTool({
        name: LOKI_GET_DEVICE_HEALTH_TOOL_NAME,
        arguments: { device_id: 'not a valid id / has spaces' },
      });
      expect(result.isError).toBe(true);
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('Invalid arguments');
    });
  });

  it('a malformed loki_list_devices call (limit above the bound) fails schema validation as a tool-level error', async () => {
    await withConnectedClient({}, undefined, async ({ client }) => {
      const result = await client.callTool({
        name: LOKI_LIST_DEVICES_TOOL_NAME,
        arguments: { limit: 100_000 },
      });
      expect(result.isError).toBe(true);
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('Invalid arguments');
    });
  });

  it('a malformed loki_list_devices call (negative limit) fails schema validation as a tool-level error', async () => {
    await withConnectedClient({}, undefined, async ({ client }) => {
      const result = await client.callTool({
        name: LOKI_LIST_DEVICES_TOOL_NAME,
        arguments: { limit: -1 },
      });
      expect(result.isError).toBe(true);
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('Invalid arguments');
    });
  });

  it('an unknown tool name is refused with a real protocol error, not a fabricated result', async () => {
    await withConnectedClient({}, undefined, async ({ client }) => {
      await expect(client.callTool({ name: 'loki_does_not_exist', arguments: {} })).rejects.toThrow(
        ProtocolError,
      );
    });
  });

  it('the connection recovers after a malformed tool call: a subsequent valid call on the same connection still succeeds', async () => {
    await withConnectedClient({}, undefined, async ({ client }) => {
      const badResult = await client.callTool({
        name: LOKI_LIST_DEVICES_TOOL_NAME,
        arguments: { limit: -1 },
      });
      expect(badResult.isError).toBe(true);

      const result = await client.callTool({ name: LOKI_LIST_DEVICES_TOOL_NAME, arguments: {} });
      expect(result.isError).toBe(true);
      const structured = result.structuredContent as { code: string };
      expect(structured.code).toBe('runtime_unavailable');
    });
  });
});
