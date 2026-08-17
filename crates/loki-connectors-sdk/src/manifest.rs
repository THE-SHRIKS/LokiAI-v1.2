//! Connector package manifest — `Docs/CONNECTOR_SPEC.md` §3 "Required
//! metadata". Every bullet in that list gets a named field below; where §3
//! names a concept without a defined shape (e.g. "match rules", "retention
//! ... hints"), this crate represents it as a validated free-text field
//! rather than inventing a parser/DSL the spec does not define — see each
//! field's doc comment for its exact §3 source bullet.

use semver::{Version, VersionReq};

use crate::capability::CapabilityDescriptor;
use crate::error::ValidationError;
use crate::ids::ConnectorId;
use crate::validate::non_empty_bounded;
use crate::vocabulary::SupportTier;

/// §3 "publisher and integrity/signature/provenance".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// Content-integrity digest of the connector package (e.g.
    /// `"sha256:..."`). This crate does not compute or verify digests — it
    /// only carries an already-computed one.
    pub integrity_digest: String,
    /// Reference to a detached signature, if the publisher signs releases.
    pub signature_ref: Option<String>,
    /// Where the package came from (registry URL, build provenance
    /// attestation reference, etc.).
    pub source_ref: String,
}

impl Provenance {
    pub fn new(
        integrity_digest: impl Into<String>,
        source_ref: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let integrity_digest = integrity_digest.into();
        non_empty_bounded("integrity_digest", &integrity_digest, 256)?;
        let source_ref = source_ref.into();
        non_empty_bounded("source_ref", &source_ref, 256)?;
        Ok(Self {
            integrity_digest,
            signature_ref: None,
            source_ref,
        })
    }

    #[must_use]
    pub fn with_signature_ref(mut self, signature_ref: impl Into<String>) -> Self {
        self.signature_ref = Some(signature_ref.into());
        self
    }
}

/// §3 "supported OS/architectures". Free-text, not a closed enum:
/// `Docs/TECH_STACK.md` (out of this node's required reading) is where exact
/// supported target triples would be pinned; this crate does not duplicate
/// that decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformTarget {
    pub os: String,
    pub arch: String,
}

impl PlatformTarget {
    pub fn new(os: impl Into<String>, arch: impl Into<String>) -> Result<Self, ValidationError> {
        let os = os.into();
        non_empty_bounded("os", &os, 64)?;
        let arch = arch.into();
        non_empty_bounded("arch", &arch, 64)?;
        Ok(Self { os, arch })
    }
}

/// §3 "device families, board/module identifiers, and match rules".
/// `match_rule` is an opaque, connector-defined descriptor: §3 requires
/// match rules to exist but does not define a matching grammar, so this
/// crate does not invent one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFamilyMatch {
    pub family_id: String,
    pub board_ids: Vec<String>,
    pub match_rule: String,
}

impl DeviceFamilyMatch {
    pub fn new(
        family_id: impl Into<String>,
        board_ids: Vec<String>,
        match_rule: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let family_id = family_id.into();
        non_empty_bounded("family_id", &family_id, 128)?;
        let match_rule = match_rule.into();
        non_empty_bounded("match_rule", &match_rule, 256)?;
        Ok(Self {
            family_id,
            board_ids,
            match_rule,
        })
    }
}

/// §3 "transports and required host permissions/drivers". `transport` is
/// free text (`Docs/CONNECTOR_SPEC.md` §5 gives examples — "COM port, IP
/// address, BLE address" — but never a closed list, unlike, e.g., health
/// status).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportRequirement {
    pub transport: String,
    pub required_permissions: Vec<String>,
    pub required_drivers: Vec<String>,
}

impl TransportRequirement {
    pub fn new(transport: impl Into<String>) -> Result<Self, ValidationError> {
        let transport = transport.into();
        non_empty_bounded("transport", &transport, 64)?;
        Ok(Self {
            transport,
            required_permissions: Vec::new(),
            required_drivers: Vec::new(),
        })
    }

    #[must_use]
    pub fn with_required_permissions(mut self, required_permissions: Vec<String>) -> Self {
        self.required_permissions = required_permissions;
        self
    }

    #[must_use]
    pub fn with_required_drivers(mut self, required_drivers: Vec<String>) -> Self {
        self.required_drivers = required_drivers;
        self
    }
}

/// §3 "external toolchain names, pinned versions, integrity source, and
/// licenses" (see also §9 "External toolchains").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainDescriptor {
    pub name: String,
    pub pinned_version: String,
    pub integrity_source: String,
    pub license: String,
}

