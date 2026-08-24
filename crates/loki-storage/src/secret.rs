//! Purpose-bound Windows DPAPI persistence for database keys and IPC tokens.
//! Raw byte protection is private so consumers cannot create generic secrets.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom, CRYPT_INTEGER_BLOB,
    CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetFileSecurityW, GetSecurityDescriptorControl,
    PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED, SetFileSecurityW,
};

use crate::key::DatabaseKey;

const DATABASE_KEY_BYTES: usize = 32;
const MAX_PROTECTED_SECRET_BYTES: u64 = 64 * 1024;
const ENVELOPE_MAGIC: &[u8; 16] = b"LOKIAI.SECRET.V1";
const ENVELOPE_HEADER_BYTES: usize = ENVELOPE_MAGIC.len() + 1 + 4;
const LEGACY_ENTROPY: &[u8] = b"LokiAI Main v1.2 local secret boundary";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SecretPurpose {
    DatabaseKey = 1,
    IpcToken = 2,
}

impl SecretPurpose {
    fn entropy(self) -> &'static [u8] {
        match self {
            Self::DatabaseKey => b"LokiAI Main v1.2 database key boundary",
            Self::IpcToken => b"LokiAI Main v1.2 IPC token boundary",
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

#[derive(Debug)]
pub enum SecretError {
    Io(std::io::Error),
    MissingProtectedSecret(PathBuf),
    MalformedProtectedSecret { detail: &'static str },
    PurposeMismatch,
    IdentityInaccessible(std::io::Error),
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
    let mut plaintext = load_purpose_secret(path, SecretPurpose::DatabaseKey)?;
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

    let (purpose, protected, legacy) = decode_envelope(&encoded, expected_purpose)?;
    let entropy = if legacy {
        LEGACY_ENTROPY
    } else {
        purpose.entropy()
    };
    let result = unprotect(protected, entropy);
    if legacy && let Ok(plaintext) = &result {
        replace_purpose_secret(path, expected_purpose, plaintext)?;
    }
    result
}

fn decode_envelope(
    encoded: &[u8],
    expected: SecretPurpose,
) -> Result<(SecretPurpose, &[u8], bool), SecretError> {
    if !encoded.starts_with(ENVELOPE_MAGIC) {
        return Ok((expected, encoded, true));
    }
    if encoded.len() < ENVELOPE_HEADER_BYTES {
        return Err(SecretError::MalformedProtectedSecret {
            detail: "truncated envelope header",
        });
    }
    let purpose = match encoded[ENVELOPE_MAGIC.len()] {
        1 => SecretPurpose::DatabaseKey,
        2 => SecretPurpose::IpcToken,
        _ => {
            return Err(SecretError::MalformedProtectedSecret {
                detail: "unknown purpose identifier",
            });
        }
    };
    if purpose != expected {
        return Err(SecretError::PurposeMismatch);
    }
    let length_offset = ENVELOPE_MAGIC.len() + 1;
    let length = u32::from_le_bytes(
        encoded[length_offset..length_offset + 4]
            .try_into()
            .expect("fixed envelope length slice"),
    ) as usize;
    if length == 0 || encoded.len() != ENVELOPE_HEADER_BYTES + length {
        return Err(SecretError::MalformedProtectedSecret {
            detail: "protected payload length does not match envelope",
        });
    }
    Ok((purpose, &encoded[ENVELOPE_HEADER_BYTES..], false))
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
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(path)?;
    file.write_all(encoded)?;
    file.sync_all()?;
    Ok(())
}

fn encode_protected(purpose: SecretPurpose, plaintext: &[u8]) -> Result<Vec<u8>, SecretError> {
    let protected = protect(plaintext, purpose.entropy())?;
    let protected_len =
        u32::try_from(protected.len()).map_err(|_| SecretError::MalformedProtectedSecret {
            detail: "DPAPI output is too large",
        })?;
    let mut encoded = Vec::with_capacity(ENVELOPE_HEADER_BYTES + protected.len());
    encoded.extend_from_slice(ENVELOPE_MAGIC);
    encoded.push(purpose as u8);
    encoded.extend_from_slice(&protected_len.to_le_bytes());
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

fn unprotect(protected: &[u8], entropy_value: &[u8]) -> Result<Vec<u8>, SecretError> {
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
        return Err(SecretError::IdentityInaccessible(
            std::io::Error::last_os_error(),
        ));
    }
    copy_and_free(output)
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
        "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;OW)"
    } else {
        "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;OW)"
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
    verify_protected_acl(path)
}

fn verify_protected_acl(path: &Path) -> Result<(), SecretError> {
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
        verify_protected_acl(&path).unwrap();
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
        let mut malformed = ENVELOPE_MAGIC.to_vec();
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
}
