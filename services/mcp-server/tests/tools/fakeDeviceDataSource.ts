import type { DeviceId } from '@lokiai/contracts';
import type {
  DataSourceOutcome,
  DeviceDataSource,
  DeviceHealthSnapshot,
  DeviceListPage,
  DeviceSummary,
  GetDeviceHealthParams,
  ListDevicesParams,
} from '../../src/devices/types.js';

/**
 * A real, non-mocked `DeviceDataSource` implementation used only by tests —
 * exercises the exact interface `src/devices/runtimeUnavailableDataSource.ts`
 * implements in production, so `src/tools/listDevices.ts`/`getDeviceHealth.ts`'s
 * pagination/scope-filtering/size-bound logic can be proven against real
 * (if synthetic) data, per `Docs/graph/tasks/MCP-002.md` Deliverable 6's
 * pagination/response-size/auth-scope test requirements — none of which are
 * reachable against the always-unavailable production data source alone.
 */
export interface FakeDeviceDataSourceOptions {
  readonly devices?: readonly DeviceSummary[];
  readonly health?: ReadonlyMap<string, DeviceHealthSnapshot>;
  readonly source?: string;
  readonly observedAt?: string;
}

export function createFakeDeviceDataSource(
  options: FakeDeviceDataSourceOptions = {},
): DeviceDataSource {
  const devices = options.devices ?? [];
  const health = options.health ?? new Map<string, DeviceHealthSnapshot>();
  const source = options.source ?? 'fake-device-data-source';
  const observedAt = options.observedAt ?? '2026-08-18T00:00:00.000Z';

  return {
    listDevices(params: ListDevicesParams): Promise<DataSourceOutcome<DeviceListPage>> {
      const authorized = devices.filter((device) => params.isDeviceAuthorized(device.device_id));
      const page = authorized.slice(params.offset, params.offset + params.limit);
      return Promise.resolve({
        available: true,
        data: { devices: page, totalAuthorizedCount: authorized.length },
        source,
        observedAt,
      });
    },
    getDeviceHealth(
      deviceId: DeviceId,
      _params: GetDeviceHealthParams,
    ): Promise<DataSourceOutcome<DeviceHealthSnapshot | null>> {
      const snapshot = health.get(deviceId) ?? null;
      return Promise.resolve({ available: true, data: snapshot, source, observedAt });
    },
  };
}

/** Builds `count` synthetic health evidence entries — for size-bound/truncation tests. */
export function buildHealthEntries(count: number): DeviceHealthSnapshot['entries'] {
  return Array.from({ length: count }, (_, i) => ({
    check_id: `check.${i}`,
    check_version: '1',
    category: 'diagnostic',
    status: 'passed' as const,
    observed_at: '2026-08-18T00:00:00.000Z',
    received_at: '2026-08-18T00:00:00.000Z',
  }));
}
