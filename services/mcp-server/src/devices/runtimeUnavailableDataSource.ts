import type {
  DataSourceOutcome,
  DeviceDataSource,
  DeviceHealthSnapshot,
  DeviceListPage,
} from './types.js';
import { buildRuntimeUnavailableError } from './errors.js';

/**
 * The only production `DeviceDataSource` implementation this node ships.
 *
 * `Docs/graph/tasks/MCP-002.md` Section 3 directs this node to "be honest
 * about how it actually reaches [CON-002's registry] data: either a real IPC
 * call through RUN-002's protocol (preferred ...) or, if no runtime
 * process/consumer exists yet to answer such a call, an honestly-reported
 * `runtime_unavailable` result — the same discipline DSK-002's IPC bridge
 * already established." Confirmed this milestone (see the worker handoff,
 * `Docs/graph/handoffs/MCP-002-1.md`, for the full evidence trail) that the
 * preferred real path does not exist yet:
 *
 * - `RUN-003` (the background host process that would embed
 *   `crates/loki-connector-registry` and own a `crates/loki-ipc` server
 *   endpoint) is `proposed`/in progress, not `accepted`
 *   (`Docs/graph/TASK_GRAPH.md`).
 * - `crates/loki-ipc` (`RUN-002`, accepted) defines a transport, handshake,
 *   and deadline/cancellation/streaming primitives, but no device or health
 *   RPC method — its own task packet scopes it to "no business logic ...
 *   just the transport + handshake + a minimal echo/ping method."
 * - `crates/loki-connector-registry` (`CON-002`, accepted) has no
 *   `crates/loki-ipc` dependency at all (by its own explicit, documented
 *   design choice — see its worker handoff Section 7, decision 2) and is a
 *   Rust-only, in-process library; nothing exposes it over IPC to any
 *   process, Node.js or otherwise.
 * - `crates/loki-ipc`'s client (`src/client.rs`) is itself a Rust API; no
 *   Node/TypeScript client for its wire protocol exists.
 *
 * So there is currently no process boundary this Node.js MCP server could
 * even open a connection across to reach real device data, independent of
 * whether this node were scoped to build one (it is explicitly not,
 * `Docs/graph/tasks/MCP-002.md` Section 2: "Do not touch `crates/loki-ipc/**`
 * ... this node consumes both, it does not modify either"). This is
 * therefore the honest, permitted resolution the task packet's own Section 3
 * anticipated, not a workaround: every call reports `runtime_unavailable`,
 * never a fabricated device.
 *
 * The `DeviceDataSource` interface itself (`src/devices/types.ts`) is real
 * and load-bearing — `src/tools/listDevices.ts`/`getDeviceHealth.ts` consume
 * it exactly as they would a real backend, so a future node (once `RUN-003`
 * exists and a device/health RPC method is added to `crates/loki-ipc`) only
 * needs to swap this implementation for a real IPC-backed one at the single
 * call site in `src/tools/index.ts` — no tool-handling logic changes.
 */
export function createRuntimeUnavailableDeviceDataSource(): DeviceDataSource {
  return {
    listDevices(): Promise<DataSourceOutcome<DeviceListPage>> {
      return Promise.resolve({
        available: false,
        error: buildRuntimeUnavailableError('loki_list_devices'),
      });
    },
    getDeviceHealth(): Promise<DataSourceOutcome<DeviceHealthSnapshot | null>> {
      return Promise.resolve({
        available: false,
        error: buildRuntimeUnavailableError('loki_get_device_health'),
      });
    },
  };
}
