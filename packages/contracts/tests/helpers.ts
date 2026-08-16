import { expect } from 'vitest';
import type { z } from 'zod';

/** Asserts `value` parses successfully and returns the parsed (branded) data. */
export function expectValid<Schema extends z.ZodTypeAny>(
  schema: Schema,
  value: unknown,
): z.infer<Schema> {
  const result = schema.safeParse(value);
  if (!result.success) {
    throw new Error(`expected valid input to pass, got: ${JSON.stringify(result.error.issues)}`);
  }
  expect(result.success).toBe(true);
  return result.data;
}

/** Asserts `value` is rejected by `schema.safeParse`. */
export function expectInvalid<Schema extends z.ZodTypeAny>(schema: Schema, value: unknown): void {
  const result = schema.safeParse(value);
  expect(result.success).toBe(false);
}

/**
 * Returns a shallow copy of `obj` without `key` — used to build "missing
 * required field" malformed-input fixtures without tripping
 * `@typescript-eslint/no-unused-vars` on a discard-only destructured binding.
 */
export function omit<T extends Record<string, unknown>, K extends keyof T>(
  obj: T,
  key: K,
): Omit<T, K> {
  const clone = { ...obj };
  delete clone[key];
  return clone;
}
