//! Subscription/backpressure mechanism for later consumers (`MON-002`, UI)
//! — `Docs/graph/tasks/MON-001.md` Deliverable 3: "a slow consumer does not
//! crash or unboundedly memory-balloon the producer side", and the
//! objective's "a subscription mechanism later nodes ... will consume."
//!
//! Built on `tokio::sync::broadcast`: a fixed-capacity ring buffer shared
//! across every subscriber. `publish` is synchronous and never blocks or
//! grows past that fixed capacity, regardless of how many subscribers exist
//! or how slow any one of them is — a slow subscriber that falls behind
//! capacity gets an explicit `Lagged(n)` signal on its next receive instead
//! of stalling the producer or silently losing data with no record of it
//! (`AGENTS.md` rule 1).

use tokio::sync::broadcast;

use crate::gap::GapNotice;
use crate::sample::{LogRecord, TelemetrySample};

/// One event this crate's pipeline publishes to subscribers.
#[derive(Debug, Clone)]
pub enum TelemetryEvent {
    Sample(TelemetrySample),
    Log(LogRecord),
    Gap(GapNotice),
    /// Emitted when [`crate::buffer::BoundedBuffer`] itself evicted an
    /// entry because it was full — a second, buffer-level honesty signal
    /// distinct from a sequence [`GapNotice`] (this can happen even for
    /// signals with no sequence numbers at all).
    BufferOverflow {
        signal_key: String,
    },
}

/// A bounded, multi-subscriber event bus.
pub struct EventBus {
    sender: broadcast::Sender<TelemetryEvent>,
    capacity: usize,
}

