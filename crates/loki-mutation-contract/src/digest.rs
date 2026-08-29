//! Immutable operation identity and its one canonical digest algorithm.
//!
//! Per `Docs/graph/tasks/P1-MUTATION-CONTRACT-001.md` §14 (human direction,
//! 2026-08-25): "workspace" is dropped entirely from this digest for Part
//! One rather than represented with a placeholder value. See this crate's
//! `request` module doc comment for the full citation and the still-open
//! ADR-0006 amendment this implies (`P1-MUTATION-CONTRACT-001` §9 item 5).

use sha2::{Digest, Sha256};

use crate::error::MutationContractError;
use crate::method::MutationMethod;

const MAX_FIELD: usize = 256;

/// A canonical, order-stable SHA-256 digest, `"sha256:<64 lowercase hex
/// chars>"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestDigest(String);

impl RequestDigest {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validates an externally-produced digest string into this type.
    pub fn try_from_str(value: impl Into<String>) -> Result<Self, MutationContractError> {
        let value = value.into();
        let hex =
            value
                .strip_prefix("sha256:")
                .ok_or_else(|| MutationContractError::InvalidInput {
                    field: "request_digest",
                    reason: "must start with 'sha256:'".to_string(),
                })?;
        if hex.len() != 64
            || !hex
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(MutationContractError::InvalidInput {
                field: "request_digest",
                reason: "must be 'sha256:' followed by exactly 64 lowercase hex characters"
                    .to_string(),
            });
        }
        Ok(Self(value))
    }
}

impl std::fmt::Display for RequestDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Bounded, validated idempotency key newtype. Reuses the
/// `loki-approvals`/`loki-agents` `validate()` shape/bounds convention:
/// non-empty, <= 256 bytes, no control characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn new(value: impl Into<String>) -> Result<Self, MutationContractError> {
        let value = value.into();
        validate_field("idempotency_key", &value, MAX_FIELD)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The complete immutable identity of one consequential operation. Both the
/// approval service and runtime acceptance must derive their digest from this
/// exact value; neither layer may maintain a second canonicalization scheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationBinding {
    principal_sid: String,
    actor_id: String,
    grant_generation: u64,
    method: MutationMethod,
    target_id: String,
    artifact_digest: Option<String>,
    parameters_digest: String,
    idempotency_key: IdempotencyKey,
}

impl OperationBinding {
    #[expect(
        clippy::too_many_arguments,
        reason = "the closed binding field list is security-significant and must remain explicit"
    )]
    pub fn new(
        principal_sid: impl Into<String>,
        actor_id: impl Into<String>,
        grant_generation: u64,
        method: MutationMethod,
        target_id: impl Into<String>,
        artifact_digest: Option<String>,
        parameters_digest: impl Into<String>,
        idempotency_key: IdempotencyKey,
    ) -> Result<Self, MutationContractError> {
        let binding = Self {
            principal_sid: principal_sid.into(),
            actor_id: actor_id.into(),
            grant_generation,
            method,
            target_id: target_id.into(),
            artifact_digest,
            parameters_digest: parameters_digest.into(),
            idempotency_key,
        };
        validate_field("principal_sid", &binding.principal_sid, MAX_FIELD)?;
        validate_field("actor_id", &binding.actor_id, MAX_FIELD)?;
        if binding.grant_generation == 0 {
            return Err(MutationContractError::InvalidInput {
                field: "grant_generation",
                reason: "must be greater than zero".to_string(),
            });
        }
        validate_field("target_id", &binding.target_id, MAX_FIELD)?;
        validate_field("parameters_digest", &binding.parameters_digest, MAX_FIELD)?;
        if let Some(artifact) = &binding.artifact_digest {
            validate_field("artifact_digest", artifact, MAX_FIELD)?;
        }
        Ok(binding)
    }

    pub fn digest(&self) -> RequestDigest {
        let mut canonical = Vec::new();
        canonical.extend_from_slice(b"lokiai-operation-binding-v1");
        append_field(&mut canonical, &self.principal_sid);
        append_field(&mut canonical, &self.actor_id);
        append_field(&mut canonical, &self.grant_generation.to_string());
        append_field(&mut canonical, self.method.as_str());
        append_field(&mut canonical, &self.target_id);
        match &self.artifact_digest {
            None => canonical.push(0),
            Some(value) => {
                canonical.push(1);
                append_field(&mut canonical, value);
            }
        }
        append_field(&mut canonical, &self.parameters_digest);
        append_field(&mut canonical, self.idempotency_key.as_str());
        RequestDigest(format!("sha256:{:x}", Sha256::digest(canonical)))
    }

    pub fn principal_sid(&self) -> &str {
        &self.principal_sid
    }

    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    pub fn grant_generation(&self) -> u64 {
        self.grant_generation
    }

    pub fn method(&self) -> MutationMethod {
        self.method
    }

    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    pub fn artifact_digest(&self) -> Option<&str> {
        self.artifact_digest.as_deref()
    }

    pub fn parameters_digest(&self) -> &str {
        &self.parameters_digest
    }

    pub fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }
}

