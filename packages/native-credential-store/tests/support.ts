import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const packageDir = path.resolve(here, '..');
const repoRoot = path.resolve(packageDir, '..', '..');

export const targetDir = process.env.CARGO_TARGET_DIR
  ? path.resolve(process.env.CARGO_TARGET_DIR)
  : path.join(repoRoot, 'target');

export const mintTestCredentialExe = path.join(targetDir, 'debug', 'mint_test_credential.exe');

/**
 * Mints (or loads, idempotently) a real, DPAPI-protected credential via the
 * real Rust `loki_storage::credential::mint_or_load_credential` call —
 * driven from this Node test process, exactly as `P1-NODE-IPC-CLIENT-001`
 * §6 Deliverable 6 requires ("it must actually run both the real Rust
 * minting call and the real compiled Node native addon in the same test
 * run").
 *
 * The minted plaintext crosses the process boundary only via a transient,
 * test-local file this function reads exactly once and deletes immediately
 * afterward — never via argv/env/stdout, and never printed/logged. Returns
 * the plaintext `Buffer` so the caller can compare it byte-for-byte against
 * what the compiled native addon independently decrypts.
 */
export function mintRealCredential(
  kind: 'desktop_installation' | 'agent_connection',
  protectedPath: string,
): Buffer {
  const evidencePath = path.join(
    os.tmpdir(),
    `lokiai-credential-evidence-${process.pid}-${Math.random().toString(36).slice(2)}.tmp`,
  );
  try {
    const result = spawnSync(mintTestCredentialExe, [kind, protectedPath, evidencePath], {
      stdio: 'inherit',
    });
    if (result.status !== 0) {
      throw new Error(
        `mint_test_credential exited with status ${String(result.status)} for kind ${kind}`,
      );
    }
    return fs.readFileSync(evidencePath);
  } finally {
    if (fs.existsSync(evidencePath)) {
      fs.unlinkSync(evidencePath);
    }
  }
}
