//! Grant generation identity — ADR-0006 "Decision" step 1: "revalidate the
//! opaque principal credential and exact durable grant generation."
//!
//! # `GrantGeneration` is not `loki_local_authority::LocalSession`
//!
//! This is `P1-MUTATION-CONTRACT-001` §9 open question 3, recorded here per
//! the packet's requirement to "document the distinction, not a resolution
//! of how they will later relate": `loki_local_authority::LocalSession`
//! (`crates/loki-local-authority/src/model.rs`) is a short-lived,
//! per-IPC-session token (1s..=24h lifetime, tied one-to-one to a completed
//! IPC handshake's `ipc_session_id`). ADR-0006's "grant generation" is a
//! durable credential epoch for one principal that must remain valid and
//! comparable across many IPC sessions of that same principal, so a
//! revalidation inside a single acceptance transaction can detect "this
//! principal's authority was rotated/revoked since the caller last checked"
//! even though the caller's current IPC session is still open. No code
//! today derives one from the other; this crate defines `GrantGeneration`
//! as a new, distinct type and does not assume that relationship —
//! reconciling `GrantGeneration` with `LocalSession` (if at all) is
//! `P1-PRINCIPAL-GRANT-001`'s named scope, not this node's.

use crate::error::MutationContractError;
use crate::result::MutationRejection;

const MAX_FIELD: usize = 256;

/// One durable grant generation for one principal: the identity/generation
/// pair a `MutationAcceptanceRequest` is revalidated against, plus its
/// issued/expiry/revocation timestamps. Opaque and immutable once
/// constructed — this crate never mutates a `GrantGeneration` in place;
/// revocation is represented by constructing a new value with
/// `revoked_at_ms` set (the durable store, not this crate, decides how a
/// revocation is persisted and looked up).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantGeneration {
    principal_sid: String,
    generation: u64,
    issued_at_ms: i64,
    expires_at_ms: i64,
    revoked_at_ms: Option<i64>,
}

impl GrantGeneration {
    /// Validates and constructs a `GrantGeneration`. Non-panicking on any
    /// caller input (`AGENTS.md` rule 4).
    pub fn new(
        principal_sid: impl Into<String>,
        generation: u64,
        issued_at_ms: i64,
        expires_at_ms: i64,
        revoked_at_ms: Option<i64>,
    ) -> Result<Self, MutationContractError> {
        let principal_sid = principal_sid.into();
        validate_field("principal_sid", &principal_sid, MAX_FIELD)?;
        if issued_at_ms < 0 {
            return Err(MutationContractError::InvalidInput {
                field: "issued_at_ms",
                reason: "must be non-negative UTC epoch milliseconds".to_string(),
            });
        }
        if expires_at_ms <= issued_at_ms {
            return Err(MutationContractError::InvalidInput {
                field: "expires_at_ms",
                reason: "must be strictly after issued_at_ms".to_string(),
            });
        }
        if let Some(revoked_at_ms) = revoked_at_ms
            && revoked_at_ms < issued_at_ms
        {
            return Err(MutationContractError::InvalidInput {
                field: "revoked_at_ms",
                reason: "must not precede issued_at_ms".to_string(),
            });
        }
        Ok(Self {
            principal_sid,
            generation,
            issued_at_ms,
            expires_at_ms,
            revoked_at_ms,
        })
    }

    pub fn principal_sid(&self) -> &str {
        &self.principal_sid
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn issued_at_ms(&self) -> i64 {
        self.issued_at_ms
    }

    pub fn expires_at_ms(&self) -> i64 {
        self.expires_at_ms
    }

    pub fn revoked_at_ms(&self) -> Option<i64> {
        self.revoked_at_ms
    }
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

/// Pure evaluation of a `GrantGeneration` against `now_ms`. ADR-0006:
/// "Revocation or expiry before commit denies acceptance." This function
/// performs no I/O and does not look anything up — the caller
/// (`P1-ACCEPTANCE-STORE-001`, inside its real transaction) is responsible
/// for fetching the exact current `GrantGeneration` row for the requesting
/// principal before calling this.
pub fn evaluate_grant(generation: &GrantGeneration, now_ms: i64) -> Result<(), MutationRejection> {
    if generation.revoked_at_ms.is_some() {
        return Err(MutationRejection::GrantRevoked);
    }
    if now_ms >= generation.expires_at_ms {
        return Err(MutationRejection::GrantExpired);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> GrantGeneration {
        GrantGeneration::new("S-1-5-21-principal", 1, 1_000, 10_000, None).unwrap()
    }

    #[test]
    fn malformed_grant_generation_is_rejected() {
        assert!(matches!(
            GrantGeneration::new("", 1, 1_000, 10_000, None),
            Err(MutationContractError::InvalidInput {
                field: "principal_sid",
                ..
            })
        ));
        let oversized = "s".repeat(MAX_FIELD + 1);
        assert!(matches!(
            GrantGeneration::new(oversized, 1, 1_000, 10_000, None),
            Err(MutationContractError::InvalidInput {
                field: "principal_sid",
                ..
            })
        ));
        assert!(matches!(
            GrantGeneration::new("bad\u{0007}sid", 1, 1_000, 10_000, None),
            Err(MutationContractError::InvalidInput {
                field: "principal_sid",
                ..
            })
        ));
        assert!(matches!(
            GrantGeneration::new("S-1-5-21-principal", 1, -1, 10_000, None),
            Err(MutationContractError::InvalidInput {
                field: "issued_at_ms",
                ..
            })
        ));
        assert!(matches!(
            GrantGeneration::new("S-1-5-21-principal", 1, 10_000, 10_000, None),
            Err(MutationContractError::InvalidInput {
                field: "expires_at_ms",
                ..
            })
        ));
        assert!(matches!(
            GrantGeneration::new("S-1-5-21-principal", 1, 5_000, 10_000, Some(1_000)),
            Err(MutationContractError::InvalidInput {
                field: "revoked_at_ms",
                ..
            })
        ));
    }

    #[test]
    fn revoked_grant_before_evaluation_time_denies_acceptance() {
        let generation =
            GrantGeneration::new("S-1-5-21-principal", 1, 1_000, 10_000, Some(2_000)).unwrap();
        assert!(matches!(
            evaluate_grant(&generation, 3_000),
            Err(MutationRejection::GrantRevoked)
        ));
    }

    #[test]
    fn expired_grant_denies_acceptance() {
        let generation = valid();
        assert!(matches!(
            evaluate_grant(&generation, 10_000),
            Err(MutationRejection::GrantExpired)
        ));
        assert!(matches!(
            evaluate_grant(&generation, 50_000),
            Err(MutationRejection::GrantExpired)
        ));
    }

    #[test]
    fn valid_grant_within_window_is_accepted() {
        let generation = valid();
        assert!(evaluate_grant(&generation, 5_000).is_ok());
    }
}
