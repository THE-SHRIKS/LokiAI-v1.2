/**
 * DSK-002 — real unit tests for `electron/preload/contract.cts`'s runtime
 * validators: valid input is accepted, malformed input (missing field, wrong
 * type, unknown enum value, bad charset, oversized string) is rejected. Same
 * `node:test` approach as `electron/main/security.test.cts` (DSK-001).
 */
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  EVENT_TOPIC_CHANNELS,
  EVENT_TOPICS,
  buildFailure,
  isDeviceListResult,
  isEventTopic,
  isLokiErrorLike,
  isPlainObject,
  isRuntimeStatusResult,
  type DeviceListResult,
  type RuntimeStatusResult,
} from './contract.cjs';
import type { LokiError } from '@lokiai/contracts' with { 'resolution-mode': 'import' };

const VALID_ERROR: LokiError = Object.freeze({
  code: 'runtime_unavailable',
  category: 'transport',
  message: 'no runtime connection exists yet',
  retryable: true,
  correlation_id: 'corr-1234' as unknown as LokiError['correlation_id'],
});

// --- isPlainObject ---------------------------------------------------------------

void test('isPlainObject accepts a plain object and rejects arrays/null/primitives', () => {
  assert.equal(isPlainObject({}), true);
  assert.equal(isPlainObject({ a: 1 }), true);
  assert.equal(isPlainObject([]), false);
  assert.equal(isPlainObject(null), false);
  assert.equal(isPlainObject(undefined), false);
  assert.equal(isPlainObject('x'), false);
  assert.equal(isPlainObject(1), false);
});

// --- isEventTopic ------------------------------------------------------------------

void test('isEventTopic accepts every declared topic', () => {
  for (const topic of EVENT_TOPICS) {
    assert.equal(isEventTopic(topic), true, topic);
  }
});

void test('isEventTopic rejects unknown topics and non-strings', () => {
  assert.equal(isEventTopic('device_status'), false);
  assert.equal(isEventTopic(''), false);
  assert.equal(isEventTopic(42), false);
  assert.equal(isEventTopic(null), false);
  assert.equal(isEventTopic(undefined), false);
  assert.equal(isEventTopic({}), false);
});

void test('EVENT_TOPIC_CHANNELS has an entry for every declared topic, each with 3 distinct channels', () => {
  for (const topic of EVENT_TOPICS) {
    const channels = EVENT_TOPIC_CHANNELS[topic];
    assert.ok(channels, topic);
    const set = new Set([channels.subscribe, channels.unsubscribe, channels.event]);
    assert.equal(set.size, 3, 'subscribe/unsubscribe/event channel names must be distinct');
  }
});

// --- isLokiErrorLike -----------------------------------------------------------------

void test('isLokiErrorLike accepts a well-formed error', () => {
  assert.equal(isLokiErrorLike(VALID_ERROR), true);
});

void test('isLokiErrorLike accepts an optional retry_after_ms/details when well-formed', () => {
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, retry_after_ms: 5000 }), true);
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, details: { hint: 'x' } }), true);
});

void test('isLokiErrorLike rejects non-objects', () => {
  assert.equal(isLokiErrorLike(null), false);
  assert.equal(isLokiErrorLike('error'), false);
  assert.equal(isLokiErrorLike([VALID_ERROR]), false);
});

void test('isLokiErrorLike rejects a missing required field', () => {
  for (const key of ['code', 'category', 'message', 'retryable', 'correlation_id']) {
    const { [key]: _omit, ...rest } = VALID_ERROR as Record<string, unknown>;
    assert.equal(isLokiErrorLike(rest), false, `missing ${key} must be rejected`);
  }
});

void test('isLokiErrorLike rejects a wrong-typed field', () => {
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, retryable: 'yes' }), false);
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, message: 123 }), false);
});

void test('isLokiErrorLike rejects a code/category that is not a lowercase snake_case slug', () => {
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, code: 'Runtime_Unavailable' }), false);
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, code: 'runtime unavailable' }), false);
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, category: '1transport' }), false);
});

void test('isLokiErrorLike rejects an oversized message (> 2000 chars, mirrors LokiErrorSchema)', () => {
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, message: 'x'.repeat(2001) }), false);
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, message: 'x'.repeat(2000) }), true);
});

void test('isLokiErrorLike rejects a malformed correlation_id (bad charset, empty, oversized)', () => {
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, correlation_id: '' }), false);
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, correlation_id: 'corr with spaces' }), false);
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, correlation_id: 'x'.repeat(201) }), false);
});

