import { describe, expect, it } from 'vitest';
import { DeviceIdSchema } from '@lokiai/contracts';
import {
  createAllowAllDeviceScope,
  createAllowListDeviceScope,
  createDenyAllDeviceScope,
} from '../../src/devices/scope.js';

const deviceA = DeviceIdSchema.parse('device-a');
const deviceB = DeviceIdSchema.parse('device-b');

/**
 * `Docs/graph/tasks/MCP-002.md` Deliverable 1/6: the honest deny-by-default
 * scope (no real `AUTH-001`/workspace primitive exists yet), and the
 * filtering *code path* itself (exercised here against an allowlist, since
 * production always uses the deny-all default).
 */
describe('createDenyAllDeviceScope', () => {
  it('authorizes no device at all — the honest default given no real auth primitive exists', () => {
    const scope = createDenyAllDeviceScope();
    expect(scope.isDeviceAuthorized(deviceA)).toBe(false);
    expect(scope.isDeviceAuthorized(deviceB)).toBe(false);
  });
});

describe('createAllowListDeviceScope', () => {
  it('authorizes only the devices in its list', () => {
    const scope = createAllowListDeviceScope([deviceA]);
    expect(scope.isDeviceAuthorized(deviceA)).toBe(true);
    expect(scope.isDeviceAuthorized(deviceB)).toBe(false);
  });

  it('authorizes nothing for an empty list', () => {
    const scope = createAllowListDeviceScope([]);
    expect(scope.isDeviceAuthorized(deviceA)).toBe(false);
  });
});

describe('createAllowAllDeviceScope', () => {
  it('authorizes every device', () => {
    const scope = createAllowAllDeviceScope();
    expect(scope.isDeviceAuthorized(deviceA)).toBe(true);
    expect(scope.isDeviceAuthorized(deviceB)).toBe(true);
  });
});
