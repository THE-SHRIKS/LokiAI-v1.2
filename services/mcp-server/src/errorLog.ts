import { LokiErrorSchema, type LokiError, type CorrelationId } from '@lokiai/contracts';

/**
 * Maps a caught error into a `Docs/SPEC.md` Section 1 `LokiError`-shaped
 * structured diagnostic record for logging (`Docs/graph/tasks/MCP-001.md`
 * Deliverable 3: "Structured error responses (using `@lokiai/contracts`'
 * error shapes where applicable)").
 *
 * This is diagnostics only, never the wire response: the actual JSON-RPC
 * error a malformed/unsupported request receives is built and sent entirely
 * by the official SDK (`@modelcontextprotocol/server`'s `ProtocolError`/
 * `serveStdio` machinery) — `Docs/MCP_SPEC.md` Section 9 requires the
 * previous-version adapter and error handling to never "weaken current
 * schemas or policy," and this repo depends on the official SDK rather than
 * reimplementing protocol framing. See `Docs/graph/handoffs/MCP-001-1.md`
 * for the exact SDK error shapes this was verified against.
 */

/**
 * `@modelcontextprotocol/server`'s `ProtocolError` (and subclasses such as
 * `UnsupportedProtocolVersionError`): a real JSON-RPC wire error, numeric
 * `code`. Duck-typed rather than `instanceof`-checked so this module never
 * needs its own copy of the SDK's error classes.
 */
interface WireProtocolErrorShape {
  code: number;
  message: string;
  data?: unknown;
}

/**
 * `@modelcontextprotocol/server`'s `SdkError`: "local errors that never
 * cross the wire" per the SDK's own doc comment — string `code`, no numeric
 * JSON-RPC code exists for it. Duck-typed for the same reason as above.
 */
interface SdkLocalErrorShape {
  code: string;
  message: string;
  data?: unknown;
}

function isWireProtocolErrorShape(value: unknown): value is WireProtocolErrorShape {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { code?: unknown }).code === 'number' &&
    typeof (value as { message?: unknown }).message === 'string'
  );
}

function isSdkLocalErrorShape(value: unknown): value is SdkLocalErrorShape {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { code?: unknown }).code === 'string' &&
    typeof (value as { message?: unknown }).message === 'string'
  );
}

/**
 * The official SDK's `ProtocolErrorCode` enum (JSON-RPC + MCP-specific wire
 * codes), mapped to `LokiErrorSchema`'s open `code` vocabulary.
 * `Docs/MCP_SPEC.md` Section 7's list ("authentication required, ...,
 * version incompatible, ...") is explicitly illustrative/non-exhaustive
 * ("such as"), so codes without an obvious match there use a
 * `mcp_`-prefixed slug instead of forcing an inexact fit onto a documented
 * one. `version_incompatible` and `internal_correlation_error` below are the
 * two genuine matches to that list.
 */
const WIRE_ERROR_CODE_SLUGS: Record<number, string> = {
  [-32700]: 'mcp_parse_error',
  [-32600]: 'mcp_invalid_request',
  [-32601]: 'mcp_method_not_found',
  [-32602]: 'invalid_request_envelope',
  [-32603]: 'internal_correlation_error',
  [-32002]: 'mcp_resource_not_found',
  [-32021]: 'mcp_missing_client_capability',
  [-32022]: 'version_incompatible',
  [-32042]: 'mcp_url_elicitation_required',
};

const DETAILS_MAX_ENTRIES = 20;

/** Bounds an SDK error's `data` payload before it is logged, per `AGENTS.md` rule 9 ("redact sensitive values from logs and evidence") and `Docs/MCP_SPEC.md` Section 9 ("Tool requests and results are auditable and redact secrets"). The SDK's own error `data` is protocol metadata (supported/requested versions, offending `_meta` key) — never a secret — but is still bounded defensively since it originates from an untrusted request. */
function boundedDetails(data: unknown): Record<string, unknown> {
  if (data === undefined) {
    return {};
  }
  if (data === null || typeof data !== 'object' || Array.isArray(data)) {
    return { value: data };
  }
  return Object.fromEntries(
    Object.entries(data as Record<string, unknown>).slice(0, DETAILS_MAX_ENTRIES),
  );
}

export function toDiagnosticLokiError(error: unknown, correlationId: CorrelationId): LokiError {
  if (isWireProtocolErrorShape(error)) {
    const code = WIRE_ERROR_CODE_SLUGS[error.code] ?? 'mcp_protocol_error';
    return LokiErrorSchema.parse({
      code,
      category: 'mcp_protocol',
      message: error.message.slice(0, 2000) || 'MCP protocol error',
      retryable: false,
      details: { ...boundedDetails(error.data), jsonrpc_code: error.code },
      correlation_id: correlationId,
    });
  }

  if (isSdkLocalErrorShape(error)) {
    return LokiErrorSchema.parse({
      code: `mcp_sdk_${error.code.toLowerCase()}`,
      category: 'mcp_sdk',
      message: error.message.slice(0, 2000) || 'MCP SDK error',
      retryable: false,
      details: boundedDetails(error.data),
      correlation_id: correlationId,
    });
  }

  const message = error instanceof Error ? error.message : String(error);
  return LokiErrorSchema.parse({
    code: 'internal_correlation_error',
    category: 'internal',
    message: message.slice(0, 2000) || 'Unknown error',
    retryable: false,
    correlation_id: correlationId,
  });
}
