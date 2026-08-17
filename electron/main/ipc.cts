/**
 * DSK-002 — main-process side of the typed `window.loki` preload bridge
 * (`electron/preload/bridge.cts`). Registers exactly the `ipcMain.handle`/
 * `ipcMain.on` counterparts the preload's new methods need — see
 * `Docs/graph/tasks/DSK-002.md` Section 5 ("additive changes to
 * `electron/main/**` strictly limited to registering the new ipcMain.handle/
 * ipcMain.on counterparts the preload's new methods need").
 *
 * Every handler here:
 *   - validates the IPC sender is the app's own tracked `webContents`
 *     (`Docs/SECURITY.md` §5 "validate the IPC sender and top-level frame for
 *     every handler") before doing anything else;
 *   - validates its arguments (none of these methods accept renderer-supplied
 *     arguments today, so "validation" means rejecting unexpected extra args,
 *     not blindly trusting an empty-looking call);
 *   - returns an honest `runtime_unavailable` result, never a fabricated
 *     success — `RUN-002` (the local IPC transport to `crates/loki-ipc`) is
 *     not integrated with Electron main as of this node (confirmed: no
 *     `Docs/graph/handoffs/RUN-002-1.md` exists and no `crates/loki-ipc`
 *     directory exists in this worktree at the time this node was written —
 *     see the worker handoff Section 3 "which case applied").
 *
 * Imports the shared channel names/types/validators from
 * `electron/preload/contract.cts` rather than redefining them: the preload
 * bridge is the party defining the renderer-facing IPC contract (it lives
 * under `electron/preload/**`, this node's primary ownership per the task
 * packet), and this file implements the handler side of that *same* contract
 * — not a second, parallel copy of it.
 */
import { randomUUID } from 'node:crypto';
import {
  DEVICES_LIST_CHANNEL,
  RUNTIME_GET_STATUS_CHANNEL,
  RUNTIME_STATUS_EVENT_CHANNEL,
  RUNTIME_STATUS_SUBSCRIBE_CHANNEL,
  RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL,
  buildFailure,
  isDeviceListResult,
  isRuntimeStatusResult,
  type DeviceListResult,
  type RuntimeStatusResult,
} from '../preload/contract.cjs';
import type { LokiError } from '@lokiai/contracts' with { 'resolution-mode': 'import' };

export interface Logger {
  info(event: string, data?: Record<string, unknown>): void;
  warn(event: string, data?: Record<string, unknown>): void;
  error(event: string, data?: Record<string, unknown>): void;
}

// --- Minimal Electron surfaces this module depends on ---------------------------------
//
// Narrow, structural interfaces (not the full `electron` types) so the real
// `ipcMain`/`webContents` (which satisfy these structurally) and a plain
// in-memory fake (used by `ipc.test.cts`) both work without casts.

export interface IpcSenderLike {
  readonly id: number;
}

export interface IpcMainInvokeEventLike {
  readonly sender: IpcSenderLike;
}

export interface IpcMainEventLike {
  readonly sender: IpcSenderLike;
}

export interface WebContentsLike {
  readonly id: number;
  send(channel: string, ...args: readonly unknown[]): void;
}

export interface MinimalIpcMain {
  handle(
    channel: string,
    // `unknown` already covers a `Promise<unknown>` return (Electron's real
    // `ipcMain.handle` accepts either); a `unknown | Promise<unknown>` union
    // is redundant per `@typescript-eslint/no-redundant-type-constituents`.
    listener: (event: IpcMainInvokeEventLike, ...args: readonly unknown[]) => unknown,
  ): void;
  on(
    channel: string,
    listener: (event: IpcMainEventLike, ...args: readonly unknown[]) => void,
  ): void;
  removeHandler(channel: string): void;
  removeAllListeners(channel: string): void;
}

export interface RegisterLokiIpcHandlersDeps {
  readonly ipcMain: MinimalIpcMain;
  /** Always re-read at call time (not captured once) so this stays correct across window re-creation (`index.cts`'s `activate` handler). */
  readonly getTrustedWebContents: () => WebContentsLike | null;
  readonly logger: Logger;
}