fn append_field(canonical: &mut Vec<u8>, value: &str) {
    canonical.extend_from_slice(&(value.len() as u64).to_be_bytes());
    canonical.extend_from_slice(value.as_bytes());
}

fn validate_field(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), MutationContractError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(MutationContractError::InvalidInput {
            field,
            reason: format!("must contain 1..={max} non-control bytes"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> IdempotencyKey {
        IdempotencyKey::new("idem-1").unwrap()
    }

    fn binding(
        principal_sid: &str,
        generation: u64,
        target_id: &str,
        artifact_digest: Option<String>,
    ) -> OperationBinding {
        OperationBinding::new(
            principal_sid,
            "agent-1",
            generation,
            MutationMethod::Flash,
            target_id,
            artifact_digest,
            "sha256:parameters",
            key(),
        )
        .unwrap()
    }

    #[test]
    fn idempotency_key_rejects_malformed_input() {
        assert!(IdempotencyKey::new("").is_err());
        assert!(IdempotencyKey::new("a".repeat(MAX_FIELD + 1)).is_err());
        assert!(IdempotencyKey::new("bad\u{0007}key").is_err());
        assert!(IdempotencyKey::new("idem-1").is_ok());
    }

    #[test]
    fn digest_distinguishes_field_boundaries_and_artifact_absence() {
        let base = binding(
            "S-1-5-21-principal",
            1,
            "target-1",
            Some("sha256:artifact".to_string()),
        )
        .digest();
        let no_artifact = binding("S-1-5-21-principal", 1, "target-1", None).digest();
        assert_ne!(base, no_artifact);

        let boundary_a = binding("ab", 1, "c-target", None).digest();
        let boundary_b = binding("a", 1, "bc-target", None).digest();
        assert_ne!(boundary_a, boundary_b);
    }

    #[test]
    fn digest_changes_with_grant_generation() {
        let gen1 = binding("S-1-5-21-principal", 1, "target-1", None).digest();
        let gen2 = binding("S-1-5-21-principal", 2, "target-1", None).digest();
        assert_ne!(gen1, gen2);
    }

    #[test]
    fn zero_grant_generation_is_rejected() {
        assert!(matches!(
            OperationBinding::new(
                "S-1-5-21-principal",
                "agent-1",
                0,
                MutationMethod::Flash,
                "target-1",
                None,
                "sha256:parameters",
                key(),
            ),
            Err(MutationContractError::InvalidInput {
                field: "grant_generation",
                ..
            })
        ));
    }

    #[test]
    fn digest_changes_with_actor_parameters_and_artifact_tag() {
        let base = binding("S-1-5-21-principal", 1, "target-1", None);
        let actor_changed = OperationBinding::new(
            base.principal_sid(),
            "agent-2",
            base.grant_generation(),
            base.method(),
            base.target_id(),
            None,
            base.parameters_digest(),
            base.idempotency_key().clone(),
        )
        .unwrap();
        let parameters_changed = OperationBinding::new(
            base.principal_sid(),
            base.actor_id(),
            base.grant_generation(),
            base.method(),
            base.target_id(),
            None,
            "sha256:other-parameters",
            base.idempotency_key().clone(),
        )
        .unwrap();
        let literal_absent = OperationBinding::new(
            base.principal_sid(),
            base.actor_id(),
            base.grant_generation(),
            base.method(),
            base.target_id(),
            Some("<absent>".to_string()),
            base.parameters_digest(),
            base.idempotency_key().clone(),
        )
        .unwrap();
        assert_ne!(base.digest(), actor_changed.digest());
        assert_ne!(base.digest(), parameters_changed.digest());
        assert_ne!(base.digest(), literal_absent.digest());
    }

    #[test]
    fn request_digest_try_from_str_validates_shape() {
        let valid = "sha256:".to_string() + &"a".repeat(64);
        assert!(RequestDigest::try_from_str(valid).is_ok());
        assert!(RequestDigest::try_from_str("not-a-digest").is_err());
        assert!(RequestDigest::try_from_str("sha256:tooshort").is_err());
        let uppercase = "sha256:".to_string() + &"A".repeat(64);
        assert!(RequestDigest::try_from_str(uppercase).is_err());
    }
}
