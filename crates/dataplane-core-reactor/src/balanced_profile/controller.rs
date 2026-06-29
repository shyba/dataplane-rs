use super::park::{
    BalancedParkLease, BalancedParkSlots, BalancedWakeToken, EmbeddedParkStore, ParkStoreOps,
};
use super::timer::{BalancedTimerOwner, EmbeddedTimerStore, TimerStoreOps};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalancedControllerPhase {
    DrainCompletions,
    ProcessTimers,
    RunTasks,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalancedHostAction {
    Continue,
    WaitUntil { deadline_ns: u64 },
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalancedControllerStep {
    pub phase: BalancedControllerPhase,
    pub expired_timers: usize,
    pub host_action: BalancedHostAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalancedController<TimerStore = BalancedTimerOwner, ParkStore = BalancedParkSlots> {
    timer_store: TimerStore,
    park_store: ParkStore,
    phase: BalancedControllerPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbeddedResourceError {
    ParkSlotsExhausted,
    TimerCapacityExceeded,
}

impl<TimerStore, ParkStore> BalancedController<TimerStore, ParkStore>
where
    TimerStore: TimerStoreOps,
    ParkStore: ParkStoreOps,
{
    #[inline]
    pub fn new(timer_store: TimerStore, park_store: ParkStore) -> Self {
        Self {
            timer_store,
            park_store,
            phase: BalancedControllerPhase::Idle,
        }
    }

    #[inline]
    pub fn timer_store(&self) -> &TimerStore {
        &self.timer_store
    }

    #[inline]
    pub fn timer_store_mut(&mut self) -> &mut TimerStore {
        &mut self.timer_store
    }

    #[inline]
    pub fn park_store(&self) -> &ParkStore {
        &self.park_store
    }

    #[inline]
    pub fn park_store_mut(&mut self) -> &mut ParkStore {
        &mut self.park_store
    }

    #[inline]
    pub fn phase(&self) -> BalancedControllerPhase {
        self.phase
    }

    #[inline]
    pub fn next_deadline(&self) -> Option<u64> {
        self.timer_store.next_deadline()
    }

    #[inline]
    pub fn step(
        &mut self,
        now_ns: u64,
        has_runtime_work: bool,
        has_task_work: bool,
    ) -> BalancedControllerStep {
        self.phase = if has_runtime_work {
            BalancedControllerPhase::DrainCompletions
        } else if self
            .timer_store
            .next_deadline()
            .is_some_and(|deadline| deadline <= now_ns)
        {
            BalancedControllerPhase::ProcessTimers
        } else if has_task_work {
            BalancedControllerPhase::RunTasks
        } else {
            BalancedControllerPhase::Idle
        };

        let expired_timers = if self.phase == BalancedControllerPhase::ProcessTimers {
            self.timer_store.drain_expired(now_ns, |wake| {
                let _ = self.park_store.wake(wake.lease);
            })
        } else {
            0
        };

        let host_action = if has_runtime_work || has_task_work || expired_timers > 0 {
            BalancedHostAction::Continue
        } else if let Some(deadline_ns) = self.timer_store.next_deadline() {
            BalancedHostAction::WaitUntil { deadline_ns }
        } else {
            BalancedHostAction::Idle
        };

        BalancedControllerStep {
            phase: self.phase,
            expired_timers,
            host_action,
        }
    }
}

impl BalancedController<BalancedTimerOwner, BalancedParkSlots> {
    #[inline]
    pub fn timer_owner(&self) -> &BalancedTimerOwner {
        &self.timer_store
    }

    #[inline]
    pub fn timer_owner_mut(&mut self) -> &mut BalancedTimerOwner {
        &mut self.timer_store
    }

    #[inline]
    pub fn park_slots(&self) -> &BalancedParkSlots {
        &self.park_store
    }

    #[inline]
    pub fn park_slots_mut(&mut self) -> &mut BalancedParkSlots {
        &mut self.park_store
    }
}

impl BalancedController<EmbeddedTimerStore, EmbeddedParkStore> {
    #[inline]
    pub fn try_arm_park(
        &mut self,
        wake_token: BalancedWakeToken,
    ) -> Result<BalancedParkLease, EmbeddedResourceError> {
        self.park_store
            .arm_next(wake_token)
            .ok_or(EmbeddedResourceError::ParkSlotsExhausted)
    }

    #[inline]
    pub fn try_arm_timer(
        &mut self,
        deadline_ns: u64,
        lease: BalancedParkLease,
    ) -> Result<(), EmbeddedResourceError> {
        self.timer_store.try_arm(deadline_ns, lease)
    }
}
