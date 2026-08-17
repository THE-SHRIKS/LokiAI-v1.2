//! CRUD round-trip coverage for the three required repositories
//! (`Docs/graph/tasks/DAT-001.md` Deliverable 3): device registry, job
//! records, and the outbox/inbox pair.

mod common;

use loki_storage::StorageError;
use loki_storage::devices::DeviceRepository;
use loki_storage::ids::{CausationId, CorrelationId, DeviceId, JobId, MessageId};
use loki_storage::inbox::{InboxRepository, NewInboxMessage};
use loki_storage::jobs::JobRepository;
use loki_storage::outbox::{NewOutboxMessage, OutboxRepository};
use loki_storage::states::{
    DeviceLifecycleState, InboxStatus, JobState, OutboxStatus, SupportTier,
};

// --- devices -------------------------------------------------------------------------

#[test]
fn device_insert_get_list_and_lifecycle_update_round_trip() {
    let (_dir, _path, storage) = common::open_temp_storage();

    assert!(
        storage
            .get_device(&DeviceId::new("dev-1").unwrap())
            .unwrap()
            .is_none()
    );
    assert_eq!(storage.list_devices().unwrap().len(), 0);

    let device = common::sample_device("dev-1");
    storage.insert_device(&device).unwrap();

    let fetched = storage.get_device(&device.device_id).unwrap().unwrap();
    assert_eq!(fetched.device_id, device.device_id);
    assert_eq!(fetched.connector_id, device.connector_id);
    assert_eq!(fetched.support_tier, SupportTier::Certified);
    assert_eq!(fetched.lifecycle_state, DeviceLifecycleState::Discovered);
    assert_eq!(fetched.board_identity.as_deref(), Some("test-board-rev-a"));
    assert_eq!(
        fetched.firmware_artifact_ref, None,
        "an unobserved field must round-trip as None, not a fabricated value"
    );

    storage
        .update_device_lifecycle_state(
            &device.device_id,
            DeviceLifecycleState::Ready,
            common::T1,
            common::T1,
        )
        .unwrap();
    let updated = storage.get_device(&device.device_id).unwrap().unwrap();
    assert_eq!(updated.lifecycle_state, DeviceLifecycleState::Ready);
    assert_eq!(updated.last_seen_at, common::T1);

    storage
        .insert_device(&common::sample_device("dev-2"))
        .unwrap();
    let mut all = storage.list_devices().unwrap();
    all.sort_by(|a, b| a.device_id.as_str().cmp(b.device_id.as_str()));
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].device_id.as_str(), "dev-1");
    assert_eq!(all[1].device_id.as_str(), "dev-2");
}

#[test]
fn updating_lifecycle_state_of_unknown_device_is_not_found() {
    let (_dir, _path, storage) = common::open_temp_storage();
    let err = storage
        .update_device_lifecycle_state(
            &DeviceId::new("does-not-exist").unwrap(),
            DeviceLifecycleState::Ready,
            common::T0,
            common::T0,
        )
        .unwrap_err();
    assert!(matches!(err, StorageError::NotFound { kind: "device", .. }));
}

#[test]
fn device_insert_rejects_invalid_timestamp_before_writing_anything() {
    let (_dir, _path, storage) = common::open_temp_storage();
    let mut device = common::sample_device("dev-1");
    device.last_seen_at = "not-a-timestamp".to_string();

    let err = storage.insert_device(&device).unwrap_err();
    assert!(matches!(err, StorageError::InvalidTimestamp { .. }));
    assert!(
        storage.get_device(&device.device_id).unwrap().is_none(),
        "a rejected insert must not partially write"
    );
}

// --- jobs ----------------------------------------------------------------------------

