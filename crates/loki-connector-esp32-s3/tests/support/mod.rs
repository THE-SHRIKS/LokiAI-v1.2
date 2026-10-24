//! `FakeEsp32Transport` — the injected fake implementation of
//! [`loki_connector_esp32_s3::transport::Esp32Transport`] this crate's own
//! tests use, mirroring `crates/loki-connectors-sdk/tests/support/mod.rs`'s
//! `MockConnector` pattern exactly: **not** part of this crate's published
//! library surface (`src/lib.rs`'s module map note says so explicitly), used
//! only by `tests/*.rs`, and every call counted so a test can assert not
//! only *what* an operation returned but *whether* a given transport method
//! was actually invoked.
//!
//! `flash_memory` gives `write_flash`/`read_flash` a real (in-memory, no
//! device) round trip — a byte actually written by `write_flash` is what a
//! later `read_flash` returns by default, exactly like real flash, unlike a
//! canned fixed response. `with_read_flash_override` exists specifically to
//! simulate a readback that does *not* match what was written (a corrupted
//! write, or a device that silently rolled back) for this node's required
//! "execute succeeds but verify fails" adversarial scenario.

use std::collections::VecDeque;

use loki_connector_esp32_s3::transport::{
    ChipIdResponse, Esp32Transport, FlashIdResponse, ResetMode, TransportError,
};

use async_trait::async_trait;

/// One transport method's configured behavior for this fake.
#[derive(Debug, Clone)]
pub enum FakeOutcome<T: Clone> {
    Ok(T),
    Busy(String),
    PermissionDenied(String),
    Disconnected(String),
    Malformed(String),
    Timeout,
}

impl<T: Clone> FakeOutcome<T> {
    fn resolve(&self) -> Result<T, TransportError> {
        match self {
            FakeOutcome::Ok(value) => Ok(value.clone()),
            FakeOutcome::Busy(detail) => Err(TransportError::Busy(detail.clone())),
            FakeOutcome::PermissionDenied(detail) => {
                Err(TransportError::PermissionDenied(detail.clone()))
            }
            FakeOutcome::Disconnected(detail) => Err(TransportError::Disconnected(detail.clone())),
            FakeOutcome::Malformed(detail) => Err(TransportError::Malformed(detail.clone())),
            FakeOutcome::Timeout => Err(TransportError::Timeout),
        }
    }
}

/// Call counters, exposed read-only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallCounts {
    pub query_chip_id: u32,
    pub query_flash_id: u32,
    pub momentary_reset: u32,
    pub write_flash: u32,
    pub read_flash: u32,
}

/// The confirmed-profile fixture chip-id response (`Docs/CONTEXT.md`
/// OQ-001). The MAC address is an obviously-fake fixture value, not a real
/// device identifier.
pub fn fixture_chip_id() -> ChipIdResponse {
    ChipIdResponse {
        chip_type: "esp32s3".to_string(),
        silicon_revision: "v0.2".to_string(),
        mac_address: "aa:bb:cc:dd:ee:ff".to_string(),
    }
}

/// The confirmed-profile fixture flash-id response: 16MB, `Docs/CONTEXT.md`
/// OQ-001.
pub fn fixture_flash_id() -> FlashIdResponse {
    FlashIdResponse {
        manufacturer_id: 0xC8,
        device_id: 0x4018,
        size_bytes: 16 * 1024 * 1024,
    }
}

/// A 32-byte fixture installation key. Real installation keys are opaque,
/// OS-protected secrets (`Docs/ARCHITECTURE.md`); this crate never generates
/// one, so tests use a fixed, clearly-fake value — matches
/// `loki-connectors-sdk/src/identity.rs`'s own test fixture convention.
pub fn fixture_installation_key() -> Vec<u8> {
    b"fixture-installation-key-32bytes".to_vec()
}

pub struct FakeEsp32Transport {
    chip_id_behavior: FakeOutcome<ChipIdResponse>,
    /// Optional per-call override sequence for `query_chip_id`, consumed
    /// front-to-back before falling back to `chip_id_behavior` once
    /// exhausted — lets a test give different, ordered answers to
    /// `preflight`'s vs. `verify`'s (vs. `recover`'s) chip-id query within
    /// one operation (e.g. "present before restart, gone after it"),
    /// something a single fixed [`FakeOutcome`] cannot express.
    chip_id_sequence: VecDeque<FakeOutcome<ChipIdResponse>>,
    flash_id_behavior: FakeOutcome<FlashIdResponse>,
    reset_behavior: FakeOutcome<()>,
    write_flash_behavior: FakeOutcome<()>,
    read_flash_override: Option<FakeOutcome<Vec<u8>>>,
    flash_memory: Vec<u8>,
    calls: CallCounts,
}

