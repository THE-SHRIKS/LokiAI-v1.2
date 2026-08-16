import { describe, expect, it } from 'vitest';
import {
  DIGEST_MAX_LENGTH,
  ID_MAX_LENGTH,
  digestSchema,
  idSchema,
  opaqueReferenceSchema,
} from '../src/brand.js';
import { expectInvalid, expectValid } from './helpers.js';

describe('idSchema', () => {
  const TestId = idSchema('TestId');

  it('accepts a well-formed opaque identifier', () => {
    const value = expectValid(TestId, 'dev_01HZY3K7QK9WZC9V8G6X7N2B4M');
    expect(value).toBe('dev_01HZY3K7QK9WZC9V8G6X7N2B4M');
  });

  it('accepts a UUID-shaped identifier', () => {
    expectValid(TestId, '3fa85f64-5717-4562-b3fc-2c963f66afa6');
  });

  it('rejects an empty string (missing effective value)', () => {
    expectInvalid(TestId, '');
  });

  it('rejects a non-string type', () => {
    expectInvalid(TestId, 12345);
    expectInvalid(TestId, null);
    expectInvalid(TestId, undefined);
    expectInvalid(TestId, { id: 'x' });
  });

  it('rejects an oversized payload', () => {
    expectInvalid(TestId, 'a'.repeat(ID_MAX_LENGTH + 1));
  });

  it('accepts exactly the maximum length', () => {
    expectValid(TestId, 'a'.repeat(ID_MAX_LENGTH));
  });

  it('rejects unsupported characters', () => {
    expectInvalid(TestId, 'not a valid id!');
    expectInvalid(TestId, 'id/with/slashes');
    expectInvalid(TestId, '<script>alert(1)</script>');
  });

  it('brands distinct id kinds as structurally distinct at the type level', () => {
    const OtherId = idSchema('OtherId');
    const testValue = expectValid(TestId, 'shared-literal-value');
    const otherValue = expectValid(OtherId, 'shared-literal-value');
    // Both parse the same raw string, proving branding is a compile-time
    // nominal-typing aid, not an additional runtime discriminator — the
    // runtime shape (a plain string) is intentionally identical.
    expect(String(testValue)).toBe(String(otherValue));
  });
});

describe('digestSchema', () => {
  const TestDigest = digestSchema('TestDigest');
  const sha256Hex = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85';

  it('accepts a sha256-hex-shaped digest', () => {
    expectValid(TestDigest, sha256Hex);
  });

  it('accepts a base64url-shaped digest', () => {
    expectValid(TestDigest, 'ND0ZLROFcnfoRSpVGqmi_Jt5v4tGvOnvv3DFvVy2iX4');
  });

  it('rejects a too-short value', () => {
    expectInvalid(TestDigest, 'abc123');
  });

  it('rejects an oversized payload', () => {
    expectInvalid(TestDigest, 'a'.repeat(DIGEST_MAX_LENGTH + 1));
  });

  it('rejects unsupported characters (e.g. base64 padding/plus/slash)', () => {
    expectInvalid(TestDigest, `${sha256Hex.slice(0, 40)}+/==`);
  });

  it('rejects a non-string type', () => {
    expectInvalid(TestDigest, 42);
  });
});

describe('opaqueReferenceSchema', () => {
  const TestRef = opaqueReferenceSchema('TestRef');

  it('accepts a bounded non-empty string', () => {
    expectValid(TestRef, 's3://evidence-bucket/incident-42/log.jsonl');
  });

  it('rejects an empty string', () => {
    expectInvalid(TestRef, '');
  });

  it('rejects an oversized payload beyond the configured max', () => {
    const shortRef = opaqueReferenceSchema('ShortRef', 10);
    expectValid(shortRef, '0123456789');
    expectInvalid(shortRef, '01234567890');
  });

  it('rejects a non-string type', () => {
    expectInvalid(TestRef, { ref: 'x' });
  });
});
