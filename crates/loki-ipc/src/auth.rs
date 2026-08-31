//! Shared-secret token type, handshake accept/reject decision logic, and
//! dynamic per-credential principal resolution.
//!
//! This module is pure (no I/O): given a server's issued nonce/secret and a
//! candidate `Hello`, it decides accept or a specific [`HandshakeRejectReason`].
//! [`crate::server`] is the only caller that actually reads/writes frames;
//! keeping the decision itself IO-free makes every branch unit-testable
//! directly (see this module's tests) in addition to the end-to-end
//! `tests/auth.rs`.
//!
//! # Dynamic session revalidation (closes REVIEW-002 finding 1)
//!
//! `evaluate_hello_with_resolver` resolves a principal from the presented
//! token exactly once, at `Hello`. Per REVIEW-002: "An established
//! privileged session retains its cloned principal indefinitely, including
//! new calls and subscriptions after revocation or expiry." The fix is
//! [`CredentialResolver::revalidate`]: [`crate::server`] stores only the
//! resolved [`AuthenticatedPrincipal`]'s `principal_id` as an opaque
//! **credential reference** for the life of the connection (never the raw
//! token/secret — "no plaintext credential retained across the
//! revalidation"), and calls `revalidate` again immediately before
//! dispatching every unary call and before authorizing every stream
//! subscription (and periodically while an already-authorized stream is
//! still running — see `crate::server::execute_stream_method`). A revoked
//! or expired credential fails `revalidate` and the call/stream is denied
//! with `RpcError::CredentialRevalidationFailed`, not merely at the next
//! `Hello`.
//!
//! # Where the token comes from
//!
//! This crate does not generate, store, or distribute the production
//! shared secret — that is a deliberate scope boundary (this node is "pure
//! transport", per `Docs/graph/tasks/RUN-002.md` §1). [`ServerConfig`]/
//! [`ClientConfig`] (in [`crate::server`]/[`crate::client`]) accept an
//! [`AuthToken`] as a caller-supplied value.
//!
//! # Why a token *and* an OS ACL
//!
//! The named-pipe DACL ([`crate::acl`]) already stops any process running
//! as a *different* Windows user from ever opening a handle to the pipe.
//! The token adds a second, independent layer: it stops an unrelated
//! process running as the *same* Windows user (which the ACL cannot
//! distinguish — ACLs are per-SID, not per-process) from completing a
//! handshake, since it does not know the shared secret.

use std::collections::BTreeSet;

use uuid::Uuid;

use crate::business::AuthenticatedPrincipal;
use crate::protocol::{HandshakeRejectReason, ProtocolVersion, WireMessage};

/// A local shared secret used to authenticate a connecting client.
///
/// Deliberately opaque (`Debug` does not print the value) so it cannot leak
/// into logs by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthToken(String);

impl AuthToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Generates a fresh, random token. Test/local-development use only.
    pub fn generate() -> Self {
        Self(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuthToken(<redacted>)")
    }
}

impl PartialEq<str> for AuthToken {
    fn eq(&self, other: &str) -> bool {
        constant_time_eq(self.0.as_bytes(), other.as_bytes())
    }
}

/// Constant-time byte comparison, so token comparison does not leak timing
/// information proportional to the number of matching leading bytes.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[derive(Clone)]
pub struct CredentialRecord {
    token: AuthToken,
    pub principal: AuthenticatedPrincipal,
    pub expires_at_unix_millis: Option<u64>,
    pub revoked: bool,
}

impl CredentialRecord {
    pub fn active(token: AuthToken, principal: AuthenticatedPrincipal) -> Self {
        Self {
            token,
            principal,
            expires_at_unix_millis: None,
            revoked: false,
        }
    }

    pub fn expiring_at(mut self, expires_at_unix_millis: u64) -> Self {
        self.expires_at_unix_millis = Some(expires_at_unix_millis);
        self
    }

    pub fn revoked(mut self) -> Self {
        self.revoked = true;
        self
    }

    fn is_active(&self, now_unix_millis: u64) -> bool {
        !self.revoked
            && self
                .expires_at_unix_millis
                .is_none_or(|expiry| now_unix_millis < expiry)
    }
}

