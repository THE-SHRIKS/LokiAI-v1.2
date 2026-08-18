import type { DeviceId } from '@lokiai/contracts';

/**
 * `Docs/graph/tasks/MCP-002.md` Deliverable 1: "returns only devices the
 * connected client is authorized to see (workspace/scope boundary — if no
 * real auth/workspace scoping exists yet at this milestone per the
 * OQ-002/AUTH-001 blocker, this must be explicitly and honestly reported as
 * an open scope gap, not silently bypassed by returning everything
 * unscoped)."
 *
 * `AUTH-001` (browser authentication, workspace/session identity) is
 * `blocked` (`Docs/graph/TASK_GRAPH.md`, `OQ-002`) as of this node. Nothing
 * in this MCP server's current connection model (`MCP-001`: stateless stdio,
 * "no auth/session state" per its own scope note in `src/server.ts`) carries
 * a real authenticated workspace or device-ownership identity to check
 * against. Given that gap, this module's default is deliberately
 * deny-by-default — `createDenyAllDeviceScope()` — rather than allow-all:
 * the honest position when authorization cannot be proven is "authorized for
 * nothing," not "authorized for everything." This is what makes
 * Deliverable 6's acceptance test true today ("a client without an
 * authorized device sees nothing for it") even though the *reason* is a
 * documented stub, not a real per-client policy decision yet.
 *
 * `DeviceAccessScope` itself is the real, load-bearing code path
 * (`src/tools/listDevices.ts`/`getDeviceHealth.ts` both call it on every
 * request) — a future `AUTH-001`/`ENR-001`-backed implementation replaces
 * only the *provider* (`createDenyAllDeviceScope`'s call site in
 * `src/tools/index.ts`), not the interface or the tools that consume it.
 */
export interface DeviceAccessScope {
  isDeviceAuthorized(deviceId: DeviceId): boolean;
}

/**
 * The honest default given no real authentication/workspace-scoping
 * primitive exists yet (see module doc above). Every device is denied, not
 * because every device is actually unauthorized in some real policy sense,
 * but because this server has no way to prove otherwise today.
 */
export function createDenyAllDeviceScope(): DeviceAccessScope {
  return {
    isDeviceAuthorized: () => false,
  };
}

/**
 * A fixed-allowlist scope, for tests (and for a future real backend that
 * resolves a concrete authorized-device set per connection/workspace) to
 * exercise the *filtering* code path against something other than the
 * deny-all default.
 */
export function createAllowListDeviceScope(
  authorizedDeviceIds: readonly DeviceId[],
): DeviceAccessScope {
  const authorized = new Set<string>(authorizedDeviceIds);
  return {
    isDeviceAuthorized: (deviceId) => authorized.has(deviceId),
  };
}

/** Authorizes every device — for tests that need to observe unfiltered data-source output. */
export function createAllowAllDeviceScope(): DeviceAccessScope {
  return {
    isDeviceAuthorized: () => true,
  };
}
