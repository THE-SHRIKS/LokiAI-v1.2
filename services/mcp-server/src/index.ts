#!/usr/bin/env node
import { startMcpServer } from './bootstrap.js';
import { createStructuredLogger } from './logging.js';

/**
 * Process entry point. `Docs/TECH_STACK.md`'s MCP row: "MCP ships with its
 * own Node 24 runtime or equivalent self-contained launcher, not Electron's
 * embedded Node." A coding client (Codex, Claude Code, Cursor, ...) spawns
 * this exact file as a child process and speaks MCP over its stdin/stdout;
 * `Docs/MCP_SPEC.md` Section 2: "v1.2 default transport: local stdio,
 * started by the coding client."
 *
 * This file is intentionally the only impure, untested-by-vitest module in
 * `src/` — it owns real process lifecycle (stdio, signals, exit code) and is
 * covered instead by the process-level smoke evidence in
 * `Docs/graph/handoffs/MCP-001-1.md`. Every other module here is pure/
 * injectable and has real vitest coverage against an in-memory transport.
 */

const logger = createStructuredLogger('mcp-server');
const handle = startMcpServer();

let shuttingDown = false;

function shutdown(signal: string): void {
  if (shuttingDown) {
    return;
  }
  shuttingDown = true;
  logger.info('mcp server shutting down', { signal });
  void handle
    .close()
    .catch((error: unknown) => {
      logger.error('error during shutdown', {
        message: error instanceof Error ? error.message : String(error),
      });
    })
    .finally(() => {
      process.exit(0);
    });
}

process.on('SIGINT', () => shutdown('SIGINT'));
process.on('SIGTERM', () => shutdown('SIGTERM'));
