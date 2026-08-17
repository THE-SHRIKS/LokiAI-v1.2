import { describe, expect, it } from 'vitest';
import { ProtocolError } from '@modelcontextprotocol/client';
import {
  createTestClient,
  startRawTestServer,
  startTestServer,
  withConnectedClient,
} from './helpers.js';

/**
 * `Docs/graph/tasks/MCP-001.md` Deliverable 4: "the previous-version edge
 * adapter path" — `Docs/MCP_SPEC.md` Section 3's "one explicitly bounded
 * previous-version edge adapter."
 *
 * `Docs/graph/handoffs/MCP-001-1.md` records the exact evidence this is
 * built on: the official SDK models exactly two protocol "eras" — `modern`
 * (`2026-07-28` only) and `legacy` (`2024-10-07` through `2025-11-25`, the
 * whole pre-2026 family sharing one `initialize`-handshake wire behavior).
 * `serveStdio`'s default `legacy: 'serve'` posture IS that one bounded
 * adapter: it answers any 2025-era opening from the same factory, without
 * this server hand-picking or hardcoding a single supported prior date.
 * These tests exercise several concrete dates from that family to prove the
 * bound is real (the whole legacy family, verified — not just whichever one
 * date happens to be convenient), plus the `legacy: 'reject'` flip point
 * `Docs/MCP_SPEC.md` Section 9 requires for the adapter's eventual removal.
 */
describe('previous-version (2025-era legacy) edge adapter', () => {
  it("a real Client's default (legacy) connect succeeds", async () => {
    // `withConnectedClient`'s callback type is `Promise<T>` — `async` is
    // structurally required to satisfy it, not because this body awaits.
    // eslint-disable-next-line @typescript-eslint/require-await
    await withConnectedClient({}, undefined, async ({ client }) => {
      expect(client.getProtocolEra()).toBe('legacy');
      // The SDK client's own default legacy `initialize` request asks for
      // its LATEST_PROTOCOL_VERSION (2025-11-25); asserting the exact value
      // here pins down that this is a real negotiated result, not a stub.
      expect(client.getNegotiatedProtocolVersion()).toBe('2025-11-25');
    });
  });

  it('a real Client explicitly pinned to legacy mode connects the same way', async () => {
    await withConnectedClient(
      {},
      { versionNegotiation: { mode: 'legacy' } },
      // eslint-disable-next-line @typescript-eslint/require-await -- see above
      async ({ client }) => {
        expect(client.getProtocolEra()).toBe('legacy');
      },
    );
  });

  it.each(['2025-06-18', '2025-03-26', '2024-11-05', '2024-10-07'])(
    'echoes back the exact requested legacy revision %s (not just the newest legacy default)',
    async (requestedVersion) => {
      const server = await startRawTestServer();
      await server.sendRaw({
        jsonrpc: '2.0',
        id: 'init-1',
        method: 'initialize',
        params: {
          protocolVersion: requestedVersion,
          capabilities: {},
          clientInfo: { name: 'raw-legacy-client', version: '1.0.0' },
        },
      });
      const [response] = await server.collector.waitForCount(1);
      expect(response).toMatchObject({
        jsonrpc: '2.0',
        id: 'init-1',
        result: {
          protocolVersion: requestedVersion,
          serverInfo: { name: 'lokiai-mcp-server' },
        },
      });
      await server.handle.close();
    },
  );

  it('falls back to the server default legacy version for an unrecognized legacy-shaped version, rather than crashing', async () => {
    const server = await startRawTestServer();
    await server.sendRaw({
      jsonrpc: '2.0',
      id: 'init-1',
      method: 'initialize',
      params: {
        protocolVersion: '1999-01-01',
        capabilities: {},
        clientInfo: { name: 'raw-legacy-client', version: '1.0.0' },
      },
    });
    const [response] = await server.collector.waitForCount(1);
    expect(response).toMatchObject({
      jsonrpc: '2.0',
      id: 'init-1',
      result: { protocolVersion: '2025-11-25' },
    });
    await server.handle.close();
  });

  it('legacy: "reject" refuses a 2025-era opening — the adapter is a real, off-switchable bound, not permanent', async () => {
    const { handle, clientTransport } = startTestServer({ legacy: 'reject' });
    const legacyClient = createTestClient({ versionNegotiation: { mode: 'legacy' } });
    await expect(legacyClient.connect(clientTransport)).rejects.toThrow(ProtocolError);
    await handle.close();
  });

  it('legacy: "reject" still serves a modern connection on the same server', async () => {
    const { handle, clientTransport } = startTestServer({ legacy: 'reject' });
    const modernClient = createTestClient({ versionNegotiation: { mode: { pin: '2026-07-28' } } });
    await modernClient.connect(clientTransport);
    expect(modernClient.getProtocolEra()).toBe('modern');
    await modernClient.close();
    await handle.close();
  });
});
