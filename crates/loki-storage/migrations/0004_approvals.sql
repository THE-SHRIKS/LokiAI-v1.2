-- P1-ACCEPTANCE-STORE-001: consolidates `loki-approvals`'s previously
-- independent `local_approvals` table (created ad hoc in that crate's own
-- `ApprovalStore::from_connection`, on its own SQLite connection) into the
-- one runtime-owned `loki-storage` database, so a real cross-repository
-- `Storage::transaction()` (ADR-0006) can see and mutate approval rows in
-- the same transaction as `jobs`/`outbox_messages`/`mutation_acceptances`.
--
-- Column-for-column mechanical mirror of the exact `CREATE TABLE IF NOT
-- EXISTS local_approvals (...)` statement `loki-approvals`'s
-- `ApprovalStore::from_connection` ran directly on its own connection
-- before this migration existed (see `crates/loki-approvals/src/lib.rs`
-- git history) — not a redesign. `loki-storage/src/approvals.rs` is the
-- repository module this migration backs.
CREATE TABLE IF NOT EXISTS local_approvals (
   approval_id TEXT PRIMARY KEY,
   request_digest TEXT NOT NULL UNIQUE,
   requester_sid TEXT NOT NULL,
   actor_id TEXT NOT NULL,
   operation TEXT NOT NULL,
   target_id TEXT NOT NULL,
   artifact_digest TEXT,
   parameters_digest TEXT NOT NULL,
   idempotency_key TEXT NOT NULL,
   reason TEXT NOT NULL,
   requested_at_ms INTEGER NOT NULL,
   expires_at_ms INTEGER NOT NULL,
   granted_at_ms INTEGER,
   granted_by_sid TEXT,
   consumed_at_ms INTEGER,
   CHECK (expires_at_ms > requested_at_ms)
);
