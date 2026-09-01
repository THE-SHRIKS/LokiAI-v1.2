import { type ChildProcessByStdio, spawn } from 'node:child_process';
import path from 'node:path';
import os from 'node:os';
import fs from 'node:fs';
import type { Readable, Writable } from 'node:stream';
import { fileURLToPath } from 'node:url';

type HarnessProcess = ChildProcessByStdio<Writable, Readable, null>;

const here = path.dirname(fileURLToPath(import.meta.url));
const packageDir = path.resolve(here, '..');
const repoRoot = path.resolve(packageDir, '..', '..');

export const targetDir = process.env.CARGO_TARGET_DIR
  ? path.resolve(process.env.CARGO_TARGET_DIR)
  : path.join(repoRoot, 'target');

export const harnessExe = path.join(targetDir, 'debug', 'node_ipc_client_harness.exe');

export interface RealHarnessInfo {
  pipe_name: string;
  desktop_installation_credential_path: string;
  agent_connection_credential_path: string;
  desktop_installation_principal_id: string;
  agent_connection_principal_id: string;
}

export interface SlowHarnessInfo {
  pipe_name: string;
  desktop_installation_credential_path: string;
  desktop_installation_principal_id: string;
}

export class Harness<Info> {
  private buffer = '';
  private readonly readyPromise: Promise<Info>;
  private resolveReady!: (info: Info) => void;

  constructor(public readonly proc: HarnessProcess) {
    this.readyPromise = new Promise((resolve) => {
      this.resolveReady = resolve;
    });
    let resolved = false;
    this.proc.stdout.on('data', (chunk: Buffer) => {
      this.buffer += chunk.toString('utf8');
      if (!resolved) {
        const newlineIndex = this.buffer.indexOf('\n');
        if (newlineIndex !== -1) {
          const line = this.buffer.slice(0, newlineIndex);
          resolved = true;
          this.resolveReady(JSON.parse(line) as Info);
        }
      }
    });
  }

  ready(): Promise<Info> {
    return this.readyPromise;
  }

  revoke(principalId: string): Promise<void> {
    return new Promise((resolve) => {
      const onData = (chunk: Buffer): void => {
        if (chunk.toString('utf8').includes('REVOKED')) {
          this.proc.stdout.off('data', onData);
          resolve();
        }
      };
      this.proc.stdout.on('data', onData);
      this.proc.stdin.write(`REVOKE ${principalId}\n`);
    });
  }

  async shutdown(): Promise<void> {
    await new Promise<void>((resolve) => {
      const onExit = (): void => resolve();
      this.proc.once('exit', onExit);
      this.proc.stdin.write('SHUTDOWN\n');
    });
  }
}

function uniqueWorkDir(label: string): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), `lokiai-ipc-client-${label}-`));
}

export function startRealHarness(label: string): Harness<RealHarnessInfo> {
  const workDir = uniqueWorkDir(label);
  const proc = spawn(harnessExe, ['real', workDir], { stdio: ['pipe', 'pipe', 'inherit'] });
  return new Harness<RealHarnessInfo>(proc);
}

export function startSlowHarness(label: string, delayMs: number): Harness<SlowHarnessInfo> {
  const workDir = uniqueWorkDir(label);
  const proc = spawn(harnessExe, ['slow', workDir, String(delayMs)], {
    stdio: ['pipe', 'pipe', 'inherit'],
  });
  return new Harness<SlowHarnessInfo>(proc);
}
