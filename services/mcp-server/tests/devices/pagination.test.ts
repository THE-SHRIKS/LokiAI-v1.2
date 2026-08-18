import { describe, expect, it } from 'vitest';
import {
  MAX_PAGE_SIZE,
  MAX_HEALTH_ENTRIES,
  DEFAULT_PAGE_SIZE,
  DEFAULT_MAX_HEALTH_ENTRIES,
  clampLimit,
  clampMaxEntries,
  decodeCursor,
  encodeCursor,
} from '../../src/devices/pagination.js';

/**
 * `Docs/graph/tasks/MCP-002.md` Deliverable 3/4: "listing tools support a
 * bounded page size; no unbounded response" and "no single tool response can
 * be unbounded." These are the pure bound-enforcement primitives every tool
 * handler builds on.
 */
describe('clampLimit', () => {
  it('defaults to DEFAULT_PAGE_SIZE when omitted', () => {
    expect(clampLimit(undefined)).toBe(DEFAULT_PAGE_SIZE);
  });

  it('never exceeds MAX_PAGE_SIZE even for a huge requested value', () => {
    expect(clampLimit(1_000_000)).toBe(MAX_PAGE_SIZE);
  });

  it('never goes below 1 even for zero/negative input', () => {
    expect(clampLimit(0)).toBe(1);
    expect(clampLimit(-50)).toBe(1);
  });

  it('passes through an in-range integer unchanged', () => {
    expect(clampLimit(7)).toBe(7);
  });

  it('truncates a non-integer to an integer', () => {
    expect(clampLimit(7.9)).toBe(7);
  });
});

describe('clampMaxEntries', () => {
  it('defaults to DEFAULT_MAX_HEALTH_ENTRIES when omitted', () => {
    expect(clampMaxEntries(undefined)).toBe(DEFAULT_MAX_HEALTH_ENTRIES);
  });

  it('never exceeds MAX_HEALTH_ENTRIES', () => {
    expect(clampMaxEntries(999_999)).toBe(MAX_HEALTH_ENTRIES);
  });

  it('never goes below 1', () => {
    expect(clampMaxEntries(-3)).toBe(1);
  });
});

describe('cursor encode/decode', () => {
  it('round-trips an offset through encode then decode', () => {
    const cursor = encodeCursor(42);
    expect(decodeCursor(cursor)).toEqual({ offset: 42 });
  });

  it('decodes undefined as the first page (offset 0)', () => {
    expect(decodeCursor(undefined)).toEqual({ offset: 0 });
  });

  it('rejects a cursor this server did not issue (garbage/tampered)', () => {
    expect(decodeCursor('not-a-real-cursor')).toBeNull();
  });

  it('rejects a cursor decoding to a negative offset', () => {
    const forged = Buffer.from(JSON.stringify({ offset: -1 }), 'utf8').toString('base64url');
    expect(decodeCursor(forged)).toBeNull();
  });

  it('rejects a cursor decoding to a non-integer offset', () => {
    const forged = Buffer.from(JSON.stringify({ offset: 1.5 }), 'utf8').toString('base64url');
    expect(decodeCursor(forged)).toBeNull();
  });

  it('rejects a cursor decoding to an unrelated JSON shape', () => {
    const forged = Buffer.from(JSON.stringify({ notAnOffset: 1 }), 'utf8').toString('base64url');
    expect(decodeCursor(forged)).toBeNull();
  });

  it('the encoded cursor only uses characters CursorSchema accepts (base64url, no padding)', () => {
    const cursor = encodeCursor(123456);
    expect(cursor).toMatch(/^[A-Za-z0-9._:-]+$/);
    expect(cursor).not.toContain('=');
  });
});
