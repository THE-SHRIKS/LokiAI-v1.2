//! `Docs/graph/tasks/DAT-001.md` Deliverable 5: "a corrupted DB file is
//! detected, not silently accepted."
//!
//! Also proves the two adjacent honest cases so the corruption check can't
//! be satisfied by simply rejecting every pre-existing file: a path that
//! does not exist yet is a legitimate first run, and a zero-byte file is a
//! legitimate empty SQLite database (not corruption) by SQLite's own
//! on-disk format rules.

mod common;

use std::fs;

use loki_storage::devices::DeviceRepository;
use loki_storage::{Storage, StorageError};

#[test]
fn garbage_file_is_detected_as_corrupt_not_silently_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt.db");
    fs::write(
        &path,
        b"this is definitely not a valid sqlite database, just garbage bytes 0123456789",
    )
    .unwrap();

    let result = Storage::open(&path, common::cipher_for(&path));
    match result {
        Err(StorageError::Corrupt {
            path: reported_path,
            ..
        }) => {
            assert!(reported_path.contains("corrupt.db"));
        }
        other => panic!("expected StorageError::Corrupt for a garbage file, got {other:?}"),
    }
}

#[test]
fn truncated_sqlite_header_is_detected_as_corrupt() {
    // Build a real database, then truncate it to a handful of bytes — a
    // more realistic corruption scenario than pure random bytes (e.g. a
    // crash mid-write to a filesystem without atomic rename semantics).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("truncated.db");
    {
        let storage = Storage::open(&path, common::cipher_for(&path)).unwrap();
        storage.run_migrations(common::T0).unwrap();
        storage
            .insert_device(&common::sample_device("dev-1"))
            .unwrap();
    }

    let bytes = fs::read(&path).unwrap();
    assert!(
        bytes.len() > 32,
        "precondition: the real database must be larger than our truncation point"
    );
    fs::write(&path, &bytes[..16]).unwrap();

    let result = Storage::open(&path, common::cipher_for(&path));
    assert!(
        matches!(result, Err(StorageError::Corrupt { .. })),
        "truncated file must be detected as corrupt"
    );
}

#[test]
fn missing_path_creates_a_fresh_database_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("does-not-exist-yet.db");
    assert!(!path.exists());

    let storage = Storage::open(&path, common::cipher_for(&path))
        .expect("opening a non-existent path must create a fresh database");
    let report = storage.run_migrations(common::T0).unwrap();
    assert!(path.exists());
    assert_eq!(report.applied_versions, vec![1, 2, 3]);
}

#[test]
fn zero_byte_file_is_a_legitimate_empty_database_not_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.db");
    fs::write(&path, b"").unwrap();

    let storage = Storage::open(&path, common::cipher_for(&path))
        .expect("a zero-byte file is a valid empty SQLite database, not corruption");
    storage.run_migrations(common::T0).unwrap();
    assert_eq!(
        storage.integrity_check().unwrap(),
        loki_storage::IntegrityStatus::Ok
    );
}