impl ToolchainDescriptor {
    pub fn new(
        name: impl Into<String>,
        pinned_version: impl Into<String>,
        integrity_source: impl Into<String>,
        license: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let name = name.into();
        non_empty_bounded("name", &name, 128)?;
        let pinned_version = pinned_version.into();
        non_empty_bounded("pinned_version", &pinned_version, 64)?;
        let integrity_source = integrity_source.into();
        non_empty_bounded("integrity_source", &integrity_source, 256)?;
        let license = license.into();
        non_empty_bounded("license", &license, 128)?;
        Ok(Self {
            name,
            pinned_version,
            integrity_source,
            license,
        })
    }
}

/// A named/versioned schema pointer, matching how
/// `packages/contracts/src/connector.ts`'s capability descriptor points at
/// `schema_ref`/`schema_version` instead of embedding a full schema inline
/// (that module's own doc comment explains why: no schema-compiler
/// toolchain "merely for convenience"). Reused here for §3's "telemetry/log
/// schemas" bullet, which has no existing TS shape to mirror field-for-field
/// against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaRef {
    pub schema_ref: String,
    pub schema_version: String,
}

impl SchemaRef {
    pub fn new(
        schema_ref: impl Into<String>,
        schema_version: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let schema_ref = schema_ref.into();
        non_empty_bounded("schema_ref", &schema_ref, 200)?;
        let schema_version = schema_version.into();
        non_empty_bounded("schema_version", &schema_version, 50)?;
        Ok(Self {
            schema_ref,
            schema_version,
        })
    }
}

/// §3 "physical certification matrix and evidence references", read
/// together with §2's support-level table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificationEntry {
    pub target_id: String,
    pub support_tier: SupportTier,
    pub evidence_ref: String,
}

impl CertificationEntry {
    pub fn new(
        target_id: impl Into<String>,
        support_tier: SupportTier,
        evidence_ref: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let target_id = target_id.into();
        non_empty_bounded("target_id", &target_id, 128)?;
        let evidence_ref = evidence_ref.into();
        non_empty_bounded("evidence_ref", &evidence_ref, 256)?;
        Ok(Self {
            target_id,
            support_tier,
            evidence_ref,
        })
    }
}

/// The full connector package manifest, `Docs/CONNECTOR_SPEC.md` §3.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectorManifest {
    pub connector_id: ConnectorId,
    pub version: Version,
    pub publisher: String,
    pub provenance: Provenance,
    pub runtime_protocol_range: VersionReq,
    pub supported_platforms: Vec<PlatformTarget>,
    pub device_families: Vec<DeviceFamilyMatch>,
    pub transports: Vec<TransportRequirement>,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub toolchains: Vec<ToolchainDescriptor>,
    pub telemetry_schema: Option<SchemaRef>,
    pub log_schema: Option<SchemaRef>,
    pub retention_redaction_hint: Option<String>,
    pub certification: Vec<CertificationEntry>,
}

impl ConnectorManifest {
    pub fn new(
        connector_id: ConnectorId,
        version: Version,
        publisher: impl Into<String>,
        provenance: Provenance,
        runtime_protocol_range: VersionReq,
    ) -> Result<Self, ValidationError> {
        let publisher = publisher.into();
        non_empty_bounded("publisher", &publisher, 200)?;
        Ok(Self {
            connector_id,
            version,
            publisher,
            provenance,
            runtime_protocol_range,
            supported_platforms: Vec::new(),
            device_families: Vec::new(),
            transports: Vec::new(),
            capabilities: Vec::new(),
            toolchains: Vec::new(),
            telemetry_schema: None,
            log_schema: None,
            retention_redaction_hint: None,
            certification: Vec::new(),
        })
    }

    #[must_use]
    pub fn with_supported_platform(mut self, platform: PlatformTarget) -> Self {
        self.supported_platforms.push(platform);
        self
    }

    #[must_use]
    pub fn with_device_family(mut self, family: DeviceFamilyMatch) -> Self {
        self.device_families.push(family);
        self
    }

    #[must_use]
    pub fn with_transport(mut self, transport: TransportRequirement) -> Self {
        self.transports.push(transport);
        self
    }

    #[must_use]
    pub fn with_capability(mut self, capability: CapabilityDescriptor) -> Self {
        self.capabilities.push(capability);
        self
    }

