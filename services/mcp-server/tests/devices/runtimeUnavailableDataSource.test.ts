import { describe, expect, it } from 'vitest';
import { DeviceIdSchema } from '@lokiai/contracts';
import { createRuntimeUnavailableDeviceDataSource } from '../../src/devices/runtimeUnavailableDataSource.js';

/**
 * `Docs/graph/tasks/MCP-002.md` Deliverable 5: "Truthful 'no runtime/no
 * registry available' results distinct from 'zero devices registered' — no
 * state silently defaults to an empty-but-successful response when the
 * underlying call actually failed/is unavailable." This is the only
 * production `DeviceDataSource` this node ships (see its own module doc for
 * the full evidence trail on why no real backend is reachable yet); it must
 * never look like a successful empty list.
 */
describe('createRuntimeUnavailableDeviceDataSource', () => {
  it('listDevices reports unavailable, never an empty-but-successful list', async () => {
    const dataSource = createRuntimeUnavailableDeviceDataSource();
    const outcome = await dataSource.listDevices({
      limit: 10,
      offset: 0,
      isDeviceAuthorized: () => true,
    });

    expect(outcome.available).toBe(false);
    if (!outcome.available) {
      expect(outcome.error.code).toBe('runtime_unavailable');
      expect(outcome.error.retryable).toBe(true);
      expect(outcome.error.correlation_id).toBeTruthy();
    }
  });

  it('getDeviceHealth reports unavailable, never a fabricated or empty snapshot', async () => {
    const dataSource = createRuntimeUnavailableDeviceDataSource();
    const outcome = await dataSource.getDeviceHealth(DeviceIdSchema.parse('device-a'), {
      maxEntries: 20,
    });

    expect(outcome.available).toBe(false);
    if (!outcome.available) {
      expect(outcome.error.code).toBe('runtime_unavailable');
      expect(outcome.error.correlation_id).toBeTruthy();
    }
  });

  it('two calls mint two distinct correlation IDs (never reused/cached)', async () => {
    const dataSource = createRuntimeUnavailableDeviceDataSource();
    const first = await dataSource.listDevices({
      limit: 1,
      offset: 0,
      isDeviceAuthorized: () => true,
    });
    const second = await dataSource.listDevices({
      limit: 1,
      offset: 0,
      isDeviceAuthorized: () => true,
    });
    if (!first.available && !second.available) {
      expect(first.error.correlation_id).not.toBe(second.error.correlation_id);
    } else {
      throw new Error('expected both outcomes to be unavailable');
    }
  });
});
