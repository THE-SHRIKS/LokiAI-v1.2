/**
 * `@lokiai/native-credential-store` — `P1-NODE-IPC-CLIENT-001` / `ADR-0009`
 * (Option A, accepted).
 *
 * Thin TypeScript wrapper around the compiled `crates/loki-credential-native`
 * native Node addon: the one shared, narrow, read-only mechanism a Node.js
 * process (Electron main, the MCP server) uses to decrypt one of the two
 * purpose-bound, Windows-DPAPI-protected credentials
 * `crates/loki-storage/src/credential.rs` mints
 * (`crates/loki-runtime/src/host/credential.rs`).
 *
 * **Read-only.** This package never mints a credential — only
 * `loadDesktopInstallationCredential`/`loadAgentConnectionCredential` are
 * exported, matching the native addon's own least-privilege boundary
 * exactly. Each function returns the raw decrypted secret bytes in a
 * `Buffer`, entirely in-process (the compiled addon call itself never
 * touches a temp file, stdout, or an environment variable) — never a
 * default/empty credential on failure; every failure mode is a specific,
 * typed, distinguishable `Error` subclass below, so a caller can fail
 * closed on the *specific* reason.
 */

import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/** Base class for every typed error this package throws. Never thrown directly. */
export abstract class CredentialNativeError extends Error {
  constructor(message: string) {
    super(message);
    this.name = new.target.name;
  }
}

/** The protected credential file does not exist at the given path. */
export class CredentialMissingError extends CredentialNativeError {}

/**
 * The protected credential file exists but could not be decrypted/parsed —
 * corrupted, truncated, or otherwise not a valid protected credential.
 */
export class CredentialCorruptError extends CredentialNativeError {}

/**
 * The file decrypted successfully but is a *different* purpose-bound
 * credential kind than the caller asked to load (e.g. calling
 * `loadAgentConnectionCredential` against a `DesktopInstallation`-protected
 * file) — never silently treated as success or as generic corruption.
 */
export class CredentialKindMismatchError extends CredentialNativeError {}

/**
 * A Windows-API-level failure (DPAPI itself, or the underlying file I/O)
 * unrelated to the credential's own kind/format.
 */
export class CredentialWindowsError extends CredentialNativeError {}

/** Thrown on any non-Windows platform — DPAPI is Windows-only, matching
 * every other `loki-storage` secret-protection mechanism in this workspace
 * (`Docs/TECH_STACK.md` §2 scopes v1.2 to Windows 11 x64). Never silently
 * falls back to an unprotected/no-op credential path. */
export class UnsupportedPlatformError extends CredentialNativeError {}

interface NativeAddon {
  loadDesktopInstallationCredential(protectedPath: string): Buffer;
  loadAgentConnectionCredential(protectedPath: string): Buffer;
}

const here = path.dirname(fileURLToPath(import.meta.url));
const nativeModuleRequire = createRequire(import.meta.url);

let cachedAddon: NativeAddon | undefined;

/**
 * Loads (and caches) the compiled native addon. Overridable only through
 * `LOKIAI_NATIVE_CREDENTIAL_STORE_PATH` (an absolute path to an alternate
 * compiled `.node` file) for this package's own test suite — production
 * callers always get the addon built alongside this package
 * (`native/loki_credential_native.node`, produced by `npm run build`).
 */
function loadAddon(): NativeAddon {
  if (cachedAddon) {
    return cachedAddon;
  }
  if (process.platform !== 'win32') {
    throw new UnsupportedPlatformError(
      `@lokiai/native-credential-store requires Windows (DPAPI); unsupported platform: ${process.platform}`,
    );
  }
  const overridePath = process.env.LOKIAI_NATIVE_CREDENTIAL_STORE_PATH;
  const modulePath = overridePath
    ? path.resolve(overridePath)
    : path.join(here, '..', 'native', 'loki_credential_native.node');
  cachedAddon = nativeModuleRequire(modulePath) as NativeAddon;
  return cachedAddon;
}

/**
 * Maps a thrown native-addon error to the specific typed subclass above,
 * using the stable `CREDENTIAL_*:` prefix
 * `crates/loki-credential-native/src/lib.rs::map_error` encodes. Never
 * changes/inspects the message beyond that prefix — the detail suffix
 * (path/kind names) is preserved verbatim for operator diagnosis, and never
 * contains decrypted secret bytes (there are none to include at an error
 * path).
 */
function mapNativeError(error: unknown): Error {
  if (!(error instanceof Error)) {
    return new Error(String(error));
  }
  const { message } = error;
  if (message.startsWith('CREDENTIAL_MISSING:')) {
    return new CredentialMissingError(message);
  }
  if (message.startsWith('CREDENTIAL_CORRUPT:')) {
    return new CredentialCorruptError(message);
  }
  if (message.startsWith('CREDENTIAL_KIND_MISMATCH:')) {
    return new CredentialKindMismatchError(message);
  }
  if (message.startsWith('CREDENTIAL_WINDOWS_ERROR:')) {
    return new CredentialWindowsError(message);
  }
  return error;
}

/**
 * Reads and decrypts the `DesktopInstallation` credential at
 * `protectedPath`. Fails closed with one of this module's typed errors on a
 * missing, corrupt, or wrong-kind file — never returns a default/empty
 * `Buffer`.
 */
export function loadDesktopInstallationCredential(protectedPath: string): Buffer {
  try {
    return loadAddon().loadDesktopInstallationCredential(protectedPath);
  } catch (error) {
    throw mapNativeError(error);
  }
}

/**
 * Reads and decrypts the `AgentConnection` credential at `protectedPath`.
 * Same fail-closed contract as {@link loadDesktopInstallationCredential}.
 */
export function loadAgentConnectionCredential(protectedPath: string): Buffer {
  try {
    return loadAddon().loadAgentConnectionCredential(protectedPath);
  } catch (error) {
    throw mapNativeError(error);
  }
}

/** Which purpose-bound credential kind to load — matches
 * `loki_ipc::business::PrincipalKind`'s two non-`RuntimeService` variants. */
export type CredentialKind = 'desktop_installation' | 'agent_connection';

/**
 * Convenience dispatcher over {@link loadDesktopInstallationCredential}/
 * {@link loadAgentConnectionCredential} for a caller that already has a
 * `CredentialKind` value in hand (e.g. `packages/ipc-client`'s own
 * `connect(config)`).
 */
export function loadCredential(kind: CredentialKind, protectedPath: string): Buffer {
  switch (kind) {
    case 'desktop_installation':
      return loadDesktopInstallationCredential(protectedPath);
    case 'agent_connection':
      return loadAgentConnectionCredential(protectedPath);
    default: {
      const exhaustive: never = kind;
      throw new Error(`unknown credential kind: ${String(exhaustive)}`);
    }
  }
}
