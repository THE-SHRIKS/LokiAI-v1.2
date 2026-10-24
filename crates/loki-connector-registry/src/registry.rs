//! `ConnectorRegistry` — register/deregister, list registered connectors and
//! their capabilities, plus the orchestration entry points for discovery
//! (`CON-002-R1`) and health, all guarded so concurrent registry access
//! cannot corrupt state.
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
//!   any *one* connector's mutable state.
//! - `Entry::manifest`/`capabilities` are cloned once at registration and
//!   never mutated afterward, so `list`/`get`/projection reads need no lock
//!   at all beyond the outer map's.
//! - `Entry::health: RwLock<Option<HealthStatusRecord>>` — the latest health
//!   snapshot, updated by `run_health_cycle`, read by `health_status`
//!   without ever touching the connector's own mutex.
//! - `reconciled: RwLock<HashMap<String, ReconciledDevice>>` — the
//!   stable-identity reconciliation state (Deliverable 3), keyed on
//!   `PublicDeviceIdentity::device_id()`. Guarded independently of
//!   `entries` so a discovery cycle's reconciliation update never needs to
//!   hold the connector map's write lock, and register/deregister never
//!   need to touch reconciliation state at all.
//!
//! ## Stable-identity reconciliation (Deliverable 3)
//!
//! Reconciliation is keyed on `PublicDeviceIdentity::device_id()` — the
//! stable, HMAC-derived string `loki_connectors_sdk::identity` computes from
//! device-derived evidence, never on any candidate-derived string (COM port,
//! connector id, ephemeral `candidate_id`). Each discovery cycle:
//!
//! 1. Asks the caller-supplied `CandidateSource` for every currently
//!    observed candidate, per registered connector.
//! 2. Runs a real `identify` lifecycle for each candidate
//!    (`crate::discovery::interpret_identify_outcome`), producing one
//!    [`crate::discovery::CandidateProbe`] per candidate — never fabricated
//!    from anything but that connector's own `preflight`/`execute`/`verify`
//!    result.
//! 3. For each connector, gathers every piece of device-derived evidence its
//!    candidates reported this cycle and asks
//!    `IdentityProbeResult::verified` to merge them:
//!    - If every candidate's evidence resolves to the *same* fingerprint
//!      digest (0 or 1 unique digests), the merge succeeds — this is both
//!      the "duplicate candidate" case (two candidates, one real device) and
//!      the ordinary single-candidate case. The resulting
//!      `PublicDeviceIdentity` is accepted and every candidate that
//!      contributed matching evidence is reported with that
//!      `public_identity`.
//!    - If candidates resolve to *different* digests, `verified` itself
//!      returns an error (its own fail-closed digest-consistency check) —
//!      this registry treats that as `reconciliation_conflict: true` for
//!      every one of this connector's candidates this cycle: no
//!      `public_identity` is accepted for any of them, and the persisted
//!      reconciliation state for this connector is left untouched (an
//!      attacker cannot flip a device's stable identity via a conflicting
//!      duplicate candidate).
//! 4. Accepted identities are upserted into `reconciled` (`live: true`,
//!    `last_seen_at` refreshed) — a reconnect (new candidate, same digest)
//!    finds the *same* `device_id` key regardless of which connector
//!    instance or candidate locator observed it this time, so it is
//!    recognized as the same device, not a new one.
//! 5. Any connector that reported *zero* candidates this cycle (a clean
//!    disconnect — no error, nothing to identify) never runs step 2/3 for
//!    itself; any reconciled device previously tied to that connector and
//!    still marked `live` is flipped to `live: false` and reported in
//!    [`crate::discovery::DiscoveryCycleReport::newly_absent`]. No identity
//!    is ever reported as still-live without new evidence this cycle
//!    (`AGENTS.md` rule 1).
//!
//! A connector whose candidates were present but whose `identify` calls
//! simply failed (busy/permission/generic — see
//! `crate::discovery::DiscoveryFailureClass`) is *not* swept into
//! `newly_absent`: that is a distinct, transient-failure case from a clean
//! disconnect, and this registry does not assume the underlying device is
//! gone just because one cycle's identify attempt failed.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use loki_connectors_sdk::connector::Connector;
use loki_connectors_sdk::identity::{IdentityProbeResult, PublicDeviceIdentity};
use loki_connectors_sdk::ids::ConnectorId;
use loki_connectors_sdk::manifest::ConnectorManifest;
use loki_connectors_sdk::operation::OperationContext;
use loki_connectors_sdk::run_operation;
use tokio::sync::{Mutex, RwLock};

