//! Bounded ingestion buffer — `Docs/graph/tasks/MON-001.md` Deliverable 1:
//! "a real, tested upper bound (dropping oldest or rejecting new,
//! explicitly one or the other, never 'grows forever')".
//!
//! This buffer uses **drop-oldest**, not reject-new: telemetry/log ingest is
//! a live stream where the most recent reading is normally the one a
//! monitoring consumer cares about most (`Docs/SPEC.md` §8 Monitoring route:
//! "signal freshness/quality ... bounded logs, gaps"). Dropping the oldest
//! entry on overflow keeps the buffer representative of "recent state"
//! rather than stalling on a backlog. Every drop is counted and reported
//! (`AGENTS.md` rule 1 — a drop is a first-class, honestly reported fact,
//! never silently absorbed).

use std::collections::VecDeque;

/// Outcome of one [`BoundedBuffer::push`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushOutcome<T> {
    /// The buffer had room; nothing was dropped.
    Accepted,
    /// The buffer was at capacity; the oldest entry was evicted to make
    /// room for the new one. Carries the evicted entry so a caller can
    /// still observe/log/persist it before it is gone for good, if it
    /// chooses to.
    AcceptedEvictedOldest(T),
}

/// A capacity-bounded FIFO buffer with an explicit drop-oldest overflow
/// policy. Capacity is fixed at construction and never grows.
#[derive(Debug)]
pub struct BoundedBuffer<T> {
    capacity: usize,
    items: VecDeque<T>,
    /// Total entries ever evicted by this buffer instance — the buffer's
    /// own honest "how much have you actually lost" counter, independent of
    /// any single consumer's view.
    dropped_count: u64,
}

impl<T> BoundedBuffer<T> {
    /// # Panics
    /// If `capacity` is zero — a zero-capacity "buffer" cannot honestly
    /// accept anything and is almost certainly a caller bug, not a valid
    /// bound.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "BoundedBuffer capacity must be at least 1");
        Self {
            capacity,
            items: VecDeque::with_capacity(capacity),
            dropped_count: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.items.len() == self.capacity
    }

    /// Total entries evicted over this buffer's lifetime.
    pub fn dropped_count(&self) -> u64 {
        self.dropped_count
    }

    /// Pushes `item`. If the buffer is already at capacity, evicts the
    /// oldest entry first (drop-oldest policy) and reports it via
    /// [`PushOutcome::AcceptedEvictedOldest`].
    pub fn push(&mut self, item: T) -> PushOutcome<T> {
        if self.items.len() == self.capacity {
            // `unwrap`: length just checked against a >0 capacity, so a
            // full buffer always has a front element.
            let evicted = self.items.pop_front().unwrap();
            self.items.push_back(item);
            self.dropped_count += 1;
            PushOutcome::AcceptedEvictedOldest(evicted)
        } else {
            self.items.push_back(item);
            PushOutcome::Accepted
        }
    }

    /// Removes and returns every currently buffered item, oldest first,
    /// leaving the buffer empty. Does not affect [`Self::dropped_count`].
    pub fn drain(&mut self) -> Vec<T> {
        self.items.drain(..).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "capacity must be at least 1")]
    fn zero_capacity_panics() {
        let _: BoundedBuffer<i32> = BoundedBuffer::new(0);
    }

    #[test]
    fn accepts_up_to_capacity_without_dropping() {
        let mut buf = BoundedBuffer::new(3);
        assert_eq!(buf.push(1), PushOutcome::Accepted);
        assert_eq!(buf.push(2), PushOutcome::Accepted);
        assert_eq!(buf.push(3), PushOutcome::Accepted);
        assert_eq!(buf.len(), 3);
        assert!(buf.is_full());
        assert_eq!(buf.dropped_count(), 0);
    }

    #[test]
    fn overflow_drops_the_oldest_entry_never_the_newest() {
        let mut buf = BoundedBuffer::new(2);
        buf.push(1);
        buf.push(2);
        // Buffer full at [1, 2]; pushing 3 must evict 1 (oldest), not 2.
        let outcome = buf.push(3);
        assert_eq!(outcome, PushOutcome::AcceptedEvictedOldest(1));
        assert_eq!(buf.len(), 2, "capacity is never exceeded");
        assert_eq!(buf.drain(), vec![2, 3], "newest entries survive");
    }

    #[test]
    fn dropped_count_accumulates_and_never_resets_on_its_own() {
        let mut buf = BoundedBuffer::new(1);
        buf.push(1);
        buf.push(2);
        buf.push(3);
        buf.push(4);
        assert_eq!(buf.dropped_count(), 3);
        assert_eq!(buf.len(), 1, "capacity bound is never exceeded");
    }

    #[test]
    fn a_large_stream_never_grows_the_buffer_past_capacity() {
        let mut buf = BoundedBuffer::new(16);
        for i in 0..100_000u64 {
            buf.push(i);
        }
        assert_eq!(buf.len(), 16, "capacity is a real, enforced upper bound");
        assert_eq!(buf.dropped_count(), 100_000 - 16);
        // The 16 survivors must be exactly the most recent ones.
        let survivors = buf.drain();
        let expected: Vec<u64> = (100_000 - 16..100_000).collect();
        assert_eq!(survivors, expected);
    }

    #[test]
    fn drain_empties_the_buffer_but_not_the_drop_counter() {
        let mut buf = BoundedBuffer::new(2);
        buf.push(1);
        buf.push(2);
        buf.push(3); // drops 1
        assert_eq!(buf.drain(), vec![2, 3]);
        assert!(buf.is_empty());
        assert_eq!(buf.dropped_count(), 1);
    }
}
