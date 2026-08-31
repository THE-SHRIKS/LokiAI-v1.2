/**
 * `@lokiai/contracts` — LokiAI Main v1.2 shared cross-language contracts.
 *
 * See `packages/contracts/README.md` for scope, versioning policy, and import
 * guidance. Every exported type has a paired runtime (zod) validator per
 * `AGENTS.md` rule 4 — search for the `*Schema` export next to each type.
 */

/**
 * This package's own version. Kept in sync with `package.json`'s `version`
 * field by convention (not derived automatically, since reading `package.json`
 * at runtime is not reliable across every consumer's bundler). Consumers that
 * need to detect a contracts-package mismatch across process boundaries can
 * compare this value; it is distinct from `Envelope.protocol_version`, which
 * is a per-message wire-protocol version, not a package version.
 */
export const CONTRACTS_PACKAGE_VERSION = '0.1.0';

export * from './brand.js';
export * from './ids.js';
export * from './timestamp.js';
export * from './states.js';
export * from './connector.js';
export * from './envelope.js';
export * from './errors.js';
export * from './mcp.js';
export * from './businessRpc.js';