use crate::discovery::{
    CandidateProbe, CandidateSource, DiscoveryCycleReport, DiscoveryOutcome, ObservedCandidate,
    build_identify_request, interpret_identify_outcome,
};
use crate::error::RegistryError;
use crate::health::HealthStatusRecord;
use crate::projection::{self, RegisteredConnectorSummary};

struct Entry {
    manifest: ConnectorManifest,
    connector: Mutex<Box<dyn Connector>>,
    health: RwLock<Option<HealthStatusRecord>>,
}

/// This registry's persisted view of one stable device identity — see this
/// module's "Stable-identity reconciliation" doc section.
#[derive(Debug, Clone, PartialEq)]
struct ReconciledDevice {
    public_identity: PublicDeviceIdentity,
    connector_id: ConnectorId,
    first_seen_at: SystemTime,
    last_seen_at: SystemTime,
    live: bool,
}

/// A read-only snapshot of one reconciled device's state, for callers that
/// want to inspect reconciliation outcomes directly (e.g. this crate's own
/// reconnect/duplicate/clean-disconnect tests) without reaching into
/// registry-internal storage.
#[derive(Debug, Clone, PartialEq)]
pub struct ReconciledDeviceSnapshot {
    pub public_identity: PublicDeviceIdentity,
    pub connector_id: ConnectorId,
    pub first_seen_at: SystemTime,
    pub last_seen_at: SystemTime,
    pub live: bool,
}

impl From<&ReconciledDevice> for ReconciledDeviceSnapshot {
    fn from(device: &ReconciledDevice) -> Self {
        Self {
            public_identity: device.public_identity.clone(),
            connector_id: device.connector_id.clone(),
            first_seen_at: device.first_seen_at,
            last_seen_at: device.last_seen_at,
            live: device.live,
        }
    }
}

/// The connector registry: register/deregister connector instances, list
/// them with their projected capabilities/certification, and orchestrate
/// discovery (identify + stable-identity reconciliation) and health cycles
/// against every currently registered connector.
#[derive(Default)]
pub struct ConnectorRegistry {
    entries: RwLock<HashMap<ConnectorId, Arc<Entry>>>,
    reconciled: RwLock<HashMap<String, ReconciledDevice>>,
}

