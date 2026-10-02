use bump_scope::Bump;
use quanta::Clock;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;
use thunderdome::Arena;

use super::focus::{FocusPolicy, SingleTaskFocus};
use super::mesh::{build_route, normalize_placements, ShardMeshEndpoint};
use super::push::{PushPolicy, TopologyRoutePush};
use super::task_cell::{poll_async_future, TaskCell, TaskPayload};
use super::trace::{TraceStamp, TraceStep, TRACE_STAMP_DEPTH};
use super::types::{
    SchedulerPlacement, SchedulerStats, ShardSchedulerConfig, SubmitPlacement, TaskId,
    TaskPriority, TaskScope, TickReport, WorkDisposition,
};

pub struct LocalMeshScheduler<
    Op,
    const STACK_BYTES: usize,
    Focus: FocusPolicy = SingleTaskFocus,
    Push: PushPolicy = TopologyRoutePush,
> {
    shard_id: usize,
    domain_id: usize,
    domains: Box<[usize]>,
    route: Box<[usize]>,
    pub(crate) config: ShardSchedulerConfig,
    tasks: Arena<TaskCell<Op, STACK_BYTES>>,
    pub(crate) ready: VecDeque<TaskId>,
    mesh: ShardMeshEndpoint<TaskCell<Op, STACK_BYTES>>,
    bus_tx: kanal::Sender<TaskCell<Op, STACK_BYTES>>,
    bus_rx: Arc<kanal::Receiver<TaskCell<Op, STACK_BYTES>>>,
    bus_staging: Vec<TaskCell<Op, STACK_BYTES>>,
    bus_carryover: VecDeque<TaskCell<Op, STACK_BYTES>>,
    ingress_staging: Vec<TaskCell<Op, STACK_BYTES>>,
    pub(crate) focus_round: Vec<TaskId>,
    scratch: Bump,
    clock: Clock,
    focus_check_interval: usize,
    ticks_since_focus_scale: usize,
    stats: SchedulerStats,
    _focus: std::marker::PhantomData<Focus>,
    _push: std::marker::PhantomData<Push>,
}