void test('isLokiErrorLike rejects a negative or non-integer retry_after_ms', () => {
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, retry_after_ms: -1 }), false);
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, retry_after_ms: 1.5 }), false);
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, retry_after_ms: 'soon' }), false);
});

void test('isLokiErrorLike rejects a non-object details field', () => {
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, details: 'x' }), false);
  assert.equal(isLokiErrorLike({ ...VALID_ERROR, details: [1, 2] }), false);
});

// --- isRuntimeStatusResult ------------------------------------------------------------

void test('isRuntimeStatusResult accepts a well-formed failure result', () => {
  const result: unknown = buildFailure(VALID_ERROR) satisfies RuntimeStatusResult;
  assert.equal(isRuntimeStatusResult(result), true);
});

void test('isRuntimeStatusResult accepts a well-formed success result', () => {
  const result = { ok: true, data: { state: 'connected', observedAt: '2026-08-16T12:00:00.000Z' } };
  assert.equal(isRuntimeStatusResult(result), true);
});

void test('isRuntimeStatusResult accepts a fractional-seconds UTC timestamp and rejects a non-UTC offset', () => {
  assert.equal(
    isRuntimeStatusResult({
      ok: true,
      data: { state: 'connected', observedAt: '2026-08-16T12:00:00.123Z' },
    }),
    true,
  );
  assert.equal(
    isRuntimeStatusResult({
      ok: true,
      data: { state: 'connected', observedAt: '2026-08-16T12:00:00+05:30' },
    }),
    false,
  );
});

void test('isRuntimeStatusResult rejects an unknown state value', () => {
  assert.equal(
    isRuntimeStatusResult({
      ok: true,
      data: { state: 'bogus', observedAt: '2026-08-16T12:00:00.000Z' },
    }),
    false,
  );
});

void test('isRuntimeStatusResult rejects a failure result with a malformed error', () => {
  assert.equal(
    isRuntimeStatusResult({ ok: false, error: { ...VALID_ERROR, retryable: 'yes' } }),
    false,
  );
});

void test('isRuntimeStatusResult rejects a missing/non-boolean ok discriminant and non-objects', () => {
  assert.equal(isRuntimeStatusResult({ data: {} }), false);
  assert.equal(isRuntimeStatusResult({ ok: 'true', data: {} }), false);
  assert.equal(isRuntimeStatusResult(null), false);
  assert.equal(isRuntimeStatusResult('connected'), false);
});

// --- isDeviceListResult ----------------------------------------------------------------

void test('isDeviceListResult accepts a well-formed empty and non-empty success result', () => {
  const empty: DeviceListResult = { ok: true, data: [] };
  assert.equal(isDeviceListResult(empty), true);
  const nonEmpty = { ok: true, data: [{ deviceId: 'dev-1', lifecycleState: 'discovered' }] };
  assert.equal(isDeviceListResult(nonEmpty), true);
});

void test('isDeviceListResult accepts a well-formed failure result', () => {
  assert.equal(isDeviceListResult(buildFailure(VALID_ERROR)), true);
});

void test('isDeviceListResult rejects a non-array data field', () => {
  assert.equal(
    isDeviceListResult({ ok: true, data: { deviceId: 'dev-1', lifecycleState: 'discovered' } }),
    false,
  );
});

void test('isDeviceListResult rejects an item with an unknown lifecycle state', () => {
  assert.equal(
    isDeviceListResult({ ok: true, data: [{ deviceId: 'dev-1', lifecycleState: 'exploded' }] }),
    false,
  );
});

void test('isDeviceListResult rejects an item with a malformed deviceId', () => {
  assert.equal(
    isDeviceListResult({
      ok: true,
      data: [{ deviceId: 'dev with spaces', lifecycleState: 'discovered' }],
    }),
    false,
  );
  assert.equal(isDeviceListResult({ ok: true, data: [{ lifecycleState: 'discovered' }] }), false);
});

void test('isDeviceListResult rejects one bad item among otherwise-good ones (no partial pass)', () => {
  const mixed = {
    ok: true,
    data: [
      { deviceId: 'dev-1', lifecycleState: 'discovered' },
      { deviceId: 'dev-2', lifecycleState: 'not-a-real-state' },
    ],
  };
  assert.equal(isDeviceListResult(mixed), false);
});
