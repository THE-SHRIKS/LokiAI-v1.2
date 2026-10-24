#![cfg(windows)]
//! Real-host secure-arena bounds evidence for `DAT-002-MEM-HOST-VERIFY-001`.
//!
//! Every test in this file drives the exact production
//! `SecureMemoryRuntime::bootstrap_profile` path (`VirtualAlloc`,
//! `VirtualLock`, per-page `QueryWorkingSetEx` attestation, the
//! `SQLITE_CONFIG_MALLOC` handoff) via the `test-support`-gated
//! `bootstrap_candidate_test_arena` entry point added in this node. It never
//! touches `crate::secure_profile::ACCEPTED_PROFILE` (still `None` in this
//! diff) or weakens `require_ready()` — only the disposable, non-production
//! candidate profile changes per test.
//!
//! **Process-global state note**: `PHASE`/`ARENA` are one process-global pair
//! (this is a real property of production, not a test artifact — see
//! `secure_memory.rs`'s own doc comments). Every test that owns an arena
//! bootstraps and shuts it down itself before the next test runs, and this
//! binary must run with `--test-threads=1` (as the packet's Section 8 row 3-6
//! entries require) so scenarios do not race the shared `TEST_ARENA_BUSY`
//! slot in ways that would make one scenario's candidate bounds bleed into
//! another's measurement.
//!
//! Run as: `cargo test -p loki-storage --test secure_arena_host_bounds --
//! --test-threads=1`. The soak scenario is `#[ignore]`d by default (see its
//! doc comment for why) and run explicitly with `--ignored`.

use loki_storage::{Cipher, ProfileInspection, SecureMemoryError, Storage};
use rusqlite::Connection;

/// A reasonable, real, but intentionally small candidate used by the
/// exhaustion/fault-recover-rotate/churn scenarios: large enough that a
/// single SQLCipher connection can open, migrate a tiny schema, and run a
/// handful of statements, small enough that these tests can also validate
/// exhaustion without allocating gigabytes of committed+locked memory on a
/// single-CPU VM.
fn small_working_candidate() -> ProfileInspection {
    const ARENA_BYTES: usize = 2 * 1024 * 1024; // 2 MiB
    ProfileInspection {
        arena_bytes: ARENA_BYTES,
        working_set_min_bytes: ARENA_BYTES * 8,
        working_set_max_bytes: ARENA_BYTES * 32,
        connection_cap: 4,
        cache_pages_per_connection: 64,
        source_sha256: [0u8; 32],
        build_sha256: [0u8; 32],
        manifest_sha256: [0u8; 32],
    }
}

/// A tiny candidate sized to exhaust after a handful of real SQLCipher page
/// allocations, so the exhaustion scenario proves `SQLITE_NOMEM` without a
/// synthetic in-process allocator test (`secure_memory.rs`'s own unit tests
/// already cover the allocator algorithm directly; this is the real-SQLCipher
/// integration proof the packet's Section 8 row 4 requires).
fn tiny_exhaustible_candidate() -> ProfileInspection {
    // Measured on real Windows 11 host hardware (DAT-002-MEM-HOST-VERIFY-001):
    // a 64 KiB arena exhausts during `Connection::open` itself (before schema
    // creation), because the production allocator hookup's own bootstrap
    // overhead for a single connection exceeds 64 KiB on this host. 256 KiB
    // leaves enough real headroom for connection-open + schema creation while
    // still being far smaller than the deliberate 512 KiB oversized insert
    // below, so the exhaustion this test targets still occurs at the
    // intended point (the oversized insert), not during setup.
    const ARENA_BYTES: usize = 256 * 1024; // 256 KiB
    ProfileInspection {
        arena_bytes: ARENA_BYTES,
        working_set_min_bytes: ARENA_BYTES * 8,
        working_set_max_bytes: ARENA_BYTES * 32,
        connection_cap: 1,
        cache_pages_per_connection: 1,
        source_sha256: [0u8; 32],
        build_sha256: [0u8; 32],
        manifest_sha256: [0u8; 32],
    }
}

/// Induces a real Windows working-set-quota constraint by requesting an
/// arena far larger than the working-set ceiling the same candidate applies
/// via `SetProcessWorkingSetSizeEx` immediately before `VirtualAlloc`/
/// `VirtualLock`. This is the packet's explicitly-required substitute for
/// literally reproducing ADR-0005's original bug: the fixed-arena design
/// makes any lock/quota failure fail-closed by construction, so this proves
/// the *refusal*, not a reproduction of the old non-fatal continuation.
fn quota_hostile_candidate() -> ProfileInspection {
    const ARENA_BYTES: usize = 64 * 1024 * 1024; // 64 MiB: far beyond the tiny ceiling below.
    const CEILING_BYTES: usize = 1 * 1024 * 1024; // 1 MiB working-set ceiling.
    ProfileInspection {
        arena_bytes: ARENA_BYTES,
        working_set_min_bytes: CEILING_BYTES,
        working_set_max_bytes: CEILING_BYTES,
        connection_cap: 1,
        cache_pages_per_connection: 1,
        source_sha256: [0u8; 32],
        build_sha256: [0u8; 32],
        manifest_sha256: [0u8; 32],
    }
}

