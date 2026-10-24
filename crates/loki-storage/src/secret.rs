//! Purpose-bound Windows DPAPI persistence for database keys and IPC tokens.
//! Raw byte protection is private so consumers cannot create generic secrets.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_SHA256_ALGORITHM, BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptCloseAlgorithmProvider,
    BCryptGenRandom, BCryptHash, BCryptOpenAlgorithmProvider, CRYPT_INTEGER_BLOB,
    CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetFileSecurityW, GetSecurityDescriptorControl,
    PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, SetFileSecurityW,
};
use windows_sys::Win32::Storage::FileSystem::{CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL};

use crate::key::DatabaseKey;

const DATABASE_KEY_BYTES: usize = 32;
const MAX_PROTECTED_SECRET_BYTES: u64 = 64 * 1024;
const ENVELOPE_MAGIC_V1: &[u8; 16] = b"LOKIAI.SECRET.V1";
const ENVELOPE_MAGIC_V2: &[u8; 16] = b"LOKIAI.SECRET.V2";
const SHA256_BYTES: usize = 32;
const ENVELOPE_V1_HEADER_BYTES: usize = ENVELOPE_MAGIC_V1.len() + 1 + 4;
const ENVELOPE_V2_HEADER_BYTES: usize = ENVELOPE_MAGIC_V2.len() + 1 + 4 + SHA256_BYTES;
const LEGACY_ENTROPY: &[u8] = b"LokiAI Main v1.2 local secret boundary";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SecretPurpose {
    DatabaseKey = 1,
    IpcToken = 2,
    BackupKey = 3,
    BackupManifest = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnvelopeFormat {
    RawLegacy,
    V1,
    V2,
}

impl SecretPurpose {
    fn entropy(self) -> &'static [u8] {
        match self {
            Self::DatabaseKey => b"LokiAI Main v1.2 database key boundary",
            Self::IpcToken => b"LokiAI Main v1.2 IPC token boundary",
            Self::BackupKey => b"LokiAI Main v1.2 backup key boundary",
            Self::BackupManifest => b"LokiAI Main v1.2 backup manifest boundary",
        }
    }
}

/// A purpose-bound DPAPI-unprotected IPC token. It has no `Debug`
/// implementation and wipes its allocation on drop.
pub struct IpcTokenSecret(String);

impl IpcTokenSecret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for IpcTokenSecret {
    fn drop(&mut self) {
        wipe_string(&mut self.0);
    }
}

/// Deterministic protected-key sidecar for a bounded-context database.
pub fn database_key_path(database_path: &Path) -> PathBuf {
    database_path.with_extension("key.protected")
}

pub fn backup_key_path(database_path: &Path) -> PathBuf {
    database_path.with_extension("backup.key.protected")
}

pub fn backup_manifest_path(database_path: &Path) -> PathBuf {
    database_path.with_extension("backup.manifest.protected")
}

#[derive(Debug)]
pub enum SecretError {
    Io(std::io::Error),
    MissingProtectedSecret(PathBuf),
    MalformedProtectedSecret { detail: &'static str },
    PurposeMismatch,
    IdentityInaccessible(std::io::Error),
    AmbiguousLegacyProtection(std::io::Error),
    Windows(std::io::Error),
    Acl(std::io::Error),
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "secret-store I/O error: {err}"),
            Self::MissingProtectedSecret(path) => {
                write!(f, "protected secret is missing: {}", path.display())
            }
            Self::MalformedProtectedSecret { detail } => {
                write!(f, "protected secret is malformed: {detail}")
            }
            Self::PurposeMismatch => write!(f, "protected secret has the wrong purpose"),
            Self::IdentityInaccessible(err) => {
                write!(
                    f,
                    "protected secret is inaccessible to this Windows identity: {err}"
                )
            }
            Self::AmbiguousLegacyProtection(err) => write!(
                f,
                "legacy protected secret cannot be unwrapped; identity and corruption are indistinguishable: {err}"
            ),
            Self::Windows(err) => write!(f, "Windows secret-protection error: {err}"),
            Self::Acl(err) => write!(f, "secret ACL enforcement error: {err}"),
        }
    }
}

