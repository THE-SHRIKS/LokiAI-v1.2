//! Builds this connector's static [`ConnectorManifest`] — `Docs/CONNECTOR_SPEC.md`
//! §3, for the confirmed ESP32-S3 profile (`Docs/CONTEXT.md` OQ-001: QFN56
//! rev v0.2, 8MB embedded PSRAM AP_3v3, 16MB external quad flash, 3.3V,
//! USB-Serial/JTAG native).
//!
//! ## `SupportTier::Custom`, never `Certified` (open question 1 of 3 this
//! node must not resolve silently — recorded in the worker handoff)
//!
//! `Docs/CONNECTOR_SPEC.md` §2: "Certified: LokiAI release tests the exact
//! board/module, connector, toolchain, and required physical scenarios." No
//! such evidence exists — `Docs/graph/handoffs/CON-003-PRE-001-1.md` proved
//! only a candidate transport was present (COM5, Espressif VID/PID), never
//! opened the port, and explicitly found "current registry identity is not
//! device-derived." This node's own packet (orchestrator decision, header)
//! is explicit: `SupportTier::Custom`/undecided, not `Certified`. Every
//! [`CertificationEntry`] below therefore carries `SupportTier::Custom`.
//!
//! ## Toolchain metadata: honestly provisional, not silently omitted
//!
//! `Docs/CONNECTOR_SPEC.md` §10 lists "intended framework/build system and
//! partition/bootloader ownership" as one of six pre-implementation
//! confirmation items; `Docs/CONTEXT.md` OQ-001 resolved the module/flash/
//! PSRAM/voltage/USB-transport rows but explicitly did **not** resolve this
//! one (this node's packet, Section 9, open question 1). This module does
//! not pin the locally-inventoried ESP-IDF checkout
//! (`Docs/graph/handoffs/CON-003-PRE-001-1.md` found one, but that audit
//! itself says "presumed as the intended framework only for inventory; it is
//! not accepted or pinned for production"). Silently omitting the
//! [`ToolchainDescriptor`] entirely risks a future reader misreading "no
//! entry" as "no toolchain needed" rather than "not yet decided" — so this
//! module includes exactly one entry whose every field states, in the field
//! value itself, that it is unresolved, rather than guessing a real
//! framework name/version.

use semver::{Version, VersionReq};

use loki_connectors_sdk::capability::CapabilityDescriptor;
use loki_connectors_sdk::ids::ConnectorId;
use loki_connectors_sdk::manifest::{
    CertificationEntry, ConnectorManifest, DeviceFamilyMatch, PlatformTarget, Provenance,
    ToolchainDescriptor, TransportRequirement,
};
use loki_connectors_sdk::vocabulary::SupportTier;

/// The confirmed board/module target id, `Docs/CONTEXT.md` OQ-001's profile
/// collapsed to one stable slug — used as [`DeviceFamilyMatch::board_ids`]
/// and [`CertificationEntry::target_id`] so both stay in lockstep.
pub const CONFIRMED_BOARD_ID: &str = "esp32-s3-qfn56-v0.2-8mb-psram-16mb-flash";

/// This connector's own declared identifier — distinct from any device
/// identity; `Docs/CONNECTOR_SPEC.md` §3 "connector ID and semantic
/// version".
pub const CONNECTOR_ID: &str = "com.lokiai.connector.esp32-s3";

/// This connector package's own semantic version (not the device's, not the
/// runtime's). `0.1.0`: first implementation, not yet released.
pub const CONNECTOR_VERSION: &str = "0.1.0";

/// Builds the full manifest, wiring in the already-constructed capability
/// descriptors from [`crate::capabilities::build_capabilities`].
pub fn build_manifest(
    capabilities: Vec<CapabilityDescriptor>,
) -> Result<ConnectorManifest, loki_connectors_sdk::error::ValidationError> {
    let provenance = Provenance::new(
        // No signed package exists yet for a `Custom`-tier, unreleased
        // connector — an honest placeholder, not a fabricated digest
        // (AGENTS.md rule 1: "Do not invent ... successful operations").
        "unsigned:not-yet-packaged",
        "internal-development-repository",
    )?;

    let mut manifest = ConnectorManifest::new(
        ConnectorId::new(CONNECTOR_ID)?,
        Version::parse(CONNECTOR_VERSION).expect("literal, valid semver"),
        "LokiAI",
        provenance,
        VersionReq::parse(">=1.0.0, <2.0.0").expect("literal, valid semver range"),
    )?
    .with_supported_platform(PlatformTarget::new("windows", "x86_64")?)
    .with_device_family(DeviceFamilyMatch::new(
        "esp32-s3",
        vec![CONFIRMED_BOARD_ID.to_string()],
        "chip_id.chip_type == \"esp32s3\" && chip_id.silicon_revision == \"v0.2\"",
    )?)
    .with_transport(
        TransportRequirement::new("usb_serial_jtag_native")?
            .with_required_permissions(vec!["serial_port_access".to_string()])
            .with_required_drivers(vec!["usb_serial_jtag_composite".to_string()]),
    )
    .with_toolchain(ToolchainDescriptor::new(
        "unconfirmed_build_toolchain",
        "provisional-unconfirmed-pending-human-decision",
        "Docs/CONNECTOR_SPEC.md §10 item 5 (framework/build-system and \
         partition/bootloader ownership) unresolved as of CON-003-R1 — see \
         Docs/CONTEXT.md OQ-001",
        "unconfirmed",
    )?)
    .with_certification(CertificationEntry::new(
        CONFIRMED_BOARD_ID,
        SupportTier::Custom,
        "Docs/graph/handoffs/CON-003-PRE-001-1.md",
    )?);

    for capability in capabilities {
        manifest = manifest.with_capability(capability);
    }

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::build_capabilities;

    #[test]
    fn manifest_declares_custom_not_certified() {
        let manifest = build_manifest(build_capabilities()).expect("valid manifest");
        assert_eq!(manifest.certification.len(), 1);
        assert_eq!(manifest.certification[0].support_tier, SupportTier::Custom);
    }

    #[test]
    fn manifest_toolchain_is_honestly_provisional() {
        let manifest = build_manifest(build_capabilities()).expect("valid manifest");
        assert_eq!(manifest.toolchains.len(), 1);
        assert!(
            manifest.toolchains[0]
                .pinned_version
                .contains("provisional")
        );
    }

    #[test]
    fn manifest_declares_all_six_capabilities() {
        let manifest = build_manifest(build_capabilities()).expect("valid manifest");
        assert_eq!(manifest.capabilities.len(), 6);
    }
}
