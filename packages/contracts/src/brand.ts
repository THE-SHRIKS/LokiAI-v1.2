import { z } from 'zod';

/**
 * Shared primitives for LokiAI's opaque, stable identifiers and content digests.
 *
 * `Docs/SPEC.md` Section 1 requires "Every externally visible identifier is
 * opaque and stable" but does not pin one concrete generation scheme (UUID,
 * ULID, nanoid, and human-readable prefixed IDs such as `dev_...` are all
 * plausible across the different subsystems that will mint these IDs). Rather
 * than invent a specific format, this module defines a conservative, shared
 * *wire shape* every ID/digest must satisfy — non-empty, length-bounded, and
 * restricted to a safe charset — and brands the TypeScript type so callers
 * cannot silently pass a `WorkspaceId` where a `DeviceId` is expected. The
 * bound exists so oversized/malformed input is always rejected at the trust
 * boundary per `AGENTS.md` rule 4, not to encode a product decision about ID
 * length.
 *
 * Runtime validation (zod) always accompanies the compile-time brand per
 * `AGENTS.md` rule 4 — "types alone are not runtime validation."
 */

/** Maximum accepted length for any opaque LokiAI identifier. */
export const ID_MAX_LENGTH = 200;

/** Maximum accepted length for any opaque LokiAI content digest. */
export const DIGEST_MAX_LENGTH = 128;

/** Safe charset for opaque identifiers: alphanumeric plus `. _ : -`. */
const ID_PATTERN = /^[A-Za-z0-9._:-]+$/;

/** Safe charset for content digests: hex or base64url (no padding). */
const DIGEST_PATTERN = /^[A-Za-z0-9_-]+$/;

/**
 * Builds a branded, runtime-validated schema for an opaque LokiAI identifier.
 *
 * The resulting schema accepts a plain `string` as input and produces a
 * nominal `string & Brand` on successful parse, so two different ID kinds
 * (e.g. `DeviceId` and `WorkspaceId`) are not interchangeable at compile time
 * even though both are plain strings on the wire.
 */
export function idSchema<Brand extends string>(brand: Brand) {
  return z
    .string()
    .min(1, `${brand} must not be empty`)
    .max(ID_MAX_LENGTH, `${brand} exceeds maximum length of ${ID_MAX_LENGTH}`)
    .regex(ID_PATTERN, `${brand} contains unsupported characters`)
    .brand<Brand>();
}

/**
 * Builds a branded, runtime-validated schema for an opaque LokiAI content
 * digest (artifact digest, bound-request digest, toolchain digest, etc.).
 * The exact digest algorithm is a producer decision outside this package's
 * scope; the shared shape only bounds length and charset so a malformed or
 * oversized digest is always rejected at the trust boundary.
 */
export function digestSchema<Brand extends string>(brand: Brand) {
  return z
    .string()
    .min(16, `${brand} is shorter than any supported digest algorithm output`)
    .max(DIGEST_MAX_LENGTH, `${brand} exceeds maximum length of ${DIGEST_MAX_LENGTH}`)
    .regex(DIGEST_PATTERN, `${brand} contains unsupported characters`)
    .brand<Brand>();
}

/** A generic bounded, non-empty opaque reference (storage/evidence/etc.). */
export function opaqueReferenceSchema(brand: string, maxLength = 2000) {
  return z
    .string()
    .min(1, `${brand} must not be empty`)
    .max(maxLength, `${brand} exceeds maximum length of ${maxLength}`);
}