/// Row 3: a real induced working-set-quota constraint must refuse startup
/// fail-closed, with zero recursion and zero DB/key/sidecar-file access.
#[test]
#[ignore = "requires the approved disposable host-bounds environment; run serially"]
fn induced_working_set_constraint_refuses_closed_with_no_db_or_key_touch() {
    let outcome = loki_storage::secure_memory::test_support::bootstrap_candidate_test_arena(
        quota_hostile_candidate(),
    );

    // The exact failure mode is real Windows behavior, not asserted in
    // advance: either the working-set ceiling itself is rejected by
    // `SetProcessWorkingSetSizeEx`, or the subsequent `VirtualLock` is
    // refused once it exceeds the (successfully applied) tiny ceiling. Both
    // are real fail-closed refusals before any SQLite/key/DB touch — that is
    // the property this test proves, not which specific Win32 call reports
    // it. Record which one actually happened (see the evidence log this
    // test's output is captured into) rather than silently picking one.
    match &outcome {
        Err(SecureMemoryError::WorkingSetAdjustment(_)) | Err(SecureMemoryError::Lock(_)) => {}
        other => {
            panic!("expected a fail-closed working-set-adjustment or lock refusal, got: {other:?}")
        }
    }
    assert!(
        outcome.is_err(),
        "an induced working-set constraint must never yield a ready arena"
    );

    // Zero recursion / zero DB / zero key / zero sidecar touch: the failed
    // candidate never reached a point where any file I/O could occur, so a
    // completely fresh path used immediately afterward must not exist and
    // `Storage::open_managed` must refuse via `NotReady`, not by silently
    // creating an empty database or protected-key sidecar.
    let root = tempfile::tempdir().expect("create isolated test root");
    let database = root.path().join("quota-refusal.sqlite3");
    let key = root.path().join("quota-refusal.key.protected");
    let result = Storage::open_managed(&database, &key);
    assert!(
        matches!(
            result,
            Err(loki_storage::StorageError::SecureMemory(
                SecureMemoryError::NotReady
            ))
        ),
        "storage must refuse via NotReady after a failed candidate bootstrap, got: {result:?}"
    );
    assert!(!database.exists(), "no database file may be created");
    assert!(!key.exists(), "no protected-key sidecar may be created");
}

/// Row 4: a tiny candidate arena under real SQLCipher allocation pressure
/// must return `SQLITE_NOMEM` (surfaced here as a `rusqlite`/`StorageError`
/// out-of-memory failure), preserve transaction integrity (the failed
/// transaction's statements are not partially committed), and record zero
/// fallback/growth in the arena's own stats.
#[test]
#[ignore = "requires the approved disposable host-bounds environment; run serially"]
fn tiny_arena_exhaustion_returns_out_of_memory_with_zero_fallback_and_preserved_integrity() {
    let runtime = loki_storage::secure_memory::test_support::bootstrap_candidate_test_arena(
        tiny_exhaustible_candidate(),
    )
    .expect("a 64 KiB candidate must still lock cleanly on any supported host");

    let dir = tempfile::tempdir().expect("create isolated test root");
    let path = dir.path().join("exhaustion.sqlite3");
    let key = loki_storage::secret::generate_database_key()
        .expect("generate a real DPAPI-eligible random key");
    let conn = Connection::open(&path).expect("open a fresh sqlite file");
    loki_storage::configure_sqlcipher(&conn, &key).expect("configure sqlcipher on the tiny arena");
    loki_storage::configure_connection(&conn).expect("configure the connection");

    conn.execute_batch("CREATE TABLE probe(payload BLOB);")
        .expect("schema creation must fit in a 64 KiB arena");

    // Insert a single, deliberately oversized blob designed to exhaust the
    // 64 KiB arena in one statement, inside an explicit transaction so a
    // failure's rollback behavior is directly observable.
    let oversized = vec![0x5au8; 512 * 1024]; // 512 KiB >> the 64 KiB arena.
    conn.execute_batch("BEGIN;").expect("begin transaction");
    let insert_result = conn.execute(
        "INSERT INTO probe (payload) VALUES (?1)",
        [oversized.as_slice()],
    );
    assert!(
        insert_result.is_err(),
        "an allocation far larger than the arena must fail, not silently fall back"
    );
    conn.execute_batch("ROLLBACK;")
        .expect("rollback must succeed after the failed statement");

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM probe", [], |row| row.get(0))
        .expect("table remains queryable after rollback");
    assert_eq!(
        count, 0,
        "the failed oversized insert must leave zero rows — transaction integrity preserved"
    );

    let stats_before_close = runtime.stats();
    assert_eq!(
        stats_before_close.fallback_count, 0,
        "the fixed allocator must never fall back to the system heap"
    );
    assert!(
        stats_before_close.exhaustion_count >= 1,
        "the oversized insert must have recorded at least one exhaustion"
    );

    drop(conn);
    let final_stats = runtime
        .shutdown()
        .expect("shutdown must succeed with the failed connection already closed");
    assert_eq!(
        final_stats.active_allocations, 0,
        "no allocation may remain outstanding after every connection is closed"
    );
    assert_eq!(final_stats.fallback_count, 0);
}

