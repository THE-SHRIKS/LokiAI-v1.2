import { describe, expect, it } from 'vitest';
import { computeFreshness, FRESHNESS_STALE_THRESHOLD_MS } from '../../src/devices/freshness.js';

describe('computeFreshness', () => {
  it('reports fresh when observed just now', () => {
    const now = Date.parse('2026-08-18T00:00:30.000Z');
    expect(computeFreshness('2026-08-18T00:00:30.000Z', now)).toBe('fresh');
  });

  it('reports fresh right at the threshold boundary', () => {
    const now = Date.parse('2026-08-18T00:00:00.000Z') + FRESHNESS_STALE_THRESHOLD_MS;
    expect(computeFreshness('2026-08-18T00:00:00.000Z', now)).toBe('fresh');
  });

  it('reports stale just past the threshold', () => {
    const now = Date.parse('2026-08-18T00:00:00.000Z') + FRESHNESS_STALE_THRESHOLD_MS + 1;
    expect(computeFreshness('2026-08-18T00:00:00.000Z', now)).toBe('stale');
  });

  it('reports stale for an unparseable timestamp rather than throwing', () => {
    expect(() => computeFreshness('not-a-timestamp')).not.toThrow();
    expect(computeFreshness('not-a-timestamp')).toBe('stale');
  });
});
