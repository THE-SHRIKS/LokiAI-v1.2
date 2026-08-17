/**
 * DSK-002 — pure, dependency-injected factory for the `window.loki` bridge
 * object `electron/preload/index.cts` exposes via `contextBridge`.
 *
 * Takes a minimal `ipcRenderer`-shaped dependency (`IpcRendererLike`, not the
 * real `electron` module) so this can be unit-tested with a plain fake — no
 * real Electron renderer process needed. Same pattern `electron/main/
 * security.cts` uses for `electron/main/security.test.cts` (DSK-001): pure
 * functions, Electron API wiring kept to the thin `index.cts` glue file.
 *
 * Every method here is named and typed — no generic `invoke(channel, ...
 * args)` passthrough is exposed, and no renderer-supplied value is ever used
 * to construct an IPC channel name (`AGENTS.md` locked-architecture rule;
 * `Docs/graph/tasks/DSK-002.md` Forbidden scope).
 */
import {
  DEVICES_LIST_CHANNEL,
  EVENT_TOPIC_CHANNELS,
  RUNTIME_GET_STATUS_CHANNEL,
  buildFailure,
  isDeviceListResult,
  isEventTopic,
  isRuntimeStatusResult,
  type DeviceListResult,
  type EventTopic,
  type FailableResult,
  type RuntimeStatusResult,
} from './contract.cjs';
import type { LokiError } from '@lokiai/contracts' with { 'resolution-mode': 'import' };

/**
 * The exact subset of Electron's real `ipcRenderer` this bridge uses. The
 * real `ipcRenderer` (imported in `index.cts`) satisfies this structurally;
 * tests pass a plain in-memory fake instead.
 */
export interface IpcRendererLike {
  invoke(channel: string, ...args: readonly unknown[]): Promise<unknown>;
  send(channel: string, ...args: readonly unknown[]): void;
  on(channel: string, listener: (event: unknown, ...args: readonly unknown[]) => void): void;
  removeListener(
    channel: string,
    listener: (event: unknown, ...args: readonly unknown[]) => void,
  ): void;
}

export interface LokiBridge {
  readonly runtime: {
    readonly getStatus: () => Promise<RuntimeStatusResult>;
  };
  readonly devices: {
    readonly list: () => Promise<DeviceListResult>;
  };
  readonly events: {
    readonly subscribe: (
      topic: EventTopic,
      callback: (event: RuntimeStatusResult) => void,
    ) => () => void;
  };
}

/**
 * Diagnostic-only, non-cryptographic ID for the rare defensive branch below
 * (main returned a response that fails validation). Correlation IDs are not
 * secrets, so `Math.random()` is an acceptable source here; this deliberately
 * does not use `node:crypto`'s `randomUUID` because sandboxed preload cannot
 * `require('node:crypto')` (see `contract.cts`'s module doc comment for the
 * empirically-confirmed reason) — main's equivalent helper
 * (`electron/main/ipc.cts` `mintCorrelationId`) has full Node access and uses
 * the real thing.
 */
function mintWeakDiagnosticId(): string {
  return `preload-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

/** A freshly minted diagnostic ID is, by construction, a non-empty string of `[A-Za-z0-9-]` characters — structurally satisfies `packages/contracts/src/brand.ts`'s `ID_PATTERN`/length bound, so this single narrowing cast to the branded type is safe. */
function toCorrelationId(rawId: string): LokiError['correlation_id'] {
  return rawId as LokiError['correlation_id'];
}

function buildPreloadValidationError(source: string): LokiError {
  return {
    code: 'internal_correlation_error',
    category: 'ipc',
    message: `window.loki.${source} received a response from the main process that failed local shape validation; discarded rather than forwarded.`,
    retryable: true,
    correlation_id: toCorrelationId(mintWeakDiagnosticId()),
  };
}

function buildPreloadTransportError(source: string, detail: string): LokiError {
  return {
    code: 'internal_correlation_error',
    category: 'ipc',
    message: `window.loki.${source} failed: ${detail}`,
    retryable: true,
    correlation_id: toCorrelationId(mintWeakDiagnosticId()),
  };
}

async function invokeAndValidate<TData>(
  ipcRenderer: IpcRendererLike,
  channel: string,
  source: string,
  isValid: (value: unknown) => value is FailableResult<TData>,
): Promise<FailableResult<TData>> {
  let raw: unknown;
  try {
    raw = await ipcRenderer.invoke(channel);
  } catch (err) {
    return buildFailure(
      buildPreloadTransportError(source, err instanceof Error ? err.message : String(err)),
    );
  }
  if (!isValid(raw)) {
    return buildFailure(buildPreloadValidationError(source));
  }
  return raw;
}

function subscribe(ipcRenderer: IpcRendererLike, topic: unknown, callback: unknown): () => void {
  if (!isEventTopic(topic)) {
    throw new TypeError(`window.loki.events.subscribe: unknown topic ${JSON.stringify(topic)}`);
  }
  if (typeof callback !== 'function') {
    throw new TypeError('window.loki.events.subscribe: callback must be a function');
  }
  const typedCallback = callback as (event: RuntimeStatusResult) => void;
  const channels = EVENT_TOPIC_CHANNELS[topic];

  let active = true;
  const listener = (_event: unknown, payload: unknown): void => {
    if (!active) {
      return;
    }
    if (!isRuntimeStatusResult(payload)) {
      // Main is trusted in this app's threat model, but the preload<->
      // renderer boundary is still a trust boundary per `AGENTS.md` rule 4 —
      // a malformed push (a coding bug, not an attack, given main's
      // trust level) is dropped rather than forwarded, and logged so it is
      // not silently lost from a debugging perspective.
      console.warn(`window.loki.events.subscribe: dropped a malformed "${topic}" event`);
      return;
    }
    typedCallback(payload);
  };

  ipcRenderer.on(channels.event, listener);
  ipcRenderer.send(channels.subscribe);

  return function unsubscribe(): void {
    if (!active) {
      return;
    }
    active = false;
    ipcRenderer.removeListener(channels.event, listener);
    ipcRenderer.send(channels.unsubscribe);
  };
}

export function createLokiBridge(ipcRenderer: IpcRendererLike): LokiBridge {
  return Object.freeze({
    runtime: Object.freeze({
      getStatus: (): Promise<RuntimeStatusResult> =>
        invokeAndValidate(
          ipcRenderer,
          RUNTIME_GET_STATUS_CHANNEL,
          'runtime.getStatus',
          isRuntimeStatusResult,
        ),
    }),
    devices: Object.freeze({
      list: (): Promise<DeviceListResult> =>
        invokeAndValidate(ipcRenderer, DEVICES_LIST_CHANNEL, 'devices.list', isDeviceListResult),
    }),
    events: Object.freeze({
      subscribe: (
        topic: EventTopic,
        callback: (event: RuntimeStatusResult) => void,
      ): (() => void) => subscribe(ipcRenderer, topic, callback),
    }),
  });
}
