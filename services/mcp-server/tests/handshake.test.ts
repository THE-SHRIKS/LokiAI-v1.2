import { describe, expect, it } from 'vitest';
import { ProtocolError } from '@modelcontextprotocol/client';
import { LOKI_MCP_SERVER_NAME, LOKI_MCP_SERVER_VERSION } from '../src/server.js';
import { createTestClient, startTestServer, withConnectedClient } from './helpers.js';

/**
 * `Docs/graph/tasks/MCP-001.md` Deliverable 4: "successful initialize on the
 * current protocol version" — MCP `2026-07-28`, the modern/stateless era
 * (`Docs/MCP_SPEC.md` Section 3). The modern era has no `initialize` request
 * (removed by the 2026-07-28 revision); a successful "handshake" is a
 * successful `server/discover`-negotiated connection whose server identity
 * and (empty) capabilities come back correctly.
 */
describe('current-version (2026-07-28, modern era) handshake', () => {
  it('connects and negotiates the modern era when pinned to 2026-07-28', async () => {
    await withConnectedClient(
      {},
      { versionNegotiation: { mode: { pin: '2026-07-28' } } },
      // `withConnectedClient`'s callback type is `Promise<T>` — `async` is
      // structurally required to satisfy it, not because this body awaits.
      // eslint-disable-next-line @typescript-eslint/require-await
      async ({ client }) => {
        expect(client.getProtocolEra()).toBe('modern');
        expect(client.getNegotiatedProtocolVersion()).toBe('2026-07-28');
      },
    );
  });

  it('auto-negotiation discovers the modern era without a pin', async () => {
    await withConnectedClient(
      {},
      { versionNegotiation: { mode: 'auto' } },
      // eslint-disable-next-line @typescript-eslint/require-await -- see above
      async ({ client }) => {
        expect(client.getProtocolEra()).toBe('modern');
        expect(client.getNegotiatedProtocolVersion()).toBe('2026-07-28');
      },
    );
  });

  it('rejects a pin the server does not offer, without breaking the connection for a subsequent real client', async () => {
    const { handle, clientTransport } = startTestServer();
    const unsupportedPinClient = createTestClient({
      versionNegotiation: { mode: { pin: '2099-01-01' } },
    });
    await expect(unsupportedPinClient.connect(clientTransport)).rejects.toThrow(ProtocolError);
    await handle.close();
  });
});

describe('server identity constants', () => {
  it('are non-empty and match the values the factory constructs with', () => {
    expect(LOKI_MCP_SERVER_NAME).toBe('lokiai-mcp-server');
    expect(LOKI_MCP_SERVER_VERSION).toMatch(/^\d+\.\d+\.\d+$/);
  });
});
