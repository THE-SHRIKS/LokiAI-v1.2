//! [`Esp32S3Connector`] — the concrete `loki_connectors_sdk::Connector`
//! implementation for the confirmed ESP32-S3 profile (`Docs/CONTEXT.md`
//! OQ-001), dispatching every capability's `preflight`/`execute`/`verify`/
//! `recover` stage onto the injected [`Esp32Transport`] — see that module's
//! doc for why the trait is shaped the way it is, and this node's task
//! packet (`Docs/graph/tasks/CON-003-R1.md`) Section 6/7 for the exact
//! per-capability contract this file implements.
//!
//! ## `rollback`'s honest policy gap (open question 2 of 3 — not resolved
//! silently)
//!
//! `Docs/CONNECTOR_SPEC.md` §10 lists "recovery access and known-good
//! artifact policy" as an unresolved pre-implementation confirmation item;
//! `Docs/CONTEXT.md` OQ-001 did not resolve it. [`Esp32S3Connector`]'s
//! `rollback` dispatch therefore never reaches the transport at all: its
//! `preflight` always fails with an explicit `recovery_policy_undecided`
//! reason (see [`Esp32S3Connector::dispatch_rollback_preflight`]) rather
//! than fabricating a rollback this connector has no known-good artifact or
//! recovery-access policy to perform (`AGENTS.md` rule 1). `flash_artifact`
//! and `restart`, by contrast, do have a meaningful (if best-effort,
//! non-guaranteed) `recover()` action — see each dispatch function's own
//! doc comment for why the two are treated differently.
//!
//! ## `identify`'s `IdentityConfidence` (open question 3 of 3 — this node's
//! own documented choice, not inherited)
//!
//! [`Esp32S3Connector::dispatch_identify_verify`] reports
//! [`IdentityConfidence::Moderate`] for the chip-ID evidence it attaches.
//! This is a deliberate, documented judgment call, not a default: a real
//! ESP32 ROM chip-ID query (factory MAC + silicon revision) is
//! device-derived and not trivially spoofable without physical chip access,
//! which would normally justify `Strong` — but this node only proves that
//! *structure* against a **fake** transport (this node's hard forbidden
//! scope: zero real device I/O). No physical evidence yet exists that the
//! real transport implementation (`CON-003-PHYS-001`) will actually observe
//! chip-ID material with that same strength in practice (transport noise,
//! partial reads, etc.), so this connector reports `Moderate` rather than
//! `Strong` until real hardware evidence justifies raising it. This is
//! recorded here and in the worker handoff precisely so a future node does
//! not read `Moderate` as an inherited, already-settled policy.

use std::time::SystemTime;

use async_trait::async_trait;

use loki_connectors_sdk::Connector;
use loki_connectors_sdk::capability::CapabilityDescriptor;
use loki_connectors_sdk::error::{ConnectorError, ValidationError};
use loki_connectors_sdk::health::{HealthCheckEntry, HealthReport};
use loki_connectors_sdk::identity::{
    DeviceIdentityEvidence, DeviceIdentityEvidenceInput, IdentityConfidence,
};
use loki_connectors_sdk::manifest::ConnectorManifest;
use loki_connectors_sdk::operation::{OperationContext, OperationRequest, OperationStage};
use loki_connectors_sdk::outcome::{StageEvidence, StageFailure, StageOutcome, StageTimeout};
use loki_connectors_sdk::vocabulary::HealthCheckStatus;

use crate::capabilities::build_capabilities;
use crate::clock::now_iso8601;
use crate::manifest::{CONNECTOR_ID, CONNECTOR_VERSION, build_manifest};
use crate::transport::{ChipIdResponse, Esp32Transport, ResetMode, TransportError};

/// Confirmed external flash size, `Docs/CONTEXT.md` OQ-001: "16MB external
/// quad flash".
const EXPECTED_FLASH_SIZE_BYTES: u64 = 16 * 1024 * 1024;

