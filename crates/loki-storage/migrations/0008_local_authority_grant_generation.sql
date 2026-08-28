-- P1-PRINCIPAL-GRANT-001 (ADR-0007 Option B): adds a durable, per-principal
-- session-generation counter to `local_authority_sessions`. Additive only —
-- no existing column/row/behavior changes.
--
-- Generation semantics (see `crates/loki-storage/src/local_authority.rs`'s
-- module doc comment and `Docs/graph/decisions/ADR-0007-grant-generation-identity-unification.md`
-- Option B): a session is its own row, never mutated in place except
-- `revoked_at_ms`; `generation` is assigned once, atomically, at
-- `insert_authority_session` time as `MAX(generation)+1` scoped to
-- `principal_sid`, computed and inserted inside the same
-- `Storage::lock()`-held critical section. Uniqueness of
-- `(principal_sid, generation)` is therefore an APPLICATION invariant
-- enforced by the single-writer `Storage` mutex, not a database constraint
-- — deliberately no `UNIQUE` index here (a pre-existing dev/test database
-- with rows already at the `DEFAULT 0` backfill value could otherwise
-- violate one, and Part One has no shipped installations to migrate
-- cleanly in the first place). The non-unique index below exists purely
-- for `MAX(generation)`/lookup performance.
ALTER TABLE local_authority_sessions ADD COLUMN generation INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_local_sessions_principal_generation
    ON local_authority_sessions(principal_sid, generation);
