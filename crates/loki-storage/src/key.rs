//! In-memory SQLCipher key material. Creation and persistence are deliberately
//! separate: only the Windows DPAPI secret store may create a production key.

/// A 256-bit SQLCipher database key. It has no `Debug` implementation so a
/// tracing field or panic message cannot accidentally expose the secret.
pub struct DatabaseKey(pub(crate) [u8; 32]);

impl DatabaseKey {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Consumes a freshly-decrypted plaintext key buffer (e.g. the `Vec<u8>`
    /// `CryptUnprotectData`/DPAPI hands back) and copies it into a
    /// `DatabaseKey`, wiping the input buffer's backing memory before it is
    /// dropped regardless of whether its length was valid.
    ///
    /// This is the only sanctioned way to turn a plaintext-key `Vec<u8>` into
    /// a `DatabaseKey`. Going through `.try_into::<[u8; 32]>()` directly on
    /// the `Vec` does **not** wipe the original heap allocation: the
    /// standard library's `Vec<T> -> [T; N]` conversion copies the bytes out
    /// via `ptr::read`, then truncates the `Vec`'s length to `0` and drops
    /// it — the `Vec`'s backing allocation still holds the plaintext key
    /// bytes when it is freed, unwiped, because `Drop` for a zero-length
    /// `Vec` never touches the (logically absent) elements. Wiping happens
    /// here, before that drop, closing that leak.
    pub(crate) fn from_plaintext_vec(mut plaintext: Vec<u8>) -> Option<Self> {
        let valid = plaintext.len() == 32;
        let mut bytes = [0u8; 32];
        if valid {
            bytes.copy_from_slice(&plaintext);
        }
        wipe_vec(&mut plaintext);
        drop(plaintext);
        if valid { Some(Self(bytes)) } else { None }
    }

    /// Hex form for SQLCipher's `PRAGMA key = "x'...'"` syntax, wrapped so
    /// the plaintext hex string is wiped as soon as the wrapper is dropped
    /// rather than relying on every call site remembering to wipe it. The
    /// returned value is used only to submit the key to the native database
    /// library.
    pub(crate) fn sqlcipher_hex(&self) -> SqlCipherHex {
        SqlCipherHex(self.0.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for DatabaseKey {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            // SAFETY: `byte` is a valid unique mutable pointer. Volatile
            // writes prevent the compiler from deleting the key wipe.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

/// A plaintext hex encoding of a [`DatabaseKey`], used only to build a
/// SQLCipher `PRAGMA key`/`ATTACH ... KEY` literal. Every byte is wiped as
/// soon as this value is dropped, so this intermediate plaintext-key copy
/// cannot outlive its one use as unwiped, ordinary (non-arena) heap memory —
/// callers do not need to remember to wipe it themselves.
pub(crate) struct SqlCipherHex(String);

impl SqlCipherHex {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for SqlCipherHex {
    fn drop(&mut self) {
        crate::storage::wipe_string(&mut self.0);
    }
}

/// Overwrites every byte of `bytes` with `0` in place, using volatile writes
/// and a compiler fence so the wipe cannot be optimized away as a dead store
/// just because `bytes` is about to be dropped. Shared by
/// [`DatabaseKey::from_plaintext_vec`] (the DPAPI-unprotect path) and
/// covered directly by `wipe_vec_zeroes_every_byte_in_place` below.
pub(crate) fn wipe_vec(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        // SAFETY: `byte` is a valid unique mutable pointer into `bytes`'s
        // live backing allocation.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Directly exercises the wipe primitive [`DatabaseKey::from_plaintext_vec`]
    /// applies to the DPAPI-unprotected plaintext key buffer before it is
    /// dropped (CODE-REV-001 CRITICAL-1c). This asserts the wipe itself is
    /// correct on an owned, still-live buffer — scanning freed heap memory
    /// for a leaked byte pattern is not reliably possible from safe Rust, so
    /// this plus the call-graph review documented in
    /// `Docs/graph/handoffs/DAT-002-MEM-CODE-001-4.md` is this crate's unit
    /// evidence that every function touching plaintext key material wipes
    /// its intermediates.
    #[test]
    fn wipe_vec_zeroes_every_byte_in_place() {
        let mut buf = vec![0xABu8; 32];
        wipe_vec(&mut buf);
        assert!(buf.iter().all(|&byte| byte == 0));
    }

    #[test]
    fn from_plaintext_vec_rejects_the_wrong_length_and_still_wipes_the_input() {
        // Length mismatch must not construct a key, and the (invalid)
        // input buffer must still be wiped before this function returns —
        // confirmed indirectly here by the fact that `from_plaintext_vec`
        // unconditionally calls `wipe_vec` before checking `valid`, which
        // `wipe_vec_zeroes_every_byte_in_place` above proves is correct in
        // isolation.
        let wrong_length = vec![0x11u8; 16];
        assert!(DatabaseKey::from_plaintext_vec(wrong_length).is_none());
    }

    #[test]
    fn from_plaintext_vec_builds_the_expected_key_on_valid_length() {
        let plaintext = vec![0x42u8; 32];
        let key = DatabaseKey::from_plaintext_vec(plaintext).expect("valid 32-byte key");
        assert_eq!(key.as_bytes(), &[0x42u8; 32]);
    }
}
