//! Black-box coverage of `Storage::run_migrations` through the public API
//! (the internal runner already has granular unit tests in
//! `src/migrations.rs`; these prove the same guarantees hold end-to-end
//! through `Storage::open`, which is what production code actually calls).
//!
//! `Docs/graph/tasks/DAT-001.md` Deliverable 5: "migration applies cleanly
//! on empty DB; migration is idempotent (running twice doesn't fail/
//! duplicate)."

mod common;

use loki_storage::Storage;
use loki_storage::devices::DeviceRepository;
use loki_storage::inbox::InboxRepository;
use loki_storage::jobs::JobRepository;
use loki_storage::outbox::OutboxRepository;
use loki_storage::states::{JobState, OutboxStatus};

#[test]
fn migration_applies_cleanly_on_an_empty_db() {
    let (_dir, path, storage) = common::open_temp_storage();
    assert!(
        path.exists(),
        "Storage::open must create the file on first use"
    );
    assert_eq!(
        storage.integrity_check().unwrap(),
        loki_storage::IntegrityStatus::Ok
    );

    // Every table the three embedded migrations create must actually be
    // present and queryable through its repository — not merely "no error
    // was raised by run_migrations itself."
    assert_eq!(storage.list_devices().unwrap().len(), 0);
    assert_eq!(
        storage.list_jobs_by_state(JobState::Queued).unwrap().len(),
        0
    );
    assert_eq!(
        storage
            .list_outbox_messages_by_status(OutboxStatus::Pending, 10)
            .unwrap()
            .len(),
        0
    );
    assert!(
        storage
            .get_inbox_message(&loki_storage::ids::MessageId::new("nonexistent").unwrap())
            .unwrap()
            .is_none()
    );

    // A second, independent handle onto the same file confirms the schema
    // was durably written to disk, not only visible to the writer's own
    // in-process connection.
    let second_handle = Storage::open(&path, common::cipher_for(&path)).unwrap();
    assert_eq!(
        second_handle.integrity_check().unwrap(),
        loki_storage::IntegrityStatus::Ok
    );
    assert_eq!(second_handle.list_devices().unwrap().len(), 0);
}

#[test]
fn running_migrations_twice_is_idempotent() {
    let (_dir, _path, storage) = common::open_temp_storage();

    // `open_temp_storage` already called `run_migrations` once. Calling it
    // again must be a clean no-op, not an error and not a duplicate.
    let second = storage.run_migrations(common::T1).unwrap();
    assert!(
        second.is_up_to_date(),
        "second call must find nothing new to apply"
    );

    let third = storage.run_migrations(common::T1).unwrap();
    assert!(third.is_up_to_date());
}

#[test]
fn fresh_storage_reports_which_versions_it_applied() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fresh.db");
    let storage = Storage::open(&path, common::cipher_for(&path)).unwrap();

    let report = storage.run_migrations(common::T0).unwrap();
    assert_eq!(
        report.applied_versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        "all nine embedded migrations must apply in order"
    );
    assert!(!report.is_up_to_date());
}
