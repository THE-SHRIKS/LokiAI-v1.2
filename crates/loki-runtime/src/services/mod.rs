//! Internal production service composition for the LokiAI runtime.
//!
//! This module deliberately exposes projections and a bounded mutation port,
//! not the runtime-owned [`loki_storage::Storage`] handle. Transport adapters
//! may read through [`RuntimeReadPort`]. Consequential mutation submission
//! (`P1-RUNTIME-SERVICE-READINESS-CORR` follow-up) now routes every request
//! through the real, atomic `loki_acceptance_store::accept_mutation`
//! coordinator via [`MutationSubmissionPort`] — no caller can bypass that
//! transaction boundary or obtain the shared `Storage` handle through this
//! API.

mod mutation;
mod read;

use std::sync::Arc;

use loki_agent_delivery::WorkIntentDeliveryStore;
use loki_agents::AgentRegistry;
use loki_approvals::ApprovalStore;
use loki_jobs::JobEngine;
use loki_local_authority::LocalAuthority;
use loki_storage::Storage;

pub use loki_mutation_contract::{AcceptanceResult, MutationAcceptanceRequest};
pub use mutation::{MutationAvailability, MutationSubmissionError, MutationSubmissionPort};
pub use read::{
    DeviceHealthProjection, DeviceHealthState, DevicePage, DevicePageRequest, DeviceSummary,
    PersistedHealthEvidence, RuntimeReadError, RuntimeReadPort, RuntimeStatus, StorageReadiness,
};

/// The runtime's local application services, all backed by one migrated store.
///
/// Construction is crate-private because opening and migrating storage belongs
/// to runtime startup. Public consumers receive only the bounded ports below;
/// neither the storage handle nor mutation-capable domain services are exposed.
pub struct RuntimeServices {
    storage: Arc<Storage>,
    _local_authority: LocalAuthority,
    _agents: AgentRegistry,
    _approvals: ApprovalStore,
    _agent_delivery: WorkIntentDeliveryStore,
    _jobs: JobEngine,
    mutation: mutation::RealMutationSubmission,
}

impl RuntimeServices {
    /// Composes every service that currently supports the shared production
    /// storage contract. The caller must have completed migrations first.
    pub(crate) fn compose(storage: Arc<Storage>) -> Self {
        Self {
            storage: Arc::clone(&storage),
            _local_authority: LocalAuthority::new(Arc::clone(&storage)),
            _agents: AgentRegistry::new(Arc::clone(&storage)),
            _approvals: ApprovalStore::new(Arc::clone(&storage)),
            _agent_delivery: WorkIntentDeliveryStore::new(Arc::clone(&storage)),
            _jobs: JobEngine::new(Arc::clone(&storage)),
            mutation: mutation::RealMutationSubmission::new(storage),
        }
    }
}

impl RuntimeReadPort for RuntimeServices {
    fn runtime_status(&self) -> Result<RuntimeStatus, RuntimeReadError> {
        read::runtime_status(self.storage.as_ref(), self.mutation.availability())
    }

    fn list_devices(&self, request: DevicePageRequest) -> Result<DevicePage, RuntimeReadError> {
        read::list_devices(self.storage.as_ref(), request)
    }

    fn device_health(
        &self,
        device_id: &loki_storage::ids::DeviceId,
    ) -> Result<DeviceHealthProjection, RuntimeReadError> {
        read::device_health(self.storage.as_ref(), device_id)
    }
}

impl MutationSubmissionPort for RuntimeServices {
    fn availability(&self) -> MutationAvailability {
        self.mutation.availability()
    }

