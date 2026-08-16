import { describe, it } from 'vitest';
import { IsoUtcTimestampSchema } from '../src/timestamp.js';
import { expectInvalid, expectValid } from './helpers.js';

describe('IsoUtcTimestampSchema', () => {
  it('accepts a UTC ISO 8601 timestamp with a Z suffix', () => {
    expectValid(IsoUtcTimestampSchema, '2026-08-15T12:00:00Z');
  });

  it('accepts a UTC ISO 8601 timestamp with fractional seconds', () => {
    expectValid(IsoUtcTimestampSchema, '2026-08-15T12:00:00.123Z');
  });

  it('rejects a numeric-offset timestamp (not UTC)', () => {
    expectInvalid(IsoUtcTimestampSchema, '2026-08-15T12:00:00+05:30');
  });

  it('rejects a timestamp with no timezone designator', () => {
    expectInvalid(IsoUtcTimestampSchema, '2026-08-15T12:00:00');
  });

  it('rejects a date-only string', () => {
    expectInvalid(IsoUtcTimestampSchema, '2026-08-15');
  });

  it('rejects a non-date string', () => {
    expectInvalid(IsoUtcTimestampSchema, 'not-a-timestamp');
  });

  it('rejects a missing value', () => {
    expectInvalid(IsoUtcTimestampSchema, undefined);
  });

  it('rejects a wrong type', () => {
    expectInvalid(IsoUtcTimestampSchema, Date.now());
  });
});
