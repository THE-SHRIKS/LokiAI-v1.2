//! `Esp32Transport` — the injectable ROM/bootloader transport boundary this
//! connector's lifecycle methods drive.
//!
//! ## Why this trait, and why shaped this way
//!
//! `Docs/CONNECTOR_SPEC.md` §4 says a connector receives "a least-privilege
//! transport handle or approved tool adapter", but never standardizes what
//! that handle *is* (`loki-connectors-sdk/src/operation.rs`'s module doc:
//! "each connector implementation owns its own transport wiring privately").
//! This node's packet requires that ownership to take the shape of an
//! injectable trait covering "exactly the ROM/bootloader operations needed
//! (chip-ID query, flash-ID query, momentary reset, write-flash, read-back
//! verify)" — the five operations below — so that:
//!
//! - this crate's own tests can inject a fake (`tests/support/mod.rs`'s
//!   `FakeEsp32Transport`) and reach zero real device I/O (this node's hard
//!   forbidden-scope item), while
//! - a later, separately-gated node (`CON-003-PHYS-001`) can implement this
//!   *exact* trait against a real serial/USB-JTAG transport without any
//!   connector-lifecycle rework, because every real esptool-class operation
//!   this connector's `preflight`/`execute`/`verify`/`recover` methods need
//!   is already named here.
//!
//! Each method models what a real ROM-bootloader ("esptool-class") exchange
//! actually is, not merely what this node's fake transport happens to need:
//!
//! - [`Esp32Transport::query_chip_id`]: the ROM bootloader's chip-info
//!   readback (chip type/silicon revision/factory MAC) — the standard,
//!   non-mutating way to prove a specific silicon device is present. Used by
//!   `identify`'s preflight (reachability) and verify (re-identification)
//!   stages, and by every other capability's own preflight as a
//!   "device present and responsive" check.
//! - [`Esp32Transport::query_flash_id`]: the SPI flash's own JEDEC ID and
//!   size readback (`Docs/CONTEXT.md` OQ-001: 16MB external quad flash) —
//!   non-mutating, used to confirm flash geometry before any write.
//! - [`Esp32Transport::momentary_reset`]: the RTS/DTR-line reset sequence
//!   every ESP32 tool performs to enter the ROM bootloader or to boot the
//!   application image (`Docs/graph/handoffs/CON-003-PRE-001-1.md`:
//!   "standard momentary RTS-line reset only, as every ESP32 tool performs
//!   to talk to the ROM bootloader"). Takes a [`ResetMode`] because "reset
//!   into bootloader" and "reset into application" are observably different
//!   device-changing operations with different risk/verification
//!   consequences, not one undifferentiated "reset".
//! - [`Esp32Transport::write_flash`]: one bounded, offset-addressed flash
//!   write — mirrors the real esptool `write_flash` primitive's shape
//!   (address + bytes), not a whole-artifact "flash everything" black box,
//!   so a real implementation can chunk/retry/report progress per write
//!   without this trait needing to change.
//! - [`Esp32Transport::read_flash`]: the read-back counterpart used for
//!   post-write verification (`Docs/CONNECTOR_SPEC.md` §8 "verification
//!   plan"; §10 "digest/readback verification where available").
//!
//! No method returns a bare `bool`/`()` on failure: every method returns
//! [`Result<_, TransportError>`], and [`TransportError`] is a closed,
//! *distinguishable* set (busy/permission/disconnected/malformed/timeout/
//! other) so this connector's dispatch code can produce the specific, honest
//! `ConnectorError` `Docs/CONNECTOR_SPEC.md` §5 requires ("a structured
//! unsupported/permission/busy/error result"), not a single undifferentiated
//! failure.

use async_trait::async_trait;

/// The ROM bootloader's chip-info readback. `mac_address` is real
/// device-derived material used only as HMAC input material for
/// [`crate::connector::Esp32S3Connector`]'s `identify` verify stage — never
/// persisted raw (`loki_connectors_sdk::identity`'s
/// `raw_material_handling: "consumed_not_persisted"` contract).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipIdResponse {
    /// e.g. `"esp32s3"` — the ROM-reported chip family string.
    pub chip_type: String,
    /// e.g. `"v0.2"` — matches `Docs/CONTEXT.md` OQ-001's confirmed silicon
    /// revision.
    pub silicon_revision: String,
    /// Factory-programmed MAC address, hex-formatted (e.g.
    /// `"aa:bb:cc:dd:ee:ff"`). Sensitive device-derived material — see the
    /// module doc.
    pub mac_address: String,
}

