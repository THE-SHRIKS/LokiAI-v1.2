/**
 * DSK-002 — shared preload<->main IPC contract: channel names, event topics,
 * request/response types, and runtime validators for LokiAI's narrow, typed
 * `contextBridge` API (`Docs/graph/tasks/DSK-002.md`).
 *
 * This module has zero runtime dependency on `@lokiai/contracts`'s `zod`
 * schemas and zero dependency on any Node built-in beyond plain JS primitives
 * (`typeof`, `Array.isArray`, `RegExp`, `Object.hasOwn`) — by design, not by
 * oversight, for two confirmed, empirically-verified reasons:
 *
 * 1. **`@lokiai/contracts` cannot be `require()`d/`import()`ed at runtime from
 *    this project's compiled Electron `.cjs` output.** That package's
 *    `"exports"` field points directly at TypeScript source
 *    (`packages/contracts/src/index.ts`), documented in its own README as
 *    relying on a bundler/Vite/Vitest-style consumer to resolve `.ts` sources
 *    directly — and explicitly flagged there as something "whichever node
 *    first wires a non-Vite consumer should revisit ... if plain TS-source
 *    resolution turns out to be insufficient for that consumer's build."
 *    `electron/main` and `electron/preload` are exactly such a non-bundler
 *    consumer (plain `tsc` -> CommonJS `.cjs`, run directly by Node/Electron,
 *    per `electron/tsconfig.json`'s own header comment). Confirmed this task
 *    by compiling and running a real probe: a dynamic `import('@lokiai/
 *    contracts')` from compiled `electron/dist/main/*.cjs` throws
 *    `ERR_MODULE_NOT_FOUND` resolving that package's own internal
 *    `./brand.js`-style relative specifiers (Node's native TypeScript type-
 *    stripping — on by default in Node 24 — does not remap a `.js` specifier
 *    to a sibling `.ts` file the way a bundler or `ts-node`/`tsx` loader
 *    would). `packages/contracts/**` is outside this node's writable scope
 *    (`Docs/graph/tasks/DSK-002.md` Section 5), so this is recorded here and
 *    in the worker handoff as a real, confirmed follow-up for whoever owns
 *    that package next — not silently routed around by editing it.
 * 2. **Electron's sandboxed preload (`sandbox: true`, `Docs/SECURITY.md` §5,
 *    unchanged by this node) could not use `@lokiai/contracts`'s validators
 *    even if (1) were fixed by a future compiled/dual-format build.** A
 *    sandboxed preload's polyfilled `require()` only loads a small allowlist
 *    of Node built-ins. Confirmed empirically this task (a real, disposable
 *    `BrowserWindow` with `sandbox: true` + a probe preload script): `require
 *    ('node:events')` and `require('node:url')` succeed; `require('node:
 *    crypto')` and `require('node:fs')` throw "module not found"; and the Web
 *    Crypto `globalThis.crypto` global (which would otherwise be a
 *    Node-independent way to mint IDs) is also `undefined` in the sandboxed
 *    preload's isolated-world context. So *this* file — shared by both main
 *    (full Node access) and preload (sandboxed) — must not assume any Node
 *    built-in beyond what both sides actually have, which in practice means
 *    "none." (`electron/main/ipc.cts` is main-only and does use `node:
 *    crypto`'s `randomUUID` for the one thing that needs real randomness —
 *    minting a `LokiError.correlation_id` — see that file's own doc comment.)
 *
 * *Types*, by contrast, ARE imported directly from `@lokiai/contracts` below
 * (type-only imports have zero runtime footprint and resolve fine against the
 * package's `.ts` source at compile time — see the `with { 'resolution-mode':
 * 'import' }` clause TypeScript's Node16 resolution requires for a type-only
 * import of an ESM-only package from a CommonJS file). Every shape in this
 * module is therefore checked against the real upstream contract at compile
 * time, per `Docs/graph/tasks/DSK-002.md` Section 3 ("Type every exposed
 * method's request/response against `@lokiai/contracts` shapes where one
 * exists; do not invent a parallel type"). Only the *runtime validators*
 * below are hand-written, and each one cites the exact upstream rule/file it
 * mirrors so it can be kept in sync by inspection. Every closed enum this
 * module cares about is additionally mirrored through an exhaustive
 * `Record<Union, true>` map keyed by the imported union type itself, so an
 * upstream enum member being added/removed/renamed is a **compile error
 * here**, not a silent drift — TypeScript will refuse to compile this file
 * until the mirror is updated to match.
 */
import type {
  DeviceId,
  DeviceLifecycleState,
  LokiError,
  RuntimeConnectionState,
} from '@lokiai/contracts' with { 'resolution-mode': 'import' };