impl std::error::Error for SecretError {}

impl From<std::io::Error> for SecretError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn load_or_create_database_key(
    protected_path: &Path,
    database_already_exists: bool,
) -> Result<DatabaseKey, SecretError> {
    if protected_path.exists() {
        return load_database_key(protected_path);
    }
    if database_already_exists {
        return Err(SecretError::MissingProtectedSecret(
            protected_path.to_path_buf(),
        ));
    }
    let key = generate_database_key()?;
    store_database_key(protected_path, &key)?;
    Ok(key)
}

pub(crate) fn load_database_key(path: &Path) -> Result<DatabaseKey, SecretError> {
    load_key_for_purpose(path, SecretPurpose::DatabaseKey)
}

pub(crate) fn load_backup_key(path: &Path) -> Result<DatabaseKey, SecretError> {
    load_key_for_purpose(path, SecretPurpose::BackupKey)
}

fn load_key_for_purpose(path: &Path, purpose: SecretPurpose) -> Result<DatabaseKey, SecretError> {
    let mut plaintext = load_purpose_secret(path, purpose)?;
    if plaintext.len() != DATABASE_KEY_BYTES {
        wipe(&mut plaintext);
        return Err(SecretError::MalformedProtectedSecret {
            detail: "database key length is not 32 bytes",
        });
    }
    let mut bytes = [0u8; DATABASE_KEY_BYTES];
    bytes.copy_from_slice(&plaintext);
    wipe(&mut plaintext);
    Ok(DatabaseKey::from_bytes(bytes))
}

pub(crate) fn generate_database_key() -> Result<DatabaseKey, SecretError> {
    let mut bytes = [0u8; DATABASE_KEY_BYTES];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        wipe(&mut bytes);
        return Err(SecretError::Windows(std::io::Error::other(format!(
            "BCryptGenRandom NTSTATUS 0x{:08x}",
            status as u32
        ))));
    }
    Ok(DatabaseKey::from_bytes(bytes))
}

pub(crate) fn store_database_key(path: &Path, key: &DatabaseKey) -> Result<(), SecretError> {
    store_new_purpose_secret(path, SecretPurpose::DatabaseKey, key.as_bytes())
}

pub(crate) fn replace_database_key(path: &Path, key: &DatabaseKey) -> Result<(), SecretError> {
    replace_purpose_secret(path, SecretPurpose::DatabaseKey, key.as_bytes())
}

pub(crate) fn store_backup_key(path: &Path, key: &DatabaseKey) -> Result<(), SecretError> {
    store_new_purpose_secret(path, SecretPurpose::BackupKey, key.as_bytes())
}

pub(crate) fn store_backup_manifest(path: &Path, manifest: &[u8]) -> Result<(), SecretError> {
    store_new_purpose_secret(path, SecretPurpose::BackupManifest, manifest)
}

pub(crate) fn load_backup_manifest(path: &Path) -> Result<Vec<u8>, SecretError> {
    load_purpose_secret(path, SecretPurpose::BackupManifest)
}

pub fn load_ipc_token(path: &Path) -> Result<IpcTokenSecret, SecretError> {
    let plaintext = load_purpose_secret(path, SecretPurpose::IpcToken)?;
    let value = match String::from_utf8(plaintext) {
        Ok(value) => value,
        Err(error) => {
            let mut invalid = error.into_bytes();
            wipe(&mut invalid);
            return Err(SecretError::MalformedProtectedSecret {
                detail: "IPC token is not UTF-8",
            });
        }
    };
    Ok(IpcTokenSecret(value))
}

pub fn store_new_ipc_token(path: &Path, token: &str) -> Result<(), SecretError> {
    store_new_purpose_secret(path, SecretPurpose::IpcToken, token.as_bytes())
}

