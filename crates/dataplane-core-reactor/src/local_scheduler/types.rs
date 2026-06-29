use thunderdome::Index;

pub type TaskId = Index;

pub(crate) const UNKNOWN_SHARD: usize = usize::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchedulerPlacement {
    pub shard: usize,
    pub core_id: usize,
    pub domain: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskScope {
    Local,
    Domain,
    Global,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskPriority {
    High,
    Normal,
    Low,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskMeta {
    pub scope: TaskScope,
    pub priority: TaskPriority,
    pub origin_shard: usize,
    pub origin_domain: usize,
}

impl TaskMeta {
    #[inline(always)]
    pub const fn new(scope: TaskScope, priority: TaskPriority) -> Self {
        Self {
            scope,
            priority,
            origin_shard: UNKNOWN_SHARD,
            origin_domain: UNKNOWN_SHARD,
        }
    }

    #[inline(always)]
    pub const fn local(priority: TaskPriority) -> Self {
        Self::new(TaskScope::Local, priority)
    }

    #[inline(always)]
    pub const fn domain(priority: TaskPriority) -> Self {
        Self::new(TaskScope::Domain, priority)
    }

    #[inline(always)]
    pub const fn global(priority: TaskPriority) -> Self {
        Self::new(TaskScope::Global, priority)
    }

    #[inline(always)]
    pub(crate) fn ensure_origin(&mut self, shard: usize, domain: usize) {
        if self.origin_shard == UNKNOWN_SHARD {
            self.origin_shard = shard;
            self.origin_domain = domain;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShardSchedulerConfig {
    pub local_queue_capacity: usize,
    pub overload_soft_limit: usize,
    pub ingress_drain_budget: usize,
    pub ingress_scan_budget: usize,
    pub ingress_source_budget: usize,
    pub bus_drain_budget: usize,
    pub run_budget: usize,
    pub poll_in_place_time_budget_ns: u64,
    pub poll_in_place_time_check_interval: usize,
    pub poll_in_place_clock_scale: bool,
    pub poll_in_place_clock_rescale_ticks: usize,
    pub poll_in_place_clock_probe_iters: usize,
}

impl Default for ShardSchedulerConfig {
    fn default() -> Self {
        Self {
            local_queue_capacity: 4096,
            overload_soft_limit: 1024,
            ingress_drain_budget: 512,
            ingress_scan_budget: 8,
            ingress_source_budget: 32,
            bus_drain_budget: 128,
            run_budget: 512,
            poll_in_place_time_budget_ns: 100_000,
            poll_in_place_time_check_interval: 4,
            poll_in_place_clock_scale: true,
            poll_in_place_clock_rescale_ticks: 1024,
            poll_in_place_clock_probe_iters: 64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitPlacement {
    Local(TaskId),
    Offloaded(usize),
    BusDeferred,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkDisposition {
    Complete,
    AllDone,
    Requeue,
    DeferBus,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickReport {
    pub elapsed_ns: u64,
    pub drain_ns: u64,
    pub focus_ns: u64,
    pub idle_ns: u64,
    pub local_executed: usize,
    pub ingress_drained: usize,
    pub bus_drained: usize,
    pub offloaded: usize,
    pub bus_deferred: usize,
    pub dropped: usize,
    pub focus_exit_all_done: usize,
    pub focus_exit_complete: usize,
    pub focus_exit_requeue_budget: usize,
    pub focus_exit_requeue_time: usize,
    pub focus_exit_requeue_other: usize,
    pub focus_exit_defer_bus: usize,
    pub focus_exit_missing_task: usize,
    pub focus_exit_queue_empty: usize,
}