/// The concrete ESP32-S3 connector. Generic over no transport type
/// parameter deliberately: it owns a `Box<dyn Esp32Transport>` so exactly
/// one connector type exists regardless of whether it is constructed with
/// this crate's own fake (tests only) or a future real implementation
/// (`CON-003-PHYS-001`) — the injection point is [`Esp32S3Connector::new`],
/// not the type itself.
pub struct Esp32S3Connector {
    manifest: ConnectorManifest,
    capabilities: Vec<CapabilityDescriptor>,
    transport: Box<dyn Esp32Transport>,
    /// Opaque per-installation secret (`Docs/ARCHITECTURE.md`: "a
    /// per-installation secret stored in OS-protected storage"). This
    /// connector does not generate, persist, or load it — see
    /// `loki_connectors_sdk::identity`'s module doc — it only receives it at
    /// construction, exactly like `transport` above.
    installation_key: Vec<u8>,
    /// Scratch state: the chip-id response `identify`'s own `preflight`
    /// observed, compared against `verify`'s re-query of the same value.
    /// `None` whenever no `identify` preflight has run yet in this
    /// connector's lifetime, or the operation being dispatched is not
    /// `identify`.
    last_identify_preflight_chip_id: Option<ChipIdResponse>,
    /// Scratch state: the `(offset, bytes)` `flash_artifact`'s own `execute`
    /// actually wrote, compared against `verify`'s read-back. `None`
    /// whenever no `flash_artifact` execute has completed yet.
    last_flash_write: Option<(u32, Vec<u8>)>,
}

impl Esp32S3Connector {
    /// Constructs a connector over the given transport and installation key.
    /// `transport` is the sole device-I/O boundary this connector ever
    /// calls through — see `src/transport.rs`'s module doc.
    pub fn new(
        transport: Box<dyn Esp32Transport>,
        installation_key: Vec<u8>,
    ) -> Result<Self, ValidationError> {
        let capabilities = build_capabilities();
        let manifest = build_manifest(capabilities.clone())?;
        Ok(Self {
            manifest,
            capabilities,
            transport,
            installation_key,
            last_identify_preflight_chip_id: None,
            last_flash_write: None,
        })
    }
}

// --- Shared helpers -------------------------------------------------------

/// Builds a [`ConnectorError`] with a guaranteed-valid, non-empty message —
/// `ConnectorError::new` itself rejects an empty message, and this
/// connector's own transport-error detail strings are already non-empty by
/// construction, but this helper stays defensive rather than `.expect`-ing
/// that invariant at every call site.
fn safe_connector_error(code: &'static str, message: impl Into<String>) -> ConnectorError {
    let message = message.into();
    let message = if message.trim().is_empty() {
        "no further detail available".to_string()
    } else {
        message
    };
    ConnectorError::new(code, message)
        .expect("code is a fixed non-empty literal; message defaulted when empty")
}

fn success(summary: impl Into<String>) -> StageOutcome {
    let mut summary = summary.into();
    // `StageEvidence::new` bounds `summary` to 2000 bytes; truncate rather
    // than fail an otherwise-successful stage over a formatting overflow.
    if summary.len() > 2000 {
        summary.truncate(2000);
    }
    StageOutcome::Success(
        StageEvidence::new(summary, SystemTime::now()).expect("bounded, non-empty summary"),
    )
}

/// Converts a transport failure into the matching [`StageOutcome`] —
/// [`TransportError::Timeout`] becomes a real
/// [`StageOutcome::Timeout`] (this connector's own cooperative
/// "the transport told me it couldn't finish" report, per
/// `loki_connectors_sdk::outcome`'s module doc), every other variant a
/// classified, distinguishable [`StageOutcome::Failure`].
fn stage_outcome_from_transport_error(
    stage: OperationStage,
    ctx: &OperationContext,
    error: TransportError,
) -> StageOutcome {
    match error {
        TransportError::Timeout => StageOutcome::Timeout(StageTimeout {
            stage,
            observed_at: SystemTime::now(),
            deadline: ctx.deadline.unwrap_or_else(SystemTime::now),
        }),
        TransportError::Busy(detail) => {
            StageOutcome::Failure(safe_connector_error("transport_busy", detail).retryable())
        }
        TransportError::PermissionDenied(detail) => {
            StageOutcome::Failure(safe_connector_error("permission_denied", detail))
        }
        TransportError::Disconnected(detail) => {
            StageOutcome::Failure(safe_connector_error("device_disconnected", detail).retryable())
        }
        TransportError::Malformed(detail) => {
            StageOutcome::Failure(safe_connector_error("malformed_response", detail))
        }
        TransportError::Other(detail) => {
            StageOutcome::Failure(safe_connector_error("transport_error", detail))
        }
    }
}

