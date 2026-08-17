//! `ConnectorRegistry` — this node's Deliverable 1 (register/deregister,
//! list registered connectors and their capabilities) plus the orchestration
//! entry points for discovery (Deliverable 2) and health (Deliverable 3),
//! all guarded so concurrent registry access cannot corrupt state
//! (Deliverable 5's concurrency test).
//!
//! ## Concurrency shape
//!
//! - `entries: RwLock<HashMap<ConnectorId, Arc<Entry>>>` — the map itself.
//!   `register`/`deregister` take the write lock; `list`/`get`/discovery/
//!   health-cycle only need a brief read lock to clone the `Arc<Entry>`s
//!   they operate on, so a long-running discovery/health cycle never blocks
//!   a concurrent `register`/`deregister`/`list` call (and vice versa)
//!   longer than the map-snapshot copy itself takes.
//! - `Entry::connector: tokio::sync::Mutex<Box<dyn Connector>>` — every
//!   `Connector` lifecycle method takes `&mut self`
//!   (`crates/loki-connectors-sdk/src/connector.rs`), so this is the
//!   interior-mutability boundary that lets two different registry
//!   operations (e.g. a health check and a discovery identify) run against
//!   *different* connectors concurrently while still serializing access to
//!   any *one* connector's mutable state, matching
//!   `Docs/CONNECTOR_SPEC.md` §4's "must not ... continue a device-changing
//!   stage after losing the owning job lease" expectation that only one
//!   logical caller drives a connector's mutable lifecycle at a time.
//! - `Entry::manifest`/`capabilities` are cloned once at registration
//!   (`ConnectorManifest`/`CapabilityDescriptor` are both `Clone`) and never
//!   mutated afterward, so `list`/`get`/projection reads need no lock at
//!   all beyond the outer map's.
//! - `Entry::health: RwLock<Option<HealthStatusRecord>>` — the latest health
//!   snapshot, updated by `run_health_cycle`, read by `health_status`
//!   without ever touching the connector's own mutex.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use loki_connectors_sdk::connector::Connector;
use loki_connectors_sdk::ids::ConnectorId;
use loki_connectors_sdk::manifest::ConnectorManifest;
use loki_connectors_sdk::operation::OperationContext;
use loki_connectors_sdk::run_operation;
use tokio::sync::{Mutex, RwLock};

use crate::discovery::{
    CandidateId, DiscoveryOutcome, build_identify_request, interpret_identify_outcome,
};
use crate::error::RegistryError;
use crate::health::HealthStatusRecord;
use crate::projection::{self, RegisteredConnectorSummary};

struct Entry {
    manifest: ConnectorManifest,
    connector: Mutex<Box<dyn Connector>>,
    health: RwLock<Option<HealthStatusRecord>>,
}

/// The connector registry: register/deregister connector instances, list
/// them with their projected capabilities/certification, and orchestrate
/// discovery (identify) and health cycles against every currently
/// registered connector.
#[derive(Default)]
pub struct ConnectorRegistry {
    entries: RwLock<HashMap<ConnectorId, Arc<Entry>>>,
}

