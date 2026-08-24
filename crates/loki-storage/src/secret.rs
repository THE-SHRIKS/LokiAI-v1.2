//! Windows DPAPI-backed secret persistence for database keys and local IPC
//! credentials. Protected blobs are opaque and bound to the current Windows
//! identity; plaintext is never written to disk.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom, CRYPT_INTEGER_BLOB,
    CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

use crate::key::DatabaseKey;

const DATABASE_KEY_BYTES: usize = 32;
const ENTROPY: &[u8] = b"LokiAI Main v1.2 local secret boundary";

/// Deterministic protected-key sidecar for a bounded-context database.
/// The sidecar is a DPAPI blob, never plaintext key material.
pub fn database_key_path(database_path: &Path) -> PathBuf {
    database_path.with_extension("key.protected")
}

pub fn load_or_create_key_for_database(database_path: &Path) -> Result<DatabaseKey, SecretError> {
    load_or_create_database_key(&database_key_path(database_path), database_path.exists())
}

#[derive(Debug)]
pub enum SecretError {
    Io(std::io::Error),
    MissingProtectedSecret(PathBuf),
    InvalidProtectedSecret,
    Windows(std::io::Error),
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "secret-store I/O error: {err}"),
            Self::MissingProtectedSecret(path) => {
                write!(f, "protected secret is missing: {}", path.display())
            }
            Self::InvalidProtectedSecret => {
                write!(f, "protected secret is invalid or has an unexpected length")
            }
            Self::Windows(err) => write!(f, "Windows secret-protection error: {err}"),
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
        let plaintext = load_secret(protected_path)?;
        let bytes: [u8; DATABASE_KEY_BYTES] = plaintext
            .try_into()
            .map_err(|_| SecretError::InvalidProtectedSecret)?;
        return Ok(DatabaseKey::from_bytes(bytes));
    }
    if database_already_exists {
        return Err(SecretError::MissingProtectedSecret(
            protected_path.to_path_buf(),
        ));
    }
    let key = generate_database_key()?;
    store_new_secret(protected_path, key.as_bytes())?;
    Ok(key)
}

pub fn generate_database_key() -> Result<DatabaseKey, SecretError> {
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
        return Err(SecretError::Windows(std::io::Error::from_raw_os_error(
            status,
        )));
    }
    Ok(DatabaseKey::from_bytes(bytes))
}

pub fn store_database_key(path: &Path, key: &DatabaseKey) -> Result<(), SecretError> {
    store_new_secret(path, key.as_bytes())
}

pub fn load_secret(path: &Path) -> Result<Vec<u8>, SecretError> {
    let mut protected = Vec::new();
    OpenOptions::new()
        .read(true)
        .open(path)?
        .read_to_end(&mut protected)?;
    if protected.is_empty() {
        return Err(SecretError::InvalidProtectedSecret);
    }
    unprotect(&mut protected)
}

pub fn store_new_secret(path: &Path, plaintext: &[u8]) -> Result<(), SecretError> {
    let protected = protect(plaintext)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("protected.new");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&protected)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary, path)?;
    Ok(())
}

fn blob(bytes: &mut [u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_mut_ptr(),
    }
}

fn protect(plaintext: &[u8]) -> Result<Vec<u8>, SecretError> {
    let mut input = plaintext.to_vec();
    let mut entropy = ENTROPY.to_vec();
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

fn unprotect(protected: &mut [u8]) -> Result<Vec<u8>, SecretError> {
    let mut entropy = ENTROPY.to_vec();
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptUnprotectData(
            &blob(protected),
            std::ptr::null_mut(),
            &blob(&mut entropy),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(SecretError::Windows(std::io::Error::last_os_error()));
    }
    copy_and_free(output)
}

fn copy_and_free(output: CRYPT_INTEGER_BLOB) -> Result<Vec<u8>, SecretError> {
    if output.pbData.is_null() {
        return Err(SecretError::InvalidProtectedSecret);
    }
    let native = unsafe { std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize) };
    let result = native.to_vec();
    wipe(native);
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(result)
}

fn wipe(bytes: &mut [u8]) {
    for byte in bytes {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}
