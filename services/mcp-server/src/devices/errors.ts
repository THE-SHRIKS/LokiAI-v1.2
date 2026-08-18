import { LokiErrorSchema, type DeviceId, type LokiError } from '@lokiai/contracts';
import { newCorrelationId } from '../correlationId.js';

/**
 * Shared `LokiError` builders for the device tools
 * (`src/tools/listDevices.ts`, `src/tools/getDeviceHealth.ts`). Every error
 * uses one of `packages/contracts/src/errors.ts`'s `KNOWN_ERROR_CODES`
 * (`Docs/MCP_SPEC.md` Section 7's own illustrative code list) and mints a
 * fresh, real, validated correlation ID per `AGENTS.md` rule 5 — the same
 * pattern `electron/main/ipc.cts`'s `buildRuntimeUnavailableError`/
 * `buildPermissionDeniedError`/`buildInvalidRequestError` already
 * established for the identical honesty requirement.
 */

/**
 * No real IPC path reaches device/registry data at this milestone:
 * `RUN-003` (the background host process that would embed
 * `loki-connector-registry` and answer `crates/loki-ipc` calls) is not
 * accepted yet (`Docs/graph/TASK_GRAPH.md`), and `crates/loki-ipc` itself
 * (`RUN-002`) defines no device/health RPC method beyond its own transport
 * proof-of-life echo/ping — only a future node adds one. See
 * `src/devices/runtimeUnavailableDataSource.ts` for where this is used and
 * the worker handoff (`Docs/graph/handoffs/MCP-002-1.md`) for the full
 * trace of what specifically is missing.
 */
export function buildRuntimeUnavailableError(source: string): LokiError {
  return LokiErrorSchema.parse({
    code: 'runtime_unavailable',
    category: 'transport',
    message: `LokiAI runtime local IPC (RUN-002) has no background host process (RUN-003) or device/health RPC method yet; ${source} cannot reach real device data.`,
    retryable: true,
    correlation_id: newCorrelationId(),
  });
}

/**
 * `Docs/MCP_SPEC.md` Section 11: "Unauthorized workspace/device access is
 * rejected without data leakage." Returned for both "device exists but is
 * unauthorized" and (implicitly, since this check runs before any
 * data-source call) "device may not even exist" — the caller cannot
 * distinguish the two from this error alone, which is the point.
 */
export function buildDeviceUnauthorizedError(source: string, deviceId: DeviceId): LokiError {
  return LokiErrorSchema.parse({
    code: 'permission_denied',
    category: 'mcp_tool_authorization',
    message: `${source}: the connected client is not authorized to read device ${deviceId}.`,
    retryable: false,
    correlation_id: newCorrelationId(),
  });
}

/**
 * The data source was reachable and answered, but no such device exists —
 * distinct from `runtime_unavailable` (the query could not be made at all).
 * See `DeviceDataSource.getDeviceHealth`'s doc comment
 * (`src/devices/types.ts`) for why the two are never conflated.
 */
export function buildDeviceUnavailableError(source: string, deviceId: DeviceId): LokiError {
  return LokiErrorSchema.parse({
    code: 'device_unavailable',
    category: 'mcp_tool_input',
    message: `${source}: device ${deviceId} was not found.`,
    retryable: false,
    correlation_id: newCorrelationId(),
  });
}

/**
 * A cursor that is well-formed per `CursorSchema`'s wire shape (so the SDK's
 * own zod input validation accepts it) but is not a token this server
 * actually issued — the domain-level malformed-input case
 * `Docs/graph/tasks/MCP-002.md` Deliverable 6 names alongside the
 * protocol-level ones the SDK already rejects.
 */
export function buildInvalidCursorError(source: string): LokiError {
  return LokiErrorSchema.parse({
    code: 'invalid_request',
    category: 'mcp_tool_input',
    message: `${source}: cursor is not a recognizable page token issued by this server.`,
    retryable: false,
    correlation_id: newCorrelationId(),
  });
}

/**
 * A `DeviceDataSource`/internal invariant produced a result this module's
 * own schema does not accept — mirrors `electron/main/ipc.cts`'s
 * `buildInternalError` for the same "never trust your own internals blindly"
 * defense-in-depth pattern.
 */
export function buildInternalError(source: string, detail: string): LokiError {
  return LokiErrorSchema.parse({
    code: 'internal_correlation_error',
    category: 'mcp_tool_internal',
    message: `${source}: ${detail}`,
    retryable: true,
    correlation_id: newCorrelationId(),
  });
}