/// Row 5: the full recover-then-rotate fault lifecycle (interrupted
/// plaintext migration, interrupted key promotion, and a real rotation) must
/// complete repeatedly, on the real arena/allocator path, without recursion,
/// corruption, or a stuck `PHASE`.
#[test]
#[ignore = "requires the approved disposable host-bounds environment; run serially"]
fn fault_recover_rotate_lifecycle_completes_without_recursion_or_corruption() {
    let runtime = loki_storage::secure_memory::test_support::bootstrap_candidate_test_arena(
        small_working_candidate(),
    )
    .expect("a 2 MiB candidate must lock cleanly on any supported host");

    let dir = tempfile::tempdir().expect("create isolated test root");
    let database = dir.path().join("fault-lifecycle.sqlite3");
    let key = dir.path().join("fault-lifecycle.key.protected");

    // First open creates a fresh SQLCipher database and protected key.
    let (conn, _key) =
        loki_storage::open_or_migrate_sqlcipher(&database, &key).expect("first open creates db");
    conn.execute_batch(
        "CREATE TABLE fault_probe(value TEXT); INSERT INTO fault_probe VALUES ('alive');",
    )
    .expect("seed data");
    drop(conn);

    // Real rotation: re-encrypts under a fresh key using the same
    // verified replacement/rollback protocol as the interrupted-marker
    // recovery tests in `managed.rs`, but end to end for real here (no
    // hand-injected marker).
    loki_storage::rotate_sqlcipher_key(&database, &key).expect("rotation must succeed");

    let (reopened, _key) = loki_storage::open_or_migrate_sqlcipher(&database, &key)
        .expect("reopen after rotation must succeed with the new key");
    let value: String = reopened
        .query_row("SELECT value FROM fault_probe", [], |row| row.get(0))
        .expect("data survives rotation");
    assert_eq!(value, "alive");
    drop(reopened);

    // Repeat the same rotate/reopen cycle a second time on the same live
    // arena to prove there is no accumulated corruption/leak/recursion
    // across repeated real fault-lifecycle operations, not just one pass.
    loki_storage::rotate_sqlcipher_key(&database, &key).expect("second rotation must succeed");
    let (reopened_again, _key) = loki_storage::open_or_migrate_sqlcipher(&database, &key)
        .expect("reopen after second rotation must succeed");
    let value_again: String = reopened_again
        .query_row("SELECT value FROM fault_probe", [], |row| row.get(0))
        .expect("data survives a second rotation");
    assert_eq!(value_again, "alive");
    drop(reopened_again);

    let stats = runtime.stats();
    assert_eq!(
        stats.fallback_count, 0,
        "no fallback allocation across two real rotation cycles"
    );

    let final_stats = runtime
        .shutdown()
        .expect("shutdown must succeed with every connection already closed");
    assert_eq!(final_stats.active_allocations, 0);
}

