/**
 * DSK-001 — resolves what the main window should load: the live Vite dev
 * server (`electron:dev`) or the packaged/built renderer file. Pure functions
 * only (no Electron import) so they are directly unit-testable.
 */
import { existsSync } from 'node:fs';
import path from 'node:path';

/** Environment variable `electron/scripts/dev.mjs` sets to the running Vite dev server's URL. */
export const DEV_SERVER_URL_ENV_VAR = 'LOKIAI_DEV_SERVER_URL';

export interface RendererTarget {
  readonly kind: 'dev-server' | 'built-file' | 'missing-build';
  /** URL to hand to `BrowserWindow.loadURL`, or the built `index.html` path for `loadFile`. */
  readonly url?: string;
  readonly filePath?: string;
  readonly contentSecurityPolicy: string;
}

/**
 * `LokiAI Desktop UI/dist/index.html` relative to this compiled module's own
 * location. `electron/tsconfig.json` compiles `main/*.cts` to
 * `electron/dist/main/*.cjs`, so from there the renderer package sits three
 * directories up (`electron/dist/main` -> `electron/dist` -> `electron` ->
 * repo root -> `LokiAI Desktop UI/dist`). This relative shape is identical
 * unpackaged and packaged, because `forge.config.js` packages the repository
 * root as-is (see its `ignore` allowlist) rather than moving files around.
 */
export function resolveBuiltIndexHtmlPath(mainDirname: string): string {
  return path.join(mainDirname, '..', '..', '..', 'LokiAI Desktop UI', 'dist', 'index.html');
}

export function resolveRendererTarget(
  mainDirname: string,
  env: NodeJS.ProcessEnv,
  productionCsp: string,
  developmentCsp: string,
): RendererTarget {
  const devServerUrl = env[DEV_SERVER_URL_ENV_VAR];
  if (devServerUrl) {
    return { kind: 'dev-server', url: devServerUrl, contentSecurityPolicy: developmentCsp };
  }

  const indexHtmlPath = resolveBuiltIndexHtmlPath(mainDirname);
  if (!existsSync(indexHtmlPath)) {
    return { kind: 'missing-build', filePath: indexHtmlPath, contentSecurityPolicy: productionCsp };
  }

  return { kind: 'built-file', filePath: indexHtmlPath, contentSecurityPolicy: productionCsp };
}

/**
 * Truthful, explicit failure page shown when the renderer has not been built
 * yet (`LokiAI Desktop UI/dist/index.html` missing). Loaded via a `data:` URL
 * so it needs no extra packaged asset and cannot be confused with the real
 * app — DSK-001 requires this state be explicit, never a blank silent window.
 * Plain HTML with no inline `style`/`script` so it renders identically
 * whether or not a CSP happens to apply to a `data:` navigation.
 */
export function buildMissingRendererHtml(expectedPath: string): string {
  const escapedPath = expectedPath
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
  return `<!doctype html>
<html>
<head><meta charset="utf-8"><title>LokiAI Main — renderer not built</title></head>
<body>
<h1>LokiAI renderer build not found</h1>
<p>Expected a built renderer at:</p>
<pre>${escapedPath}</pre>
<p>Run "pnpm run electron:build" (or "pnpm --filter lokiai-desktop-ui run build") before packaging or launching, then retry.</p>
<p>This is an explicit failure state, not a blank window.</p>
</body>
</html>`;
}
