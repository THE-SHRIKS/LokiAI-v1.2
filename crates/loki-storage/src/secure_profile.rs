//! Immutable bounds profile for the process-global SQLCipher secure arena.
//!
//! A profile is operational only when its complete manifest digest is
//! compiled into this binary after human acceptance.  This node deliberately
//! embeds no accepted digest: real supported-host measurements belong to
//! `DAT-002-MEM-HOST-VERIFY-001`, so production startup fails closed until
//! that later node supplies one.

use std::fmt;

const PROFILE_VERSION: &str = "1";
const MAX_MANIFEST_BYTES: usize = 4_096;
const REQUIRED_KEYS: [&str; 10] = [
    "version",
    "arena_bytes",
    "working_set_min_bytes",
    "working_set_max_bytes",
    "connection_cap",
    "cache_pages_per_connection",
    "source_sha256",
    "build_sha256",
    "accepted_by",
    "accepted_at_utc",
];

// No production bounds are accepted yet.  A later, separately reviewed node
// must replace both values atomically with the exact accepted manifest and its
// SHA-256 digest.  Keeping this as `None` is the fail-closed production state.
const ACCEPTED_PROFILE: Option<(&[u8], [u8; 32])> = None;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    Unavailable,
    Oversized,
    InvalidUtf8,
    InvalidShape(&'static str),
    InvalidValue(&'static str),
    DigestMismatch,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => write!(
                f,
                "no digest-bound human-accepted bounds profile is embedded"
            ),
            Self::Oversized => write!(f, "secure-memory profile exceeds the bounded manifest size"),
            Self::InvalidUtf8 => write!(f, "secure-memory profile is not valid UTF-8"),
            Self::InvalidShape(reason) => {
                write!(f, "invalid secure-memory profile shape: {reason}")
            }
            Self::InvalidValue(field) => {
                write!(f, "invalid secure-memory profile value for {field}")
            }
            Self::DigestMismatch => write!(
                f,
                "secure-memory profile digest does not match its accepted digest"
            ),
        }
    }
}

impl std::error::Error for ProfileError {}

/// Parsed, structurally validated profile information.  Inspection does not
/// confer operational acceptance; only the private embedded digest can do so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileInspection {
    pub arena_bytes: usize,
    pub working_set_min_bytes: usize,
    pub working_set_max_bytes: usize,
    pub connection_cap: u32,
    pub cache_pages_per_connection: u32,
    pub source_sha256: [u8; 32],
    pub build_sha256: [u8; 32],
    pub manifest_sha256: [u8; 32],
}

#[derive(Debug, Clone)]
pub(crate) struct AcceptedProfile {
    pub(crate) inspection: ProfileInspection,
}

/// Validate a candidate manifest against an expected digest for review and
/// tooling.  The returned value is intentionally not an authorization token.
pub fn inspect_profile(
    manifest: &[u8],
    expected_manifest_sha256: [u8; 32],
) -> Result<ProfileInspection, ProfileError> {
    if manifest.len() > MAX_MANIFEST_BYTES {
        return Err(ProfileError::Oversized);
    }
    let actual_digest = sha256(manifest);
    if !constant_time_eq(&actual_digest, &expected_manifest_sha256) {
        return Err(ProfileError::DigestMismatch);
    }

    let text = std::str::from_utf8(manifest).map_err(|_| ProfileError::InvalidUtf8)?;
    let mut values: [Option<&str>; REQUIRED_KEYS.len()] = [None; REQUIRED_KEYS.len()];
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or(ProfileError::InvalidShape("each line must be key=value"))?;
        let index = REQUIRED_KEYS
            .iter()
            .position(|candidate| *candidate == key)
            .ok_or(ProfileError::InvalidShape("unknown key"))?;
        if values[index].replace(value).is_some() {
            return Err(ProfileError::InvalidShape("duplicate key"));
        }
    }
    if values.iter().any(Option::is_none) {
        return Err(ProfileError::InvalidShape("missing required key"));
    }

    let get = |name: &'static str| -> Result<&str, ProfileError> {
        let index = REQUIRED_KEYS
            .iter()
            .position(|candidate| *candidate == name)
            .expect("required profile key is defined");
        values[index].ok_or(ProfileError::InvalidShape("missing required key"))
    };
    if get("version")? != PROFILE_VERSION {
        return Err(ProfileError::InvalidValue("version"));
    }

    let arena_bytes = parse_usize(get("arena_bytes")?, "arena_bytes")?;
    let working_set_min_bytes =
        parse_usize(get("working_set_min_bytes")?, "working_set_min_bytes")?;
    let working_set_max_bytes =
        parse_usize(get("working_set_max_bytes")?, "working_set_max_bytes")?;
    let connection_cap = parse_u32(get("connection_cap")?, "connection_cap")?;
    let cache_pages_per_connection = parse_u32(
        get("cache_pages_per_connection")?,
        "cache_pages_per_connection",
    )?;

    // These are structural invariants, not guessed production bounds.  Exact
    // values remain entirely supplied by the measured accepted profile.
    if arena_bytes == 0 || !arena_bytes.is_multiple_of(4_096) {
        return Err(ProfileError::InvalidValue("arena_bytes"));
    }
    if working_set_min_bytes == 0
        || working_set_max_bytes < working_set_min_bytes
        || working_set_min_bytes < arena_bytes
        || connection_cap == 0
        || cache_pages_per_connection == 0
    {
        return Err(ProfileError::InvalidValue("bounds"));
    }

    let accepted_by = get("accepted_by")?;
    if accepted_by.is_empty() || accepted_by.len() > 128 || !accepted_by.is_ascii() {
        return Err(ProfileError::InvalidValue("accepted_by"));
    }
    let accepted_at = get("accepted_at_utc")?;
    if accepted_at.len() < 20
        || accepted_at.len() > 64
        || !accepted_at.ends_with('Z')
        || !accepted_at.is_ascii()
    {
        return Err(ProfileError::InvalidValue("accepted_at_utc"));
    }

    Ok(ProfileInspection {
        arena_bytes,
        working_set_min_bytes,
        working_set_max_bytes,
        connection_cap,
        cache_pages_per_connection,
        source_sha256: parse_hex_digest(get("source_sha256")?, "source_sha256")?,
        build_sha256: parse_hex_digest(get("build_sha256")?, "build_sha256")?,
        manifest_sha256: actual_digest,
    })
}

