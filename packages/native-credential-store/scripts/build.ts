#!/usr/bin/env node
// `P1-NODE-IPC-CLIENT-001`: builds `crates/loki-credential-native` and
// copies the resulting cdylib into this package's own `native/` directory
// as a `.node` file Node's `require()` can load directly (a `.node` file is
// simply a renamed native library — no bundler/loader dependency needed).
//
// `.ts` (not `.mjs`): this workspace's shared `eslint.config.js` only
// exempts `tools/scripts/**`/`electron/scripts/**` from type-aware linting
// (out of this node's writable paths — not something this node may edit);
// a plain `.ts` script under this package's own `scripts/` directory is
// already covered by the same per-package `tsconfig.json`-driven typed
// linting every other `.ts` file in this package gets, with no config
// change needed. Runs directly under Node's native TypeScript support
// (`engines.node` already pins `>=24.19.0`, which strips types without a
// separate build step for this script's plain, enum-/namespace-free syntax).
//
// Modes:
//   `release`      — production addon: `cargo build --release`, no
//                     `test-support` feature, never builds the test-only
//                     `mint_test_credential` helper binary.
//   `test-support` — debug addon + the `mint_test_credential` helper binary
//                     this package's own vitest suite spawns to drive a
//                     real Rust minting call (`P1-NODE-IPC-CLIENT-001` §6
//                     Deliverable 6's cross-language round-trip test).

import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const packageDir = path.resolve(scriptDir, '..');
const repoRoot = path.resolve(packageDir, '..', '..');

const mode = process.argv[2] ?? 'release';
if (mode !== 'release' && mode !== 'test-support') {
  console.error(`unknown build mode: ${mode} (expected "release" or "test-support")`);
  process.exit(1);
}

if (process.platform !== 'win32') {
  console.log(
    `@lokiai/native-credential-store: skipping native build on non-Windows platform (${process.platform}); ` +
      "DPAPI is Windows-only, matching this workspace's scope (Docs/TECH_STACK.md §2).",
  );
  process.exit(0);
}

const targetDir = process.env.CARGO_TARGET_DIR
  ? path.resolve(process.env.CARGO_TARGET_DIR)
  : path.join(repoRoot, 'target');

function cargo(args: string[]): void {
  const result = spawnSync('cargo', args, {
    cwd: repoRoot,
    stdio: 'inherit',
    env: { ...process.env, CARGO_TARGET_DIR: targetDir },
    shell: false,
  });
  if (result.error) {
    console.error(result.error);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

const nativeDir = path.join(packageDir, 'native');
fs.mkdirSync(nativeDir, { recursive: true });

let profileDir: string;
if (mode === 'release') {
  cargo(['build', '--release', '-p', 'loki-credential-native']);
  profileDir = 'release';
} else {
  cargo(['build', '-p', 'loki-credential-native', '--features', 'test-support']);
  profileDir = 'debug';
}

const builtDll = path.join(targetDir, profileDir, 'loki_credential_native.dll');
if (!fs.existsSync(builtDll)) {
  console.error(`expected cargo build to produce ${builtDll}, but it does not exist`);
  process.exit(1);
}
const destination = path.join(nativeDir, 'loki_credential_native.node');
fs.copyFileSync(builtDll, destination);
console.log(`@lokiai/native-credential-store: built (${mode}) -> ${destination}`);

if (mode === 'test-support') {
  const helperExe = path.join(targetDir, 'debug', 'mint_test_credential.exe');
  if (!fs.existsSync(helperExe)) {
    console.error(`expected cargo build to produce ${helperExe}, but it does not exist`);
    process.exit(1);
  }
  console.log(`@lokiai/native-credential-store: test helper binary at ${helperExe}`);
}
