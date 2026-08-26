//! Fail-closed, process-global SQLite allocator backed by one verified locked
//! Windows arena.  Normal startup can only consume the private digest-bound
//! profile from [`crate::secure_profile`]; no caller-supplied bounds can make
//! production storage ready.

use std::cell::UnsafeCell;
use std::ffi::{c_int, c_void};
use std::fmt;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use rusqlite::ffi;
use windows_sys::Win32::System::Memory::{
    GetProcessWorkingSetSizeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
    SetProcessWorkingSetSizeEx, VirtualAlloc, VirtualFree, VirtualLock, VirtualUnlock,
};
use windows_sys::Win32::System::ProcessStatus::{
    PSAPI_WORKING_SET_EX_INFORMATION, QueryWorkingSetEx,
};
use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

#[cfg(any(test, feature = "test-support"))]
use crate::secure_profile::ProfileInspection;
use crate::secure_profile::{AcceptedProfile, ProfileError, accepted_profile};

const PHASE_UNINITIALIZED: u8 = 0;
const PHASE_PREPARING: u8 = 1;
const PHASE_READY: u8 = 2;
const PHASE_SHUTTING_DOWN: u8 = 3;
const PHASE_FAILED: u8 = 4;
const PHASE_SHUT_DOWN: u8 = 5;
const BLOCK_ALIGNMENT: usize = 16;
const FREE_MARKER: usize = 0x4c4f_4b49_4652_4545;
const USED_MARKER: usize = 0x4c4f_4b49_5553_4544;
const LOCKED_BIT: usize = 1 << 22;

static PHASE: AtomicU8 = AtomicU8::new(PHASE_UNINITIALIZED);
static ARENA: GlobalArena = GlobalArena::new();

#[derive(Debug)]
pub enum SecureMemoryError {
    Profile(ProfileError),
    AlreadyActive,
    NotReady,
    InvalidPageSize,
    ProfilePageMismatch,
    WorkingSetQuery(std::io::Error),
    WorkingSetAdjustment(std::io::Error),
    Allocation(std::io::Error),
    Lock(std::io::Error),
    Attestation(std::io::Error),
    UnlockedPage { page_index: usize },
    SqliteConfiguration { code: c_int },
    SqliteShutdown { code: c_int },
    OutstandingAllocations { count: usize },
    Unlock(std::io::Error),
    Free(std::io::Error),
    WorkingSetRestore(std::io::Error),
}

impl fmt::Display for SecureMemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Profile(error) => write!(f, "{error}"),
            Self::AlreadyActive => write!(
                f,
                "the process-global SQLite secure arena is already active"
            ),
            Self::NotReady => write!(f, "the process-global SQLite secure arena is not ready"),
            Self::InvalidPageSize => write!(f, "Windows reported an invalid memory page size"),
            Self::ProfilePageMismatch => write!(
                f,
                "accepted arena bytes are not aligned to the host page size"
            ),
            Self::WorkingSetQuery(error) => {
                write!(f, "cannot read current-process working-set bounds: {error}")
            }
            Self::WorkingSetAdjustment(error) => write!(
                f,
                "cannot apply accepted current-process working-set bounds: {error}"
            ),
            Self::Allocation(error) => {
                write!(f, "cannot reserve and commit the secure arena: {error}")
            }
            Self::Lock(error) => write!(f, "cannot lock the complete secure arena: {error}"),
            Self::Attestation(error) => write!(f, "cannot attest secure-arena pages: {error}"),
            Self::UnlockedPage { page_index } => {
                write!(f, "secure-arena page {page_index} is not reported Locked")
            }
            Self::SqliteConfiguration { code } => write!(
                f,
                "sqlite3_config(SQLITE_CONFIG_MALLOC) failed with code {code}"
            ),
            Self::SqliteShutdown { code } => write!(f, "sqlite3_shutdown failed with code {code}"),
            Self::OutstandingAllocations { count } => write!(
                f,
                "SQLite shutdown left {count} secure-arena allocations active"
            ),
            Self::Unlock(error) => write!(f, "cannot unlock the wiped secure arena: {error}"),
            Self::Free(error) => write!(f, "cannot release the wiped secure arena: {error}"),
            Self::WorkingSetRestore(error) => write!(
                f,
                "cannot restore current-process working-set bounds: {error}"
            ),
        }
    }
}

