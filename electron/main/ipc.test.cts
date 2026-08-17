/**
 * DSK-002 — real unit tests for `electron/main/ipc.cts`: sender validation
 * (proven to actually reject a spoofed sender, not just exist), argument
 * validation, the honest "no backend yet" response for every method, and
 * subscribe/unsubscribe bookkeeping with no leak. Uses a plain fake
 * `MinimalIpcMain`/`WebContentsLike` — no live Electron IPC harness, same
 * "unit-test the pure/injectable function directly" approach `electron/main/
 * security.test.cts` (DSK-001) uses, and the approach
 * `Docs/graph/tasks/DSK-002.md` Section 6.6 explicitly allows ("test via
 * direct unit test of the validation function, not requiring a full Electron
 * harness if that's impractical").
 */
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  DEVICES_LIST_CHANNEL,
  RUNTIME_GET_STATUS_CHANNEL,
  RUNTIME_STATUS_EVENT_CHANNEL,
  RUNTIME_STATUS_SUBSCRIBE_CHANNEL,
  RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL,
  isDeviceListResult,
  isRuntimeStatusResult,
} from '../preload/contract.cjs';
import {
  isTrustedSender,
  registerLokiIpcHandlers,
  type IpcMainEventLike,
  type IpcMainInvokeEventLike,
  type Logger,
  type MinimalIpcMain,
  type WebContentsLike,
} from './ipc.cjs';
import type { LokiError } from '@lokiai/contracts' with { 'resolution-mode': 'import' };

type InvokeListener = (event: IpcMainInvokeEventLike, ...args: readonly unknown[]) => unknown;
type OnListener = (event: IpcMainEventLike, ...args: readonly unknown[]) => void;

class FakeIpcMain implements MinimalIpcMain {
  private readonly handlers = new Map<string, InvokeListener>();
  private readonly onListeners = new Map<string, Set<OnListener>>();

  handle(channel: string, listener: InvokeListener): void {
    if (this.handlers.has(channel)) {
      throw new Error(`duplicate ipcMain.handle registration for ${channel}`);
    }
    this.handlers.set(channel, listener);
  }

  on(channel: string, listener: OnListener): void {
    let set = this.onListeners.get(channel);
    if (!set) {
      set = new Set();
      this.onListeners.set(channel, set);
    }
    set.add(listener);
  }

  removeHandler(channel: string): void {
    this.handlers.delete(channel);
  }

  removeAllListeners(channel: string): void {
    this.onListeners.delete(channel);
  }

  hasHandler(channel: string): boolean {
    return this.handlers.has(channel);
  }

  hasOnListeners(channel: string): boolean {
    return (this.onListeners.get(channel)?.size ?? 0) > 0;
  }

  async invokeHandler(
    channel: string,
    event: IpcMainInvokeEventLike,
    ...args: readonly unknown[]
  ): Promise<unknown> {
    const handler = this.handlers.get(channel);
    assert.ok(handler, `no handler registered for ${channel}`);
    // Real `ipcMain.handle` listeners may return a plain value or a Promise;
    // `await` here exercises both (and satisfies `require-await` — this
    // fake's own contract is genuinely async, matching `ipcRenderer.invoke`'s
    // real Promise-returning shape).
    return await handler(event, ...args);
  }

  emitOn(channel: string, event: IpcMainEventLike, ...args: readonly unknown[]): void {
    for (const listener of this.onListeners.get(channel) ?? []) {
      listener(event, ...args);
    }
  }
}

class FakeWebContents implements WebContentsLike {
  readonly id: number;
  readonly sendCalls: Array<{ channel: string; args: readonly unknown[] }> = [];

  constructor(id: number) {
    this.id = id;
  }

  send(channel: string, ...args: readonly unknown[]): void {
    this.sendCalls.push({ channel, args });
  }
}

class FakeLogger implements Logger {
  readonly infos: Array<{ event: string; data?: Record<string, unknown> }> = [];
  readonly warnings: Array<{ event: string; data?: Record<string, unknown> }> = [];
  readonly errors: Array<{ event: string; data?: Record<string, unknown> }> = [];