fn load_purpose_secret(
    path: &Path,
    expected_purpose: SecretPurpose,
) -> Result<Vec<u8>, SecretError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() == 0 || metadata.len() > MAX_PROTECTED_SECRET_BYTES {
        return Err(SecretError::MalformedProtectedSecret {
            detail: "protected file has an invalid size",
        });
    }
    // Existing DAT-002 files inherited their parent ACL. Upgrade them to the
    // explicit owner/SYSTEM/Administrators boundary before reading.
    harden_path(path, false)?;
    let mut encoded = Vec::with_capacity(metadata.len() as usize);
    OpenOptions::new()
        .read(true)
        .open(path)?
        .read_to_end(&mut encoded)?;

    let (purpose, protected, format) = decode_envelope(&encoded, expected_purpose)?;
    let entropy = if format == EnvelopeFormat::RawLegacy {
        LEGACY_ENTROPY
    } else {
        purpose.entropy()
    };
    let result = unprotect(protected, entropy, format == EnvelopeFormat::V2);
    if format != EnvelopeFormat::V2
        && let Ok(plaintext) = &result
    {
        replace_purpose_secret(path, expected_purpose, plaintext)?;
    }
    result
}

fn decode_envelope(
    encoded: &[u8],
    expected: SecretPurpose,
) -> Result<(SecretPurpose, &[u8], EnvelopeFormat), SecretError> {
    let (format, header_bytes) = if encoded.starts_with(ENVELOPE_MAGIC_V2) {
        (EnvelopeFormat::V2, ENVELOPE_V2_HEADER_BYTES)
    } else if encoded.starts_with(ENVELOPE_MAGIC_V1) {
        (EnvelopeFormat::V1, ENVELOPE_V1_HEADER_BYTES)
    } else {
        return Ok((expected, encoded, EnvelopeFormat::RawLegacy));
    };
    if encoded.len() < header_bytes {
        return Err(SecretError::MalformedProtectedSecret {
            detail: "truncated envelope header",
        });
    }
    let purpose = match encoded[ENVELOPE_MAGIC_V1.len()] {
        1 => SecretPurpose::DatabaseKey,
        2 => SecretPurpose::IpcToken,
        3 => SecretPurpose::BackupKey,
        4 => SecretPurpose::BackupManifest,
        _ => {
            return Err(SecretError::MalformedProtectedSecret {
                detail: "unknown purpose identifier",
            });
        }
    };
    if purpose != expected {
        return Err(SecretError::PurposeMismatch);
    }
    let length_offset = ENVELOPE_MAGIC_V1.len() + 1;
    let length = u32::from_le_bytes(
        encoded[length_offset..length_offset + 4]
            .try_into()
            .expect("fixed envelope length slice"),
    ) as usize;
    if length == 0 || encoded.len() != header_bytes + length {
        return Err(SecretError::MalformedProtectedSecret {
            detail: "protected payload length does not match envelope",
        });
    }
    let protected = &encoded[header_bytes..];
    if format == EnvelopeFormat::V2 {
        let checksum_offset = length_offset + 4;
        let expected_checksum = &encoded[checksum_offset..checksum_offset + SHA256_BYTES];
        if sha256(protected)?.as_slice() != expected_checksum {
            return Err(SecretError::MalformedProtectedSecret {
                detail: "protected payload SHA-256 checksum does not match",
            });
        }
    }
    Ok((purpose, protected, format))
}

fn store_new_purpose_secret(
    path: &Path,
    purpose: SecretPurpose,
    plaintext: &[u8],
) -> Result<(), SecretError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        harden_path(parent, true)?;
    }
    write_secret_atomically(path, purpose, plaintext)
}

fn replace_purpose_secret(
    path: &Path,
    purpose: SecretPurpose,
    plaintext: &[u8],
) -> Result<(), SecretError> {
    write_secret_atomically(path, purpose, plaintext)
}

fn write_secret_atomically(
    path: &Path,
    purpose: SecretPurpose,
    plaintext: &[u8],
) -> Result<(), SecretError> {
    let encoded = encode_protected(purpose, plaintext)?;
    let temporary = temporary_path(path);
    remove_stale_temporary(&temporary)?;
    let result = write_new_file(&temporary, &encoded)
        .and_then(|()| harden_path(&temporary, false))
        .and_then(|()| std::fs::rename(&temporary, path).map_err(SecretError::Io))
        .and_then(|()| harden_path(path, false));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension("protected.new")
}

