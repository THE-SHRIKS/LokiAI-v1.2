//! Support-tier and capability projection — this node's Deliverable 4:
//! "what `Docs/CONNECTOR_SPEC.md` calls a capability's support tier and risk
//! level are surfaced correctly from the registered connector's manifest."
//!
//! `Docs/CONNECTOR_SPEC.md` keeps these on two different axes and this
//! module deliberately does not collapse them into one invented field:
//!
//! - §6 "capabilities and risk classification": `risk` is declared **per
//!   capability** (`CapabilityDescriptor::risk`).
//! - §3 "physical certification matrix" / §2 "Support levels": `support_tier`
//!   is declared **per certified target** (`CertificationEntry::target_id` +
//!   `::support_tier`), not per capability — a connector can be `Certified`
//!   for one board and only `Supported` for another while offering the exact
//!   same capability set to both.
//!
//! [`RegisteredConnectorSummary`] therefore surfaces both lists side by side,
//! each projected verbatim from the registered connector's own
//! `ConnectorManifest` — nothing here is invented, defaulted, or inferred
//! beyond what the manifest itself declared at registration time (`AGENTS.md`
//! rule 1).

use loki_connectors_sdk::ids::{CapabilityId, ConnectorId};
use loki_connectors_sdk::manifest::ConnectorManifest;
use loki_connectors_sdk::vocabulary::{CapabilityRiskLevel, DeviceLifecycleState, SupportTier};
use semver::Version;

/// One capability's risk/verification/recovery contract, projected from
/// `Docs/CONNECTOR_SPEC.md` §6's `CapabilityDescriptor` — everything §6
/// requires an advertised capability to include, except `schema_ref`/
/// `schema_version`/`timeout_ms` (mechanical invocation details a registry
/// listing has no product reason to surface; a caller that needs those still
/// has the full `ConnectorManifest` available via
/// [`ConnectorRegistry::manifest`](crate::registry::ConnectorRegistry::manifest)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityProjection {
    pub capability_id: CapabilityId,
    pub risk: CapabilityRiskLevel,
    pub required_state: Vec<DeviceLifecycleState>,
    pub exclusive_access: bool,
    pub verification_required: bool,
    pub recovery_supported: bool,
}

/// One certified/supported target, projected from §3's "physical
/// certification matrix and evidence references" / §2's support-level
/// table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificationProjection {
    pub target_id: String,
    pub support_tier: SupportTier,
    pub evidence_ref: String,
}

/// The full read-only view [`ConnectorRegistry::list`](crate::registry::ConnectorRegistry::list)
/// and [`ConnectorRegistry::get`](crate::registry::ConnectorRegistry::get)
/// return for one registered connector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredConnectorSummary {
    pub connector_id: ConnectorId,
    pub version: Version,
    pub publisher: String,
    pub capabilities: Vec<CapabilityProjection>,
    pub certification: Vec<CertificationProjection>,
}

/// Builds a [`RegisteredConnectorSummary`] from a connector's own manifest —
/// the single place this projection is computed, so `list`/`get` (and any
/// future caller) never diverge.
pub(crate) fn project(manifest: &ConnectorManifest) -> RegisteredConnectorSummary {
    RegisteredConnectorSummary {
        connector_id: manifest.connector_id.clone(),
        version: manifest.version.clone(),
        publisher: manifest.publisher.clone(),
        capabilities: manifest
            .capabilities
            .iter()
            .map(|c| CapabilityProjection {
                capability_id: c.capability_id.clone(),
                risk: c.risk,
                required_state: c.required_state.clone(),
                exclusive_access: c.exclusive_access,
                verification_required: c.verification_required,
                recovery_supported: c.recovery_supported,
            })
            .collect(),
        certification: manifest
            .certification
            .iter()
            .map(|c| CertificationProjection {
                target_id: c.target_id.clone(),
                support_tier: c.support_tier,
                evidence_ref: c.evidence_ref.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_connectors_sdk::capability::CapabilityDescriptor;
    use loki_connectors_sdk::manifest::{CertificationEntry, Provenance};
    use semver::VersionReq;
    use std::num::NonZeroU32;

    fn manifest() -> ConnectorManifest {
        let capability = CapabilityDescriptor::new(
            CapabilityId::new("flash_artifact").unwrap(),
            "schema://flash_artifact",
            "1.0.0",
            CapabilityRiskLevel::High,
            vec![DeviceLifecycleState::Ready],
            NonZeroU32::new(60_000).unwrap(),
        )
        .unwrap()
        .with_verification_required(true)
        .with_recovery_supported(true);

        ConnectorManifest::new(
            ConnectorId::new("loki-mock-connector").unwrap(),
            Version::parse("1.2.3").unwrap(),
            "LokiAI",
            Provenance::new("sha256:aa", "src").unwrap(),
            VersionReq::parse(">=1.0.0").unwrap(),
        )
        .unwrap()
        .with_capability(capability)
        .with_certification(
            CertificationEntry::new("board-esp32-s3-ref", SupportTier::Custom, "evidence://x")
                .unwrap(),
        )
    }

    #[test]
    fn projects_capability_risk_and_certification_support_tier_verbatim() {
        let summary = project(&manifest());

        assert_eq!(summary.connector_id.as_str(), "loki-mock-connector");
        assert_eq!(summary.version, Version::parse("1.2.3").unwrap());

        assert_eq!(summary.capabilities.len(), 1);
        let cap = &summary.capabilities[0];
        assert_eq!(cap.capability_id.as_str(), "flash_artifact");
        assert_eq!(cap.risk, CapabilityRiskLevel::High);
        assert!(cap.verification_required);
        assert!(cap.recovery_supported);

        assert_eq!(summary.certification.len(), 1);
        let cert = &summary.certification[0];
        assert_eq!(cert.target_id, "board-esp32-s3-ref");
        assert_eq!(cert.support_tier, SupportTier::Custom);
    }

    #[test]
    fn empty_manifest_projects_empty_lists_not_fabricated_defaults() {
        let m = ConnectorManifest::new(
            ConnectorId::new("bare").unwrap(),
            Version::parse("0.1.0").unwrap(),
            "LokiAI",
            Provenance::new("sha256:bb", "src").unwrap(),
            VersionReq::parse("*").unwrap(),
        )
        .unwrap();
        let summary = project(&m);
        assert!(summary.capabilities.is_empty());
        assert!(summary.certification.is_empty());
    }
}