impl ConnectorRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            reconciled: RwLock::new(HashMap::new()),
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
    /// no-op apart from a real removal. Does not touch reconciliation
    /// state: a device this connector previously identified stays in
    /// `reconciled` (still queryable, `live` will fall stale via the normal
    /// absence sweep the next cycle runs) rather than being silently erased.
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

    /// Runs one discovery (identify + reconciliation) cycle against every
    /// currently registered connector — Deliverables 1-4. See this module's
    /// "Stable-identity reconciliation" doc section for exactly what
    /// happens for duplicate/conflicting/absent candidates. `candidates` is
    /// the injectable candidate-sourcing abstraction (Deliverable 1);
    /// `ctx_factory` is called once per candidate so each identify call gets
    /// its own `OperationContext` (e.g. a fresh correlation id) rather than
    /// this crate inventing one silently.
    pub async fn run_discovery_cycle(
        &self,
        candidates: &dyn CandidateSource,
        mut ctx_factory: impl FnMut(&ConnectorId) -> OperationContext,
    ) -> DiscoveryCycleReport {
        let snapshot = self.snapshot().await;
        let mut outcomes = Vec::new();
        let mut connectors_with_candidates: HashSet<ConnectorId> = HashSet::new();

        for (connector_id, entry) in snapshot {
            let observed_candidates = candidates.candidates_for(&connector_id);
            if observed_candidates.is_empty() {
                continue;
            }
            connectors_with_candidates.insert(connector_id.clone());

            let mut probes = Vec::with_capacity(observed_candidates.len());
            for observed in observed_candidates {
                // `build_identify_request` can fail closed with
                // `DiscoveryFailureClass::CandidateNotAddressable` for a
                // legitimately-sized `candidate_id` this registry's own
                // ephemeral-id wrapper cannot fit under the SDK's id-length
                // bound (see `discovery.rs`'s doc comment on that function).
                // This is never a connector-reported failure, so it is
                // reported as its own outcome without ever invoking the
                // connector.
                let request = match build_identify_request(&observed) {
                    Ok(request) => request,
                    Err(failure_class) => {
                        probes.push(crate::discovery::unaddressable_probe(
                            observed,
                            failure_class,
                            SystemTime::now(),
                        ));
                        continue;
                    }
                };
                let ctx = ctx_factory(&connector_id);
                let lifecycle = {
                    let mut guard = entry.connector.lock().await;
                    run_operation(&mut **guard, &request, &ctx).await
                };
                probes.push(interpret_identify_outcome(
                    observed,
                    lifecycle,
                    SystemTime::now(),
                ));
            }

            outcomes.extend(self.reconcile_connector_probes(&connector_id, probes).await);
        }

        let newly_absent = self
            .mark_absent_connectors(&connectors_with_candidates)
            .await;

        DiscoveryCycleReport {
            outcomes,
            newly_absent,
        }
    }

    /// Runs one identify cycle against a single candidate of a single
    /// registered connector. Convenience wrapper over the same
    /// identify/reconciliation logic
    /// [`ConnectorRegistry::run_discovery_cycle`] uses, for a caller that
    /// only wants to (re-)identify one specific candidate rather than every
    /// candidate every registered connector currently reports.
    pub async fn identify_one(
        &self,
        connector_id: &ConnectorId,
        candidate: ObservedCandidate,
        ctx: &OperationContext,
    ) -> Result<DiscoveryOutcome, RegistryError> {
        let entry = {
            let entries = self.entries.read().await;
            entries
                .get(connector_id)
                .cloned()
                .ok_or_else(|| RegistryError::NotFound(connector_id.clone()))?
        };

        let probe = match build_identify_request(&candidate) {
            Ok(request) => {
                let lifecycle = {
                    let mut guard = entry.connector.lock().await;
                    run_operation(&mut **guard, &request, ctx).await
                };
                interpret_identify_outcome(candidate, lifecycle, SystemTime::now())
            }
            Err(failure_class) => {
                crate::discovery::unaddressable_probe(candidate, failure_class, SystemTime::now())
            }
        };
        let mut outcomes = self
            .reconcile_connector_probes(connector_id, vec![probe])
            .await;
        Ok(outcomes
            .pop()
            .expect("reconciling exactly one probe always yields exactly one outcome"))
    }

    /// Merges every [`CandidateProbe`] one connector produced this cycle
    /// into that connector's `DiscoveryOutcome`s, applying the
    /// digest-consistency reconciliation rule described in this module's
    /// doc comment, and persists an accepted identity into `reconciled`.
    async fn reconcile_connector_probes(
        &self,
        connector_id: &ConnectorId,
        probes: Vec<CandidateProbe>,
    ) -> Vec<DiscoveryOutcome> {
        let merge_evidence: Vec<_> = probes.iter().filter_map(|p| p.evidence.clone()).collect();
        let representative_candidate = probes
            .iter()
            .find(|p| p.evidence.is_some())
            .map(|p| p.candidate.clone());

        let (accepted_identity, conflict) = if merge_evidence.is_empty() {
            (None, false)
        } else {
            let representative_candidate = representative_candidate
                .expect("merge_evidence non-empty implies at least one probe carried evidence");
            match IdentityProbeResult::verified(
                representative_candidate,
                merge_evidence,
                "reconciled_cycle_evidence",
            ) {
                Ok(result) => (result.public_identity().cloned(), false),
                Err(_) => {
                    // `verified`'s own fail-closed digest-consistency check
                    // rejected this merge: this connector's candidates this
                    // cycle disagreed about the device's fingerprint digest.
                    // Fail closed — no identity accepted, existing persisted
                    // reconciliation state for this connector is left
                    // untouched.
                    (None, true)
                }
            }
        };

        if let (Some(identity), false) = (&accepted_identity, conflict) {
            self.upsert_reconciled(identity.clone(), connector_id.clone())
                .await;
        }

        probes
            .into_iter()
            .map(|probe| {
                let public_identity = if conflict {
                    None
                } else if probe
                    .probe_result
                    .as_ref()
                    .is_some_and(|r| r.public_identity().is_some())
                {
                    accepted_identity.clone()
                } else {
                    None
                };
                DiscoveryOutcome {
                    connector_id: connector_id.clone(),
                    candidate: probe.candidate,
                    lifecycle: probe.lifecycle,
                    probe_result: probe.probe_result,
                    public_identity,
                    reconciliation_conflict: conflict,
                    failure_class: probe.failure_class,
                    observed_at: probe.observed_at,
                }
            })
            .collect()
    }

    /// Inserts or refreshes one reconciled device record — `live: true`,
    /// `last_seen_at` updated to now. `first_seen_at` is preserved across
    /// updates (only set once, at first insertion) so a reconnect does not
    /// reset how long this device has actually been known.
    async fn upsert_reconciled(&self, identity: PublicDeviceIdentity, connector_id: ConnectorId) {
        let now = SystemTime::now();
        let mut reconciled = self.reconciled.write().await;
        reconciled
            .entry(identity.device_id().to_string())
            .and_modify(|record| {
                record.connector_id = connector_id.clone();
                record.last_seen_at = now;
                record.live = true;
            })
            .or_insert_with(|| ReconciledDevice {
                public_identity: identity,
                connector_id,
                first_seen_at: now,
                last_seen_at: now,
                live: true,
            });
    }

    /// Flips `live: false` for every reconciled device whose connector did
    /// not report any candidate this cycle (`seen_with_candidates`), and
    /// returns the identities that changed — the clean-disconnect case (see
    /// this module's doc comment). A connector's candidates that were
    /// present but failed to identify (busy/permission/generic) never
    /// reaches this sweep for that connector, since it *is* present in
    /// `seen_with_candidates`.
    async fn mark_absent_connectors(
        &self,
        seen_with_candidates: &HashSet<ConnectorId>,
    ) -> Vec<PublicDeviceIdentity> {
        let mut reconciled = self.reconciled.write().await;
        let mut newly_absent = Vec::new();
        for record in reconciled.values_mut() {
            if record.live && !seen_with_candidates.contains(&record.connector_id) {
                record.live = false;
                newly_absent.push(record.public_identity.clone());
            }
        }
        newly_absent
    }

    /// The current reconciliation snapshot for one stable device id (as
    /// returned by `PublicDeviceIdentity::device_id()`), or `None` if this
    /// registry has never reconciled a device with that id.
    pub async fn reconciled_device(&self, device_id: &str) -> Option<ReconciledDeviceSnapshot> {
        let reconciled = self.reconciled.read().await;
        reconciled
            .get(device_id)
            .map(ReconciledDeviceSnapshot::from)
    }

    /// Every reconciled device this registry currently considers `live`
    /// (confirmed present as of the most recent discovery cycle that saw
    /// its owning connector report at least one candidate).
    pub async fn live_reconciled_devices(&self) -> Vec<PublicDeviceIdentity> {
        let reconciled = self.reconciled.read().await;
        reconciled
            .values()
            .filter(|r| r.live)
            .map(|r| r.public_identity.clone())
            .collect()
    }

    /// Runs one health-check cycle against every currently registered
    /// connector, updating each connector's stored [`HealthStatusRecord`].
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
    use crate::discovery::ObservedCandidate;
    use crate::health::AggregatedHealthStatus;
    use crate::test_support::{self, Behavior, TestConnector};
    use loki_connectors_sdk::ids::CorrelationId;

    fn ctx() -> OperationContext {
        OperationContext::new(CorrelationId::new("corr-1").unwrap())
    }

    /// A `CandidateSource` reporting exactly one fixed candidate per
    /// connector — the common case, mirroring the mock-era "each registered
    /// connector is itself one candidate" shortcut this node replaces, but
    /// now expressed as a real caller-supplied source instead of a hardcoded
    /// registry behavior.
    fn single_candidate_source(
        candidate_id: &'static str,
        locator: &'static str,
    ) -> impl Fn(&ConnectorId) -> Vec<ObservedCandidate> {
        move |connector_id: &ConnectorId| {
            vec![ObservedCandidate::new(
                candidate_id,
                test_support::candidate_for(connector_id.as_str(), candidate_id, locator),
            )]
        }
    }

    fn empty_source() -> impl Fn(&ConnectorId) -> Vec<ObservedCandidate> {
        |_connector_id: &ConnectorId| Vec::new()
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
            .register(Box::new(
                TestConnector::identifiable("conn-a").with_default_identity_evidence(
                    test_support::identity_evidence("conn-a", "device-material-a"),
                ),
            ))
            .await
            .unwrap();

        let source = single_candidate_source("cand-1", "COM3");
        let report = registry.run_discovery_cycle(&source, |_id| ctx()).await;
        assert_eq!(report.outcomes.len(), 1);
        let outcome = &report.outcomes[0];
        assert_eq!(outcome.connector_id.as_str(), "conn-a");
        assert!(outcome.lifecycle.as_ref().is_some_and(|l| l.succeeded()));
        assert!(outcome.public_identity.is_some());
        assert!(!outcome.reconciliation_conflict);
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

        let source = single_candidate_source("cand-1", "COM3");
        let report = registry.run_discovery_cycle(&source, |_id| ctx()).await;
        assert_eq!(report.outcomes.len(), 1);
        assert!(
            !report.outcomes[0]
                .lifecycle
                .as_ref()
                .is_some_and(|l| l.succeeded())
        );
        assert!(report.outcomes[0].public_identity.is_none());
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

    #[tokio::test]
    async fn duplicate_candidates_resolving_to_the_same_digest_reconcile_to_one_device() {
        let registry = ConnectorRegistry::new();
        let evidence = test_support::identity_evidence("conn-a", "device-material-shared");
        registry
            .register(Box::new(
                TestConnector::identifiable("conn-a").with_default_identity_evidence(evidence),
            ))
            .await
            .unwrap();

        let source = |connector_id: &ConnectorId| {
            vec![
                ObservedCandidate::new(
                    "cand-1",
                    test_support::candidate_for(connector_id.as_str(), "cand-1", "COM3"),
                ),
                ObservedCandidate::new(
                    "cand-2",
                    test_support::candidate_for(connector_id.as_str(), "cand-2", "COM5"),
                ),
            ]
        };

        let report = registry.run_discovery_cycle(&source, |_id| ctx()).await;
        assert_eq!(report.outcomes.len(), 2);
        assert!(!report.outcomes[0].reconciliation_conflict);
        assert!(!report.outcomes[1].reconciliation_conflict);
        let device_id_a = report.outcomes[0]
            .public_identity
            .as_ref()
            .expect("first duplicate candidate resolves")
            .device_id()
            .to_string();
        let device_id_b = report.outcomes[1]
            .public_identity
            .as_ref()
            .expect("second duplicate candidate resolves")
            .device_id()
            .to_string();
        assert_eq!(
            device_id_a, device_id_b,
            "duplicate candidates for the same physical device must reconcile to one identity"
        );
        assert_eq!(
            registry.live_reconciled_devices().await.len(),
            1,
            "not double-reported"
        );
    }

    #[tokio::test]
    async fn conflicting_digests_within_one_cycle_fail_closed() {
        let registry = ConnectorRegistry::new();
        let mut connector = TestConnector::identifiable("conn-a");
        connector.set_identity_evidence_for_target(
            "cand-1",
            test_support::identity_evidence("conn-a", "device-material-x"),
        );
        connector.set_identity_evidence_for_target(
            "cand-2",
            test_support::identity_evidence("conn-a", "device-material-y"),
        );
        registry.register(Box::new(connector)).await.unwrap();

        let source = |connector_id: &ConnectorId| {
            vec![
                ObservedCandidate::new(
                    "cand-1",
                    test_support::candidate_for(connector_id.as_str(), "cand-1", "COM3"),
                ),
                ObservedCandidate::new(
                    "cand-2",
                    test_support::candidate_for(connector_id.as_str(), "cand-2", "COM5"),
                ),
            ]
        };

        let report = registry.run_discovery_cycle(&source, |_id| ctx()).await;
        assert_eq!(report.outcomes.len(), 2);
        for outcome in &report.outcomes {
            assert!(outcome.reconciliation_conflict);
            assert!(
                outcome.public_identity.is_none(),
                "conflicting digests must fail closed: no PublicDeviceIdentity"
            );
        }
        assert!(registry.live_reconciled_devices().await.is_empty());
    }

    #[tokio::test]
    async fn reconnect_with_a_different_candidate_is_recognized_as_the_same_device() {
        let registry = ConnectorRegistry::new();
        let evidence = test_support::identity_evidence("conn-a", "device-material-stable");
        registry
            .register(Box::new(
                TestConnector::identifiable("conn-a").with_default_identity_evidence(evidence),
            ))
            .await
            .unwrap();

        // Cycle 1: candidate A verified.
        let source_a = single_candidate_source("cand-a", "COM3");
        let report1 = registry.run_discovery_cycle(&source_a, |_id| ctx()).await;
        let device_id = report1.outcomes[0]
            .public_identity
            .as_ref()
            .expect("verified")
            .device_id()
            .to_string();

        // Cycle 2: candidate A absent (clean disconnect).
        let report2 = registry
            .run_discovery_cycle(&empty_source(), |_id| ctx())
            .await;
        assert!(report2.outcomes.is_empty());
        assert_eq!(report2.newly_absent.len(), 1);
        assert_eq!(report2.newly_absent[0].device_id(), device_id);
        let snapshot = registry
            .reconciled_device(&device_id)
            .await
            .expect("still known, just not live");
        assert!(!snapshot.live);

        // Cycle 3: candidate B (different locator/candidate id), same device.
        let source_b = single_candidate_source("cand-b", "COM7");
        let report3 = registry.run_discovery_cycle(&source_b, |_id| ctx()).await;
        assert_eq!(
            report3.outcomes[0]
                .public_identity
                .as_ref()
                .unwrap()
                .device_id(),
            device_id,
            "a reconnect via a different candidate must resolve to the same stable identity"
        );
        let snapshot_after_reconnect = registry.reconciled_device(&device_id).await.unwrap();
        assert!(snapshot_after_reconnect.live);
    }

    #[tokio::test]
    async fn busy_port_is_reported_distinctly_and_later_cycle_recovers() {
        let mut connector = TestConnector::identifiable("conn-a");
        let preflight_slot = connector.preflight_slot();
        connector.set_preflight(Behavior::FailWithCode(
            "transport_busy",
            "port in use".to_string(),
        ));
        connector.set_default_identity_evidence(test_support::identity_evidence(
            "conn-a",
            "device-material-busy",
        ));
        let registry = ConnectorRegistry::new();
        registry.register(Box::new(connector)).await.unwrap();

        let source = single_candidate_source("cand-1", "COM3");
        let report1 = registry.run_discovery_cycle(&source, |_id| ctx()).await;
        assert_eq!(
            report1.outcomes[0].failure_class,
            Some(crate::discovery::DiscoveryFailureClass::Busy)
        );
        assert!(report1.outcomes[0].public_identity.is_none());

        preflight_slot.set(Behavior::Succeed);
        let report2 = registry.run_discovery_cycle(&source, |_id| ctx()).await;
        assert!(
            report2.outcomes[0]
                .lifecycle
                .as_ref()
                .is_some_and(|l| l.succeeded())
        );
        assert!(report2.outcomes[0].public_identity.is_some());
    }

    #[tokio::test]
    async fn permission_denied_is_distinct_from_busy_and_generic() {
        let mut connector = TestConnector::identifiable("conn-a");
        connector.set_preflight(Behavior::FailWithCode(
            "permission_denied",
            "no access to serial port".to_string(),
        ));
        let registry = ConnectorRegistry::new();
        registry.register(Box::new(connector)).await.unwrap();

        let source = single_candidate_source("cand-1", "COM3");
        let report = registry.run_discovery_cycle(&source, |_id| ctx()).await;
        assert_eq!(
            report.outcomes[0].failure_class,
            Some(crate::discovery::DiscoveryFailureClass::PermissionDenied)
        );
    }

    #[tokio::test]
    async fn clean_disconnect_never_fabricates_identity_and_clears_liveness() {
        let registry = ConnectorRegistry::new();
        let evidence = test_support::identity_evidence("conn-a", "device-material-disconnect");
        registry
            .register(Box::new(
                TestConnector::identifiable("conn-a").with_default_identity_evidence(evidence),
            ))
            .await
            .unwrap();

        let source = single_candidate_source("cand-1", "COM3");
        let report1 = registry.run_discovery_cycle(&source, |_id| ctx()).await;
        assert!(report1.outcomes[0].public_identity.is_some());

        let report2 = registry
            .run_discovery_cycle(&empty_source(), |_id| ctx())
            .await;
        assert!(
            report2.outcomes.is_empty(),
            "no candidate this cycle means no identify call and no fabricated outcome"
        );
        assert_eq!(report2.newly_absent.len(), 1);
        assert!(registry.live_reconciled_devices().await.is_empty());
    }

    #[tokio::test]
    async fn unsupported_candidate_reports_unsupported_and_grants_no_capability() {
        let mut connector = TestConnector::identifiable("conn-a");
        connector.set_identify_verify(Behavior::FailWithCode(
            "unsupported_candidate",
            "device family mismatch".to_string(),
        ));
        let registry = ConnectorRegistry::new();
        let id = registry.register(Box::new(connector)).await.unwrap();

        let source = single_candidate_source("cand-1", "COM3");
        let report = registry.run_discovery_cycle(&source, |_id| ctx()).await;
        assert_eq!(
            report.outcomes[0].failure_class,
            Some(crate::discovery::DiscoveryFailureClass::Unsupported)
        );
        assert!(report.outcomes[0].public_identity.is_none());

        // No flash/restart capability is ever offered — this connector's
        // manifest never declared one, and an unsupported identify result
        // cannot grant one dynamically.
        let summary = registry.get(&id).await.unwrap();
        assert!(
            summary
                .capabilities
                .iter()
                .all(|c| c.capability_id.as_str() == "identify")
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn concurrent_register_deregister_discover_and_reconcile_do_not_corrupt_state() {
        let registry = Arc::new(ConnectorRegistry::new());
        let connector_count = 50usize;

        let mut register_tasks = Vec::new();
        for i in 0..connector_count {
            let registry = Arc::clone(&registry);
            register_tasks.push(tokio::spawn(async move {
                let evidence =
                    test_support::identity_evidence(&format!("conn-{i}"), &format!("material-{i}"));
                registry
                    .register(Box::new(
                        TestConnector::identifiable(&format!("conn-{i}"))
                            .with_default_identity_evidence(evidence),
                    ))
                    .await
            }));
        }
        for task in register_tasks {
            task.await.unwrap().unwrap();
        }
        assert_eq!(registry.list().await.len(), connector_count);

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
                    let source = single_candidate_source("cand-1", "COM3");
                    let _ = registry.list().await;
                    let _ = registry.run_health_cycle(&ctx()).await;
                    let _ = registry.run_discovery_cycle(&source, |_id| ctx()).await;
                }));
            }
        }
        for task in mixed_tasks {
            task.await.unwrap();
        }

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

        // Every odd-indexed connector ran a real discovery cycle with
        // matching identity evidence; reconciliation state must have exactly
        // as many live devices as remaining connectors, never more (no
        // duplication) or fewer (no lost state) from concurrent access.
        assert_eq!(
            registry.live_reconciled_devices().await.len(),
            connector_count / 2
        );
    }
}