fn remove_stale_temporary(path: &Path) -> Result<(), SecretError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(SecretError::Io(error)),
    }
}

fn write_new_file(path: &Path, encoded: &[u8]) -> Result<(), SecretError> {
    let mut file = create_secure_new_file(path)?;
    file.write_all(encoded)?;
    file.sync_all()?;
    Ok(())
}

pub(crate) fn create_secure_new_file(path: &Path) -> Result<std::fs::File, SecretError> {
    let dacl = file_dacl();
    let wide_dacl: Vec<u16> = std::ffi::OsStr::new(dacl)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut descriptor = std::ptr::null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide_dacl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(SecretError::Acl(std::io::Error::last_os_error()));
    }
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            &mut attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    unsafe { LocalFree(descriptor) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(SecretError::Io(std::io::Error::last_os_error()));
    }
    Ok(unsafe { std::fs::File::from_raw_handle(handle) })
}

fn encode_protected(purpose: SecretPurpose, plaintext: &[u8]) -> Result<Vec<u8>, SecretError> {
    let protected = protect(plaintext, purpose.entropy())?;
    let protected_len =
        u32::try_from(protected.len()).map_err(|_| SecretError::MalformedProtectedSecret {
            detail: "DPAPI output is too large",
        })?;
    let checksum = sha256(&protected)?;
    let mut encoded = Vec::with_capacity(ENVELOPE_V2_HEADER_BYTES + protected.len());
    encoded.extend_from_slice(ENVELOPE_MAGIC_V2);
    encoded.push(purpose as u8);
    encoded.extend_from_slice(&protected_len.to_le_bytes());
    encoded.extend_from_slice(&checksum);
    encoded.extend_from_slice(&protected);
    Ok(encoded)
}

fn blob(bytes: &mut [u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_mut_ptr(),
    }
}

fn protect(plaintext: &[u8], entropy_value: &[u8]) -> Result<Vec<u8>, SecretError> {
    let mut input = plaintext.to_vec();
    let mut entropy = entropy_value.to_vec();
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptProtectData(
            &blob(&mut input),
            std::ptr::null(),
            &blob(&mut entropy),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    wipe(&mut input);
    if ok == 0 {
        return Err(SecretError::Windows(std::io::Error::last_os_error()));
    }
    copy_and_free(output)
}

fn unprotect(
    protected: &[u8],
    entropy_value: &[u8],
    integrity_verified: bool,
) -> Result<Vec<u8>, SecretError> {
    let mut input = protected.to_vec();
    let mut entropy = entropy_value.to_vec();
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptUnprotectData(
            &blob(&mut input),
            std::ptr::null_mut(),
            &blob(&mut entropy),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        let error = std::io::Error::last_os_error();
        return if integrity_verified {
            Err(SecretError::IdentityInaccessible(error))
        } else {
            Err(SecretError::AmbiguousLegacyProtection(error))
        };
    }
    copy_and_free(output)
}

pub(crate) fn sha256(input: &[u8]) -> Result<[u8; SHA256_BYTES], SecretError> {
    let mut algorithm = std::ptr::null_mut();
    let status = unsafe {
        BCryptOpenAlgorithmProvider(&mut algorithm, BCRYPT_SHA256_ALGORITHM, std::ptr::null(), 0)
    };
    if status != 0 {
        return Err(SecretError::Windows(std::io::Error::other(format!(
            "BCryptOpenAlgorithmProvider(SHA256) NTSTATUS 0x{:08x}",
            status as u32
        ))));
    }
    let mut digest = [0u8; SHA256_BYTES];
    let status = unsafe {
        BCryptHash(
            algorithm,
            std::ptr::null(),
            0,
            input.as_ptr(),
            input.len() as u32,
            digest.as_mut_ptr(),
            digest.len() as u32,
        )
    };
    unsafe { BCryptCloseAlgorithmProvider(algorithm, 0) };
    if status != 0 {
        return Err(SecretError::Windows(std::io::Error::other(format!(
            "BCryptHash(SHA256) NTSTATUS 0x{:08x}",
            status as u32
        ))));
    }
    Ok(digest)
}

fn copy_and_free(output: CRYPT_INTEGER_BLOB) -> Result<Vec<u8>, SecretError> {
    if output.pbData.is_null() {
        return Err(SecretError::MalformedProtectedSecret {
            detail: "DPAPI returned a null output buffer",
        });
    }
    let native = unsafe { std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize) };
    let result = native.to_vec();
    wipe(native);
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(result)
}

