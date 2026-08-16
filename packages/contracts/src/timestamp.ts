import { z } from 'zod';

/**
 * `Docs/SPEC.md` Section 1: "All timestamps are UTC ISO 8601." `z.iso.datetime()`
 * defaults to `offset: false`, which — per zod's ISO 8601 datetime validator —
 * accepts only the `Z` (UTC/Zulu) suffix and rejects numeric-offset forms such
 * as `+05:30`, so the validator itself enforces "UTC", not merely "ISO 8601".
 * `local: false` (the default) additionally requires a timezone designator,
 * rejecting timezone-less forms.
 */
export const IsoUtcTimestampSchema = z.iso.datetime();
export type IsoUtcTimestamp = z.infer<typeof IsoUtcTimestampSchema>;
