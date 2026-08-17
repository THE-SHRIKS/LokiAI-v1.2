import { describe, expect, it } from 'vitest';
import { CorrelationIdSchema } from '@lokiai/contracts';
import { toDiagnosticLokiError } from '../src/errorLog.js';

const correlationId = CorrelationIdSchema.parse('corr-test-1');

/**
 * `toDiagnosticLokiError` maps a caught SDK/transport error into a real,
 * `LokiErrorSchema`-validated `LokiError` for structured logging
 * (`Docs/graph/tasks/MCP-001.md` Deliverable 3). Every case here is a real
 * shape the official SDK is verified to throw/return (see
 * `Docs/graph/handoffs/MCP-001-1.md`), not an invented one.
 */
describe('toDiagnosticLokiError', () => {
  it('maps a wire ProtocolError-shaped UnsupportedProtocolVersion (-32022) to the documented version_incompatible code', () => {
    const result = toDiagnosticLokiError(
      {
        code: -32022,
        message: 'Unsupported protocol version: 2099-01-01',
        data: { supported: ['2026-07-28'], requested: '2099-01-01' },
      },
      correlationId,
    );
    expect(result.code).toBe('version_incompatible');
    expect(result.category).toBe('mcp_protocol');
    expect(result.retryable).toBe(false);
    expect(result.correlation_id).toBe(correlationId);
    expect(result.details).toMatchObject({
      jsonrpc_code: -32022,
      supported: ['2026-07-28'],
      requested: '2099-01-01',
    });
  });

  it('maps a wire InvalidParams (-32602, malformed envelope) to invalid_request_envelope', () => {
    const result = toDiagnosticLokiError(
      { code: -32602, message: 'Invalid _meta envelope for protocol revision 2026-07-28: ...' },
      correlationId,
    );
    expect(result.code).toBe('invalid_request_envelope');
    expect(result.category).toBe('mcp_protocol');
  });

  it('maps an unrecognized numeric wire code to a generic mcp_protocol_error rather than guessing a closer-sounding one', () => {
    const result = toDiagnosticLokiError(
      { code: -32099, message: 'reserved range error' },
      correlationId,
    );
    expect(result.code).toBe('mcp_protocol_error');
    expect(result.category).toBe('mcp_protocol');
    expect(result.details).toMatchObject({ jsonrpc_code: -32099 });
  });

  it('maps a local SdkError-shaped value (string code, never crosses the wire) to an mcp_sdk_-prefixed code', () => {
    const result = toDiagnosticLokiError(
      { code: 'CAPABILITY_NOT_SUPPORTED', message: 'Server does not support tools' },
      correlationId,
    );
    expect(result.code).toBe('mcp_sdk_capability_not_supported');
    expect(result.category).toBe('mcp_sdk');
  });

  it('maps a generic Error to internal_correlation_error', () => {
    const result = toDiagnosticLokiError(new Error('boom'), correlationId);
    expect(result.code).toBe('internal_correlation_error');
    expect(result.category).toBe('internal');
    expect(result.message).toBe('boom');
  });

  it('maps a non-Error thrown value without crashing', () => {
    const result = toDiagnosticLokiError('a raw string throw', correlationId);
    expect(result.code).toBe('internal_correlation_error');
    expect(result.message).toBe('a raw string throw');
  });

  it('bounds an oversized message to LokiErrorSchema max length', () => {
    const result = toDiagnosticLokiError(new Error('x'.repeat(5000)), correlationId);
    expect(result.message.length).toBeLessThanOrEqual(2000);
  });

  it('every produced record is itself schema-valid (parse never throws for these inputs)', () => {
    const inputs: unknown[] = [
      { code: -32022, message: 'v', data: { supported: [], requested: 'x' } },
      { code: 'REQUEST_TIMEOUT', message: 't' },
      new Error('e'),
      undefined,
      null,
      42,
    ];
    for (const input of inputs) {
      expect(() => toDiagnosticLokiError(input, correlationId)).not.toThrow();
    }
  });
});
