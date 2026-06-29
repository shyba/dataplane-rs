use dataplane_core_reactor::balanced_profile::EmbeddedProfilePolicy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmbeddedPolicySummary {
    pub task_capacity: usize,
    pub task_budget: usize,
    pub completion_budget: usize,
    pub ingress_budget: usize,
    pub timer_wake_batch: usize,
    pub park_slot_count: usize,
}

impl EmbeddedPolicySummary {
    #[inline]
    pub const fn from_policy(task_capacity: usize, policy: EmbeddedProfilePolicy) -> Self {
        Self {
            task_capacity,
            task_budget: policy.budgets.task_budget,
            completion_budget: policy.budgets.completion_budget,
            ingress_budget: policy.budgets.ingress_budget,
            timer_wake_batch: policy.timer_owner.wake_batch,
            park_slot_count: policy.park_slots.slot_count,
        }
    }
}
