pub use dataplane_core_reactor::io_fairness::{
    FifoScheduler, ScheduledItem, SchedulerPolicy, SharesScheduler,
};

#[cfg(all(feature = "scheduler-fifo", feature = "scheduler-shares"))]
compile_error!("enable only one scheduler backend feature at a time");

#[cfg(not(any(feature = "scheduler-fifo", feature = "scheduler-shares")))]
compile_error!("enable at least one scheduler backend feature");

#[cfg(feature = "scheduler-fifo")]
pub type SelectedScheduler<R, W> = FifoScheduler<R, W>;

#[cfg(feature = "scheduler-shares")]
pub type SelectedScheduler<R, W> = SharesScheduler<R, W>;

pub fn make_selected_scheduler<R, W>() -> SelectedScheduler<R, W> {
    #[cfg(feature = "scheduler-fifo")]
    {
        FifoScheduler::new()
    }

    #[cfg(feature = "scheduler-shares")]
    {
        SharesScheduler::new(4)
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
