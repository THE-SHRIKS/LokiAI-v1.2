/**
 * DSK-002 — real unit tests for `electron/preload/bridge.cts`'s
 * `createLokiBridge` factory, using an in-memory fake `IpcRendererLike` (no
 * live Electron renderer process needed — see `bridge.cts`'s module doc
 * comment for why this is the chosen approach, matching `electron/main/
 * security.test.cts`'s precedent of testing pure functions directly).
 */
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  DEVICES_LIST_CHANNEL,
  RUNTIME_GET_STATUS_CHANNEL,
  RUNTIME_STATUS_EVENT_CHANNEL,
  RUNTIME_STATUS_SUBSCRIBE_CHANNEL,
  RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL,
} from './contract.cjs';
import { createLokiBridge, type IpcRendererLike } from './bridge.cjs';

type Listener = (event: unknown, ...args: readonly unknown[]) => void;

/** Minimal, real (not mocked-away) event bookkeeping — the same shape Electron's own `ipcRenderer.on`/`removeListener` provide, per `IpcRendererLike`. */
class FakeIpcRenderer implements IpcRendererLike {
  invokeImpl: (channel: string) => Promise<unknown> = () => Promise.resolve(undefined);
  readonly invokeCalls: string[] = [];
  readonly sendCalls: Array<{ channel: string; args: readonly unknown[] }> = [];
  private readonly listeners = new Map<string, Set<Listener>>();

  invoke(channel: string, ...args: readonly unknown[]): Promise<unknown> {
    this.invokeCalls.push(channel);
    void args;
    return this.invokeImpl(channel);
  }

  send(channel: string, ...args: readonly unknown[]): void {
    this.sendCalls.push({ channel, args });
  }

  on(channel: string, listener: Listener): void {
    let set = this.listeners.get(channel);
    if (!set) {
      set = new Set();
      this.listeners.set(channel, set);
    }
    set.add(listener);
  }

  removeListener(channel: string, listener: Listener): void {
    this.listeners.get(channel)?.delete(listener);
  }

  listenerCount(channel: string): number {
    return this.listeners.get(channel)?.size ?? 0;
  }

  /** Simulates `webContents.send(channel, ...)` arriving in the renderer. */
  emit(channel: string, payload: unknown): void {
    for (const listener of this.listeners.get(channel) ?? []) {
      listener({}, payload);
    }
  }
}

const VALID_ERROR = Object.freeze({
  code: 'runtime_unavailable',
  category: 'transport',
  message: 'no runtime connection exists yet',
  retryable: true,
  correlation_id: 'corr-1234',
});

const VALID_STATUS_EVENT = Object.freeze({
  ok: true,
  data: { state: 'connected', observedAt: '2026-08-16T12:00:00.000Z' },
});

// --- runtime.getStatus() / devices.list() ---------------------------------------------

void test('runtime.getStatus resolves a well-formed result from main unchanged', async () => {
  const ipcRenderer = new FakeIpcRenderer();
  ipcRenderer.invokeImpl = () => Promise.resolve({ ok: false, error: VALID_ERROR });
  const bridge = createLokiBridge(ipcRenderer);

  const result = await bridge.runtime.getStatus();

  assert.deepEqual(result, { ok: false, error: VALID_ERROR });
  assert.deepEqual(ipcRenderer.invokeCalls, [RUNTIME_GET_STATUS_CHANNEL]);
});

void test('runtime.getStatus never fabricates a success — RUN-002 has no backend today', async () => {
  const ipcRenderer = new FakeIpcRenderer();
  ipcRenderer.invokeImpl = () => Promise.resolve({ ok: false, error: VALID_ERROR });
  const bridge = createLokiBridge(ipcRenderer);

  const result = await bridge.runtime.getStatus();

  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.equal(result.error.code, 'runtime_unavailable');
  }
});

