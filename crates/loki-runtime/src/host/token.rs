//! DPAPI-protected persistence for the local IPC authentication token.

use std::io;
use std::path::Path;
use uuid::Uuid;

const MIN_TOKEN_LEN: usize = 32;
const LEGACY_OVERWRITE_CHUNK_BYTES: usize = 8192;

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
        let protected = loki_storage::secret::load_ipc_token(protected_path).map_err(secret_io)?;
        let token = validate(protected.expose())?;
        remove_legacy_plaintext(legacy_plaintext_path)?;
        return Ok(token);
    }
    let token = match std::fs::read_to_string(legacy_plaintext_path) {
        Ok(value) if value.trim().len() >= MIN_TOKEN_LEN => value.trim().to_owned(),
        Ok(_) => generate(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => generate(),
        Err(err) => return Err(err),
    };
    loki_storage::secret::store_new_ipc_token(protected_path, &token).map_err(secret_io)?;
    remove_legacy_plaintext(legacy_plaintext_path)?;
    Ok(token)
}

fn validate(value: &str) -> io::Result<String> {
    if value.len() < MIN_TOKEN_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "protected IPC token is truncated",
        ));
    }
    Ok(value.to_owned())
}

fn remove_legacy_plaintext(path: &Path) -> io::Result<()> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    overwrite_with_zeroes(&mut file, metadata.len())?;
    file.set_len(0)?;
    file.sync_all()?;
    drop(file);
    std::fs::remove_file(path)
}

fn overwrite_with_zeroes(writer: &mut impl std::io::Write, mut remaining: u64) -> io::Result<()> {
    let zeroes = [0u8; LEGACY_OVERWRITE_CHUNK_BYTES];
    while remaining > 0 {
        let count = usize::try_from(remaining.min(LEGACY_OVERWRITE_CHUNK_BYTES as u64))
            .expect("bounded overwrite chunk fits usize");
        writer.write_all(&zeroes[..count])?;
        remaining -= count as u64;
    }
    Ok(())
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

    #[derive(Default)]
    struct BoundedWriter {
        total: u64,
        largest_write: usize,
    }

    impl std::io::Write for BoundedWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            assert!(buffer.iter().all(|byte| *byte == 0));
            self.total += buffer.len() as u64;
            self.largest_write = self.largest_write.max(buffer.len());
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

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

    #[test]
    fn existing_protected_token_still_removes_late_legacy_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let protected = dir.path().join("ipc.token.protected");
        let legacy = dir.path().join("ipc.token");
        let expected = load_or_create_with_legacy(&protected, &legacy).unwrap();
        std::fs::write(&legacy, "late plaintext token that must not survive").unwrap();
        assert_eq!(
            load_or_create_with_legacy(&protected, &legacy).unwrap(),
            expected
        );
        assert!(!legacy.exists());
    }

    #[test]
    fn legacy_overwrite_is_zero_filled_and_bounded_to_fixed_chunks() {
        let mut writer = BoundedWriter::default();
        let simulated_large_file = (LEGACY_OVERWRITE_CHUNK_BYTES as u64 * 3) + 17;
        overwrite_with_zeroes(&mut writer, simulated_large_file).unwrap();
        assert_eq!(writer.total, simulated_large_file);
        assert_eq!(writer.largest_write, LEGACY_OVERWRITE_CHUNK_BYTES);
    }
}