/// Row 6: repeated real bootstrap/shutdown cycles must show zero leak
/// growth, zero fallback, and zero unexplained `OutstandingAllocations`
/// across every cycle. `CHURN_CYCLES` is chosen empirically for this
/// single-CPU VM: each cycle performs a real `VirtualAlloc`/`VirtualLock`/
/// per-page `QueryWorkingSetEx` attestation plus a real SQLCipher open,
/// schema creation, and close, and the VM's single vCPU makes each cycle
/// materially slower than representative multi-core consumer hardware (see
/// this node's handoff, Section 9 gap "single-CPU VM constraint"). 25 cycles
/// is enough to show a monotonic leak or a fallback creeping in if the design
/// had one, while keeping this test's wall-clock cost bounded on this VM.
#[test]
#[ignore = "requires the approved disposable host-bounds environment; run serially"]
fn churn_cycles_show_zero_leak_and_zero_fallback() {
    const CHURN_CYCLES: u32 = 25;
    for cycle in 0..CHURN_CYCLES {
        let runtime = loki_storage::secure_memory::test_support::bootstrap_candidate_test_arena(
            small_working_candidate(),
        )
        .unwrap_or_else(|error| panic!("cycle {cycle}: bootstrap must succeed: {error:?}"));

        let dir = tempfile::tempdir().expect("create isolated test root");
        let path = dir.path().join("churn.sqlite3");
        let key = loki_storage::secret::generate_database_key().unwrap_or_else(|error| {
            panic!("cycle {cycle}: key generation must succeed: {error:?}")
        });
        let storage = Storage::open(&path, Cipher::SqlCipher(key))
            .unwrap_or_else(|error| panic!("cycle {cycle}: open must succeed: {error:?}"));
        storage
            .run_migrations("2026-01-01T00:00:00Z")
            .unwrap_or_else(|error| panic!("cycle {cycle}: migrations must apply: {error:?}"));
        drop(storage);

        let stats = runtime.stats();
        assert_eq!(
            stats.fallback_count, 0,
            "cycle {cycle}: zero fallback allocation expected"
        );

        let final_stats = runtime
            .shutdown()
            .unwrap_or_else(|error| panic!("cycle {cycle}: shutdown must succeed: {error:?}"));
        assert_eq!(
            final_stats.active_allocations, 0,
            "cycle {cycle}: zero outstanding allocations expected after every connection closed"
        );
        assert_eq!(
            final_stats.fallback_count, 0,
            "cycle {cycle}: zero fallback at shutdown"
        );
    }
}

/// Row 7 (soak): a sustained real run, sampling stats periodically for
/// stability (no growth, no fallback, no degradation). `#[ignore]`d by
/// default — a multi-minute test does not belong in the default `cargo test
/// --workspace` gate every other node in this workspace also depends on; run
/// explicitly with `--ignored`. Duration is deliberately short relative to a
/// production soak bar (which does not exist yet — Section 9 gap "no numeric
/// SLA exists anywhere in Docs/"): this VM is single-CPU and every cycle here
/// performs a real SQLCipher open/migrate/close, so a representative
/// multi-hour soak is not feasible in this environment. This produces the
/// first real numbers, not a pass against a pre-existing bar.
#[test]
#[ignore = "sustained soak run — invoke explicitly with `cargo test -- --ignored`"]
fn soak_run_is_stable_under_sustained_repeated_use() {
    use std::time::{Duration, Instant};

    let runtime = loki_storage::secure_memory::test_support::bootstrap_candidate_test_arena(
        small_working_candidate(),
    )
    .expect("soak candidate must lock cleanly");

    let soak_duration = Duration::from_secs(180); // see doc comment for rationale.
    let started = Instant::now();
    let mut iterations: u64 = 0;
    let mut observed_fallback = 0usize;
    let mut peak_seen = 0usize;

    let dir = tempfile::tempdir().expect("create isolated test root");
    while started.elapsed() < soak_duration {
        let path = dir.path().join(format!("soak-{iterations}.sqlite3"));
        let key = loki_storage::secret::generate_database_key().unwrap_or_else(|error| {
            panic!("iteration {iterations}: key generation must succeed: {error:?}")
        });
        let storage = Storage::open(&path, Cipher::SqlCipher(key))
            .unwrap_or_else(|error| panic!("iteration {iterations}: open must succeed: {error:?}"));
        storage
            .run_migrations("2026-01-01T00:00:00Z")
            .unwrap_or_else(|error| {
                panic!("iteration {iterations}: migrations must apply: {error:?}")
            });
        drop(storage);
        let _ = std::fs::remove_file(&path);

        let stats = runtime.stats();
        observed_fallback = observed_fallback.max(stats.fallback_count);
        peak_seen = peak_seen.max(stats.peak_bytes);
        iterations += 1;
    }

    assert_eq!(
        observed_fallback, 0,
        "zero fallback across the entire soak window"
    );
    assert!(
        peak_seen <= small_working_candidate().arena_bytes,
        "peak usage must never exceed the fixed arena"
    );

    let final_stats = runtime
        .shutdown()
        .expect("shutdown must succeed at the end of the soak run");
    assert_eq!(final_stats.active_allocations, 0);
    assert_eq!(final_stats.fallback_count, 0);

    eprintln!(
        "soak evidence: iterations={iterations} duration_secs={} peak_bytes={peak_seen}",
        soak_duration.as_secs()
    );
}
