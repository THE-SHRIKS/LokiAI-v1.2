//! Windows named-pipe security-descriptor construction.
//!
//! Restricts a named pipe's DACL so only a chosen Windows SID can open a
//! handle to it — the OS-enforced half of connection-level authentication
//! (see `Docs/SECURITY.md` §4's "Local client impersonation" threat row and
//! this crate's top-level docs). By default, [`AclScope::CurrentUser`]
//! resolves and uses the *current process's* user SID, matching the task
//! packet's "restricting to the current Windows user session".
//!
//! [`AclScope::OnlySid`] exists to make the enforcement itself testable
//! without provisioning a second real Windows user account (see
//! `tests/acl.rs`): binding a pipe with `OnlySid("<some other real SID>")`
//! and then trying to connect *as the current user* must be denied. If that
//! negative test fails, the ACL wiring is broken — it does not just skip
//! silently.
//!
//! All FFI calls in this module are documented individually with the
//! safety invariant they rely on. No public function here can be called in
//! a way that causes memory unsafety from safe-Rust-only misuse; the
//! `unsafe` is entirely about correct Win32 API usage (buffer sizing,
//! ownership/freeing), not about upholding invariants the caller must
//! independently guarantee.

use std::ffi::{OsStr, c_void};
use std::os::windows::ffi::OsStrExt;
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Which principal a bound pipe's DACL grants access to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AclScope {
    /// Production default: only the current process's Windows user SID.
    CurrentUser,
    /// Advanced/test-only: only the given SID string (e.g.
    /// `"S-1-5-21-...-1001"` or a well-known SID such as `"S-1-0-0"`, the
    /// Null SID). Exists so the enforcement mechanism itself can be tested
    /// deterministically without a second real logged-on Windows account —
    /// see this module's top doc comment.
    OnlySid(String),
}

impl AclScope {
    fn resolve_sid(&self) -> std::io::Result<String> {
        match self {
            AclScope::CurrentUser => current_user_sid_string(),
            AclScope::OnlySid(sid) => Ok(sid.clone()),
        }
    }

    /// Builds the SDDL string for this scope: a protected DACL (`P` — not
    /// subject to inherited ACEs overriding it) containing exactly one
    /// allow-ACE granting `GENERIC_ALL` (`GA`) to the resolved SID. Absence
    /// of any other ACE means every other principal is implicitly denied —
    /// no explicit deny entry is needed for that (standard Windows DACL
    /// semantics: an access check against a DACL with no matching allow ACE
    /// denies access).
    fn to_sddl(&self) -> std::io::Result<String> {
        let sid = self.resolve_sid()?;
        Ok(format!("D:P(A;;GA;;;{sid})"))
    }
}

/// An owned, self-relative Windows security descriptor allocated by
/// `ConvertStringSecurityDescriptorToSecurityDescriptorW`. Freed with
/// `LocalFree` on drop.
pub struct SecurityDescriptor {
    ptr: PSECURITY_DESCRIPTOR,
}

// SAFETY: the underlying buffer is a plain, self-contained heap allocation
// (no thread-affine handles or borrowed state); moving it across threads is
// sound. It is not `Sync` (no interior synchronization) but that is not
// needed here.
unsafe impl Send for SecurityDescriptor {}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: `self.ptr` was allocated by
            // `ConvertStringSecurityDescriptorToSecurityDescriptorW`, which
            // documents that the returned buffer must be freed with
            // `LocalFree`. It is freed at most once (this is the only place
            // that frees it, and `Drop::drop` runs at most once).
            unsafe {
                LocalFree(self.ptr as HLOCAL);
            }
        }
    }
}

impl SecurityDescriptor {
    /// Parses `sddl` into a security descriptor.
    pub fn from_sddl(sddl: &str) -> std::io::Result<Self> {
        let wide = to_wide_null_terminated(sddl);
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        let mut size: u32 = 0;

        // SAFETY: `wide` is a valid, NUL-terminated UTF-16 buffer that
        // outlives this call. `descriptor`/`size` are valid out-params on
        // the stack. On success the function allocates a new buffer via
        // `LocalAlloc` and writes its pointer into `descriptor`; ownership
        // of that allocation transfers to `SecurityDescriptor`, which frees
        // it exactly once in `Drop`.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                &mut size,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { ptr: descriptor })
    }

