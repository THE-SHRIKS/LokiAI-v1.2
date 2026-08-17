-- DAT-001: durable job records table.
--
-- Docs/SPEC.md Section 4.7 Durable job + 3.12 "job ID" + Section 1
-- `OperationResult<T>` variants (`accepted`/`approval_required`/`rejected`/
-- `completed`), which this table's evidence_refs_json/result_json/
-- error_json columns persist for a completed/rejected job.
--
-- idempotency_key is NOT NULL and unique: Docs/SPEC.md 4.7 "A device-
-- changing stage cannot run twice under the same idempotency key" and
-- AGENTS.md rule 5 ("Use stable IDs ... and idempotency keys for
-- operations that may be retried") — every durable job this table stores
-- is exactly this kind of retryable operation.
--
-- Per-stage/attempt tracking (Docs/SPEC.md 4.7 "Every stage has attempt,
-- start/end time, bounded logs, and result") is intentionally NOT modeled
-- as its own table here: SPEC does not give "stage" a Section-3-style field
-- list the way it does for Device, and building one speculatively would be
-- exactly what Docs/graph/tasks/DAT-001.md Deliverable 3 warns against
-- ("do not build every entity's repository speculatively"). A later job-
-- engine node owns that.
CREATE TABLE IF NOT EXISTS jobs (
    job_id               TEXT NOT NULL PRIMARY KEY,
    state                 TEXT NOT NULL CHECK (state IN (
                               'queued', 'policy-check', 'waiting-approval', 'preflight',
                               'executing', 'restarting', 'testing', 'observing',
                               'succeeded', 'failed', 'cancelled', 'rollback-pending',
                               'rolling-back', 'rolled-back', 'outcome-unknown'
                           )),
    idempotency_key        TEXT NOT NULL,
    correlation_id          TEXT NOT NULL,
    evidence_refs_json      TEXT,
    result_json             TEXT,
    error_json              TEXT,
    created_at               TEXT NOT NULL,
    updated_at               TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_idempotency_key ON jobs (idempotency_key);
CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs (state);
CREATE INDEX IF NOT EXISTS idx_jobs_correlation_id ON jobs (correlation_id);
