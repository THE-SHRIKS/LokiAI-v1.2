//! DPAPI-protected persistence for the local IPC authentication token.

use std::io;
use std::path::Path;
use uuid::Uuid;

const MIN_TOKEN_LEN: usize = 32;

/// Loads the protected token corresponding to the legacy token path. Kept as
/// the stable host-test/consumer seam while storage has moved to DPAPI.
pub fn load_or_create(legacy_plaintext_path: &Path) -> io::Result<String> {
    let protected_path = legacy_plaintext_path.with_extension("token.protected");
    load_or_create_with_legacy(&protected_path, legacy_plaintext_path)
}

pub fn load_or_create_with_legacy(
    protected_path: &Path,
    legacy_plaintext_path: &Path,
) -> io::Result<String> {
    if protected_path.exists() {
        return decode(loki_storage::secret::load_secret(protected_path).map_err(secret_io)?);
    }
    let token = match std::fs::read_to_string(legacy_plaintext_path) {
        Ok(value) if value.trim().len() >= MIN_TOKEN_LEN => value.trim().to_owned(),
        Ok(_) => generate(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => generate(),
        Err(err) => return Err(err),
    };
    loki_storage::secret::store_new_secret(protected_path, token.as_bytes()).map_err(secret_io)?;
    if legacy_plaintext_path.exists() {
        std::fs::remove_file(legacy_plaintext_path)?;
    }
    Ok(token)
}

fn decode(bytes: Vec<u8>) -> io::Result<String> {
    let value = String::from_utf8(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "protected IPC token is not UTF-8",
        )
    })?;
    if value.len() < MIN_TOKEN_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "protected IPC token is truncated",
        ));
    }
    Ok(value)
}

fn secret_io(error: loki_storage::secret::SecretError) -> io::Error {
    io::Error::other(error)
}
fn generate() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_reuses_only_a_protected_token() {
        let dir = tempfile::tempdir().unwrap();
        let protected = dir.path().join("ipc.token.protected");
        let legacy = dir.path().join("ipc.token");
        let first = load_or_create_with_legacy(&protected, &legacy).unwrap();
        let second = load_or_create_with_legacy(&protected, &legacy).unwrap();
        assert_eq!(first, second);
        assert!(!legacy.exists());
        assert!(
            !std::fs::read(&protected)
                .unwrap()
                .windows(first.len())
                .any(|w| w == first.as_bytes())
        );
    }

    #[test]
    fn migrates_and_removes_a_legacy_plaintext_token() {
        let dir = tempfile::tempdir().unwrap();
        let protected = dir.path().join("ipc.token.protected");
        let legacy = dir.path().join("ipc.token");
        let old = generate();
        std::fs::write(&legacy, &old).unwrap();
        assert_eq!(
            load_or_create_with_legacy(&protected, &legacy).unwrap(),
            old
        );
        assert!(!legacy.exists());
        assert!(protected.exists());
    }
}