    /// Builds a `SECURITY_ATTRIBUTES` referencing this descriptor. The
    /// returned struct borrows `self` (via the raw pointer inside it) and
    /// must not outlive it.
    fn as_security_attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>())
                .expect("size_of::<SECURITY_ATTRIBUTES>() fits in u32"),
            lpSecurityDescriptor: self.ptr,
            bInheritHandle: 0,
        }
    }
}

/// Builds the security descriptor for `scope`.
pub fn security_descriptor_for(scope: &AclScope) -> std::io::Result<SecurityDescriptor> {
    let sddl = scope.to_sddl()?;
    SecurityDescriptor::from_sddl(&sddl)
}

/// Creates a named pipe server instance whose DACL is restricted per
/// `scope`, by way of
/// [`tokio`'s `create_with_security_attributes_raw`](tokio::net::windows::named_pipe::ServerOptions::create_with_security_attributes_raw).
pub fn create_named_pipe_with_acl(
    options: &tokio::net::windows::named_pipe::ServerOptions,
    addr: &str,
    scope: &AclScope,
) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    let descriptor = security_descriptor_for(scope)?;
    let mut attrs = descriptor.as_security_attributes();

    // SAFETY: `attrs` is a fully-initialized `SECURITY_ATTRIBUTES` whose
    // `lpSecurityDescriptor` points at `descriptor`'s live allocation for
    // the entire duration of this call; `descriptor` is not dropped until
    // after `create_with_security_attributes_raw` returns, satisfying the
    // method's documented safety requirement ("must either be null or point
    // at a valid instance of SECURITY_ATTRIBUTES").
    let result = unsafe {
        options.create_with_security_attributes_raw(addr, &mut attrs as *mut _ as *mut c_void)
    };
    drop(descriptor);
    result
}

/// The current process's user SID, as a string (e.g.
/// `"S-1-5-21-...-1001"`), resolved via `OpenProcessToken` +
/// `GetTokenInformation(TokenUser)` + `ConvertSidToStringSidW`.
pub fn current_user_sid_string() -> std::io::Result<String> {
    // SAFETY: `GetCurrentProcess` returns a pseudo-handle that does not need
    // closing (documented Win32 behavior) and is always valid for the
    // calling process.
    let process: HANDLE = unsafe { GetCurrentProcess() };

    let mut token: HANDLE = ptr::null_mut();
    // SAFETY: `process` is a valid pseudo-handle; `token` is a valid
    // stack out-param.
    let opened = unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) };
    if opened == 0 {
        return Err(std::io::Error::last_os_error());
    }

    let result = read_token_user_sid_string(token);

    // SAFETY: `token` was successfully opened above by this function and is
    // not used anywhere else; closed exactly once, after every use of it.
    unsafe {
        CloseHandle(token);
    }

    result
}

