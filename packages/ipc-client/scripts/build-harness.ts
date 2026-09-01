#!/usr/bin/env node
// `P1-NODE-IPC-CLIENT-001`: builds the test-only `node_ipc_client_harness`
// binary (`crates/loki-credential-native`'s `node-client-harness` feature —
// never linked into the shipped production addon) this package's own real
// integration tests spawn to drive a real `loki-ipc` server.
//
// `.ts` (not `.mjs`): see `packages/native-credential-store/scripts/
// build.ts`'s own top comment for why — the same reasoning applies here
// verbatim.

import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const packageDir = path.resolve(scriptDir, '..');
const repoRoot = path.resolve(packageDir, '..', '..');

if (process.platform !== 'win32') {
  console.log('@lokiai/ipc-client: skipping harness build on non-Windows platform.');
  process.exit(0);
}

const targetDir = process.env.CARGO_TARGET_DIR
  ? path.resolve(process.env.CARGO_TARGET_DIR)
  : path.join(repoRoot, 'target');

const result = spawnSync(
  'cargo',
  ['build', '-p', 'loki-credential-native', '--features', 'node-client-harness'],
  {
    cwd: repoRoot,
    stdio: 'inherit',
    env: { ...process.env, CARGO_TARGET_DIR: targetDir },
  },
);
if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

const exe = path.join(targetDir, 'debug', 'node_ipc_client_harness.exe');
if (!fs.existsSync(exe)) {
  console.error(`expected cargo build to produce ${exe}, but it does not exist`);
  process.exit(1);
}
console.log(`@lokiai/ipc-client: harness ready at ${exe}`);