/// `Docs/CONNECTOR_SPEC.md` §4: a connector "must not ... change undeclared
/// targets or operations." This connector's `preflight`/`execute`/`verify`/
/// `recover` dispatch is a closed match over its own six declared
/// capability ids; anything else (a caller bug, or a future capability this
/// connector has not yet implemented) fails honestly here instead of
/// panicking or silently doing nothing.
fn unsupported_capability_outcome(capability_id: &str) -> StageOutcome {
    StageOutcome::Failure(safe_connector_error(
        "unsupported_capability",
        format!("this connector does not implement dispatch for capability '{capability_id}'"),
    ))
}

fn health_status_for_transport_error(error: &TransportError) -> (HealthCheckStatus, &'static str) {
    match error {
        TransportError::Busy(_) => (HealthCheckStatus::Unavailable, "transport_busy"),
        TransportError::PermissionDenied(_) => {
            (HealthCheckStatus::Unavailable, "permission_denied")
        }
        TransportError::Disconnected(_) => (HealthCheckStatus::Unavailable, "device_disconnected"),
        // The device responded, just not with a well-formed message — that
        // is itself real (if bad) evidence, not an absence of evidence, so
        // this is `Failed`, not `Unavailable`.
        TransportError::Malformed(_) => (HealthCheckStatus::Failed, "malformed_response"),
        TransportError::Timeout => (HealthCheckStatus::Interrupted, "transport_timeout"),
        TransportError::Other(_) => (HealthCheckStatus::Unavailable, "transport_error"),
    }
}