impl EventBus {
    /// # Panics
    /// If `capacity` is zero (see `tokio::sync::broadcast::channel`'s own
    /// panic condition — a zero-capacity bus could never deliver anything).
    pub fn new(capacity: usize) -> Self {
        let (sender, _receiver) = broadcast::channel(capacity);
        Self { sender, capacity }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of currently active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Registers a new subscriber. Returns a handle whose
    /// [`Subscription::recv`] never blocks the publisher side — publishing
    /// proceeds regardless of whether any subscription is ever read.
    pub fn subscribe(&self) -> Subscription {
        Subscription {
            receiver: self.sender.subscribe(),
        }
    }

    /// Publishes `event` to every current subscriber. Never blocks, never
    /// grows memory past this bus's fixed `capacity`: if a slow subscriber
    /// has fallen behind, the oldest still-unread entries for it are simply
    /// no longer retrievable by it (reported to that subscriber as
    /// `Lagged(n)` on its next `recv`, per [`SubscriptionError::Lagged`]) —
    /// this call itself always succeeds immediately as long as at least one
    /// subscriber exists.
    pub fn publish(&self, event: TelemetryEvent) -> PublishOutcome {
        match self.sender.send(event) {
            Ok(delivered_to) => PublishOutcome::Delivered {
                subscriber_count: delivered_to,
            },
            // `SendError` from `broadcast::Sender::send` means there are
            // currently zero subscribers — not a failure of the bus, just
            // nobody listening right now. Never propagated as an error: a
            // producer must be able to publish freely regardless of
            // whether any consumer currently exists.
            Err(_) => PublishOutcome::NoSubscribers,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    Delivered { subscriber_count: usize },
    NoSubscribers,
}

/// A live subscription. Dropping this ends the subscription; nothing else
/// needs to be called to unsubscribe (`Docs/SPEC.md` §7's "Subscription
/// APIs return an explicit unsubscribe handle" principle applied at this
/// layer — dropping the handle *is* the unsubscribe here).
pub struct Subscription {
    receiver: broadcast::Receiver<TelemetryEvent>,
}

/// What one [`Subscription::recv`] call can honestly report.
#[derive(Debug)]
pub enum SubscriptionError {
    /// The bus itself is gone (every [`EventBus`]/[`EventBus::publish`]
    /// side has been dropped) — no more events will ever arrive.
    Closed,
    /// This subscriber fell behind the bus's fixed capacity and missed
    /// exactly `skipped` events. Reported honestly rather than silently
    /// resuming as if nothing were lost (`AGENTS.md` rule 1).
    Lagged { skipped: u64 },
}

impl Subscription {
    /// Awaits the next event. See [`SubscriptionError::Lagged`] for the
    /// explicit, honest signal a slow consumer receives instead of this
    /// call ever blocking the publisher or silently skipping data with no
    /// record of it.
    pub async fn recv(&mut self) -> Result<TelemetryEvent, SubscriptionError> {
        match self.receiver.recv().await {
            Ok(event) => Ok(event),
            Err(broadcast::error::RecvError::Closed) => Err(SubscriptionError::Closed),
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                Err(SubscriptionError::Lagged { skipped })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn overflow_event(n: u32) -> TelemetryEvent {
        TelemetryEvent::BufferOverflow {
            signal_key: format!("dev-1::metric-{n}"),
        }
    }

    #[test]
    fn publish_without_any_subscriber_reports_no_subscribers_not_an_error() {
        let bus = EventBus::new(4);
        assert_eq!(
            bus.publish(overflow_event(1)),
            PublishOutcome::NoSubscribers
        );
    }

    #[tokio::test]
    async fn a_subscriber_receives_published_events_in_order() {
        let bus = EventBus::new(4);
        let mut sub = bus.subscribe();
        bus.publish(overflow_event(1));
        bus.publish(overflow_event(2));

        let first = sub.recv().await.unwrap();
        let second = sub.recv().await.unwrap();
        assert!(
            matches!(first, TelemetryEvent::BufferOverflow { signal_key } if signal_key.ends_with("1"))
        );
        assert!(
            matches!(second, TelemetryEvent::BufferOverflow { signal_key } if signal_key.ends_with("2"))
        );
    }

    #[tokio::test]
    async fn publishing_never_blocks_even_far_past_a_subscribers_read_rate() {
        // Deliverable 3: "a slow consumer does not crash or unboundedly
        // memory-balloon the producer side." Publish 10x the bus capacity
        // with zero reads in between and require every publish to return
        // immediately.
        let bus = EventBus::new(4);
        let _sub = bus.subscribe(); // exists, but never reads

        let result = tokio::time::timeout(Duration::from_secs(2), async {
            for i in 0..40u32 {
                bus.publish(overflow_event(i));
            }
        })
        .await;
        assert!(
            result.is_ok(),
            "publish must never block on a slow/non-reading subscriber"
        );
    }

    #[tokio::test]
    async fn a_lagging_subscriber_gets_an_explicit_lagged_signal_not_silence() {
        let bus = EventBus::new(2);
        let mut sub = bus.subscribe();

        // Publish more than capacity before this subscriber reads anything.
        for i in 0..5u32 {
            bus.publish(overflow_event(i));
        }

        match sub.recv().await {
            Err(SubscriptionError::Lagged { skipped }) => {
                assert_eq!(skipped, 3, "5 published, capacity 2 => 3 skipped");
            }
            other => panic!("expected Lagged, got {other:?}"),
        }
        // After the Lagged signal, the subscriber resumes from the oldest
        // still-retained event rather than being stuck forever.
        let resumed = sub.recv().await.unwrap();
        assert!(
            matches!(resumed, TelemetryEvent::BufferOverflow { signal_key } if signal_key.ends_with("3"))
        );
    }

    #[tokio::test]
    async fn closing_the_bus_is_reported_as_closed_not_a_hang() {
        let bus = EventBus::new(2);
        let mut sub = bus.subscribe();
        drop(bus);
        match sub.recv().await {
            Err(SubscriptionError::Closed) => {}
            other => panic!("expected Closed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn multiple_subscribers_each_get_every_event_independently() {
        let bus = EventBus::new(4);
        let mut sub_a = bus.subscribe();
        let mut sub_b = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);

        let outcome = bus.publish(overflow_event(1));
        assert_eq!(
            outcome,
            PublishOutcome::Delivered {
                subscriber_count: 2
            }
        );

        assert!(sub_a.recv().await.is_ok());
        assert!(sub_b.recv().await.is_ok());
    }
}
