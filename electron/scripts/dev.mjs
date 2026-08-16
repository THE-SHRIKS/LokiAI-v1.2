#!/usr/bin/env node
/**
 * DSK-001 — `pnpm run electron:dev` orchestrator.
 *
 * 1. Compiles `electron/main` + `electron/preload` (tsc).
 * 2. Starts the existing, unmodified `LokiAI Desktop UI` Vite dev server
 *    directly (not through its own `dev` npm script, which binds
 *    `0.0.0.0` for LAN browser access — unnecessary once Electron is the
 *    only intended client, so this binds `127.0.0.1` instead) and waits for
 *    Vite's own stdout to report the URL it actually bound to (Vite
 *    auto-increments the port if 3000 is busy, so the port is never
 *    hard-coded/assumed here).
 * 3. Launches the compiled Electron main process with
 *    `LOKIAI_DEV_SERVER_URL` set to that real URL.
 *
 * This is a plain Node script (no `concurrently`/`wait-on` dependency added
 * — AGENTS.md rule 11: no dependency merely for convenience) living beside
 * `electron/main` and `electron/preload` per DSK-001's ownership grant of the
 * `electron/` tree.
 */
import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..', '..');
const rendererDir = path.join(repoRoot, 'LokiAI Desktop UI');
const mainEntry = path.join(repoRoot, 'electron', 'dist', 'main', 'index.cjs');

const READY_TIMEOUT_MS = 45_000;
const LOCAL_URL_PATTERN = /Local:\s+(http:\/\/127\.0\.0\.1:\d+\/)/;
const isWindows = process.platform === 'win32';

/** @param {string} command @param {string[]} args @param {import('node:child_process').SpawnOptions} [options] */
function spawnProcess(command, args, options = {}) {
  return spawn(command, args, { ...options, shell: isWindows });
}

/** Best-effort whole-process-tree kill (a plain `.kill()` on Windows only kills the intermediate shell when `shell: true` was used). */
function killProcessTree(child) {
  if (child.pid === undefined || child.killed) {
    return;
  }
  if (isWindows) {
    spawn('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } else {
    child.kill('SIGTERM');
  }
}

/** @param {string} command @param {string[]} args @param {string} cwd */
function runToCompletion(command, args, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawnProcess(command, args, { cwd, stdio: 'inherit' });
    child.once('error', reject);
    child.once('exit', (code) => {
      if (code === 0) {
        resolve(undefined);
      } else {
        reject(new Error(`${command} ${args.join(' ')} exited with code ${String(code)}`));
      }
    });
  });
}

/** @param {import('node:child_process').ChildProcessWithoutNullStreams} viteProcess */
function waitForViteUrl(viteProcess) {
  return new Promise((resolve, reject) => {
    let settled = false;
    let buffer = '';
    const timeout = setTimeout(() => {
      if (!settled) {
        settled = true;
        reject(new Error(`Vite dev server did not report a URL within ${READY_TIMEOUT_MS}ms`));
      }
    }, READY_TIMEOUT_MS);

    const onData = (/** @type {Buffer} */ chunk) => {
      const text = chunk.toString('utf8');
      process.stdout.write(text);
      buffer += text;
      const match = LOCAL_URL_PATTERN.exec(buffer);
      if (match && !settled) {
        settled = true;
        clearTimeout(timeout);
        resolve(match[1]);
      }
    };
    viteProcess.stdout.on('data', onData);
    viteProcess.stderr.on('data', (/** @type {Buffer} */ chunk) => process.stderr.write(chunk));
    viteProcess.once('exit', (code) => {
      if (!settled) {
        settled = true;
        clearTimeout(timeout);
        reject(new Error(`Vite dev server exited early with code ${String(code)}`));
      }
    });
  });
}

async function main() {
  console.log('[electron:dev] compiling electron/main and electron/preload...');
  await runToCompletion('pnpm', ['exec', 'tsc', '-p', 'electron/tsconfig.json'], repoRoot);

  console.log(
    '[electron:dev] starting the Vite dev server for LokiAI Desktop UI (unmodified renderer)...',
  );
  const vite = spawnProcess('pnpm', ['exec', 'vite', '--port=3000', '--host=127.0.0.1'], {
    cwd: rendererDir,
  });

  let devServerUrl;
  try {
    devServerUrl = await waitForViteUrl(vite);
  } catch (error) {
    killProcessTree(vite);
    console.error(`[electron:dev] ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
    return;
  }

  console.log(`[electron:dev] Vite ready at ${devServerUrl} — launching Electron...`);
  const electronBinary = isWindows ? 'electron.cmd' : 'electron';
  const electronBinPath = path.join(repoRoot, 'node_modules', '.bin', electronBinary);
  const electronProcess = spawnProcess(electronBinPath, [mainEntry], {
    cwd: repoRoot,
    env: { ...process.env, LOKIAI_DEV_SERVER_URL: devServerUrl },
    stdio: 'inherit',
  });

  const cleanup = () => killProcessTree(vite);

  electronProcess.once('exit', (code) => {
    cleanup();
    process.exitCode = code ?? 0;
  });
  electronProcess.once('error', (error) => {
    console.error(`[electron:dev] failed to launch Electron: ${error.message}`);
    cleanup();
    process.exitCode = 1;
  });

  for (const signal of /** @type {const} */ (['SIGINT', 'SIGTERM'])) {
    process.once(signal, () => {
      killProcessTree(electronProcess);
      cleanup();
    });
  }
}

main().catch((error) => {
  console.error(`[electron:dev] ${error instanceof Error ? error.stack : String(error)}`);
  process.exitCode = 1;
});
