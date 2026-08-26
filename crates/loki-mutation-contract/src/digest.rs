//! `RequestDigest` and `IdempotencyKey`, plus [`compute_request_digest`] —
//! generalizes `loki_approvals::ApprovalRequest::digest()`'s length-prefixed
//! field-concatenation pattern (`crates/loki-approvals/src/lib.rs`) over an
//! explicit, closed field list bound by ADR-0006 "Decision" step 2.
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
/// chars>"` — same wire shape as `loki_approvals::ApprovalRequest::digest()`
/// and `loki_approvals::ApprovalRecord::request_digest`, so a value
/// produced by that crate can be validated into this type
/// ([`RequestDigest::try_from_str`]) for equality comparison against a
/// digest this crate computed ([`compute_request_digest`]) without either
/// crate depending on the other.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestDigest(String);

impl RequestDigest {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validates an externally-produced digest string into this type. Does
    /// not compute a digest — see [`compute_request_digest`] for that.
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

/// Computes the canonical request digest over an explicit, closed field
/// list: `principal_sid`, `grant_generation`, `method`, `target_id`,
/// `artifact_digest` (absence and empty-string are distinguished, mirroring
/// `ApprovalRequest::digest()`), and `idempotency_key`. Deliberately does
/// not take a `workspace` parameter — see this module's top doc comment.
///
/// Length-prefixes every field before concatenation (as
/// `ApprovalRequest::digest()` does) so `("ab","c")` and `("a","bc")` never
/// collide.
pub fn compute_request_digest(
    principal_sid: &str,
    grant_generation: u64,
    method: MutationMethod,
    target_id: &str,
    artifact_digest: Option<&str>,
    idempotency_key: &IdempotencyKey,
) -> Result<RequestDigest, MutationContractError> {
    validate_field("principal_sid", principal_sid, MAX_FIELD)?;
    validate_field("target_id", target_id, MAX_FIELD)?;
    if let Some(digest) = artifact_digest {
        validate_field("artifact_digest", digest, MAX_FIELD)?;
    }
    let mut canonical = Vec::new();
    canonical.extend_from_slice(b"lokiai-mutation-v1");
    let generation_field = grant_generation.to_string();
    for value in [
        principal_sid,
        generation_field.as_str(),
        method.as_str(),
        target_id,
        artifact_digest.unwrap_or("<absent>"),
        idempotency_key.as_str(),
    ] {
        canonical.extend_from_slice(&(value.len() as u64).to_be_bytes());
        canonical.extend_from_slice(value.as_bytes());
    }
    Ok(RequestDigest(format!(
        "sha256:{:x}",
        Sha256::digest(canonical)
    )))
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

    #[test]
    fn idempotency_key_rejects_malformed_input() {
        assert!(IdempotencyKey::new("").is_err());
        assert!(IdempotencyKey::new("a".repeat(MAX_FIELD + 1)).is_err());
        assert!(IdempotencyKey::new("bad\u{0007}key").is_err());
        assert!(IdempotencyKey::new("idem-1").is_ok());
    }

    #[test]
    fn digest_distinguishes_field_boundaries_and_artifact_absence() {
        let base = compute_request_digest(
            "S-1-5-21-principal",
            1,
            MutationMethod::Flash,
            "target-1",
            Some("sha256:artifact"),
            &key(),
        )
        .unwrap();
        let no_artifact = compute_request_digest(
            "S-1-5-21-principal",
            1,
            MutationMethod::Flash,
            "target-1",
            None,
            &key(),
        )
        .unwrap();
        assert_ne!(base, no_artifact);

        let boundary_a =
            compute_request_digest("ab", 1, MutationMethod::Flash, "c-target", None, &key())
                .unwrap();
        let boundary_b =
            compute_request_digest("a", 1, MutationMethod::Flash, "bc-target", None, &key())
                .unwrap();
        assert_ne!(boundary_a, boundary_b);
    }

    #[test]
    fn digest_changes_with_grant_generation() {
        let gen1 = compute_request_digest(
            "S-1-5-21-principal",
            1,
            MutationMethod::Restart,
            "target-1",
            None,
            &key(),
        )
        .unwrap();
        let gen2 = compute_request_digest(
            "S-1-5-21-principal",
            2,
            MutationMethod::Restart,
            "target-1",
            None,
            &key(),
        )
        .unwrap();
        assert_ne!(gen1, gen2);
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
