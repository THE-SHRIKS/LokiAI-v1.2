import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import {
  KNOWN_ERROR_CODES,
  KnownErrorCodeSchema,
  LokiErrorSchema,
  OPERATION_RESULT_OUTCOMES,
  OperationResultOutcomeSchema,
  operationResultSchema,
} from '../src/errors.js';
import { expectInvalid, expectValid, omit } from './helpers.js';

describe('KnownErrorCodeSchema', () => {
  it('accepts every known error code', () => {
    for (const code of KNOWN_ERROR_CODES) {
      expectValid(KnownErrorCodeSchema, code);
    }
  });

  it('rejects an unlisted code (closed enum, unlike LokiErrorSchema.code)', () => {
    expectInvalid(KnownErrorCodeSchema, 'a_future_error_code');
  });
});

describe('OperationResultOutcomeSchema', () => {
  it('accepts every documented outcome value', () => {
    for (const outcome of OPERATION_RESULT_OUTCOMES) {
      expectValid(OperationResultOutcomeSchema, outcome);
    }
  });

  it('rejects an unknown outcome value', () => {
    expectInvalid(OperationResultOutcomeSchema, 'pending');
  });

  it('rejects a wrong type', () => {
    expectInvalid(OperationResultOutcomeSchema, null);
  });
});

describe('LokiErrorSchema', () => {
  const valid = {
    code: 'device_busy',
    category: 'availability',
    message: 'Device is executing another operation.',
    retryable: true,
    correlation_id: 'corr-1',
  };

  it('accepts a minimal well-formed error', () => {
    expectValid(LokiErrorSchema, valid);
  });

  it('accepts every known error code (illustrative, not exhaustive)', () => {
    for (const code of KNOWN_ERROR_CODES) {
      expectValid(LokiErrorSchema, { ...valid, code });
    }
  });

  it('accepts an unlisted but well-formed error code', () => {
    expectValid(LokiErrorSchema, { ...valid, code: 'a_future_error_code' });
  });

  it('accepts optional retry_after_ms and details', () => {
    expectValid(LokiErrorSchema, {
      ...valid,
      retry_after_ms: 5000,
      details: { device_id: 'device-1' },
    });
  });

  it('rejects a missing required field', () => {
    expectInvalid(LokiErrorSchema, omit(valid, 'message'));
  });

  it('rejects a wrong type for retryable', () => {
    expectInvalid(LokiErrorSchema, { ...valid, retryable: 'true' });
  });

  it('rejects a negative retry_after_ms', () => {
    expectInvalid(LokiErrorSchema, { ...valid, retry_after_ms: -1 });
  });

  it('rejects a non-slug code', () => {
    expectInvalid(LokiErrorSchema, { ...valid, code: 'Device Busy' });
  });

  it('rejects an oversized message', () => {
    expectInvalid(LokiErrorSchema, { ...valid, message: 'x'.repeat(2001) });
  });
});

describe('operationResultSchema', () => {
  const resultPayload = z.object({ verified: z.boolean() });
  const schema = operationResultSchema(resultPayload);

  it('accepts a well-formed "accepted" result', () => {
    expectValid(schema, {
      outcome: 'accepted',
      job_id: 'job-1',
      state: 'queued',
      accepted_at: '2026-08-15T12:00:00Z',
    });
  });

  it('accepts a well-formed "approval_required" result', () => {
    expectValid(schema, {
      outcome: 'approval_required',
      approval_id: 'approval-1',
      bound_request_digest: 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85',
      expires_at: '2026-08-15T13:00:00Z',
    });
  });

  it('accepts a well-formed "rejected" result', () => {
    expectValid(schema, {
      outcome: 'rejected',
      error: {
        code: 'permission_denied',
        category: 'authorization',
        message: 'Workspace role does not permit this operation.',
        retryable: false,
        correlation_id: 'corr-1',
      },
    });
  });

  it('accepts a well-formed "completed" result using the caller payload schema', () => {
    const parsed = expectValid(schema, {
      outcome: 'completed',
      job_id: 'job-1',
      evidence_refs: ['s3://evidence/1'],
      result: { verified: true },
    });
    expect(parsed.outcome).toBe('completed');
  });

  it('rejects an unknown outcome discriminant', () => {
    expectInvalid(schema, { outcome: 'pending', job_id: 'job-1' });
  });

  it('rejects a variant missing its required fields', () => {
    expectInvalid(schema, { outcome: 'accepted', job_id: 'job-1' });
  });

  it('rejects a "completed" result whose result payload fails the caller schema', () => {
    expectInvalid(schema, {
      outcome: 'completed',
      job_id: 'job-1',
      evidence_refs: [],
      result: { verified: 'yes' },
    });
  });

  it('rejects fields from the wrong variant mixed into another', () => {
    expectInvalid(schema, {
      outcome: 'rejected',
      job_id: 'job-1', // "rejected" requires `error`, not `job_id`
    });
  });

  it('rejects a missing outcome', () => {
    expectInvalid(schema, { job_id: 'job-1' });
  });
});
