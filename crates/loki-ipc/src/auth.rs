//! Shared-secret token type and handshake accept/reject decision logic.
//!
//! This module is pure (no I/O): given a server's issued nonce/secret and a
//! candidate `Hello`, it decides accept or a specific [`HandshakeRejectReason`].
//! [`crate::server`] is the only caller that actually reads/writes frames;
//! keeping the decision itself IO-free makes every branch unit-testable
//! directly (see this module's tests) in addition to the end-to-end
//! `tests/auth.rs`.
//!
//! # Where the token comes from
//!
//! This crate does not generate, store, or distribute the production
//! shared secret — that is a deliberate scope boundary (this node is "pure
//! transport", per `Docs/graph/tasks/RUN-002.md` §1). [`ServerConfig`]/
//! [`ClientConfig`] (in [`crate::server`]/[`crate::client`]) accept an
//! [`AuthToken`] as a caller-supplied value. A later node (the real
//! `loki-runtime` integration, `RUN-003`) decides how the runtime process
//! generates/persists it (`Docs/ARCHITECTURE.md` §5: "A per-installation
//! secret stored in OS-protected storage authenticates local clients") and
//! how a legitimate desktop/MCP client obtains its copy. [`AuthToken::generate`]
//! exists only for tests/local development, not as a production
//! distribution mechanism.
//!
//! # Why a token *and* an OS ACL
//!
//! The named-pipe DACL ([`crate::acl`]) already stops any process running
//! as a *different* Windows user from ever opening a handle to the pipe.
//! The token adds a second, independent layer: it stops an unrelated
//! process running as the *same* Windows user (which the ACL cannot
//! distinguish — ACLs are per-SID, not per-process) from completing a
//! handshake, since it does not know the shared secret. Sending the token
//! in cleartext over the pipe does not weaken this: a peer capable of
//! reading pipe traffic has, by construction, already passed the ACL check
//! that gates opening the pipe at all, so nothing is exposed to a principal
//! who could not already open it.

use std::collections::BTreeSet;

use uuid::Uuid;

use crate::business::AuthenticatedPrincipal;
use crate::protocol::{HandshakeRejectReason, ProtocolVersion, WireMessage};

/// A local shared secret used to authenticate a connecting client.
///
/// Deliberately opaque (`Debug` does not print the value) so it cannot leak
/// into logs by accident — the whole point of `Docs/SECURITY.md`'s "Logs
/// record credential references/fingerprints only, never credential
/// values" for exactly this class of value.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthToken(String);

impl AuthToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Generates a fresh, random token. Test/local-development use only —
    /// see this module's top doc comment on why production distribution is
    /// explicitly out of this crate's scope.
    pub fn generate() -> Self {
        // Two random UUIDs concatenated: 256 bits of entropy from
        // `uuid`'s already-justified "v4" feature (see Cargo.toml), rather
        // than adding a dedicated randomness crate for this alone.
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
///
/// Hand-rolled rather than pulling in a dedicated crate (e.g. `subtle`):
/// this is a five-line XOR-accumulate loop, and AGENTS.md rule 11 requires
/// a dependency to be more than a convenience. The threat model here is
/// local IPC with no network timing channel; this is defense in depth, not
/// a hardened cross-network constant-time primitive.
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
    pub credential_id: String,
    token: AuthToken,
    pub principal: AuthenticatedPrincipal,
    pub expires_at_unix_millis: Option<u64>,
    pub revoked: bool,
}

impl CredentialRecord {
    pub fn active(token: AuthToken, principal: AuthenticatedPrincipal) -> Self {
        Self {
            credential_id: Uuid::new_v4().to_string(),
            token,
            principal,
            expires_at_unix_millis: None,
            revoked: false,
        }
    }

    pub fn with_credential_id(mut self, credential_id: impl Into<String>) -> Self {
        self.credential_id = credential_id.into();
        self
    }

    pub fn expiring_at(mut self, expires_at_unix_millis: u64) -> Self {
        self.expires_at_unix_millis = Some(expires_at_unix_millis);
        self
    }