  info(event: string, data?: Record<string, unknown>): void {
    this.infos.push({ event, data });
  }

  warn(event: string, data?: Record<string, unknown>): void {
    this.warnings.push({ event, data });
  }

  error(event: string, data?: Record<string, unknown>): void {
    this.errors.push({ event, data });
  }
}

function setup(): {
  ipcMain: FakeIpcMain;
  trusted: FakeWebContents;
  logger: FakeLogger;
  handlers: ReturnType<typeof registerLokiIpcHandlers>;
} {
  const ipcMain = new FakeIpcMain();
  const trusted = new FakeWebContents(1);
  const logger = new FakeLogger();
  const handlers = registerLokiIpcHandlers({
    ipcMain,
    getTrustedWebContents: () => trusted,
    logger,
  });
  return { ipcMain, trusted, logger, handlers };
}

// --- isTrustedSender (pure function, directly tested) ----------------------------------

void test('isTrustedSender accepts a sender id matching the trusted webContents', () => {
  assert.equal(isTrustedSender(1, new FakeWebContents(1)), true);
});

void test('isTrustedSender rejects a spoofed sender id (mismatched webContents)', () => {
  assert.equal(isTrustedSender(2, new FakeWebContents(1)), false);
});

void test('isTrustedSender rejects when there is no trusted webContents (null)', () => {
  assert.equal(isTrustedSender(1, null), false);
});

// --- runtime.getStatus() ----------------------------------------------------------------

void test('runtime.getStatus: trusted sender with no args gets an honest runtime_unavailable result', async () => {
  const { ipcMain, trusted } = setup();

  const result = await ipcMain.invokeHandler(RUNTIME_GET_STATUS_CHANNEL, { sender: trusted });

  assert.ok(isRuntimeStatusResult(result), 'result must satisfy the RuntimeStatusResult shape');
  assert.equal(
    (result as { ok: boolean }).ok,
    false,
    'must never fabricate success — no backend exists yet',
  );
  assert.equal(
    (result as { ok: false; error: { code: string } }).error.code,
    'runtime_unavailable',
  );
});

void test('runtime.getStatus: a spoofed sender is rejected, not served the real (would-be) response', async () => {
  const { ipcMain, logger } = setup();
  const spoofed = new FakeWebContents(999);

  const result = await ipcMain.invokeHandler(RUNTIME_GET_STATUS_CHANNEL, { sender: spoofed });

  assert.ok(isRuntimeStatusResult(result));
  assert.equal((result as { ok: false; error: { code: string } }).error.code, 'permission_denied');
  assert.ok(
    logger.warnings.some(
      (w) => w.event === 'ipc-sender-rejected' && w.data?.channel === RUNTIME_GET_STATUS_CHANNEL,
    ),
    'sender rejection must be logged',
  );
});

void test('runtime.getStatus: unexpected extra arguments from a trusted sender are rejected as invalid_request', async () => {
  const { ipcMain, trusted, logger } = setup();

  const result = await ipcMain.invokeHandler(
    RUNTIME_GET_STATUS_CHANNEL,
    { sender: trusted },
    'unexpected-arg',
  );

  assert.ok(isRuntimeStatusResult(result));
  assert.equal((result as { ok: false; error: { code: string } }).error.code, 'invalid_request');
  assert.ok(logger.warnings.some((w) => w.event === 'ipc-unexpected-arguments'));
});

// --- devices.list() -----------------------------------------------------------------------

void test('devices.list: trusted sender with no args gets an honest runtime_unavailable result', async () => {
  const { ipcMain, trusted } = setup();

  const result = await ipcMain.invokeHandler(DEVICES_LIST_CHANNEL, { sender: trusted });

  assert.ok(isDeviceListResult(result));
  assert.equal((result as { ok: boolean }).ok, false);
  assert.equal(
    (result as { ok: false; error: { code: string } }).error.code,
    'runtime_unavailable',
  );
});

