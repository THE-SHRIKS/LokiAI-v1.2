//! `Docs/graph/tasks/DAT-001.md` Deliverable 4: "Integrity check (e.g.,
//! `PRAGMA integrity_check`) and a backup primitive (file-level copy with a
//! documented consistency guarantee — WAL checkpoint before copy, or
//! equivalent)."

mod common;

use std::fs;

use loki_storage::devices::DeviceRepository;
use loki_storage::ids::DeviceId;
use loki_storage::{Cipher, IntegrityStatus, Storage};

#[test]
fn integrity_check_passes_on_a_healthy_freshly_migrated_database() {
    let (_dir, _path, storage) = common::open_temp_storage();
    assert_eq!(storage.integrity_check().unwrap(), IntegrityStatus::Ok);
}

#[test]
fn integrity_check_passes_after_writes() {
    let (_dir, _path, storage) = common::open_temp_storage();
    storage
        .insert_device(&common::sample_device("dev-1"))
        .unwrap();
    assert_eq!(storage.integrity_check().unwrap(), IntegrityStatus::Ok);
}

#[test]
fn backup_produces_a_fully_independent_readable_copy() {
    let (source_dir, _source_path, source) = common::open_temp_storage();
    source
        .insert_device(&common::sample_device("dev-1"))
        .unwrap();
    source
        .insert_job(&common::sample_job("job-1", "idem-1"))
        .unwrap();

    let backup_dir = tempfile::tempdir().unwrap();
    let backup_path = backup_dir.path().join("backup.db");
    source.backup_to(&backup_path).unwrap();
    assert!(backup_path.exists());

    // A brand-new, independent Storage handle on the backup path — not the
    // same in-process connection as `source` — must see the same data.
    let restored = Storage::open(&backup_path, Cipher::None).unwrap();
    assert_eq!(restored.integrity_check().unwrap(), IntegrityStatus::Ok);

    let device = restored
        .get_device(&DeviceId::new("dev-1").unwrap())
        .unwrap();
    assert!(
        device.is_some(),
        "backup must contain data written before the backup ran"
    );

    use loki_storage::jobs::JobRepository;
    let job = restored.get_job_by_idempotency_key("idem-1").unwrap();
    assert!(job.is_some());

    // The backup must be a genuinely separate file: writing to the source
    // afterward must not appear in the already-taken backup.
    source
        .insert_device(&common::sample_device("dev-2"))
        .unwrap();
    assert!(
        restored
            .get_device(&DeviceId::new("dev-2").unwrap())
            .unwrap()
            .is_none(),
        "a write to the source after backup_to() must not retroactively appear in the backup file"
    );

    drop(source_dir);
}

#[test]
fn backup_of_an_empty_freshly_migrated_database_is_still_a_valid_database() {
    let (_dir, _path, source) = common::open_temp_storage();

    let backup_dir = tempfile::tempdir().unwrap();
    let backup_path = backup_dir.path().join("backup-empty.db");
    source.backup_to(&backup_path).unwrap();

    let restored = Storage::open(&backup_path, Cipher::None).unwrap();
    assert_eq!(restored.integrity_check().unwrap(), IntegrityStatus::Ok);
    assert_eq!(restored.list_devices().unwrap().len(), 0);
}

#[test]
fn backup_file_size_is_nonzero_and_not_a_stub() {
    let (_dir, _path, source) = common::open_temp_storage();
    source
        .insert_device(&common::sample_device("dev-1"))
        .unwrap();

    let backup_dir = tempfile::tempdir().unwrap();
    let backup_path = backup_dir.path().join("backup.db");
    source.backup_to(&backup_path).unwrap();

    let metadata = fs::metadata(&backup_path).unwrap();
    assert!(
        metadata.len() > 0,
        "backup file must contain real page data, not be an empty stub"
    );
}
