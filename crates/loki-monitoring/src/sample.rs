//! Telemetry sample and log record shapes — `Docs/SPEC.md` §3.5 "Telemetry
//! sample and log record":
//!
//! > Samples include source device/runtime, metric name, typed value/unit,
//! > source time, receive time, quality (`good`, `late`, `gap`, `invalid`),
//! > sequence where available, and retention class.
//! >
//! > Logs include source, level, timestamp, bounded message, schema
//! > version, correlation, redaction markers, and retention class. Logs are
//! > untrusted input.

use std::fmt;
use std::str::FromStr;
use std::time::SystemTime;

use loki_connectors_sdk::error::ValidationError;
use loki_connectors_sdk::ids::{CorrelationId, DeviceId};

use crate::validate::non_empty_bounded;

/// §3.5's closed, definitive `quality` list: `good, late, gap, invalid`.
/// Set by the caller (connector/ingestion layer), never guessed by this
/// crate from a fabricated threshold — see `Docs/graph/handoffs/MON-001-1.md`
/// for why this crate does not invent a "late" cutoff `Docs/SPEC.md` does not
/// define. This crate *does* mechanically detect sequence gaps itself (see
/// [`crate::gap`]) since that is a structural computation, not an invented
/// business rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleQuality {
    Good,
    Late,
    Gap,
    Invalid,
}

impl SampleQuality {
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            SampleQuality::Good => "good",
            SampleQuality::Late => "late",
            SampleQuality::Gap => "gap",
            SampleQuality::Invalid => "invalid",
        }
    }
}

impl fmt::Display for SampleQuality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

impl FromStr for SampleQuality {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "good" => Ok(SampleQuality::Good),
            "late" => Ok(SampleQuality::Late),
            "gap" => Ok(SampleQuality::Gap),
            "invalid" => Ok(SampleQuality::Invalid),
            other => Err(ValidationError::new(
                "quality",
                format!("unknown sample quality {other:?} (Docs/SPEC.md §3.5)"),
            )),
        }
    }
}

/// §3.5's "typed value/unit" — mirrors
/// `loki_connectors_sdk::health::ObservedValue`'s shape exactly (this crate
/// ingests from that same connector-neutral vocabulary; not duplicating an
/// independently-invented shape).
#[derive(Debug, Clone, PartialEq)]
pub struct TypedValue {
    pub value: f64,
    pub unit: String,
}

/// `retention_class` has no documented closed vocabulary anywhere in
/// `Docs/SPEC.md` or `Docs/CONNECTOR_SPEC.md` (unlike `quality`, which does)
/// — following `crates/loki-connectors-sdk/src/ids.rs`'s "no divergent copy"
/// precedent for an undocumented value set (e.g. `ConnectorId`), this is an
/// open, shape-validated string, not a fabricated closed enum.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RetentionClass(String);

impl RetentionClass {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        non_empty_bounded("retention_class", &value, 64)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RetentionClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One telemetry sample (`Docs/SPEC.md` §3.5).
#[derive(Debug, Clone, PartialEq)]
pub struct TelemetrySample {
    /// "source device/runtime" — reuses `loki-connectors-sdk`'s validated
    /// `DeviceId` rather than a raw `String` (device-neutral per this
    /// node's objective).
    pub source_id: DeviceId,
    pub metric_name: String,
    pub value: TypedValue,
    pub source_time: SystemTime,
    pub receive_time: SystemTime,
    pub quality: SampleQuality,
    /// "sequence where available" — used by [`crate::gap`] to mechanically
    /// detect missing samples per `(source_id, metric_name)`.
    pub sequence: Option<u64>,
    pub retention_class: RetentionClass,
    pub correlation_id: Option<CorrelationId>,
}

impl TelemetrySample {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_id: DeviceId,
        metric_name: impl Into<String>,
        value: f64,
        unit: impl Into<String>,
        source_time: SystemTime,
        receive_time: SystemTime,
        quality: SampleQuality,
        retention_class: RetentionClass,
    ) -> Result<Self, ValidationError> {
        let metric_name = metric_name.into();
        non_empty_bounded("metric_name", &metric_name, 128)?;
        let unit = unit.into();
        non_empty_bounded("unit", &unit, 32)?;

        Ok(Self {
            source_id,
            metric_name,
            value: TypedValue { value, unit },
            source_time,
            receive_time,
            quality,
            sequence: None,
            retention_class,
            correlation_id: None,
        })
    }

    #[must_use]
    pub fn with_sequence(mut self, sequence: u64) -> Self {
        self.sequence = Some(sequence);
        self
    }

