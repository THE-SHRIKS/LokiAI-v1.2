import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

/**
 * Bundles `src/index.ts` into a single self-contained `dist/index.js`.
 *
 * Why this exists (`Docs/graph/handoffs/MCP-001-1.md` has the full
 * evidence): `@lokiai/contracts`' `package.json` `"exports"` resolves
 * directly to TypeScript source with no compiled `dist/` (a deliberate
 * `CTR-001` decision for its Vite/vitest consumers). Node's own native
 * TypeScript support explicitly refuses to type-strip `.ts` files reached
 * through a `node_modules` path, AND its ESM resolver requires an exact
 * file-extension match — `@lokiai/contracts`' internal `./brand.js`-style
 * relative imports (valid, standard TS under this repo's
 * `moduleResolution: bundler` baseline) do not resolve under Node's own
 * resolver. A real standalone process launched with plain
 * `node src/index.ts` therefore cannot import `@lokiai/contracts` at all.
 * `tsc --noEmit` and `vitest` are unaffected (both already resolve/transform
 * this correctly), so this only matters for the one thing neither of those
 * commands does: actually running the server as a real process.
 *
 * esbuild resolves and inlines `@lokiai/contracts` (and this package's own
 * `src/**`) into plain JS at build time; the real npm runtime dependencies
 * (the MCP SDK, zod) are left external and loaded normally from
 * `node_modules` at run time, since they already ship real compiled output.
 */

const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.join(here, '..');

await build({
  entryPoints: [path.join(packageRoot, 'src', 'index.ts')],
  outfile: path.join(packageRoot, 'dist', 'index.js'),
  bundle: true,
  platform: 'node',
  format: 'esm',
  sourcemap: true,
  logLevel: 'info',
  external: ['@modelcontextprotocol/server', '@modelcontextprotocol/server/stdio', 'zod'],
});