void test('runtime.getStatus falls back to a local error (not a throw, not a fabricated success) when main rejects the invoke', async () => {
  const ipcRenderer = new FakeIpcRenderer();
  ipcRenderer.invokeImpl = () => Promise.reject(new Error('main process not responding'));
  const bridge = createLokiBridge(ipcRenderer);

  const result = await bridge.runtime.getStatus();

  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.match(result.error.message, /main process not responding/);
  }
});

void test('runtime.getStatus discards a malformed payload from main rather than forwarding it', async () => {
  const ipcRenderer = new FakeIpcRenderer();
  // `invokeImpl` is intentionally typed to return `Promise<unknown>` (main is
  // an independent process; preload cannot statically trust its shape) — the
  // malformed shape below is exactly what `isRuntimeStatusResult` must catch.
  ipcRenderer.invokeImpl = () => Promise.resolve({ ok: true, data: { state: 'not-a-real-state' } });
  const bridge = createLokiBridge(ipcRenderer);

  const result = await bridge.runtime.getStatus();

  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.equal(result.error.code, 'internal_correlation_error');
  }
});

void test('devices.list resolves a well-formed result and calls the correct channel', async () => {
  const ipcRenderer = new FakeIpcRenderer();
  ipcRenderer.invokeImpl = () => Promise.resolve({ ok: false, error: VALID_ERROR });
  const bridge = createLokiBridge(ipcRenderer);

  const result = await bridge.devices.list();

  assert.deepEqual(result, { ok: false, error: VALID_ERROR });
  assert.deepEqual(ipcRenderer.invokeCalls, [DEVICES_LIST_CHANNEL]);
});

// --- events.subscribe(topic, callback) — argument validation --------------------------

void test('events.subscribe throws synchronously on an unknown topic and never touches ipcRenderer', () => {
  const ipcRenderer = new FakeIpcRenderer();
  const bridge = createLokiBridge(ipcRenderer);

  assert.throws(() => {
    // @ts-expect-error deliberately malformed for this test
    bridge.events.subscribe('not_a_real_topic', () => {});
  }, TypeError);
  assert.equal(ipcRenderer.listenerCount(RUNTIME_STATUS_EVENT_CHANNEL), 0);
  assert.deepEqual(ipcRenderer.sendCalls, []);
});

void test('events.subscribe throws synchronously when callback is not a function and never touches ipcRenderer', () => {
  const ipcRenderer = new FakeIpcRenderer();
  const bridge = createLokiBridge(ipcRenderer);

  assert.throws(() => {
    // @ts-expect-error deliberately malformed for this test
    bridge.events.subscribe('runtime_status', 'not-a-function');
  }, TypeError);
  assert.equal(ipcRenderer.listenerCount(RUNTIME_STATUS_EVENT_CHANNEL), 0);
  assert.deepEqual(ipcRenderer.sendCalls, []);
});

// --- events.subscribe(topic, callback) — delivery + malformed-event handling ----------

void test('events.subscribe delivers a well-formed pushed event to the callback', () => {
  const ipcRenderer = new FakeIpcRenderer();
  const bridge = createLokiBridge(ipcRenderer);
  const received: unknown[] = [];

  bridge.events.subscribe('runtime_status', (event) => received.push(event));
  ipcRenderer.emit(RUNTIME_STATUS_EVENT_CHANNEL, VALID_STATUS_EVENT);

  assert.deepEqual(received, [VALID_STATUS_EVENT]);
});

void test('events.subscribe drops a malformed pushed event instead of forwarding it', () => {
  const ipcRenderer = new FakeIpcRenderer();
  const bridge = createLokiBridge(ipcRenderer);
  const received: unknown[] = [];

  bridge.events.subscribe('runtime_status', (event) => received.push(event));
  ipcRenderer.emit(RUNTIME_STATUS_EVENT_CHANNEL, { ok: true, data: { state: 'bogus-state' } });

  assert.deepEqual(received, [], 'malformed event must not reach the renderer callback');
});

