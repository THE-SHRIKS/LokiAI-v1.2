import { describe, expect, it } from 'vitest';
import { createStructuredLogger, type LogSink } from '../src/logging.js';
import { CorrelationIdSchema, LokiErrorSchema } from '@lokiai/contracts';

function fakeSink(): { sink: LogSink; lines: string[] } {
  const lines: string[] = [];
  return { sink: { write: (line) => lines.push(line) }, lines };
}

/**
 * `Docs/graph/tasks/MCP-001.md` Deliverable 3: "structured diagnostics/
 * logging." `Docs/MCP_SPEC.md` Section 2 / the official SDK's stdio guide:
 * stdout is the strict JSON-RPC channel — a host parses every line of it as
 * a protocol message, so diagnostics must never appear there. This module
 * never references `process.stdout` or `console.*`; these tests assert the
 * shape written to an injected sink, not real process streams.
 */
describe('createStructuredLogger', () => {
  it('writes one JSON object per line with the fixed envelope fields', () => {
    const { sink, lines } = fakeSink();
    const logger = createStructuredLogger('mcp-server', sink);

    logger.info('server started');

    expect(lines).toHaveLength(1);
    expect(lines[0]?.endsWith('\n')).toBe(true);
    // JSON.parse returns `any` with no runtime guarantee — acceptable here
    // since this test parses its own known-shape fixture output (the
    // logger's own emitted line), not external/untrusted input that would
    // need a real runtime validator (contrast packages/contracts' zod
    // schemas, which exist precisely for boundaries that do).
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    const record: Record<string, unknown> = JSON.parse(lines[0]);
    expect(record.level).toBe('info');
    expect(record.component).toBe('mcp-server');
    expect(record.message).toBe('server started');
    expect(typeof record.timestamp).toBe('string');
    // UTC ISO 8601 (AGENTS.md rule 5): must parse back to the same instant.
    expect(new Date(record.timestamp as string).toISOString()).toBeTruthy();
    expect((record.timestamp as string).endsWith('Z')).toBe(true);
  });

  it('supports debug/warn/error levels and merges caller-supplied fields', () => {
    const { sink, lines } = fakeSink();
    const logger = createStructuredLogger('mcp-server', sink);

    logger.debug('debug line', { extra: 1 });
    logger.warn('warn line', { extra: 2 });
    logger.error('error line', { extra: 3 });

    const records = lines.map((line) => JSON.parse(line) as Record<string, unknown>);
    expect(records.map((record) => record.level)).toEqual(['debug', 'warn', 'error']);
    expect(records.map((record) => record.extra)).toEqual([1, 2, 3]);
  });

  it('accepts a correlation_id and a LokiError-shaped error field and both remain intact in the emitted record', () => {
    const { sink, lines } = fakeSink();
    const logger = createStructuredLogger('mcp-server', sink);
    const correlationId = CorrelationIdSchema.parse('corr-test-1');
    const error = LokiErrorSchema.parse({
      code: 'version_incompatible',
      category: 'mcp_protocol',
      message: 'Unsupported protocol version',
      retryable: false,
      correlation_id: correlationId,
    });

    logger.error('mcp connection error', { correlation_id: correlationId, error });

    // See the fixture-output note above.
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    const record: { correlation_id: string; error: unknown } = JSON.parse(lines[0]);
    expect(record.correlation_id).toBe(correlationId);
    expect(LokiErrorSchema.safeParse(record.error).success).toBe(true);
  });

  it('never writes to process.stdout', () => {
    // `.bind()`, not a bare method reference: a detached
    // `process.stdout.write` could lose its `this`, and binding it here
    // also satisfies the fact this is stored/restored, never called
    // directly by this test.
    const stdoutWrite = process.stdout.write.bind(process.stdout);
    const stdoutCalls: unknown[] = [];
    process.stdout.write = (chunk: unknown) => {
      stdoutCalls.push(chunk);
      return true;
    };
    try {
      const { sink } = fakeSink();
      const logger = createStructuredLogger('mcp-server', sink);
      logger.info('should not touch stdout');
      logger.error('nor should this');
    } finally {
      process.stdout.write = stdoutWrite;
    }
    expect(stdoutCalls).toHaveLength(0);
  });
});