/// The SPI flash's JEDEC ID and reported size readback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashIdResponse {
    pub manufacturer_id: u8,
    pub device_id: u16,
    pub size_bytes: u64,
}

/// Which state a [`Esp32Transport::momentary_reset`] call should leave the
/// device in. Real ESP32 boards use the same RTS/DTR toggle sequence for
/// both directions; which one happens is a matter of *timing* relative to
/// the boot-mode strapping pin (GPIO0), not a different physical action —
/// but the two are observably different device-changing operations from
/// this connector's point of view (one enters a flashable state, one runs
/// the application), so they are named distinctly here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    /// Reset while holding the device in the ROM bootloader (download mode)
    /// — required before [`Esp32Transport::write_flash`]/
    /// [`Esp32Transport::read_flash`] can run.
    EnterBootloader,
    /// Reset into the normal application boot path — used by `restart` and
    /// by `flash_artifact`'s post-write re-identification stage.
    RunApp,
}

/// A closed, distinguishable transport-failure classification. Every
/// variant carries a human-readable detail string (except [`Self::Timeout`],
/// which is inherently reason-free) so a connector's own `ConnectorError`
/// construction never has to fabricate one (`AGENTS.md` rule 1;
/// `Docs/CONNECTOR_SPEC.md` §7's "returns unavailable with reason" honesty
/// requirement, applied here to transport failures generally).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// The transport (port/USB endpoint) is already in use by another
    /// operation or process.
    Busy(String),
    /// The host denied access to the transport (e.g. insufficient OS
    /// permission on the serial device).
    PermissionDenied(String),
    /// The device is not present/responding at all (unplugged, wrong
    /// candidate, dropped mid-operation).
    Disconnected(String),
    /// The device responded, but the response did not parse as a
    /// well-formed ROM-bootloader reply (garbled bytes, wrong sync byte,
    /// checksum mismatch, unexpected length).
    Malformed(String),
    /// The transport itself detected it could not complete the exchange
    /// before the operation's cooperative deadline
    /// (`loki_connectors_sdk::operation::OperationContext::deadline`) — see
    /// `loki_connectors_sdk::outcome`'s module doc on connector-reported
    /// timeouts from within an already-started stage.
    Timeout,
    /// Any other transport-level failure not covered above.
    Other(String),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Busy(detail) => write!(f, "transport busy: {detail}"),
            TransportError::PermissionDenied(detail) => {
                write!(f, "permission denied: {detail}")
            }
            TransportError::Disconnected(detail) => write!(f, "device disconnected: {detail}"),
            TransportError::Malformed(detail) => write!(f, "malformed response: {detail}"),
            TransportError::Timeout => write!(f, "transport-level timeout"),
            TransportError::Other(detail) => write!(f, "transport error: {detail}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// The injectable ROM/bootloader transport boundary — see the module doc
/// for why each method exists and why the set is exactly these five.
///
/// `Send + Sync`: this connector (like every [`loki_connectors_sdk::Connector`])
/// must itself be `Send + Sync`, so whatever transport it owns must be too.
#[async_trait]
pub trait Esp32Transport: Send + Sync {
    /// Reads the ROM bootloader's chip-info response. Non-mutating.
    async fn query_chip_id(&mut self) -> Result<ChipIdResponse, TransportError>;

    /// Reads the SPI flash's JEDEC ID and size. Non-mutating.
    async fn query_flash_id(&mut self) -> Result<FlashIdResponse, TransportError>;

    /// Performs the momentary RTS/DTR reset sequence, leaving the device in
    /// the requested [`ResetMode`]. Device-changing (the device's running
    /// state changes), but does not itself alter flash contents.
    async fn momentary_reset(&mut self, mode: ResetMode) -> Result<(), TransportError>;

    /// Writes `data` at the given byte `offset` into flash. Requires the
    /// device to already be in [`ResetMode::EnterBootloader`] state — this
    /// trait does not enforce that precondition itself (it has no notion of
    /// "current state"); the calling connector is responsible for having
    /// called [`Esp32Transport::momentary_reset`] first, per
    /// `Docs/CONNECTOR_SPEC.md` §8's declared "execution stages" ordering.
    async fn write_flash(&mut self, offset: u32, data: &[u8]) -> Result<(), TransportError>;

    /// Reads `length` bytes of flash starting at `offset`, for post-write
    /// verification. Same bootloader-state precondition as
    /// [`Esp32Transport::write_flash`].
    async fn read_flash(&mut self, offset: u32, length: u32) -> Result<Vec<u8>, TransportError>;
}