#[async_trait]
impl Connector for Esp32S3Connector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn capabilities(&self) -> &[CapabilityDescriptor] {
        &self.capabilities
    }

    async fn health_check(&mut self, _ctx: &OperationContext) -> HealthReport {
        let now = SystemTime::now();
        let mut entries = Vec::with_capacity(4);

        match self.transport.query_chip_id().await {
            Ok(chip) => entries.push(
                HealthCheckEntry::new(
                    "esp32s3.chip_reachability",
                    "1",
                    "connectivity",
                    HealthCheckStatus::Passed,
                    None,
                    now,
                    now,
                )
                .expect("Passed requires no detail")
                .with_evidence_ref(format!(
                    "chip_type={},silicon_revision={}",
                    chip.chip_type, chip.silicon_revision
                )),
            ),
            Err(error) => {
                let (status, code) = health_status_for_transport_error(&error);
                entries.push(
                    HealthCheckEntry::new(
                        "esp32s3.chip_reachability",
                        "1",
                        "connectivity",
                        status,
                        Some(format!("{code}: {error}")),
                        now,
                        now,
                    )
                    .expect("non-empty detail supplied"),
                );
            }
        }

        match self.transport.query_flash_id().await {
            Ok(flash) => {
                let matches_confirmed_profile = flash.size_bytes == EXPECTED_FLASH_SIZE_BYTES;
                let status = if matches_confirmed_profile {
                    HealthCheckStatus::Passed
                } else {
                    HealthCheckStatus::Warning
                };
                let detail = if matches_confirmed_profile {
                    None
                } else {
                    Some(format!(
                        "reported flash size {} bytes does not match the confirmed \
                         profile ({EXPECTED_FLASH_SIZE_BYTES} bytes, Docs/CONTEXT.md OQ-001)",
                        flash.size_bytes
                    ))
                };
                entries.push(
                    HealthCheckEntry::new(
                        "esp32s3.flash_geometry",
                        "1",
                        "storage",
                        status,
                        detail,
                        now,
                        now,
                    )
                    .expect("Warning carries detail; Passed carries none")
                    .with_observed_value(flash.size_bytes as f64, "bytes"),
                );
            }
            Err(error) => {
                let (status, code) = health_status_for_transport_error(&error);
                entries.push(
                    HealthCheckEntry::new(
                        "esp32s3.flash_geometry",
                        "1",
                        "storage",
                        status,
                        Some(format!("{code}: {error}")),
                        now,
                        now,
                    )
                    .expect("non-empty detail supplied"),
                );
            }
        }

        // `Docs/CONNECTOR_SPEC.md` §7: "A connector cannot infer voltage,
        // thermal, memory, firmware, or bus health when it has no evidence
        // source. It returns unavailable with reason." `Esp32Transport`
        // deliberately has no voltage/thermal query (this node's packet
        // scopes the trait to exactly chip-ID/flash-ID/reset/write/read) —
        // so these two checks are honestly `Unavailable`, never fabricated.
        entries.push(
            HealthCheckEntry::new(
                "esp32s3.power_voltage",
                "1",
                "power",
                HealthCheckStatus::Unavailable,
                Some(
                    "no onboard voltage telemetry exposed by the ESP32-S3 USB-Serial/JTAG \
                     transport (Docs/CONNECTOR_SPEC.md §7)"
                        .to_string(),
                ),
                now,
                now,
            )
            .expect("non-empty detail supplied"),
        );
        entries.push(
            HealthCheckEntry::new(
                "esp32s3.thermal",
                "1",
                "thermal",
                HealthCheckStatus::Unavailable,
                Some(
                    "no onboard thermal telemetry exposed by the ESP32-S3 USB-Serial/JTAG \
                     transport (Docs/CONNECTOR_SPEC.md §7)"
                        .to_string(),
                ),
                now,
                now,
            )
            .expect("non-empty detail supplied"),
        );

        HealthReport::new(entries, now)
    }

    async fn preflight(&mut self, op: &OperationRequest, ctx: &OperationContext) -> StageOutcome {
        match op.capability_id.as_str() {
            "identify" => self.dispatch_identify_preflight(ctx).await,
            "read_health" => self.dispatch_read_health_preflight(ctx).await,
            "restart" => self.dispatch_restart_preflight(ctx).await,
            "flash_artifact" => self.dispatch_flash_artifact_preflight(op, ctx).await,
            "verify_artifact" => self.dispatch_verify_artifact_preflight(op, ctx).await,
            "rollback" => self.dispatch_rollback_preflight(),
            other => unsupported_capability_outcome(other),
        }
    }

    async fn execute(&mut self, op: &OperationRequest, ctx: &OperationContext) -> StageOutcome {
        match op.capability_id.as_str() {
            "identify" => self.dispatch_identify_execute(ctx).await,
            "read_health" => self.dispatch_read_health_execute(ctx).await,
            "restart" => self.dispatch_restart_execute(ctx).await,
            "flash_artifact" => self.dispatch_flash_artifact_execute(op, ctx).await,
            "verify_artifact" => self.dispatch_verify_artifact_execute(op, ctx).await,
            "rollback" => self.dispatch_rollback_preflight(),
            other => unsupported_capability_outcome(other),
        }
    }

    async fn verify(&mut self, op: &OperationRequest, ctx: &OperationContext) -> StageOutcome {
        match op.capability_id.as_str() {
            "identify" => self.dispatch_identify_verify(ctx).await,
            "read_health" => self.dispatch_read_health_verify(ctx).await,
            "restart" => self.dispatch_restart_verify(ctx).await,
            "flash_artifact" => self.dispatch_flash_artifact_verify(op, ctx).await,
            "verify_artifact" => self.dispatch_verify_artifact_verify(op, ctx).await,
            "rollback" => self.dispatch_rollback_preflight(),
            other => unsupported_capability_outcome(other),
        }
    }

    async fn recover(
        &mut self,
        op: &OperationRequest,
        ctx: &OperationContext,
        _failure: &StageFailure,
    ) -> StageOutcome {
        match op.capability_id.as_str() {
            "identify" => StageOutcome::Failure(safe_connector_error(
                "no_recovery_applicable",
                "identify has no recovery mechanism; a failed identification carries nothing \
                 to roll back",
            )),
            "read_health" => StageOutcome::Failure(safe_connector_error(
                "no_recovery_applicable",
                "read_health is read-only; a failed health read carries nothing to roll back",
            )),
            "restart" => self.dispatch_restart_recover(ctx).await,
            "flash_artifact" => self.dispatch_flash_artifact_recover(ctx).await,
            "verify_artifact" => StageOutcome::Failure(safe_connector_error(
                "no_recovery_applicable",
                "verify_artifact is read-only; a failed verification carries nothing to roll \
                 back",
            )),
            "rollback" => StageOutcome::Failure(safe_connector_error(
                "no_recovery_applicable",
                "rollback itself has no further recovery mechanism behind it — see this \
                 connector's own recovery-policy-gap failure at the preflight stage",
            )),
            other => unsupported_capability_outcome(other),
        }
    }
}

