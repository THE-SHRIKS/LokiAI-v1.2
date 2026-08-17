//! `Docs/graph/tasks/DAT-001.md` Deliverable 5: "a transaction that fails
//! mid-way rolls back completely (no partial write observable)."
//!
//! This crate's cross-repository transaction primitive
//! (`Storage::transaction`, wrapping `rusqlite::Transaction`) is
//! intentionally `pub(crate)`, not part of the public API yet (see
//! `Docs/graph/handoffs/DAT-001-1.md` §7) — so the most direct,
//! multi-`INSERT`-statement proof of mid-transaction rollback lives as
//! `#[cfg(test)]` unit tests colocated with that code:
//!
//! - `src/storage.rs::tests::failed_multi_step_transaction_leaves_no_partial_write`
//!   — two `INSERT`s in one transaction, the second violates a UNIQUE
//!   index, asserts zero rows remain (not one).
//! - `src/migrations.rs::tests::broken_migration_rolls_back_completely` and
//!   `::earlier_committed_migration_survives_a_later_one_failing` — proves
//!   the migration runner's per-migration transaction (DDL + its own
//!   `schema_migrations` bookkeeping row) is atomic.
//!
//! This file complements those with black-box, public-API-only proof that
//! a single failed write is atomic and leaves the store exactly as it was
//! (SQLite's own per-statement guarantee, exercised through the repository
//! API real callers use).

mod common;

use loki_storage::devices::DeviceRepository;
use loki_storage::jobs::JobRepository;

#[test]
fn failed_device_insert_does_not_corrupt_the_existing_row() {
    let (_dir, _path, storage) = common::open_temp_storage();

    let mut first = common::sample_device("dev-1");
    first.lifecycle_state = loki_storage::states::DeviceLifecycleState::Ready;
    storage.insert_device(&first).unwrap();

    // Same connector_id + hardware_identity as `first` (via the same
    // device id substituted into `sample_device`'s fixture format) but a
    // different primary key — violates the UNIQUE(connector_id,
    // hardware_identity) index from migrations/0001_devices.sql.
    let mut conflicting = common::sample_device("dev-1");
    conflicting.device_id = loki_storage::ids::DeviceId::new("dev-1-duplicate-hardware").unwrap();
    // `sample_device` derives hardware_identity from the id argument, so
    // force it back to collide with `first`'s.
    conflicting.hardware_identity = first.hardware_identity.clone();

    let result = storage.insert_device(&conflicting);
    assert!(
        result.is_err(),
        "a duplicate connector-qualified hardware identity must be rejected"
    );

    // The original row must be completely unaffected — not merely "still
    // present" but unchanged in every field a partial/torn write could
    // have corrupted.
    let reread = storage
        .get_device(&first.device_id)
        .unwrap()
        .expect("original device must still exist");
    assert_eq!(
        reread.lifecycle_state,
        loki_storage::states::DeviceLifecycleState::Ready
    );
    assert_eq!(reread.hardware_identity, first.hardware_identity);

    // And the rejected duplicate must not exist under its own id either.
    assert!(
        storage
            .get_device(&conflicting.device_id)
            .unwrap()
            .is_none()
    );

    assert_eq!(
        storage.list_devices().unwrap().len(),
        1,
        "exactly the one successfully-inserted device"
    );
}

#[test]
fn failed_job_insert_does_not_leave_a_partial_row() {
    let (_dir, _path, storage) = common::open_temp_storage();

    storage
        .insert_job(&common::sample_job("job-1", "shared-idempotency-key"))
        .unwrap();

    let duplicate_key_result =
        storage.insert_job(&common::sample_job("job-2", "shared-idempotency-key"));
    assert!(
        duplicate_key_result.is_err(),
        "a duplicate idempotency_key must be rejected"
    );

    // "job-2" must not exist at all — not half-written, not present with
    // default/placeholder values.
    assert!(
        storage
            .get_job(&loki_storage::ids::JobId::new("job-2").unwrap())
            .unwrap()
            .is_none()
    );

    // "job-1" (the legitimate row sharing that idempotency key) is
    // unaffected.
    let job1 = storage
        .get_job_by_idempotency_key("shared-idempotency-key")
        .unwrap()
        .unwrap();
    assert_eq!(job1.job_id.as_str(), "job-1");
}