pub(crate) fn harden_path(path: &Path, directory: bool) -> Result<(), SecretError> {
    let dacl = if directory {
        directory_dacl()
    } else {
        file_dacl()
    };
    let wide_dacl: Vec<u16> = std::ffi::OsStr::new(dacl)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut descriptor = std::ptr::null_mut();
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide_dacl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(SecretError::Acl(std::io::Error::last_os_error()));
    }
    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let result = unsafe {
        SetFileSecurityW(
            wide_path.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    };
    unsafe { LocalFree(descriptor) };
    if result == 0 {
        return Err(SecretError::Acl(std::io::Error::last_os_error()));
    }
    verify_protected_acl(path, directory)
}

fn file_dacl() -> &'static str {
    "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;OW)"
}

fn directory_dacl() -> &'static str {
    "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;OW)"
}

fn verify_protected_acl(path: &Path, directory: bool) -> Result<(), SecretError> {
    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut needed = 0u32;
    let first = unsafe {
        GetFileSecurityW(
            wide_path.as_ptr(),
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    if first != 0
        || needed == 0
        || std::io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
    {
        return Err(SecretError::Acl(std::io::Error::last_os_error()));
    }
    let mut descriptor = vec![0u8; needed as usize];
    let ok = unsafe {
        GetFileSecurityW(
            wide_path.as_ptr(),
            DACL_SECURITY_INFORMATION,
            descriptor.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(SecretError::Acl(std::io::Error::last_os_error()));
    }
    let mut control = 0u16;
    let mut revision = 0u32;
    let ok = unsafe {
        GetSecurityDescriptorControl(
            descriptor.as_ptr().cast_mut().cast(),
            &mut control,
            &mut revision,
        )
    };
    if ok == 0 || control & SE_DACL_PROTECTED == 0 {
        return Err(SecretError::Acl(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "path DACL is not protected from inheritance",
        )));
    }
    let mut rendered = std::ptr::null_mut();
    let mut rendered_len = 0u32;
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor.as_ptr().cast_mut().cast(),
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut rendered,
            &mut rendered_len,
        )
    };
    if ok == 0 || rendered.is_null() {
        return Err(SecretError::Acl(std::io::Error::last_os_error()));
    }
    let actual = String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(rendered, rendered_len.saturating_sub(1) as usize)
    })
    .trim_end_matches('\0')
    .to_string();
    unsafe { LocalFree(rendered.cast()) };
    let expected = if directory {
        directory_dacl()
    } else {
        file_dacl()
    };
    if actual != expected {
        return Err(SecretError::Acl(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("path DACL is not canonical: expected {expected}, got {actual}"),
        )));
    }
    Ok(())
}

