import type { ServerCapabilities } from '@modelcontextprotocol/server';

/**
 * The MCP capability set this server honestly advertises today, during the
 * legacy-era `initialize` handshake and the modern-era `server/discover`
 * advertisement alike (both are populated by the SDK directly from this
 * value — see `server.ts`).
 *
 * `Docs/graph/tasks/MCP-001.md` Section 5 (Ownership) is explicit that this
 * node ships no concrete tools/resources beyond whatever minimal surface the
 * protocol itself requires to be a valid server ("MCP-002 adds the real
 * read/diagnostic tools"), and Section 7's acceptance criteria requires "No
 * capability advertised that isn't actually implemented."
 *
 * Per the MCP spec (see the official SDK's own `ServerOptions.capabilities`
 * doc comment), a server that declares a capability — `tools`, `resources`,
 * `prompts`, `completions`, `logging`, `tasks` — commits to answering that
 * capability's requests, even with an empty result, rather than the
 * protocol's normal "method not found" response. Declaring any of them here
 * with nothing registered would therefore be an aspirational, dishonest
 * advertisement, not a neutral placeholder. This constant is the empty
 * object until a later node (MCP-002 for tools/resources; a later node for
 * prompts/completions if ever adopted) registers the real handlers and
 * extends it in the same change — never before.
 */
export const LOKI_MCP_SERVER_CAPABILITIES: ServerCapabilities = {};