/// Reads `TokenUser` from an already-opened token handle and converts its
/// SID to a string. Split out of [`current_user_sid_string`] only so the
/// token handle's `CloseHandle` call has one obvious, unconditional call
/// site regardless of which branch below returns.
fn read_token_user_sid_string(token: HANDLE) -> std::io::Result<String> {
    // First call with a zero-size buffer to learn the required size; Win32
    // documents this as failing with ERROR_INSUFFICIENT_BUFFER while still
    // writing the required size into `needed` — exactly the pattern this
    // code relies on.
    let mut needed: u32 = 0;
    // SAFETY: a null buffer pointer with length 0 is the documented way to
    // query the required size; `needed` is a valid stack out-param.
    unsafe {
        GetTokenInformation(
            token,
            windows_sys::Win32::Security::TokenUser,
            ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 {
        return Err(std::io::Error::last_os_error());
    }

    let mut buffer = vec![0u8; needed as usize];
    let mut actual: u32 = 0;
    // SAFETY: `buffer` is a live allocation of exactly `needed` bytes,
    // matching the length passed in; `actual` is a valid stack out-param.
    // `TOKEN_USER` (read below) is `#[repr(C)]` and this buffer is sized to
    // hold it plus its variable-length trailing SID data, per Win32's
    // documented `GetTokenInformation(TokenUser, ...)` contract.
    let ok = unsafe {
        GetTokenInformation(
            token,
            windows_sys::Win32::Security::TokenUser,
            buffer.as_mut_ptr() as *mut c_void,
            needed,
            &mut actual,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY: `buffer` was filled by a successful `GetTokenInformation`
    // call requesting `TokenUser`, which guarantees it contains a valid,
    // fully-initialized `TOKEN_USER` at its start.
    let token_user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
    let sid = token_user.User.Sid;

    let mut sid_string_ptr: *mut u16 = ptr::null_mut();
    // SAFETY: `sid` is a valid PSID pointing into `buffer`, which is still
    // alive; `sid_string_ptr` is a valid stack out-param. On success this
    // allocates a new buffer that must be freed with `LocalFree`, freed
    // immediately below regardless of outcome.
    let converted = unsafe { ConvertSidToStringSidW(sid, &mut sid_string_ptr) };
    if converted == 0 {
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY: `sid_string_ptr` was just returned by a successful
    // `ConvertSidToStringSidW` call, which documents it as a NUL-terminated
    // wide string.
    let sid_string = unsafe { read_wide_c_string(sid_string_ptr) };
    // SAFETY: frees the allocation `ConvertSidToStringSidW` made, exactly
    // once, after its only use above.
    unsafe {
        LocalFree(sid_string_ptr as HLOCAL);
    }

    Ok(sid_string)
}

/// Converts a Rust `&str` into a NUL-terminated UTF-16 buffer suitable for
/// a `PCWSTR` argument.
fn to_wide_null_terminated(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Reads a NUL-terminated UTF-16 string starting at `ptr`.
///
/// # Safety
///
/// `ptr` must point at a valid, NUL-terminated UTF-16 string.
unsafe fn read_wide_c_string(ptr: *const u16) -> String {
    let mut len = 0usize;
    // SAFETY: caller guarantees `ptr` is NUL-terminated, so this loop
    // terminates within the string's actual allocation.
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
    }
    // SAFETY: `ptr..ptr+len` was just walked above and confirmed to
    // contain `len` valid `u16` values before the NUL terminator.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf16_lossy(slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_user_sid_string_resolves_a_plausible_sid() {
        let sid = current_user_sid_string().expect("must resolve the current process's SID");
        assert!(
            sid.starts_with("S-1-"),
            "expected a Windows SID string, got {sid}"
        );
    }

    #[test]
    fn current_user_scope_builds_sddl_with_a_real_sid() {
        let sddl = AclScope::CurrentUser.to_sddl().expect("resolve sddl");
        assert!(sddl.starts_with("D:P(A;;GA;;;S-1-"));
        assert!(sddl.ends_with(')'));
    }

    #[test]
    fn only_sid_scope_builds_sddl_with_the_given_sid_verbatim() {
        let sddl = AclScope::OnlySid("S-1-0-0".to_string())
            .to_sddl()
            .expect("build sddl");
        assert_eq!(sddl, "D:P(A;;GA;;;S-1-0-0)");
    }

    #[test]
    fn security_descriptor_from_sddl_accepts_a_well_known_sid() {
        let descriptor = SecurityDescriptor::from_sddl("D:P(A;;GA;;;S-1-0-0)");
        assert!(descriptor.is_ok());
    }

    #[test]
    fn security_descriptor_from_sddl_rejects_garbage_not_a_panic() {
        let result = SecurityDescriptor::from_sddl("this is not valid SDDL at all");
        assert!(result.is_err());
    }

    #[test]
    fn security_descriptor_for_only_sid_scope_succeeds() {
        let descriptor = security_descriptor_for(&AclScope::OnlySid("S-1-0-0".to_string()));
        assert!(descriptor.is_ok());
    }

    #[test]
    fn security_descriptor_for_current_user_scope_succeeds() {
        let descriptor = security_descriptor_for(&AclScope::CurrentUser);
        assert!(descriptor.is_ok());
    }
}