    #[must_use]
    pub fn with_toolchain(mut self, toolchain: ToolchainDescriptor) -> Self {
        self.toolchains.push(toolchain);
        self
    }

    #[must_use]
    pub fn with_telemetry_schema(mut self, schema: SchemaRef) -> Self {
        self.telemetry_schema = Some(schema);
        self
    }

    #[must_use]
    pub fn with_log_schema(mut self, schema: SchemaRef) -> Self {
        self.log_schema = Some(schema);
        self
    }

    #[must_use]
    pub fn with_retention_redaction_hint(mut self, hint: impl Into<String>) -> Self {
        self.retention_redaction_hint = Some(hint.into());
        self
    }

    #[must_use]
    pub fn with_certification(mut self, entry: CertificationEntry) -> Self {
        self.certification.push(entry);
        self
    }

    /// Look up a declared capability by id. `Docs/CONNECTOR_SPEC.md` §4:
    /// connector must not "change undeclared targets or operations" — a
    /// caller can use this to reject a request for a capability this
    /// manifest never declared, before invoking the connector at all.
    pub fn find_capability(
        &self,
        capability_id: &crate::ids::CapabilityId,
    ) -> Option<&CapabilityDescriptor> {
        self.capabilities
            .iter()
            .find(|c| &c.capability_id == capability_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::CapabilityId;
    use crate::vocabulary::{CapabilityRiskLevel, DeviceLifecycleState};
    use std::num::NonZeroU32;
    use std::str::FromStr;

    fn sample_manifest() -> ConnectorManifest {
        ConnectorManifest::new(
            ConnectorId::new("loki-mock-connector").unwrap(),
            Version::parse("1.0.0").unwrap(),
            "LokiAI",
            Provenance::new("sha256:deadbeef", "internal-mock").unwrap(),
            VersionReq::parse(">=1.0.0, <2.0.0").unwrap(),
        )
        .expect("valid manifest inputs")
    }

    #[test]
    fn rejects_empty_publisher() {
        let result = ConnectorManifest::new(
            ConnectorId::new("x").unwrap(),
            Version::parse("1.0.0").unwrap(),
            "",
            Provenance::new("sha256:deadbeef", "src").unwrap(),
            VersionReq::parse("*").unwrap(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn builder_accumulates_repeatable_fields() {
        let cap = CapabilityDescriptor::new(
            CapabilityId::new("identify").unwrap(),
            "schema://identify",
            "1.0.0",
            CapabilityRiskLevel::Low,
            vec![DeviceLifecycleState::Discovered],
            NonZeroU32::new(5_000).unwrap(),
        )
        .unwrap();

        let manifest = sample_manifest()
            .with_capability(cap)
            .with_supported_platform(PlatformTarget::new("windows", "x86_64").unwrap())
            .with_transport(TransportRequirement::new("serial").unwrap())
            .with_certification(
                CertificationEntry::new("board-x", SupportTier::Custom, "evidence://x").unwrap(),
            );

        assert_eq!(manifest.capabilities.len(), 1);
        assert_eq!(manifest.supported_platforms.len(), 1);
        assert_eq!(manifest.transports.len(), 1);
        assert_eq!(manifest.certification.len(), 1);
    }

    #[test]
    fn runtime_protocol_range_uses_real_semver_matching() {
        let manifest = sample_manifest();
        let compatible = Version::parse("1.4.0").unwrap();
        let incompatible = Version::parse("2.0.0").unwrap();
        assert!(manifest.runtime_protocol_range.matches(&compatible));
        assert!(!manifest.runtime_protocol_range.matches(&incompatible));
    }

    #[test]
    fn find_capability_looks_up_by_id() {
        let cap = CapabilityDescriptor::new(
            CapabilityId::new("restart").unwrap(),
            "schema://restart",
            "1.0.0",
            CapabilityRiskLevel::Medium,
            vec![DeviceLifecycleState::Ready],
            NonZeroU32::new(10_000).unwrap(),
        )
        .unwrap();
        let manifest = sample_manifest().with_capability(cap);

        assert!(
            manifest
                .find_capability(&CapabilityId::new("restart").unwrap())
                .is_some()
        );
        assert!(
            manifest
                .find_capability(&CapabilityId::new("flash_artifact").unwrap())
                .is_none()
        );
    }

    #[test]
    fn version_parsing_rejects_non_semver() {
        assert!(Version::from_str("not-a-version").is_err());
    }
}