impl<Op, const STACK_BYTES: usize, Focus: FocusPolicy, Push: PushPolicy>
    LocalMeshScheduler<Op, STACK_BYTES, Focus, Push>
{
    pub fn new(
        shard: SchedulerPlacement,
        placements: &[SchedulerPlacement],
        mesh: ShardMeshEndpoint<TaskCell<Op, STACK_BYTES>>,
        bus_tx: kanal::Sender<TaskCell<Op, STACK_BYTES>>,
        bus_rx: Arc<kanal::Receiver<TaskCell<Op, STACK_BYTES>>>,
        config: ShardSchedulerConfig,
    ) -> Self {
        let by_shard = normalize_placements(placements);
        debug_assert!(
            bus_tx.is_bounded(),
            "LocalMeshScheduler requires a bounded bus channel; an unbounded \
             bus has no backpressure and grows without limit under overload"
        );
        assert!(
            shard.shard < by_shard.len(),
            "shard is outside placement set"
        );
        assert_eq!(mesh.shard_id(), shard.shard, "mesh shard id mismatch");

        let route = build_route(shard.shard, &by_shard).into_boxed_slice();
        let domains: Vec<usize> = by_shard.iter().map(|p| p.domain).collect();

        Self {
            shard_id: shard.shard,
            domain_id: shard.domain,
            domains: domains.into_boxed_slice(),
            route,
            config,
            tasks: Arena::new(),
            ready: VecDeque::with_capacity(config.local_queue_capacity.max(1)),
            mesh,
            bus_tx,
            bus_rx,
            bus_staging: Vec::with_capacity(config.bus_drain_budget.max(1) * 2),
            bus_carryover: VecDeque::with_capacity(config.bus_drain_budget.max(1) * 2),
            ingress_staging: Vec::new(),
            focus_round: Vec::new(),
            scratch: Bump::new(),
            clock: Clock::new(),
            focus_check_interval: config.poll_in_place_time_check_interval.max(1),
            // Force first tick to calibrate immediately when scaling is enabled.
            ticks_since_focus_scale: config.poll_in_place_clock_rescale_ticks.max(1),
            stats: SchedulerStats::default(),
            _focus: std::marker::PhantomData,
            _push: std::marker::PhantomData,
        }
    }

    #[inline(always)]
    pub fn shard_id(&self) -> usize {
        self.shard_id
    }

    #[inline(always)]
    pub fn route(&self) -> &[usize] {
        &self.route
    }

    #[inline(always)]
    pub fn bus_sender(&self) -> kanal::Sender<TaskCell<Op, STACK_BYTES>> {
        self.bus_tx.clone()
    }

    #[inline(always)]
    pub fn pending_local(&self) -> usize {
        self.ready.len()
    }

    #[inline(always)]
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn submit_work(
        &mut self,
        meta: super::types::TaskMeta,
        op: Op,
    ) -> Result<SubmitPlacement, TaskCell<Op, STACK_BYTES>> {
        self.submit_task_cell(TaskCell::new_work(meta, op))
    }

    pub fn submit_async<F>(
        &mut self,
        meta: super::types::TaskMeta,
        future: F,
    ) -> Result<SubmitPlacement, TaskCell<Op, STACK_BYTES>>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.submit_task_cell(TaskCell::from_future(meta, future))
    }

    pub fn submit_task_cell(
        &mut self,
        mut task: TaskCell<Op, STACK_BYTES>,
    ) -> Result<SubmitPlacement, TaskCell<Op, STACK_BYTES>> {
        task.meta.ensure_origin(self.shard_id, self.domain_id);
        task.stamp(&self.clock, TraceStep::Ingress);

        if self.ready.len() < self.config.overload_soft_limit {
            match self.try_insert_local(task) {
                Ok(id) => return Ok(SubmitPlacement::Local(id)),
                Err(returned) => task = returned,
            }
        }

        match self.try_offload(task) {
            Ok(dst) => return Ok(SubmitPlacement::Offloaded(dst)),
            Err(returned) => task = returned,
        }

        match self.try_insert_local(task) {
            Ok(id) => Ok(SubmitPlacement::Local(id)),
            Err(returned) => {
                task = returned;
                if task.meta.priority == TaskPriority::Low {
                    let mut deferred = Some(task);
                    if matches!(self.bus_tx.try_send_option(&mut deferred), Ok(true)) {
                        Ok(SubmitPlacement::BusDeferred)
                    } else {
                        Err(deferred.take().expect("task retained on failed bus send"))
                    }
                } else {
                    Err(task)
                }
            }
        }
    }

    fn try_insert_local(
        &mut self,
        mut task: TaskCell<Op, STACK_BYTES>,
    ) -> Result<TaskId, TaskCell<Op, STACK_BYTES>> {
        if self.ready.len() >= self.config.local_queue_capacity {
            return Err(task);
        }

        if !self.scope_allows_destination(
            task.meta.scope,
            task.meta.origin_shard,
            task.meta.origin_domain,
            self.shard_id,
        ) {
            return Err(task);
        }

        task.stamp(&self.clock, TraceStep::Queued);
        let id = self.tasks.insert(task);
        self.ready.push_back(id);
        Ok(id)
    }

    fn try_offload(
        &mut self,
        task: TaskCell<Op, STACK_BYTES>,
    ) -> Result<usize, TaskCell<Op, STACK_BYTES>> {
        Push::try_offload(self, task)
    }

    pub(crate) fn scope_allows_destination(
        &self,
        scope: TaskScope,
        origin_shard: usize,
        origin_domain: usize,
        dst: usize,
    ) -> bool {
        match scope {
            TaskScope::Local => dst == origin_shard,
            TaskScope::Domain => self.domains.get(dst).copied() == Some(origin_domain),
            TaskScope::Global => true,
        }
    }

    #[inline(always)]
    pub(crate) fn mesh_try_push(
        &mut self,
        dst: usize,
        task: TaskCell<Op, STACK_BYTES>,
    ) -> Result<(), TaskCell<Op, STACK_BYTES>> {
        self.mesh.try_push(dst, task)
    }

    #[inline(always)]
    fn maybe_rescale_focus_check_interval(&mut self) {
        if self.config.poll_in_place_time_budget_ns == 0 {
            self.focus_check_interval = self.config.poll_in_place_time_check_interval.max(1);
            return;
        }
        if !self.config.poll_in_place_clock_scale {
            self.focus_check_interval = self.config.poll_in_place_time_check_interval.max(1);
            return;
        }

        self.ticks_since_focus_scale = self.ticks_since_focus_scale.saturating_add(1);
        if self.ticks_since_focus_scale < self.config.poll_in_place_clock_rescale_ticks.max(1) {
            return;
        }
        self.ticks_since_focus_scale = 0;

        let probe_iters = self.config.poll_in_place_clock_probe_iters.max(8);
        let start = self.clock.raw();
        let mut i = 0usize;
        while i < probe_iters {
            std::hint::black_box(self.clock.raw());
            i += 1;
        }
        let elapsed = self.clock.delta_as_nanos(start, self.clock.raw()).max(1);
        let per_check_ns = (elapsed / probe_iters as u64).max(1);
        let target_overhead_ns = (self.config.poll_in_place_time_budget_ns / 10).max(1);
        let scaled = (target_overhead_ns / per_check_ns).max(1) as usize;
        self.focus_check_interval = scaled.max(1);
    }

    pub(crate) fn focus_task<F>(
        &mut self,
        task_id: TaskId,
        on_work: &mut F,
        report_local_executed: usize,
    ) -> Option<(WorkDisposition, usize, bool, bool)>
    where
        F: FnMut(&mut Op) -> WorkDisposition,
    {
        let focus_start = self.clock.raw();
        let mut local_polls = 0usize;
        let mut budget_requeue = false;
        let mut time_requeue = false;

        let action = {
            let task = self.tasks.get_mut(task_id)?;
            loop {
                task.stamp(&self.clock, TraceStep::Dispatch);
                let action = match &mut task.payload {
                    TaskPayload::Work(op) => on_work(op),
                    TaskPayload::Future(future) => {
                        if poll_async_future::<STACK_BYTES>(future).is_pending() {
                            WorkDisposition::Requeue
                        } else {
                            WorkDisposition::AllDone
                        }
                    }
                };
                task.stamp(&self.clock, TraceStep::Executed);

                local_polls = local_polls.saturating_add(1);
                // A terminal result must win over the polling budget; otherwise
                // the last completed task is executed again on the next tick.
                if action == WorkDisposition::Requeue
                    && report_local_executed.saturating_add(local_polls) >= self.config.run_budget
                {
                    budget_requeue = true;
                    break WorkDisposition::Requeue;
                }

                match action {
                    WorkDisposition::Requeue => {
                        if self.config.poll_in_place_time_budget_ns != 0 {
                            let check_interval = self.focus_check_interval.max(1);
                            if local_polls.is_multiple_of(check_interval) {
                                let elapsed =
                                    self.clock.delta_as_nanos(focus_start, self.clock.raw());
                                if elapsed >= self.config.poll_in_place_time_budget_ns {
                                    time_requeue = true;
                                    break WorkDisposition::Requeue;
                                }
                            }
                        }
                        continue;
                    }
                    WorkDisposition::Complete => break WorkDisposition::Complete,
                    WorkDisposition::AllDone => break WorkDisposition::AllDone,
                    WorkDisposition::DeferBus => break WorkDisposition::DeferBus,
                }
            }
        };

        Some((action, local_polls, budget_requeue, time_requeue))
    }

    pub(crate) fn apply_task_action(
        &mut self,
        task_id: TaskId,
        action: WorkDisposition,
        report: &mut TickReport,
    ) {
        match action {
            WorkDisposition::Requeue => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    task.stamp(&self.clock, TraceStep::Queued);
                }
                self.ready.push_back(task_id);
            }
            WorkDisposition::Complete => {
                if let Some(mut removed) = self.tasks.remove(task_id) {
                    removed.stamp(&self.clock, TraceStep::Completed);
                }
            }
            WorkDisposition::AllDone => {
                if let Some(mut removed) = self.tasks.remove(task_id) {
                    removed.stamp(&self.clock, TraceStep::Completed);
                }
            }
            WorkDisposition::DeferBus => {
                if let Some(mut removed) = self.tasks.remove(task_id) {
                    removed.stamp(&self.clock, TraceStep::Queued);
                    if removed.meta.priority == TaskPriority::Low {
                        let mut deferred = Some(removed);
                        if matches!(self.bus_tx.try_send_option(&mut deferred), Ok(true)) {
                            report.bus_deferred += 1;
                        } else {
                            let requeued =
                                deferred.take().expect("task retained on failed bus send");
                            let id = self.tasks.insert(requeued);
                            self.ready.push_back(id);
                            report.bus_rejected += 1;
                        }
                    } else {
                        // Reinsertion mints a new generation; the removed key
                        // is stale even if the same physical slot is reused.
                        let id = self.tasks.insert(removed);
                        self.ready.push_back(id);
                    }
                }
            }
        }
    }

    pub fn tick<F>(&mut self, mut on_work: F) -> TickReport
    where
        F: FnMut(&mut Op) -> WorkDisposition,
    {
        crate::scheduler_trace!(
            target: "dataplane.scheduler",
            shard = self.shard_id,
            pending_local = self.ready.len(),
            "tick_start"
        );
        let start = self.clock.raw();
        let checkpoint = self.scratch.checkpoint();
        let _ = self.scratch.alloc(0u8);
        self.maybe_rescale_focus_check_interval();

        let mut report = TickReport::default();
        let drain_start = self.clock.raw();

        self.ingress_staging.clear();
        let drained = self.mesh.drain_incoming(
            self.config.ingress_drain_budget,
            self.config.ingress_scan_budget,
            self.config.ingress_source_budget,
            |task| self.ingress_staging.push(task),
        );
        report.ingress_drained = drained;

        while let Some(task) = self.ingress_staging.pop() {
            self.accept_or_deflect(task, &mut report);
        }

        let direct_backlog = self.ready.len().min(self.config.run_budget);
        let mut budget_left = self
            .config
            .run_budget
            .saturating_sub(direct_backlog)
            .min(self.config.bus_drain_budget);
        while budget_left > 0 {
            let Some(mut task) = self.bus_carryover.pop_front() else {
                break;
            };
            task = match self.try_insert_local(task) {
                Ok(_) => {
                    report.bus_drained += 1;
                    budget_left -= 1;
                    continue;
                }
                Err(task) => task,
            };
            match self.try_offload(task) {
                Ok(_) => {
                    report.bus_drained += 1;
                    report.offloaded += 1;
                }
                Err(returned) => {
                    self.bus_carryover.push_back(returned);
                }
            }
            budget_left -= 1;
        }

        if budget_left > 0 {
            self.bus_staging.clear();
            let _ = self.bus_rx.drain_into(&mut self.bus_staging);

            let global_take_limit = budget_left;
            if self.bus_staging.len() > global_take_limit {
                let overflow = self.bus_staging.split_off(global_take_limit);
                for task in overflow {
                    self.bus_carryover.push_back(task);
                }
            }
            report.bus_drained += self.bus_staging.len();

            while let Some(mut task) = self.bus_staging.pop() {
                if budget_left == 0 {
                    self.bus_carryover.push_back(task);
                    continue;
                }
                task = match self.try_insert_local(task) {
                    Ok(_) => {
                        budget_left -= 1;
                        continue;
                    }
                    Err(task) => task,
                };

                match self.try_offload(task) {
                    Ok(_) => {
                        report.offloaded += 1;
                    }
                    Err(returned) => {
                        self.bus_carryover.push_back(returned);
                    }
                }
                budget_left -= 1;
            }
        }
        let drain_end = self.clock.raw();
        report.drain_ns = self.clock.delta_as_nanos(drain_start, drain_end);
        crate::scheduler_trace!(
            target: "dataplane.scheduler",
            shard = self.shard_id,
            ingress = report.ingress_drained,
            bus = report.bus_drained,
            offloaded = report.offloaded,
            drain_ns = report.drain_ns,
            "tick_drain_done"
        );

        let focus_start = self.clock.raw();
        Focus::run(self, &mut on_work, &mut report);
        let focus_end = self.clock.raw();
        report.focus_ns = self.clock.delta_as_nanos(focus_start, focus_end);
        crate::scheduler_trace!(
            target: "dataplane.scheduler",
            shard = self.shard_id,
            executed = report.local_executed,
            focus_ns = report.focus_ns,
            "tick_focus_done"
        );

        // SAFETY: `checkpoint` was obtained from `self.scratch.checkpoint()` at the
        // start of this `tick` invocation. All scratch allocations in this `tick` are
        // ephemeral and confined to the call stack, so no references escape `tick`.
        unsafe { self.scratch.reset_to(checkpoint) };

        let end = self.clock.raw();
        report.elapsed_ns = self.clock.delta_as_nanos(start, end);
        report.idle_ns = report
            .elapsed_ns
            .saturating_sub(report.drain_ns.saturating_add(report.focus_ns));
        crate::scheduler_trace!(
            target: "dataplane.scheduler",
            shard = self.shard_id,
            elapsed_ns = report.elapsed_ns,
            idle_ns = report.idle_ns,
            "tick_end"
        );
        self.stats.fold(&report);
        report
    }

    /// Cumulative telemetry folded from every [`tick`](Self::tick) on this shard.
    /// Cheap struct copy; aggregate across shards by summing snapshots.
    #[inline(always)]
    pub fn stats(&self) -> SchedulerStats {
        self.stats
    }

    /// Reset the cumulative counters (e.g. after exporting a snapshot interval).
    #[inline(always)]
    pub fn reset_stats(&mut self) {
        self.stats = SchedulerStats::default();
    }

    /// Snapshot the counters and reset them in one step: returns the telemetry
    /// accumulated since the last `take_stats`, ready for periodic export.
    #[inline(always)]
    pub fn take_stats(&mut self) -> SchedulerStats {
        core::mem::take(&mut self.stats)
    }

    fn accept_or_deflect(&mut self, task: TaskCell<Op, STACK_BYTES>, report: &mut TickReport) {
        let mut task = match self.try_insert_local(task) {
            Ok(_) => return,
            Err(task) => task,
        };

        match self.try_offload(task) {
            Ok(_) => {
                report.offloaded += 1;
            }
            Err(returned) => {
                task = returned;
                if task.meta.priority == TaskPriority::Low {
                    let mut deferred = Some(task);
                    if matches!(self.bus_tx.try_send_option(&mut deferred), Ok(true)) {
                        report.bus_deferred += 1;
                    } else {
                        crate::scheduler_trace!("mesh ingress task dropped: bus full");
                        report.bus_rejected += 1;
                        report.dropped += 1;
                    }
                } else {
                    crate::scheduler_trace!(
                        "mesh ingress task dropped: no local slot or offload route"
                    );
                    report.dropped += 1;
                }
            }
        }
    }

    pub fn task_trace(&self, id: TaskId) -> Option<[TraceStamp; TRACE_STAMP_DEPTH]> {
        self.tasks.get(id).and_then(|task| task.trace_snapshot())
    }
}
