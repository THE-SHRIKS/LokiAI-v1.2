//! Device-neutral discovery and device-derived identity contracts.
//!
//! Transport locators are sensitive, unstable probe inputs. They can never be
//! promoted to [`PublicDeviceIdentity`]. Only validated device-derived evidence
//! produces a stable public ID, and unresolved/ambiguous/conflicting results
//! fail closed.
//!
//! Per the approved Connector Identity Protocol v1.0.0
//! (`Docs/graph/tasks/CON-ID-CONTRACT-001.md` Section 10), the public
//! identity is an *installation-scoped* HMAC-SHA256 of device-derived
//! evidence, not a bare unkeyed hash: [`DeviceIdentityEvidence::new`] takes
//! the caller's per-installation secret as opaque key material so the same
//! physical device yields a different public `device_id` in a different
//! installation. This crate never generates, stores, or loads that secret.

use std::fmt;

use hmac::{Hmac, Mac};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::error::ValidationError;
use crate::validate::non_empty_bounded;

type HmacSha256 = Hmac<Sha256>;

const SCHEMA_VERSION: &str = "1.0.0";
const FINGERPRINT_METHOD: &str = "hmac_sha256_device_material_v1";
const PUBLIC_SCHEME: &str = "lokiai_device_hmac_sha256_v1";
const DOMAIN: &[u8] = b"lokiai-device-identity-v1\0";
/// Minimum length (bytes) accepted for the caller-supplied installation key.
/// Matches this crate's own bounded-input discipline (`AGENTS.md` rule 4);
/// the key itself is generated/stored by whatever owns the per-installation
/// secret (`Docs/ARCHITECTURE.md`), not by this crate.
const MIN_INSTALLATION_KEY_LEN: usize = 32;

#[derive(Clone, PartialEq, Eq)]
pub struct SensitiveUnstableLocator {
    value: String,
    sensitivity: &'static str,
    stability: &'static str,
}

impl SensitiveUnstableLocator {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        non_empty_bounded("locator.value", &value, 512)?;
        Ok(Self {
            value,
            sensitivity: "sensitive",
            stability: "unstable",
        })
    }

    pub fn expose_to_transport(&self) -> &str {
        &self.value
    }

    /// Explicit, deliberately-named wire/transport-only projection that
    /// carries the *real* locator value. The default [`Serialize`] impl
    /// below (used by ordinary JSON serialization, logging, and evidence
    /// persistence) redacts `value`; this method exists solely for the one
    /// legitimate case a connector genuinely needs the real port/USB
    /// descriptor to open the device, and it must be invoked explicitly by
    /// name — it is never reached via `serde_json::to_value`/`to_string` on
    /// a `SensitiveUnstableLocator`, `TransportCandidate`, or
    /// `IdentityProbeResult` by default.
    pub fn serialize_for_transport(&self) -> serde_json::Value {
        serde_json::json!({
            "value": self.value,
            "sensitivity": self.sensitivity,
            "stability": self.stability,
        })
    }
}

/// Redacts the raw locator by default. Ordinary JSON serialization (logging,
/// evidence persistence, any default `serde_json::to_value`/`to_string` call)
/// must never emit the real transport locator. Use
/// [`SensitiveUnstableLocator::serialize_for_transport`] (or
/// [`TransportCandidate::serialize_for_transport`] /
/// [`IdentityProbeResult::serialize_for_transport`]) for the one legitimate
/// wire path that needs the real value.
impl Serialize for SensitiveUnstableLocator {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("SensitiveUnstableLocator", 3)?;
        state.serialize_field("value", "[REDACTED]")?;
        state.serialize_field("sensitivity", self.sensitivity)?;
        state.serialize_field("stability", self.stability)?;
        state.end()
    }
}

