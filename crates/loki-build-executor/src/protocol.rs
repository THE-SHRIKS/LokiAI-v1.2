use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{BuildError, ErrorCode};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAX_INPUTS: usize = 128;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_NONCES: usize = 4096;
const MIN_KEY_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DigestId(String);

impl DigestId {
    pub fn parse(value: impl Into<String>) -> Result<Self, BuildError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(BuildError::new(
                ErrorCode::Malformed,
                "digest must be 64 lowercase hexadecimal characters",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<(), BuildError> {
        Self::parse(self.0.clone()).map(|_| ())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundedId(String);

impl BoundedId {
    pub fn parse(value: impl Into<String>) -> Result<Self, BuildError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_ID_BYTES
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
        {
            return Err(BuildError::new(
                ErrorCode::Malformed,
                "identifier is empty, oversized, or contains unsupported characters",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<(), BuildError> {
        Self::parse(self.0.clone()).map(|_| ())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputKind {
    SourceSnapshot,
    Toolchain,
    Lockfile,
    Executor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StagedInput {
    pub id: BoundedId,
    pub kind: InputKind,
    pub digest: DigestId,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceLimits {
    pub memory_bytes: u64,
    pub disk_bytes: u64,
    pub cpu_time_ms: u64,
    pub wall_time_ms: u64,
    pub process_count: u32,
    pub output_bytes: u64,
}

impl ResourceLimits {
    pub fn validate(&self) -> Result<(), BuildError> {
        let valid = (64 * 1024 * 1024..=64 * 1024 * 1024 * 1024).contains(&self.memory_bytes)
            && (128 * 1024 * 1024..=512 * 1024 * 1024 * 1024).contains(&self.disk_bytes)
            && (1_000..=86_400_000).contains(&self.cpu_time_ms)
            && (1_000..=86_400_000).contains(&self.wall_time_ms)
            && (1..=256).contains(&self.process_count)
            && (1..=16 * 1024 * 1024 * 1024).contains(&self.output_bytes)
            && self.output_bytes <= self.disk_bytes;
        valid.then_some(()).ok_or_else(|| {
            BuildError::new(
                ErrorCode::Malformed,
                "resource limits are outside the certified bounds",
            )
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrokerAction {
    Create,
    Attach,
    Mount,
    PrepareAcl,
    AuthorizeExecution,
    QuarantineOutput,
    Detach,
    Destroy,
    Reconcile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerRequest {
    pub job_id: BoundedId,
    pub capsule_id: BoundedId,
    pub sealed_base_digest: DigestId,
    pub expected_volume_id: Option<BoundedId>,
    pub appcontainer_sid_digest: DigestId,
    pub staged_inputs: Vec<StagedInput>,
    pub limits: ResourceLimits,
    pub action: BrokerAction,
}

impl BrokerRequest {
    pub fn validate(&self) -> Result<(), BuildError> {
        self.job_id.validate()?;
        self.capsule_id.validate()?;
        self.sealed_base_digest.validate()?;
        self.appcontainer_sid_digest.validate()?;
        if let Some(volume_id) = &self.expected_volume_id {
            volume_id.validate()?;
        }
        if self.staged_inputs.is_empty() || self.staged_inputs.len() > MAX_INPUTS {
            return Err(BuildError::new(
                ErrorCode::Oversized,
                "staged input count is outside the protocol bound",
            ));
        }
        let mut input_ids = HashSet::with_capacity(self.staged_inputs.len());
        self.limits.validate()?;
        for input in &self.staged_inputs {
            input.id.validate()?;
            input.digest.validate()?;
            if input.bytes == 0 {
                return Err(BuildError::new(
                    ErrorCode::Malformed,
                    "staged input size must be non-zero",
                ));
            }
            if !input_ids.insert(input.id.as_str()) {
                return Err(BuildError::new(
                    ErrorCode::IdentityMismatch,
                    "duplicate staged input identity rejected",
                ));
            }
        }
        let total = self
            .staged_inputs
            .iter()
            .try_fold(0_u64, |acc, input| acc.checked_add(input.bytes))
            .ok_or_else(|| {
                BuildError::new(ErrorCode::Oversized, "staged input byte total overflow")
            })?;
        if total > self.limits.disk_bytes {
            return Err(BuildError::new(
                ErrorCode::Oversized,
                "staged inputs exceed capsule disk limit",
            ));
        }
        if matches!(self.action, BrokerAction::Create) && self.expected_volume_id.is_some() {
            return Err(BuildError::new(
                ErrorCode::IdentityMismatch,
                "create must not predict or select a volume identity",
            ));
        }
        if !matches!(self.action, BrokerAction::Create) && self.expected_volume_id.is_none() {
            return Err(BuildError::new(
                ErrorCode::IdentityMismatch,
                "post-create action requires the expected volume identity",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedEnvelope {
    pub version: u16,
    pub nonce: BoundedId,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub request: BrokerRequest,
    pub mac: String,
}

#[derive(Debug)]
pub struct Authenticator {
    key: Vec<u8>,
    seen: Mutex<HashMap<String, u64>>,
}

impl Authenticator {
    pub fn new(key: &[u8]) -> Result<Self, BuildError> {
        if key.len() < MIN_KEY_BYTES {
            return Err(BuildError::new(
                ErrorCode::Authentication,
                "broker key must contain at least 256 bits",
            ));
        }
        Ok(Self {
            key: key.to_vec(),
            seen: Mutex::new(HashMap::new()),
        })
    }

    pub fn sign(
        &self,
        nonce: BoundedId,
        issued_at_ms: u64,
        expires_at_ms: u64,
        request: BrokerRequest,
    ) -> Result<SignedEnvelope, BuildError> {
        request.validate()?;
        let mut envelope = SignedEnvelope {
            version: PROTOCOL_VERSION,
            nonce,
            issued_at_ms,
            expires_at_ms,
            request,
            mac: String::new(),
        };
        envelope.mac = hex(&hmac_sha256(&self.key, &canonical_bytes(&envelope)?));
        Ok(envelope)
    }

    pub fn verify_and_consume(&self, raw: &[u8], now_ms: u64) -> Result<BrokerRequest, BuildError> {
        if raw.len() > MAX_MESSAGE_BYTES {
            return Err(BuildError::new(
                ErrorCode::Oversized,
                "broker message exceeds 64 KiB",
            ));
        }
        let envelope: SignedEnvelope = serde_json::from_slice(raw).map_err(|_| {
            BuildError::new(
                ErrorCode::Malformed,
                "broker message is not canonical protocol JSON",
            )
        })?;
        if envelope.version != PROTOCOL_VERSION {
            return Err(BuildError::new(
                ErrorCode::Malformed,
                "unsupported broker protocol version",
            ));
        }
        envelope.nonce.validate()?;
        if envelope.expires_at_ms < envelope.issued_at_ms
            || now_ms < envelope.issued_at_ms
            || now_ms > envelope.expires_at_ms
            || envelope.expires_at_ms - envelope.issued_at_ms > 60_000
        {
            return Err(BuildError::new(
                ErrorCode::Expired,
                "broker message is outside its bounded validity window",
            ));
        }
        envelope.request.validate()?;
        let supplied = decode_hex_32(&envelope.mac)?;
        let expected = hmac_sha256(&self.key, &canonical_bytes(&envelope)?);
        if !constant_time_eq(&supplied, &expected) {
            return Err(BuildError::new(
                ErrorCode::Authentication,
                "broker message authentication failed",
            ));
        }
        let nonce = envelope.nonce.as_str().to_owned();
        let mut seen = self.seen.lock().map_err(|_| {
            BuildError::new(ErrorCode::Authentication, "replay cache is unavailable")
        })?;
        seen.retain(|_, expires_at_ms| *expires_at_ms >= now_ms);
        if seen.contains_key(&nonce) {
            return Err(BuildError::new(
                ErrorCode::Replay,
                "broker nonce has already been consumed",
            ));
        }
        if seen.len() >= MAX_NONCES {
            return Err(BuildError::new(
                ErrorCode::Authentication,
                "broker replay cache is saturated; request refused fail closed",
            ));
        }
        seen.insert(nonce, envelope.expires_at_ms);
        Ok(envelope.request)
    }
}

fn canonical_bytes(envelope: &SignedEnvelope) -> Result<Vec<u8>, BuildError> {
    #[derive(Serialize)]
    struct Unsigned<'a> {
        version: u16,
        nonce: &'a BoundedId,
        issued_at_ms: u64,
        expires_at_ms: u64,
        request: &'a BrokerRequest,
    }
    serde_json::to_vec(&Unsigned {
        version: envelope.version,
        nonce: &envelope.nonce,
        issued_at_ms: envelope.issued_at_ms,
        expires_at_ms: envelope.expires_at_ms,
        request: &envelope.request,
    })
    .map_err(|_| {
        BuildError::new(
            ErrorCode::Malformed,
            "broker message could not be canonicalized",
        )
    })
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut block = [0_u8; 64];
    if key.len() > block.len() {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; 64];
    let mut outer_pad = [0x5c_u8; 64];
    for i in 0..64 {
        inner_pad[i] ^= block[i];
        outer_pad[i] ^= block[i];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(data);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_hash);
    outer.finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn decode_hex_32(value: &str) -> Result<[u8; 32], BuildError> {
    if value.len() != 64 {
        return Err(BuildError::new(
            ErrorCode::Authentication,
            "MAC has invalid length",
        ));
    }
    let mut out = [0_u8; 32];
    for (i, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        out[i] = u8::from_str_radix(
            std::str::from_utf8(chunk).map_err(|_| {
                BuildError::new(ErrorCode::Authentication, "MAC is not hexadecimal")
            })?,
            16,
        )
        .map_err(|_| BuildError::new(ErrorCode::Authentication, "MAC is not hexadecimal"))?;
    }
    Ok(out)
}
fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0_u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc_4231_case_one() {
        assert_eq!(
            hex(&hmac_sha256(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }
}
