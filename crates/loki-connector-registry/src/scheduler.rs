//! Periodic health-check scheduling — the "periodic" half of Deliverable 3
//! ("periodic health-check invocation"). [`crate::registry::ConnectorRegistry::run_health_cycle`]
//! is the "run it once" primitive; this module is the only place that
//! repeats it on an interval, so the repetition policy (interval, stop
//! behavior) lives in exactly one spot.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use loki_connectors_sdk::ids::CorrelationId;
use loki_connectors_sdk::operation::OperationContext;
use tokio::task::JoinHandle;

use crate::registry::ConnectorRegistry;

/// A monotonically increasing counter giving each scheduled cycle a
/// distinct correlation id — not wall-clock- or elapsed-time-derived, so it
/// can never collide even across cycles that complete faster than the
/// clock's resolution.
static CYCLE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A running periodic health-check task. Dropping this handle does **not**
/// stop the task (matching `tokio::spawn`'s own semantics — the task is
/// detached); call [`PeriodicHealthChecker::stop`] to actually cancel and
/// wait for it to finish its current cycle.
pub struct PeriodicHealthChecker {
    handle: JoinHandle<()>,
}

impl PeriodicHealthChecker {
    /// Spawns a task that calls
    /// [`ConnectorRegistry::run_health_cycle`] once per `interval`, forever,
    /// until [`PeriodicHealthChecker::stop`] is called. Each cycle gets its
    /// own fresh `OperationContext` with a distinct correlation id (per
    /// `AGENTS.md` rule 5) — never one long-lived context reused across
    /// unrelated cycles.
    #[must_use]
    pub fn spawn(registry: Arc<ConnectorRegistry>, interval: Duration) -> Self {
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // The first tick fires immediately; that is the desired
            // behavior here (run a cycle right away, not only after the
            // first full interval has elapsed).
            loop {
                ticker.tick().await;
                let sequence = CYCLE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
                let correlation_id = CorrelationId::new(format!("health-cycle-{sequence}"))
                    .expect("counter-derived id stays within the SDK's length bound");
                let ctx = OperationContext::new(correlation_id);
                registry.run_health_cycle(&ctx).await;
            }
        });
        Self { handle }
    }

    /// Cancels the periodic task and waits for it to actually stop. Safe to
    /// call even if a cycle is currently in flight — the running cycle is
    /// aborted, not awaited to completion, since this crate's cancellation
    /// model is "stop scheduling further cycles now", not "let the current
    /// one run to completion".
    pub async fn stop(self) {
        self.handle.abort();
        // `abort()` on a task blocked awaiting `ticker.tick()` completes
        // immediately; a task partway through `run_health_cycle` is
        // cancelled at its next await point. Either way, awaiting the
        // (now-aborted) handle here just drains the resulting `JoinError`
        // rather than leaving a detached task behind.
        let _ = self.handle.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestConnector;

    #[tokio::test(flavor = "multi_thread")]
    async fn periodic_checker_runs_at_least_one_cycle_and_stops_cleanly() {
        let registry = Arc::new(ConnectorRegistry::new());
        let id = registry
            .register(Box::new(TestConnector::identifiable("conn-a")))
            .await
            .unwrap();
        assert!(registry.health_status(&id).await.is_none());

        let checker =
            PeriodicHealthChecker::spawn(Arc::clone(&registry), Duration::from_millis(10));
        // The first tick fires immediately; give the spawned task a moment
        // to actually run before asserting on its effect.
        tokio::time::sleep(Duration::from_millis(100)).await;
        checker.stop().await;

        assert!(
            registry.health_status(&id).await.is_some(),
            "at least one health cycle must have run before stop() returned"
        );
    }
}
