-- P1-ACCEPTANCE-STORE-001: the canonical ADR-0006 step 2/3 durable
-- acceptance record ("bind ... idempotency key ..."; "claim or replay the
-- idempotency record"). This is a brand-new table, not a reuse of
-- `jobs.idempotency_key` (`0002_jobs.sql`) — `jobs.idempotency_key` is
-- `JOB-001`'s own key space for durable job records and must not be
-- shared with agent-mutation idempotency (see
-- `Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md` §3's known starting
-- conditions and Deliverable 5).
--
-- `idempotency_key` is the primary key: it is this table's natural unique
-- identity (one committed acceptance per idempotency key), independent of
-- `jobs.idempotency_key`'s own separate `UNIQUE` index on a different
-- table.
--
-- `job_id` is nullable: populated only for methods that create a real
-- `jobs` row (approval-gated device operations: `restart`, `flash`,
-- `apply_config`, `contained_build`). Agent methods
-- (`AgentRunRegister`/`AgentRunUpdate`/`WorkIntentAcknowledge`) populate a
-- synthesized stable identifier through the same `JobId` shape (see
-- `crates/loki-acceptance-store/src/lib.rs` doc comment for the exact
-- minting scheme) without ever inserting a corresponding `jobs` table row
-- — `job_id` here is never a foreign key into `jobs` for that reason.
--
-- `accepted_at_ms` is stored as `INTEGER` epoch milliseconds, not the
-- `TEXT` UTC-ISO-8601 convention every other `loki-storage` timestamp
-- column uses (`crates/loki-storage/src/timestamp.rs`). This is a
-- deliberate, cited exception: `loki_mutation_contract::AcceptedRecord`/
-- `AcceptanceRecordView` (the accepted `P1-MUTATION-CONTRACT-001` contract
-- this table exists to persist) already define `accepted_at_ms` as a
-- native `i64` epoch-millisecond field, and round-tripping it through an
-- ISO-8601 string here would only add a lossy conversion for no benefit —
-- see `crates/loki-storage/src/mutation_acceptance.rs`'s module doc
-- comment for the same note colocated with the code.
CREATE TABLE IF NOT EXISTS mutation_acceptances (
    idempotency_key   TEXT NOT NULL PRIMARY KEY,
    principal_sid      TEXT NOT NULL,
    grant_generation    INTEGER NOT NULL,
    method              TEXT NOT NULL,
    target_id           TEXT NOT NULL,
    artifact_digest     TEXT,
    request_digest      TEXT NOT NULL,
    job_id              TEXT,
    correlation_id      TEXT NOT NULL,
    accepted_at_ms       INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_mutation_acceptances_job_id
    ON mutation_acceptances (job_id);
