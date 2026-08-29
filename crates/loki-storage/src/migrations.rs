//! Versioned, idempotent SQL migration runner. Migrations are embedded at
//! compile time (`include_str!`) from `migrations/*.sql` and applied in
//! ascending version order, each inside its own transaction — so a
//! migration that fails partway rolls back completely, including its
//! `schema_migrations` bookkeeping row (see `broken_migration_rolls_back_completely`
//! below, and `Docs/graph/tasks/DAT-001.md` Deliverable 5: "a transaction
//! that fails mid-way rolls back completely").

use std::collections::HashSet;

use rusqlite::{Connection, params};

use crate::error::StorageError;
use crate::timestamp::validate_utc_timestamp;

pub(crate) struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

/// The full, ordered set of embedded migrations this crate ships.
pub(crate) const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "devices",
        sql: include_str!("../migrations/0001_devices.sql"),
    },
    Migration {
        version: 2,
        name: "jobs",
        sql: include_str!("../migrations/0002_jobs.sql"),
    },
    Migration {
        version: 3,
        name: "outbox_inbox",
        sql: include_str!("../migrations/0003_outbox_inbox.sql"),
    },
    // P1-ACCEPTANCE-STORE-001: consolidates `loki-approvals`/`loki-agents`/
    // `loki-agent-delivery`/`loki-local-authority`'s previously independent
    // per-crate SQLite connections into this one runtime-owned database, so
    // a real cross-repository `Storage::transaction()` is possible (see
    // `Docs/graph/decisions/ADR-0006-transactional-runtime-mutation-acceptance.md`).
    // Append-only, in this exact order, after `0003` — never edit `0001`-`0003`.
    Migration {
        version: 4,
        name: "approvals",
        sql: include_str!("../migrations/0004_approvals.sql"),
    },
    Migration {
        version: 5,
        name: "agents",
        sql: include_str!("../migrations/0005_agents.sql"),
    },
    Migration {
        version: 6,
        name: "agent_delivery_and_local_authority",
        sql: include_str!("../migrations/0006_agent_delivery_and_local_authority.sql"),
    },
    Migration {
        version: 7,
        name: "mutation_acceptances",
        sql: include_str!("../migrations/0007_mutation_acceptances.sql"),
    },
    // P1-PRINCIPAL-GRANT-001 (ADR-0007 Option B): additive, independent
    // durable generation counters for each identity model — see
    // `crates/loki-storage/src/{local_authority.rs,agents.rs}`'s module doc
    // comments. Append-only, in this exact order, after `0007` — never edit
    // `0001`-`0007`.
    Migration {
        version: 8,
        name: "local_authority_grant_generation",
        sql: include_str!("../migrations/0008_local_authority_grant_generation.sql"),
    },
    Migration {
        version: 9,
        name: "agent_grant_generation",
        sql: include_str!("../migrations/0009_agent_grant_generation.sql"),
    },
    Migration {
        version: 10,
        name: "approval_operation_binding",
        sql: include_str!("../migrations/0010_approval_operation_binding.sql"),
    },
];

/// What [`crate::Storage::run_migrations`] actually did on a given call.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MigrationReport {
    /// Versions applied *by this call*, in the order they were applied.
    /// Empty means the database was already fully up to date — the
    /// idempotent "running twice" case
    /// (`Docs/graph/tasks/DAT-001.md` Deliverable 5).
    pub applied_versions: Vec<i64>,
}

impl MigrationReport {
    /// `true` if this call found nothing new to apply.
    pub fn is_up_to_date(&self) -> bool {
        self.applied_versions.is_empty()
    }
}

fn ensure_migrations_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version     INTEGER NOT NULL PRIMARY KEY,
            name         TEXT NOT NULL,
            applied_at   TEXT NOT NULL
        ) STRICT;",
    )
}

fn applied_versions(conn: &Connection) -> rusqlite::Result<HashSet<i64>> {
    let mut stmt = conn.prepare("SELECT version FROM schema_migrations")?;
    let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    rows.collect()
}