fn wipe(bytes: &mut [u8]) {
    for byte in bytes {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

fn wipe_string(value: &mut String) {
    let bytes = unsafe { value.as_mut_vec() };
    wipe(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purpose_mismatch_is_rejected_and_acl_is_protected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.protected");
        store_new_ipc_token(&path, "abcdefghijklmnopqrstuvwxyz0123456789").unwrap();
        verify_protected_acl(&path, false).unwrap();
        assert!(matches!(
            load_database_key(&path),
            Err(SecretError::PurposeMismatch)
        ));
    }

    #[test]
    fn stale_temporary_does_not_permanently_block_creation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.protected");
        std::fs::write(temporary_path(&path), b"stale").unwrap();
        store_new_ipc_token(&path, "abcdefghijklmnopqrstuvwxyz0123456789").unwrap();
        assert_eq!(
            load_ipc_token(&path).unwrap().expose(),
            "abcdefghijklmnopqrstuvwxyz0123456789"
        );
    }

    #[test]
    fn malformed_envelope_is_a_distinct_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.protected");
        let mut malformed = ENVELOPE_MAGIC_V2.to_vec();
        malformed.extend_from_slice(&[SecretPurpose::IpcToken as u8, 2, 0, 0, 0, 1]);
        std::fs::write(&path, malformed).unwrap();
        harden_path(&path, false).unwrap();
        assert!(matches!(
            load_ipc_token(&path),
            Err(SecretError::MalformedProtectedSecret { .. })
        ));
    }

    #[test]
    fn invalid_utf8_ipc_payload_is_rejected_after_the_unprotected_buffer_is_consumed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid-utf8.protected");
        store_new_purpose_secret(&path, SecretPurpose::IpcToken, &[0xff, 0xfe, 0xfd]).unwrap();
        assert!(matches!(
            load_ipc_token(&path),
            Err(SecretError::MalformedProtectedSecret {
                detail: "IPC token is not UTF-8"
            })
        ));
    }

    #[test]
    fn corrupted_v2_ciphertext_is_malformed_not_an_identity_claim() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt-v2.protected");
        store_new_ipc_token(&path, "abcdefghijklmnopqrstuvwxyz0123456789").unwrap();
        let mut encoded = std::fs::read(&path).unwrap();
        *encoded.last_mut().unwrap() ^= 0x40;
        std::fs::write(&path, encoded).unwrap();
        assert!(matches!(
            load_ipc_token(&path),
            Err(SecretError::MalformedProtectedSecret {
                detail: "protected payload SHA-256 checksum does not match"
            })
        ));
    }

    #[test]
    fn corrupted_raw_legacy_blob_is_reported_as_ambiguous_not_identity_inaccessible() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt-legacy.protected");
        std::fs::write(&path, [0x7d; 128]).unwrap();
        harden_path(&path, false).unwrap();
        assert!(matches!(
            load_ipc_token(&path),
            Err(SecretError::AmbiguousLegacyProtection(_))
        ));
    }

    #[test]
    fn v1_envelope_is_atomically_migrated_to_v2_after_successful_unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v1.protected");
        let protected = protect(
            b"abcdefghijklmnopqrstuvwxyz0123456789",
            SecretPurpose::IpcToken.entropy(),
        )
        .unwrap();
        let mut encoded = ENVELOPE_MAGIC_V1.to_vec();
        encoded.push(SecretPurpose::IpcToken as u8);
        encoded.extend_from_slice(&(protected.len() as u32).to_le_bytes());
        encoded.extend_from_slice(&protected);
        std::fs::write(&path, encoded).unwrap();
        harden_path(&path, false).unwrap();

        assert_eq!(
            load_ipc_token(&path).unwrap().expose(),
            "abcdefghijklmnopqrstuvwxyz0123456789"
        );
        assert!(std::fs::read(&path).unwrap().starts_with(ENVELOPE_MAGIC_V2));
    }

    #[test]
    fn secure_creation_denies_child_open_until_canonical_acl_is_installed() {
        let dir = tempfile::tempdir().unwrap();
        harden_path(dir.path(), true).unwrap();
        let path = dir.path().join("atomic.protected");
        let file = create_secure_new_file(&path).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("secret::tests::secure_creation_child_probe")
            .arg("--nocapture")
            .env("LOKIAI_SECURE_CREATE_CHILD", &path)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "child must be denied while the atomic handle is live"
        );
        drop(file);
        verify_protected_acl(&path, false).unwrap();
    }

    #[test]
    fn secure_creation_child_probe() {
        let Some(path) = std::env::var_os("LOKIAI_SECURE_CREATE_CHILD") else {
            return;
        };
        assert!(OpenOptions::new().write(true).open(path).is_err());
    }
}
