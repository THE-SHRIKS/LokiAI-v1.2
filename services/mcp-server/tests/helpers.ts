import {
  Client,
  InMemoryTransport,
  type ClientOptions,
  type JSONRPCMessage,
  type Transport,
} from '@modelcontextprotocol/client';
import { serveStdio } from '@modelcontextprotocol/server/stdio';
import type { ServeStdioOptions, StdioServerHandle } from '@modelcontextprotocol/server/stdio';
import { lokiMcpServerFactory } from '../src/server.js';

/**
 * Real, non-mocked test infrastructure. Every helper here drives the actual
 * `serveStdio` + `lokiMcpServerFactory` production code path
 * (`src/bootstrap.ts` / `src/server.ts`) over an in-memory transport pair
 * instead of a spawned child process — the exact era-negotiation, legacy
 * adapter, and error-handling logic under test is the SDK's real
 * `serveStdio`, not a stand-in.
 */

export type TestServeOptions = Partial<Pick<ServeStdioOptions, 'legacy' | 'onerror'>>;

export interface TestServer {
  handle: StdioServerHandle;
  clientTransport: Transport;
}

/** Starts the real server over one half of an in-memory transport pair. */
export function startTestServer(options: TestServeOptions = {}): TestServer {
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const handle = serveStdio(lokiMcpServerFactory, { transport: serverTransport, ...options });
  return { handle, clientTransport };
}

export function createTestClient(options?: ClientOptions): Client {
  return new Client({ name: 'mcp-001-test-harness', version: '1.0.0' }, options);
}

/** Starts a real server, connects a real `Client`, runs `fn`, then tears both down (client first, per the SDK's own testing guide). */
export async function withConnectedClient<T>(
  serverOptions: TestServeOptions,
  clientOptions: ClientOptions | undefined,
  fn: (ctx: { client: Client; server: TestServer }) => Promise<T>,
): Promise<T> {
  const server = startTestServer(serverOptions);
  const client = createTestClient(clientOptions);
  await client.connect(server.clientTransport);
  try {
    return await fn({ client, server });
  } finally {
    await client.close().catch(() => {});
    await server.handle.close().catch(() => {});
  }
}

/**
 * Collects every message the (already-linked) transport half receives, with
 * a deterministic "wait for the Nth message" instead of a fixed sleep.
 */
export interface MessageCollector {
  readonly received: JSONRPCMessage[];
  waitForCount(count: number, timeoutMs?: number): Promise<JSONRPCMessage[]>;
}

function createMessageCollector(transport: Transport): MessageCollector {
  const received: JSONRPCMessage[] = [];
  const waiters: Array<{ count: number; resolve: () => void }> = [];

  transport.onmessage = (message: JSONRPCMessage) => {
    received.push(message);
    for (const waiter of [...waiters]) {
      if (received.length >= waiter.count) {
        waiters.splice(waiters.indexOf(waiter), 1);
        waiter.resolve();
      }
    }
  };

  return {
    received,
    async waitForCount(count, timeoutMs = 2000) {
      if (received.length >= count) {
        return received;
      }
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => {
          reject(new Error(`Timed out waiting for ${count} message(s); got ${received.length}`));
        }, timeoutMs);
        waiters.push({
          count,
          resolve: () => {
            clearTimeout(timer);
            resolve();
          },
        });
      });
      return received;
    },
  };
}

export interface RawTestServer {
  handle: StdioServerHandle;
  clientTransport: Transport;
  collector: MessageCollector;
  /** Sends a hand-crafted (possibly deliberately malformed) message directly over the wire, bypassing `Client`'s well-behaved request building. */
  sendRaw(message: unknown): Promise<void>;
}

/**
 * Same real server as `startTestServer`, but for tests that need to send a
 * deliberately malformed or unusual raw JSON-RPC message — something a
 * well-behaved `Client` would never construct itself.
 */
export async function startRawTestServer(options: TestServeOptions = {}): Promise<RawTestServer> {
  const { handle, clientTransport } = startTestServer(options);
  const collector = createMessageCollector(clientTransport);
  await clientTransport.start();
  return {
    handle,
    clientTransport,
    collector,
    sendRaw: (message: unknown) => clientTransport.send(message as JSONRPCMessage),
  };
}
