import { describe, expect, it } from 'vitest';
import { DeviceIdSchema } from '@lokiai/contracts';
import { listDevicesHandler } from '../../src/tools/listDevices.js';
import { createRuntimeUnavailableDeviceDataSource } from '../../src/devices/runtimeUnavailableDataSource.js';
import {
  createAllowAllDeviceScope,
  createAllowListDeviceScope,
  createDenyAllDeviceScope,
} from '../../src/devices/scope.js';
import { createFakeDeviceDataSource } from './fakeDeviceDataSource.js';
import type { DeviceSummary } from '../../src/devices/types.js';

function device(id: string): DeviceSummary {
  return { device_id: DeviceIdSchema.parse(id), lifecycle_state: 'ready' };
}

/**
 * `Docs/graph/tasks/MCP-002.md` Deliverable 6: pagination test, auth/scope
 * test, response-size test, malformed-request tests, and the honest
 * "runtime unavailable" distinct-from-empty behavior.
 */
describe('loki_list_devices handler', () => {
  it('reports an honest runtime_unavailable error, never an empty-but-successful list (production default)', async () => {
    const result = await listDevicesHandler(
      {},
      { dataSource: createRuntimeUnavailableDeviceDataSource(), scope: createDenyAllDeviceScope() },
    );

    expect(result.isError).toBe(true);
    const structured = result.structuredContent as { code: string; data?: unknown };
    expect(structured.code).toBe('runtime_unavailable');
    expect(structured.data).toBeUndefined();
  });

  it('paginates: a bounded page size is honored and next_cursor walks through every page', async () => {
    const devices = [device('d1'), device('d2'), device('d3'), device('d4'), device('d5')];
    const dataSource = createFakeDeviceDataSource({ devices });
    const scope = createAllowAllDeviceScope();

    const page1 = await listDevicesHandler({ limit: 2 }, { dataSource, scope });
    expect(page1.isError).toBeFalsy();
    const page1Data = page1.structuredContent as { data: DeviceSummary[]; next_cursor?: string };
    expect(page1Data.data).toHaveLength(2);
    expect(page1Data.data.map((d) => d.device_id)).toEqual(['d1', 'd2']);
    expect(page1Data.next_cursor).toBeTruthy();

    const page2 = await listDevicesHandler(
      { limit: 2, cursor: page1Data.next_cursor },
      { dataSource, scope },
    );
    const page2Data = page2.structuredContent as { data: DeviceSummary[]; next_cursor?: string };
    expect(page2Data.data.map((d) => d.device_id)).toEqual(['d3', 'd4']);
    expect(page2Data.next_cursor).toBeTruthy();

    const page3 = await listDevicesHandler(
      { limit: 2, cursor: page2Data.next_cursor },
      { dataSource, scope },
    );
    const page3Data = page3.structuredContent as { data: DeviceSummary[]; next_cursor?: string };
    expect(page3Data.data.map((d) => d.device_id)).toEqual(['d5']);
    // No unbounded/never-ending pagination: the last page carries no next_cursor.
    expect(page3Data.next_cursor).toBeUndefined();
  });

  it('never returns more than the requested (bounded) limit even with many devices available', async () => {
    const devices = Array.from({ length: 500 }, (_, i) => device(`d${i}`));
    const dataSource = createFakeDeviceDataSource({ devices });
    const result = await listDevicesHandler(
      { limit: 10 },
      { dataSource, scope: createAllowAllDeviceScope() },
    );
    const data = result.structuredContent as { data: DeviceSummary[] };
    expect(data.data).toHaveLength(10);
  });

  it('a client without an authorized device sees nothing for it (scope filtering is real, not bypassed)', async () => {
    const devices = [device('authorized'), device('unauthorized')];
    const dataSource = createFakeDeviceDataSource({ devices });
    const scope = createAllowListDeviceScope([DeviceIdSchema.parse('authorized')]);

    const result = await listDevicesHandler({}, { dataSource, scope });
    const data = result.structuredContent as { data: DeviceSummary[] };
    expect(data.data.map((d) => d.device_id)).toEqual(['authorized']);
  });

  it('the deny-all default (no real auth primitive) returns zero devices even when the data source has some', async () => {
    const devices = [device('d1'), device('d2')];
    const dataSource = createFakeDeviceDataSource({ devices });
    const result = await listDevicesHandler({}, { dataSource, scope: createDenyAllDeviceScope() });
    const data = result.structuredContent as { data: DeviceSummary[] };
    expect(data.data).toEqual([]);
  });

  it('rejects a well-formed-but-unrecognized cursor as a domain-level malformed request, not silently as page 1', async () => {
    const devices = [device('d1')];
    const dataSource = createFakeDeviceDataSource({ devices });
    const result = await listDevicesHandler(
      { cursor: 'not-a-real-cursor-but-id-shaped' },
      { dataSource, scope: createAllowAllDeviceScope() },
    );
    expect(result.isError).toBe(true);
    const structured = result.structuredContent as { code: string };
    expect(structured.code).toBe('invalid_request');
  });

  it('every successful response carries a correlation ID and source/freshness fields', async () => {
    const dataSource = createFakeDeviceDataSource({
      devices: [device('d1')],
      observedAt: new Date().toISOString(),
    });
    const result = await listDevicesHandler({}, { dataSource, scope: createAllowAllDeviceScope() });
    const data = result.structuredContent as {
      correlation_id: string;
      source: string;
      freshness: string;
      observed_at: string;
    };
    expect(typeof data.correlation_id).toBe('string');
    expect(data.correlation_id.length).toBeGreaterThan(0);
    expect(data.source).toBeTruthy();
    expect(data.freshness).toBe('fresh');
  });
});
