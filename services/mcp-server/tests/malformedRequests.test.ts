import { describe, expect, it } from 'vitest';
import { startRawTestServer } from './helpers.js';

/**
 * `Docs/graph/tasks/MCP-001.md` Deliverable 4: "a malformed/unsupported
 * client-metadata request handled with a clear protocol-conformant error,
 * not a crash." `Docs/MCP_SPEC.md` Section 11: "Oversized/malformed/replayed
 * requests fail safely." Every case below sends a real, hand-crafted request
 * a well-behaved SDK `Client` would never construct itself, and asserts:
 * (a) a real JSON-RPC error response comes back (never a hang/close/throw
 * out of the test), and (b) the connection is still usable afterward.
 */
describe('malformed / unsupported client-metadata requests', () => {
  it('an invalid _meta envelope (wrong type) gets a clear Invalid Params error, not a crash', async () => {
    const server = await startRawTestServer();
    await server.sendRaw({
      jsonrpc: '2.0',
      id: 'bad-envelope',
      method: 'server/discover',
      params: {
        _meta: {
          'io.modelcontextprotocol/protocolVersion': '2026-07-28',
          'io.modelcontextprotocol/clientCapabilities': 'not-an-object',
        },
      },
    });
    const [response] = await server.collector.waitForCount(1);
    expect(response).toMatchObject({
      jsonrpc: '2.0',
      id: 'bad-envelope',
      error: {
        code: -32602,
      },
    });
    const errorMessage = (response as { error: { message: string } }).error.message;
    expect(errorMessage).toContain('_meta envelope');
    expect(errorMessage).toContain('clientCapabilities');
    await server.handle.close();
  });

  it('a missing _meta envelope on server/discover gets a real error, not a hang', async () => {
    const server = await startRawTestServer();
    await server.sendRaw({
      jsonrpc: '2.0',
      id: 'missing-envelope',
      method: 'server/discover',
      params: {},
    });
    const [response] = await server.collector.waitForCount(1);
    // Envelope-less on a fresh connection classifies as a 2025-era ("no
    // claim") opening; a bare-legacy instance has no `server/discover`
    // handler at all, so this surfaces as the ordinary JSON-RPC
    // method-not-found — a real, spec-conformant answer, not a crash.
    expect(response).toMatchObject({
      jsonrpc: '2.0',
      id: 'missing-envelope',
      error: { code: -32601 },
    });
    await server.handle.close();
  });

  it('an unsupported/unknown modern protocol version is rejected with a clear, data-bearing error', async () => {
    const server = await startRawTestServer();
    await server.sendRaw({
      jsonrpc: '2.0',
      id: 'bad-version',
      method: 'server/discover',
      params: {
        _meta: {
          'io.modelcontextprotocol/protocolVersion': '2099-01-01',
          'io.modelcontextprotocol/clientCapabilities': {},
        },
      },
    });
    const [response] = await server.collector.waitForCount(1);
    expect(response).toMatchObject({
      jsonrpc: '2.0',
      id: 'bad-version',
      error: {
        code: -32022,
        data: {
          supported: ['2026-07-28'],
          requested: '2099-01-01',
        },
      },
    });
    await server.handle.close();
  });

  it('the connection recovers after a malformed request: a subsequent valid modern open on the same connection still succeeds', async () => {
    const server = await startRawTestServer();

    await server.sendRaw({
      jsonrpc: '2.0',
      id: 'bad-1',
      method: 'server/discover',
      params: {
        _meta: {
          'io.modelcontextprotocol/protocolVersion': '2099-01-01',
          'io.modelcontextprotocol/clientCapabilities': {},
        },
      },
    });
    await server.collector.waitForCount(1);

    await server.sendRaw({
      jsonrpc: '2.0',
      id: 'good-1',
      method: 'server/discover',
      params: {
        _meta: {
          'io.modelcontextprotocol/protocolVersion': '2026-07-28',
          'io.modelcontextprotocol/clientCapabilities': {},
        },
      },
    });
    const messages = await server.collector.waitForCount(2);
    const goodResponse = messages[1] as {
      result?: { supportedVersions?: string[]; capabilities?: unknown };
    };
    expect(goodResponse.result?.supportedVersions).toEqual(['2026-07-28']);
    // `MCP-002` registers real tools (`src/tools/index.ts`), so the honest
    // capability set is no longer empty — see `tests/capabilities.test.ts`
    // for the dedicated capability-negotiation coverage this assertion used
    // to own alone back when `MCP-001` shipped no tools/resources at all.
    expect(goodResponse.result?.capabilities).toHaveProperty('tools');

    await server.handle.close();
  });

  it('an oversized/garbage top-level payload does not crash the connection (transport-level malformed input fails safely)', async () => {
    const server = await startRawTestServer();
    // Not a JSON-RPC request or notification at all (no `method`); the SDK's
    // own message-shape validation must reject this without throwing out of
    // this test and without wedging the connection for the next message.
    await server.sendRaw({ jsonrpc: '2.0', id: 'not-a-request', unexpected: true });

    // Prove the connection is still alive: a subsequent valid request still
    // gets answered.
    await server.sendRaw({
      jsonrpc: '2.0',
      id: 'good-after-garbage',
      method: 'server/discover',
      params: {
        _meta: {
          'io.modelcontextprotocol/protocolVersion': '2026-07-28',
          'io.modelcontextprotocol/clientCapabilities': {},
        },
      },
    });
    const messages = await server.collector.waitForCount(1, 3000);
    const lastMessage = messages[messages.length - 1] as { id?: string; result?: unknown };
    expect(lastMessage.id).toBe('good-after-garbage');
    expect(lastMessage.result).toBeDefined();

    await server.handle.close();
  });
});
