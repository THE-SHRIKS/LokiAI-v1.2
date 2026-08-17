import { describe, expect, it } from 'vitest';
import { LOKI_MCP_SERVER_CAPABILITIES } from '../src/capabilities.js';
import { startRawTestServer, withConnectedClient } from './helpers.js';

/**
 * `Docs/graph/tasks/MCP-001.md` acceptance criterion: "No capability
 * advertised that isn't actually implemented" — honest capability
 * negotiation, not aspirational, on both protocol eras.
 */
describe('honest capability negotiation', () => {
  it('the static capability set this node ships is empty (no tools/resources yet)', () => {
    expect(LOKI_MCP_SERVER_CAPABILITIES).toEqual({});
  });

  it('the modern server/discover advertisement carries no tools/resources/prompts capability', async () => {
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
    expect(result?.capabilities).toEqual({});
    expect(result?.capabilities).not.toHaveProperty('tools');
    expect(result?.capabilities).not.toHaveProperty('resources');
    expect(result?.capabilities).not.toHaveProperty('prompts');
    await server.handle.close();
  });

  it('the legacy initialize result carries no tools/resources/prompts capability', async () => {
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
    expect(result?.capabilities).toEqual({});
    await server.handle.close();
  });

  it('a live client sees no server capabilities after a real modern connect', async () => {
    await withConnectedClient(
      {},
      { versionNegotiation: { mode: { pin: '2026-07-28' } } },
      // `withConnectedClient`'s callback type is `Promise<T>` — `async` is
      // structurally required to satisfy it, not because this body awaits.
      // eslint-disable-next-line @typescript-eslint/require-await
      async ({ client }) => {
        expect(client.getServerCapabilities()).toEqual({});
      },
    );
  });

  it('a live client sees no server capabilities after a real legacy connect', async () => {
    await withConnectedClient(
      {},
      { versionNegotiation: { mode: 'legacy' } },
      // eslint-disable-next-line @typescript-eslint/require-await -- see above
      async ({ client }) => {
        expect(client.getServerCapabilities()).toEqual({});
      },
    );
  });

  it('calling tools/list is refused as an unsupported capability, not answered with a fabricated empty list', async () => {
    // A server that declared `capabilities.tools` would answer `tools/list`
    // with a (possibly empty) result. Because this node does not declare
    // it, the spec-correct answer is a capability error — proving the
    // empty capability set is enforced, not merely advertised.
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
    expect(response).toMatchObject({ jsonrpc: '2.0', id: 'list-1', error: {} });
    expect(response).not.toHaveProperty('result');
    await server.handle.close();
  });
});
