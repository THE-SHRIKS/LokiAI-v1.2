import type { ServerCapabilities } from '@modelcontextprotocol/server';

/**
 * The MCP capability set this server honestly advertises today, during the
 * legacy-era `initialize` handshake and the modern-era `server/discover`
 * advertisement alike (both are populated by the SDK directly from this
 * value — see `server.ts`).
 *
 * `Docs/graph/tasks/MCP-001.md` Section 5 (Ownership) is explicit that that
 * node ships no concrete tools/resources beyond whatever minimal surface the
 * protocol itself requires to be a valid server ("MCP-002 adds the real
 * read/diagnostic tools"), and its Section 7 acceptance criteria requires
 * "No capability advertised that isn't actually implemented."
 *
 * Per the MCP spec (see the official SDK's own `ServerOptions.capabilities`
 * doc comment), a server that declares a capability — `tools`, `resources`,
 * `prompts`, `completions`, `logging`, `tasks` — commits to answering that
 * capability's requests, even with an empty result, rather than the
 * protocol's normal "method not found" response. Declaring any of them here
 * with nothing registered would therefore be an aspirational, dishonest
 * advertisement, not a neutral placeholder.
 *
 * This constant was the empty object through `MCP-001`. `MCP-002` — this
 * node — is exactly the "later node for tools/resources" that module doc
 * predicted, and registers two real tools in the same change
 * (`src/tools/index.ts`, called from `src/server.ts`'s
 * `lokiMcpServerFactory`): `tools: {}` below is honest because those tools
 * genuinely exist by the time any client ever sees this capability
 * advertised. `resources`/`prompts`/`completions` remain unset — this node
 * ships tools only (`Docs/graph/tasks/MCP-002.md` Section 4 accepts either
 * "tool/resource" naming; the two device-read operations are implemented as
 * tools here, so declaring a `resources` capability with nothing registered
 * would repeat the exact dishonesty this module already guards against).
 */
export const LOKI_MCP_SERVER_CAPABILITIES: ServerCapabilities = { tools: {} };