// --- events.subscribe(topic, callback) — subscribe/unsubscribe bookkeeping, no leak ---

void test('events.subscribe registers exactly one listener and sends exactly one subscribe control message', () => {
  const ipcRenderer = new FakeIpcRenderer();
  const bridge = createLokiBridge(ipcRenderer);

  bridge.events.subscribe('runtime_status', () => {});

  assert.equal(ipcRenderer.listenerCount(RUNTIME_STATUS_EVENT_CHANNEL), 1);
  assert.deepEqual(
    ipcRenderer.sendCalls.map((call) => call.channel),
    [RUNTIME_STATUS_SUBSCRIBE_CHANNEL],
  );
});

void test('the returned unsubscribe function removes the listener and sends exactly one unsubscribe control message', () => {
  const ipcRenderer = new FakeIpcRenderer();
  const bridge = createLokiBridge(ipcRenderer);

  const unsubscribe = bridge.events.subscribe('runtime_status', () => {});
  unsubscribe();

  assert.equal(ipcRenderer.listenerCount(RUNTIME_STATUS_EVENT_CHANNEL), 0);
  assert.deepEqual(
    ipcRenderer.sendCalls.map((call) => call.channel),
    [RUNTIME_STATUS_SUBSCRIBE_CHANNEL, RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL],
  );
});

void test('calling unsubscribe twice is a safe no-op the second time (no double removeListener/send, no throw)', () => {
  const ipcRenderer = new FakeIpcRenderer();
  const bridge = createLokiBridge(ipcRenderer);

  const unsubscribe = bridge.events.subscribe('runtime_status', () => {});
  unsubscribe();
  assert.doesNotThrow(() => unsubscribe());

  assert.equal(ipcRenderer.listenerCount(RUNTIME_STATUS_EVENT_CHANNEL), 0);
  assert.deepEqual(
    ipcRenderer.sendCalls.map((call) => call.channel),
    [RUNTIME_STATUS_SUBSCRIBE_CHANNEL, RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL],
    'the second unsubscribe() call must not send another unsubscribe control message',
  );
});

void test('two independent subscriptions are isolated: unsubscribing one leaves the other receiving events (no leak, no cross-talk)', () => {
  const ipcRenderer = new FakeIpcRenderer();
  const bridge = createLokiBridge(ipcRenderer);
  const receivedA: unknown[] = [];
  const receivedB: unknown[] = [];

  const unsubscribeA = bridge.events.subscribe('runtime_status', (event) => receivedA.push(event));
  bridge.events.subscribe('runtime_status', (event) => receivedB.push(event));
  assert.equal(ipcRenderer.listenerCount(RUNTIME_STATUS_EVENT_CHANNEL), 2);

  unsubscribeA();
  assert.equal(
    ipcRenderer.listenerCount(RUNTIME_STATUS_EVENT_CHANNEL),
    1,
    'exactly one listener must remain',
  );

  ipcRenderer.emit(RUNTIME_STATUS_EVENT_CHANNEL, VALID_STATUS_EVENT);

  assert.deepEqual(receivedA, [], 'unsubscribed callback must not receive further events');
  assert.deepEqual(
    receivedB,
    [VALID_STATUS_EVENT],
    'the still-subscribed callback must still receive events',
  );
});

void test('repeated subscribe/unsubscribe cycles never accumulate listeners (no leak over many cycles)', () => {
  const ipcRenderer = new FakeIpcRenderer();
  const bridge = createLokiBridge(ipcRenderer);

  for (let i = 0; i < 25; i += 1) {
    const unsubscribe = bridge.events.subscribe('runtime_status', () => {});
    assert.equal(ipcRenderer.listenerCount(RUNTIME_STATUS_EVENT_CHANNEL), 1);
    unsubscribe();
    assert.equal(ipcRenderer.listenerCount(RUNTIME_STATUS_EVENT_CHANNEL), 0);
  }
});