    pub fn revoked(mut self) -> Self {
        self.revoked = true;
        self
    }
}

pub trait CredentialResolver: Send + Sync {
    fn resolve(&self, presented_token: &str, now_unix_millis: u64) -> Option<ResolvedCredential>;

    fn revalidate(
        &self,
        credential_id: &str,
        now_unix_millis: u64,
    ) -> Option<AuthenticatedPrincipal>;
}

#[derive(Clone)]
pub struct ResolvedCredential {
    pub credential_id: String,
    pub principal: AuthenticatedPrincipal,
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
}

impl CredentialResolver for CredentialSet {
    fn resolve(&self, presented_token: &str, now_unix_millis: u64) -> Option<ResolvedCredential> {
        let mut resolved = None;
        let mut active_matches = 0_u8;
        for record in &self.records {
            let token_matches = record.token == *presented_token;
            let active = !record.revoked
                && record
                    .expires_at_unix_millis
                    .is_none_or(|expiry| now_unix_millis < expiry);
            if token_matches && active {
                active_matches = active_matches.saturating_add(1);
                resolved = Some(ResolvedCredential {
                    credential_id: record.credential_id.clone(),
                    principal: record.principal.clone(),
                });
            }
        }
        (active_matches == 1).then_some(resolved).flatten()
    }

    fn revalidate(
        &self,
        credential_id: &str,
        now_unix_millis: u64,
    ) -> Option<AuthenticatedPrincipal> {
        let mut matches = self
            .records
            .iter()
            .filter(|record| record.credential_id == credential_id);
        let record = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        (!record.revoked
            && record
                .expires_at_unix_millis
                .is_none_or(|expiry| now_unix_millis < expiry))
        .then(|| record.principal.clone())
    }
}

/// The outcome of evaluating a client's [`WireMessage::Hello`].
#[derive(Debug)]
pub struct HandshakeAccept {
    pub negotiated_capabilities: BTreeSet<String>,
}

/// Evaluates a candidate `Hello` against this connection's expected state.
///
/// `expected_nonce` is the nonce this connection's own `Challenge` carried
/// (never a different connection's nonce — that is exactly the replay case
/// this function must reject via [`HandshakeRejectReason::StaleNonce`]).
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
) -> Result<(HandshakeAccept, ResolvedCredential), HandshakeRejectReason> {
    if hello_nonce != expected_nonce {
        return Err(HandshakeRejectReason::StaleNonce);
    }
    let credential = credential_resolver
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
        credential,
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
        // A replayed Hello carries the *correct* token (it was a real,
        // valid Hello once) but a stale nonce. This must surface as
        // StaleNonce, not AuthenticationFailed, so a replay is
        // diagnosable as exactly that.
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
        // Forward compatibility: a newer client may declare capabilities
        // this server build does not recognize. That must not be treated
        // as an error as long as the required baseline is still present.
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
    fn revoked_and_expired_credentials_fail_closed() {
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
    fn ambiguous_tokens_and_credential_ids_fail_closed() {
        let principal = AuthenticatedPrincipal::runtime_service("runtime-1");
        let token = AuthToken::new("duplicate-token");
        let first = CredentialRecord::active(token.clone(), principal.clone())
            .with_credential_id("duplicate-id");
        let second = CredentialRecord::active(token, principal).with_credential_id("duplicate-id");
        let resolver = CredentialSet::new(vec![first, second]);
        assert!(resolver.resolve("duplicate-token", 500).is_none());
        assert!(resolver.revalidate("duplicate-id", 500).is_none());
    }

    #[test]
    fn generated_credential_ids_are_distinct_from_principal_identity() {
        let principal = AuthenticatedPrincipal::runtime_service("runtime-1");
        let first = CredentialRecord::active(AuthToken::new("token-a"), principal.clone());
        let second = CredentialRecord::active(AuthToken::new("token-b"), principal);
        assert_ne!(first.credential_id, second.credential_id);
        assert_ne!(first.credential_id, "runtime-1");
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
}
