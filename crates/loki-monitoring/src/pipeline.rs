//! Ties this crate's pieces together into one ingestion pipeline:
//! [`crate::buffer::BoundedBuffer`] (Deliverable 1), [`crate::freshness`]
//! (Deliverable 2), [`crate::subscription::EventBus`] (Deliverable 3), and
//! [`crate::store::SqliteTelemetryStore`] (Deliverable 4).
//!
//! ## What survives a restart and what does not
//!
//! Only [`crate::store::SqliteTelemetryStore`] is restart-durable. The
//! in-memory ingest buffer, freshness tracker, gap detector, and event bus
//! are deliberately **not** persisted and reset on every new `Pipeline`
//! (i.e. every process restart): re-inventing "was this in-memory working
//! set fresh a moment ago" across a restart would mean either fabricating
//! state this crate cannot actually verify (`AGENTS.md` rule 1) or silently
//! treating pre-restart freshness as still valid post-restart, which
//! `Docs/SPEC.md` §4.3 explicitly forbids at the device level ("Device
//! status becomes `unknown` after its freshness window; it must not remain
//! healthy solely because the last sample was healthy"). A signal is
//! honestly [`crate::freshness::Freshness::Unknown`] to a freshly started
//! `Pipeline` until a new sample actually arrives — this is the correct,
//! honest behavior, not a gap in Deliverable 4; see
//! `Docs/graph/handoffs/MON-001-1.md` §7 for the restart test that proves
//! durable data survives while in-memory working state honestly resets.

use std::sync::Mutex;
use std::time::Duration;

use crate::buffer::{BoundedBuffer, PushOutcome};
use crate::error::MonitoringError;
use crate::freshness::{Freshness, FreshnessTracker};
use crate::gap::{GapNotice, SequenceGapDetector, SequenceOutcome};
use crate::sample::{LogRecord, TelemetrySample};
use crate::store::SqliteTelemetryStore;
use crate::subscription::{EventBus, PublishOutcome, Subscription, TelemetryEvent};

/// Full report of what happened during one [`Pipeline::ingest_sample`] call
/// — every stage's outcome is surfaced, not just "ok"/"error", so a caller
/// can honestly observe e.g. "accepted, but the buffer had to evict an
/// older reading" or "accepted, but persistence failed" rather than either
/// outcome being silently absorbed.
#[derive(Debug)]
pub struct SampleIngestReport {
    pub buffer_outcome: PushOutcome<TelemetrySample>,
    pub gap: Option<GapNotice>,
    pub publish_outcome: PublishOutcome,
    /// `None` if this pipeline has no durable store configured;
    /// `Some(Err(_))` if persistence was attempted and failed — never
    /// silently swallowed.
    pub persisted: Option<Result<(), MonitoringError>>,
}

/// The log-record counterpart of [`SampleIngestReport`] — a distinct type
/// rather than a reused/overloaded sample report, since a log record has no
/// gap-detection outcome and its own buffer eviction slot.
#[derive(Debug)]
pub struct LogIngestReport {
    pub buffer_outcome: PushOutcome<LogRecord>,
    pub publish_outcome: PublishOutcome,
    pub persisted: Option<Result<(), MonitoringError>>,
}

pub struct Pipeline {
    buffer: Mutex<BoundedBuffer<TelemetrySample>>,
    log_buffer: Mutex<BoundedBuffer<LogRecord>>,
    freshness: Mutex<FreshnessTracker>,
    gap_detector: Mutex<SequenceGapDetector>,
    bus: EventBus,
    store: Option<SqliteTelemetryStore>,
}

impl Pipeline {
    /// Builds a pipeline with bounded in-memory buffers/bus only — no
    /// durable persistence. Suitable for tests of the buffer/freshness/
    /// backpressure deliverables in isolation.
    pub fn new(
        sample_buffer_capacity: usize,
        log_buffer_capacity: usize,
        bus_capacity: usize,
    ) -> Self {
        Self {
            buffer: Mutex::new(BoundedBuffer::new(sample_buffer_capacity)),
            log_buffer: Mutex::new(BoundedBuffer::new(log_buffer_capacity)),
            freshness: Mutex::new(FreshnessTracker::new()),
            gap_detector: Mutex::new(SequenceGapDetector::new()),
            bus: EventBus::new(bus_capacity),
            store: None,
        }
    }