// --- identify ---------------------------------------------------------

impl Esp32S3Connector {
    async fn dispatch_identify_preflight(&mut self, ctx: &OperationContext) -> StageOutcome {
        match self.transport.query_chip_id().await {
            Ok(chip) => {
                let summary = format!(
                    "chip id query ok: chip_type={}, silicon_revision={}",
                    chip.chip_type, chip.silicon_revision
                );
                self.last_identify_preflight_chip_id = Some(chip);
                success(summary)
            }
            Err(error) => {
                self.last_identify_preflight_chip_id = None;
                stage_outcome_from_transport_error(OperationStage::Preflight, ctx, error)
            }
        }
    }

    async fn dispatch_identify_execute(&mut self, ctx: &OperationContext) -> StageOutcome {
        match self.transport.query_flash_id().await {
            Ok(flash) => success(format!(
                "flash id query ok: manufacturer_id={:#04x}, device_id={:#06x}, size_bytes={}",
                flash.manufacturer_id, flash.device_id, flash.size_bytes
            )),
            Err(error) => stage_outcome_from_transport_error(OperationStage::Execute, ctx, error),
        }
    }

    /// Re-queries the chip id (§8 "post-operation re-identification"),
    /// confirms it matches what `preflight` observed, and — only on a match
    /// — attaches [`DeviceIdentityEvidence`] built from the fake transport's
    /// response. See this module's doc comment for the `IdentityConfidence`
    /// choice.
    async fn dispatch_identify_verify(&mut self, ctx: &OperationContext) -> StageOutcome {
        let Some(previous) = self.last_identify_preflight_chip_id.clone() else {
            return StageOutcome::Failure(safe_connector_error(
                "identify_state_missing",
                "verify ran without a prior successful preflight chip-id query",
            ));
        };

        let chip = match self.transport.query_chip_id().await {
            Ok(chip) => chip,
            Err(error) => {
                return stage_outcome_from_transport_error(OperationStage::Verify, ctx, error);
            }
        };

        if chip != previous {
            return StageOutcome::Failure(safe_connector_error(
                "identity_mismatch",
                format!("chip id changed between preflight and verify: {previous:?} vs {chip:?}"),
            ));
        }

        let fingerprint_material = format!(
            "{}:{}:{}",
            chip.chip_type, chip.silicon_revision, chip.mac_address
        );
        let evidence = DeviceIdentityEvidence::new(DeviceIdentityEvidenceInput {
            evidence_type: "esp32_rom_chip_id".to_string(),
            fingerprint_material,
            installation_key: &self.installation_key,
            confidence: IdentityConfidence::Moderate,
            connector_id: CONNECTOR_ID.to_string(),
            connector_version: CONNECTOR_VERSION.to_string(),
            observed_at: now_iso8601(),
        });

        match evidence {
            Ok(evidence) => {
                let stage_evidence = StageEvidence::new(
                    format!(
                        "re-identified chip: chip_type={}, silicon_revision={}",
                        chip.chip_type, chip.silicon_revision
                    ),
                    SystemTime::now(),
                )
                .expect("bounded, non-empty summary")
                .with_identity_evidence(evidence);
                StageOutcome::Success(stage_evidence)
            }
            Err(validation_error) => StageOutcome::Failure(safe_connector_error(
                "identity_evidence_construction_failed",
                validation_error.to_string(),
            )),
        }
    }
}