export interface LokiIpcHandlers {
  /** Removes every registered handler/listener. Not called during normal app shutdown (process exit tears everything down); provided for test isolation and orderly re-registration. */
  dispose(): void;
  /** Call from the window's `closed` handler with a webContents ID captured *before* destruction, so a closed window never keeps receiving broadcast sends. */
  forgetSubscriber(webContentsId: number): void;
  /**
   * Not called anywhere in this node's production code paths — there is no
   * ongoing runtime-connection monitor yet (that begins with real `RUN-002`
   * integration plus a future runtime-connection node). Exported and tested
   * now so the "fan out to every current subscriber" logic is real,
   * reviewed, and ready rather than written untested the day it first
   * becomes load-bearing.
   */
  broadcastRuntimeStatus(status: RuntimeStatusResult): void;
}

/**
 * `packages/contracts/src/ids.ts` `CorrelationIdSchema` brands a bounded,
 * charset-restricted string (mirrored as `ID_PATTERN`/`ID_MAX_LENGTH` in
 * `electron/preload/contract.cts`); this single cast is safe because
 * `randomUUID()`'s output (36 lowercase hex digits and hyphens) always
 * satisfies that exact charset and length bound. Not calling the real branded
 * schema constructor (`CorrelationIdSchema.parse(...)`) because
 * `@lokiai/contracts`'s `zod` runtime cannot be imported from this compiled
 * CommonJS process — see `contract.cts`'s module doc comment for the
 * empirically-confirmed reason. Main process only (unlike preload, main has
 * full Node access, so `node:crypto` is safe to use here).
 */
function mintCorrelationId(): LokiError['correlation_id'] {
  return randomUUID() as LokiError['correlation_id'];
}

function buildRuntimeUnavailableError(source: string): LokiError {
  return {
    code: 'runtime_unavailable',
    category: 'transport',
    message: `LokiAI runtime local IPC (RUN-002) is not yet integrated with the desktop app; ${source} has no runtime connection to query.`,
    retryable: true,
    correlation_id: mintCorrelationId(),
  };
}

function buildPermissionDeniedError(source: string): LokiError {
  return {
    code: 'permission_denied',
    category: 'ipc',
    message: `${source}: rejected an IPC request from an untrusted sender.`,
    retryable: false,
    correlation_id: mintCorrelationId(),
  };
}

function buildInvalidRequestError(source: string, detail: string): LokiError {
  return {
    code: 'invalid_request',
    category: 'ipc',
    message: `${source}: ${detail}`,
    retryable: false,
    correlation_id: mintCorrelationId(),
  };
}

function buildInternalError(source: string, detail: string): LokiError {
  return {
    code: 'internal_correlation_error',
    category: 'ipc',
    message: `${source}: ${detail}`,
    retryable: true,
    correlation_id: mintCorrelationId(),
  };
}

/**
 * Pure predicate, directly unit-testable with plain fake objects (no live
 * Electron IPC harness needed — same approach `electron/main/security.
 * test.cts` uses for its own predicate functions, DSK-001). Compares by
 * `webContents.id` rather than object identity: this is the Electron-
 * community-standard pattern for "is this event really from my own window,"
 * and it is what makes a plain `{ id: 1 }`-shaped fake usable in tests.
 */
export function isTrustedSender(senderId: number, trusted: WebContentsLike | null): boolean {
  return trusted !== null && senderId === trusted.id;
}

/**
 * `RUN-002` (local IPC transport to `crates/loki-ipc`) is not integrated with
 * Electron main as of this node — see the worker handoff Section 3. This is
 * the single place a future node wires the real runtime-status query; today
 * it always returns the honest "no backend" error, never a fabricated status.
 */
function provideRuntimeStatus(): RuntimeStatusResult {
  return buildFailure(buildRuntimeUnavailableError('runtime.getStatus'));
}

/** Same honest-stub shape as `provideRuntimeStatus` — see that function's doc comment. */
function provideDeviceList(): DeviceListResult {
  return buildFailure(buildRuntimeUnavailableError('devices.list'));
}

