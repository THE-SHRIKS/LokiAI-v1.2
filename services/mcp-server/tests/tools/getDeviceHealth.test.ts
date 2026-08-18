import { describe, expect, it } from 'vitest';
import { DeviceIdSchema } from '@lokiai/contracts';
import { getDeviceHealthHandler } from '../../src/tools/getDeviceHealth.js';
import { createRuntimeUnavailableDeviceDataSource } from '../../src/devices/runtimeUnavailableDataSource.js';
import { createAllowAllDeviceScope, createDenyAllDeviceScope } from '../../src/devices/scope.js';
import { buildHealthEntries, createFakeDeviceDataSource } from './fakeDeviceDataSource.js';
import type { DeviceHealthSnapshot } from '../../src/devices/types.js';

const deviceA = DeviceIdSchema.parse('device-a');

/**
 * `Docs/graph/tasks/MCP-002.md` Deliverable 2/4/6: bounded health evidence
 * with correlation and freshness, response-size test (a large evidence set
 * is bounded not dumped), auth/scope test, and honest unavailable-vs-
 * not-found distinction.
 */
describe('loki_get_device_health handler', () => {
  it('reports permission_denied for an unauthorized device without ever calling the data source', async () => {
    let dataSourceCalled = false;
    const dataSource = createFakeDeviceDataSource();
    const spiedDataSource = {
      ...dataSource,
      getDeviceHealth: (...args: Parameters<typeof dataSource.getDeviceHealth>) => {
        dataSourceCalled = true;
        return dataSource.getDeviceHealth(...args);
      },
    };

    const result = await getDeviceHealthHandler(
      { device_id: deviceA },
      { dataSource: spiedDataSource, scope: createDenyAllDeviceScope() },
    );

    expect(result.isError).toBe(true);
    const structured = result.structuredContent as { code: string };
    expect(structured.code).toBe('permission_denied');
    expect(dataSourceCalled).toBe(false);
  });

  it('reports an honest runtime_unavailable error for an authorized device (production default)', async () => {
    const result = await getDeviceHealthHandler(
      { device_id: deviceA },
      {
        dataSource: createRuntimeUnavailableDeviceDataSource(),
        scope: createAllowAllDeviceScope(),
      },
    );
    expect(result.isError).toBe(true);
    const structured = result.structuredContent as { code: string };
    expect(structured.code).toBe('runtime_unavailable');
  });

  it('reports device_unavailable (not found) distinctly from runtime_unavailable when the query succeeds but the device does not exist', async () => {
    const dataSource = createFakeDeviceDataSource({ health: new Map() });
    const result = await getDeviceHealthHandler(
      { device_id: deviceA },
      { dataSource, scope: createAllowAllDeviceScope() },
    );
    expect(result.isError).toBe(true);
    const structured = result.structuredContent as { code: string };
    expect(structured.code).toBe('device_unavailable');
  });

  it('bounds a large evidence set instead of dumping it whole, and reports truncated: true honestly', async () => {
    const snapshot: DeviceHealthSnapshot = {
      device_id: deviceA,
      aggregated_status: 'healthy',
      checked_at: '2026-08-18T00:00:00.000Z',
      entries: buildHealthEntries(500),
      truncated: false,
      total_entry_count: 500,
    };
    const dataSource = createFakeDeviceDataSource({ health: new Map([[deviceA, snapshot]]) });

    const result = await getDeviceHealthHandler(
      { device_id: deviceA, max_entries: 20 },
      { dataSource, scope: createAllowAllDeviceScope() },
    );

    expect(result.isError).toBeFalsy();
    const data = result.structuredContent as {
      data: DeviceHealthSnapshot;
    };
    expect(data.data.entries).toHaveLength(20);
    expect(data.data.truncated).toBe(true);
    expect(data.data.total_entry_count).toBe(500);
  });

  it('reports truncated: false when every entry fits within max_entries', async () => {
    const snapshot: DeviceHealthSnapshot = {
      device_id: deviceA,
      aggregated_status: 'healthy',
      checked_at: '2026-08-18T00:00:00.000Z',
      entries: buildHealthEntries(3),
      truncated: false,
      total_entry_count: 3,
    };
    const dataSource = createFakeDeviceDataSource({ health: new Map([[deviceA, snapshot]]) });

    const result = await getDeviceHealthHandler(
      { device_id: deviceA, max_entries: 20 },
      { dataSource, scope: createAllowAllDeviceScope() },
    );

    const data = result.structuredContent as { data: DeviceHealthSnapshot };
    expect(data.data.entries).toHaveLength(3);
    expect(data.data.truncated).toBe(false);
  });

  it('never returns more entries than max_entries even against a huge evidence set (defense in depth)', async () => {
    const snapshot: DeviceHealthSnapshot = {
      device_id: deviceA,
      aggregated_status: 'degraded',
      checked_at: '2026-08-18T00:00:00.000Z',
      entries: buildHealthEntries(10_000),
      truncated: false,
      total_entry_count: 10_000,
    };
    const dataSource = createFakeDeviceDataSource({ health: new Map([[deviceA, snapshot]]) });

    const result = await getDeviceHealthHandler(
      { device_id: deviceA, max_entries: 5 },
      { dataSource, scope: createAllowAllDeviceScope() },
    );
    const data = result.structuredContent as { data: DeviceHealthSnapshot };
    expect(data.data.entries.length).toBeLessThanOrEqual(5);
  });

  it('a successful response carries a correlation ID and freshness', async () => {
    const snapshot: DeviceHealthSnapshot = {
      device_id: deviceA,
      aggregated_status: 'healthy',
      checked_at: new Date().toISOString(),
      entries: buildHealthEntries(1),
      truncated: false,
      total_entry_count: 1,
    };
    const dataSource = createFakeDeviceDataSource({
      health: new Map([[deviceA, snapshot]]),
      observedAt: new Date().toISOString(),
    });

    const result = await getDeviceHealthHandler(
      { device_id: deviceA },
      { dataSource, scope: createAllowAllDeviceScope() },
    );
    const data = result.structuredContent as { correlation_id: string; freshness: string };
    expect(typeof data.correlation_id).toBe('string');
    expect(data.correlation_id.length).toBeGreaterThan(0);
    expect(data.freshness).toBe('fresh');
  });
});
