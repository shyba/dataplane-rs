extern crate alloc;

use alloc::collections::VecDeque;

const LOCAL_BUDGET_MULTIPLIER: usize = 8;

pub enum ScheduledItem<R, W> {
    Ready(R),
    WriteReady(W),
}

pub trait SchedulerPolicy<R, W> {
    fn push_ready(&mut self, op: R);
    fn push_write_ready(&mut self, item: W);
    fn pop_next(&mut self) -> Option<ScheduledItem<R, W>>;
    fn has_work(&self) -> bool;
    fn ready_depth(&self) -> usize;
    fn write_ready_depth(&self) -> usize;
    fn drive_budget(base_budget: usize) -> usize;
}

pub struct FifoScheduler<R, W> {
    ready: VecDeque<R>,
    write_ready: VecDeque<W>,
    prefer_write: bool,
}

impl<R, W> FifoScheduler<R, W> {
    pub fn new() -> Self {
        Self {
            ready: VecDeque::new(),
            write_ready: VecDeque::new(),
            prefer_write: true,
        }
    }
}

impl<R, W> Default for FifoScheduler<R, W> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R, W> SchedulerPolicy<R, W> for FifoScheduler<R, W> {
    fn push_ready(&mut self, op: R) {
        self.ready.push_back(op);
    }

    fn push_write_ready(&mut self, item: W) {
        self.write_ready.push_back(item);
    }

    fn pop_next(&mut self) -> Option<ScheduledItem<R, W>> {
        let choose_write = if self.ready.is_empty() {
            true
        } else if self.write_ready.is_empty() {
            false
        } else {
            self.prefer_write
        };
        let item = if choose_write {
            self.write_ready.pop_front().map(ScheduledItem::WriteReady)
        } else {
            self.ready.pop_front().map(ScheduledItem::Ready)
        };
        if item.is_some() {
            self.prefer_write = !choose_write;
        }
        item
    }

    fn has_work(&self) -> bool {
        !self.ready.is_empty() || !self.write_ready.is_empty()
    }

    fn ready_depth(&self) -> usize {
        self.ready.len()
    }

    fn write_ready_depth(&self) -> usize {
        self.write_ready.len()
    }

    fn drive_budget(base_budget: usize) -> usize {
        base_budget * LOCAL_BUDGET_MULTIPLIER
    }
}

pub struct SharesScheduler<R, W> {
    ready: VecDeque<R>,
    write_ready: VecDeque<W>,
    write_burst: usize,
    write_credit: usize,
}

impl<R, W> SharesScheduler<R, W> {
    pub fn new(write_burst: usize) -> Self {
        let write_burst = write_burst.max(1);
        Self {
            ready: VecDeque::new(),
            write_ready: VecDeque::new(),
            write_burst,
            write_credit: write_burst,
        }
    }
}

impl<R, W> SchedulerPolicy<R, W> for SharesScheduler<R, W> {
    fn push_ready(&mut self, op: R) {
        self.ready.push_back(op);
    }

    fn push_write_ready(&mut self, item: W) {
        self.write_ready.push_back(item);
    }

    fn pop_next(&mut self) -> Option<ScheduledItem<R, W>> {
        if !self.write_ready.is_empty() && self.write_credit > 0 {
            self.write_credit -= 1;
            return self.write_ready.pop_front().map(ScheduledItem::WriteReady);
        }
        if !self.ready.is_empty() {
            self.write_credit = self.write_burst;
            return self.ready.pop_front().map(ScheduledItem::Ready);
        }
        if !self.write_ready.is_empty() {
            self.write_credit = self.write_burst.saturating_sub(1);
            return self.write_ready.pop_front().map(ScheduledItem::WriteReady);
        }
        None
    }

    fn has_work(&self) -> bool {
        !self.ready.is_empty() || !self.write_ready.is_empty()
    }

    fn ready_depth(&self) -> usize {
        self.ready.len()
    }

    fn write_ready_depth(&self) -> usize {
        self.write_ready.len()
    }

    fn drive_budget(base_budget: usize) -> usize {
        base_budget * LOCAL_BUDGET_MULTIPLIER
    }
}

#[cfg(test)]
mod tests {
    use super::{FifoScheduler, ScheduledItem, SchedulerPolicy, SharesScheduler};

    #[test]
    fn fifo_preserves_independent_queue_order() {
        let mut sched: FifoScheduler<u8, u8> = FifoScheduler::new();
        sched.push_ready(1);
        sched.push_ready(2);
        sched.push_write_ready(10);
        sched.push_write_ready(11);

        assert!(matches!(
            sched.pop_next(),
            Some(ScheduledItem::WriteReady(10))
        ));
        assert!(matches!(sched.pop_next(), Some(ScheduledItem::Ready(1))));
        assert!(matches!(
            sched.pop_next(),
            Some(ScheduledItem::WriteReady(11))
        ));
        assert!(matches!(sched.pop_next(), Some(ScheduledItem::Ready(2))));
    }

    #[test]
    fn shares_scheduler_limits_write_burst() {
        let mut sched: SharesScheduler<u8, u8> = SharesScheduler::new(2);
        sched.push_write_ready(10);
        sched.push_write_ready(11);
        sched.push_write_ready(12);
        sched.push_ready(1);
        sched.push_ready(2);

        assert!(matches!(
            sched.pop_next(),
            Some(ScheduledItem::WriteReady(10))
        ));
        assert!(matches!(
            sched.pop_next(),
            Some(ScheduledItem::WriteReady(11))
        ));
        assert!(matches!(sched.pop_next(), Some(ScheduledItem::Ready(1))));
        assert!(matches!(
            sched.pop_next(),
            Some(ScheduledItem::WriteReady(12))
        ));
        assert!(matches!(sched.pop_next(), Some(ScheduledItem::Ready(2))));
    }
}
