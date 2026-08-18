import type { McpServer } from '@modelcontextprotocol/server';
import { createRuntimeUnavailableDeviceDataSource } from '../devices/runtimeUnavailableDataSource.js';
import { createDenyAllDeviceScope } from '../devices/scope.js';
import type { DeviceDataSource } from '../devices/types.js';
import type { DeviceAccessScope } from '../devices/scope.js';
import { registerListDevicesTool } from './listDevices.js';
import { registerGetDeviceHealthTool } from './getDeviceHealth.js';

export interface RegisterLokiToolsDeps {
  /** Defaults to `createRuntimeUnavailableDeviceDataSource()` — see that module's doc comment for why. */
  readonly dataSource?: DeviceDataSource;
  /** Defaults to `createDenyAllDeviceScope()` — see `src/devices/scope.ts`'s doc comment for why. */
  readonly scope?: DeviceAccessScope;
}

/**
 * Registers every MCP-002 tool on a freshly constructed `McpServer`
 * (`src/server.ts`'s `lokiMcpServerFactory`). Deliberately accepts injectable
 * `deps` (defaulted, not hardcoded) so a future node that adds a real
 * IPC-backed `DeviceDataSource`/`DeviceAccessScope` only needs to change this
 * one call site — no tool-handling logic changes — and so this package's own
 * tests can exercise the tools against fakes without a real backend.
 */
export function registerLokiTools(server: McpServer, deps: RegisterLokiToolsDeps = {}): void {
  const dataSource = deps.dataSource ?? createRuntimeUnavailableDeviceDataSource();
  const scope = deps.scope ?? createDenyAllDeviceScope();
  registerListDevicesTool(server, { dataSource, scope });
  registerGetDeviceHealthTool(server, { dataSource, scope });
}

export {
  LOKI_LIST_DEVICES_TOOL_NAME,
  listDevicesHandler,
  registerListDevicesTool,
} from './listDevices.js';
export {
  LOKI_GET_DEVICE_HEALTH_TOOL_NAME,
  getDeviceHealthHandler,
  registerGetDeviceHealthTool,
} from './getDeviceHealth.js';
