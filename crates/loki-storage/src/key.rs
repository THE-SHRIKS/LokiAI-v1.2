//! In-memory SQLCipher key material. Creation and persistence are deliberately
//! separate: only the Windows DPAPI secret store may create a production key.

/// A 256-bit SQLCipher database key. It has no `Debug` implementation so a
/// tracing field or panic message cannot accidentally expose the secret.
pub struct DatabaseKey(pub(crate) [u8; 32]);

impl DatabaseKey {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    /// Hex form for SQLCipher's `PRAGMA key = "x'...'"` syntax. The returned
    /// value is used only to submit the key to the native database library.
    pub(crate) fn sqlcipher_hex(&self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
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