// --- read_health --------------------------------------------------------

impl Esp32S3Connector {
    async fn dispatch_read_health_preflight(&mut self, ctx: &OperationContext) -> StageOutcome {
        match self.transport.query_chip_id().await {
            Ok(_) => success("device reachable"),
            Err(error) => stage_outcome_from_transport_error(OperationStage::Preflight, ctx, error),
        }
    }

    async fn dispatch_read_health_execute(&mut self, ctx: &OperationContext) -> StageOutcome {
        match self.transport.query_flash_id().await {
            Ok(flash) => success(format!(
                "gathered flash geometry evidence: size_bytes={}",
                flash.size_bytes
            )),
            Err(error) => stage_outcome_from_transport_error(OperationStage::Execute, ctx, error),
        }
    }

    async fn dispatch_read_health_verify(&mut self, ctx: &OperationContext) -> StageOutcome {
        match self.transport.query_chip_id().await {
            Ok(_) => success("device still reachable after health read"),
            Err(error) => stage_outcome_from_transport_error(OperationStage::Verify, ctx, error),
        }
    }
}

// --- restart -------------------------------------------------------------

impl Esp32S3Connector {
    async fn dispatch_restart_preflight(&mut self, ctx: &OperationContext) -> StageOutcome {
        match self.transport.query_chip_id().await {
            Ok(_) => success("device present and reachable before restart"),
            Err(error) => stage_outcome_from_transport_error(OperationStage::Preflight, ctx, error),
        }
    }

    async fn dispatch_restart_execute(&mut self, ctx: &OperationContext) -> StageOutcome {
        match self.transport.momentary_reset(ResetMode::RunApp).await {
            Ok(()) => success("momentary reset issued (run-app mode)"),
            Err(error) => stage_outcome_from_transport_error(OperationStage::Execute, ctx, error),
        }
    }

    async fn dispatch_restart_verify(&mut self, ctx: &OperationContext) -> StageOutcome {
        match self.transport.query_chip_id().await {
            Ok(chip) => success(format!(
                "post-restart re-identification ok: chip_type={}, silicon_revision={}",
                chip.chip_type, chip.silicon_revision
            )),
            Err(error) => stage_outcome_from_transport_error(OperationStage::Verify, ctx, error),
        }
    }

    /// A real, best-effort recovery attempt: try the reset once more, then
    /// confirm the device responds. This is honest evidence-gathering (a
    /// real, if fake-transport, call), not a fabricated success — if the
    /// retry also fails, this reports [`StageOutcome::Failure`], never
    /// [`StageOutcome::Success`].
    async fn dispatch_restart_recover(&mut self, ctx: &OperationContext) -> StageOutcome {
        if let Err(error) = self.transport.momentary_reset(ResetMode::RunApp).await {
            return stage_outcome_from_transport_error(OperationStage::Recover, ctx, error);
        }
        match self.transport.query_chip_id().await {
            Ok(chip) => success(format!(
                "recovery re-reset succeeded; device responded: chip_type={}",
                chip.chip_type
            )),
            Err(error) => stage_outcome_from_transport_error(OperationStage::Recover, ctx, error),
        }
    }
}

// --- flash_artifact --------------------------------------------------------

impl Esp32S3Connector {
    async fn dispatch_flash_artifact_preflight(
        &mut self,
        op: &OperationRequest,
        ctx: &OperationContext,
    ) -> StageOutcome {
        if op.canonical_inputs.is_empty() {
            return StageOutcome::Failure(safe_connector_error(
                "malformed_request",
                "flash_artifact requires non-empty canonical_inputs (the firmware image bytes)",
            ));
        }
        let flash = match self.transport.query_flash_id().await {
            Ok(flash) => flash,
            Err(error) => {
                return stage_outcome_from_transport_error(OperationStage::Preflight, ctx, error);
            }
        };
        if (op.canonical_inputs.len() as u64) > flash.size_bytes {
            return StageOutcome::Failure(safe_connector_error(
                "artifact_too_large_for_flash",
                format!(
                    "artifact is {} bytes; flash reports only {} bytes available",
                    op.canonical_inputs.len(),
                    flash.size_bytes
                ),
            ));
        }
        if let Err(error) = self.transport.query_chip_id().await {
            return stage_outcome_from_transport_error(OperationStage::Preflight, ctx, error);
        }
        success(format!(
            "preflight ok: artifact_bytes={}, flash_size_bytes={}",
            op.canonical_inputs.len(),
            flash.size_bytes
        ))
    }