impl ConnectorRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Registers a connector instance under the `ConnectorId` its own
    /// manifest declares. Rejects a duplicate id rather than silently
    /// overwriting an existing registration — a caller that intends to
    /// replace one must `deregister` it first.
    pub async fn register(
        &self,
        connector: Box<dyn Connector>,
    ) -> Result<ConnectorId, RegistryError> {
        let manifest = connector.manifest().clone();
        let connector_id = manifest.connector_id.clone();

        let mut entries = self.entries.write().await;
        if entries.contains_key(&connector_id) {
            return Err(RegistryError::AlreadyRegistered(connector_id));
        }
        entries.insert(
            connector_id.clone(),
            Arc::new(Entry {
                manifest,
                connector: Mutex::new(connector),
                health: RwLock::new(None),
            }),
        );
        Ok(connector_id)
    }

    /// Removes a connector's registration. Returns [`RegistryError::NotFound`]
    /// if `connector_id` was never registered or was already deregistered —
    /// deregistration is not idempotent-success, so a caller can tell a
    /// no-op apart from a real removal.
    pub async fn deregister(&self, connector_id: &ConnectorId) -> Result<(), RegistryError> {
        let mut entries = self.entries.write().await;
        entries
            .remove(connector_id)
            .map(|_| ())
            .ok_or_else(|| RegistryError::NotFound(connector_id.clone()))
    }

    /// True if `connector_id` currently has a live registration.
    pub async fn is_registered(&self, connector_id: &ConnectorId) -> bool {
        self.entries.read().await.contains_key(connector_id)
    }

    /// Lists every currently registered connector's projected summary
    /// (capabilities, certification/support-tier — see
    /// [`crate::projection`]). Order is not significant; callers that need a
    /// stable order should sort by `connector_id` themselves.
    pub async fn list(&self) -> Vec<RegisteredConnectorSummary> {
        let entries = self.entries.read().await;
        entries
            .values()
            .map(|e| projection::project(&e.manifest))
            .collect()
    }

    /// The single registered connector's projected summary, or `None` if it
    /// is not registered.
    pub async fn get(&self, connector_id: &ConnectorId) -> Option<RegisteredConnectorSummary> {
        let entries = self.entries.read().await;
        entries
            .get(connector_id)
            .map(|e| projection::project(&e.manifest))
    }

    /// The full manifest of one registered connector (beyond what
    /// [`RegisteredConnectorSummary`] projects), or `None` if it is not
    /// registered.
    pub async fn manifest(&self, connector_id: &ConnectorId) -> Option<ConnectorManifest> {
        let entries = self.entries.read().await;
        entries.get(connector_id).map(|e| e.manifest.clone())
    }

    /// A snapshot of every currently registered `(ConnectorId, Arc<Entry>)`
    /// pair. Cloning the `Arc`s (not the connectors themselves) lets
    /// discovery/health cycles release the map lock immediately, so they
    /// never hold up a concurrent `register`/`deregister`/`list` call for
    /// the duration of a whole cycle.
    async fn snapshot(&self) -> Vec<(ConnectorId, Arc<Entry>)> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .map(|(id, entry)| (id.clone(), Arc::clone(entry)))
            .collect()
    }

    /// Runs one discovery (identify) cycle against every currently
    /// registered connector — Deliverable 2. See `crate::discovery`'s
    /// module doc for exactly what "discovery" means at this milestone (no
    /// real transport probing yet; each registered instance is itself the
    /// one candidate identified). `ctx_factory` is called once per
    /// candidate so each identify call gets its own `OperationContext`
    /// (e.g. a fresh correlation id) rather than this crate inventing one
    /// silently.
    pub async fn run_discovery_cycle(
        &self,
        mut ctx_factory: impl FnMut(&ConnectorId) -> OperationContext,
    ) -> Vec<DiscoveryOutcome> {
        let snapshot = self.snapshot().await;
        let mut outcomes = Vec::with_capacity(snapshot.len());
        for (connector_id, entry) in snapshot {
            let candidate = CandidateId::from_connector_id(&connector_id);
            let request = build_identify_request(&candidate);
            let ctx = ctx_factory(&connector_id);

            let lifecycle = {
                let mut guard = entry.connector.lock().await;
                run_operation(&mut **guard, &request, &ctx).await
            };

            outcomes.push(interpret_identify_outcome(
                connector_id,
                entry.manifest.version.clone(),
                candidate,
                lifecycle,
                SystemTime::now(),
            ));
        }
        outcomes
    }

    /// Runs one identify cycle against a single registered connector.
    /// Convenience wrapper over the same logic
    /// [`ConnectorRegistry::run_discovery_cycle`] uses, for a caller that
    /// only wants to (re-)identify one connector rather than every
    /// registered one.
    pub async fn identify_one(
        &self,
        connector_id: &ConnectorId,
        ctx: &OperationContext,
    ) -> Result<DiscoveryOutcome, RegistryError> {
        let entry = {
            let entries = self.entries.read().await;
            entries
                .get(connector_id)
                .cloned()
                .ok_or_else(|| RegistryError::NotFound(connector_id.clone()))?
        };

        let candidate = CandidateId::from_connector_id(connector_id);
        let request = build_identify_request(&candidate);
        let lifecycle = {
            let mut guard = entry.connector.lock().await;
            run_operation(&mut **guard, &request, ctx).await
        };
        Ok(interpret_identify_outcome(
            connector_id.clone(),
            entry.manifest.version.clone(),
            candidate,
            lifecycle,
            SystemTime::now(),
        ))
    }

    /// Runs one health-check cycle against every currently registered
    /// connector, updating each connector's stored [`HealthStatusRecord`] —
    /// Deliverable 3's "periodic health-check invocation" (the periodic
    /// *scheduling* itself lives in [`crate::scheduler`]; this method is
    /// the single "run it once" primitive the scheduler repeatedly calls).
    /// An unhealthy connector's aggregated status is stored exactly as
    /// computed by [`crate::health::AggregatedHealthStatus::from_report`] —
    /// never overwritten with a healthier status than the report actually
    /// supports.
    pub async fn run_health_cycle(
        &self,
        ctx: &OperationContext,
    ) -> Vec<(ConnectorId, HealthStatusRecord)> {
        let snapshot = self.snapshot().await;
        let mut results = Vec::with_capacity(snapshot.len());
        for (connector_id, entry) in snapshot {
            let report = {
                let mut guard = entry.connector.lock().await;
                guard.health_check(ctx).await
            };
            let record = HealthStatusRecord::from_report(report, SystemTime::now());
            *entry.health.write().await = Some(record.clone());
            results.push((connector_id, record));
        }
        results
    }

    /// The most recently recorded [`HealthStatusRecord`] for one connector,
    /// or `None` if it is not registered or has never been health-checked
    /// yet. Never fabricates a status for a connector that has not actually
    /// reported one.
    pub async fn health_status(&self, connector_id: &ConnectorId) -> Option<HealthStatusRecord> {
        let entries = self.entries.read().await;
        let entry = entries.get(connector_id)?;
        entry.health.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::AggregatedHealthStatus;
    use crate::test_support::{Behavior, TestConnector};
    use loki_connectors_sdk::ids::CorrelationId;

    fn ctx() -> OperationContext {
        OperationContext::new(CorrelationId::new("corr-1").unwrap())
    }

    #[tokio::test]
    async fn register_then_list_reflects_the_connector() {
        let registry = ConnectorRegistry::new();
        let id = registry
            .register(Box::new(TestConnector::identifiable("conn-a")))
            .await
            .unwrap();
        assert_eq!(id.as_str(), "conn-a");

        let listed = registry.list().await;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].connector_id.as_str(), "conn-a");
    }

    #[tokio::test]
    async fn duplicate_registration_is_rejected() {
        let registry = ConnectorRegistry::new();
        registry
            .register(Box::new(TestConnector::identifiable("conn-a")))
            .await
            .unwrap();
        let err = registry
            .register(Box::new(TestConnector::identifiable("conn-a")))
            .await
            .unwrap_err();
        assert!(matches!(err, RegistryError::AlreadyRegistered(_)));
    }

    #[tokio::test]
    async fn deregister_round_trip() {
        let registry = ConnectorRegistry::new();
        let id = registry
            .register(Box::new(TestConnector::identifiable("conn-a")))
            .await
            .unwrap();
        assert!(registry.is_registered(&id).await);

        registry.deregister(&id).await.unwrap();
        assert!(!registry.is_registered(&id).await);
        assert!(registry.list().await.is_empty());
    }

    #[tokio::test]
    async fn deregistering_an_unknown_id_is_an_honest_error() {
        let registry = ConnectorRegistry::new();
        let unknown = ConnectorId::new("never-registered").unwrap();
        let err = registry.deregister(&unknown).await.unwrap_err();
        assert!(matches!(err, RegistryError::NotFound(_)));
    }

    #[tokio::test]
    async fn discovery_cycle_finds_and_identifies_the_registered_connector() {
        let registry = ConnectorRegistry::new();
        registry
            .register(Box::new(TestConnector::identifiable("conn-a")))
            .await
            .unwrap();

        let outcomes = registry.run_discovery_cycle(|_id| ctx()).await;
        assert_eq!(outcomes.len(), 1);
        let outcome = &outcomes[0];
        assert_eq!(outcome.connector_id.as_str(), "conn-a");
        assert!(outcome.lifecycle.succeeded());
        assert!(outcome.identity.is_some());
    }

    #[tokio::test]
    async fn discovery_cycle_reports_a_connector_that_fails_identify_honestly() {
        let registry = ConnectorRegistry::new();
        registry
            .register(Box::new(
                TestConnector::identifiable("conn-a")
                    .with_identify_verify(Behavior::Fail("chip mismatch".to_string())),
            ))
            .await
            .unwrap();

        let outcomes = registry.run_discovery_cycle(|_id| ctx()).await;
        assert_eq!(outcomes.len(), 1);
        assert!(!outcomes[0].lifecycle.succeeded());
        assert!(outcomes[0].identity.is_none());
    }

    #[tokio::test]
    async fn health_cycle_reports_unhealthy_connector_without_masking_it() {
        let registry = ConnectorRegistry::new();
        let id = registry
            .register(Box::new(TestConnector::unhealthy("conn-a")))
            .await
            .unwrap();

        let results = registry.run_health_cycle(&ctx()).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.aggregated, AggregatedHealthStatus::Unhealthy);

        let stored = registry.health_status(&id).await.unwrap();
        assert_eq!(stored.aggregated, AggregatedHealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn health_cycle_reports_healthy_connector_accurately() {
        let registry = ConnectorRegistry::new();
        let id = registry
            .register(Box::new(TestConnector::identifiable("conn-a")))
            .await
            .unwrap();

        registry.run_health_cycle(&ctx()).await;
        let stored = registry.health_status(&id).await.unwrap();
        assert_eq!(stored.aggregated, AggregatedHealthStatus::Healthy);
    }

    #[tokio::test]
    async fn health_status_is_none_before_any_cycle_has_run() {
        let registry = ConnectorRegistry::new();
        let id = registry
            .register(Box::new(TestConnector::identifiable("conn-a")))
            .await
            .unwrap();
        assert!(registry.health_status(&id).await.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn concurrent_register_deregister_and_list_do_not_corrupt_state() {
        let registry = Arc::new(ConnectorRegistry::new());
        let connector_count = 50usize;

        // Register `connector_count` distinct connectors concurrently.
        let mut register_tasks = Vec::new();
        for i in 0..connector_count {
            let registry = Arc::clone(&registry);
            register_tasks.push(tokio::spawn(async move {
                registry
                    .register(Box::new(TestConnector::identifiable(&format!("conn-{i}"))))
                    .await
            }));
        }
        for task in register_tasks {
            task.await.unwrap().unwrap();
        }
        assert_eq!(registry.list().await.len(), connector_count);

        // Concurrently deregister half of them while running discovery/health
        // cycles and list() calls against whatever is currently registered.
        let mut mixed_tasks = Vec::new();
        for i in 0..connector_count {
            let registry = Arc::clone(&registry);
            if i % 2 == 0 {
                mixed_tasks.push(tokio::spawn(async move {
                    let id = ConnectorId::new(format!("conn-{i}")).unwrap();
                    let _ = registry.deregister(&id).await;
                }));
            } else {
                mixed_tasks.push(tokio::spawn(async move {
                    let _ = registry.list().await;
                    let _ = registry.run_health_cycle(&ctx()).await;
                }));
            }
        }
        for task in mixed_tasks {
            task.await.unwrap();
        }

        // Exactly the odd-indexed half must remain — no entry lost or
        // duplicated, no entry that should have been removed still present.
        let remaining = registry.list().await;
        assert_eq!(remaining.len(), connector_count / 2);
        for summary in &remaining {
            let idx: usize = summary
                .connector_id
                .as_str()
                .strip_prefix("conn-")
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(idx % 2, 1, "only odd-indexed connectors should remain");
        }
    }
}
