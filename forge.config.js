// DSK-001 — Electron Forge configuration (packager + fuses).
//
// Loaded by `@electron-forge/cli` from the repository root (the same
// directory as `package.json`, which Forge always packages `dir: cwd`
// regardless of anything set here — see the header comment on `ignore`
// below for why that matters). Written as plain ESM `.js`: the repo root
// `package.json` already declares `"type": "module"`, and Forge's config
// loader (`@electron-forge/core`'s `forge-config.js`) tries `require()` and
// falls back to a real `import()` on `ERR_REQUIRE_ESM`, so this loads
// correctly either way — verified against the installed
// `@electron-forge/core@7.11.2` source, not assumed.
//
// TECH_STACK.md explicitly rejects the experimental Forge Vite plugin
// (renderer/main/preload are built explicitly instead — see
// `electron/tsconfig.json` and the root `electron:build*` scripts), so this
// file only configures `packagerConfig`, `makers` (deliberately empty —
// DSK-001 produces a runnable packaged app via `electron-forge package`,
// not an installer; installers/signing are `PKG-001`'s scope per the task
// packet's human-gate note), and the `@electron/fuses` baseline.
import { FuseV1Options, FuseVersion } from '@electron/fuses';
import { FusesPlugin } from '@electron-forge/plugin-fuses';

/**
 * `@electron/packager`'s `dir` option is always the Forge CLI's invocation
 * directory (`electron-forge package` run from the repo root), not anything
 * set in `packagerConfig` — confirmed by reading
 * `@electron-forge/core`'s `dist/api/package.js`, which destructures
 * `dir: providedDir = process.cwd()` from the CLI call and never reads
 * `packagerConfig.dir`. Rather than fight that (or introduce a second,
 * generated `forge.config` inside a staged sub-directory), this `ignore`
 * allowlists exactly the paths the packaged app needs out of the whole
 * monorepo checkout: the compiled `electron/dist/{main,preload}` output
 * (minus test files/sourcemaps), the existing renderer's build output
 * (`LokiAI Desktop UI/dist`, untouched — DSK-001 does not move or rebuild
 * it differently), and the root `package.json` Electron itself requires to
 * find its `main` entry. Everything else — `node_modules` (~600 dev-only
 * packages, none needed at runtime; see `prune: false` below for why this
 * function, not packager's own devDependency pruner, must be authoritative),
 * `Docs/`, `.git`, `tools/`, `Cargo.*`, lockfiles, `.claude/`, source of the
 * *other* not-yet-built pieces — is denied by default.
 *
 * Path shape received here is confirmed from `@electron/packager`'s own
 * `copy-filter.js`: repo-root-relative, leading `/`, forward-slash
 * normalized even on Windows.
 */
const KEEP_DIRECTORIES = [
  '/electron/dist/main',
  '/electron/dist/preload',
  '/LokiAI Desktop UI/dist',
];
const EXCLUDED_SUFFIXES = ['.test.cjs', '.test.cjs.map', '.map', '.d.ts', '.d.ts.map'];

function isPathAllowed(relativePath) {
  if (relativePath === '/package.json') {
    return true;
  }
  for (const keepDir of KEEP_DIRECTORIES) {
    if (relativePath === keepDir) {
      return true; // the kept directory itself
    }
    if (keepDir.startsWith(`${relativePath}/`)) {
      return true; // relativePath is an ancestor of a kept directory (e.g. "/electron" of "/electron/dist/main") — must stay "keep" or the walker prunes recursion before reaching it
    }
    if (relativePath.startsWith(`${keepDir}/`)) {
      return !EXCLUDED_SUFFIXES.some((suffix) => relativePath.endsWith(suffix));
    }
  }
  return false;
}

/** @type {import('@electron-forge/shared-types').ForgeConfig} */
const config = {
  packagerConfig: {
    name: 'LokiAI',
    executableName: 'lokiai',
    asar: true,
    // This function is authoritative for every path including
    // `node_modules/**` only when `prune` is disabled — @electron/packager's
    // own devDependency-based pruner otherwise intercepts `node_modules/*`
    // entries ahead of a custom `ignore` function, and its heuristics are
    // written for npm-flat node_modules, not pnpm's symlinked/virtual-store
    // layout this workspace uses.
    prune: false,
    ignore: (file) => !isPathAllowed(file),
    // Packaged output at the repo root's `electron/build/`, not the Forge
    // default `out/` — reuses the existing `**/build/**` ignore pattern
    // already in `.gitignore` and `eslint.config.js` (see FND-002), so
    // DSK-001 does not need to edit either shared file to keep generated
    // packages out of git/lint.
    out: 'electron/build',
  },
  rebuildConfig: {},
  makers: [],
  plugins: [
    new FusesPlugin({
      version: FuseVersion.V1,
      [FuseV1Options.RunAsNode]: false,
      [FuseV1Options.EnableCookieEncryption]: true,
      [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
      [FuseV1Options.EnableNodeCliInspectArguments]: false,
      [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: true,
      [FuseV1Options.OnlyLoadAppFromAsar]: true,
    }),
  ],
};

export default config;