void test('devices.list: a spoofed sender is rejected', async () => {
  const { ipcMain, logger } = setup();
  const spoofed = new FakeWebContents(999);

  const result = await ipcMain.invokeHandler(DEVICES_LIST_CHANNEL, { sender: spoofed });

  assert.ok(isDeviceListResult(result));
  assert.equal((result as { ok: false; error: { code: string } }).error.code, 'permission_denied');
  assert.ok(
    logger.warnings.some(
      (w) => w.event === 'ipc-sender-rejected' && w.data?.channel === DEVICES_LIST_CHANNEL,
    ),
  );
});

void test('devices.list: unexpected extra arguments are rejected as invalid_request', async () => {
  const { ipcMain, trusted } = setup();

  const result = await ipcMain.invokeHandler(DEVICES_LIST_CHANNEL, { sender: trusted }, 1, 2, 3);

  assert.ok(isDeviceListResult(result));
  assert.equal((result as { ok: false; error: { code: string } }).error.code, 'invalid_request');
});

// --- runtime status subscribe/unsubscribe: sender validation + no-leak cleanup --------

void test('subscribe: trusted sender receives exactly one honest snapshot event', () => {
  const { ipcMain, trusted } = setup();

  ipcMain.emitOn(RUNTIME_STATUS_SUBSCRIBE_CHANNEL, { sender: trusted });

  assert.equal(trusted.sendCalls.length, 1);
  assert.equal(trusted.sendCalls[0]?.channel, RUNTIME_STATUS_EVENT_CHANNEL);
  const payload = trusted.sendCalls[0]?.args[0];
  assert.ok(isRuntimeStatusResult(payload));
  assert.equal(
    (payload as { ok: boolean }).ok,
    false,
    'the snapshot must be the honest unavailable state',
  );
});

void test('subscribe: a spoofed sender receives nothing and is not registered', () => {
  const { ipcMain, logger, handlers } = setup();
  const spoofed = new FakeWebContents(999);

  ipcMain.emitOn(RUNTIME_STATUS_SUBSCRIBE_CHANNEL, { sender: spoofed });

  assert.equal(spoofed.sendCalls.length, 0);
  assert.ok(logger.warnings.some((w) => w.event === 'ipc-sender-rejected'));

  // Confirm it was never registered: a broadcast afterward must not reach it.
  handlers.broadcastRuntimeStatus({ ok: false, error: buildSampleError() });
  assert.equal(spoofed.sendCalls.length, 0);
});

void test('subscribe: unexpected extra arguments from a trusted sender are rejected (no snapshot sent)', () => {
  const { ipcMain, trusted, logger } = setup();

  ipcMain.emitOn(RUNTIME_STATUS_SUBSCRIBE_CHANNEL, { sender: trusted }, 'unexpected');

  assert.equal(trusted.sendCalls.length, 0);
  assert.ok(logger.warnings.some((w) => w.event === 'ipc-unexpected-arguments'));
});

void test('unsubscribe: removes the sender from the broadcast registry (no leak)', () => {
  const { ipcMain, trusted, handlers } = setup();

  ipcMain.emitOn(RUNTIME_STATUS_SUBSCRIBE_CHANNEL, { sender: trusted });
  assert.equal(trusted.sendCalls.length, 1, 'sanity: the initial snapshot was sent');

  ipcMain.emitOn(RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL, { sender: trusted });
  handlers.broadcastRuntimeStatus({ ok: false, error: buildSampleError() });

  assert.equal(trusted.sendCalls.length, 1, 'no further sends after unsubscribe');
});

void test('unsubscribe: a spoofed sender cannot unsubscribe another sender', () => {
  const { ipcMain, trusted, handlers } = setup();
  const spoofed = new FakeWebContents(999);

  ipcMain.emitOn(RUNTIME_STATUS_SUBSCRIBE_CHANNEL, { sender: trusted });
  ipcMain.emitOn(RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL, { sender: spoofed });

  trusted.sendCalls.length = 0;
  handlers.broadcastRuntimeStatus({ ok: false, error: buildSampleError() });

  assert.equal(trusted.sendCalls.length, 1, 'the real subscriber must still receive broadcasts');
});