    /// Attaches a durable store this pipeline will persist every accepted
    /// sample/log/gap to, in addition to the in-memory bounded/bus paths.
    #[must_use]
    pub fn with_store(mut self, store: SqliteTelemetryStore) -> Self {
        self.store = Some(store);
        self
    }

    pub fn subscribe(&self) -> Subscription {
        self.bus.subscribe()
    }

    /// Ingests one sample: gap detection, freshness update, bounded buffer
    /// insert (drop-oldest on overflow), bus publish, and — if a store is
    /// attached — durable persistence.
    pub fn ingest_sample(&self, sample: TelemetrySample) -> SampleIngestReport {
        let signal_key = sample.signal_key();

        let gap = sample.sequence.and_then(|seq| {
            let mut detector = self.gap_detector.lock().unwrap_or_else(|p| p.into_inner());
            match detector.observe(&signal_key, seq, sample.receive_time) {
                SequenceOutcome::Gap(notice) => Some(notice),
                _ => None,
            }
        });
        if let Some(gap) = &gap {
            self.bus.publish(TelemetryEvent::Gap(gap.clone()));
            if let Some(store) = &self.store {
                // A failed gap persistence must not be silently dropped,
                // but it also must not stop the sample itself from being
                // ingested — logged via `tracing`, distinct from the
                // sample-level `persisted` result below.
                if let Err(err) = store.insert_gap(gap) {
                    tracing::warn!(signal_key = %signal_key, error = %err, "monitoring: failed to persist gap notice");
                }
            }
        }

        {
            let mut freshness = self.freshness.lock().unwrap_or_else(|p| p.into_inner());
            freshness.observe(signal_key.clone(), sample.receive_time);
        }

        let buffer_outcome = {
            let mut buf = self.buffer.lock().unwrap_or_else(|p| p.into_inner());
            buf.push(sample.clone())
        };
        if matches!(buffer_outcome, PushOutcome::AcceptedEvictedOldest(_)) {
            self.bus.publish(TelemetryEvent::BufferOverflow {
                signal_key: signal_key.clone(),
            });
        }

        let publish_outcome = self.bus.publish(TelemetryEvent::Sample(sample.clone()));

        let persisted = self
            .store
            .as_ref()
            .map(|store| store.insert_sample(&sample));

        SampleIngestReport {
            buffer_outcome,
            gap,
            publish_outcome,
            persisted,
        }
    }

    /// Ingests one log record: bounded buffer insert (drop-oldest on
    /// overflow), bus publish, and — if a store is attached — durable
    /// persistence. Logs carry no sequence number (`Docs/SPEC.md` §3.5), so
    /// no gap detection applies.
    pub fn ingest_log(&self, log: LogRecord) -> LogIngestReport {
        let signal_key = format!("{}::log", log.source_id.as_str());

        {
            let mut freshness = self.freshness.lock().unwrap_or_else(|p| p.into_inner());
            freshness.observe(signal_key.clone(), log.timestamp);
        }

        let buffer_outcome = {
            let mut buf = self.log_buffer.lock().unwrap_or_else(|p| p.into_inner());
            buf.push(log.clone())
        };
        if matches!(buffer_outcome, PushOutcome::AcceptedEvictedOldest(_)) {
            self.bus.publish(TelemetryEvent::BufferOverflow {
                signal_key: signal_key.clone(),
            });
        }

        let publish_outcome = self.bus.publish(TelemetryEvent::Log(log.clone()));
        let persisted = self.store.as_ref().map(|store| store.insert_log(&log));

        LogIngestReport {
            buffer_outcome,
            publish_outcome,
            persisted,
        }
    }

    /// Freshness of `signal_key` as of `now`, given `window` — see
    /// [`crate::freshness::FreshnessTracker::assess`].
    pub fn freshness(
        &self,
        signal_key: &str,
        now: std::time::SystemTime,
        window: Duration,
    ) -> Freshness {
        let freshness = self.freshness.lock().unwrap_or_else(|p| p.into_inner());
        freshness.assess(signal_key, now, window)
    }