    async fn dispatch_flash_artifact_execute(
        &mut self,
        op: &OperationRequest,
        ctx: &OperationContext,
    ) -> StageOutcome {
        if let Err(error) = self
            .transport
            .momentary_reset(ResetMode::EnterBootloader)
            .await
        {
            return stage_outcome_from_transport_error(OperationStage::Execute, ctx, error);
        }
        if let Err(error) = self.transport.write_flash(0, &op.canonical_inputs).await {
            self.last_flash_write = None;
            return stage_outcome_from_transport_error(OperationStage::Execute, ctx, error);
        }
        self.last_flash_write = Some((0, op.canonical_inputs.clone()));
        success(format!(
            "wrote {} bytes at offset 0",
            op.canonical_inputs.len()
        ))
    }

    /// §8 "verification plan": read back exactly what was written and
    /// compare, then §10's "post-flash restart/re-identification" — reset
    /// back into application mode and re-identify. All four must succeed
    /// for `flash_artifact` to be considered verified.
    async fn dispatch_flash_artifact_verify(
        &mut self,
        _op: &OperationRequest,
        ctx: &OperationContext,
    ) -> StageOutcome {
        let Some((offset, written)) = self.last_flash_write.clone() else {
            return StageOutcome::Failure(safe_connector_error(
                "flash_state_missing",
                "verify ran without a prior successful flash write",
            ));
        };

        let read_back = match self
            .transport
            .read_flash(offset, written.len() as u32)
            .await
        {
            Ok(bytes) => bytes,
            Err(error) => {
                return stage_outcome_from_transport_error(OperationStage::Verify, ctx, error);
            }
        };
        if read_back != written {
            return StageOutcome::Failure(safe_connector_error(
                "verification_mismatch",
                format!(
                    "flash readback did not match written artifact ({} bytes written, {} bytes \
                     read back)",
                    written.len(),
                    read_back.len()
                ),
            ));
        }

        if let Err(error) = self.transport.momentary_reset(ResetMode::RunApp).await {
            return stage_outcome_from_transport_error(OperationStage::Verify, ctx, error);
        }
        let chip = match self.transport.query_chip_id().await {
            Ok(chip) => chip,
            Err(error) => {
                return stage_outcome_from_transport_error(OperationStage::Verify, ctx, error);
            }
        };

        success(format!(
            "flash verified: {} bytes matched readback; device re-identified after boot: \
             chip_type={}",
            written.len(),
            chip.chip_type
        ))
    }

    /// No known-good-artifact/recovery-access policy exists (open question 2
    /// — see this module's doc comment): this attempts one best-effort
    /// action (reset back toward application mode and check the device
    /// responds) purely as diagnostic evidence, but always reports
    /// [`StageOutcome::Failure`] — never a fabricated recovery success —
    /// because there is no policy under which this connector could honestly
    /// claim the device was actually restored to a known-good state.
    async fn dispatch_flash_artifact_recover(&mut self, ctx: &OperationContext) -> StageOutcome {
        let reset_result = self.transport.momentary_reset(ResetMode::RunApp).await;
        let reachability = match reset_result {
            Ok(()) => match self.transport.query_chip_id().await {
                Ok(chip) => format!("device responded after reset: chip_type={}", chip.chip_type),
                Err(error) => format!("device did not respond after reset: {error}"),
            },
            Err(error) => format!("best-effort recovery reset itself failed: {error}"),
        };
        let _ = ctx; // no cooperative deadline check needed for this best-effort diagnostic step
        StageOutcome::Failure(safe_connector_error(
            "recovery_policy_undecided",
            format!(
                "flash verification failed; attempted a best-effort reset ({reachability}), but \
                 no known-good-artifact rollback policy exists (Docs/CONNECTOR_SPEC.md §10) — \
                 manual recovery/reflash required"
            ),
        ))
    }
}

