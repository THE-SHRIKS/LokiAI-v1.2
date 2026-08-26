mod support;
use loki_build_executor::ErrorCode;
use loki_build_executor::broker::CapsuleBroker;
use loki_build_executor::journal::{Journal, JournalEntry, JournalEntryDraft, MemoryJournal};
use loki_build_executor::lifecycle::CapsuleState;
use loki_build_executor::native::HostOperationsDisabled;
use loki_build_executor::protocol::{Authenticator, BrokerAction, MAX_MESSAGE_BYTES, MAX_NONCES};
use support::*;
fn broker() -> CapsuleBroker<MemoryJournal, HostOperationsDisabled> {
    CapsuleBroker::new(
        Authenticator::new(&KEY).unwrap(),
        MemoryJournal::default(),
        HostOperationsDisabled,
    )
    .unwrap()
}
#[test]
fn malformed_unknown_and_oversized_messages_fail_before_native_boundary() {
    assert_eq!(
        broker().handle(b"not-json", 5).unwrap_err().code,
        ErrorCode::Malformed
    );
    assert_eq!(
        broker()
            .handle(br#"{"version":1,"unknown":true}"#, 5)
            .unwrap_err()
            .code,
        ErrorCode::Malformed
    );
    assert_eq!(
        broker()
            .handle(&vec![b'x'; MAX_MESSAGE_BYTES + 1], 5)
            .unwrap_err()
            .code,
        ErrorCode::Oversized
    );
}
#[test]
fn bad_mac_expiry_and_replay_fail_closed() {
    let mut message = signed(BrokerAction::Create, "nonce-1", 10, 20);
    message.mac.replace_range(..2, "00");
    assert_eq!(
        broker().handle(&bytes(&message), 15).unwrap_err().code,
        ErrorCode::Authentication
    );
    let expired = signed(BrokerAction::Create, "nonce-2", 10, 20);
    assert_eq!(
        broker().handle(&bytes(&expired), 21).unwrap_err().code,
        ErrorCode::Expired
    );
    let replay = signed(BrokerAction::Create, "nonce-3", 10, 20);
    let raw = bytes(&replay);
    let mut instance = broker();
    let _ = instance.handle(&raw, 15);
    assert_eq!(
        instance.handle(&raw, 15).unwrap_err().code,
        ErrorCode::Replay
    );
}

#[test]
fn replay_cache_saturation_fails_closed_without_evicting_live_nonces() {
    let verifier = Authenticator::new(&KEY).unwrap();
    for index in 0..MAX_NONCES {
        let raw = bytes(&signed(
            BrokerAction::Create,
            &format!("saturation-{index}"),
            1,
            60_000,
        ));
        verifier.verify_and_consume(&raw, 2).unwrap();
    }
    let overflow = bytes(&signed(
        BrokerAction::Create,
        "saturation-overflow",
        1,
        60_000,
    ));
    assert_eq!(
        verifier.verify_and_consume(&overflow, 2).unwrap_err().code,
        ErrorCode::Authentication
    );
    let first = bytes(&signed(BrokerAction::Create, "saturation-0", 1, 60_000));
    assert_eq!(
        verifier.verify_and_consume(&first, 2).unwrap_err().code,
        ErrorCode::Replay
    );
}
#[test]
fn arbitrary_authority_fields_are_rejected_by_closed_schema() {
    let mut value = serde_json::to_value(signed(BrokerAction::Create, "nonce-4", 10, 20)).unwrap();
    let request = value.get_mut("request").unwrap().as_object_mut().unwrap();
    for field in ["path", "command", "executable", "url", "environment"] {
        request.insert(
            field.into(),
            serde_json::Value::String("host-controlled".into()),
        );
    }
    assert_eq!(
        broker()
            .handle(&serde_json::to_vec(&value).unwrap(), 15)
            .unwrap_err()
            .code,
        ErrorCode::Malformed
    );
}
#[test]
fn bounds_and_required_post_create_volume_identity_are_enforced() {
    let mut req = request(BrokerAction::Attach);
    req.expected_volume_id = None;
    assert_eq!(
        req.validate().unwrap_err().code,
        ErrorCode::IdentityMismatch
    );
    let mut req = request(BrokerAction::Create);
    req.limits.output_bytes = req.limits.disk_bytes + 1;
    assert_eq!(req.validate().unwrap_err().code, ErrorCode::Malformed);
}

#[test]
fn non_create_first_action_is_rejected_without_poisoning_identity() {
    let mut instance = CapsuleBroker::new(
        Authenticator::new(&KEY).unwrap(),
        MemoryJournal::default(),
        RecordingApi::default(),
    )
    .unwrap();
    let attach = bytes(&signed(BrokerAction::Attach, "first-attach", 10, 20));
    assert_eq!(
        instance.handle(&attach, 15).unwrap_err().code,
        ErrorCode::InvalidTransition
    );
    let create = bytes(&signed(BrokerAction::Create, "then-create", 10, 20));
    assert!(instance.handle(&create, 15).is_ok());
}

#[test]
fn deserialization_cannot_bypass_identifier_or_digest_validation() {
    let mut value =
        serde_json::to_value(signed(BrokerAction::Create, "nonce-ids", 10, 20)).unwrap();
    value["request"]["job_id"] = serde_json::Value::String("../host".into());
    assert_eq!(
        broker()
            .handle(&serde_json::to_vec(&value).unwrap(), 15)
            .unwrap_err()
            .code,
        ErrorCode::Malformed
    );
    let mut value =
        serde_json::to_value(signed(BrokerAction::Create, "nonce-digest", 10, 20)).unwrap();
    value["request"]["sealed_base_digest"] = serde_json::Value::String("not-a-digest".into());
    assert_eq!(
        broker()
            .handle(&serde_json::to_vec(&value).unwrap(), 15)
            .unwrap_err()
            .code,
        ErrorCode::Malformed
    );
}
#[test]
fn cross_job_and_wrong_volume_substitution_are_rejected() {
    let mut instance = CapsuleBroker::new(
        Authenticator::new(&KEY).unwrap(),
        MemoryJournal::default(),
        RecordingApi::default(),
    )
    .unwrap();
    assert!(
        instance
            .handle(&bytes(&signed(BrokerAction::Create, "nonce-a", 10, 20)), 15)
            .is_ok()
    );
    let signer = Authenticator::new(&KEY).unwrap();
    let mut wrong_job = request(BrokerAction::Attach);
    wrong_job.job_id = id("job-2");
    let wrong_job = signer.sign(id("nonce-b"), 10, 20, wrong_job).unwrap();
    assert_eq!(
        instance.handle(&bytes(&wrong_job), 15).unwrap_err().code,
        ErrorCode::IdentityMismatch
    );
    let signer = Authenticator::new(&KEY).unwrap();
    let mut wrong_volume = request(BrokerAction::Attach);
    wrong_volume.expected_volume_id = Some(id("volume-2"));
    let wrong_volume = signer.sign(id("nonce-c"), 10, 20, wrong_volume).unwrap();
    assert_eq!(
        instance.handle(&bytes(&wrong_volume), 15).unwrap_err().code,
        ErrorCode::IdentityMismatch
    );
}

#[test]
fn appcontainer_sid_digest_substitution_is_rejected() {
    let mut instance = CapsuleBroker::new(
        Authenticator::new(&KEY).unwrap(),
        MemoryJournal::default(),
        RecordingApi::default(),
    )
    .unwrap();
    assert!(
        instance
            .handle(
                &bytes(&signed(BrokerAction::Create, "sid-nonce-a", 10, 20)),
                15
            )
            .is_ok()
    );
    let signer = Authenticator::new(&KEY).unwrap();
    let mut wrong_sid = request(BrokerAction::PrepareAcl);
    wrong_sid.appcontainer_sid_digest = digest('9');
    let wrong_sid = signer.sign(id("sid-nonce-b"), 10, 20, wrong_sid).unwrap();
    assert_eq!(
        instance.handle(&bytes(&wrong_sid), 15).unwrap_err().code,
        ErrorCode::IdentityMismatch
    );
}

/// Simulates a process crash between the journaled *intent* for an action and
/// the recorded *completion*/native call, for each of the three actions whose
/// intent and complete lifecycle states coincide (`target_states` maps them
/// to the same `CapsuleState`). Reconstructing a fresh broker from such a
/// journal must never short-circuit to success on the next request without
/// invoking the native operation; it must quarantine instead.
fn crashed_journal_never_reports_silent_success(
    setup: impl FnOnce(&mut CapsuleBroker<MemoryJournal, RecordingApi>, u64),
    dangling_intent_state: CapsuleState,
    retry_action: BrokerAction,
    retry_nonce: &str,
) {
    let mut broker = CapsuleBroker::new(
        Authenticator::new(&KEY).unwrap(),
        MemoryJournal::default(),
        RecordingApi::default(),
    )
    .unwrap();
    setup(&mut broker, 15);
    let (mut journal, _first_api) = broker.into_parts();
    let entries = journal.entries().unwrap();
    let last = entries.last().unwrap().clone();
    // Journal only the *intent* for the next action, as `handle` does before
    // calling `apply`, then stop -- modeling a crash before the native call's
    // result was ever recorded.
    let dangling = JournalEntry::new(JournalEntryDraft {
        sequence: entries.len() as u64,
        job_id: last.job_id.clone(),
        capsule_id: last.capsule_id.clone(),
        sealed_base_digest: last.sealed_base_digest.clone(),
        appcontainer_sid_digest: last.appcontainer_sid_digest.clone(),
        volume_id: last.volume_id.clone(),
        state: dangling_intent_state,
        intent: true,
        timestamp_ms: 16,
        previous_hash: last.entry_hash.clone(),
        detail: "crash simulation: intent journaled, native completion never recorded".to_string(),
    })
    .unwrap();
    journal.append_durable(dangling).unwrap();

    // Reconstruct a fresh broker over the crashed journal with a fresh,
    // empty-ledger native adapter -- if the broker silently treated the
    // dangling intent as complete, it would either return `Ok` here without
    // calling the adapter, or it would call the adapter a second time as if
    // nothing had happened. Neither is acceptable: it must fail closed.
    let mut recovered = CapsuleBroker::new(
        Authenticator::new(&KEY).unwrap(),
        journal,
        RecordingApi::default(),
    )
    .unwrap();
    let retry = bytes(&signed(retry_action, retry_nonce, 10, 20));
    let outcome = recovered.handle(&retry, 15);
    assert!(
        outcome.is_err(),
        "reconstruction from a dangling intent must never report success"
    );
    assert_eq!(outcome.unwrap_err().code, ErrorCode::Quarantined);
    let (_journal, api) = recovered.into_parts();
    assert!(
        api.operations.is_empty(),
        "the native operation must not be silently assumed to have already succeeded, \
         nor blindly re-invoked without explicit reconciliation"
    );
}

#[test]
fn crash_between_authorize_execution_intent_and_completion_quarantines_on_reconstruction() {
    crashed_journal_never_reports_silent_success(
        |broker, now| {
            for (action, nonce) in [
                (BrokerAction::Create, "crash-ae-create"),
                (BrokerAction::Attach, "crash-ae-attach"),
                (BrokerAction::Mount, "crash-ae-mount"),
                (BrokerAction::PrepareAcl, "crash-ae-acl"),
            ] {
                assert!(
                    broker
                        .handle(&bytes(&signed(action, nonce, 10, 20)), now)
                        .is_ok()
                );
            }
        },
        CapsuleState::Executing,
        BrokerAction::AuthorizeExecution,
        "crash-ae-retry",
    );
}

#[test]
fn crash_between_quarantine_output_intent_and_completion_quarantines_on_reconstruction() {
    crashed_journal_never_reports_silent_success(
        |broker, now| {
            for (action, nonce) in [
                (BrokerAction::Create, "crash-qo-create"),
                (BrokerAction::Attach, "crash-qo-attach"),
                (BrokerAction::Mount, "crash-qo-mount"),
                (BrokerAction::PrepareAcl, "crash-qo-acl"),
                (BrokerAction::AuthorizeExecution, "crash-qo-exec"),
            ] {
                assert!(
                    broker
                        .handle(&bytes(&signed(action, nonce, 10, 20)), now)
                        .is_ok()
                );
            }
        },
        CapsuleState::OutputQuarantined,
        BrokerAction::QuarantineOutput,
        "crash-qo-retry",
    );
}

#[test]
fn crash_between_reconcile_intent_and_completion_quarantines_on_reconstruction() {
    crashed_journal_never_reports_silent_success(
        |broker, now| {
            assert!(
                broker
                    .handle(
                        &bytes(&signed(BrokerAction::Create, "crash-rc-create", 10, 20)),
                        now
                    )
                    .is_ok()
            );
        },
        CapsuleState::Quarantined,
        BrokerAction::Reconcile,
        "crash-rc-retry",
    );
}