/// Resolves a presented token to a principal at `Hello`, and re-checks a
/// previously-resolved principal's *current* revocation/expiry state on
/// every subsequent call/subscription without needing the original
/// plaintext token again. Implementations must be fail-closed: an unknown
/// reference is `None`, never a default/wildcard principal.
pub trait CredentialResolver: Send + Sync {
    /// Called once, at `Hello`, with the client's presented plaintext
    /// token.
    fn resolve(
        &self,
        presented_token: &str,
        now_unix_millis: u64,
    ) -> Option<AuthenticatedPrincipal>;

    /// Called before every subsequent call/subscription dispatch, keyed by
    /// the *already-resolved* principal's `principal_id` — never the raw
    /// token. Returns the principal's current authority (which may have
    /// changed, e.g. a narrower authority set after a policy update) if the
    /// credential is still active, or `None` if it has since been revoked,
    /// expired, or no longer exists.
    fn revalidate(
        &self,
        principal_id: &str,
        now_unix_millis: u64,
    ) -> Option<AuthenticatedPrincipal>;
}

#[derive(Clone)]
pub struct CredentialSet {
    records: Vec<CredentialRecord>,
}

impl CredentialSet {
    pub fn new(records: Vec<CredentialRecord>) -> Self {
        Self { records }
    }

    pub fn single(token: AuthToken, principal: AuthenticatedPrincipal) -> Self {
        Self::new(vec![CredentialRecord::active(token, principal)])
    }

    /// Mutates the record whose principal has `principal_id`, marking it
    /// revoked. Test/local-composition convenience — production credential
    /// mutation is a future node's job (this resolver is an in-memory
    /// fixture, not a durable credential store).
    pub fn revoke(&mut self, principal_id: &str) {
        for record in &mut self.records {
            if record.principal.principal_id == principal_id {
                record.revoked = true;
            }
        }
    }
}

impl CredentialResolver for CredentialSet {
    fn resolve(
        &self,
        presented_token: &str,
        now_unix_millis: u64,
    ) -> Option<AuthenticatedPrincipal> {
        let mut resolved = None;
        for record in &self.records {
            let token_matches = record.token == *presented_token;
            if token_matches && record.is_active(now_unix_millis) {
                resolved = Some(record.principal.clone());
            }
        }
        resolved
    }

    fn revalidate(
        &self,
        principal_id: &str,
        now_unix_millis: u64,
    ) -> Option<AuthenticatedPrincipal> {
        self.records
            .iter()
            .find(|record| record.principal.principal_id == principal_id)
            .filter(|record| record.is_active(now_unix_millis))
            .map(|record| record.principal.clone())
    }
}

/// The outcome of evaluating a client's [`WireMessage::Hello`].
#[derive(Debug)]
pub struct HandshakeAccept {
    pub negotiated_capabilities: BTreeSet<String>,
}

