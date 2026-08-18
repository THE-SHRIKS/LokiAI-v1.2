//! End-to-end integration coverage — this crate's public API exactly as a
//! future embedding (e.g. `RUN-003`'s runtime process) would call it, not
//! internal-module coverage (that lives in each `src/*.rs`'s own
//! `#[cfg(test)]` module).

mod support;

use std::sync::Arc;
use std::time::Duration;

use loki_connector_registry::ConnectorRegistry;
use loki_connector_registry::health::AggregatedHealthStatus;
use loki_connector_registry::scheduler::PeriodicHealthChecker;
use loki_connectors_sdk::ids::{ConnectorId, CorrelationId};
use loki_connectors_sdk::operation::OperationContext;
use loki_connectors_sdk::vocabulary::CapabilityRiskLevel;
use support::{Behavior, TestConnector};

fn ctx() -> OperationContext {
    OperationContext::new(CorrelationId::new("corr-integration").unwrap())
}

#[tokio::test]
async fn register_list_deregister_round_trip() {
    let registry = ConnectorRegistry::new();
    assert!(registry.list().await.is_empty());

    let id = registry
        .register(Box::new(TestConnector::identifiable("esp32-like")))
        .await
        .expect("first registration succeeds");
    assert_eq!(id.as_str(), "esp32-like");

    let summaries = registry.list().await;
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].connector_id, id);

    registry
        .deregister(&id)
        .await
        .expect("registered id deregisters");
    assert!(registry.list().await.is_empty());

    // Deregistering again is an honest error, not a silent no-op success.
    assert!(registry.deregister(&id).await.is_err());
}

#[tokio::test]
async fn discovery_cycle_finds_the_conformance_test_double_via_real_lifecycle() {
    let registry = ConnectorRegistry::new();
    registry
        .register(Box::new(TestConnector::identifiable("esp32-like")))
        .await
        .unwrap();

    let outcomes = registry.run_discovery_cycle(|_id| ctx()).await;
    assert_eq!(outcomes.len(), 1);
    let outcome = &outcomes[0];

    assert_eq!(outcome.connector_id.as_str(), "esp32-like");
    assert!(
        outcome.lifecycle.succeeded(),
        "real preflight->execute->verify against the test double must succeed for a \
         connector configured to succeed at every stage"
    );
    let identity = outcome
        .identity
        .as_ref()
        .expect("successful identify produces identity");
    assert_eq!(identity.connector_id.as_str(), "esp32-like");
}

#[tokio::test]
async fn discovery_cycle_of_an_empty_registry_returns_no_outcomes() {
    let registry = ConnectorRegistry::new();
    let outcomes = registry.run_discovery_cycle(|_id| ctx()).await;
    assert!(outcomes.is_empty());
}

#[tokio::test]
async fn identify_failure_is_reported_honestly_not_fabricated() {
    let registry = ConnectorRegistry::new();
    registry
        .register(Box::new(
            TestConnector::identifiable("flaky")
                .with_identify_execute(Behavior::Fail("transport disconnected".to_string())),
        ))
        .await
        .unwrap();

    let outcomes = registry.run_discovery_cycle(|_id| ctx()).await;
    assert_eq!(outcomes.len(), 1);
    assert!(!outcomes[0].lifecycle.succeeded());
    assert!(outcomes[0].identity.is_none());
}

#[tokio::test]
async fn unhealthy_connector_status_is_reflected_not_masked() {
    let registry = ConnectorRegistry::new();
    let healthy_id = registry
        .register(Box::new(TestConnector::identifiable("healthy-one")))
        .await
        .unwrap();
    let unhealthy_id = registry
        .register(Box::new(TestConnector::unhealthy("unhealthy-one")))
        .await
        .unwrap();

    registry.run_health_cycle(&ctx()).await;

    let healthy_status = registry.health_status(&healthy_id).await.unwrap();
    assert_eq!(healthy_status.aggregated, AggregatedHealthStatus::Healthy);

    let unhealthy_status = registry.health_status(&unhealthy_id).await.unwrap();
    assert_eq!(
        unhealthy_status.aggregated,
        AggregatedHealthStatus::Unhealthy,
        "a connector reporting a failed health check must never be shown as healthy"
    );
    assert!(
        unhealthy_status
            .report
            .entries
            .iter()
            .any(|e| e.detail.as_deref() == Some("test double configured to report failure")),
        "the underlying failure detail must be preserved, not summarized away"
    );
}

#[tokio::test]
async fn health_status_of_an_unregistered_connector_is_none_not_fabricated() {
    let registry = ConnectorRegistry::new();
    let unknown = ConnectorId::new("never-registered").unwrap();
    assert!(registry.health_status(&unknown).await.is_none());
}

#[tokio::test]
async fn capability_risk_and_support_tier_project_correctly_from_the_manifest() {
    let registry = ConnectorRegistry::new();
    registry
        .register(Box::new(TestConnector::identifiable("esp32-like")))
        .await
        .unwrap();

    let summary = registry
        .get(&ConnectorId::new("esp32-like").unwrap())
        .await
        .expect("registered connector has a projected summary");

    assert_eq!(summary.capabilities.len(), 1);
    let identify = &summary.capabilities[0];
    assert_eq!(identify.capability_id.as_str(), "identify");
    assert_eq!(identify.risk, CapabilityRiskLevel::Low);
    assert!(identify.verification_required);

    // TestConnector::identifiable declares no certification entries — the
    // projection must stay empty, not invent a default tier.
    assert!(summary.certification.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn periodic_health_checker_updates_status_over_multiple_cycles() {
    let registry = Arc::new(ConnectorRegistry::new());
    let id = registry
        .register(Box::new(TestConnector::identifiable("esp32-like")))
        .await
        .unwrap();

    let checker = PeriodicHealthChecker::spawn(Arc::clone(&registry), Duration::from_millis(10));
    tokio::time::sleep(Duration::from_millis(120)).await;
    checker.stop().await;

    let status = registry
        .health_status(&id)
        .await
        .expect("periodic checker ran at least once");
    assert_eq!(status.aggregated, AggregatedHealthStatus::Healthy);
}

#[tokio::test]
async fn duplicate_registration_of_the_same_connector_id_is_rejected() {
    let registry = ConnectorRegistry::new();
    registry
        .register(Box::new(TestConnector::identifiable("dup")))
        .await
        .unwrap();
    let err = registry
        .register(Box::new(TestConnector::identifiable("dup")))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("already registered"));
}
