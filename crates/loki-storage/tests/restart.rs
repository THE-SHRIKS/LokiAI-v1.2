//! `Docs/graph/tasks/DAT-001.md` Deliverable 5: "a restart with existing
//! data preserves it."

mod common;

use loki_storage::devices::DeviceRepository;
use loki_storage::ids::DeviceId;
use loki_storage::jobs::JobRepository;
use loki_storage::outbox::{NewOutboxMessage, OutboxRepository};
use loki_storage::states::{DeviceLifecycleState, OutboxStatus};
use loki_storage::{Cipher, Storage};

#[test]
fn restart_with_existing_data_preserves_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("restart.db");

    {
        // "Process 1": open, migrate, write, then drop (simulates a clean
        // process exit — the `Connection` inside `Storage` closes when
        // `storage` is dropped at the end of this block).
        let storage = Storage::open(&path, Cipher::None).unwrap();
        storage.run_migrations(common::T0).unwrap();
        storage
            .insert_device(&common::sample_device("dev-1"))
            .unwrap();
        storage
            .insert_job(&common::sample_job("job-1", "idem-1"))
            .unwrap();
        storage
            .enqueue_outbox_message(&NewOutboxMessage {
                message_id: loki_storage::ids::MessageId::new("msg-1").unwrap(),
                stream_key: "dev-1".to_string(),
                message_type: "device.discovered".to_string(),
                correlation_id: loki_storage::ids::CorrelationId::new("corr-1").unwrap(),
                causation_id: None,
                payload_json: r#"{"ok":true}"#.to_string(),
                created_at: common::T0.to_string(),
                updated_at: common::T0.to_string(),
            })
            .unwrap();
        storage
            .update_device_lifecycle_state(
                &DeviceId::new("dev-1").unwrap(),
                DeviceLifecycleState::Identified,
                common::T1,
                common::T1,
            )
            .unwrap();
    }

    // "Process 2": open the same path fresh.
    let storage = Storage::open(&path, Cipher::None).unwrap();

    // Re-running migrations on restart must be a no-op, not a re-apply.
    let report = storage.run_migrations(common::T1).unwrap();
    assert!(
        report.is_up_to_date(),
        "restart must not re-apply already-applied migrations"
    );

    let device = storage
        .get_device(&DeviceId::new("dev-1").unwrap())
        .unwrap()
        .expect("device inserted before restart must still be present");
    assert_eq!(
        device.lifecycle_state,
        DeviceLifecycleState::Identified,
        "the update before restart must persist too"
    );
    assert_eq!(device.last_seen_at, common::T1);

    let job = storage
        .get_job_by_idempotency_key("idem-1")
        .unwrap()
        .expect("job inserted before restart must still be present");
    assert_eq!(job.job_id.as_str(), "job-1");

    let outbox_messages = storage
        .list_outbox_messages_by_status(OutboxStatus::Pending, 10)
        .unwrap();
    assert_eq!(
        outbox_messages.len(),
        1,
        "outbox message inserted before restart must still be present"
    );
    assert_eq!(outbox_messages[0].message_id.as_str(), "msg-1");

    assert_eq!(
        storage.integrity_check().unwrap(),
        loki_storage::IntegrityStatus::Ok
    );
}

#[test]
fn restart_across_more_than_two_opens_keeps_accumulating_data_correctly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("multi-restart.db");

    for i in 1..=3 {
        let storage = Storage::open(&path, Cipher::None).unwrap();
        storage.run_migrations(common::T0).unwrap();
        storage
            .insert_device(&common::sample_device(&format!("dev-{i}")))
            .unwrap();
        assert_eq!(
            storage.list_devices().unwrap().len(),
            i,
            "device count after open #{i}"
        );
    }

    let storage = Storage::open(&path, Cipher::None).unwrap();
    assert_eq!(storage.list_devices().unwrap().len(), 3);
}