impl FakeEsp32Transport {
    /// A transport configured to behave like a healthy, present device at
    /// every stage.
    pub fn healthy() -> Self {
        Self {
            chip_id_behavior: FakeOutcome::Ok(fixture_chip_id()),
            chip_id_sequence: VecDeque::new(),
            flash_id_behavior: FakeOutcome::Ok(fixture_flash_id()),
            reset_behavior: FakeOutcome::Ok(()),
            write_flash_behavior: FakeOutcome::Ok(()),
            read_flash_override: None,
            flash_memory: Vec::new(),
            calls: CallCounts::default(),
        }
    }

    /// Sets the fallback `query_chip_id` behavior used once any configured
    /// [`Self::with_chip_id_sequence`] is exhausted (or when no sequence was
    /// ever configured).
    #[must_use]
    pub fn with_chip_id(mut self, behavior: FakeOutcome<ChipIdResponse>) -> Self {
        self.chip_id_behavior = behavior;
        self
    }

    /// Installs an ordered sequence of `query_chip_id` answers — the first
    /// call gets `sequence[0]`, the second `sequence[1]`, and so on; once
    /// exhausted, every further call gets [`Self::with_chip_id`]'s fallback
    /// (or the `healthy()` default if never set).
    #[must_use]
    pub fn with_chip_id_sequence(mut self, sequence: Vec<FakeOutcome<ChipIdResponse>>) -> Self {
        self.chip_id_sequence = sequence.into();
        self
    }

    #[must_use]
    pub fn with_flash_id(mut self, behavior: FakeOutcome<FlashIdResponse>) -> Self {
        self.flash_id_behavior = behavior;
        self
    }

    #[must_use]
    pub fn with_reset(mut self, behavior: FakeOutcome<()>) -> Self {
        self.reset_behavior = behavior;
        self
    }

    #[must_use]
    pub fn with_write_flash(mut self, behavior: FakeOutcome<()>) -> Self {
        self.write_flash_behavior = behavior;
        self
    }

    /// Overrides `read_flash`'s default "read back what was actually
    /// written" behavior — see this module's doc comment.
    #[must_use]
    pub fn with_read_flash_override(mut self, behavior: FakeOutcome<Vec<u8>>) -> Self {
        self.read_flash_override = Some(behavior);
        self
    }

    /// Pre-loads `data` into simulated flash starting at `offset`, without
    /// going through `write_flash` — lets a test exercise `verify_artifact`
    /// (a read-only capability) against already-"flashed" content, without
    /// first having to run a `flash_artifact` operation.
    #[must_use]
    pub fn with_preloaded_flash(mut self, offset: usize, data: &[u8]) -> Self {
        let end = offset + data.len();
        if self.flash_memory.len() < end {
            self.flash_memory.resize(end, 0xFF);
        }
        self.flash_memory[offset..end].copy_from_slice(data);
        self
    }

    pub fn calls(&self) -> CallCounts {
        self.calls
    }
}

#[async_trait]
impl Esp32Transport for FakeEsp32Transport {
    async fn query_chip_id(&mut self) -> Result<ChipIdResponse, TransportError> {
        self.calls.query_chip_id += 1;
        match self.chip_id_sequence.pop_front() {
            Some(next) => next.resolve(),
            None => self.chip_id_behavior.resolve(),
        }
    }

    async fn query_flash_id(&mut self) -> Result<FlashIdResponse, TransportError> {
        self.calls.query_flash_id += 1;
        self.flash_id_behavior.resolve()
    }

    async fn momentary_reset(&mut self, _mode: ResetMode) -> Result<(), TransportError> {
        self.calls.momentary_reset += 1;
        self.reset_behavior.resolve()
    }

    async fn write_flash(&mut self, offset: u32, data: &[u8]) -> Result<(), TransportError> {
        self.calls.write_flash += 1;
        self.write_flash_behavior.resolve()?;
        let start = offset as usize;
        let end = start + data.len();
        if self.flash_memory.len() < end {
            // Real erased flash reads as `0xFF`; padding with that value
            // (rather than `0`) matches what a real device would report for
            // never-written regions.
            self.flash_memory.resize(end, 0xFF);
        }
        self.flash_memory[start..end].copy_from_slice(data);
        Ok(())
    }

    async fn read_flash(&mut self, offset: u32, length: u32) -> Result<Vec<u8>, TransportError> {
        self.calls.read_flash += 1;
        if let Some(behavior) = &self.read_flash_override {
            return behavior.resolve();
        }
        let start = offset as usize;
        let end = start + length as usize;
        if self.flash_memory.len() < end {
            self.flash_memory.resize(end, 0xFF);
        }
        Ok(self.flash_memory[start..end].to_vec())
    }
}