// --- verify_artifact --------------------------------------------------------

impl Esp32S3Connector {
    async fn dispatch_verify_artifact_preflight(
        &mut self,
        op: &OperationRequest,
        ctx: &OperationContext,
    ) -> StageOutcome {
        if op.canonical_inputs.is_empty() {
            return StageOutcome::Failure(safe_connector_error(
                "malformed_request",
                "verify_artifact requires non-empty canonical_inputs (the expected bytes)",
            ));
        }
        match self.transport.query_chip_id().await {
            Ok(_) => success("device reachable before artifact verification"),
            Err(error) => stage_outcome_from_transport_error(OperationStage::Preflight, ctx, error),
        }
    }

    async fn dispatch_verify_artifact_execute(
        &mut self,
        op: &OperationRequest,
        ctx: &OperationContext,
    ) -> StageOutcome {
        let expected = &op.canonical_inputs;
        let read_back = match self.transport.read_flash(0, expected.len() as u32).await {
            Ok(bytes) => bytes,
            Err(error) => {
                return stage_outcome_from_transport_error(OperationStage::Execute, ctx, error);
            }
        };
        if &read_back != expected {
            return StageOutcome::Failure(safe_connector_error(
                "artifact_mismatch",
                format!(
                    "first read did not match expected artifact ({} bytes expected, {} bytes \
                     read)",
                    expected.len(),
                    read_back.len()
                ),
            ));
        }
        success(format!(
            "first read matched expected artifact ({} bytes)",
            expected.len()
        ))
    }

    /// A second, independent read — distinct from `execute`'s own read —
    /// guarding against a fluky/cached first result, matching
    /// `Docs/CONNECTOR_SPEC.md` §8's "verification plan" being an
    /// independent stage, not a repeat of `execute`'s own success.
    async fn dispatch_verify_artifact_verify(
        &mut self,
        op: &OperationRequest,
        ctx: &OperationContext,
    ) -> StageOutcome {
        let expected = &op.canonical_inputs;
        let read_back = match self.transport.read_flash(0, expected.len() as u32).await {
            Ok(bytes) => bytes,
            Err(error) => {
                return stage_outcome_from_transport_error(OperationStage::Verify, ctx, error);
            }
        };
        if &read_back != expected {
            return StageOutcome::Failure(safe_connector_error(
                "artifact_mismatch",
                format!(
                    "second (independent) read did not match expected artifact ({} bytes \
                     expected, {} bytes read)",
                    expected.len(),
                    read_back.len()
                ),
            ));
        }
        success(format!(
            "second, independent read confirmed the artifact ({} bytes)",
            expected.len()
        ))
    }
}

// --- rollback --------------------------------------------------------------

impl Esp32S3Connector {
    /// Always honestly fails — see this module's doc comment ("`rollback`'s
    /// honest policy gap"). Never touches `self.transport`: this connector
    /// has no legitimate device-changing action to attempt without a
    /// known-good-artifact/recovery-access policy, so it must not even look
    /// like it tried one. Reused for `execute`/`verify` too (both would be
    /// unreachable via `loki_connectors_sdk::run_operation`, since
    /// `preflight` failing here short-circuits them — see
    /// `crates/loki-connectors-sdk/src/lifecycle.rs` — but each is still
    /// individually defensive against direct/out-of-order invocation).
    fn dispatch_rollback_preflight(&self) -> StageOutcome {
        StageOutcome::Failure(safe_connector_error(
            "recovery_policy_undecided",
            "rollback cannot run: no known-good-artifact/recovery-access policy has been \
             decided (Docs/CONNECTOR_SPEC.md §10; Docs/graph/tasks/CON-003-R1.md Section 9 \
             open question 2)",
        ))
    }
}