impl fmt::Debug for SensitiveUnstableLocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SensitiveUnstableLocator")
            .field("value", &"[REDACTED]")
            .field("sensitivity", &self.sensitivity)
            .field("stability", &self.stability)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransportCandidate {
    schema_version: &'static str,
    candidate_id: String,
    candidate_id_stability: &'static str,
    connector_id: String,
    connector_version: String,
    transport_kind: String,
    locator: SensitiveUnstableLocator,
    discovered_at: String,
}

impl TransportCandidate {
    pub fn new(
        candidate_id: impl Into<String>,
        connector_id: impl Into<String>,
        connector_version: impl Into<String>,
        transport_kind: impl Into<String>,
        locator: SensitiveUnstableLocator,
        discovered_at: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let candidate_id = candidate_id.into();
        non_empty_bounded("candidate_id", &candidate_id, 128)?;
        let connector_id = connector_id.into();
        non_empty_bounded("connector_id", &connector_id, 128)?;
        let connector_version = connector_version.into();
        validate_version(&connector_version)?;
        let transport_kind = transport_kind.into();
        validate_slug("transport_kind", &transport_kind)?;
        let discovered_at = discovered_at.into();
        validate_utc("discovered_at", &discovered_at)?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            candidate_id,
            candidate_id_stability: "ephemeral",
            connector_id,
            connector_version,
            transport_kind,
            locator,
            discovered_at,
        })
    }

    /// Explicit, deliberately-named wire/transport-only projection carrying
    /// the *real* locator value. See
    /// [`SensitiveUnstableLocator::serialize_for_transport`]: this method
    /// exists only for the one legitimate case a connector needs the real
    /// port/USB descriptor to open the device, and the default `Serialize`
    /// impl of `TransportCandidate` still redacts `locator.value` because it
    /// delegates to `SensitiveUnstableLocator`'s redacting `Serialize`.
    pub fn serialize_for_transport(&self) -> serde_json::Value {
        serde_json::json!({
            "schema_version": self.schema_version,
            "candidate_id": self.candidate_id,
            "candidate_id_stability": self.candidate_id_stability,
            "connector_id": self.connector_id,
            "connector_version": self.connector_version,
            "transport_kind": self.transport_kind,
            "locator": self.locator.serialize_for_transport(),
            "discovered_at": self.discovered_at,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityConfidence {
    Weak,
    Moderate,
    Strong,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct DeviceIdentityEvidence {
    evidence_type: String,
    fingerprint_method: &'static str,
    raw_material_handling: &'static str,
    fingerprint_digest: String,
    confidence: IdentityConfidence,
    connector_id: String,
    connector_version: String,
    observed_at: String,
    evidence_digest: String,
}

pub struct DeviceIdentityEvidenceInput<'a> {
    pub evidence_type: String,
    pub fingerprint_material: String,
    /// Opaque per-installation secret (`Docs/ARCHITECTURE.md`'s "per-
    /// installation secret stored in OS-protected storage"). This crate does
    /// not generate, persist, or load it; it only keys the HMAC with it so
    /// the resulting public identity is installation-scoped rather than a
    /// globally portable device fingerprint, per the approved Connector
    /// Identity Protocol v1.0.0 (`Docs/graph/tasks/CON-ID-CONTRACT-001.md`
    /// Section 10). Must be at least 32 bytes.
    pub installation_key: &'a [u8],
    pub confidence: IdentityConfidence,
    pub connector_id: String,
    pub connector_version: String,
    pub observed_at: String,
}

impl DeviceIdentityEvidence {
    pub fn new(input: DeviceIdentityEvidenceInput<'_>) -> Result<Self, ValidationError> {
        let DeviceIdentityEvidenceInput {
            evidence_type,
            fingerprint_material,
            installation_key,
            confidence,
            connector_id,
            connector_version,
            observed_at,
        } = input;
        validate_slug("evidence_type", &evidence_type)?;
        non_empty_bounded("fingerprint_material", &fingerprint_material, 4096)?;
        if installation_key.len() < MIN_INSTALLATION_KEY_LEN {
            return Err(ValidationError::new(
                "installation_key",
                "must be at least 32 bytes",
            ));
        }
        non_empty_bounded("connector_id", &connector_id, 128)?;
        validate_version(&connector_version)?;
        validate_utc("observed_at", &observed_at)?;
        let fingerprint_digest = fingerprint(
            installation_key,
            &evidence_type,
            fingerprint_material.as_bytes(),
        );
        let evidence_digest = fingerprint_digest.clone();
        Ok(Self {
            evidence_type,
            fingerprint_method: FINGERPRINT_METHOD,
            raw_material_handling: "consumed_not_persisted",
            fingerprint_digest,
            confidence,
            connector_id,
            connector_version,
            observed_at,
            evidence_digest,
        })
    }

    pub fn fingerprint_digest(&self) -> &str {
        &self.fingerprint_digest
    }
}

impl fmt::Debug for DeviceIdentityEvidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceIdentityEvidence")
            .field("evidence_type", &self.evidence_type)
            .field("fingerprint_method", &self.fingerprint_method)
            .field("raw_material_handling", &self.raw_material_handling)
            .field("fingerprint_digest", &self.fingerprint_digest)
            .field("confidence", &self.confidence)
            .field("connector_id", &self.connector_id)
            .field("connector_version", &self.connector_version)
            .field("observed_at", &self.observed_at)
            .field("evidence_digest", &self.evidence_digest)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicDeviceIdentity {
    scheme: &'static str,
    device_id: String,
    fingerprint_digest: String,
}

impl PublicDeviceIdentity {
    fn from_digest(digest: &str) -> Self {
        Self {
            scheme: PUBLIC_SCHEME,
            device_id: format!("dev1_{digest}"),
            fingerprint_digest: digest.to_owned(),
        }
    }
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityResolutionStatus {
    Unknown,
    Partial,
    Verified,
    Ambiguous,
    Conflicting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IdentityProbeResult {
    schema_version: &'static str,
    candidate: TransportCandidate,
    status: IdentityResolutionStatus,
    evidence: Vec<DeviceIdentityEvidence>,
    public_identity: Option<PublicDeviceIdentity>,
    reason_code: String,
}

impl IdentityProbeResult {
    pub fn from_json_str(json: &str) -> Result<Self, ValidationError> {
        let wire: ProbeWire = serde_json::from_str(json)
            .map_err(|error| ValidationError::new("identity_probe_result", error.to_string()))?;
        wire.try_into()
    }
    pub fn unresolved(
        candidate: TransportCandidate,
        status: IdentityResolutionStatus,
        evidence: Vec<DeviceIdentityEvidence>,
        reason_code: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        if status == IdentityResolutionStatus::Verified {
            return Err(ValidationError::new(
                "status",
                "verified requires verified constructor",
            ));
        }
        if status == IdentityResolutionStatus::Unknown && !evidence.is_empty() {
            return Err(ValidationError::new(
                "evidence",
                "unknown must not carry evidence",
            ));
        }
        if matches!(
            status,
            IdentityResolutionStatus::Ambiguous | IdentityResolutionStatus::Conflicting
        ) && evidence.len() < 2
        {
            return Err(ValidationError::new(
                "evidence",
                "ambiguous/conflicting requires multiple items",
            ));
        }
        validate_evidence_count(evidence.len())?;
        let reason_code = reason_code.into();
        validate_slug("reason_code", &reason_code)?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            candidate,
            status,
            evidence,
            public_identity: None,
            reason_code,
        })
    }

    pub fn verified(
        candidate: TransportCandidate,
        evidence: Vec<DeviceIdentityEvidence>,
        reason_code: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        validate_evidence_count(evidence.len())?;
        if evidence.is_empty() {
            return Err(ValidationError::new(
                "evidence",
                "verified requires evidence",
            ));
        }
        let digest = evidence[0].fingerprint_digest();
        if evidence
            .iter()
            .any(|item| item.fingerprint_digest() != digest)
        {
            return Err(ValidationError::new(
                "evidence",
                "verified fingerprints conflict",
            ));
        }
        let public_identity = Some(PublicDeviceIdentity::from_digest(digest));
        let reason_code = reason_code.into();
        validate_slug("reason_code", &reason_code)?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            candidate,
            status: IdentityResolutionStatus::Verified,
            evidence,
            public_identity,
            reason_code,
        })
    }

    pub fn public_identity(&self) -> Option<&PublicDeviceIdentity> {
        self.public_identity.as_ref()
    }

    /// Explicit, deliberately-named wire/transport-only projection carrying
    /// the *real* candidate locator value (via
    /// [`TransportCandidate::serialize_for_transport`]). Every other field
    /// here is already safe by construction (evidence carries only hashed
    /// digests, `public_identity` is a derived HMAC projection), so this
    /// differs from the default `Serialize` impl only in that one field. A
    /// caller must invoke this by name; default JSON serialization of an
    /// `IdentityProbeResult` still redacts the raw locator.
    pub fn serialize_for_transport(&self) -> serde_json::Value {
        serde_json::json!({
            "schema_version": self.schema_version,
            "candidate": self.candidate.serialize_for_transport(),
            "status": self.status,
            "evidence": self.evidence,
            "public_identity": self.public_identity,
            "reason_code": self.reason_code,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocatorWire {
    value: String,
    sensitivity: String,
    stability: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateWire {
    schema_version: String,
    candidate_id: String,
    candidate_id_stability: String,
    connector_id: String,
    connector_version: String,
    transport_kind: String,
    locator: LocatorWire,
    discovered_at: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceWire {
    evidence_type: String,
    fingerprint_method: String,
    raw_material_handling: String,
    fingerprint_digest: String,
    confidence: IdentityConfidence,
    connector_id: String,
    connector_version: String,
    observed_at: String,
    evidence_digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicIdentityWire {
    scheme: String,
    device_id: String,
    fingerprint_digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeWire {
    schema_version: String,
    candidate: CandidateWire,
    status: IdentityResolutionStatus,
    evidence: Vec<EvidenceWire>,
    public_identity: Option<PublicIdentityWire>,
    reason_code: String,
}

impl TryFrom<ProbeWire> for IdentityProbeResult {
    type Error = ValidationError;
    fn try_from(wire: ProbeWire) -> Result<Self, Self::Error> {
        if wire.schema_version != SCHEMA_VERSION || wire.candidate.schema_version != SCHEMA_VERSION
        {
            return Err(ValidationError::new(
                "schema_version",
                "unsupported connector identity schema",
            ));
        }
        if wire.candidate.candidate_id_stability != "ephemeral"
            || wire.candidate.locator.sensitivity != "sensitive"
            || wire.candidate.locator.stability != "unstable"
        {
            return Err(ValidationError::new(
                "candidate",
                "candidate and locator security semantics are mandatory",
            ));
        }
        let candidate = TransportCandidate::new(
            wire.candidate.candidate_id,
            wire.candidate.connector_id,
            wire.candidate.connector_version,
            wire.candidate.transport_kind,
            SensitiveUnstableLocator::new(wire.candidate.locator.value)?,
            wire.candidate.discovered_at,
        )?;
        let mut evidence = Vec::with_capacity(wire.evidence.len());
        for item in wire.evidence {
            if item.fingerprint_method != FINGERPRINT_METHOD
                || item.raw_material_handling != "consumed_not_persisted"
            {
                return Err(ValidationError::new(
                    "evidence",
                    "fingerprint method and sensitivity are mandatory",
                ));
            }
            let supplied_fingerprint = item.fingerprint_digest;
            let supplied_evidence_digest = item.evidence_digest;
            if supplied_fingerprint != supplied_evidence_digest {
                return Err(ValidationError::new(
                    "evidence_digest",
                    "does not match canonical identity evidence",
                ));
            }
            validate_sha256("fingerprint_digest", &supplied_fingerprint)?;
            validate_slug("evidence_type", &item.evidence_type)?;
            non_empty_bounded("connector_id", &item.connector_id, 128)?;
            validate_version(&item.connector_version)?;
            validate_utc("observed_at", &item.observed_at)?;
            evidence.push(DeviceIdentityEvidence {
                evidence_type: item.evidence_type,
                fingerprint_method: FINGERPRINT_METHOD,
                raw_material_handling: "consumed_not_persisted",
                fingerprint_digest: supplied_fingerprint,
                confidence: item.confidence,
                connector_id: item.connector_id,
                connector_version: item.connector_version,
                observed_at: item.observed_at,
                evidence_digest: supplied_evidence_digest,
            });
        }
        let result = if wire.status == IdentityResolutionStatus::Verified {
            IdentityProbeResult::verified(candidate, evidence, wire.reason_code)?
        } else {
            IdentityProbeResult::unresolved(candidate, wire.status, evidence, wire.reason_code)?
        };
        match (&result.public_identity, wire.public_identity) {
            (None, None) => Ok(result),
            (Some(expected), Some(actual))
                if actual.scheme == PUBLIC_SCHEME
                    && actual.device_id == expected.device_id
                    && actual.fingerprint_digest == expected.fingerprint_digest =>
            {
                Ok(result)
            }
            _ => Err(ValidationError::new(
                "public_identity",
                "must exactly match verified device-derived evidence",
            )),
        }
    }
}

fn fingerprint(installation_key: &[u8], evidence_type: &str, material: &[u8]) -> String {
    // `Hmac::<Sha256>::new_from_slice` never fails for HMAC (any key length
    // is accepted internally), so this cannot panic; the caller-facing
    // `MIN_INSTALLATION_KEY_LEN` bound above is this crate's own policy, not
    // an HMAC algorithm requirement.
    let mut mac =
        HmacSha256::new_from_slice(installation_key).expect("HMAC accepts any key length");
    mac.update(DOMAIN);
    mac.update(evidence_type.as_bytes());
    mac.update(&[0]);
    mac.update(material);
    let bytes = mac.finalize().into_bytes();
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn validate_evidence_count(len: usize) -> Result<(), ValidationError> {
    if len > 16 {
        Err(ValidationError::new(
            "evidence",
            "must contain at most 16 items",
        ))
    } else {
        Ok(())
    }
}
fn validate_slug(field: &'static str, value: &str) -> Result<(), ValidationError> {
    if value.is_empty()
        || value.len() > 64
        || !value.bytes().enumerate().all(|(i, b)| {
            if i == 0 {
                b.is_ascii_lowercase()
            } else {
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
            }
        })
    {
        Err(ValidationError::new(
            field,
            "must be a lowercase snake_case slug of 1-64 bytes",
        ))
    } else {
        Ok(())
    }
}
fn validate_version(value: &str) -> Result<(), ValidationError> {
    if value.len() > 64 || Version::parse(value).is_err() {
        Err(ValidationError::new(
            "connector_version",
            "must be bounded SemVer",
        ))
    } else {
        Ok(())
    }
}
fn validate_sha256(field: &'static str, value: &str) -> Result<(), ValidationError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(ValidationError::new(field, "must be lowercase SHA-256 hex"))
    }
}
fn validate_utc(field: &'static str, value: &str) -> Result<(), ValidationError> {
    let bytes = value.as_bytes();
    let shape = (20..=30).contains(&bytes.len())
        && value.ends_with('Z')
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':')
        && (bytes.len() == 20 || bytes.get(19) == Some(&b'.'))
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19)
                || index == bytes.len() - 1
                || byte.is_ascii_digit()
        });
    if !shape {
        return Err(ValidationError::new(
            field,
            "must be an ISO-8601 UTC timestamp",
        ));
    }
    let number = |start: usize, end: usize| value[start..end].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        number(0, 4),
        number(5, 7),
        number(8, 10),
        number(11, 13),
        number(14, 16),
        number(17, 19),
    ) else {
        return Err(ValidationError::new(
            field,
            "must be an ISO-8601 UTC timestamp",
        ));
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if day >= 1 && day <= max_day && hour <= 23 && minute <= 59 && second <= 59 {
        Ok(())
    } else {
        Err(ValidationError::new(
            field,
            "must be an ISO-8601 UTC timestamp",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> TransportCandidate {
        TransportCandidate::new(
            "candidate_1",
            "com.lokiai.fixture",
            "1.2.3",
            "fixture_transport",
            SensitiveUnstableLocator::new("COM-fixture").unwrap(),
            "2026-08-24T12:00:00.000Z",
        )
        .unwrap()
    }
    /// 32-byte fixture installation key. Real installation keys are opaque,
    /// OS-protected secrets (`Docs/ARCHITECTURE.md`); this crate never
    /// generates one, so tests use a fixed, clearly-fake value.
    const FIXTURE_INSTALLATION_KEY: &[u8; 32] = b"fixture-installation-key-32bytes";

    fn evidence(material: &str) -> DeviceIdentityEvidence {
        DeviceIdentityEvidence::new(DeviceIdentityEvidenceInput {
            evidence_type: "hardware_uid".into(),
            fingerprint_material: material.into(),
            installation_key: FIXTURE_INSTALLATION_KEY,
            confidence: IdentityConfidence::Strong,
            connector_id: "com.lokiai.fixture".into(),
            connector_version: "1.2.3".into(),
            observed_at: "2026-08-24T12:00:01.000Z".into(),
        })
        .unwrap()
    }

    #[test]
    fn rejects_short_installation_key() {
        let result = DeviceIdentityEvidence::new(DeviceIdentityEvidenceInput {
            evidence_type: "hardware_uid".into(),
            fingerprint_material: "device-material-001".into(),
            installation_key: b"too-short",
            confidence: IdentityConfidence::Strong,
            connector_id: "com.lokiai.fixture".into(),
            connector_version: "1.2.3".into(),
            observed_at: "2026-08-24T12:00:01.000Z".into(),
        });
        assert!(result.is_err());
    }

    #[test]
    fn same_material_differs_by_installation_key() {
        let a = evidence("device-material-001");
        let other_key_evidence = DeviceIdentityEvidence::new(DeviceIdentityEvidenceInput {
            evidence_type: "hardware_uid".into(),
            fingerprint_material: "device-material-001".into(),
            installation_key: b"different-fixture-installation-x",
            confidence: IdentityConfidence::Strong,
            connector_id: "com.lokiai.fixture".into(),
            connector_version: "1.2.3".into(),
            observed_at: "2026-08-24T12:00:01.000Z".into(),
        })
        .unwrap();
        assert_ne!(
            a.fingerprint_digest(),
            other_key_evidence.fingerprint_digest()
        );
    }

    #[test]
    fn derives_expected_public_identity() {
        let result = IdentityProbeResult::verified(
            candidate(),
            vec![evidence("device-material-001")],
            "device_evidence_verified",
        )
        .unwrap();
        assert_eq!(
            result.public_identity().unwrap().device_id(),
            format!(
                "dev1_{}",
                evidence("device-material-001").fingerprint_digest()
            )
        );
    }
    #[test]
    fn unresolved_never_has_public_identity() {
        for status in [
            IdentityResolutionStatus::Unknown,
            IdentityResolutionStatus::Partial,
            IdentityResolutionStatus::Ambiguous,
            IdentityResolutionStatus::Conflicting,
        ] {
            let evidence = if matches!(status, IdentityResolutionStatus::Unknown) {
                vec![]
            } else if matches!(
                status,
                IdentityResolutionStatus::Ambiguous | IdentityResolutionStatus::Conflicting
            ) {
                vec![evidence("a"), evidence("b")]
            } else {
                vec![evidence("a")]
            };
            assert!(
                IdentityProbeResult::unresolved(candidate(), status, evidence, "not_verified")
                    .unwrap()
                    .public_identity()
                    .is_none()
            );
        }
    }
    #[test]
    fn conflicting_verified_evidence_fails_closed() {
        assert!(
            IdentityProbeResult::verified(
                candidate(),
                vec![evidence("a"), evidence("b")],
                "verified"
            )
            .is_err()
        );
    }
    #[test]
    fn sensitive_debug_is_redacted() {
        let e = evidence("raw-secret-material");
        let rendered = format!("{e:?}");
        assert!(rendered.contains("consumed_not_persisted"));
        assert!(!rendered.contains("raw-secret-material"));
        let l = SensitiveUnstableLocator::new("private-path").unwrap();
        assert!(!format!("{l:?}").contains("private-path"));
    }
    #[test]
    fn shared_fixture_round_trips_exactly_via_explicit_transport_path() {
        // The fixture carries the real (fixture) locator value, matching the
        // one legitimate wire use: a connector genuinely needs the real
        // locator once to open the device. That fidelity is only reachable
        // through the explicitly-named `serialize_for_transport` path, never
        // through default `Serialize`.
        let json = include_str!("../../../packages/contracts/fixtures/connector-identity-v1.json");
        let parsed = IdentityProbeResult::from_json_str(json).unwrap();
        let actual = parsed.serialize_for_transport();
        let expected: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn default_serialize_redacts_raw_locator() {
        let json = include_str!("../../../packages/contracts/fixtures/connector-identity-v1.json");
        let parsed = IdentityProbeResult::from_json_str(json).unwrap();

        // Default serialization of the whole probe result must never emit
        // the raw locator value, e.g. via ordinary logging/evidence
        // persistence paths.
        let default_probe = serde_json::to_string(&parsed).unwrap();
        assert!(!default_probe.contains("fixture-locator-not-a-real-device"));
        assert!(default_probe.contains("[REDACTED]"));

        // Same for the candidate and the locator on their own.
        let default_candidate = serde_json::to_string(&candidate()).unwrap();
        assert!(!default_candidate.contains("COM-fixture"));
        assert!(default_candidate.contains("[REDACTED]"));

        let locator = SensitiveUnstableLocator::new("private-path").unwrap();
        let default_locator = serde_json::to_string(&locator).unwrap();
        assert!(!default_locator.contains("private-path"));
        assert!(default_locator.contains("[REDACTED]"));
    }

    #[test]
    fn explicit_transport_path_carries_real_locator() {
        let locator = SensitiveUnstableLocator::new("COM7-real-fixture").unwrap();
        let transport_value = locator.serialize_for_transport();
        assert_eq!(transport_value["value"], "COM7-real-fixture");
        assert_eq!(transport_value["sensitivity"], "sensitive");
        assert_eq!(transport_value["stability"], "unstable");

        let candidate = TransportCandidate::new(
            "candidate_1",
            "com.lokiai.fixture",
            "1.2.3",
            "fixture_transport",
            SensitiveUnstableLocator::new("COM7-real-fixture").unwrap(),
            "2026-08-24T12:00:00.000Z",
        )
        .unwrap();
        let candidate_transport_value = candidate.serialize_for_transport();
        assert_eq!(
            candidate_transport_value["locator"]["value"],
            "COM7-real-fixture"
        );
    }
    #[test]
    fn wire_parser_rejects_extra_fields_and_forged_fingerprint() {
        let json = include_str!("../../../packages/contracts/fixtures/connector-identity-v1.json");
        let mut value: serde_json::Value = serde_json::from_str(json).unwrap();
        value["unexpected"] = serde_json::json!(true);
        assert!(IdentityProbeResult::from_json_str(&value.to_string()).is_err());
        let mut value: serde_json::Value = serde_json::from_str(json).unwrap();
        value["evidence"][0]["fingerprint_digest"] = serde_json::json!("b".repeat(64));
        assert!(IdentityProbeResult::from_json_str(&value.to_string()).is_err());
    }
}
