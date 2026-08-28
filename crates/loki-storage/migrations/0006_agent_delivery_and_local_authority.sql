-- P1-ACCEPTANCE-STORE-001: consolidates `loki-agent-delivery`'s
-- `work_intent_deliveries` table and `loki-local-authority`'s
-- `local_installation_authority`/`local_authority_sessions` tables (each
-- previously created ad hoc on its own independent SQLite connection) into
-- the one runtime-owned `loki-storage` database — same rationale as
-- `0004_approvals.sql`. Both crates land in one migration file because
-- neither owns enough schema alone to justify a separate migration number,
-- and the task packet (`Docs/graph/tasks/P1-ACCEPTANCE-STORE-001.md`
-- Deliverable 1) names this exact combined filename.
--
-- Column-for-column mechanical mirror of each crate's exact prior
-- `CREATE TABLE IF NOT EXISTS (...)` statements (see
-- `crates/loki-agent-delivery/src/lib.rs` and
-- `crates/loki-local-authority/src/store.rs` git history) — not a
-- redesign. `PRAGMA foreign_keys=ON` is not repeated here: it is already
-- set by `loki_storage::storage::configure_connection` on every connection.
-- `loki-storage/src/agent_delivery.rs` and
-- `loki-storage/src/local_authority.rs` are the repository modules this
-- migration backs.
CREATE TABLE IF NOT EXISTS work_intent_deliveries(
    work_intent_id TEXT PRIMARY KEY,
    incident_id TEXT NOT NULL,
    requested_capability TEXT NOT NULL,
    evidence_refs_json TEXT NOT NULL,
    delivery_requirements TEXT NOT NULL,
    state TEXT NOT NULL,
    delivery_attempts INTEGER NOT NULL CHECK(delivery_attempts>=0),
    claimed_connection_id TEXT,
    acknowledged_agent_run TEXT,
    failure_reason TEXT,
    created_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL,
    delivered_at_ms INTEGER,
    terminal_at_ms INTEGER,
    revision INTEGER NOT NULL CHECK(revision>0)
);

CREATE TABLE IF NOT EXISTS local_installation_authority (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    installation_id TEXT NOT NULL UNIQUE,
    owner_sid TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

CREATE TABLE IF NOT EXISTS local_authority_sessions (
    session_id TEXT PRIMARY KEY,
    installation_id TEXT NOT NULL,
    principal_sid TEXT NOT NULL,
    ipc_session_id TEXT NOT NULL,
    correlation_id TEXT NOT NULL UNIQUE,
    issued_at_ms INTEGER NOT NULL CHECK (issued_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL CHECK (expires_at_ms > issued_at_ms),
    revoked_at_ms INTEGER,
    FOREIGN KEY (installation_id)
      REFERENCES local_installation_authority(installation_id)
);

CREATE INDEX IF NOT EXISTS idx_local_sessions_expiry
    ON local_authority_sessions(expires_at_ms);