/// Evaluates a candidate `Hello` against this connection's expected state
/// using a single static token/principal (`RUN-002`'s original core-only
/// shape — retained for the legacy default `ServerConfig::new` path and
/// this module's own unit tests). Production composition uses
/// [`evaluate_hello_with_resolver`] for per-credential principal
/// resolution.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_hello(
    client_protocol_version: ProtocolVersion,
    client_capabilities: &BTreeSet<String>,
    hello_nonce: Uuid,
    hello_token: &str,
    server_protocol_version: ProtocolVersion,
    expected_nonce: Uuid,
    expected_token: &AuthToken,
    required_capability: &str,
) -> Result<HandshakeAccept, HandshakeRejectReason> {
    if hello_nonce != expected_nonce {
        return Err(HandshakeRejectReason::StaleNonce);
    }
    if expected_token != hello_token {
        return Err(HandshakeRejectReason::AuthenticationFailed);
    }
    if !server_protocol_version.is_compatible_with(&client_protocol_version) {
        return Err(HandshakeRejectReason::VersionMismatch {
            server_version: server_protocol_version,
            client_version: client_protocol_version,
        });
    }
    if !client_capabilities.contains(required_capability) {
        return Err(HandshakeRejectReason::MissingCapability {
            required: required_capability.to_string(),
        });
    }

    let negotiated_capabilities = client_capabilities
        .intersection(&crate::protocol::capabilities_for_version(
            client_protocol_version,
        ))
        .cloned()
        .collect();

    Ok(HandshakeAccept {
        negotiated_capabilities,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn evaluate_hello_with_resolver(
    client_protocol_version: ProtocolVersion,
    client_capabilities: &BTreeSet<String>,
    hello_nonce: Uuid,
    hello_token: &str,
    server_protocol_version: ProtocolVersion,
    expected_nonce: Uuid,
    credential_resolver: &dyn CredentialResolver,
    required_capability: &str,
    now_unix_millis: u64,
) -> Result<(HandshakeAccept, AuthenticatedPrincipal), HandshakeRejectReason> {
    if hello_nonce != expected_nonce {
        return Err(HandshakeRejectReason::StaleNonce);
    }
    let principal = credential_resolver
        .resolve(hello_token, now_unix_millis)
        .ok_or(HandshakeRejectReason::AuthenticationFailed)?;
    if !server_protocol_version.is_compatible_with(&client_protocol_version) {
        return Err(HandshakeRejectReason::VersionMismatch {
            server_version: server_protocol_version,
            client_version: client_protocol_version,
        });
    }
    if !client_capabilities.contains(required_capability) {
        return Err(HandshakeRejectReason::MissingCapability {
            required: required_capability.to_string(),
        });
    }
    let negotiated_capabilities = client_capabilities
        .intersection(&crate::protocol::capabilities_for_version(
            client_protocol_version,
        ))
        .cloned()
        .collect();
    Ok((
        HandshakeAccept {
            negotiated_capabilities,
        },
        principal,
    ))
}

/// Extracts the fields [`evaluate_hello`] needs from a raw
/// [`WireMessage::Hello`], or a [`HandshakeRejectReason::MalformedHello`] if
/// `message` is not a `Hello` at all (e.g. a client that sends `Call` as
/// its first message, skipping the handshake).
#[derive(Debug)]
pub struct HelloFields {
    pub client_protocol_version: ProtocolVersion,
    pub client_capabilities: BTreeSet<String>,
    pub client_type: String,
    pub client_version: String,
    pub nonce: Uuid,
    pub token: String,
}

pub fn expect_hello(message: WireMessage) -> Result<HelloFields, HandshakeRejectReason> {
    match message {
        WireMessage::Hello {
            client_protocol_version,
            client_capabilities,
            client_type,
            client_version,
            nonce,
            token,
        } => Ok(HelloFields {
            client_protocol_version,
            client_capabilities,
            client_type,
            client_version,
            nonce,
            token,
        }),
        other => Err(HandshakeRejectReason::MalformedHello {
            message: format!(
                "expected Hello as the first client message, got {}",
                wire_message_kind(&other)
            ),
        }),
    }
}

fn wire_message_kind(message: &WireMessage) -> &'static str {
    match message {
        WireMessage::Challenge { .. } => "Challenge",
        WireMessage::Hello { .. } => "Hello",
        WireMessage::HelloAck { .. } => "HelloAck",
        WireMessage::HelloReject { .. } => "HelloReject",
        WireMessage::Call { .. } => "Call",
        WireMessage::CallResult { .. } => "CallResult",
        WireMessage::CallStream { .. } => "CallStream",
        WireMessage::StreamItem { .. } => "StreamItem",
        WireMessage::StreamEnd { .. } => "StreamEnd",
        WireMessage::CancelCall { .. } => "CancelCall",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::BUSINESS_CAPABILITY;
    use crate::protocol::{CORE_CAPABILITY, PROTOCOL_VERSION, baseline_capabilities};

    fn accept(
        client_version: ProtocolVersion,
        client_caps: BTreeSet<String>,
        nonce: Uuid,
        token: &str,
        expected_nonce: Uuid,
        expected_token: &AuthToken,
    ) -> Result<HandshakeAccept, HandshakeRejectReason> {
        evaluate_hello(
            client_version,
            &client_caps,
            nonce,
            token,
            ProtocolVersion::new(1, 0),
            expected_nonce,
            expected_token,
            CORE_CAPABILITY,
        )
    }

    #[test]
    fn matching_nonce_token_version_and_capability_is_accepted() {
        let nonce = Uuid::new_v4();
        let token = AuthToken::new("s3cret");
        let result = accept(
            ProtocolVersion::new(1, 0),
            baseline_capabilities(),
            nonce,
            "s3cret",
            nonce,
            &token,
        );
        assert!(result.is_ok());
        assert!(
            result
                .unwrap()
                .negotiated_capabilities
                .contains(CORE_CAPABILITY)
        );
    }

    #[test]
    fn mismatched_nonce_is_rejected_as_stale_not_auth_failed() {
        let issued = Uuid::new_v4();
        let replayed = Uuid::new_v4();
        let token = AuthToken::new("s3cret");
        let result = accept(
            ProtocolVersion::new(1, 0),
            baseline_capabilities(),
            replayed,
            "s3cret",
            issued,
            &token,
        );
        assert_eq!(result.unwrap_err(), HandshakeRejectReason::StaleNonce);
    }

    #[test]
    fn wrong_token_is_rejected_as_authentication_failed() {
        let nonce = Uuid::new_v4();
        let token = AuthToken::new("s3cret");
        let result = accept(
            ProtocolVersion::new(1, 0),
            baseline_capabilities(),
            nonce,
            "wrong-token",
            nonce,
            &token,
        );
        assert_eq!(
            result.unwrap_err(),
            HandshakeRejectReason::AuthenticationFailed
        );
    }

    #[test]
    fn nonce_is_checked_before_token_so_a_replay_is_never_mistaken_for_a_bad_password() {
        let issued = Uuid::new_v4();
        let replayed = Uuid::new_v4();
        let token = AuthToken::new("s3cret");
        let result = accept(
            ProtocolVersion::new(1, 0),
            baseline_capabilities(),
            replayed,
            "s3cret",
            issued,
            &token,
        );
        assert_eq!(result.unwrap_err(), HandshakeRejectReason::StaleNonce);
    }

    #[test]
    fn major_version_mismatch_is_rejected_with_both_versions_named() {
        let nonce = Uuid::new_v4();
        let token = AuthToken::new("s3cret");
        let result = accept(
            ProtocolVersion::new(99, 0),
            baseline_capabilities(),
            nonce,
            "s3cret",
            nonce,
            &token,
        );
        match result.unwrap_err() {
            HandshakeRejectReason::VersionMismatch {
                server_version,
                client_version,
            } => {
                assert_eq!(server_version, ProtocolVersion::new(1, 0));
                assert_eq!(client_version, ProtocolVersion::new(99, 0));
            }
            other => panic!("expected VersionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn missing_required_capability_is_rejected() {
        let nonce = Uuid::new_v4();
        let token = AuthToken::new("s3cret");
        let result = accept(
            ProtocolVersion::new(1, 0),
            BTreeSet::new(),
            nonce,
            "s3cret",
            nonce,
            &token,
        );
        assert_eq!(
            result.unwrap_err(),
            HandshakeRejectReason::MissingCapability {
                required: CORE_CAPABILITY.to_string()
            }
        );
    }

    #[test]
    fn an_unknown_extra_client_capability_does_not_break_negotiation() {
        let nonce = Uuid::new_v4();
        let token = AuthToken::new("s3cret");
        let mut caps = baseline_capabilities();
        caps.insert("some.future.capability".to_string());
        let result = accept(
            ProtocolVersion::new(1, 0),
            caps,
            nonce,
            "s3cret",
            nonce,
            &token,
        );
        assert!(result.is_ok());
        assert!(
            !result
                .unwrap()
                .negotiated_capabilities
                .contains("some.future.capability")
        );
    }

    #[test]
    fn expect_hello_extracts_fields_from_a_real_hello() {
        let nonce = Uuid::new_v4();
        let message = WireMessage::Hello {
            client_protocol_version: ProtocolVersion::new(1, 0),
            client_capabilities: baseline_capabilities(),
            client_type: "test-client".to_string(),
            client_version: "0.0.0".to_string(),
            nonce,
            token: "s3cret".to_string(),
        };
        let fields = expect_hello(message).expect("must parse");
        assert_eq!(fields.nonce, nonce);
        assert_eq!(fields.token, "s3cret");
        assert_eq!(fields.client_type, "test-client");
    }

    #[test]
    fn expect_hello_rejects_a_non_hello_message_as_malformed() {
        let message = WireMessage::CancelCall {
            correlation_id: Uuid::nil(),
        };
        let result = expect_hello(message);
        match result.unwrap_err() {
            HandshakeRejectReason::MalformedHello { message } => {
                assert!(message.contains("CancelCall"));
            }
            other => panic!("expected MalformedHello, got {other:?}"),
        }
    }

    #[test]
    fn auth_token_debug_never_prints_the_secret() {
        let token = AuthToken::new("super-secret-value");
        let rendered = format!("{token:?}");
        assert!(!rendered.contains("super-secret-value"));
        assert!(rendered.contains("redacted"));
    }

    #[test]
    fn auth_token_generate_produces_distinct_high_entropy_values() {
        let a = AuthToken::generate();
        let b = AuthToken::generate();
        assert_ne!(a.as_str(), b.as_str());
        assert!(a.as_str().len() >= 32);
    }

    #[test]
    fn constant_time_eq_matches_regular_equality_semantics() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"", b"a"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn revoked_and_expired_credentials_fail_closed_at_resolve() {
        let principal = AuthenticatedPrincipal::runtime_service("runtime-1");
        let resolver = CredentialSet::new(vec![
            CredentialRecord::active(AuthToken::new("revoked"), principal.clone()).revoked(),
            CredentialRecord::active(AuthToken::new("expired"), principal).expiring_at(1_000),
        ]);
        assert!(resolver.resolve("revoked", 500).is_none());
        assert!(resolver.resolve("expired", 1_000).is_none());
        assert!(resolver.resolve("unknown", 500).is_none());
    }

    #[test]
    fn v1_0_cannot_negotiate_business_even_when_it_advertises_it() {
        let nonce = Uuid::new_v4();
        let token = AuthToken::new("secret");
        let accepted = evaluate_hello(
            ProtocolVersion::new(1, 0),
            &baseline_capabilities(),
            nonce,
            token.as_str(),
            PROTOCOL_VERSION,
            nonce,
            &token,
            CORE_CAPABILITY,
        )
        .expect("same-major core handshake");
        assert!(
            !accepted
                .negotiated_capabilities
                .contains(BUSINESS_CAPABILITY)
        );
    }

    /// The dynamic-revalidation contract itself, in isolation: a credential
    /// resolved once (`resolve`) and later revoked must fail `revalidate`
    /// even though nothing about the original `resolve` call changes.
    #[test]
    fn revalidate_fails_closed_after_revocation_even_though_resolve_already_succeeded() {
        let principal = AuthenticatedPrincipal {
            kind: crate::business::PrincipalKind::DesktopInstallation,
            principal_id: "desktop-1".to_string(),
            authorized_entity_ids: std::collections::BTreeSet::new(),
            authority: std::collections::BTreeSet::new(),
        };
        let mut resolver = CredentialSet::single(AuthToken::new("secret"), principal);
        assert!(resolver.resolve("secret", 0).is_some());
        assert!(resolver.revalidate("desktop-1", 0).is_some());
        resolver.revoke("desktop-1");
        assert!(resolver.revalidate("desktop-1", 0).is_none());
    }

    #[test]
    fn revalidate_fails_closed_after_expiry() {
        let principal = AuthenticatedPrincipal {
            kind: crate::business::PrincipalKind::AgentConnection,
            principal_id: "agent-1".to_string(),
            authorized_entity_ids: std::collections::BTreeSet::new(),
            authority: std::collections::BTreeSet::new(),
        };
        let resolver = CredentialSet::new(vec![
            CredentialRecord::active(AuthToken::new("secret"), principal).expiring_at(1_000),
        ]);
        assert!(resolver.revalidate("agent-1", 500).is_some());
        assert!(resolver.revalidate("agent-1", 1_000).is_none());
    }

    #[test]
    fn revalidate_of_an_unknown_principal_id_is_none_not_a_default() {
        let resolver = CredentialSet::new(vec![]);
        assert!(resolver.revalidate("nobody", 0).is_none());
    }
}
