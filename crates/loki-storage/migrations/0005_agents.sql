-- P1-ACCEPTANCE-STORE-001: consolidates `loki-agents`'s three previously
-- independent tables (created ad hoc in that crate's own
-- `AgentRegistry::from_connection`, on its own SQLite connection) into the
-- one runtime-owned `loki-storage` database — same rationale as
-- `0004_approvals.sql`.
--
-- Column-for-column mechanical mirror of `loki-agents`'s exact prior
-- `CREATE TABLE IF NOT EXISTS agent_connections/agent_runs/
-- agent_run_events (...)` statements (see `crates/loki-agents/src/lib.rs`
-- git history) — not a redesign. `PRAGMA foreign_keys=ON` is not repeated
-- here: `loki_storage::storage::configure_connection` already sets it on
-- every connection this crate opens (`crates/loki-storage/src/storage.rs`).
-- `loki-storage/src/agents.rs` is the repository module this migration
-- backs.
CREATE TABLE IF NOT EXISTS agent_connections(
    connection_id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    secret_hash TEXT NOT NULL,
    scopes TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    revoked_at_ms INTEGER
);

CREATE TABLE IF NOT EXISTS agent_runs(
    run_id TEXT PRIMARY KEY,
    connection_id TEXT NOT NULL,
    parent_run_id TEXT,
    correlation_id TEXT NOT NULL UNIQUE,
    idempotency_key TEXT NOT NULL,
    state TEXT NOT NULL,
    started_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    UNIQUE(connection_id,idempotency_key),
    FOREIGN KEY(connection_id) REFERENCES agent_connections(connection_id),
    FOREIGN KEY(parent_run_id) REFERENCES agent_runs(run_id)
);

CREATE TABLE IF NOT EXISTS agent_run_events(
    run_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK(sequence>0),
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    summary TEXT NOT NULL,
    occurred_at_ms INTEGER NOT NULL,
    PRIMARY KEY(run_id,sequence),
    FOREIGN KEY(run_id) REFERENCES agent_runs(run_id)
);