/// Applies every migration in `migrations` not already recorded in
/// `schema_migrations`, in slice order, each inside its own transaction.
/// `applied_at` is a caller-supplied UTC ISO 8601 timestamp (validated) —
/// this crate does not call a system clock (see `src/timestamp.rs`).
pub(crate) fn apply_all(
    conn: &mut Connection,
    migrations: &[Migration],
    applied_at: &str,
) -> Result<MigrationReport, StorageError> {
    validate_utc_timestamp("applied_at", applied_at)?;
    ensure_migrations_table(conn)?;
    let already = applied_versions(conn)?;

    let mut applied_now = Vec::new();
    for migration in migrations {
        if already.contains(&migration.version) {
            continue;
        }

        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            params![migration.version, migration.name, applied_at],
        )?;
        tx.commit()?;
        applied_now.push(migration.version);
    }

    Ok(MigrationReport {
        applied_versions: applied_now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_cleanly_on_empty_db() {
        // `Connection::open_in_memory()` here bypasses `Storage`/`Cipher`
        // entirely, but still performs SQLite's process-global lazy default
        // init the first time any connection opens in this process — the
        // same one-shot window `secure_memory::test_support::
        // ensure_test_arena_ready`'s `sqlite3_config(SQLITE_CONFIG_MALLOC)`
        // needs (see `storage.rs`'s equivalent tests for the full
        // explanation; this file shares the `loki-storage --lib` binary
        // with `storage.rs` and `managed.rs`).
        crate::secure_memory::test_support::ensure_test_arena_ready();
        let mut conn = Connection::open_in_memory().unwrap();
        let report = apply_all(&mut conn, MIGRATIONS, "2026-01-01T00:00:00Z").unwrap();
        assert_eq!(report.applied_versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN
                 ('devices', 'jobs', 'outbox_messages', 'inbox_messages', 'schema_migrations',
                  'local_approvals', 'agent_connections', 'agent_runs', 'agent_run_events',
                  'work_intent_deliveries', 'local_installation_authority',
                  'local_authority_sessions', 'mutation_acceptances')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            table_count, 13,
            "every migration's tables plus the bookkeeping table must exist"
        );
    }

    #[test]
    fn is_idempotent_across_repeated_calls() {
        crate::secure_memory::test_support::ensure_test_arena_ready();
        let mut conn = Connection::open_in_memory().unwrap();
        let first = apply_all(&mut conn, MIGRATIONS, "2026-01-01T00:00:00Z").unwrap();
        assert_eq!(first.applied_versions.len(), 10);

        let second = apply_all(&mut conn, MIGRATIONS, "2026-01-02T00:00:00Z").unwrap();
        assert!(second.is_up_to_date(), "second run must apply nothing new");

        let recorded: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(recorded, 10, "must not duplicate schema_migrations rows");
    }

    #[test]
    fn rejects_invalid_applied_at_before_touching_the_schema() {
        crate::secure_memory::test_support::ensure_test_arena_ready();
        let mut conn = Connection::open_in_memory().unwrap();
        let err = apply_all(&mut conn, MIGRATIONS, "not-a-timestamp").unwrap_err();
        assert!(matches!(err, StorageError::InvalidTimestamp { .. }));

        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'devices'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            table_count, 0,
            "an invalid applied_at must reject before any migration runs"
        );
    }

    #[test]
    fn broken_migration_rolls_back_completely() {
        crate::secure_memory::test_support::ensure_test_arena_ready();
        let mut conn = Connection::open_in_memory().unwrap();
        let broken = [Migration {
            version: 1,
            name: "broken",
            sql: "CREATE TABLE partial_table (id INTEGER PRIMARY KEY); THIS IS NOT VALID SQL;",
        }];

        let result = apply_all(&mut conn, &broken, "2026-01-01T00:00:00Z");
        assert!(
            result.is_err(),
            "malformed migration SQL must surface as an error"
        );

        let table_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'partial_table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            table_exists, 0,
            "partial DDL from a failed migration must not persist"
        );

        let recorded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            recorded, 0,
            "a failed migration must not be recorded as applied"
        );
    }

    #[test]
    fn earlier_committed_migration_survives_a_later_one_failing() {
        crate::secure_memory::test_support::ensure_test_arena_ready();
        let mut conn = Connection::open_in_memory().unwrap();
        let mixed = [
            Migration {
                version: 1,
                name: "good",
                sql: "CREATE TABLE good_table (id INTEGER PRIMARY KEY) STRICT;",
            },
            Migration {
                version: 2,
                name: "broken",
                sql: "CREATE TABLE bad_table (id INTEGER PRIMARY KEY); NOT VALID SQL;",
            },
        ];

        let result = apply_all(&mut conn, &mixed, "2026-01-01T00:00:00Z");
        assert!(result.is_err());

        let good_recorded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            good_recorded, 1,
            "migration 1 committed independently before migration 2 failed"
        );

        let good_table_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'good_table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(good_table_exists, 1);

        let bad_recorded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bad_recorded, 0);
    }
}