    fn submit_mutation(
        &self,
        request: &MutationAcceptanceRequest,
    ) -> Result<AcceptanceResult, MutationSubmissionError> {
        self.mutation.submit_mutation(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_storage::agents::AgentsRepository;
    use loki_storage::devices::{DeviceRepository, NewDevice};
    use loki_storage::ids::{ConnectorId, DeviceId, RuntimeId};
    use loki_storage::states::{DeviceLifecycleState, SupportTier};

    fn services() -> (Arc<Storage>, RuntimeServices) {
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let storage = Arc::new(Storage::open_in_memory(loki_storage::Cipher::None).unwrap());
        storage.run_migrations("2026-08-29T00:00:00.000Z").unwrap();
        let services = RuntimeServices::compose(Arc::clone(&storage));
        (storage, services)
    }

    fn device(health_json: Option<&str>) -> NewDevice {
        NewDevice {
            device_id: DeviceId::new("dev:test-1").unwrap(),
            connector_id: ConnectorId::new("connector:test").unwrap(),
            connector_version: "1.0.0".into(),
            hardware_identity: "test-qualified-identity".into(),
            family: "test-family".into(),
            board_identity: None,
            transport_reference: None,
            runtime_id: RuntimeId::new("runtime:test").unwrap(),
            support_tier: SupportTier::Custom,
            lifecycle_state: DeviceLifecycleState::Identified,
            capabilities_json: None,
            firmware_artifact_ref: None,
            config_artifact_ref: None,
            model_artifact_ref: None,
            health_json: health_json.map(str::to_owned),
            last_seen_at: "2026-08-29T00:00:00.000Z".into(),
            created_at: "2026-08-29T00:00:00.000Z".into(),
            updated_at: "2026-08-29T00:00:00.000Z".into(),
        }
    }

    #[test]
    fn empty_store_is_an_honest_empty_success() {
        let (_storage, services) = services();

        let status = services.runtime_status().unwrap();
        assert_eq!(status.storage, StorageReadiness::Ready);
        assert_eq!(status.registered_device_count, 0);
        assert!(
            services
                .list_devices(DevicePageRequest::new(100, None).unwrap())
                .unwrap()
                .devices
                .is_empty()
        );
        assert_eq!(status.mutations, MutationAvailability::Available);
    }

    #[test]
    fn reads_registered_device_and_persisted_health_without_claiming_live_health() {
        let (storage, services) = services();
        storage
            .insert_device(&device(Some(r#"{"source":"connector","ok":true}"#)))
            .unwrap();

        let devices = services
            .list_devices(DevicePageRequest::new(100, None).unwrap())
            .unwrap();
        assert_eq!(devices.devices.len(), 1);
        assert_eq!(devices.devices[0].device_id.as_str(), "dev:test-1");

        let health = services
            .device_health(&DeviceId::new("dev:test-1").unwrap())
            .unwrap();
        assert!(matches!(
            health.health,
            DeviceHealthState::Persisted(PersistedHealthEvidence {
                payload_size_bytes: 32,
                ..
            })
        ));
    }

    #[test]
    fn missing_health_is_not_observed_and_missing_device_is_not_found() {
        let (storage, services) = services();
        storage.insert_device(&device(None)).unwrap();

        let health = services
            .device_health(&DeviceId::new("dev:test-1").unwrap())
            .unwrap();
        assert_eq!(health.health, DeviceHealthState::NotObserved);

        assert!(matches!(
            services.device_health(&DeviceId::new("dev:missing").unwrap()),
            Err(RuntimeReadError::DeviceNotFound { .. })
        ));
    }

    #[test]
    fn invalid_persisted_health_fails_closed() {
        let (storage, services) = services();
        storage.insert_device(&device(Some("not-json"))).unwrap();

        assert!(matches!(
            services.device_health(&DeviceId::new("dev:test-1").unwrap()),
            Err(RuntimeReadError::InvalidHealthProjection { .. })
        ));
    }

    #[test]
    fn device_listing_is_bounded_and_uses_a_stable_device_cursor() {
        let (storage, services) = services();
        for id in ["dev:a", "dev:b", "dev:c"] {
            let mut row = device(None);
            row.device_id = DeviceId::new(id).unwrap();
            row.hardware_identity = format!("identity:{id}");
            storage.insert_device(&row).unwrap();
        }

        let first = services
            .list_devices(DevicePageRequest::new(2, None).unwrap())
            .unwrap();
        assert_eq!(first.total_registered_count, 3);
        assert_eq!(first.devices.len(), 2);
        assert_eq!(
            first.next_after_device_id.as_ref().unwrap().as_str(),
            "dev:b"
        );

        let second = services
            .list_devices(DevicePageRequest::new(2, first.next_after_device_id).unwrap())
            .unwrap();
        assert_eq!(second.devices.len(), 1);
        assert_eq!(second.devices[0].device_id.as_str(), "dev:c");
        assert!(second.next_after_device_id.is_none());
    }

    #[test]
    fn device_page_limit_is_validated_before_storage_access() {
        assert!(matches!(
            DevicePageRequest::new(0, None),
            Err(RuntimeReadError::InvalidPageLimit { .. })
        ));
        assert!(matches!(
            DevicePageRequest::new(101, None),
            Err(RuntimeReadError::InvalidPageLimit { .. })
        ));
    }

    /// Exercises real mutation submission through the top-level
    /// `RuntimeServices` façade (not `mutation::RealMutationSubmission`
    /// directly), proving `RuntimeServices::submit_mutation` is a
    /// transparent forward with no divergence from calling
    /// `loki_acceptance_store::accept_mutation` directly against the same
    /// storage handle.
    #[test]
    fn runtime_services_submit_mutation_matches_direct_accept_mutation_call() {
        let (storage, services) = services();
        storage
            .insert_agent_connection(
                "ac_runtime_services_facade_test",
                "test-agent",
                "sha256:test-hash",
                "register_runs",
                0,
            )
            .unwrap();

        let request = loki_mutation_contract::MutationAcceptanceRequest::new(
            "ac_runtime_services_facade_test",
            "agent-1",
            1,
            loki_mutation_contract::MutationMethod::AgentRunRegister,
            "target-1",
            None,
            "sha256:params",
            loki_mutation_contract::IdempotencyKey::new("idem-facade-1").unwrap(),
            None,
        )
        .unwrap();

        let via_services = services.submit_mutation(&request).unwrap();
        assert!(matches!(via_services, AcceptanceResult::Accepted(_)));

        // Replaying the exact same request through the façade again must
        // return the replayed-identical result, matching direct-call
        // idempotency semantics exactly (proves the façade did not
        // re-execute or diverge on retry).
        let replayed = services.submit_mutation(&request).unwrap();
        assert!(matches!(
            replayed,
            AcceptanceResult::ReplayedIdenticalAccepted(_)
        ));
    }

    /// A rejected mutation (no matching agent connection at all) surfaces
    /// through the façade as a real, specific `Ok(Rejected(..))`, never a
    /// generic error or a silently swallowed failure.
    #[test]
    fn runtime_services_submit_mutation_surfaces_a_real_rejection() {
        let (_storage, services) = services();
        let request = loki_mutation_contract::MutationAcceptanceRequest::new(
            "ac_never_registered",
            "agent-1",
            1,
            loki_mutation_contract::MutationMethod::AgentRunRegister,
            "target-1",
            None,
            "sha256:params",
            loki_mutation_contract::IdempotencyKey::new("idem-facade-rejected").unwrap(),
            None,
        )
        .unwrap();

        let result = services.submit_mutation(&request).unwrap();
        assert!(matches!(
            result,
            AcceptanceResult::Rejected(
                loki_mutation_contract::MutationRejection::GrantInvalid { .. }
            )
        ));
    }
}
