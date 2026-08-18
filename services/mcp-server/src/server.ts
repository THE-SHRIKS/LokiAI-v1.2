import { McpServer, type McpServerFactory } from '@modelcontextprotocol/server';
import { LOKI_MCP_SERVER_CAPABILITIES } from './capabilities.js';
import { registerLokiTools } from './tools/index.js';

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
 * This factory declares identity and the honest capability set
 * (`capabilities.ts`), then registers `MCP-002`'s real read-only device
 * tools (`src/tools/index.ts`) — the empty-capabilities state
 * `Docs/graph/handoffs/MCP-001-1.md` describes is this node's own starting
 * point, not its final state; `capabilities.ts`'s own module doc predicted
 * exactly this extension. `AUTH-001`/`RUN-002`-backed authenticated runtime
 * access to real device data does not exist yet — see
 * `src/devices/runtimeUnavailableDataSource.ts` for the honest default this
 * factory wires in until it does.
 */
export const lokiMcpServerFactory: McpServerFactory = () => {
  const server = new McpServer(
    { name: LOKI_MCP_SERVER_NAME, version: LOKI_MCP_SERVER_VERSION },
    { capabilities: LOKI_MCP_SERVER_CAPABILITIES },
  );
  registerLokiTools(server);
  return server;
};