// --- Channel names --------------------------------------------------------
//
// Every channel is a fixed literal string, never constructed from renderer-
// supplied input — this is what makes the bridge "named methods," not a
// generic `invoke(channel, ...)` passthrough (`AGENTS.md` locked-architecture
// rule; `Docs/graph/tasks/DSK-002.md` Forbidden scope).

export const RUNTIME_GET_STATUS_CHANNEL = 'loki:runtime:get-status';
export const DEVICES_LIST_CHANNEL = 'loki:devices:list';
export const RUNTIME_STATUS_SUBSCRIBE_CHANNEL = 'loki:runtime:status:subscribe';
export const RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL = 'loki:runtime:status:unsubscribe';
export const RUNTIME_STATUS_EVENT_CHANNEL = 'loki:runtime:status:event';

// --- Event topics (Docs/SPEC.md Section 7 `window.loki.events.subscribe(...)`) --------
//
// Exactly one topic exists today (deliberately bounded scope per
// `Docs/graph/tasks/DSK-002.md` Section 6.1 — "an event-subscription
// mechanism for one class of runtime event", not a generic topic bus). Adding
// a second topic later is additive: extend `EVENT_TOPICS` and
// `EVENT_TOPIC_CHANNELS` together; the exhaustiveness map below forces both
// to be updated in lockstep.

export const EVENT_TOPICS = ['runtime_status'] as const;
export type EventTopic = (typeof EVENT_TOPICS)[number];

const EVENT_TOPIC_SET: Record<EventTopic, true> = {
  runtime_status: true,
};

export function isEventTopic(value: unknown): value is EventTopic {
  return typeof value === 'string' && Object.hasOwn(EVENT_TOPIC_SET, value);
}

export interface EventTopicChannels {
  readonly subscribe: string;
  readonly unsubscribe: string;
  readonly event: string;
}

/** Renderer-supplied `topic` is validated (`isEventTopic`) before ever being used to look up here — never used to construct a channel name string directly. */
export const EVENT_TOPIC_CHANNELS: Record<EventTopic, EventTopicChannels> = {
  runtime_status: {
    subscribe: RUNTIME_STATUS_SUBSCRIBE_CHANNEL,
    unsubscribe: RUNTIME_STATUS_UNSUBSCRIBE_CHANNEL,
    event: RUNTIME_STATUS_EVENT_CHANNEL,
  },
};

// --- Shared validation primitives (mirrors packages/contracts/src/brand.ts) -----------

/** Mirrors `packages/contracts/src/brand.ts` `ID_MAX_LENGTH`. */
const ID_MAX_LENGTH = 200;
/** Mirrors `packages/contracts/src/brand.ts` `ID_PATTERN`. */
const ID_PATTERN = /^[A-Za-z0-9._:-]+$/;

function isIdLike(value: unknown, maxLength: number = ID_MAX_LENGTH): value is string {
  return (
    typeof value === 'string' &&
    value.length >= 1 &&
    value.length <= maxLength &&
    ID_PATTERN.test(value)
  );
}

/** Mirrors `packages/contracts/src/errors.ts` `ERROR_CODE_PATTERN` (used for both `code` and `category`, exactly as upstream does). */
const ERROR_SLUG_PATTERN = /^[a-z][a-z0-9_]*$/;

function isErrorSlug(value: unknown, maxLength: number): value is string {
  return (
    typeof value === 'string' &&
    value.length >= 1 &&
    value.length <= maxLength &&
    ERROR_SLUG_PATTERN.test(value)
  );
}

/** Mirrors `packages/contracts/src/timestamp.ts` `IsoUtcTimestampSchema` (`z.iso.datetime()`, UTC-only: requires a literal `Z` suffix, rejects numeric offsets and timezone-less forms). */
const ISO_UTC_TIMESTAMP_PATTERN = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{1,9})?Z$/;

function isIsoUtcTimestamp(value: unknown): value is string {
  return typeof value === 'string' && ISO_UTC_TIMESTAMP_PATTERN.test(value);
}

export function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

// --- RuntimeConnectionState mirror (packages/contracts/src/states.ts) -----------------

const RUNTIME_CONNECTION_STATE_SET: Record<RuntimeConnectionState, true> = {
  unknown: true,
  starting: true,
  connected: true,
  degraded: true,
  disconnected: true,
  'version-incompatible': true,
  unauthorized: true,
  updating: true,
  stopping: true,
};

export function isRuntimeConnectionState(value: unknown): value is RuntimeConnectionState {
  return typeof value === 'string' && Object.hasOwn(RUNTIME_CONNECTION_STATE_SET, value);
}

// --- DeviceLifecycleState mirror (packages/contracts/src/states.ts) -------------------

