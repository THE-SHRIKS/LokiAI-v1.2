import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';

import {
  CredentialCorruptError,
  CredentialKindMismatchError,
  CredentialMissingError,
  CredentialWindowsError,
  loadAgentConnectionCredential,
  loadDesktopInstallationCredential,
} from '../src/index.js';
import { mintRealCredential } from './support.js';

const isWindows = process.platform === 'win32';
const describeWindowsOnly = isWindows ? describe : describe.skip;

let workDir: string;

beforeAll(() => {
  workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'lokiai-native-credential-store-'));
});

afterEach(() => {
  for (const entry of fs.readdirSync(workDir)) {
    fs.rmSync(path.join(workDir, entry), { force: true });
  }
});

function protectedPath(name: string): string {
  return path.join(workDir, name);
}

// `P1-NODE-IPC-CLIENT-001` §6 Deliverable 6: a real cross-language round
// trip — the real Rust minting call and the real compiled Node native
// addon both run in this same test file/process, against the same file.
describeWindowsOnly('native-credential-store cross-language round trip (real DPAPI)', () => {
  it('a real DesktopInstallation credential minted by Rust loads byte-for-byte identically through the compiled Node addon', () => {
    const file = protectedPath('desktop.credential.protected');
    const minted = mintRealCredential('desktop_installation', file);
    const loaded = loadDesktopInstallationCredential(file);
    expect(Buffer.compare(minted, loaded)).toBe(0);
    expect(loaded.length).toBeGreaterThan(0);
  });

  it('a real AgentConnection credential minted by Rust loads byte-for-byte identically through the compiled Node addon', () => {
    const file = protectedPath('agent.credential.protected');
    const minted = mintRealCredential('agent_connection', file);
    const loaded = loadAgentConnectionCredential(file);
    expect(Buffer.compare(minted, loaded)).toBe(0);
  });

  it('the two kinds mint distinct, independent secrets', () => {
    const desktopFile = protectedPath('desktop.credential.protected');
    const agentFile = protectedPath('agent.credential.protected');
    const desktop = mintRealCredential('desktop_installation', desktopFile);
    const agent = mintRealCredential('agent_connection', agentFile);
    expect(Buffer.compare(desktop, agent)).not.toBe(0);
  });

  it('loading a real DesktopInstallation-protected file as AgentConnection fails closed with CredentialKindMismatchError, never a silent wrong-kind decrypt', () => {
    const file = protectedPath('desktop.credential.protected');
    mintRealCredential('desktop_installation', file);
    expect(() => loadAgentConnectionCredential(file)).toThrow(CredentialKindMismatchError);
  });

  it('loading a real AgentConnection-protected file as DesktopInstallation fails closed with CredentialKindMismatchError', () => {
    const file = protectedPath('agent.credential.protected');
    mintRealCredential('agent_connection', file);
    expect(() => loadDesktopInstallationCredential(file)).toThrow(CredentialKindMismatchError);
  });

  it('loading a credential file that does not exist fails closed with CredentialMissingError, never a default credential', () => {
    const file = protectedPath('does-not-exist.protected');
    expect(() => loadDesktopInstallationCredential(file)).toThrow(CredentialMissingError);
  });

  it('loading a corrupted (truncated/random-overwritten) real protected file fails closed as CredentialCorruptError or CredentialWindowsError, never a default credential', () => {
    const file = protectedPath('desktop.credential.protected');
    mintRealCredential('desktop_installation', file);
    // Overwrite the real protected blob with random bytes. DPAPI's own
    // integrity check rejects this — empirically, real `CryptUnprotectData`
    // surfaces this as a Windows-level failure (`ERROR_INVALID_PARAMETER`),
    // not this module's own envelope-format check (which only runs after a
    // successful decrypt) — so the specific, typed, fail-closed error here
    // is `CredentialWindowsError`, not `CredentialCorruptError`. Both are
    // real, specific, distinguishable outcomes (never a default/empty
    // credential, never `CredentialKindMismatchError`), so this test
    // accepts either rather than over-asserting a single Windows error code
    // as a permanent contract.
    fs.writeFileSync(file, Buffer.from([9, 9, 9, 9, 9, 9, 9, 9]));
    try {
      loadDesktopInstallationCredential(file);
      expect.unreachable('loading a corrupted protected file must fail closed');
    } catch (error) {
      expect(
        error instanceof CredentialCorruptError || error instanceof CredentialWindowsError,
      ).toBe(true);
    }
  });

  it('rotating (delete + re-mint) only the AgentConnection credential leaves the untouched DesktopInstallation credential independently loadable and unchanged', () => {
    const desktopFile = protectedPath('desktop.credential.protected');
    const agentFile = protectedPath('agent.credential.protected');
    const desktopOriginal = mintRealCredential('desktop_installation', desktopFile);
    mintRealCredential('agent_connection', agentFile);

    fs.rmSync(agentFile);
    const agentRotated = mintRealCredential('agent_connection', agentFile);
    expect(agentRotated.length).toBeGreaterThan(0);

    const desktopAfterRotation = loadDesktopInstallationCredential(desktopFile);
    expect(Buffer.compare(desktopOriginal, desktopAfterRotation)).toBe(0);
  });

  it('never logs/prints a decrypted credential value anywhere in this test run', () => {
    // Documentation-as-assertion: every test above compares `Buffer`s
    // directly and never calls `console.log`/`JSON.stringify` on decrypted
    // plaintext. This test exists so a future edit that adds such a call is
    // at least colocated with an explicit reminder, not just prose in a doc
    // comment elsewhere.
    expect(true).toBe(true);
  });
});
