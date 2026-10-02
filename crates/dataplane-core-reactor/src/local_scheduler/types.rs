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
    /// Low-priority deferrals rejected because the (bounded) bus was full.
    pub bus_rejected: usize,
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

/// Cumulative, per-shard scheduler telemetry folded from every [`TickReport`].
///
/// Each shard owns its scheduler single-threaded, so these are plain counters with
/// no atomics: folding is a hot-path `+=` and [`snapshot`](super::LocalMeshScheduler::stats)
/// is a struct copy. Aggregate across shards by summing snapshots. This is the durable
/// record of shed/drop/offload activity that a bare `TickReport` throws away each tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SchedulerStats {
    pub ticks: u64,
    pub local_executed: u64,
    pub ingress_drained: u64,
    pub bus_drained: u64,
    pub offloaded: u64,
    pub bus_deferred: u64,
    /// Low-priority deferrals rejected because the bounded bus was full (load shed).
    pub bus_rejected: u64,
    /// Tasks dropped outright under saturation.
    pub dropped: u64,
    /// Time spent draining ingress plus focused execution (`drain_ns + focus_ns`).
    pub busy_ns: u64,
    pub idle_ns: u64,
}

impl SchedulerStats {
    #[inline]
    pub fn fold(&mut self, report: &TickReport) {
        self.ticks = self.ticks.saturating_add(1);
        self.local_executed = self.local_executed.saturating_add(report.local_executed as u64);
        self.ingress_drained = self.ingress_drained.saturating_add(report.ingress_drained as u64);
        self.bus_drained = self.bus_drained.saturating_add(report.bus_drained as u64);
        self.offloaded = self.offloaded.saturating_add(report.offloaded as u64);
        self.bus_deferred = self.bus_deferred.saturating_add(report.bus_deferred as u64);
        self.bus_rejected = self.bus_rejected.saturating_add(report.bus_rejected as u64);
        self.dropped = self.dropped.saturating_add(report.dropped as u64);
        self.busy_ns = self
            .busy_ns
            .saturating_add(report.drain_ns)
            .saturating_add(report.focus_ns);
        self.idle_ns = self.idle_ns.saturating_add(report.idle_ns);
    }

    /// Fraction of observed time spent doing work, in `[0.0, 1.0]`. Returns 0 before
    /// any time has accrued.
    #[inline]
    pub fn utilization(&self) -> f64 {
        let total = self.busy_ns.saturating_add(self.idle_ns);
        if total == 0 {
            return 0.0;
        }
        self.busy_ns as f64 / total as f64
    }

    /// Field-wise saturating add of another snapshot into this one.
    #[inline]
    pub fn merge(&mut self, other: &SchedulerStats) {
        self.ticks = self.ticks.saturating_add(other.ticks);
        self.local_executed = self.local_executed.saturating_add(other.local_executed);
        self.ingress_drained = self.ingress_drained.saturating_add(other.ingress_drained);
        self.bus_drained = self.bus_drained.saturating_add(other.bus_drained);
        self.offloaded = self.offloaded.saturating_add(other.offloaded);
        self.bus_deferred = self.bus_deferred.saturating_add(other.bus_deferred);
        self.bus_rejected = self.bus_rejected.saturating_add(other.bus_rejected);
        self.dropped = self.dropped.saturating_add(other.dropped);
        self.busy_ns = self.busy_ns.saturating_add(other.busy_ns);
        self.idle_ns = self.idle_ns.saturating_add(other.idle_ns);
    }

    /// Sum per-shard snapshots into one cluster-wide snapshot.
    #[inline]
    pub fn aggregate(snapshots: &[SchedulerStats]) -> SchedulerStats {
        let mut acc = SchedulerStats::default();
        for snapshot in snapshots {
            acc.merge(snapshot);
        }
        acc
    }
}
