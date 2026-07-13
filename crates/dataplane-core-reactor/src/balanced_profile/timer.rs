use super::park::BalancedParkLease;
use super::{EmbeddedProfileLayout, EmbeddedResourceError, PerformanceProfileLayout};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BalancedTimerOwnerConfig {
    pub initial_capacity: usize,
    pub wake_batch: usize,
    /// Hard cap on armed timers. `None` (the default) keeps the host-profile
    /// stores elastic; the embedded store is always bounded by its initial
    /// capacity regardless of this field.
    pub max_entries: Option<usize>,
}

impl Default for BalancedTimerOwnerConfig {
    fn default() -> Self {
        Self {
            initial_capacity: 256,
            wake_batch: 64,
            max_entries: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BalancedTimerWake {
    pub deadline_ns: u64,
    pub lease: BalancedParkLease,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BalancedTimerEntry {
    deadline_ns: u64,
    lease: BalancedParkLease,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalancedTimerOwner {
    entries: Vec<BalancedTimerEntry>,
    wake_batch: usize,
    max_entries: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddedTimerStore {
    entries: Vec<BalancedTimerEntry>,
    wake_batch: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerformanceTimerStore {
    entries: Vec<BalancedTimerEntry>,
    wake_batch: usize,
    max_entries: Option<usize>,
}

pub trait TimerStoreOps {
    fn next_deadline(&self) -> Option<u64>;
    /// Arms a timer. Bounded stores (embedded always; host-profile stores
    /// when configured with `max_entries`) return
    /// [`EmbeddedResourceError::TimerCapacityExceeded`] at capacity;
    /// unconfigured host-profile stores are elastic and never fail.
    fn arm(
        &mut self,
        deadline_ns: u64,
        lease: BalancedParkLease,
    ) -> Result<(), EmbeddedResourceError>;
    fn cancel(&mut self, lease: BalancedParkLease) -> bool;
    fn drain_expired<F>(&mut self, now_ns: u64, on_wake: F) -> usize
    where
        F: FnMut(BalancedTimerWake);
}

impl EmbeddedTimerStore {
    #[inline]
    pub fn from_config(config: BalancedTimerOwnerConfig) -> Self {
        Self {
            entries: Vec::with_capacity(config.initial_capacity),
            wake_batch: config.wake_batch.max(1),
        }
    }

    #[inline]
    pub fn config_from_layout(layout: &EmbeddedProfileLayout) -> BalancedTimerOwnerConfig {
        layout.timer_owner_config()
    }

    #[inline]
    pub fn wake_batch(&self) -> usize {
        self.wake_batch
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[inline]
    pub fn next_deadline(&self) -> Option<u64> {
        self.entries.first().map(|entry| entry.deadline_ns)
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    #[inline]
    pub fn try_arm(
        &mut self,
        deadline_ns: u64,
        lease: BalancedParkLease,
    ) -> Result<(), EmbeddedResourceError> {
        if self.entries.len() >= self.entries.capacity() {
            return Err(EmbeddedResourceError::TimerCapacityExceeded);
        }
        let entry = BalancedTimerEntry { deadline_ns, lease };
        let idx = self
            .entries
            .partition_point(|existing| existing.deadline_ns <= deadline_ns);
        self.entries.insert(idx, entry);
        Ok(())
    }
}

impl TimerStoreOps for EmbeddedTimerStore {
    #[inline]
    fn next_deadline(&self) -> Option<u64> {
        self.next_deadline()
    }

    #[inline]
    fn arm(
        &mut self,
        deadline_ns: u64,
        lease: BalancedParkLease,
    ) -> Result<(), EmbeddedResourceError> {
        self.try_arm(deadline_ns, lease)
    }

    #[inline]
    fn cancel(&mut self, lease: BalancedParkLease) -> bool {
        if let Some(idx) = self.entries.iter().position(|entry| entry.lease == lease) {
            self.entries.remove(idx);
            true
        } else {
            false
        }
    }

    #[inline]
    fn drain_expired<F>(&mut self, now_ns: u64, on_wake: F) -> usize
    where
        F: FnMut(BalancedTimerWake),
    {
        drain_expired_sorted(&mut self.entries, now_ns, self.wake_batch, on_wake)
    }
}

impl PerformanceTimerStore {
    #[inline]
    pub fn from_config(config: BalancedTimerOwnerConfig) -> Self {
        Self {
            entries: Vec::with_capacity(config.initial_capacity),
            wake_batch: config.wake_batch.max(1),
            max_entries: config.max_entries,
        }
    }

    #[inline]
    pub fn config_from_layout(layout: &PerformanceProfileLayout) -> BalancedTimerOwnerConfig {
        layout.timer_owner_config()
    }

    #[inline]
    pub fn wake_batch(&self) -> usize {
        self.wake_batch
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[inline]
    pub fn next_deadline(&self) -> Option<u64> {
        self.entries.first().map(|entry| entry.deadline_ns)
    }
}

impl TimerStoreOps for PerformanceTimerStore {
    #[inline]
    fn next_deadline(&self) -> Option<u64> {
        self.next_deadline()
    }

    #[inline]
    fn arm(
        &mut self,
        deadline_ns: u64,
        lease: BalancedParkLease,
    ) -> Result<(), EmbeddedResourceError> {
        if let Some(max) = self.max_entries {
            if self.entries.len() >= max {
                return Err(EmbeddedResourceError::TimerCapacityExceeded);
            }
        }
        let entry = BalancedTimerEntry { deadline_ns, lease };
        let idx = self
            .entries
            .partition_point(|existing| existing.deadline_ns <= deadline_ns);
        self.entries.insert(idx, entry);
        Ok(())
    }

    #[inline]
    fn cancel(&mut self, lease: BalancedParkLease) -> bool {
        if let Some(idx) = self.entries.iter().position(|entry| entry.lease == lease) {
            self.entries.remove(idx);
            true
        } else {
            false
        }
    }

    #[inline]
    fn drain_expired<F>(&mut self, now_ns: u64, on_wake: F) -> usize
    where
        F: FnMut(BalancedTimerWake),
    {
        drain_expired_sorted(&mut self.entries, now_ns, self.wake_batch, on_wake)
    }
}

impl BalancedTimerOwner {
    #[inline]
    pub fn from_config(config: BalancedTimerOwnerConfig) -> Self {
        Self {
            entries: Vec::with_capacity(config.initial_capacity),
            wake_batch: config.wake_batch.max(1),
            max_entries: config.max_entries,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[inline]
    pub fn wake_batch(&self) -> usize {
        self.wake_batch
    }

    #[inline]
    pub fn next_deadline(&self) -> Option<u64> {
        self.entries.first().map(|entry| entry.deadline_ns)
    }

    #[inline]
    pub fn arm(&mut self, deadline_ns: u64, lease: BalancedParkLease) {
        let entry = BalancedTimerEntry { deadline_ns, lease };
        let idx = self
            .entries
            .partition_point(|existing| existing.deadline_ns <= deadline_ns);
        self.entries.insert(idx, entry);
    }

    #[inline]
    pub fn cancel(&mut self, lease: BalancedParkLease) -> bool {
        if let Some(idx) = self.entries.iter().position(|entry| entry.lease == lease) {
            self.entries.remove(idx);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn drain_expired<F>(&mut self, now_ns: u64, on_wake: F) -> usize
    where
        F: FnMut(BalancedTimerWake),
    {
        drain_expired_sorted(&mut self.entries, now_ns, self.wake_batch, on_wake)
    }
}

impl TimerStoreOps for BalancedTimerOwner {
    #[inline]
    fn next_deadline(&self) -> Option<u64> {
        self.next_deadline()
    }

    #[inline]
    fn arm(
        &mut self,
        deadline_ns: u64,
        lease: BalancedParkLease,
    ) -> Result<(), EmbeddedResourceError> {
        if let Some(max) = self.max_entries {
            if self.entries.len() >= max {
                return Err(EmbeddedResourceError::TimerCapacityExceeded);
            }
        }
        self.arm(deadline_ns, lease);
        Ok(())
    }

    #[inline]
    fn cancel(&mut self, lease: BalancedParkLease) -> bool {
        self.cancel(lease)
    }

    #[inline]
    fn drain_expired<F>(&mut self, now_ns: u64, on_wake: F) -> usize
    where
        F: FnMut(BalancedTimerWake),
    {
        self.drain_expired(now_ns, on_wake)
    }
}

/// Entries are kept sorted by deadline, so the expired prefix is a single
/// contiguous range: one `partition_point` plus one `drain` instead of
/// repeated `remove(0)` front-shifts.
fn drain_expired_sorted<F>(
    entries: &mut Vec<BalancedTimerEntry>,
    now_ns: u64,
    wake_batch: usize,
    mut on_wake: F,
) -> usize
where
    F: FnMut(BalancedTimerWake),
{
    let expired = entries.partition_point(|entry| entry.deadline_ns <= now_ns);
    let count = expired.min(wake_batch);
    for entry in entries.drain(..count) {
        on_wake(BalancedTimerWake {
            deadline_ns: entry.deadline_ns,
            lease: entry.lease,
        });
    }
    count
}
