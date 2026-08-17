import { McpServer, type McpServerFactory } from '@modelcontextprotocol/server';
import { LOKI_MCP_SERVER_CAPABILITIES } from './capabilities.js';

/**
 * The MCP `Implementation` identity this server reports in every handshake
 * (legacy `initialize` result, modern `server/discover` advertisement, and
 * the `_meta` `io.modelcontextprotocol/serverInfo` stamped on every modern
 * response). Kept in sync with `package.json`'s own `"version"` field by
 * convention, matching `@lokiai/contracts`' `CONTRACTS_PACKAGE_VERSION`
 * pattern (`packages/contracts/src/index.ts`) — reading `package.json` at
 * runtime is not reliable across every future packaging path.
 */
export const LOKI_MCP_SERVER_NAME = 'lokiai-mcp-server';
export const LOKI_MCP_SERVER_VERSION = '0.1.0';

/**
 * Builds one fresh `McpServer` instance. Passed as the `factory` to
 * `@modelcontextprotocol/server/stdio`'s `serveStdio` in `bootstrap.ts` —
 * the SDK's serving entry point calls this once per stdio connection and
 * owns protocol-version/era negotiation, the legacy `initialize` handshake,
 * and the modern `server/discover` advertisement itself (see
 * `Docs/graph/handoffs/MCP-001-1.md` for why this node does not hand-roll
 * any of that, and for the verified behavior this relies on).
 *
 * This factory's only job is to declare identity and the currently-empty,
 * honest capability set (`capabilities.ts`). `MCP-002` registers real
 * tools/resources here; `AUTH-001`/`RUN-002` add authenticated runtime
 * access — neither exists yet, so neither belongs in this node.
 */
export const lokiMcpServerFactory: McpServerFactory = () => {
  return new McpServer(
    { name: LOKI_MCP_SERVER_NAME, version: LOKI_MCP_SERVER_VERSION },
    { capabilities: LOKI_MCP_SERVER_CAPABILITIES },
  );
};
