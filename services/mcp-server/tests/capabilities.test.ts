import { describe, expect, it } from 'vitest';
import { LOKI_MCP_SERVER_CAPABILITIES } from '../src/capabilities.js';
import { LOKI_LIST_DEVICES_TOOL_NAME } from '../src/tools/listDevices.js';
import { startRawTestServer, withConnectedClient } from './helpers.js';

/**
 * `Docs/graph/tasks/MCP-001.md` acceptance criterion: "No capability
 * advertised that isn't actually implemented" — honest capability
 * negotiation, not aspirational, on both protocol eras.
 *
 * `MCP-002` is the node `src/capabilities.ts`'s own module doc predicted
 * would extend this constant "in the same change" it registers real
 * handlers — these assertions are updated from MCP-001's original
 * empty-capabilities expectations to match: `tools` is now real (two tools
 * are registered, `src/tools/index.ts`), `resources`/`prompts` remain
 * genuinely absent (this node ships tools only, per its own task packet
 * Section 4's naming flexibility).
 */
describe('honest capability negotiation', () => {
  it('the static capability set this node ships declares tools (real tools are registered) and nothing else', () => {
    expect(LOKI_MCP_SERVER_CAPABILITIES).toEqual({ tools: {} });
  });

  it('the modern server/discover advertisement carries a real tools capability and no resources/prompts capability', async () => {
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
    expect(result?.capabilities).not.toHaveProperty('resources');
    expect(result?.capabilities).not.toHaveProperty('prompts');
    await server.handle.close();
  });

  it('the legacy initialize result carries a real tools capability and no resources/prompts capability', async () => {
    const server = await startRawTestServer();
    await server.sendRaw({
      jsonrpc: '2.0',
      id: 'init-1',
      method: 'initialize',
      params: {
        protocolVersion: '2025-06-18',
        capabilities: {},
        clientInfo: { name: 'raw-client', version: '1.0.0' },
      },
    });
    const [response] = await server.collector.waitForCount(1);
    const result = (response as { result?: { capabilities?: Record<string, unknown> } }).result;
    expect(result?.capabilities).toHaveProperty('tools');
    expect(result?.capabilities).not.toHaveProperty('resources');
    expect(result?.capabilities).not.toHaveProperty('prompts');
    await server.handle.close();
  });

  it('a live client sees a real tools capability after a real modern connect', async () => {
    await withConnectedClient(
      {},
      { versionNegotiation: { mode: { pin: '2026-07-28' } } },
      // eslint-disable-next-line @typescript-eslint/require-await
      async ({ client }) => {
        expect(client.getServerCapabilities()).toHaveProperty('tools');
      },
    );
  });

  it('a live client sees a real tools capability after a real legacy connect', async () => {
    await withConnectedClient(
      {},
      { versionNegotiation: { mode: 'legacy' } },
      // eslint-disable-next-line @typescript-eslint/require-await
      async ({ client }) => {
        expect(client.getServerCapabilities()).toHaveProperty('tools');
      },
    );
  });

  it('tools/list is answered for real (not refused) and includes the registered device tools', async () => {
    const server = await startRawTestServer();
    await server.sendRaw({
      jsonrpc: '2.0',
      id: 'list-1',
      method: 'tools/list',
      params: {
        _meta: {
          'io.modelcontextprotocol/protocolVersion': '2026-07-28',
          'io.modelcontextprotocol/clientCapabilities': {},
        },
      },
    });
    const [response] = await server.collector.waitForCount(1);
    const result = (response as { result?: { tools?: Array<{ name: string }> } }).result;
    expect(result?.tools?.map((t) => t.name)).toContain(LOKI_LIST_DEVICES_TOOL_NAME);
    expect(response).not.toHaveProperty('error');
    await server.handle.close();
  });
});
