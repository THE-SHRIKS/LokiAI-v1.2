# `@lokiai/mcp-server`

LokiAI Main v1.2's provider-neutral MCP server foundation (`MCP-001`). A
standalone Node stdio server built on the official MCP TypeScript SDK v2
(`@modelcontextprotocol/server` / `@modelcontextprotocol/core`), speaking
protocol revision `2026-07-28` (the modern/stateless era) plus the SDK's own
bounded 2025-era legacy adapter, per `Docs/MCP_SPEC.md` and
`Docs/TECH_STACK.md`.

## Scope

This node is the protocol shell only:

- stdio transport bootstrap (`serveStdio`);
- modern-era `server/discover` capability negotiation and legacy-era
  `initialize` handshake — both handled by the official SDK, not
  hand-rolled here;
- structured error/diagnostic logging;
- an honestly **empty** capability set — no tools, no resources, no
  prompts. `MCP-002` registers the real read/diagnostic tools and extends
  `src/capabilities.ts` in the same change that adds them, never before.

Out of scope, deliberately: any auth/session logic (`AUTH-001`/`RUN-002`),
any device/connector access, and any concrete MCP tool or resource. See
`Docs/graph/tasks/MCP-001.md` Section 5.

## Why `@modelcontextprotocol/server` (v2), not `@modelcontextprotocol/sdk` (v1)

`Docs/TECH_STACK.md`'s MCP row and `Docs/MCP_SPEC.md` both target protocol
revision `2026-07-28`. The legacy monolithic `@modelcontextprotocol/sdk`
package (latest published version at the time of this node: `1.30.0`,
2026-07-27) does not implement it — its own `LATEST_PROTOCOL_VERSION`
constant is `2025-11-25`, and it has no `server/discover` handler, `_meta`
envelope model, or `2026-07-28` wire codec at all (verified directly against
the published package contents, not assumed). The SDK's own v2 packages
(`@modelcontextprotocol/{core,server,client,...}`, all `2.0.0`, published the
same day) are the real, live, current implementation of `2026-07-28` — each
package's own README states "v2 is the stable release line, implementing the
2026-07-28 MCP spec" and "It replaces the monolithic `@modelcontextprotocol/sdk`
package from v1." This is confirmed against the real, public MCP
specification repository (`modelcontextprotocol/modelcontextprotocol`),
whose `schema/2026-07-28/` and `docs/specification/2026-07-28/changelog.mdx`
describe exactly the stateless/`_meta`-envelope/`server/discover` model
`Docs/MCP_SPEC.md` Section 3 requires.

`@modelcontextprotocol/node` (Node HTTP-framework middleware) and the
`express`/`fastify`/`hono` adapter packages are **not** dependencies here:
`Docs/MCP_SPEC.md` Section 2 — "v1.2 default transport: local stdio... Remote
Streamable HTTP is disabled until separately accepted and threat-modeled" —
so only `@modelcontextprotocol/server`'s `./stdio` subpath export
(`serveStdio`) is used.

## The 2025-era "previous-version edge adapter"

`Docs/MCP_SPEC.md` Section 3 requires "one explicitly bounded previous-version
edge adapter." The official SDK models exactly two protocol **eras**, not one
adapter per date string: `modern` (`2026-07-28` only, negotiated via
`server/discover`) and `legacy` (`2024-10-07` through `2025-11-25` — the
whole pre-2026 family, sharing one `initialize`-handshake wire behavior,
byte-identical to the pre-2026-07-28 SDK). `serveStdio`'s default
`legacy: 'serve'` posture **is** that one bounded adapter: it answers any
2025-era opening from the same factory this server ships, without this
package hand-picking or hardcoding a single supported prior date itself. This
is a real, closed, SDK-owned boundary (see `tests/legacyAdapter.test.ts`,
which exercises four concrete dates from the family plus the fallback and
rejection paths), not a gap this node papered over.