export function registerLokiIpcHandlers(deps: RegisterLokiIpcHandlersDeps): LokiIpcHandlers {
  const { ipcMain, getTrustedWebContents, logger } = deps;
  const runtimeStatusSubscribers = new Map<number, WebContentsLike>();

  function rejectUntrustedInvoke(channel: string, senderId: number): void {
    logger.warn('ipc-sender-rejected', { channel, senderId });
  }

  ipcMain.handle(
    RUNTIME_GET_STATUS_CHANNEL,
    (event: IpcMainInvokeEventLike, ...args: readonly unknown[]): RuntimeStatusResult => {
      if (!isTrustedSender(event.sender.id, getTrustedWebContents())) {
        rejectUntrustedInvoke(RUNTIME_GET_STATUS_CHANNEL, event.sender.id);
        return buildFailure(buildPermissionDeniedError('runtime.getStatus'));
      }
      if (args.length > 0) {
        logger.warn('ipc-unexpected-arguments', {
          channel: RUNTIME_GET_STATUS_CHANNEL,
          count: args.length,
        });
        return buildFailure(
          buildInvalidRequestError('runtime.getStatus', 'no arguments are accepted'),
        );
      }
      const result = provideRuntimeStatus();
      if (!isRuntimeStatusResult(result)) {
        logger.error('ipc-invalid-internal-result', { channel: RUNTIME_GET_STATUS_CHANNEL });
        return buildFailure(
          buildInternalError(
            'runtime.getStatus',
            'internal status provider returned a malformed result',
          ),
        );
      }
      return result;
    },
  );

  ipcMain.handle(
    DEVICES_LIST_CHANNEL,
    (event: IpcMainInvokeEventLike, ...args: readonly unknown[]): DeviceListResult => {
      if (!isTrustedSender(event.sender.id, getTrustedWebContents())) {
        rejectUntrustedInvoke(DEVICES_LIST_CHANNEL, event.sender.id);
        return buildFailure(buildPermissionDeniedError('devices.list'));
      }
      if (args.length > 0) {
        logger.warn('ipc-unexpected-arguments', {
          channel: DEVICES_LIST_CHANNEL,
          count: args.length,
        });
        return buildFailure(buildInvalidRequestError('devices.list', 'no arguments are accepted'));
      }
      const result = provideDeviceList();
      if (!isDeviceListResult(result)) {
        logger.error('ipc-invalid-internal-result', { channel: DEVICES_LIST_CHANNEL });
        return buildFailure(
          buildInternalError(
            'devices.list',
            'internal device provider returned a malformed result',
          ),
        );
      }
      return result;
    },
  );

  ipcMain.on(
    RUNTIME_STATUS_SUBSCRIBE_CHANNEL,
    (event: IpcMainEventLike, ...args: readonly unknown[]): void => {
      const trusted = getTrustedWebContents();
      if (!isTrustedSender(event.sender.id, trusted) || trusted === null) {
        logger.warn('ipc-sender-rejected', {
          channel: RUNTIME_STATUS_SUBSCRIBE_CHANNEL,
          senderId: event.sender.id,
        });
        return;
      }
      if (args.length > 0) {
        logger.warn('ipc-unexpected-arguments', {
          channel: RUNTIME_STATUS_SUBSCRIBE_CHANNEL,
          count: args.length,
        });
        return;
      }
      runtimeStatusSubscribers.set(trusted.id, trusted);
      // Honest one-time snapshot at subscribe time. No ongoing runtime-
      // connection monitor exists yet (see module doc comment), so nothing
      // further is sent proactively; `broadcastRuntimeStatus` is the real,
      // tested hook a future monitor node calls when that changes.
      trusted.send(RUNTIME_STATUS_EVENT_CHANNEL, provideRuntimeStatus());
    },
  );

  ipcMain.on(
    RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL,
    (event: IpcMainEventLike, ...args: readonly unknown[]): void => {
      if (!isTrustedSender(event.sender.id, getTrustedWebContents())) {
        logger.warn('ipc-sender-rejected', {
          channel: RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL,
          senderId: event.sender.id,
        });
        return;
      }
      if (args.length > 0) {
        logger.warn('ipc-unexpected-arguments', {
          channel: RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL,
          count: args.length,
        });
        return;
      }
      runtimeStatusSubscribers.delete(event.sender.id);
    },
  );

  return {
    dispose(): void {
      ipcMain.removeHandler(RUNTIME_GET_STATUS_CHANNEL);
      ipcMain.removeHandler(DEVICES_LIST_CHANNEL);
      ipcMain.removeAllListeners(RUNTIME_STATUS_SUBSCRIBE_CHANNEL);
      ipcMain.removeAllListeners(RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL);
      runtimeStatusSubscribers.clear();
    },
    forgetSubscriber(webContentsId: number): void {
      runtimeStatusSubscribers.delete(webContentsId);
    },
    broadcastRuntimeStatus(status: RuntimeStatusResult): void {
      if (!isRuntimeStatusResult(status)) {
        logger.error('broadcast-invalid-status', { channel: RUNTIME_STATUS_EVENT_CHANNEL });
        return;
      }
      for (const webContents of runtimeStatusSubscribers.values()) {
        webContents.send(RUNTIME_STATUS_EVENT_CHANNEL, status);
      }
    },
  };
}