impl std::error::Error for SecureMemoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Profile(error) => Some(error),
            Self::WorkingSetQuery(error)
            | Self::WorkingSetAdjustment(error)
            | Self::Allocation(error)
            | Self::Lock(error)
            | Self::Attestation(error)
            | Self::Unlock(error)
            | Self::Free(error)
            | Self::WorkingSetRestore(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ProfileError> for SecureMemoryError {
    fn from(error: ProfileError) -> Self {
        Self::Profile(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecureMemoryStats {
    pub arena_bytes: usize,
    pub active_allocations: usize,
    pub peak_bytes: usize,
    pub exhaustion_count: usize,
    pub fallback_count: usize,
    pub invalid_free_count: usize,
}

#[derive(Debug, Clone, Copy)]
struct WorkingSetSnapshot {
    minimum: usize,
    maximum: usize,
    flags: u32,
}

/// Owns the complete secure-memory lifecycle.  Dropping it performs the same
/// ordered best-effort teardown as [`shutdown`](Self::shutdown); callers that
/// need error reporting should call `shutdown` explicitly.
pub struct SecureMemoryRuntime {
    base: *mut u8,
    arena_bytes: usize,
    original_working_set: WorkingSetSnapshot,
    active: bool,
}

// The pointer is process-owned and never transferred; SQLite allocator calls
// synchronize through `ARENA`.  Moving this lifecycle token between threads
// does not grant unsynchronized access to the pointed-to bytes.
unsafe impl Send for SecureMemoryRuntime {}

impl fmt::Debug for SecureMemoryRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecureMemoryRuntime")
            .field("arena_bytes", &self.arena_bytes)
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

impl SecureMemoryRuntime {
    /// Bootstrap from the one digest-bound profile compiled into this binary.
    /// At this revision no profile is embedded, so this returns
    /// `Profile(Unavailable)` before any SQLite, key, database, filesystem, or
    /// Windows resource-policy operation.
    pub fn bootstrap() -> Result<Self, SecureMemoryError> {
        let profile = accepted_profile()?;
        Self::bootstrap_profile(profile)
    }

    fn bootstrap_profile(profile: AcceptedProfile) -> Result<Self, SecureMemoryError> {
        PHASE
            .compare_exchange(
                PHASE_UNINITIALIZED,
                PHASE_PREPARING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|_| SecureMemoryError::AlreadyActive)?;

        match Self::prepare(profile) {
            Ok(runtime) => {
                PHASE.store(PHASE_READY, Ordering::Release);
                Ok(runtime)
            }
            Err(error) => {
                PHASE.store(PHASE_FAILED, Ordering::Release);
                Err(error)
            }
        }
    }

    fn prepare(profile: AcceptedProfile) -> Result<Self, SecureMemoryError> {
        let bounds = profile.inspection;
        let page_size = system_page_size()?;
        if !bounds.arena_bytes.is_multiple_of(page_size) {
            return Err(SecureMemoryError::ProfilePageMismatch);
        }

        let process = unsafe { GetCurrentProcess() };
        let original_working_set = query_working_set(process)?;
        if unsafe {
            SetProcessWorkingSetSizeEx(
                process,
                bounds.working_set_min_bytes,
                bounds.working_set_max_bytes,
                0,
            )
        } == 0
        {
            return Err(SecureMemoryError::WorkingSetAdjustment(
                std::io::Error::last_os_error(),
            ));
        }

        let base = unsafe {
            VirtualAlloc(
                null(),
                bounds.arena_bytes,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_READWRITE,
            )
            .cast::<u8>()
        };
        if base.is_null() {
            let error = SecureMemoryError::Allocation(std::io::Error::last_os_error());
            let _ = restore_working_set(process, original_working_set);
            return Err(error);
        }

        // Commit every page before locking so the attestation covers actual
        // resident committed pages, not merely a reserved address range.
        for offset in (0..bounds.arena_bytes).step_by(page_size) {
            unsafe { std::ptr::write_volatile(base.add(offset), 0) };
        }
        if unsafe { VirtualLock(base.cast(), bounds.arena_bytes) } == 0 {
            let error = SecureMemoryError::Lock(std::io::Error::last_os_error());
            unsafe { VirtualFree(base.cast(), 0, MEM_RELEASE) };
            let _ = restore_working_set(process, original_working_set);
            return Err(error);
        }
        if let Err(error) = attest_every_page(process, base, bounds.arena_bytes, page_size) {
            unsafe {
                wipe(base, bounds.arena_bytes);
                VirtualUnlock(base.cast(), bounds.arena_bytes);
                VirtualFree(base.cast(), 0, MEM_RELEASE);
            }
            let _ = restore_working_set(process, original_working_set);
            return Err(error);
        }

        ARENA.initialize(base, bounds.arena_bytes);
        let methods = allocator_methods();
        let sqlite_result = unsafe {
            ffi::sqlite3_config(
                ffi::SQLITE_CONFIG_MALLOC,
                &methods as *const ffi::sqlite3_mem_methods,
            )
        };
        if sqlite_result != ffi::SQLITE_OK {
            ARENA.clear();
            unsafe {
                wipe(base, bounds.arena_bytes);
                VirtualUnlock(base.cast(), bounds.arena_bytes);
                VirtualFree(base.cast(), 0, MEM_RELEASE);
            }
            let _ = restore_working_set(process, original_working_set);
            return Err(SecureMemoryError::SqliteConfiguration {
                code: sqlite_result,
            });
        }

        Ok(Self {
            base,
            arena_bytes: bounds.arena_bytes,
            original_working_set,
            active: true,
        })
    }

    pub fn stats(&self) -> SecureMemoryStats {
        ARENA.stats()
    }

    pub fn shutdown(mut self) -> Result<SecureMemoryStats, SecureMemoryError> {
        let result = self.shutdown_inner();
        self.active = false;
        result
    }

    fn shutdown_inner(&mut self) -> Result<SecureMemoryStats, SecureMemoryError> {
        if !self.active {
            return Ok(ARENA.stats());
        }
        PHASE.store(PHASE_SHUTTING_DOWN, Ordering::Release);
        let sqlite_result = unsafe { ffi::sqlite3_shutdown() };

        // Read the pre-wipe stats, wipe the arena, and clear `ArenaState`
        // all inside one `ARENA.with_state()` critical section — the same
        // spinlock `sqlite_malloc`/`sqlite_free`/`sqlite_realloc` take. A
        // raw pointer wipe outside that lock (the previous shape of this
        // function) relied entirely on the caller-ordering convention that
        // every connection is already closed before `shutdown()` runs; it
        // was not itself an enforced invariant, so a still-in-flight
        // `sqlite_free` from elsewhere in the process could race the wipe
        // (a data race/UB, and a window where wiped bytes could be
        // overwritten by a stale free after this function believed the
        // arena was already zeroed). Moving the PHASE store to
        // `PHASE_SHUTTING_DOWN` above already makes `sqlite_malloc`/
        // `sqlite_realloc` refuse *new* allocations; taking `ARENA`'s own
        // lock here closes the remaining race against any allocator call
        // (including a `sqlite_free`, which does not check `PHASE`) instead
        // of merely detecting a leftover allocation after the fact via
        // `stats.active_allocations`.
        let base = self.base;
        let arena_bytes = self.arena_bytes;
        let stats = ARENA.with_state(|state| {
            let stats = SecureMemoryStats {
                arena_bytes: state.bytes,
                active_allocations: state.active_allocations,
                peak_bytes: state.peak_bytes,
                exhaustion_count: state.exhaustion_count,
                fallback_count: state.fallback_count,
                invalid_free_count: state.invalid_free_count,
            };
            unsafe { wipe(base, arena_bytes) };
            *state = ArenaState::EMPTY;
            stats
        });

        let unlock_result = unsafe { VirtualUnlock(self.base.cast(), self.arena_bytes) };
        let unlock_error = (unlock_result == 0).then(std::io::Error::last_os_error);
        let free_result = unsafe { VirtualFree(self.base.cast(), 0, MEM_RELEASE) };
        let free_error = (free_result == 0).then(std::io::Error::last_os_error);
        let restore_error =
            restore_working_set(unsafe { GetCurrentProcess() }, self.original_working_set).err();
        self.active = false;

        let error = if sqlite_result != ffi::SQLITE_OK {
            Some(SecureMemoryError::SqliteShutdown {
                code: sqlite_result,
            })
        } else if stats.active_allocations != 0 {
            Some(SecureMemoryError::OutstandingAllocations {
                count: stats.active_allocations,
            })
        } else if let Some(error) = unlock_error {
            Some(SecureMemoryError::Unlock(error))
        } else if let Some(error) = free_error {
            Some(SecureMemoryError::Free(error))
        } else {
            restore_error.map(SecureMemoryError::WorkingSetRestore)
        };
        PHASE.store(
            if error.is_some() {
                PHASE_FAILED
            } else {
                PHASE_SHUT_DOWN
            },
            Ordering::Release,
        );
        // Test-only: release the disposable-test-arena serialization slot
        // (see `TEST_ARENA_BUSY`/`test_support::bootstrap_disposable_test_arena`)
        // now that `PHASE` has left `PHASE_READY` and this instance's arena
        // is fully wiped/unlocked/freed. A no-op (idle flag already false)
        // for every runtime that was not bootstrapped through that path —
        // in particular, every real production shutdown, which never
        // compiles this store at all.
        #[cfg(any(test, feature = "test-support"))]
        TEST_ARENA_BUSY.store(false, Ordering::Release);
        error.map_or(Ok(stats), Err)
    }
}

pub(crate) fn require_ready() -> Result<(), SecureMemoryError> {
    if PHASE.load(Ordering::Acquire) == PHASE_READY {
        Ok(())
    } else {
        Err(SecureMemoryError::NotReady)
    }
}

/// Test-only: serializes exclusive ownership of the one process-global
/// arena/`PHASE` pair across repeated disposable bootstrap+shutdown cycles
/// within a single test binary process. See
/// [`test_support::bootstrap_disposable_test_arena`] for why this exists —
/// `ARENA`/`PHASE` are genuinely process-global (matching production's
/// true one-arena-per-process design), so only one *owned*, shutdown-able
/// disposable test arena may be logically alive at a time, even though
/// several `#[tokio::test]`/test functions in the same binary may run on
/// separate OS threads concurrently. Compiled only for this crate's own
/// tests or the opt-in `test-support` feature; absent entirely from a
/// release build.
#[cfg(any(test, feature = "test-support"))]
static TEST_ARENA_BUSY: AtomicBool = AtomicBool::new(false);

/// Test-only: forces `PHASE` back to its pristine pre-bootstrap state so
/// [`test_support::bootstrap_disposable_test_arena`] can bootstrap again in
/// the same process. Safe to call only while holding `TEST_ARENA_BUSY`
/// (that function's sole caller), which guarantees no other thread in this
/// process currently owns a live disposable arena — any previous owner
/// already ran `shutdown_inner` (leaving `PHASE` in a terminal
/// `PHASE_SHUT_DOWN`/`PHASE_FAILED` state) before releasing that slot.
/// Never compiled into a release build.
#[cfg(any(test, feature = "test-support"))]
fn reset_phase_for_disposable_test_bootstrap() {
    PHASE.store(PHASE_UNINITIALIZED, Ordering::Release);
}

impl Drop for SecureMemoryRuntime {
    fn drop(&mut self) {
        if self.active {
            let _ = self.shutdown_inner();
        }
    }
}

fn system_page_size() -> Result<usize, SecureMemoryError> {
    let mut info = SYSTEM_INFO::default();
    unsafe { GetSystemInfo(&mut info) };
    let page_size = info.dwPageSize as usize;
    if page_size == 0 || !page_size.is_power_of_two() {
        Err(SecureMemoryError::InvalidPageSize)
    } else {
        Ok(page_size)
    }
}

fn query_working_set(
    process: windows_sys::Win32::Foundation::HANDLE,
) -> Result<WorkingSetSnapshot, SecureMemoryError> {
    let mut minimum = 0_usize;
    let mut maximum = 0_usize;
    let mut flags = 0_u32;
    if unsafe { GetProcessWorkingSetSizeEx(process, &mut minimum, &mut maximum, &mut flags) } == 0 {
        Err(SecureMemoryError::WorkingSetQuery(
            std::io::Error::last_os_error(),
        ))
    } else {
        Ok(WorkingSetSnapshot {
            minimum,
            maximum,
            flags,
        })
    }
}

fn restore_working_set(
    process: windows_sys::Win32::Foundation::HANDLE,
    snapshot: WorkingSetSnapshot,
) -> Result<(), std::io::Error> {
    if unsafe {
        SetProcessWorkingSetSizeEx(process, snapshot.minimum, snapshot.maximum, snapshot.flags)
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn attest_every_page(
    process: windows_sys::Win32::Foundation::HANDLE,
    base: *mut u8,
    arena_bytes: usize,
    page_size: usize,
) -> Result<(), SecureMemoryError> {
    let page_count = arena_bytes / page_size;
    let mut pages = Vec::with_capacity(page_count);
    for page_index in 0..page_count {
        pages.push(PSAPI_WORKING_SET_EX_INFORMATION {
            VirtualAddress: unsafe { base.add(page_index * page_size) }.cast(),
            VirtualAttributes: Default::default(),
        });
    }
    let byte_count = pages
        .len()
        .checked_mul(std::mem::size_of::<PSAPI_WORKING_SET_EX_INFORMATION>())
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(SecureMemoryError::Attestation(std::io::Error::other(
            "attestation buffer exceeds QueryWorkingSetEx bounds",
        )))?;
    if unsafe { QueryWorkingSetEx(process, pages.as_mut_ptr().cast(), byte_count) } == 0 {
        return Err(SecureMemoryError::Attestation(
            std::io::Error::last_os_error(),
        ));
    }
    for (page_index, page) in pages.iter().enumerate() {
        let flags = unsafe { page.VirtualAttributes.Flags };
        if flags & LOCKED_BIT == 0 {
            return Err(SecureMemoryError::UnlockedPage { page_index });
        }
    }
    Ok(())
}

unsafe fn wipe(base: *mut u8, length: usize) {
    for offset in 0..length {
        unsafe { std::ptr::write_volatile(base.add(offset), 0) };
    }
    std::sync::atomic::compiler_fence(Ordering::SeqCst);
}

#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct BlockHeader {
    payload_bytes: usize,
    marker: usize,
}

const HEADER_BYTES: usize = std::mem::size_of::<BlockHeader>();

struct ArenaState {
    base: *mut u8,
    bytes: usize,
    active_allocations: usize,
    allocated_bytes: usize,
    peak_bytes: usize,
    exhaustion_count: usize,
    fallback_count: usize,
    invalid_free_count: usize,
}

impl ArenaState {
    const EMPTY: Self = Self {
        base: null_mut(),
        bytes: 0,
        active_allocations: 0,
        allocated_bytes: 0,
        peak_bytes: 0,
        exhaustion_count: 0,
        fallback_count: 0,
        invalid_free_count: 0,
    };

    unsafe fn initialize(&mut self, base: *mut u8, bytes: usize) {
        *self = Self {
            base,
            bytes,
            ..Self::EMPTY
        };
        unsafe {
            write_header(
                base,
                BlockHeader {
                    payload_bytes: bytes - HEADER_BYTES,
                    marker: FREE_MARKER,
                },
            );
        }
    }

    unsafe fn allocate(&mut self, requested: usize) -> *mut c_void {
        let Some(wanted) = align_up(requested, BLOCK_ALIGNMENT) else {
            self.exhaustion_count = self.exhaustion_count.saturating_add(1);
            return null_mut();
        };
        let mut offset = 0_usize;
        while offset + HEADER_BYTES <= self.bytes {
            let header_ptr = unsafe { self.base.add(offset) };
            let header = unsafe { read_header(header_ptr) };
            if !valid_block(header, offset, self.bytes) {
                self.exhaustion_count = self.exhaustion_count.saturating_add(1);
                return null_mut();
            }
            if header.marker == FREE_MARKER && header.payload_bytes >= wanted {
                let remainder = header.payload_bytes - wanted;
                let allocated_payload = if remainder >= HEADER_BYTES + BLOCK_ALIGNMENT {
                    wanted
                } else {
                    // Consume the entire free block when the tail cannot hold
                    // a valid aligned header+payload.  Leaving an untracked
                    // sliver would make the next physical scan interpret
                    // wiped payload bytes as a header.
                    header.payload_bytes
                };
                unsafe {
                    write_header(
                        header_ptr,
                        BlockHeader {
                            payload_bytes: allocated_payload,
                            marker: USED_MARKER,
                        },
                    );
                }
                if remainder >= HEADER_BYTES + BLOCK_ALIGNMENT {
                    let next = unsafe { header_ptr.add(HEADER_BYTES + wanted) };
                    unsafe {
                        write_header(
                            next,
                            BlockHeader {
                                payload_bytes: remainder - HEADER_BYTES,
                                marker: FREE_MARKER,
                            },
                        );
                    }
                }
                self.active_allocations = self.active_allocations.saturating_add(1);
                self.allocated_bytes = self.allocated_bytes.saturating_add(allocated_payload);
                self.peak_bytes = self.peak_bytes.max(self.allocated_bytes);
                return unsafe { header_ptr.add(HEADER_BYTES) }.cast();
            }
            offset += HEADER_BYTES + header.payload_bytes;
        }
        self.exhaustion_count = self.exhaustion_count.saturating_add(1);
        null_mut()
    }

    unsafe fn free(&mut self, pointer: *mut c_void) {
        if pointer.is_null() {
            return;
        }
        let Some((offset, mut header)) = (unsafe { self.find_used(pointer) }) else {
            self.invalid_free_count = self.invalid_free_count.saturating_add(1);
            return;
        };
        let header_ptr = unsafe { self.base.add(offset) };
        header.marker = FREE_MARKER;
        unsafe { wipe(header_ptr.add(HEADER_BYTES), header.payload_bytes) };
        unsafe { write_header(header_ptr, header) };
        self.active_allocations = self.active_allocations.saturating_sub(1);
        self.allocated_bytes = self.allocated_bytes.saturating_sub(header.payload_bytes);
        unsafe { self.coalesce() };
    }

    unsafe fn reallocate(&mut self, pointer: *mut c_void, requested: usize) -> *mut c_void {
        if pointer.is_null() {
            return unsafe { self.allocate(requested) };
        }
        if requested == 0 {
            unsafe { self.free(pointer) };
            return null_mut();
        }
        let Some((_offset, header)) = (unsafe { self.find_used(pointer) }) else {
            self.invalid_free_count = self.invalid_free_count.saturating_add(1);
            return null_mut();
        };
        let Some(wanted) = align_up(requested, BLOCK_ALIGNMENT) else {
            self.exhaustion_count = self.exhaustion_count.saturating_add(1);
            return null_mut();
        };
        if header.payload_bytes >= wanted {
            return pointer;
        }
        let replacement = unsafe { self.allocate(wanted) };
        if replacement.is_null() {
            return null_mut();
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                pointer.cast::<u8>(),
                replacement.cast::<u8>(),
                header.payload_bytes,
            );
            self.free(pointer);
        }
        replacement
    }

    unsafe fn allocation_size(&self, pointer: *mut c_void) -> c_int {
        let Some((_offset, header)) = (unsafe { self.find_used(pointer) }) else {
            return 0;
        };
        c_int::try_from(header.payload_bytes).unwrap_or(c_int::MAX)
    }

    unsafe fn find_used(&self, pointer: *mut c_void) -> Option<(usize, BlockHeader)> {
        if self.base.is_null() || pointer.is_null() {
            return None;
        }
        let pointer_value = pointer as usize;
        let base_value = self.base as usize;
        if pointer_value < base_value + HEADER_BYTES || pointer_value >= base_value + self.bytes {
            return None;
        }
        let mut offset = 0_usize;
        while offset + HEADER_BYTES <= self.bytes {
            let header = unsafe { read_header(self.base.add(offset)) };
            if !valid_block(header, offset, self.bytes) {
                return None;
            }
            if base_value + offset + HEADER_BYTES == pointer_value {
                return (header.marker == USED_MARKER).then_some((offset, header));
            }
            offset += HEADER_BYTES + header.payload_bytes;
        }
        None
    }

    unsafe fn coalesce(&mut self) {
        let mut offset = 0_usize;
        while offset + HEADER_BYTES <= self.bytes {
            let current_ptr = unsafe { self.base.add(offset) };
            let mut current = unsafe { read_header(current_ptr) };
            if !valid_block(current, offset, self.bytes) {
                return;
            }
            let next_offset = offset + HEADER_BYTES + current.payload_bytes;
            if next_offset + HEADER_BYTES > self.bytes {
                return;
            }
            let next_ptr = unsafe { self.base.add(next_offset) };
            let next = unsafe { read_header(next_ptr) };
            if !valid_block(next, next_offset, self.bytes) {
                return;
            }
            if current.marker == FREE_MARKER && next.marker == FREE_MARKER {
                current.payload_bytes = current
                    .payload_bytes
                    .saturating_add(HEADER_BYTES)
                    .saturating_add(next.payload_bytes);
                unsafe { write_header(current_ptr, current) };
            } else {
                offset = next_offset;
            }
        }
    }
}

struct GlobalArena {
    spin: AtomicBool,
    state: UnsafeCell<ArenaState>,
}

unsafe impl Sync for GlobalArena {}

impl GlobalArena {
    const fn new() -> Self {
        Self {
            spin: AtomicBool::new(false),
            state: UnsafeCell::new(ArenaState::EMPTY),
        }
    }

    fn with_state<T>(&self, operation: impl FnOnce(&mut ArenaState) -> T) -> T {
        while self
            .spin
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        struct Unlock<'a>(&'a AtomicBool);
        impl Drop for Unlock<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _unlock = Unlock(&self.spin);
        // SAFETY: the spin lock above provides exclusive mutable access.
        operation(unsafe { &mut *self.state.get() })
    }

    fn initialize(&self, base: *mut u8, bytes: usize) {
        self.with_state(|state| unsafe { state.initialize(base, bytes) });
    }

    fn clear(&self) {
        self.with_state(|state| *state = ArenaState::EMPTY);
    }

    fn stats(&self) -> SecureMemoryStats {
        self.with_state(|state| SecureMemoryStats {
            arena_bytes: state.bytes,
            active_allocations: state.active_allocations,
            peak_bytes: state.peak_bytes,
            exhaustion_count: state.exhaustion_count,
            fallback_count: state.fallback_count,
            invalid_free_count: state.invalid_free_count,
        })
    }
}

fn allocator_methods() -> ffi::sqlite3_mem_methods {
    ffi::sqlite3_mem_methods {
        xMalloc: Some(sqlite_malloc),
        xFree: Some(sqlite_free),
        xRealloc: Some(sqlite_realloc),
        xSize: Some(sqlite_size),
        xRoundup: Some(sqlite_roundup),
        xInit: Some(sqlite_init),
        xShutdown: Some(sqlite_shutdown_callback),
        pAppData: null_mut(),
    }
}

unsafe extern "C" fn sqlite_malloc(size: c_int) -> *mut c_void {
    if size <= 0 || PHASE.load(Ordering::Acquire) != PHASE_READY {
        return null_mut();
    }
    ARENA.with_state(|state| unsafe { state.allocate(size as usize) })
}

unsafe extern "C" fn sqlite_free(pointer: *mut c_void) {
    ARENA.with_state(|state| unsafe { state.free(pointer) });
}

unsafe extern "C" fn sqlite_realloc(pointer: *mut c_void, size: c_int) -> *mut c_void {
    if PHASE.load(Ordering::Acquire) != PHASE_READY {
        return null_mut();
    }
    ARENA.with_state(|state| unsafe { state.reallocate(pointer, size.max(0) as usize) })
}

unsafe extern "C" fn sqlite_size(pointer: *mut c_void) -> c_int {
    ARENA.with_state(|state| unsafe { state.allocation_size(pointer) })
}

unsafe extern "C" fn sqlite_roundup(size: c_int) -> c_int {
    if size <= 0 {
        return 0;
    }
    align_up(size as usize, BLOCK_ALIGNMENT)
        .and_then(|value| c_int::try_from(value).ok())
        .unwrap_or(0)
}

unsafe extern "C" fn sqlite_init(_app_data: *mut c_void) -> c_int {
    if PHASE.load(Ordering::Acquire) == PHASE_READY {
        ffi::SQLITE_OK
    } else {
        ffi::SQLITE_NOMEM
    }
}

unsafe extern "C" fn sqlite_shutdown_callback(_app_data: *mut c_void) {}

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    value
        .checked_add(alignment - 1)
        .map(|rounded| rounded & !(alignment - 1))
}

fn valid_block(header: BlockHeader, offset: usize, arena_bytes: usize) -> bool {
    matches!(header.marker, FREE_MARKER | USED_MARKER)
        && header.payload_bytes.is_multiple_of(BLOCK_ALIGNMENT)
        && offset
            .checked_add(HEADER_BYTES)
            .and_then(|value| value.checked_add(header.payload_bytes))
            .is_some_and(|end| end <= arena_bytes)
}

unsafe fn read_header(pointer: *const u8) -> BlockHeader {
    unsafe { std::ptr::read(pointer.cast::<BlockHeader>()) }
}

unsafe fn write_header(pointer: *mut u8, header: BlockHeader) {
    unsafe { std::ptr::write(pointer.cast::<BlockHeader>(), header) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_allocator_exhausts_without_fallback_and_reuses_freed_space() {
        let mut backing = vec![0_u8; 4_096];
        let mut arena = ArenaState::EMPTY;
        unsafe { arena.initialize(backing.as_mut_ptr(), backing.len()) };
        let first = unsafe { arena.allocate(3_000) };
        assert!(!first.is_null());
        let exhausted = unsafe { arena.allocate(2_000) };
        assert!(exhausted.is_null());
        assert_eq!(arena.exhaustion_count, 1);
        assert_eq!(arena.fallback_count, 0);
        unsafe { arena.free(first) };
        let reused = unsafe { arena.allocate(3_000) };
        assert!(!reused.is_null());
        unsafe { arena.free(reused) };
        assert_eq!(arena.active_allocations, 0);
    }

    #[test]
    fn reallocation_failure_preserves_original_allocation() {
        let mut backing = vec![0_u8; 4_096];
        let mut arena = ArenaState::EMPTY;
        unsafe { arena.initialize(backing.as_mut_ptr(), backing.len()) };
        let pointer = unsafe { arena.allocate(3_000) };
        unsafe { std::ptr::write_bytes(pointer, 0x5a, 3_000) };
        assert!(unsafe { arena.reallocate(pointer, 3_500) }.is_null());
        assert_eq!(unsafe { arena.allocation_size(pointer) }, 3_008);
        assert_eq!(unsafe { *pointer.cast::<u8>() }, 0x5a);
        unsafe { arena.free(pointer) };
    }

    #[test]
    fn unsplittable_tail_is_absorbed_into_the_allocation() {
        let mut backing = vec![0_u8; 4_096];
        let mut arena = ArenaState::EMPTY;
        unsafe { arena.initialize(backing.as_mut_ptr(), backing.len()) };
        let pointer = unsafe { arena.allocate(4_060) };
        assert!(!pointer.is_null());
        assert_eq!(unsafe { arena.allocation_size(pointer) }, 4_080);
        unsafe { arena.free(pointer) };
        assert_eq!(arena.active_allocations, 0);
        assert!(!unsafe { arena.allocate(4_060) }.is_null());
    }

    #[test]
    fn bootstrap_refuses_before_any_profile_or_os_mutation() {
        // `accepted_profile()` reads the compiled-in `ACCEPTED_PROFILE`
        // (always `None` at this revision) and short-circuits with `?`
        // before `bootstrap_profile` ever touches `PHASE` or any Windows
        // resource. That is a property of `bootstrap()`'s own control flow,
        // not of this test running first in the binary: this test module
        // shares the one process-global `PHASE` with `managed::tests`, which
        // deliberately bootstraps a disposable, non-production test arena
        // (see `test_support::ensure_test_arena_ready`) to exercise real
        // SQLCipher opens. So this asserts the *unchanged* phase relative to
        // whatever this process already established, not an absolute
        // pristine `PHASE_UNINITIALIZED`, which would make this test's
        // result depend on unspecified intra-binary test ordering.
        let phase_before = PHASE.load(Ordering::Acquire);
        assert!(matches!(
            SecureMemoryRuntime::bootstrap(),
            Err(SecureMemoryError::Profile(ProfileError::Unavailable))
        ));
        assert_eq!(PHASE.load(Ordering::Acquire), phase_before);
    }
}

/// Test-only bootstrap of a disposable, non-production secure arena so this
/// crate's own tests can exercise real SQLCipher opens end to end.
///
/// This deliberately bypasses [`accepted_profile`] (the sole production
/// entrypoint, which always fails closed at this revision because no digest
/// is embedded) via the crate-private [`SecureMemoryRuntime::bootstrap_profile`].
/// It compiles only for this crate's own unit-test build (`cfg(test)`) or
/// when the `test-support` Cargo feature is enabled — a feature that is off
/// by default and only turned on by this crate's own self-referential
/// `[dev-dependencies]` entry, so this bypass never reaches a downstream
/// consumer or a release build. Windows commits/locks the arena via the same
/// real `VirtualAlloc`/`VirtualLock`/`QueryWorkingSetEx` path production
/// uses; exact host bounds validation remains
/// `DAT-002-MEM-HOST-VERIFY-001`'s scope.
///
/// `pub` (not `pub(crate)`) so this crate's separate-process `tests/*.rs`
/// integration-test binaries — which link against this crate as an ordinary
/// external dependency and therefore cannot see `cfg(test)`-only or
/// crate-private items — can still reach it through the `test-support`
/// feature.
///
/// The bootstrapped runtime is intentionally leaked for the lifetime of the
/// test binary process (not shut down between tests): `SecureMemoryRuntime`
/// can only bootstrap once per process, matching its real one-time startup
/// lifecycle, so tests share one arena instance instead of racing to
/// bootstrap/shutdown it repeatedly. The OS reclaims the locked/committed
/// pages unconditionally when the test process exits.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::{
        AcceptedProfile, Ordering, ProfileInspection, SecureMemoryRuntime, TEST_ARENA_BUSY,
    };

    /// Fixed size of the disposable, non-production test-only arena.
    ///
    /// This is deliberately *not* the same value as any production bound
    /// (there is no accepted production bound yet — that is
    /// `DAT-002-MEM-HOST-VERIFY-001`'s scope) and changing it has zero
    /// effect on `crate::secure_profile`'s production `ACCEPTED_PROFILE`
    /// gate, which stays `None` regardless.
    ///
    /// Sized at 16 MiB (raised from an initial 1 MiB) after measuring real
    /// concurrent `cargo test --workspace` runs (default thread-per-core
    /// test harness, no `--test-threads=1`) across every crate that now
    /// bootstraps this arena
    /// (`loki-storage`, `loki-monitoring`, `loki-local-notifications`,
    /// `loki-local-setup`, `loki-local-authority`, `loki-local-devices`,
    /// `loki-incidents`, `loki-approvals`, `loki-agents`,
    /// `loki-agent-delivery`, `loki-runtime`): 1 MiB reproducibly exhausted
    /// under real parallel SqlCipher-backed test execution
    /// (`Sqlite(SqliteFailure(OutOfMemory))`, see
    /// `Docs/graph/handoffs/DAT-002-MEM-CODE-001-2.md` for the exact
    /// evidence), because every SqlCipher connection's page cache, prepared
    /// statements, and per-connection scratch buffers all come out of this
    /// one process-global arena and several crates' tests now legitimately
    /// hold multiple such connections open at once. 16 MiB is zero-fallback,
    /// fixed-size, and page-aligned exactly like the production design (no
    /// growth, no silent degradation) — it is simply a larger fixed test-only
    /// number, chosen empirically rather than guessed, that comfortably
    /// covers observed worst-case concurrent test demand with headroom.
    const DISPOSABLE_TEST_ARENA_BYTES: usize = 16 * 1024 * 1024;

    fn disposable_test_profile() -> AcceptedProfile {
        AcceptedProfile {
            inspection: ProfileInspection {
                arena_bytes: DISPOSABLE_TEST_ARENA_BYTES,
                working_set_min_bytes: DISPOSABLE_TEST_ARENA_BYTES * 8,
                working_set_max_bytes: DISPOSABLE_TEST_ARENA_BYTES * 32,
                connection_cap: 4,
                cache_pages_per_connection: 64,
                source_sha256: [0_u8; 32],
                build_sha256: [0_u8; 32],
                manifest_sha256: [0_u8; 32],
            },
        }
    }

    /// Bootstraps the shared disposable arena at most once per process and
    /// leaves it ready (leaked, never shut down) for the rest of the test
    /// binary's lifetime. This is the right choice for the large majority of
    /// this workspace's tests, which only need `require_ready()` to succeed
    /// (via `Storage::open`/`open_managed`) and never themselves own or shut
    /// down a [`SecureMemoryRuntime`] value.
    pub fn ensure_test_arena_ready() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let runtime = SecureMemoryRuntime::bootstrap_profile(disposable_test_profile())
                .expect("disposable test-only secure arena bootstrap");
            // Intentional test-only leak: keep the arena ready for the rest
            // of this process's tests instead of tearing it down, since
            // `bootstrap_profile` cannot run a second time in one process.
            std::mem::forget(runtime);
        });
    }

    /// Bootstraps a fresh disposable arena and returns it **owned**, for the
    /// rare caller that itself owns and must shut down a real
    /// [`SecureMemoryRuntime`] value (mirroring production's own
    /// `SecureMemoryRuntime::bootstrap()` shape) rather than merely
    /// depending on the shared, leaked, never-shut-down arena
    /// [`ensure_test_arena_ready`] provides. `crates/loki-runtime`'s
    /// `ComposedHost::open_with_test_arena` is the motivating caller: unlike
    /// every other consumer, `ComposedHost` stores the returned
    /// `SecureMemoryRuntime` and calls `.shutdown()` on it itself as part of
    /// its own real startup/shutdown lifecycle, so it needs an owned value,
    /// not a flag check.
    ///
    /// `PHASE`/`ARENA` are genuinely process-global — only one *owned*,
    /// shutdown-able disposable arena may be alive at a time, even across
    /// several test functions run concurrently in one test binary process.
    /// This function therefore blocks (a bounded, cooperative busy-wait, not
    /// a real long-term contention path — the arena's full open/use/shutdown
    /// cycle is short) until any previous disposable-arena owner in this
    /// same process has called `.shutdown()` (or been dropped), then resets
    /// `PHASE` back to its pristine pre-bootstrap state and bootstraps
    /// again. Every other test binary in the workspace runs as its own
    /// separate OS process, so this never observes or affects arena state
    /// in a different crate's test binary.
    ///
    /// Panics (rather than silently returning a non-functional value) if the
    /// disposable bootstrap itself fails — that would mean a real bug in
    /// this test-only path, not a legitimate test outcome to swallow.
    pub fn bootstrap_disposable_test_arena() -> SecureMemoryRuntime {
        while TEST_ARENA_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            std::thread::yield_now();
        }
        super::reset_phase_for_disposable_test_bootstrap();
        match SecureMemoryRuntime::bootstrap_profile(disposable_test_profile()) {
            Ok(runtime) => runtime,
            Err(error) => {
                // Bootstrap never became `active`, so `shutdown_inner` will
                // never run for it and never release the slot itself — release
                // it here so a later caller in the same process is not
                // deadlocked by this failure.
                TEST_ARENA_BUSY.store(false, Ordering::Release);
                panic!("disposable test-only owned secure arena bootstrap failed: {error}");
            }
        }
    }
}