`legacy: 'reject'` is the flip point `Docs/MCP_SPEC.md` Section 9 requires
for the adapter's eventual removal ("has ... a removal date"). This node
wires the option through (`StartMcpServerOptions.legacy`) but does not itself
pick when to flip it — the exact calendar/release trigger (e.g. "when the
last certified client ships 2026-07-28 support") is a product/release
decision, not a protocol-shell one.

## Why a build step exists (`scripts/build.ts`, `esbuild`)

Every other current workspace package (`@lokiai/contracts`,
`LokiAI Desktop UI`) is consumed through a bundler (Vite for the renderer,
Vite/esbuild-backed Vitest for tests) and ships no compiled `dist/` —
`packages/contracts`' own README flags this as "something to revisit if a
non-Vite/bundler consumer needs compiled output." This is that consumer: a
standalone process a coding client spawns directly with `node`, not through
any bundler.

Two independent, verified reasons plain `node src/index.ts` cannot run this
server as-is:

1. Node's native TypeScript support explicitly refuses to type-strip `.ts`
   files reached through a `node_modules` path ("To discourage package
   authors from publishing packages written in TypeScript, Node.js refuses
   to handle TypeScript files inside folders under a `node_modules` path" —
   `nodejs.org/api/typescript.html`). This turned out not to be the blocking
   issue here: pnpm's workspace symlink for `@lokiai/contracts` resolves
   (via `fs.realpath`) to `packages/contracts`, outside any `node_modules`
   path, so Node's own resolver does not classify it as a `node_modules`
   import.
2. The actual blocker: `@lokiai/contracts`' internal modules import each
   other with bundler-style specifiers (e.g. `from './brand.js'`, a `.ts`
   source file referenced by its post-compile `.js` extension — valid,
   idiomatic TypeScript under this repo's `moduleResolution: "bundler"`
   baseline, and exactly what Vite/Vitest and `tsc` both resolve correctly).
   Node's own ESM resolver requires an exact extension match and has no such
   mapping, so it cannot follow the import at all
   (`ERR_MODULE_NOT_FOUND ... imported from .../packages/contracts/src/index.ts`,
   reproduced directly before this module existed).

`tsc --noEmit` (`pnpm run typecheck`) and `vitest` (`pnpm run test`) are
unaffected by either point — both already resolve/transform
`@lokiai/contracts` correctly, exactly like `packages/contracts`' own test
suite does. Only "run the real compiled server as a real process" needed a
fix, so the fix (`esbuild`, a devDependency of this package only) is scoped
to exactly that: `pnpm run build` bundles `src/index.ts` — inlining this
package's own source and `@lokiai/contracts` into plain JS — into
`dist/index.js`, while leaving the real npm runtime dependencies
(`@modelcontextprotocol/server` and `zod`, both of which already ship real
compiled output) external. `pnpm run start` (or a coding client's MCP
configuration) runs `node dist/index.js` directly; nothing at that point
touches TypeScript, Node's type-stripping, or `node_modules` resolution
edge cases at all. `packages/contracts` itself is untouched — this fix is
entirely local to this package, per `MCP-001`'s ownership boundary.

## Structured error/diagnostic logging

`src/logging.ts` writes one JSON object per line to **stderr only** —
`Docs/MCP_SPEC.md` Section 2 and the official SDK's own stdio guide both
require this, since stdout is the strict JSON-RPC channel a host parses
line-by-line. `src/errorLog.ts` maps a caught SDK error (or any other thrown
value) into a `Docs/SPEC.md` Section 1 `LokiError`-shaped, `LokiErrorSchema`-
validated record for that log line — `Docs/graph/tasks/MCP-001.md`
Deliverable 3's "structured error responses (using `@lokiai/contracts`'
error shapes where applicable)." This is diagnostics only: the actual
JSON-RPC wire error response for a request is built and sent entirely by the
official SDK, never reimplemented here.

`@lokiai/contracts`' MCP-specific result/tool-name/resource-URI shapes
(`mcpReadResultSchema`, `McpOperationResultSchema`, `McpToolNameSchema`,
`parseLokiResourceUri`) are deliberately **not** used yet: this node has no
tools, resources, or reads to shape with them. Using them here would be
inventing product behavior ahead of `MCP-002`, which is exactly what
`AGENTS.md` rule 1 forbids.

## Layout

| Path                   | Contents                                                                                                                                                |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/capabilities.ts`  | The (currently empty) `ServerCapabilities` this node honestly advertises                                                                                |
| `src/server.ts`        | Server identity + the `McpServerFactory` the SDK's serving entry points call                                                                            |
| `src/bootstrap.ts`     | Wires `serveStdio` + structured error logging; `startMcpServer()`                                                                                       |
| `src/index.ts`         | The real process entry point (signals, stdio, exit code) — not unit-tested; see `Docs/graph/handoffs/MCP-001-1.md` for its process-level smoke evidence |
| `src/logging.ts`       | Structured stderr JSON-line logger                                                                                                                      |
| `src/errorLog.ts`      | Maps SDK/transport errors to a `LokiError`-shaped diagnostic record                                                                                     |
| `src/correlationId.ts` | Validated correlation-ID minting (`AGENTS.md` rule 5)                                                                                                   |
| `scripts/build.ts`     | `esbuild` bundle script (see above)                                                                                                                     |
| `tests/`               | Real protocol tests against the SDK's actual `serveStdio`, over an in-memory transport — see each file's own header comment                             |

## Commands

```bash
pnpm --filter @lokiai/mcp-server run typecheck   # tsc --noEmit
pnpm --filter @lokiai/mcp-server run test        # vitest run
pnpm --filter @lokiai/mcp-server run build       # esbuild -> dist/index.js
pnpm --filter @lokiai/mcp-server run start       # node dist/index.js (requires build first)
```