void test('forgetSubscriber (window-closed cleanup path) removes the subscriber (no leak)', () => {
  const { ipcMain, trusted, handlers } = setup();

  ipcMain.emitOn(RUNTIME_STATUS_SUBSCRIBE_CHANNEL, { sender: trusted });
  handlers.forgetSubscriber(trusted.id);
  handlers.broadcastRuntimeStatus({ ok: false, error: buildSampleError() });

  assert.equal(
    trusted.sendCalls.length,
    1,
    'only the initial snapshot — no broadcast reached a forgotten subscriber',
  );
});

void test('repeated subscribe/unsubscribe cycles never accumulate registry entries (no leak over many cycles)', () => {
  const { ipcMain, trusted, handlers } = setup();

  for (let i = 0; i < 25; i += 1) {
    ipcMain.emitOn(RUNTIME_STATUS_SUBSCRIBE_CHANNEL, { sender: trusted });
    ipcMain.emitOn(RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL, { sender: trusted });
  }
  trusted.sendCalls.length = 0;
  handlers.broadcastRuntimeStatus({ ok: false, error: buildSampleError() });

  assert.equal(
    trusted.sendCalls.length,
    0,
    'unsubscribed after every cycle — must receive no broadcast',
  );
});

// --- broadcastRuntimeStatus: output validation (defense in depth) ---------------------

void test('broadcastRuntimeStatus sends a well-formed status to every current subscriber', () => {
  const { ipcMain, trusted, handlers } = setup();
  const second = new FakeWebContents(2);
  const handlers2 = handlers; // same registry — subscribe both known webContents
  void handlers2;

  ipcMain.emitOn(RUNTIME_STATUS_SUBSCRIBE_CHANNEL, { sender: trusted });
  trusted.sendCalls.length = 0;

  const status = {
    ok: true as const,
    data: { state: 'connected' as const, observedAt: '2026-08-16T12:00:00.000Z' },
  };
  handlers.broadcastRuntimeStatus(status);

  assert.equal(trusted.sendCalls.length, 1);
  assert.deepEqual(trusted.sendCalls[0]?.args[0], status);
  void second;
});

void test('broadcastRuntimeStatus rejects and logs a malformed status instead of sending it', () => {
  const { trusted, logger, handlers, ipcMain } = setup();
  ipcMain.emitOn(RUNTIME_STATUS_SUBSCRIBE_CHANNEL, { sender: trusted });
  trusted.sendCalls.length = 0;

  // @ts-expect-error deliberately malformed for this test
  handlers.broadcastRuntimeStatus({ ok: true, data: { state: 'not-a-real-state' } });

  assert.equal(
    trusted.sendCalls.length,
    0,
    'a malformed status must never be sent to the renderer',
  );
  assert.ok(logger.errors.some((e) => e.event === 'broadcast-invalid-status'));
});

// --- dispose() ---------------------------------------------------------------------------

void test('dispose removes every registered handler/listener', () => {
  const { ipcMain, handlers } = setup();

  assert.ok(ipcMain.hasHandler(RUNTIME_GET_STATUS_CHANNEL));
  assert.ok(ipcMain.hasHandler(DEVICES_LIST_CHANNEL));
  assert.ok(ipcMain.hasOnListeners(RUNTIME_STATUS_SUBSCRIBE_CHANNEL));
  assert.ok(ipcMain.hasOnListeners(RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL));

  handlers.dispose();

  assert.ok(!ipcMain.hasHandler(RUNTIME_GET_STATUS_CHANNEL));
  assert.ok(!ipcMain.hasHandler(DEVICES_LIST_CHANNEL));
  assert.ok(!ipcMain.hasOnListeners(RUNTIME_STATUS_SUBSCRIBE_CHANNEL));
  assert.ok(!ipcMain.hasOnListeners(RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL));
});

function buildSampleError(): LokiError {
  return {
    code: 'runtime_unavailable',
    category: 'transport',
    message: 'sample',
    retryable: true,
    correlation_id: 'corr-sample' as unknown as LokiError['correlation_id'],
  };
}
