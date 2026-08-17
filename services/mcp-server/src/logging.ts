import { IsoUtcTimestampSchema, type CorrelationId, type LokiError } from '@lokiai/contracts';

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

/** Fields a call site may attach to one structured log line, beyond the fixed envelope this module always fills in. */
export interface LogFields {
  correlation_id?: CorrelationId;
  error?: LokiError;
  [extra: string]: unknown;
}

export interface StructuredLogRecord extends LogFields {
  timestamp: string;
  level: LogLevel;
  component: string;
  message: string;
}

export interface StructuredLogger {
  debug(message: string, fields?: LogFields): void;
  info(message: string, fields?: LogFields): void;
  warn(message: string, fields?: LogFields): void;
  error(message: string, fields?: LogFields): void;
}

/**
 * A single writable-line sink. Defaults to real `process.stderr`, but is
 * always injectable so tests can assert on exactly what would have been
 * written without capturing the real process stream.
 */
export interface LogSink {
  write(line: string): void;
}

const stderrSink: LogSink = {
  write: (line) => {
    process.stderr.write(line);
  },
};

/**
 * Builds a structured (one-JSON-object-per-line) logger for stderr.
 *
 * `Docs/MCP_SPEC.md` Section 2 and the official SDK's own stdio guide both
 * require this: stdout is the strict JSON-RPC channel a host parses
 * line-by-line, so every diagnostic — startup, connection errors, protocol
 * violations — MUST go to stderr instead, never stdout. This is the only
 * writer this module (or anything in this package) uses; nothing here ever
 * touches `process.stdout` or `console.log`.
 *
 * Every record includes a UTC ISO 8601 timestamp (`AGENTS.md` rule 5) and,
 * when the caller supplies one, the request/connection `correlation_id` and
 * a `Docs/SPEC.md` Section 1 `LokiError`-shaped `error` field (see
 * `errorLog.ts`) — `Docs/graph/tasks/MCP-001.md` Deliverable 3's "structured
 * error responses (using `@lokiai/contracts`' error shapes where applicable)
 * and structured diagnostics/logging."
 */
export function createStructuredLogger(
  component: string,
  sink: LogSink = stderrSink,
): StructuredLogger {
  const emit = (level: LogLevel, message: string, fields?: LogFields): void => {
    const record: StructuredLogRecord = {
      timestamp: IsoUtcTimestampSchema.parse(new Date().toISOString()),
      level,
      component,
      message,
      ...fields,
    };
    sink.write(`${JSON.stringify(record)}\n`);
  };

  return {
    debug: (message, fields) => emit('debug', message, fields),
    info: (message, fields) => emit('info', message, fields),
    warn: (message, fields) => emit('warn', message, fields),
    error: (message, fields) => emit('error', message, fields),
  };
}
