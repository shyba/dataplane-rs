pub(crate) const TIMER_MAX_GAP_BOUND: u32 = 32;

#[derive(Copy, Clone)]
pub(crate) struct TimerTimeoutCase {
    pub(crate) now_ticks: u32,
    pub(crate) deadline_ticks: u32,
    pub(crate) grace_ticks: u32,
}

impl TimerTimeoutCase {
    pub(crate) const fn age(self) -> u32 {
        self.now_ticks.wrapping_sub(self.deadline_ticks)
    }

    pub(crate) const fn is_expired(self) -> bool {
        self.age() > self.grace_ticks
    }
}

pub(crate) struct TimerTimeoutProof {
    pub(crate) stale_case: TimerTimeoutCase,
    pub(crate) expired_case: TimerTimeoutCase,
    pub(crate) positive_case: TimerTimeoutCase,
    pub(crate) wrap_case: TimerTimeoutCase,
    pub(crate) observed_gap_ticks: u32,
}

pub(crate) fn evaluate_timer_timeout_proof() -> TimerTimeoutProof {
    let stale_case = TimerTimeoutCase {
        now_ticks: 19,
        deadline_ticks: 13,
        grace_ticks: 4,
    };
    let expired_case = TimerTimeoutCase {
        now_ticks: 12,
        deadline_ticks: u32::MAX - 2,
        grace_ticks: 1,
    };
    let positive_case = TimerTimeoutCase {
        now_ticks: 21,
        deadline_ticks: 18,
        grace_ticks: 8,
    };
    let wrap_case = TimerTimeoutCase {
        now_ticks: 4,
        deadline_ticks: u32::MAX - 3,
        grace_ticks: 8,
    };

    let tick_samples = [3_u32, 5, 9, 14, 22, 30];
    let mut last_tick = tick_samples[0];
    let mut observed_gap_ticks = 0_u32;
    let mut idx = 1;
    while idx < tick_samples.len() {
        let current_tick = tick_samples[idx];
        let gap = current_tick.wrapping_sub(last_tick);
        if gap > observed_gap_ticks {
            observed_gap_ticks = gap;
        }
        last_tick = current_tick;
        idx += 1;
    }

    TimerTimeoutProof {
        stale_case,
        expired_case,
        positive_case,
        wrap_case,
        observed_gap_ticks,
    }
}