    pub fn buffered_sample_count(&self) -> usize {
        self.buffer.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    pub fn dropped_sample_count(&self) -> u64 {
        self.buffer
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .dropped_count()
    }

    pub fn buffered_log_count(&self) -> usize {
        self.log_buffer
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .len()
    }

    pub fn dropped_log_count(&self) -> u64 {
        self.log_buffer
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .dropped_count()
    }

    pub fn store(&self) -> Option<&SqliteTelemetryStore> {
        self.store.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::sample::{RetentionClass, SampleQuality};
    use loki_connectors_sdk::ids::DeviceId;

    fn make_sample(seq: Option<u64>, t: SystemTime) -> TelemetrySample {
        let mut s = TelemetrySample::new(
            DeviceId::new("dev-1").unwrap(),
            "temperature_c",
            21.0,
            "C",
            t,
            t,
            SampleQuality::Good,
            RetentionClass::new("standard").unwrap(),
        )
        .unwrap();
        s.sequence = seq;
        s
    }

    #[test]
    fn ingest_accepts_within_capacity() {
        let pipeline = Pipeline::new(4, 4, 8);
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let report = pipeline.ingest_sample(make_sample(Some(1), t));
        assert_eq!(report.buffer_outcome, PushOutcome::Accepted);
        assert_eq!(pipeline.buffered_sample_count(), 1);
    }

    #[test]
    fn ingest_reports_gap_when_sequence_skips() {
        let pipeline = Pipeline::new(4, 4, 8);
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        pipeline.ingest_sample(make_sample(Some(1), t));
        let report = pipeline.ingest_sample(make_sample(Some(4), t));
        let gap = report.gap.expect("expected a detected gap");
        assert_eq!(gap.missing_count, 2);
    }

    #[test]
    fn buffer_overflow_evicts_oldest_and_updates_drop_counter() {
        let pipeline = Pipeline::new(2, 4, 8);
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        pipeline.ingest_sample(make_sample(Some(1), t));
        pipeline.ingest_sample(make_sample(Some(2), t));
        let report = pipeline.ingest_sample(make_sample(Some(3), t));
        assert!(matches!(
            report.buffer_outcome,
            PushOutcome::AcceptedEvictedOldest(_)
        ));
        assert_eq!(
            pipeline.buffered_sample_count(),
            2,
            "capacity bound enforced"
        );
        assert_eq!(pipeline.dropped_sample_count(), 1);
    }

    #[test]
    fn freshness_reflects_ingested_samples() {
        let pipeline = Pipeline::new(4, 4, 8);
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        pipeline.ingest_sample(make_sample(Some(1), t0));

        let fresh = pipeline.freshness(
            "dev-1::temperature_c",
            t0 + Duration::from_secs(5),
            Duration::from_secs(30),
        );
        assert!(fresh.is_fresh());

        let stale = pipeline.freshness(
            "dev-1::temperature_c",
            t0 + Duration::from_secs(3600),
            Duration::from_secs(30),
        );
        assert!(!stale.is_fresh());
    }

    #[test]
    fn a_fresh_pipeline_reports_unknown_freshness_for_a_never_seen_signal() {
        let pipeline = Pipeline::new(4, 4, 8);
        let now = SystemTime::now();
        assert_eq!(
            pipeline.freshness("dev-1::temperature_c", now, Duration::from_secs(30)),
            Freshness::Unknown
        );
    }

    #[test]
    fn ingest_sample_without_a_store_reports_no_persistence_attempt() {
        let pipeline = Pipeline::new(4, 4, 8);
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let report = pipeline.ingest_sample(make_sample(Some(1), t));
        assert!(report.persisted.is_none());
    }

    #[test]
    fn ingest_sample_with_a_store_persists_and_reports_success() {
        // Even the in-memory path shares this process-wide SQLite
        // installation with this binary's other tests that open a real
        // SqlCipher-backed store through the secure-memory arena
        // (DAT-002-MEM-CODE-001). `sqlite3_config` can only run before
        // SQLite's first implicit initialization, so every SQLite-touching
        // test in this binary — in-memory or not — must bootstrap the
        // arena first, regardless of which test happens to run first.
        loki_storage::secure_memory::test_support::ensure_test_arena_ready();
        let store = SqliteTelemetryStore::open_in_memory().unwrap();
        let pipeline = Pipeline::new(4, 4, 8).with_store(store);
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let report = pipeline.ingest_sample(make_sample(Some(1), t));
        assert!(matches!(report.persisted, Some(Ok(()))));
        assert_eq!(pipeline.store().unwrap().sample_count().unwrap(), 1);
    }
}
