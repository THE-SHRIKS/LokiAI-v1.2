mod support;
use loki_build_executor::ErrorCode;
use loki_build_executor::broker::CapsuleBroker;
use loki_build_executor::journal::{Journal, MemoryJournal};
use loki_build_executor::lifecycle::CapsuleState;
use loki_build_executor::native::HostOperationsDisabled;
use loki_build_executor::protocol::{Authenticator, BrokerAction};
use support::*;
#[test]
fn production_code_node_adapter_denies_every_host_operation_and_quarantines() {
    let mut broker = CapsuleBroker::new(
        Authenticator::new(&KEY).unwrap(),
        MemoryJournal::default(),
        HostOperationsDisabled,
    )
    .unwrap();
    let error = broker
        .handle(
            &bytes(&signed(BrokerAction::Create, "dry-run-1", 10, 20)),
            15,
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NativeUnavailable);
    let (journal, _) = broker.into_parts();
    let entries = journal.entries().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].intent);
    assert_eq!(entries[1].state, CapsuleState::Quarantined);
}
#[test]
fn injected_api_failure_never_succeeds() {
    let api = RecordingApi {
        operations: Vec::new(),
        fail: Some("create"),
    };
    let mut broker = CapsuleBroker::new(
        Authenticator::new(&KEY).unwrap(),
        MemoryJournal::default(),
        api,
    )
    .unwrap();
    assert_eq!(
        broker
            .handle(
                &bytes(&signed(BrokerAction::Create, "dry-run-2", 10, 20)),
                15
            )
            .unwrap_err()
            .code,
        ErrorCode::NativeFailure
    );
    let (journal, api) = broker.into_parts();
    assert_eq!(api.operations, ["create"]);
    assert_eq!(
        journal.entries().unwrap().last().unwrap().state,
        CapsuleState::Quarantined
    );
}
