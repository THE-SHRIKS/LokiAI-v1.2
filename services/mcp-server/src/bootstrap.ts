import { serveStdio } from '@modelcontextprotocol/server/stdio';
import type { ServeStdioOptions, StdioServerHandle } from '@modelcontextprotocol/server/stdio';
import type { Transport } from '@modelcontextprotocol/server';
import { lokiMcpServerFactory } from './server.js';
import { createStructuredLogger, type StructuredLogger } from './logging.js';
import { newCorrelationId } from './correlationId.js';
import { toDiagnosticLokiError } from './errorLog.js';

export interface StartMcpServerOptions {
  /**
   * Overrides the wire transport. Production uses the default (real process
   * stdio); tests inject an in-memory pair so the exact `serveStdio`
   * era-negotiation/legacy-adapter/error-handling code path this module
   * ships is exercised without a child process.
   */
  transport?: Transport;
  /**
   * `Docs/MCP_SPEC.md` Section 3: "one explicitly bounded previous-version
   * edge adapter." `'serve'` (the SDK's default, used here) answers a
   * 2025-era opening (`2024-10-07` through `2025-11-25` — the SDK's own
   * closed "legacy era" family, not a single hand-picked date) from the same
   * factory. `'reject'` is the flip point for the adapter's eventual
   * removal (`Docs/MCP_SPEC.md` Section 9: "has ... a removal date") —
   * `Docs/graph/handoffs/MCP-001-1.md` records why the exact calendar/
   * release trigger for flipping it is a product/release decision this
   * foundation node does not make.
   */
  legacy?: ServeStdioOptions['legacy'];
  logger?: StructuredLogger;
}

/**
 * Starts the LokiAI MCP server over stdio (or an injected transport) and
 * wires structured stderr diagnostics around the SDK's own connection-level
 * error reporting. Returns the SDK's `StdioServerHandle` unchanged — callers
 * own shutdown (`handle.close()`).
 */
export function startMcpServer(options: StartMcpServerOptions = {}): StdioServerHandle {
  const logger = options.logger ?? createStructuredLogger('mcp-server');

  const handle = serveStdio(lokiMcpServerFactory, {
    transport: options.transport,
    legacy: options.legacy,
    onerror: (error) => {
      const correlationId = newCorrelationId();
      const diagnostic = toDiagnosticLokiError(error, correlationId);
      logger.error('mcp connection error', { correlation_id: correlationId, error: diagnostic });
    },
  });

  logger.info('mcp server started', {
    server_name: 'lokiai-mcp-server',
    transport: options.transport ? 'injected' : 'stdio',
    legacy_mode: options.legacy ?? 'serve',
  });

  return handle;
}