    #[must_use]
    pub fn with_correlation_id(mut self, correlation_id: CorrelationId) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// The key [`crate::gap::SequenceGapDetector`] and
    /// [`crate::freshness::FreshnessTracker`] track per-signal state under —
    /// one source can emit several distinct metrics, each with its own
    /// sequence/freshness lineage.
    pub fn signal_key(&self) -> String {
        format!("{}::{}", self.source_id.as_str(), self.metric_name)
    }
}

/// One log record (`Docs/SPEC.md` §3.5). "Logs are untrusted input" — every
/// field here is caller-supplied and bounded, never trusted as
/// pre-validated (`AGENTS.md` rule 4).
#[derive(Debug, Clone, PartialEq)]
pub struct LogRecord {
    pub source_id: DeviceId,
    /// No documented closed vocabulary for log level (unlike sample
    /// `quality`) — open, shape-validated string, same reasoning as
    /// [`RetentionClass`].
    pub level: String,
    pub timestamp: SystemTime,
    /// "bounded message" — enforced at construction (see
    /// [`LogRecord::new`]'s bound).
    pub message: String,
    pub schema_version: String,
    pub correlation_id: Option<CorrelationId>,
    /// "redaction markers" — free-form list of field/segment names the
    /// producer has already redacted; this crate does not itself redact
    /// (same division of responsibility as
    /// `loki_connectors_sdk::error::ConnectorError` — the producer redacts,
    /// this layer records that it happened).
    pub redaction_markers: Vec<String>,
    pub retention_class: RetentionClass,
}

/// `Docs/SPEC.md` §3.5 "bounded message" — a generous but real upper bound
/// so one malformed/malicious log line cannot unboundedly grow memory or
/// storage. `AGENTS.md` rule 4 requires this be enforced, not merely typed.
pub const MAX_LOG_MESSAGE_BYTES: usize = 8192;

impl LogRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_id: DeviceId,
        level: impl Into<String>,
        timestamp: SystemTime,
        message: impl Into<String>,
        schema_version: impl Into<String>,
        retention_class: RetentionClass,
    ) -> Result<Self, ValidationError> {
        let level = level.into();
        non_empty_bounded("level", &level, 16)?;
        let message = message.into();
        non_empty_bounded("message", &message, MAX_LOG_MESSAGE_BYTES)?;
        let schema_version = schema_version.into();
        non_empty_bounded("schema_version", &schema_version, 32)?;

        Ok(Self {
            source_id,
            level,
            timestamp,
            message,
            schema_version,
            correlation_id: None,
            redaction_markers: Vec::new(),
            retention_class,
        })
    }

    #[must_use]
    pub fn with_correlation_id(mut self, correlation_id: CorrelationId) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    #[must_use]
    pub fn with_redaction_markers(mut self, markers: Vec<String>) -> Self {
        self.redaction_markers = markers;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str) -> DeviceId {
        DeviceId::new(id).unwrap()
    }

    #[test]
    fn sample_quality_round_trips() {
        for q in [
            SampleQuality::Good,
            SampleQuality::Late,
            SampleQuality::Gap,
            SampleQuality::Invalid,
        ] {
            assert_eq!(q.as_wire_str().parse::<SampleQuality>().unwrap(), q);
        }
        assert!("unknown".parse::<SampleQuality>().is_err());
    }

    #[test]
    fn telemetry_sample_rejects_empty_metric_name() {
        let err = TelemetrySample::new(
            device("dev-1"),
            "",
            1.0,
            "C",
            SystemTime::now(),
            SystemTime::now(),
            SampleQuality::Good,
            RetentionClass::new("standard").unwrap(),
        );
        assert!(err.is_err());
    }

    #[test]
    fn telemetry_sample_signal_key_combines_source_and_metric() {
        let sample = TelemetrySample::new(
            device("dev-1"),
            "temperature_c",
            21.5,
            "C",
            SystemTime::now(),
            SystemTime::now(),
            SampleQuality::Good,
            RetentionClass::new("standard").unwrap(),
        )
        .unwrap();
        assert_eq!(sample.signal_key(), "dev-1::temperature_c");
    }

    #[test]
    fn log_record_rejects_oversized_message() {
        let too_big = "x".repeat(MAX_LOG_MESSAGE_BYTES + 1);
        let err = LogRecord::new(
            device("dev-1"),
            "info",
            SystemTime::now(),
            too_big,
            "1",
            RetentionClass::new("standard").unwrap(),
        );
        assert!(err.is_err());
    }

    #[test]
    fn log_record_accepts_bounded_message() {
        let ok = LogRecord::new(
            device("dev-1"),
            "info",
            SystemTime::now(),
            "boot complete",
            "1",
            RetentionClass::new("standard").unwrap(),
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn retention_class_rejects_empty() {
        assert!(RetentionClass::new("").is_err());
        assert!(RetentionClass::new("standard").is_ok());
    }
}