#[test]
fn job_insert_transition_complete_round_trip() {
    let (_dir, _path, storage) = common::open_temp_storage();

    let job = common::sample_job("job-1", "idem-1");
    storage.insert_job(&job).unwrap();

    let fetched = storage.get_job(&job.job_id).unwrap().unwrap();
    assert_eq!(fetched.state, JobState::Queued);
    assert_eq!(fetched.result_json, None);

    storage
        .transition_job_state(&job.job_id, JobState::Executing, common::T1)
        .unwrap();
    assert_eq!(
        storage.get_job(&job.job_id).unwrap().unwrap().state,
        JobState::Executing
    );

    storage
        .complete_job(
            &job.job_id,
            Some(r#"["evidence-1"]"#),
            Some(r#"{"outcome":"ok"}"#),
            common::T1,
        )
        .unwrap();
    let completed = storage.get_job(&job.job_id).unwrap().unwrap();
    assert_eq!(completed.state, JobState::Succeeded);
    assert_eq!(
        completed.evidence_refs_json.as_deref(),
        Some(r#"["evidence-1"]"#)
    );
    assert_eq!(
        completed.result_json.as_deref(),
        Some(r#"{"outcome":"ok"}"#)
    );

    let by_key = storage
        .get_job_by_idempotency_key("idem-1")
        .unwrap()
        .unwrap();
    assert_eq!(by_key.job_id, job.job_id);
}

#[test]
fn job_fail_records_error_and_terminal_state() {
    let (_dir, _path, storage) = common::open_temp_storage();
    let job = common::sample_job("job-1", "idem-1");
    storage.insert_job(&job).unwrap();

    storage
        .fail_job(&job.job_id, r#"{"code":"device_unavailable"}"#, common::T1)
        .unwrap();
    let failed = storage.get_job(&job.job_id).unwrap().unwrap();
    assert_eq!(failed.state, JobState::Failed);
    assert_eq!(
        failed.error_json.as_deref(),
        Some(r#"{"code":"device_unavailable"}"#)
    );
}

#[test]
fn list_jobs_by_state_filters_correctly() {
    let (_dir, _path, storage) = common::open_temp_storage();
    storage
        .insert_job(&common::sample_job("job-1", "idem-1"))
        .unwrap();
    storage
        .insert_job(&common::sample_job("job-2", "idem-2"))
        .unwrap();
    storage
        .transition_job_state(
            &JobId::new("job-2").unwrap(),
            JobState::Executing,
            common::T1,
        )
        .unwrap();

    assert_eq!(
        storage.list_jobs_by_state(JobState::Queued).unwrap().len(),
        1
    );
    assert_eq!(
        storage
            .list_jobs_by_state(JobState::Executing)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        storage.list_jobs_by_state(JobState::Failed).unwrap().len(),
        0
    );
}

#[test]
fn transitioning_unknown_job_is_not_found() {
    let (_dir, _path, storage) = common::open_temp_storage();
    let err = storage
        .transition_job_state(
            &JobId::new("does-not-exist").unwrap(),
            JobState::Executing,
            common::T0,
        )
        .unwrap_err();
    assert!(matches!(err, StorageError::NotFound { kind: "job", .. }));
}

// --- outbox ----------------------------------------------------------------------------

#[test]
fn outbox_enqueue_is_idempotent_by_message_id() {
    let (_dir, _path, storage) = common::open_temp_storage();
    let msg = NewOutboxMessage {
        message_id: MessageId::new("msg-1").unwrap(),
        stream_key: "dev-1".to_string(),
        message_type: "device.discovered".to_string(),
        correlation_id: CorrelationId::new("corr-1").unwrap(),
        causation_id: Some(CausationId::new("cause-1").unwrap()),
        payload_json: r#"{"a":1}"#.to_string(),
        created_at: common::T0.to_string(),
        updated_at: common::T0.to_string(),
    };

    assert!(
        storage.enqueue_outbox_message(&msg).unwrap(),
        "first enqueue must be a new row"
    );
    assert!(
        !storage.enqueue_outbox_message(&msg).unwrap(),
        "second enqueue of the same message_id must be a no-op"
    );

    let pending = storage
        .list_outbox_messages_by_status(OutboxStatus::Pending, 10)
        .unwrap();
    assert_eq!(pending.len(), 1, "must not duplicate the row");
    assert_eq!(
        pending[0].causation_id.as_ref().map(CausationId::as_str),
        Some("cause-1")
    );
}

#[test]
fn outbox_mark_sent_and_mark_failed_transition_status() {
    let (_dir, _path, storage) = common::open_temp_storage();
    let id = MessageId::new("msg-1").unwrap();
    storage
        .enqueue_outbox_message(&NewOutboxMessage {
            message_id: id.clone(),
            stream_key: "dev-1".to_string(),
            message_type: "device.discovered".to_string(),
            correlation_id: CorrelationId::new("corr-1").unwrap(),
            causation_id: None,
            payload_json: "{}".to_string(),
            created_at: common::T0.to_string(),
            updated_at: common::T0.to_string(),
        })
        .unwrap();

    storage
        .mark_outbox_message_sent(&id, common::T1, common::T1)
        .unwrap();
    let sent = storage.get_outbox_message(&id).unwrap().unwrap();
    assert_eq!(sent.status, OutboxStatus::Sent);
    assert_eq!(sent.dispatched_at.as_deref(), Some(common::T1));
    assert_eq!(
        storage
            .list_outbox_messages_by_status(OutboxStatus::Pending, 10)
            .unwrap()
            .len(),
        0
    );

    let id2 = MessageId::new("msg-2").unwrap();
    storage
        .enqueue_outbox_message(&NewOutboxMessage {
            message_id: id2.clone(),
            stream_key: "dev-1".to_string(),
            message_type: "device.discovered".to_string(),
            correlation_id: CorrelationId::new("corr-1").unwrap(),
            causation_id: None,
            payload_json: "{}".to_string(),
            created_at: common::T0.to_string(),
            updated_at: common::T0.to_string(),
        })
        .unwrap();
    storage
        .mark_outbox_message_failed(&id2, "connection refused", common::T1)
        .unwrap();
    let failed = storage.get_outbox_message(&id2).unwrap().unwrap();
    assert_eq!(failed.status, OutboxStatus::Failed);
    assert_eq!(failed.attempts, 1);
    assert_eq!(failed.last_error.as_deref(), Some("connection refused"));
}

#[test]
fn marking_unknown_outbox_message_sent_is_not_found() {
    let (_dir, _path, storage) = common::open_temp_storage();
    let err = storage
        .mark_outbox_message_sent(&MessageId::new("nope").unwrap(), common::T0, common::T0)
        .unwrap_err();
    assert!(matches!(
        err,
        StorageError::NotFound {
            kind: "outbox message",
            ..
        }
    ));
}

// --- inbox ----------------------------------------------------------------------------

#[test]
fn inbox_record_is_idempotent_by_message_id_and_tracks_processing() {
    let (_dir, _path, storage) = common::open_temp_storage();
    let msg = NewInboxMessage {
        message_id: MessageId::new("cloud-msg-1").unwrap(),
        source: "lokiai-cloud".to_string(),
        message_type: "policy.updated".to_string(),
        correlation_id: Some(CorrelationId::new("corr-9").unwrap()),
        payload_json: Some(r#"{"policy_version":2}"#.to_string()),
        received_at: common::T0.to_string(),
    };

    assert!(
        storage.record_inbox_message(&msg).unwrap(),
        "first receipt must be new"
    );
    assert!(
        !storage.record_inbox_message(&msg).unwrap(),
        "re-delivery of the same message_id must be a no-op"
    );

    assert!(
        !storage
            .was_inbox_message_processed(&msg.message_id)
            .unwrap()
    );

    storage
        .mark_inbox_message_processed(&msg.message_id, common::T1)
        .unwrap();
    assert!(
        storage
            .was_inbox_message_processed(&msg.message_id)
            .unwrap()
    );

    let record = storage.get_inbox_message(&msg.message_id).unwrap().unwrap();
    assert_eq!(record.status, InboxStatus::Processed);
    assert_eq!(record.processed_at.as_deref(), Some(common::T1));
}

#[test]
fn unreceived_inbox_message_is_not_processed() {
    let (_dir, _path, storage) = common::open_temp_storage();
    assert!(
        !storage
            .was_inbox_message_processed(&MessageId::new("never-seen").unwrap())
            .unwrap()
    );
}

#[test]
fn marking_unknown_inbox_message_processed_is_not_found() {
    let (_dir, _path, storage) = common::open_temp_storage();
    let err = storage
        .mark_inbox_message_processed(&MessageId::new("nope").unwrap(), common::T0)
        .unwrap_err();
    assert!(matches!(
        err,
        StorageError::NotFound {
            kind: "inbox message",
            ..
        }
    ));
}