const DEVICE_LIFECYCLE_STATE_SET: Record<DeviceLifecycleState, true> = {
  discovered: true,
  identifying: true,
  identified: true,
  checking: true,
  'enrollment-required': true,
  unsupported: true,
  degraded: true,
  enrolled: true,
  ready: true,
  offline: true,
  busy: true,
};

export function isDeviceLifecycleState(value: unknown): value is DeviceLifecycleState {
  return typeof value === 'string' && Object.hasOwn(DEVICE_LIFECYCLE_STATE_SET, value);
}

// --- LokiError mirror (packages/contracts/src/errors.ts LokiErrorSchema) --------------

export function isLokiErrorLike(value: unknown): value is LokiError {
  if (!isPlainObject(value)) {
    return false;
  }
  if (!isErrorSlug(value.code, 100)) {
    return false;
  }
  if (!isErrorSlug(value.category, 100)) {
    return false;
  }
  if (
    typeof value.message !== 'string' ||
    value.message.length < 1 ||
    value.message.length > 2000
  ) {
    return false;
  }
  if (typeof value.retryable !== 'boolean') {
    return false;
  }
  if (
    value.retry_after_ms !== undefined &&
    !(
      typeof value.retry_after_ms === 'number' &&
      Number.isInteger(value.retry_after_ms) &&
      value.retry_after_ms >= 0
    )
  ) {
    return false;
  }
  if (value.details !== undefined && !isPlainObject(value.details)) {
    return false;
  }
  if (!isIdLike(value.correlation_id)) {
    return false;
  }
  return true;
}

// --- Shared result envelope -------------------------------------------------------
//
// Not a shape from `@lokiai/contracts`: `Envelope<T>` there requires an
// authenticated `actor`/`workspace_id` this preload boundary does not have yet
// (no auth/workspace node has run), `OperationResult<T>` there models durable
// job outcomes (accepted/approval_required/rejected/completed), and
// `McpReadResult<T>` there is explicitly scoped to the MCP server
// (`packages/contracts/src/mcp.ts` module doc comment). None fit a simple
// preload read result. This minimal `{ ok, data } | { ok, error }` envelope
// is scoped to exactly this preload boundary and reuses the real imported
// `LokiError` type for its failure branch — it does not redefine that shape.

export type FailableResult<TData> =
  { readonly ok: true; readonly data: TData } | { readonly ok: false; readonly error: LokiError };

export function buildFailure<TData>(error: LokiError): FailableResult<TData> {
  return { ok: false, error };
}

// --- runtime.getStatus() (Docs/SPEC.md Section 4.3 Runtime connection) ----------------

export interface RuntimeStatusData {
  readonly state: RuntimeConnectionState;
  readonly observedAt: string;
}

export type RuntimeStatusResult = FailableResult<RuntimeStatusData>;

export function isRuntimeStatusResult(value: unknown): value is RuntimeStatusResult {
  if (!isPlainObject(value)) {
    return false;
  }
  if (value.ok === true) {
    const data = value.data;
    return (
      isPlainObject(data) &&
      isRuntimeConnectionState(data.state) &&
      isIsoUtcTimestamp(data.observedAt)
    );
  }
  if (value.ok === false) {
    return isLokiErrorLike(value.error);
  }
  return false;
}

// --- devices.list() (Docs/SPEC.md Section 3.3 Device — minimal projection) ------------
//
// Only `device_id` + `lifecycle_state` are projected here, not the full
// Device entity (connector identity, capability set, support tier, health
// projection, ...): no backend exists yet to observe those fields from (see
// `Docs/graph/tasks/DSK-002.md` Section 1), and inventing plausible-looking
// values for them would be exactly the kind of fabrication `AGENTS.md` rule 1
// forbids. A future node wiring a real device registry should extend this
// shape deliberately against `Docs/SPEC.md` 3.3, not have this module guess
// ahead of it.

export interface DeviceSummaryData {
  readonly deviceId: DeviceId;
  readonly lifecycleState: DeviceLifecycleState;
}

export type DeviceListResult = FailableResult<readonly DeviceSummaryData[]>;

function isDeviceSummaryData(value: unknown): value is DeviceSummaryData {
  return (
    isPlainObject(value) && isIdLike(value.deviceId) && isDeviceLifecycleState(value.lifecycleState)
  );
}

export function isDeviceListResult(value: unknown): value is DeviceListResult {
  if (!isPlainObject(value)) {
    return false;
  }
  if (value.ok === true) {
    const data: unknown = value.data;
    if (!Array.isArray(data)) {
      return false;
    }
    return data.every((item: unknown) => isDeviceSummaryData(item));
  }
  if (value.ok === false) {
    return isLokiErrorLike(value.error);
  }
  return false;
}