/// Computes the review digest for a candidate manifest.  This does not make
/// the manifest operational; bootstrap still requires the separately compiled
/// accepted digest.
pub fn digest_profile_manifest(manifest: &[u8]) -> [u8; 32] {
    sha256(manifest)
}

pub(crate) fn accepted_profile() -> Result<AcceptedProfile, ProfileError> {
    let (manifest, digest) = ACCEPTED_PROFILE.ok_or(ProfileError::Unavailable)?;
    inspect_profile(manifest, digest).map(|inspection| AcceptedProfile { inspection })
}

fn parse_usize(value: &str, field: &'static str) -> Result<usize, ProfileError> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(ProfileError::InvalidValue(field))
}

fn parse_u32(value: &str, field: &'static str) -> Result<u32, ProfileError> {
    value
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(ProfileError::InvalidValue(field))
}

fn parse_hex_digest(value: &str, field: &'static str) -> Result<[u8; 32], ProfileError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ProfileError::InvalidValue(field));
    }
    let mut result = [0_u8; 32];
    for (index, output) in result.iter_mut().enumerate() {
        let start = index * 2;
        *output = u8::from_str_radix(&value[start..start + 2], 16)
            .map_err(|_| ProfileError::InvalidValue(field))?;
    }
    Ok(result)
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

// Small self-contained SHA-256 avoids adding a dependency and changing the
// orchestrator-owned workspace lockfile for this security-critical primitive.
pub(crate) fn sha256(input: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
        let mut words = [0_u32; 64];
        for (index, chunk) in block.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes(chunk.try_into().expect("four-byte SHA word"));
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
        for index in 0..64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sum0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut state = INITIAL;
    let mut chunks = input.chunks_exact(64);
    for chunk in &mut chunks {
        compress(&mut state, chunk.try_into().expect("64-byte SHA block"));
    }
    let remainder = chunks.remainder();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut final_blocks = [0_u8; 128];
    final_blocks[..remainder.len()].copy_from_slice(remainder);
    final_blocks[remainder.len()] = 0x80;
    let used = if remainder.len() < 56 { 64 } else { 128 };
    final_blocks[used - 8..used].copy_from_slice(&bit_len.to_be_bytes());
    for block in final_blocks[..used].chunks_exact(64) {
        compress(
            &mut state,
            block.try_into().expect("64-byte SHA final block"),
        );
    }
    let mut output = [0_u8; 32];
    for (chunk, word) in output.chunks_exact_mut(4).zip(state) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &[u8] = b"version=1\narena_bytes=4096\nworking_set_min_bytes=8192\nworking_set_max_bytes=16384\nconnection_cap=1\ncache_pages_per_connection=1\nsource_sha256=0000000000000000000000000000000000000000000000000000000000000000\nbuild_sha256=1111111111111111111111111111111111111111111111111111111111111111\naccepted_by=test-reviewer\naccepted_at_utc=2026-08-24T00:00:00Z\n";

    #[test]
    fn sha256_matches_standard_vectors() {
        assert_eq!(
            sha256(b"abc"),
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ]
        );
    }

    #[test]
    fn exact_digest_and_shape_are_required() {
        let digest = sha256(MANIFEST);
        assert!(inspect_profile(MANIFEST, digest).is_ok());
        let mut tampered = MANIFEST.to_vec();
        tampered[0] ^= 1;
        assert_eq!(
            inspect_profile(&tampered, digest),
            Err(ProfileError::DigestMismatch)
        );
        assert_eq!(
            inspect_profile(
                &vec![b'x'; MAX_MANIFEST_BYTES + 1],
                sha256(&vec![b'x'; MAX_MANIFEST_BYTES + 1])
            ),
            Err(ProfileError::Oversized)
        );
    }

    #[test]
    fn production_profile_is_intentionally_unavailable() {
        assert!(matches!(accepted_profile(), Err(ProfileError::Unavailable)));
    }
}
