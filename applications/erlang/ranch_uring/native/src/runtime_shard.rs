use super::runtime_shard_recv::{
    advance_session_id, classify_cancel_recv_decision, SESSION_ID_LOCAL_MASK, SESSION_ID_SHARD_BITS,
};
// DP-RB-0008: Explicit scalar imports from runtime_command
use crate::runtime::runtime_command::{Command, ListenerShard};
// DP-RB-0009: Explicit table/id imports from runtime_api
use super::*;
use crate::runtime::ResultTarget;
// Explicit Result types proved via runtime_result_queue (DP-RB-0008, DP-RB-0009)
// Note: ResultBatchSlot and ResultEvent are only used in tests
use crate::runtime_reactor::Op;
#[cfg(feature = "exec-strategy-sqpoll")]
use crate::runtime_reactor::RingKind;
use crate::runtime_reactor::BUF_SIZE;
use crate::runtime_result_queue::{ResultReduceState, ResultReduceTrigger};
#[cfg(feature = "exec-strategy-sqpoll")]
use io_uring::types;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};

const STOP_DRAIN_LOG_EVERY: u64 = 64;
const CQE_OVERFLOW_BACKOFF_MULTIPLIER: u64 = 64;

// Re-export RecvDecision from the L4 extracted module
use super::runtime_shard_recv::RecvDecision;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShardPumpStep {
    Continue,
    Stop,
}

impl ShardState {
    /// Delegate to runtime_shard_recv::classify_recv_decision.
    /// Kept as an associated fn so existing call sites don't need to change.
    #[inline]
    fn classify_recv_decision(
        has_pending_batch: bool,
        has_pending_recv: bool,
        rx_bytes: usize,
        len: usize,
    ) -> RecvDecision {
        super::runtime_shard_recv::classify_recv_decision(
            has_pending_batch,
            has_pending_recv,
            rx_bytes,
            len,
        )
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        shard: usize,
        cpu: usize,
        latency_ring: IoUring,
        main_ring: IoUring,
        rx: ShardReceiver<Command>,
        wakeup_fd: RawFd,
        read_pool: LockedReadBufPool,
        read_fixed_base: u16,
        read_fixed_slots: Vec<Option<u16>>,
        read_fixed_enabled: bool,
        subscribe_fixed_enabled: bool,
        subscribe_fixed_base: Option<u16>,
        subscribe_fixed_slots: Vec<Option<u16>>,
        submit_pressure: bool,
        sqpoll_accepted: bool,
        _latency_sqpoll_accepted: bool,
        runtime_config: ShardRuntimeConfig,
        provided_recv_pool: Option<ProvidedRecvPool>,
        arenas: RuntimeArenas,
        control: Arc<ShardControl>,
    ) -> Self {
        ShardState {
            shard,
            cpu,
            latency_ring,
            main_ring,
            main_submit_batch: runtime_config.main_submit_batch(),
            main_submit_max_delay_ns: runtime_config.main_submit_max_delay_ns(),
            main_last_submit_ns: now_monotonic_ns(),
            recv_ring_mode: runtime_config.recv_ring_mode(),
            next_cqe_main_first: false,
            rx,
            read_pool,
            read_fixed_base,
            read_fixed_slots,
            read_fixed_enabled,
            subscribe_fixed_enabled,
            subscribe_fixed_base,
            subscribe_fixed_slots,
            submit_pressure,
            sqpoll_accepted,
            provided_recv_pool,
            arenas,
            wakeup_fd,
            control,
            wakeup_buf: [0u8; 8],
            latency_ops: OpTable::new(),
            main_ops: OpTable::new(),
            listeners: ListenerTable::default(),
            conns: ConnectionTable::default(),
            subscriptions: SubscriptionTable::default(),
            pending_statx: U64Map::default(),
            local: make_selected_scheduler(),
            deferred_submit_buf: Vec::with_capacity(DEFERRED_SUBMIT_CAP),
            deferred_exec_buf: Vec::with_capacity(DEFERRED_EXEC_CAP),
            deferred_command_buf: VecDeque::new(),
            deferred_close_fds: VecDeque::with_capacity(DEFERRED_SUBMIT_CAP),
            pending_soft_link_sqe: false,
            result_reduce: ResultReduceState::default(),
            result_reduce_cqes: 0,
            result_batches: Vec::new(),
            result_batch_nonempty_slots: 0,
            result_direct_send: runtime_config.result_direct_send(),
            callback_local_fifo: VecDeque::with_capacity(CALLBACK_LOCAL_CAP),
            latency_queue: std::array::from_fn(|_| None),
            latency_queue_len: 0,
            stop_state: ShardStopState::Running,
            stop_drain_snapshot: None,
            stop_drain_tick: 0,
            debug_loop_queues: runtime_config.debug_loop_queues(),
            debug_loop_every: runtime_config.debug_loop_every(),
            debug_loop_tick: 0,
            #[cfg(debug_assertions)]
            debug_phase_stats: DebugPhaseStats::configured(),
            last_progress_ns: now_monotonic_ns(),
            idle_wait_ns: runtime_config.idle_wait_ns(),
            sqpoll_idle_batch: runtime_config.sqpoll_idle_batch(),
            sqpoll_idle_usec: runtime_config.sqpoll_idle_usec(),
            next_session_id: 1,
            stats: ShardStats::new(),
        }
    }

    #[cfg(feature = "exec-strategy-sqpoll")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        shard: usize,
        cpu: usize,
        latency_ring: IoUring,
        rx: ShardReceiver<Command>,
        wakeup_fd: RawFd,
        read_pool: LockedReadBufPool,
        read_fixed_base: u16,
        read_fixed_slots: Vec<Option<u16>>,
        read_fixed_enabled: bool,
        subscribe_fixed_enabled: bool,
        subscribe_fixed_base: Option<u16>,
        subscribe_fixed_slots: Vec<Option<u16>>,
        submit_pressure: bool,
        sqpoll_accepted: bool,
        runtime_config: ShardRuntimeConfig,
        provided_recv_pool: Option<ProvidedRecvPool>,
        arenas: RuntimeArenas,
        control: Arc<ShardControl>,
    ) -> Self {
        ShardState {
            shard,
            cpu,
            latency_ring,
            rx,
            read_pool,
            read_fixed_base,
            read_fixed_slots,
            read_fixed_enabled,
            subscribe_fixed_enabled,
            subscribe_fixed_base,
            subscribe_fixed_slots,
            submit_pressure,
            sqpoll_accepted,
            provided_recv_pool,
            arenas,
            wakeup_fd,
            control,
            wakeup_buf: [0u8; 8],
            latency_ops: OpTable::new(),
            main_ops: OpTable::new(),
            listeners: ListenerTable::default(),
            conns: ConnectionTable::default(),
            subscriptions: SubscriptionTable::default(),
            pending_statx: U64Map::default(),
            local: make_selected_scheduler(),
            deferred_submit_buf: Vec::with_capacity(DEFERRED_SUBMIT_CAP),
            deferred_exec_buf: Vec::with_capacity(DEFERRED_EXEC_CAP),
            deferred_command_buf: VecDeque::new(),
            deferred_close_fds: VecDeque::with_capacity(DEFERRED_SUBMIT_CAP),
            pending_soft_link_sqe: false,
            result_reduce: ResultReduceState::default(),
            result_reduce_cqes: 0,
            result_batches: Vec::new(),
            result_batch_nonempty_slots: 0,
            result_direct_send: runtime_config.result_direct_send(),
            callback_local_fifo: VecDeque::with_capacity(CALLBACK_LOCAL_CAP),
            latency_queue: std::array::from_fn(|_| None),
            latency_queue_len: 0,
            stop_state: ShardStopState::Running,
            stop_drain_snapshot: None,
            stop_drain_tick: 0,
            debug_loop_queues: runtime_config.debug_loop_queues(),
            debug_loop_every: runtime_config.debug_loop_every(),
            debug_loop_tick: 0,
            #[cfg(debug_assertions)]
            debug_phase_stats: DebugPhaseStats::configured(),
            last_progress_ns: now_monotonic_ns(),
            idle_wait_ns: runtime_config.idle_wait_ns(),
            sqpoll_idle_batch: runtime_config.sqpoll_idle_batch(),
            sqpoll_idle_usec: runtime_config.sqpoll_idle_usec(),
            next_session_id: 1,
            stats: ShardStats::new(),
        }
    }

    pub(crate) fn run(&mut self) {
        self.run_hosted(Self::run_loop);
    }

    #[cfg(feature = "current-thread-shard-driver")]
    #[allow(dead_code)]
    pub(crate) fn run_current_thread_driver(&mut self) {
        self.run();
    }

    /// Inventory for `run_hosted` and the hosted shard loop:
    ///
    /// - direct inputs: `self.cpu`, `self.latency_ring`, `self.main_ring` (when enabled),
    ///   `self.rx`, `self.wakeup_fd`, `self.read_pool`, `self.provided_recv_pool`,
    ///   `self.runtime_config`-derived loop settings, and the caller-supplied `run_loop`
    ///   callback
    /// - mutable loop state: ring submit/completion state, command and wakeup buffers,
    ///   listener/connection/subscription tables, deferred submit/close queues,
    ///   reduce/result queues, stop-drain bookkeeping, latency queue state, session id
    ///   allocation, and shard stats
    ///
    /// The callback executes only after host setup succeeds, so the loop keeps its own
    /// mutable state explicit at the shard boundary instead of hiding it in the caller.
    fn run_hosted<F>(&mut self, mut run_loop: F)
    where
        F: FnMut(&mut Self),
    {
        let _ = crate::runtime_topology::pin_current_to_cpu(self.cpu);
        if self.provide_all_recv_buffers().is_err() {
            if let Some(pool) = self.provided_recv_pool.as_mut() {
                pool.begin_draining();
            }
        }
        if self.arm_wakeup_poll().is_err() {
            let _ = self.try_unregister_fixed_buffers_on_exit();
            return;
        }
        if self.advance_provided_recv_pool_lifecycle().is_err() {
            let _ = self.try_unregister_fixed_buffers_on_exit();
            return;
        }
        run_loop(self);
        self.maybe_log_stop_profile_summary();
        let _ = self.try_unregister_fixed_buffers_on_exit();
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn rings(&self) -> RingPairBorrow<'_> {
        RingPairBorrow {
            latency: &self.latency_ring,
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            main: &self.main_ring,
        }
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn rings_mut(&mut self) -> RingPairBorrowMut<'_> {
        RingPairBorrowMut {
            latency: &mut self.latency_ring,
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            main: &mut self.main_ring,
        }
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(crate) fn run_loop(&mut self) {
        loop {
            if matches!(self.run_hosted_step(), ShardPumpStep::Stop) {
                break;
            }
        }
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    #[inline]
    fn run_hosted_step(&mut self) -> ShardPumpStep {
        self.pump_step_default()
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    #[allow(clippy::never_loop)]
    fn pump_step_default(&mut self) -> ShardPumpStep {
        loop {
            self.maybe_log_loop_queues("default");
            self.maybe_log_phase_stats("default");
            let mut made_progress = false;
            loop {
                let now_ns = now_monotonic_ns();
                let mut cqe_overflow_backpressured = self.update_cqe_overflow_state(now_ns);

                let latency_drained_before = match self.drain_latency_queue() {
                    Ok(n) => n,
                    Err(_) => return ShardPumpStep::Stop,
                };

                #[cfg(debug_assertions)]
                let phase_start = now_monotonic_ns();
                let drained_commands = if cqe_overflow_backpressured {
                    0
                } else {
                    self.poll_commands()
                };
                #[cfg(debug_assertions)]
                self.debug_phase_stats
                    .record_phase(LoopPhase::DrainCommands, phase_start);

                let latency_drained_after_commands = match self.drain_latency_queue() {
                    Ok(n) => n,
                    Err(_) => return ShardPumpStep::Stop,
                };
                if self.is_stopping() {
                    self.stop_state = ShardStopState::Draining;
                }
                self.maybe_log_stop_drain_progress("default");

                #[cfg(debug_assertions)]
                let phase_start = now_monotonic_ns();
                let (latency_cqes, main_cqes) = self.process_completions_fair_budget(CQE_BUDGET);
                #[cfg(debug_assertions)]
                {
                    self.debug_phase_stats
                        .record_phase(LoopPhase::CqeLatency, phase_start);
                    self.debug_phase_stats
                        .record_phase(LoopPhase::CqeMain, phase_start);
                }
                self.tick_stats_logging();
                let completed_cqes = latency_cqes + main_cqes;
                cqe_overflow_backpressured = self.update_cqe_overflow_state(now_monotonic_ns());
                if self.advance_provided_recv_pool_lifecycle().is_err() {
                    return ShardPumpStep::Stop;
                }
                self.advance_deferred_close_fds();
                if completed_cqes > 0 {
                    self.result_reduce_cqes =
                        self.result_reduce_cqes.saturating_add(completed_cqes);
                    let outcome = self.maybe_schedule_reduce_results(ResultReduceTrigger::Cqe);
                    self.enqueue_result_callback_outcome(outcome);
                }

                let latency_drained_after_check = match self.drain_latency_queue() {
                    Ok(n) => n,
                    Err(_) => return ShardPumpStep::Stop,
                };
                if self.is_stopping() && self.is_idle_for_teardown() {
                    break;
                }

                #[cfg(debug_assertions)]
                let phase_start = now_monotonic_ns();
                let outcome = self.maybe_schedule_reduce_results(ResultReduceTrigger::Age);
                self.enqueue_result_callback_outcome(outcome);
                #[cfg(debug_assertions)]
                self.debug_phase_stats
                    .record_phase(LoopPhase::ReduceAge, phase_start);

                #[cfg(debug_assertions)]
                let phase_start = now_monotonic_ns();
                let ready_work = match self.drain_ready() {
                    Ok(n) => n,
                    Err(_) => return ShardPumpStep::Stop,
                };
                #[cfg(debug_assertions)]
                self.debug_phase_stats
                    .record_phase(LoopPhase::DrainReady, phase_start);

                let queued_sqes = if cqe_overflow_backpressured {
                    0
                } else {
                    self.queued_sqes()
                };
                if queued_sqes > 0 {
                    #[cfg(debug_assertions)]
                    let phase_start = now_monotonic_ns();
                    if self.submit_queued().is_err() {
                        return ShardPumpStep::Stop;
                    }
                    #[cfg(debug_assertions)]
                    self.debug_phase_stats
                        .record_phase(LoopPhase::SubmitQueued, phase_start);
                }
                let progressed = latency_drained_before > 0
                    || drained_commands > 0
                    || latency_drained_after_commands > 0
                    || latency_cqes > 0
                    || main_cqes > 0
                    || latency_drained_after_check > 0
                    || ready_work > 0;
                #[cfg(debug_assertions)]
                self.debug_phase_stats.record_progress(progressed);
                if progressed {
                    self.last_progress_ns = now_monotonic_ns();
                }

                let has_pending_commands =
                    self.control.pending_commands.load(Ordering::Acquire) > 0;
                let has_more_local_work = (!cqe_overflow_backpressured && has_pending_commands)
                    || self.local.has_work()
                    || self.has_callback_work()
                    || self.has_latency_queue_work();
                let has_latency_io = self.has_latency_io_work();
                let has_inflight = self.has_inflight_ops();

                if has_more_local_work {
                    made_progress = true;
                    continue;
                }

                let outcome = self.maybe_schedule_reduce_results(ResultReduceTrigger::Idle);
                if self.enqueue_result_callback_outcome(outcome) {
                    made_progress = true;
                    continue;
                }

                if progressed {
                    made_progress = true;
                }

                if self.is_stopping() && self.is_idle_for_teardown() {
                    break;
                }

                if made_progress {
                    break;
                }

                if !has_inflight {
                    #[cfg(debug_assertions)]
                    self.debug_phase_stats
                        .record_stall(StallReason::IdleNoInflight);
                    if !cqe_overflow_backpressured || !has_pending_commands {
                        break;
                    }
                    std::hint::spin_loop();
                    continue;
                }

                let wait_result = if now_ns.saturating_sub(self.last_progress_ns)
                    >= self.idle_wait_ns
                {
                    let prefer_main_wait = !self.main_ops.is_empty()
                        && !has_latency_io
                        && !matches!(self.recv_ring_mode, RecvRingMode::Latency);
                    if prefer_main_wait {
                        #[cfg(debug_assertions)]
                        let phase_start = now_monotonic_ns();
                        #[cfg(feature = "exec-strategy-sqpoll")]
                        let out = retry_eintr(|| self.main_ring.submitter().submit_and_wait(1));
                        #[cfg(not(feature = "exec-strategy-sqpoll"))]
                        let out = if self.main_ring.params().is_setup_sqpoll() {
                            self.wait_main_ready_batch()
                        } else {
                            retry_eintr(|| self.main_ring.submitter().submit_and_wait(1))
                        };
                        #[cfg(not(feature = "exec-strategy-sqpoll"))]
                        if out.is_ok() {
                            self.main_last_submit_ns = now_monotonic_ns();
                        }
                        #[cfg(debug_assertions)]
                        {
                            self.debug_phase_stats
                                .record_phase(LoopPhase::WaitMain, phase_start);
                            self.debug_phase_stats.record_stall(StallReason::WaitMain);
                        }
                        out
                    } else if has_latency_io {
                        #[cfg(debug_assertions)]
                        let phase_start = now_monotonic_ns();
                        let out = retry_eintr(|| self.latency_ring.submitter().submit_and_wait(1));
                        #[cfg(debug_assertions)]
                        {
                            self.debug_phase_stats
                                .record_phase(LoopPhase::WaitLatency, phase_start);
                            self.debug_phase_stats
                                .record_stall(StallReason::WaitLatency);
                        }
                        out
                    } else if !self.main_ops.is_empty() {
                        #[cfg(debug_assertions)]
                        let phase_start = now_monotonic_ns();
                        #[cfg(feature = "exec-strategy-sqpoll")]
                        let out = retry_eintr(|| self.main_ring.submitter().submit_and_wait(1));
                        #[cfg(not(feature = "exec-strategy-sqpoll"))]
                        let out = if self.main_ring.params().is_setup_sqpoll() {
                            self.wait_main_ready_batch()
                        } else {
                            retry_eintr(|| self.main_ring.submitter().submit_and_wait(1))
                        };
                        #[cfg(not(feature = "exec-strategy-sqpoll"))]
                        if out.is_ok() {
                            self.main_last_submit_ns = now_monotonic_ns();
                        }
                        #[cfg(debug_assertions)]
                        {
                            self.debug_phase_stats
                                .record_phase(LoopPhase::WaitMain, phase_start);
                            self.debug_phase_stats.record_stall(StallReason::WaitMain);
                        }
                        out
                    } else {
                        Ok(0)
                    }
                } else {
                    #[cfg(debug_assertions)]
                    let phase_start = now_monotonic_ns();
                    std::hint::spin_loop();
                    #[cfg(debug_assertions)]
                    {
                        self.debug_phase_stats
                            .record_phase(LoopPhase::Spin, phase_start);
                        self.debug_phase_stats.record_stall(StallReason::Spin);
                    }
                    continue;
                };
                if wait_result.is_err() {
                    return ShardPumpStep::Stop;
                }
                self.last_progress_ns = now_monotonic_ns();
                made_progress = true;
            }
            if self.is_stopping() && self.is_idle_for_teardown() {
                return ShardPumpStep::Stop;
            }
            return ShardPumpStep::Continue;
        }
    }

    #[cfg(feature = "exec-strategy-sqpoll")]
    pub(crate) fn run_loop(&mut self) {
        loop {
            if matches!(self.run_hosted_step(), ShardPumpStep::Stop) {
                break;
            }
        }
    }

    #[cfg(feature = "exec-strategy-sqpoll")]
    #[inline]
    fn run_hosted_step(&mut self) -> ShardPumpStep {
        self.pump_step_sqpoll()
    }

    #[cfg(feature = "exec-strategy-sqpoll")]
    fn pump_step_sqpoll(&mut self) -> ShardPumpStep {
        #[allow(clippy::never_loop, unused_variables)]
        loop {
            self.maybe_log_loop_queues("sqpoll");
            self.maybe_log_phase_stats("sqpoll");
            let now_ns = now_monotonic_ns();
            let mut cqe_overflow_backpressured = self.update_cqe_overflow_state(now_ns);

            let latency_drained_before = match self.drain_latency_queue() {
                Ok(n) => n,
                Err(_) => return ShardPumpStep::Stop,
            };

            #[cfg(debug_assertions)]
            let phase_start = now_monotonic_ns();
            let drained_commands = if cqe_overflow_backpressured {
                0
            } else {
                self.poll_commands()
            };
            #[cfg(debug_assertions)]
            self.debug_phase_stats
                .record_phase(LoopPhase::DrainCommands, phase_start);

            let latency_drained_after_commands = match self.drain_latency_queue() {
                Ok(n) => n,
                Err(_) => return ShardPumpStep::Stop,
            };
            if self.is_stopping() {
                self.stop_state = ShardStopState::Draining;
            }
            self.maybe_log_stop_drain_progress("sqpoll");

            #[cfg(debug_assertions)]
            let phase_start = now_monotonic_ns();
            let latency_cqes = self.process_completions(RingKind::Latency);
            cqe_overflow_backpressured = self.update_cqe_overflow_state(now_monotonic_ns());
            if self.advance_provided_recv_pool_lifecycle().is_err() {
                return ShardPumpStep::Stop;
            }
            self.advance_deferred_close_fds();
            #[cfg(debug_assertions)]
            self.debug_phase_stats
                .record_phase(LoopPhase::CqeLatency, phase_start);
            self.tick_stats_logging();
            let completed_cqes = latency_cqes;
            if completed_cqes > 0 {
                self.result_reduce_cqes = self.result_reduce_cqes.saturating_add(completed_cqes);
                let outcome = self.maybe_schedule_reduce_results(ResultReduceTrigger::Cqe);
                self.enqueue_result_callback_outcome(outcome);
            }

            let latency_drained_after_check = match self.drain_latency_queue() {
                Ok(n) => n,
                Err(_) => return ShardPumpStep::Stop,
            };
            if self.is_stopping() && self.is_idle_for_teardown() {
                return ShardPumpStep::Stop;
            }

            #[cfg(debug_assertions)]
            let phase_start = now_monotonic_ns();
            let outcome = self.maybe_schedule_reduce_results(ResultReduceTrigger::Age);
            self.enqueue_result_callback_outcome(outcome);
            #[cfg(debug_assertions)]
            self.debug_phase_stats
                .record_phase(LoopPhase::ReduceAge, phase_start);

            #[cfg(debug_assertions)]
            let phase_start = now_monotonic_ns();
            let ready_work = match self.drain_ready() {
                Ok(n) => n,
                Err(_) => return ShardPumpStep::Stop,
            };
            #[cfg(debug_assertions)]
            self.debug_phase_stats
                .record_phase(LoopPhase::DrainReady, phase_start);

            let queued_sqes = if cqe_overflow_backpressured {
                0
            } else {
                self.queued_sqes()
            };
            // In SQPOLL mode we still need to submit queued SQEs promptly;
            // deferring submit behind pressure/no-jobs causes tail stalls.
            let should_submit = queued_sqes > 0;

            if should_submit {
                #[cfg(debug_assertions)]
                let phase_start = now_monotonic_ns();
                if self.submit_queued().is_err() {
                    return ShardPumpStep::Stop;
                }
                #[cfg(debug_assertions)]
                self.debug_phase_stats
                    .record_phase(LoopPhase::SubmitQueued, phase_start);
                self.submit_pressure = false;
            }
            let progressed = latency_drained_before > 0
                || drained_commands > 0
                || latency_drained_after_commands > 0
                || latency_cqes > 0
                || latency_drained_after_check > 0
                || ready_work > 0;
            #[cfg(debug_assertions)]
            self.debug_phase_stats.record_progress(progressed);
            if progressed {
                self.last_progress_ns = now_monotonic_ns();
                return ShardPumpStep::Continue;
            }

            let outcome = self.maybe_schedule_reduce_results(ResultReduceTrigger::Idle);
            if self.enqueue_result_callback_outcome(outcome) {
                self.last_progress_ns = now_monotonic_ns();
                return ShardPumpStep::Continue;
            }

            let has_inflight = self.has_inflight_ops();
            let has_pending_commands = self.control.pending_commands.load(Ordering::Acquire) > 0;
            if self.is_stopping() && self.is_idle_for_teardown() {
                return ShardPumpStep::Stop;
            }
            if has_inflight
                && now_monotonic_ns().saturating_sub(self.last_progress_ns) >= self.idle_wait_ns
            {
                if has_pending_commands || self.has_latency_queue_work() {
                    #[cfg(debug_assertions)]
                    {
                        self.debug_phase_stats.record_stall(StallReason::Spin);
                    }
                    std::hint::spin_loop();
                    return ShardPumpStep::Continue;
                }
                #[cfg(debug_assertions)]
                let phase_start = now_monotonic_ns();
                let wait_result = self.wait_latency_ready_batch();
                #[cfg(debug_assertions)]
                {
                    self.debug_phase_stats
                        .record_phase(LoopPhase::WaitLatency, phase_start);
                    self.debug_phase_stats
                        .record_stall(StallReason::WaitLatency);
                }
                if wait_result.is_err() {
                    return ShardPumpStep::Stop;
                }
                self.last_progress_ns = now_monotonic_ns();
                return ShardPumpStep::Continue;
            }

            #[cfg(debug_assertions)]
            if !has_inflight {
                self.debug_phase_stats
                    .record_stall(StallReason::IdleNoInflight);
            }
            if cqe_overflow_backpressured && has_pending_commands {
                std::hint::spin_loop();
                return ShardPumpStep::Continue;
            }
            #[cfg(debug_assertions)]
            let phase_start = now_monotonic_ns();
            std::hint::spin_loop();
            #[cfg(debug_assertions)]
            {
                self.debug_phase_stats
                    .record_phase(LoopPhase::Spin, phase_start);
                self.debug_phase_stats.record_stall(StallReason::Spin);
            }
            return ShardPumpStep::Continue;
        }
    }

    #[inline]
    fn cqe_overflow_backoff_ns(&self) -> u64 {
        self.idle_wait_ns
            .saturating_mul(CQE_OVERFLOW_BACKOFF_MULTIPLIER)
    }

    #[inline]
    fn tick_stats_logging(&mut self) {
        self.maybe_log_stats();
    }

    #[inline]
    fn observe_cqe_overflow_delta(
        overflow_now: u32,
        last_overflow: &mut u32,
        total_overflow: &mut u64,
    ) -> u64 {
        let delta = overflow_now.wrapping_sub(*last_overflow);
        *last_overflow = overflow_now;
        if delta == 0 {
            return 0;
        }
        let delta = u64::from(delta);
        *total_overflow = total_overflow.saturating_add(delta);
        delta
    }

    #[inline]
    fn update_cqe_overflow_state(&mut self, now_ns: u64) -> bool {
        let latency_overflow = self.latency_ring.completion().overflow();
        let latency_overflow_delta = Self::observe_cqe_overflow_delta(
            latency_overflow,
            &mut self.stats.last_latency_cq_overflow,
            &mut self.stats.latency_cq_overflow,
        );

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let overflow_delta = {
            let main_overflow = self.main_ring.completion().overflow();
            latency_overflow_delta.saturating_add(Self::observe_cqe_overflow_delta(
                main_overflow,
                &mut self.stats.last_main_cq_overflow,
                &mut self.stats.main_cq_overflow,
            ))
        };
        #[cfg(feature = "exec-strategy-sqpoll")]
        let overflow_delta = latency_overflow_delta;

        if overflow_delta > 0 {
            self.stats.cqe_overflow_events = self
                .stats
                .cqe_overflow_events
                .saturating_add(overflow_delta);
            let backoff_until = now_ns.saturating_add(self.cqe_overflow_backoff_ns());
            if backoff_until > self.stats.cqe_overflow_backoff_until_ns {
                self.stats.cqe_overflow_backoff_until_ns = backoff_until;
            }
        }

        self.stats.cqe_overflow_backoff_until_ns > now_ns
    }

    #[inline]
    fn request_stop(&mut self) {
        if matches!(self.stop_state, ShardStopState::Running) {
            // Stop ordering is intentionally staged:
            // 1. mark the shard as stopping and start draining shared pools
            // 2. let in-flight receives/writes resolve through their CQE paths
            // 3. only then let the shard become teardown-idle and exit the loop
            self.stop_state = ShardStopState::StopPending;
            self.stop_drain_snapshot = None;
            self.stop_drain_tick = 0;
            if let Some(pool) = self.provided_recv_pool.as_mut() {
                pool.begin_draining();
            }
            let listener_ids = self.listeners.keys();
            let recv_sessions = self
                .conns
                .iter()
                .filter_map(|(session_id, conn)| {
                    (conn.read_in_flight && conn.read_in_flight_token.is_some())
                        .then_some(*session_id)
                })
                .collect::<Vec<_>>();
            for listener_id in listener_ids {
                self.close_listener(listener_id);
            }
            for session_id in recv_sessions {
                let deferred_close = self.cancel_conn_recv(session_id).is_ok();
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    conn.stop_close_on_recv_cqe = deferred_close;
                }
            }
            let write_sessions = self
                .conns
                .iter()
                .filter_map(|(session_id, conn)| conn.write_in_flight.then_some(*session_id))
                .collect::<Vec<_>>();
            for session_id in write_sessions {
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    conn.stop_close_on_write_cqe = true;
                }
            }
            let read_poll_sessions = self
                .conns
                .iter()
                .filter_map(|(session_id, conn)| conn.read_poll_armed.then_some(*session_id))
                .collect::<Vec<_>>();
            for session_id in read_poll_sessions {
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    conn.stop_close_on_read_poll_cqe = true;
                }
            }
            let write_poll_sessions = self
                .conns
                .iter()
                .filter_map(|(session_id, conn)| conn.write_poll_armed.then_some(*session_id))
                .collect::<Vec<_>>();
            for session_id in write_poll_sessions {
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    conn.stop_close_on_write_poll_cqe = true;
                }
            }
            let session_ids = self.conns.keys();
            for session_id in session_ids {
                let stop_close_pending = self
                    .conns
                    .get(&session_id)
                    .map(|conn| {
                        conn.stop_close_on_recv_cqe
                            || conn.stop_close_on_read_poll_cqe
                            || conn.stop_close_on_write_cqe
                            || conn.stop_close_on_write_poll_cqe
                    })
                    .unwrap_or(false);
                if !stop_close_pending {
                    self.close_connection(session_id, NifError::Closed);
                }
            }
            let subscription_ids = self.subscriptions.keys();
            for subscription_id in subscription_ids {
                self.request_stop_subscription(subscription_id);
            }
        }
    }

    #[inline]
    fn is_stopping(&self) -> bool {
        !matches!(self.stop_state, ShardStopState::Running)
    }

    #[inline]
    fn stop_drain_inflight_ops(&self) -> usize {
        let latency_inflight = self
            .latency_ops
            .values()
            .filter(|op| !matches!(op, Op::Wakeup))
            .count();
        latency_inflight + self.main_ops.len()
    }

    fn stop_drain_snapshot(&self) -> StopDrainSnapshot {
        StopDrainSnapshot {
            listeners: self.listeners.len(),
            conns: self.conns.len(),
            subscriptions: self.subscriptions.len(),
            inflight_ops: self.stop_drain_inflight_ops(),
        }
    }

    fn maybe_log_stop_drain_progress(&mut self, mode: &'static str) {
        if !super::stop_drain_profile_enabled() {
            return;
        }
        if !self.is_stopping() {
            return;
        }
        let snapshot = self.stop_drain_snapshot();
        let changed = self.stop_drain_snapshot != Some(snapshot);
        self.stop_drain_tick = self.stop_drain_tick.saturating_add(1);
        if !changed && !self.stop_drain_tick.is_multiple_of(STOP_DRAIN_LOG_EVERY) {
            return;
        }
        self.stop_drain_snapshot = Some(snapshot);
        let stop_state = match self.stop_state {
            ShardStopState::Running => "running",
            ShardStopState::StopPending => "stop_pending",
            ShardStopState::Draining => "draining",
        };
        eprintln!(
            "{} shard={} mode={} stop_state={} listeners={} conns={} subscriptions={} inflight_ops={}",
            stats_labels::LOOP,
            self.shard,
            mode,
            stop_state,
            snapshot.listeners,
            snapshot.conns,
            snapshot.subscriptions,
            snapshot.inflight_ops,
        );
    }

    fn maybe_log_stop_profile_summary(&mut self) {
        if !super::stop_drain_profile_enabled() || !self.is_stopping() {
            return;
        }
        let snapshot = self.stop_drain_snapshot();
        eprintln!(
            "{} shard={} stop=summary stop_state={} listeners={} conns={} subscriptions={} inflight_ops={} cancel_recv_cqes={} cancel_listener_accept_cqes={} close_fd_cqes={}",
            stats_labels::LOOP,
            self.shard,
            match self.stop_state {
                ShardStopState::Running => "running",
                ShardStopState::StopPending => "stop_pending",
                ShardStopState::Draining => "draining",
            },
            snapshot.listeners,
            snapshot.conns,
            snapshot.subscriptions,
            snapshot.inflight_ops,
            self.stats.cancel_recv_cqes,
            self.stats.cancel_listener_accept_cqes,
            self.stats.close_fd_cqes
        );
    }

    #[inline]
    fn close_listener(&mut self, listener_id: u64) {
        if let Some(listener) = self.listeners.remove(&listener_id) {
            if let Some(token) = listener.accept_token {
                let _ = self.cancel_listener_accept(token);
            }
            self.close_fd_or_defer(listener.fd);
        }
    }

    fn has_inflight_ops(&self) -> bool {
        self.has_latency_io_work() || !self.main_ops.is_empty()
    }

    fn is_idle_for_teardown(&self) -> bool {
        let control_backlog = self.control.pending_commands.load(Ordering::Acquire) > 0;
        let fixed_teardown_ready = self.stop_provided_recv_pool_teardown_ready();
        // The shard is only teardown-idle after live resources are gone and all
        // deferred-close bookkeeping has been flushed.
        !control_backlog
            && self.listeners.is_empty()
            && self.conns.is_empty()
            && self.subscriptions.is_empty()
            && self.deferred_close_fds.is_empty()
            && !self.local.has_work()
            && !self.has_callback_work()
            && !self.has_latency_queue_work()
            && !self.has_inflight_ops()
            && fixed_teardown_ready
    }

    #[inline]
    fn advance_deferred_close_fds(&mut self) {
        // First try to hand deferred closes back to the normal close path so the
        // fd is retired only after the connection has already been detached.
        while let Some(fd) = self.deferred_close_fds.front().copied() {
            if self.enqueue_close_fd(fd).is_err() {
                break;
            }
            let _ = self.deferred_close_fds.pop_front();
        }
        // If no inflight ops remain, the fallback close is safe because nothing
        // can still observe the fd through shard-owned queues.
        if !self.has_inflight_ops() {
            while let Some(fd) = self.deferred_close_fds.pop_front() {
                // SAFETY: `deferred_close_fds` is populated only by ownership-transfer
                // close paths, so each raw fd here is still owned by this shard and
                // is closed at most once on this final fallback path.
                unsafe { libc::close(fd) };
            }
        }
    }

    #[inline]
    fn stop_provided_recv_pool_remove_ready(&self) -> bool {
        !self.is_stopping()
            || (self.listeners.is_empty() && self.conns.is_empty() && self.subscriptions.is_empty())
    }

    #[inline]
    fn stop_provided_recv_pool_teardown_ready(&self) -> bool {
        self.provided_recv_pool
            .as_ref()
            .map(|pool| pool.is_teardown_complete())
            .unwrap_or(true)
    }

    #[inline]
    pub(crate) fn maybe_release_provided_recv_pool(&mut self) {
        let should_release = self
            .provided_recv_pool
            .as_ref()
            .is_some_and(|pool| pool.is_teardown_complete());
        if should_release {
            let _ = self.provided_recv_pool.take();
        }
    }

    #[inline]
    fn fixed_buffer_unregister_stop_line_ready(&self) -> bool {
        self.listeners.is_empty()
            && self.conns.is_empty()
            && self.subscriptions.is_empty()
            && self.stop_provided_recv_pool_teardown_ready()
            && !self.latency_ops.values().any(|op| {
                matches!(
                    op,
                    crate::runtime_reactor::Op::ConnRecv(_)
                        | crate::runtime_reactor::Op::ConnWrite(_)
                        | crate::runtime_reactor::Op::ConnReadPoll(_)
                        | crate::runtime_reactor::Op::ConnWritePoll(_)
                        | crate::runtime_reactor::Op::SubscribeRead { .. }
                        | crate::runtime_reactor::Op::SubscribeAccept { .. }
                )
            })
            && {
                #[cfg(not(feature = "exec-strategy-sqpoll"))]
                {
                    !self.main_ops.values().any(|op| {
                        matches!(
                            op,
                            crate::runtime_reactor::Op::ConnWrite(_)
                                | crate::runtime_reactor::Op::ConnWritePoll(_)
                        )
                    })
                }
                #[cfg(feature = "exec-strategy-sqpoll")]
                {
                    true
                }
            }
    }

    #[inline]
    fn advance_provided_recv_pool_lifecycle(&mut self) -> Result<()> {
        let mut should_queue_remove = false;
        let remove_ready = self.stop_provided_recv_pool_remove_ready();
        if let Some(pool) = self.provided_recv_pool.as_mut() {
            let _ = pool.advance_retirement();
            if remove_ready && pool.is_draining() {
                let _ = pool.retire_for_remove();
            }
            debug_assert!(!pool.should_queue_remove() || (pool.is_removed() && remove_ready));
            should_queue_remove = pool.should_queue_remove();
        }
        if should_queue_remove && remove_ready {
            let _ = self.enqueue_provided_recv_pool_remove()?;
        }
        Ok(())
    }

    fn can_unregister_fixed_buffers_on_exit(&self) -> bool {
        if !self.read_fixed_enabled && !self.subscribe_fixed_enabled {
            return false;
        }
        if self.conns.has_live_io() {
            return false;
        }
        self.fixed_buffer_unregister_stop_line_ready()
    }

    fn clear_fixed_buffer_registration_state(&mut self) {
        self.read_fixed_enabled = false;
        self.read_fixed_slots.clear();
        self.subscribe_fixed_enabled = false;
        self.subscribe_fixed_base = None;
        self.subscribe_fixed_slots.clear();
    }

    fn try_unregister_fixed_buffers_on_exit(&mut self) -> Result<()> {
        if !self.can_unregister_fixed_buffers_on_exit() {
            return Ok(());
        }
        debug_assert!(self.fixed_buffer_unregister_stop_line_ready());
        self.latency_ring
            .submitter()
            .unregister_buffers()
            .map_err(|e| NifError::from_errno(e.raw_os_error().unwrap_or(libc::EIO)))?;
        self.clear_fixed_buffer_registration_state();
        self.maybe_release_provided_recv_pool();
        Ok(())
    }

    // LinkTimeout policy is DEFERRED: per-op deadlines via LinkTimeout are not implemented.
    // The min_wait_usec path below is the only timeout mechanism currently used.
    // See DP-NX-0087..DP-NX-0095 conditional chain: LinkTimeout would require wire semantics changes.
    #[cfg(feature = "exec-strategy-sqpoll")]
    pub(crate) fn wait_latency_ready_batch(&mut self) -> Result<()> {
        let want = self.sqpoll_idle_batch.min(self.latency_ops.len().max(1));
        if self.latency_ring.params().is_feature_min_timeout() {
            let args = types::SubmitArgs::new().min_wait_usec(self.sqpoll_idle_usec);
            retry_eintr(|| self.latency_ring.submitter().submit_with_args(want, &args))?;
        } else {
            retry_eintr(|| self.latency_ring.submitter().submit_and_wait(want))?;
        }
        Ok(())
    }

    // LinkTimeout policy is DEFERRED: per-op deadlines via LinkTimeout are not implemented.
    // The min_wait_usec path below is the only timeout mechanism currently used.
    // See DP-NX-0087..DP-NX-0095 conditional chain: LinkTimeout would require wire semantics changes.
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(crate) fn wait_main_ready_batch(&mut self) -> Result<usize> {
        let want = self.sqpoll_idle_batch.min(self.main_ops.len().max(1));
        if self.main_ring.params().is_feature_min_timeout() {
            let args = io_uring::types::SubmitArgs::new().min_wait_usec(self.sqpoll_idle_usec);
            retry_eintr(|| self.main_ring.submitter().submit_with_args(want, &args))
        } else {
            retry_eintr(|| self.main_ring.submitter().submit_and_wait(want))
        }
    }

    pub(crate) fn has_latency_queue_work(&self) -> bool {
        self.latency_queue_len != 0
    }

    pub(crate) fn enqueue_latency_command(
        &mut self,
        cmd: Command,
        cqe_ref: Option<CqeRef>,
        soft_link: bool,
    ) {
        if self.latency_queue_len < LATENCY_QUEUE_CAP {
            self.latency_queue[self.latency_queue_len] = Some(LatencyItem::Command {
                cmd,
                cqe_ref,
                soft_link,
            });
            self.latency_queue_len += 1;
            return;
        }
        self.handle_command(cmd, soft_link);
    }

    pub(crate) fn enqueue_latency_task(&mut self, op: ReadyOp) {
        if self.latency_queue_len < LATENCY_QUEUE_CAP {
            self.latency_queue[self.latency_queue_len] = Some(LatencyItem::Task(op));
            self.latency_queue_len += 1;
            return;
        }
        self.local.push_ready(op);
    }

    pub(crate) fn drain_latency_queue(&mut self) -> Result<usize> {
        let mut drained = 0usize;
        while self.latency_queue_len > 0 {
            let batch_len = self.latency_queue_len;
            let mut batch: [Option<LatencyItem>; LATENCY_QUEUE_CAP] = std::array::from_fn(|_| None);
            for (dst, src) in batch
                .iter_mut()
                .take(batch_len)
                .zip(self.latency_queue.iter_mut())
            {
                *dst = src.take();
            }
            self.latency_queue_len = 0;

            for item in batch.iter_mut().take(batch_len) {
                if matches!(item, Some(LatencyItem::Command { .. })) {
                    if let Some(LatencyItem::Command {
                        cmd,
                        cqe_ref,
                        soft_link,
                    }) = item.take()
                    {
                        let _ = cqe_ref;
                        self.handle_command(cmd, soft_link);
                        drained += 1;
                    }
                }
            }

            for item in batch.into_iter().take(batch_len) {
                if let Some(LatencyItem::Task(op)) = item {
                    self.execute_ready_op(op)?;
                    drained += 1;
                }
            }
        }
        Ok(drained)
    }

    pub(crate) fn poll_commands(&mut self) -> usize {
        self.drain_commands()
    }

    pub(crate) fn drain_commands(&mut self) -> usize {
        let mut drained = 0usize;
        let mut deferred_submit = std::mem::take(&mut self.deferred_submit_buf);
        let mut deferred_exec = std::mem::take(&mut self.deferred_exec_buf);
        let mut deferred_commands = std::mem::take(&mut self.deferred_command_buf);
        deferred_submit.clear();
        deferred_exec.clear();
        while drained < COMMAND_BUDGET {
            let mut batch = if deferred_commands.is_empty() {
                match ingress_try_recv(&self.rx) {
                    Some(batch) => batch.into_iter().collect::<VecDeque<_>>(),
                    None => break,
                }
            } else {
                std::mem::take(&mut deferred_commands)
            };
            while drained < COMMAND_BUDGET {
                let Some(cmd) = batch.pop_front() else {
                    break;
                };
                let (link, cmd) = cmd.unwrap_linked();
                let _reactor_class = cmd.reactor_class();
                let lane = Self::command_lane(&cmd);
                if link.is_some() && matches!(lane, CommandLane::Exec) {
                    if !deferred_submit.is_empty() || !deferred_exec.is_empty() {
                        self.flush_deferred_lanes(&mut deferred_submit, &mut deferred_exec);
                    }
                    self.enqueue_latency_command(cmd, None, false);
                } else {
                    match lane {
                        CommandLane::Submit => {
                            if link.is_some() && !deferred_exec.is_empty() {
                                self.flush_deferred_lanes(&mut deferred_submit, &mut deferred_exec);
                            }
                            if link.is_some() && !deferred_submit.is_empty() {
                                let compatible_link_chain = deferred_submit
                                    .last()
                                    .and_then(|entry| entry.link)
                                    .zip(link)
                                    .map(|(left, right)| left == right)
                                    .unwrap_or(false);
                                if !compatible_link_chain {
                                    self.flush_deferred_lanes(
                                        &mut deferred_submit,
                                        &mut deferred_exec,
                                    );
                                }
                            }
                            if deferred_submit.len() == DEFERRED_SUBMIT_CAP {
                                self.flush_deferred_lanes(&mut deferred_submit, &mut deferred_exec);
                            }
                            deferred_submit.push(DeferredSubmitCommand { link, cmd });
                        }
                        CommandLane::Exec => {
                            if deferred_exec.len() == DEFERRED_EXEC_CAP {
                                self.flush_deferred_lanes(&mut deferred_submit, &mut deferred_exec);
                            }
                            deferred_exec.push(cmd);
                        }
                    }
                }
                drained += 1;
                if drained == COMMAND_BUDGET {
                    deferred_commands.extend(batch);
                    break;
                }
            }
        }
        if !deferred_submit.is_empty() || !deferred_exec.is_empty() {
            self.flush_deferred_lanes(&mut deferred_submit, &mut deferred_exec);
        }
        self.deferred_submit_buf = deferred_submit;
        self.deferred_exec_buf = deferred_exec;
        self.deferred_command_buf = deferred_commands;
        if drained > 0 {
            stats_add!(self.stats, commands_drained, drained as u64);
            self.control
                .pending_commands
                .fetch_sub(drained, Ordering::AcqRel);
        }
        drained
    }

    pub(super) fn command_lane(cmd: &Command) -> CommandLane {
        match cmd {
            Command::Linked { .. } => {
                debug_assert!(false, "linked commands are unwrapped before laneing");
                CommandLane::Submit
            }
            Command::InstallListener { .. }
            | Command::RecvAsync { .. }
            | Command::RecvSync { .. }
            | Command::Subscribe { .. }
            | Command::SubscribeControl { .. }
            | Command::SendAsync { .. }
            | Command::SendEnqueue { .. }
            | Command::BatchAsync { .. }
            | Command::StatAsync { .. } => CommandLane::Submit,

            Command::SubscribeAddConsumer { .. }
            | Command::UpdateOwner { .. }
            | Command::OwnerDown { .. }
            | Command::SetMailboxPassiveEnqueue { .. }
            | Command::SetNoDelayAsync { .. }
            | Command::SetNoDelayEnqueue { .. }
            | Command::SetNoDelay { .. } => CommandLane::Exec,

            Command::CloseListener { .. }
            | Command::CancelRecv { .. }
            | Command::SetActiveAsync { .. }
            | Command::SetActiveEnqueue { .. }
            | Command::SetActive { .. }
            | Command::ShutdownAsync { .. }
            | Command::ShutdownEnqueue { .. }
            | Command::Shutdown { .. }
            | Command::Stop
            | Command::CloseAsync { .. }
            | Command::CloseEnqueue { .. }
            | Command::Close { .. }
            | Command::GetStats { .. }
            | Command::NoopAsync { .. } => CommandLane::Exec,
        }
    }

    pub(super) fn flush_deferred_lanes(
        &mut self,
        deferred_submit: &mut Vec<DeferredSubmitCommand>,
        deferred_exec: &mut Vec<Command>,
    ) {
        if !deferred_submit.is_empty() {
            let submit_len = deferred_submit.len();
            let mut soft_links = [false; DEFERRED_SUBMIT_CAP];
            #[cfg(feature = "exec-strategy-sqpoll")]
            let allow_soft_links = true;
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            let allow_soft_links = matches!(self.recv_ring_mode, RecvRingMode::Latency);
            if allow_soft_links {
                for idx in 0..submit_len.saturating_sub(1) {
                    soft_links[idx] = deferred_submit[idx]
                        .link
                        .zip(deferred_submit[idx + 1].link)
                        .map(|(left, right)| left == right)
                        .unwrap_or(false);
                }
            }
            for (idx, entry) in deferred_submit.drain(..).enumerate() {
                self.enqueue_latency_command(entry.cmd, None, soft_links[idx]);
            }
        }
        if !deferred_exec.is_empty() {
            for cmd in deferred_exec.drain(..) {
                self.enqueue_latency_command(cmd, None, false);
            }
        }
    }

    pub(crate) fn handle_command(&mut self, cmd: Command, soft_link: bool) {
        let prev_soft_link = self.pending_soft_link_sqe;
        self.pending_soft_link_sqe = soft_link;
        self.handle_command_inner(cmd);
        self.pending_soft_link_sqe = prev_soft_link;
    }

    pub(crate) fn handle_command_inner(&mut self, cmd: Command) {
        match cmd {
            Command::Linked { cmd, .. } => {
                debug_assert!(false, "linked commands are unwrapped before handling");
                self.handle_command_inner(*cmd);
            }
            Command::Stop => {
                self.request_stop();
            }
            Command::InstallListener {
                listener_id,
                fd,
                accept_tx,
                owner,
                reply,
            } => {
                self.listeners.insert(
                    listener_id,
                    ListenerShard {
                        fd,
                        accept_tx,
                        owner,
                        accept_armed: false,
                        accept_token: None,
                    },
                );
                let result = self.arm_listener_accept(listener_id);
                let _ = reply.send(result);
            }
            Command::CloseListener { listener_id, reply } => {
                self.close_listener(listener_id);
                let _ = reply.send(Ok(()));
            }
            Command::RecvAsync {
                session_id,
                len,
                request_id,
                enqueue_raw,
                target,
            } => {
                self.observe_recv_ingress_wait(enqueue_raw);
                let mut target = Some(target);
                let mut immediate_data = None;
                let mut immediate_error = None;
                let mut should_ready = false;
                let mut request_arm = false;
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    let decision = Self::classify_recv_decision(
                        conn.pending_batch.is_some(),
                        conn.pending_recv.is_some(),
                        conn.rx_bytes,
                        len,
                    );
                    match decision {
                        RecvDecision::Busy => {
                            immediate_error = Some(NifError::from_errno(libc::EBUSY));
                        }
                        RecvDecision::Immediate(n) => {
                            let out = rx_take(conn, &mut self.arenas, n);
                            should_ready = conn.rx_bytes < RX_QUEUE_MAX_BYTES;
                            immediate_data = Some(out);
                        }
                        RecvDecision::Pending => {
                            conn.pending_recv = Some(PendingRecv {
                                len,
                                enqueued_raw: recv_clock_raw(),
                                reply: PendingRecvReply::Async {
                                    request_id,
                                    target: match target.take() {
                                        Some(target) => target,
                                        None => return,
                                    },
                                },
                            });
                            request_arm = true;
                        }
                    }
                } else {
                    immediate_error = Some(NifError::Closed);
                }
                if request_arm {
                    self.try_arm_recv_inline_or_fallback(session_id);
                }
                if let Some(data) = immediate_data {
                    if let Some(target) = target.take() {
                        self.queue_reply_data(target, request_id, data);
                    }
                    if should_ready {
                        self.try_arm_recv_inline_or_fallback(session_id);
                    }
                } else if let (Some(err), Some(target)) = (immediate_error, target.take()) {
                    self.queue_reply_error(target, request_id, err);
                }
            }
            Command::RecvSync {
                session_id,
                len,
                enqueue_raw,
                reply,
            } => {
                self.observe_recv_sync_ingress_wait(enqueue_raw);
                let mut request_arm = false;
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    let decision = Self::classify_recv_decision(
                        conn.pending_batch.is_some(),
                        conn.pending_recv.is_some(),
                        conn.rx_bytes,
                        len,
                    );
                    match decision {
                        RecvDecision::Busy => {
                            let _ = reply.send(Err(NifError::from_errno(libc::EBUSY)));
                        }
                        RecvDecision::Immediate(n) => {
                            let out = rx_take(conn, &mut self.arenas, n);
                            let _ = reply.send(Ok(out));
                            if conn.rx_bytes < RX_QUEUE_MAX_BYTES {
                                request_arm = true;
                            }
                        }
                        RecvDecision::Pending => {
                            conn.pending_recv = Some(PendingRecv {
                                len,
                                enqueued_raw: recv_clock_raw(),
                                reply: PendingRecvReply::Sync(reply),
                            });
                            request_arm = true;
                        }
                    }
                } else {
                    let _ = reply.send(Err(NifError::Closed));
                }
                if request_arm {
                    self.try_arm_recv_inline_or_fallback(session_id);
                }
            }
            Command::CancelRecv {
                session_id,
                timed_out,
            } => self.cancel_recv(session_id, timed_out),
            Command::Subscribe {
                subscription_id,
                target,
                operation,
                fd,
            } => {
                if let Err(err) =
                    self.install_subscription(subscription_id, target.clone(), operation, fd)
                {
                    self.queue_reply_error(target, subscription_id, err);
                }
            }
            Command::SubscribeControl {
                subscription_id,
                control,
            } => match control {
                SubscribeControl::Choke => {
                    if let Some(sub) = self.subscriptions.get_mut(&subscription_id) {
                        sub.choked = true;
                    }
                }
                SubscribeControl::Unchoke => {
                    if let Some(sub) = self.subscriptions.get_mut(&subscription_id) {
                        sub.choked = false;
                    }
                    let _ = self.fill_subscription_inflight(subscription_id);
                }
                SubscribeControl::Stop => {
                    self.request_stop_subscription(subscription_id);
                }
            },
            Command::SubscribeAddConsumer {
                subscription_id,
                target,
            } => {
                let _ = self.add_subscription_consumer(subscription_id, target);
            }
            Command::SendAsync {
                session_id,
                data,
                request_id,
                target,
                sync_enqueue_raw,
            } => {
                if let Some(enqueue_raw) = sync_enqueue_raw {
                    self.observe_send_sync_ingress_wait(enqueue_raw);
                }
                let data_len = data.len();
                let Some(current_tx) = self.conns.get(&session_id).map(|c| c.tx_bytes) else {
                    self.queue_reply_error(target, request_id, NifError::Closed);
                    return;
                };
                if current_tx + data_len > TX_QUEUE_MAX_BYTES {
                    self.queue_reply_error(target, request_id, NifError::from_errno(libc::EAGAIN));
                } else {
                    let pending = self.make_pending_write(data);
                    if let Some(conn) = self.conns.get_mut(&session_id) {
                        conn.tx_bytes += data_len;
                        stats_max!(self.stats, tx_queue_peak, conn.tx_bytes);
                        conn.pending_writes.push_back(pending);
                        self.enqueue_write_ready(session_id);
                        self.queue_reply_ok(target, request_id);
                    } else {
                        self.queue_reply_error(target, request_id, NifError::Closed);
                    }
                }
            }
            Command::SendEnqueue { session_id, data } => {
                let data_len = data.len();
                let Some(_) = self.conns.get(&session_id).map(|c| c.tx_bytes) else {
                    return;
                };
                let pending = self.make_pending_write(data);
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    conn.tx_bytes = conn.tx_bytes.saturating_add(data_len);
                    stats_max!(self.stats, tx_queue_peak, conn.tx_bytes);
                    conn.pending_writes.push_back(pending);
                    self.enqueue_write_ready(session_id);
                }
            }
            Command::BatchAsync {
                session_id,
                request_id,
                target,
                ops,
            } => {
                if !self.start_or_run_batch(session_id, request_id, target.clone(), ops) {
                    send_batch_reply_to(
                        &target,
                        request_id,
                        vec![(0, BatchResult::Error(NifError::Closed))],
                    );
                }
            }
            Command::SetActiveAsync {
                session_id,
                mode,
                request_id,
                target,
            } => {
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    conn.active_mode = mode;
                    self.deliver_queued_active(session_id);
                    self.local.push_ready(ReadyOp::Conn(session_id));
                    self.queue_reply_ok(target, request_id);
                } else {
                    self.queue_reply_error(target, request_id, NifError::Closed);
                }
            }
            Command::SetActiveEnqueue { session_id, mode } => {
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    conn.active_mode = mode;
                    self.deliver_queued_active(session_id);
                    self.local.push_ready(ReadyOp::Conn(session_id));
                }
            }
            Command::SetActive {
                session_id,
                mode,
                reply,
            } => {
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    conn.active_mode = mode;
                    self.deliver_queued_active(session_id);
                    self.local.push_ready(ReadyOp::Conn(session_id));
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err(NifError::Closed));
                }
            }
            Command::SetNoDelayAsync {
                session_id,
                enabled,
                request_id,
                target,
            } => {
                if let Some(conn) = self.conns.get(&session_id) {
                    let optval: libc::c_int = enabled as libc::c_int;
                    // SAFETY: `conn.fd` is a live socket owned by this shard while the
                    // connection entry exists; `optval` points to a valid `c_int` with
                    // the exact length passed to `setsockopt`.
                    let ret = unsafe {
                        libc::setsockopt(
                            conn.fd.as_raw_fd(),
                            libc::IPPROTO_TCP,
                            libc::TCP_NODELAY,
                            &optval as *const libc::c_int as *const libc::c_void,
                            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                        )
                    };
                    if ret == 0 {
                        if let SocketKind::Session(session) = &conn.handle.kind {
                            session
                                .opts
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .nodelay = enabled;
                        }
                        self.queue_reply_ok(target, request_id);
                    } else {
                        self.queue_reply_error(target, request_id, NifError::last_os_error());
                    }
                } else {
                    self.queue_reply_error(target, request_id, NifError::Closed);
                }
            }
            Command::SetNoDelayEnqueue {
                session_id,
                enabled,
            } => {
                if let Some(conn) = self.conns.get(&session_id) {
                    let optval: libc::c_int = enabled as libc::c_int;
                    // SAFETY: `conn.fd` is a live socket owned by this shard while the
                    // connection entry exists; `optval` points to a valid `c_int` with
                    // the exact length passed to `setsockopt`.
                    let ret = unsafe {
                        libc::setsockopt(
                            conn.fd.as_raw_fd(),
                            libc::IPPROTO_TCP,
                            libc::TCP_NODELAY,
                            &optval as *const libc::c_int as *const libc::c_void,
                            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                        )
                    };
                    if ret == 0 {
                        if let SocketKind::Session(session) = &conn.handle.kind {
                            session
                                .opts
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .nodelay = enabled;
                        }
                    }
                }
            }
            Command::SetMailboxPassiveEnqueue {
                session_id,
                enabled,
            } => {
                if let Some(conn) = self.conns.get(&session_id) {
                    if let SocketKind::Session(session) = &conn.handle.kind {
                        session.mailbox_passive.store(enabled, Ordering::Release);
                    }
                }
                if !enabled {
                    // Keep the cached CQE snapshot aligned with the live passive flag.
                    // This is local snapshot cleanup only and does not alter teardown.
                    self.clear_recv_mailbox_snapshot(session_id);
                }
                if enabled {
                    self.local.push_ready(ReadyOp::Conn(session_id));
                }
            }
            Command::SetNoDelay {
                session_id,
                enabled,
                reply,
            } => {
                if let Some(conn) = self.conns.get(&session_id) {
                    let optval: libc::c_int = enabled as libc::c_int;
                    // SAFETY: `conn.fd` is a live socket owned by this shard while the
                    // connection entry exists; `optval` points to a valid `c_int` with
                    // the exact length passed to `setsockopt`.
                    let ret = unsafe {
                        libc::setsockopt(
                            conn.fd.as_raw_fd(),
                            libc::IPPROTO_TCP,
                            libc::TCP_NODELAY,
                            &optval as *const libc::c_int as *const libc::c_void,
                            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                        )
                    };
                    if ret == 0 {
                        if let SocketKind::Session(session) = &conn.handle.kind {
                            session
                                .opts
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .nodelay = enabled;
                        }
                        let _ = reply.send(Ok(()));
                    } else {
                        let _ = reply.send(Err(NifError::last_os_error()));
                    }
                } else {
                    let _ = reply.send(Err(NifError::Closed));
                }
            }
            Command::UpdateOwner {
                session_id,
                pid,
                reply,
            } => {
                let stopping = self.is_stopping();
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    let is_closed = matches!(
                        &conn.handle.kind,
                        SocketKind::Session(session) if session.is_closed()
                    );
                    if is_closed || stopping {
                        let _ = reply.send(Err(NifError::Closed));
                    } else {
                        conn.owner = pid;
                        let _ = reply.send(Ok(()));
                    }
                } else {
                    let _ = reply.send(Err(NifError::Closed));
                }
            }
            Command::OwnerDown { session_id } => {
                self.begin_close_connection(session_id, NifError::Closed);
            }
            Command::ShutdownAsync {
                session_id,
                how,
                request_id,
                target,
            } => {
                if let Some(conn) = self.conns.get(&session_id) {
                    match shutdown_kind_from_how(how) {
                        Some(kind) => {
                            let sock_ref = socket2::SockRef::from(&conn.fd);
                            if sock_ref.shutdown(kind).is_ok() {
                                self.queue_reply_ok(target, request_id);
                            } else {
                                self.queue_reply_error(
                                    target,
                                    request_id,
                                    NifError::last_os_error(),
                                );
                            }
                        }
                        None => {
                            self.queue_reply_error(
                                target,
                                request_id,
                                NifError::from_errno(libc::EINVAL),
                            );
                        }
                    }
                } else {
                    self.queue_reply_error(target, request_id, NifError::Closed);
                }
            }
            Command::ShutdownEnqueue { session_id, how } => {
                if let Some(conn) = self.conns.get(&session_id) {
                    if let Some(kind) = shutdown_kind_from_how(how) {
                        let sock_ref = socket2::SockRef::from(&conn.fd);
                        let _ = sock_ref.shutdown(kind);
                    }
                }
            }
            Command::Shutdown {
                session_id,
                how,
                reply,
            } => {
                if let Some(conn) = self.conns.get(&session_id) {
                    match shutdown_kind_from_how(how) {
                        Some(kind) => {
                            let sock_ref = socket2::SockRef::from(&conn.fd);
                            if sock_ref.shutdown(kind).is_ok() {
                                let _ = reply.send(Ok(()));
                            } else {
                                let _ = reply.send(Err(NifError::last_os_error()));
                            }
                        }
                        None => {
                            let _ = reply.send(Err(NifError::from_errno(libc::EINVAL)));
                        }
                    }
                } else {
                    let _ = reply.send(Err(NifError::Closed));
                }
            }
            Command::CloseAsync {
                session_id,
                reason,
                request_id,
                target,
            } => {
                self.begin_close_connection(session_id, reason);
                self.queue_reply_ok(target, request_id);
            }
            Command::CloseEnqueue { session_id, reason } => {
                self.begin_close_connection(session_id, reason);
            }
            Command::Close {
                session_id,
                reason,
                reply,
            } => {
                self.begin_close_connection(session_id, reason);
                if let Some(reply) = reply {
                    let _ = reply.send(Ok(()));
                }
            }
            Command::GetStats { reply } => {
                let snapshot = self.stats.snapshot(
                    self.shard,
                    self.conns.len(),
                    self.local.ready_depth(),
                    self.local.write_ready_depth(),
                );
                let _ = reply.send(snapshot);
            }
            Command::NoopAsync { request_id, target } => {
                self.queue_reply_ok(target, request_id);
            }
            Command::StatAsync {
                fd,
                request_id,
                target,
            } => {
                if let Err(err) = self.arm_statx(fd, request_id, target.clone()) {
                    self.queue_reply_error(target, request_id, err);
                }
            }
        }
    }

    pub(crate) fn install_subscription(
        &mut self,
        subscription_id: u64,
        target: ResultTarget,
        operation: SubscribeOperation,
        fd: RawFd,
    ) -> Result<()> {
        if fd < 0 {
            return Err(NifError::from_errno(libc::EINVAL));
        }
        if self.subscriptions.contains(&subscription_id) {
            return Err(NifError::from_errno(libc::EINVAL));
        }

        let mut page_slots: [Option<ArenaHandle>; SUBSCRIBE_INFLIGHT] = [None; SUBSCRIBE_INFLIGHT];
        if matches!(operation, SubscribeOperation::Read) {
            for (idx, slot) in page_slots.iter_mut().enumerate() {
                if let Some(acquired) = self.arenas.acquire_class(ArenaClass::Small4K) {
                    *slot = Some(acquired);
                } else {
                    self.submit_pressure = true;
                    for slot in page_slots.iter().take(idx) {
                        if let Some(handle) = *slot {
                            self.arenas.release(handle);
                        }
                    }
                    return Err(NifError::from_errno(libc::ENOMEM));
                }
            }
        }

        self.subscriptions.insert(
            subscription_id,
            Subscription {
                target: target.clone(),
                accept_consumers: if matches!(operation, SubscribeOperation::Accept) {
                    vec![target]
                } else {
                    Vec::new()
                },
                accept_rr_next: 0,
                operation,
                fd,
                choked: false,
                stopped: false,
                in_flight: 0,
                page_slots,
                slot_in_flight: [false; SUBSCRIBE_INFLIGHT],
            },
        );
        if let Err(err) = self.fill_subscription_inflight(subscription_id) {
            self.request_stop_subscription(subscription_id);
            return Err(err);
        }
        Ok(())
    }

    pub(crate) fn add_subscription_consumer(
        &mut self,
        subscription_id: u64,
        target: ResultTarget,
    ) -> Result<()> {
        let Some(sub) = self.subscriptions.get_mut(&subscription_id) else {
            return Err(NifError::from_errno(libc::EINVAL));
        };
        if !matches!(sub.operation, SubscribeOperation::Accept) {
            return Err(NifError::from_errno(libc::EINVAL));
        }
        sub.accept_consumers.push(target);
        Ok(())
    }

    pub(crate) fn fill_subscription_inflight(&mut self, subscription_id: u64) -> Result<()> {
        loop {
            let Some(subscription) = self.subscriptions.get(&subscription_id) else {
                return Ok(());
            };
            if subscription.choked
                || subscription.stopped
                || subscription.in_flight >= SUBSCRIBE_INFLIGHT
            {
                return Ok(());
            }
            let Some(slot_idx) = subscription.slot_in_flight.iter().position(|busy| !*busy) else {
                return Ok(());
            };
            let fd = subscription.fd;
            let op = subscription.operation;
            let page_slot = subscription.page_slots[slot_idx];
            self.arm_subscription(subscription_id, slot_idx, fd, op, page_slot)?;
            if let Some(sub) = self.subscriptions.get_mut(&subscription_id) {
                if !sub.slot_in_flight[slot_idx] {
                    sub.slot_in_flight[slot_idx] = true;
                    sub.in_flight += 1;
                }
            } else {
                return Ok(());
            }
        }
    }

    pub(crate) fn drop_subscription(&mut self, subscription_id: u64) {
        if let Some(sub) = self.subscriptions.remove(&subscription_id) {
            if matches!(sub.operation, SubscribeOperation::Read) {
                for handle in sub.page_slots.into_iter().flatten() {
                    self.arenas.release(handle);
                }
            }
        }
    }

    pub(crate) fn request_stop_subscription(&mut self, subscription_id: u64) {
        if let Some(sub) = self.subscriptions.get_mut(&subscription_id) {
            sub.choked = true;
            sub.stopped = true;
            if sub.in_flight == 0 {
                self.drop_subscription(subscription_id);
            }
        }
    }

    fn cancel_recv(&mut self, session_id: u64, timed_out: bool) {
        let mut canceled_pending = None;
        let mut canceled_recv_token = None;
        let mut request_kernel_cancel = false;
        if let Some(conn) = self.conns.get_mut(&session_id) {
            canceled_pending = conn.pending_recv.take();
            let decision =
                classify_cancel_recv_decision(conn.read_in_flight, conn.read_in_flight_token);
            canceled_recv_token = decision.canceled_recv_token;
            request_kernel_cancel = decision.request_kernel_cancel;
        }
        if let Some(pending) = canceled_pending {
            self.observe_pending_recv_canceled(&pending, timed_out);
        }
        if request_kernel_cancel && self.cancel_conn_recv(session_id).is_ok() {
            if let Some(conn) = self.conns.get_mut(&session_id) {
                conn.canceled_recv_token = canceled_recv_token;
            }
        }
    }

    pub(crate) fn start_or_run_batch(
        &mut self,
        session_id: u64,
        request_id: u64,
        target: ResultTarget,
        ops: Vec<BatchOp>,
    ) -> bool {
        let Some(conn) = self.conns.get(&session_id) else {
            return false;
        };
        if conn.pending_recv.is_some() || conn.pending_batch.is_some() {
            send_batch_reply_to(
                &target,
                request_id,
                vec![(0, BatchResult::Error(NifError::from_errno(libc::EBUSY)))],
            );
            return true;
        }
        let pending = PendingBatch {
            request_id,
            target,
            ops: VecDeque::from(ops),
            results: Vec::new(),
        };
        if let Some(conn) = self.conns.get_mut(&session_id) {
            conn.pending_batch = Some(pending);
        }
        self.resume_pending_batch(session_id);
        true
    }

    pub(crate) fn resume_pending_batch(&mut self, session_id: u64) {
        loop {
            let next_op = {
                let Some(conn) = self.conns.get_mut(&session_id) else {
                    return;
                };
                let Some(batch) = conn.pending_batch.as_mut() else {
                    return;
                };
                batch.ops.pop_front()
            };

            let Some(op) = next_op else {
                if let Some(conn) = self.conns.get_mut(&session_id) {
                    if let Some(batch) = conn.pending_batch.take() {
                        send_batch_reply_to(&batch.target, batch.request_id, batch.results);
                    }
                }
                return;
            };

            match op {
                BatchOp::Write { id, data } => {
                    let data_len = data.len();
                    let Some(current_tx) = self.conns.get(&session_id).map(|c| c.tx_bytes) else {
                        return;
                    };
                    let result = if current_tx + data_len > TX_QUEUE_MAX_BYTES {
                        BatchResult::Error(NifError::from_errno(libc::EAGAIN))
                    } else {
                        let pending = self.make_pending_write(data);
                        if let Some(conn) = self.conns.get_mut(&session_id) {
                            conn.tx_bytes += data_len;
                            stats_max!(self.stats, tx_queue_peak, conn.tx_bytes);
                            conn.pending_writes.push_back(pending);
                            self.enqueue_write_ready(session_id);
                            BatchResult::Ok
                        } else {
                            return;
                        }
                    };
                    if let Some(conn) = self.conns.get_mut(&session_id) {
                        if let Some(batch) = conn.pending_batch.as_mut() {
                            batch.results.push((id, result));
                        }
                    }
                }
                BatchOp::Read { id, len } => {
                    let suspended = if let Some(conn) = self.conns.get_mut(&session_id) {
                        if conn.rx_bytes > 0 {
                            let n = if len == 0 {
                                conn.rx_bytes
                            } else {
                                len.min(conn.rx_bytes)
                            };
                            let out = rx_take(conn, &mut self.arenas, n);
                            if let Some(batch) = conn.pending_batch.as_mut() {
                                batch.results.push((id, BatchResult::Data(out)));
                            }
                            if conn.rx_bytes < RX_QUEUE_MAX_BYTES {
                                self.local.push_ready(ReadyOp::Conn(session_id));
                            }
                            false
                        } else {
                            if let Some(batch) = conn.pending_batch.as_mut() {
                                batch.ops.push_front(BatchOp::Read { id, len });
                            }
                            self.local.push_ready(ReadyOp::Conn(session_id));
                            true
                        }
                    } else {
                        return;
                    };
                    if suspended {
                        return;
                    }
                }
            }
        }
    }

    pub(crate) fn drain_ready(&mut self) -> Result<usize> {
        let mut budget = SelectedScheduler::<ReadyOp, u64>::drive_budget(DRIVE_BUDGET);
        let callback_len = self.callback_local_fifo.len();
        let callback_reserve = if callback_len > CALLBACK_FORCE_EXEC_ABOVE {
            budget
        } else if callback_len > 0 {
            CALLBACK_RESERVE_MIN.min(budget)
        } else {
            0
        };
        let mut callback_drained = 0usize;
        let mut drained = 0usize;
        while budget > 0 {
            let next = if callback_drained < callback_reserve {
                self.pop_callback_local()
                    .map(|op| (true, ScheduledItem::Ready(op)))
                    .or_else(|| self.local.pop_next().map(|item| (false, item)))
            } else {
                self.local.pop_next().map(|item| (false, item)).or_else(|| {
                    self.pop_callback_local()
                        .map(|op| (true, ScheduledItem::Ready(op)))
                })
            };

            let Some((from_callback, item)) = next else {
                break;
            };
            budget -= 1;
            drained += 1;
            if from_callback {
                callback_drained += 1;
            }
            match item {
                ScheduledItem::WriteReady(session_id) => self.execute_write_ready(session_id)?,
                ScheduledItem::Ready(op) => self.execute_ready_op(op)?,
            }
        }
        Ok(drained)
    }

    pub(crate) fn execute_write_ready(&mut self, session_id: u64) -> Result<()> {
        stats_inc!(self.stats, write_ready_drained);
        if let Some(conn) = self.conns.get_mut(&session_id) {
            conn.write_ready_queued = false;
        } else {
            return Ok(());
        }
        if self.conn_can_arm_write(session_id) {
            self.arm_conn_write(session_id)?;
        }
        Ok(())
    }

    pub(crate) fn execute_ready_op(&mut self, op: ReadyOp) -> Result<()> {
        stats_inc!(self.stats, ready_driven);
        match op {
            ReadyOp::Conn(session_id) => self.drive_connection(session_id)?,
            ReadyOp::ReduceResults => self.drive_reduce_results(),
            ReadyOp::SendResults(slot) => self.drive_send_results(slot),
        }
        Ok(())
    }

    pub(crate) fn drive_connection(&mut self, session_id: u64) -> Result<()> {
        let (recv_mailbox_passive, should_deliver_passive, should_arm_write, should_arm_read) = {
            let Some(conn) = self.conns.get(&session_id) else {
                return Ok(());
            };

            let live_mailbox_passive = match &conn.handle.kind {
                SocketKind::Session(session) => session.mailbox_passive.load(Ordering::Acquire),
                _ => false,
            };
            let recv_mailbox_passive = self
                .recv_mailbox_snapshot(session_id)
                .unwrap_or(live_mailbox_passive);
            let should_deliver_passive = recv_mailbox_passive
                && !active_enabled(&conn.active_mode)
                && conn.pending_recv.is_none()
                && conn.pending_batch.is_none()
                && conn.rx_bytes > 0;
            let should_arm_write =
                !conn.pending_writes.is_empty() && !conn.write_in_flight && !conn.write_poll_armed;
            let should_arm_read =
                conn.rx_bytes < RX_QUEUE_MAX_BYTES && !conn.read_in_flight && !conn.read_poll_armed;
            (
                recv_mailbox_passive,
                should_deliver_passive,
                should_arm_write,
                should_arm_read,
            )
        };

        if should_deliver_passive {
            self.deliver_queued_passive(session_id);
        }

        if should_arm_write {
            self.arm_conn_write(session_id)?;
        }
        if should_arm_read {
            self.set_recv_mailbox_snapshot(session_id, recv_mailbox_passive);
            if self.arm_conn_recv(session_id).is_err() {
                self.clear_recv_mailbox_snapshot(session_id);
            }
        }
        Ok(())
    }

    pub(crate) fn close_connection(&mut self, session_id: u64, reason: NifError) {
        self.clear_recv_mailbox_snapshot(session_id);
        let Some(mut conn) = self.conns.take(&session_id) else {
            return;
        };
        if let SocketKind::Session(session) = &conn.handle.kind {
            session.note_runtime_closed();
        }
        if let Some(pending) = conn.pending_recv.take() {
            self.reply_pending_recv(pending, Err(reason.clone()));
        }
        if let Some(batch) = conn.pending_batch.take() {
            reply_pending_batch(batch, Err(reason.clone()));
        }
        while let Some(write) = conn.pending_writes.pop_front() {
            if let PendingWrite::Arena(chunk) = write {
                self.arenas.release(chunk);
            }
        }
        conn.tx_bytes = 0;
        while let Some(chunk) = conn.rx_chunks.pop_front() {
            match chunk {
                RxChunk::Arena(chunk) => self.arenas.release(chunk),
                RxChunk::Heap(_) => {}
            }
        }
        conn.rx_bytes = 0;
        conn.rx_chunk_off = 0;
        if active_enabled(&conn.active_mode) {
            send_active_closed_message(&conn.handle, &conn.owner);
        } else if let SocketKind::Session(session) = &conn.handle.kind {
            if session.mailbox_passive.load(Ordering::Acquire) {
                match reason {
                    NifError::Closed => send_passive_closed_message(&conn.handle, &conn.owner),
                    _ => send_passive_error_message(&conn.handle, &conn.owner, &reason),
                }
            }
        }
        let read_buf = std::mem::replace(
            &mut conn.read_buf,
            crate::runtime_session::ReadBufLease::Heap(Vec::new()),
        );
        let released = read_buf.release(&mut self.read_pool);
        debug_assert!(
            released,
            "fixed read buffer release must fail closed on stale slot identity"
        );
        // The fd is closed only after the connection has been detached and its
        // buffers have been returned to the local pools above.
        self.close_fd_or_defer(conn.fd.into_raw_fd());
    }

    pub(crate) fn begin_close_connection(&mut self, session_id: u64, reason: NifError) {
        let request_cancel = self
            .conns
            .get(&session_id)
            .is_some_and(|conn| conn.read_in_flight && conn.read_in_flight_token.is_some());
        if request_cancel && self.cancel_conn_recv(session_id).is_ok() {
            if let Some(conn) = self.conns.get_mut(&session_id) {
                conn.stop_close_on_recv_cqe = true;
                return;
            }
        }
        self.close_connection(session_id, reason);
    }

    #[inline]
    pub(crate) fn alloc_session_id(&mut self) -> Option<u64> {
        let local = self.next_session_id;
        self.next_session_id = advance_session_id(local)?;
        Some(((self.shard as u64) << SESSION_ID_SHARD_BITS) | (local & SESSION_ID_LOCAL_MASK))
    }

    pub(crate) fn install_connection(
        &mut self,
        owner: LocalPid,
        accept_tx: Sender<ResourceArc<SocketRef>>,
        fd: RawFd,
    ) {
        if let Some(handle) = self.install_connection_handle_raw(owner, fd) {
            let _ = accept_tx.send(handle);
        } else {
            self.close_fd_or_defer(fd);
        }
    }

    pub(crate) fn install_connection_handle_raw(
        &mut self,
        owner: LocalPid,
        fd: RawFd,
    ) -> Option<ResourceArc<SocketRef>> {
        // SAFETY: this takes ownership of the accepted fd exactly once and
        // immediately forwards it to the owned-fd handoff.
        let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
        self.install_connection_handle(owner, owned_fd)
    }

    pub(crate) fn install_connection_handle(
        &mut self,
        owner: LocalPid,
        fd: OwnedFd,
    ) -> Option<ResourceArc<SocketRef>> {
        let raw_fd = fd.as_raw_fd();
        let session_id = self.alloc_session_id()?;
        let read_buf =
            crate::runtime_session::ReadBufLease::from_pool_or_heap_fallback(&mut self.read_pool);
        let handle = ResourceArc::new(SocketRef {
            kind: SocketKind::Session(SessionState {
                session_id,
                shard: self.shard,
                fd: raw_fd,
                owner: Mutex::new(owner),
                opts: Mutex::new(SocketOpts::default()),
                mailbox_passive: AtomicBool::new(false),
                local: Mutex::new(None),
                peer: Mutex::new(None),
                api_state: AtomicUsize::new(0),
                closed: AtomicBool::new(false),
            }),
        });
        let conn = Connection {
            fd,
            handle: handle.clone(),
            owner,
            read_buf,
            rx_chunks: VecDeque::new(),
            rx_chunk_off: 0,
            rx_bytes: 0,
            pending_recv: None,
            canceled_recv_token: None,
            pending_batch: None,
            pending_writes: VecDeque::new(),
            tx_bytes: 0,
            active_mode: ActiveMode::False,
            read_poll_armed: false,
            write_poll_armed: false,
            read_in_flight: false,
            read_in_flight_token: None,
            read_in_flight_ring: None,
            stop_close_on_recv_cqe: false,
            stop_close_on_read_poll_cqe: false,
            stop_close_on_write_cqe: false,
            stop_close_on_write_poll_cqe: false,
            write_in_flight: false,
            write_ready_queued: false,
            write_iovecs: Vec::with_capacity(WRITEV_BATCH),
        };
        self.conns.insert(session_id, conn);
        self.try_arm_recv_inline_or_fallback(session_id);
        Some(handle)
    }

    pub(crate) fn conn_can_arm_write(&self, session_id: u64) -> bool {
        self.conns
            .get(&session_id)
            .map(|c| !c.pending_writes.is_empty() && !c.write_in_flight && !c.write_poll_armed)
            .unwrap_or(false)
    }

    pub(crate) fn enqueue_write_ready(&mut self, session_id: u64) {
        let Some(conn) = self.conns.get_mut(&session_id) else {
            return;
        };
        if conn.pending_writes.is_empty()
            || conn.write_in_flight
            || conn.write_poll_armed
            || conn.write_ready_queued
        {
            return;
        }
        conn.write_ready_queued = true;
        stats_inc!(self.stats, write_ready_enqueued);
        self.local.push_write_ready(session_id);
    }

    pub(crate) fn maybe_log_stats(&mut self) {
        if !self.stats.log_every {
            return;
        }
        let total_cqes = self.stats.total_cqes();
        if total_cqes < self.stats.next_log_cqes {
            return;
        }
        eprintln!(
            "{} shard={} cqes={} cmds={} ready={} write_ready_enq={} write_ready_drain={} recv_sqe={} recv_cqe={} write_sqe={} write_cqe={} read_polls={} write_polls={} reprovides={} rx_peak={} tx_peak={}",
            stats_labels::STATS,
            self.shard,
            total_cqes,
            self.stats.commands_drained,
            self.stats.ready_driven,
            self.stats.write_ready_enqueued,
            self.stats.write_ready_drained,
            self.stats.recv_sqes,
            self.stats.recv_cqes,
            self.stats.write_sqes,
            self.stats.write_cqes,
            self.stats.read_polls,
            self.stats.write_polls,
            self.stats.reprovide_sqes,
            self.stats.rx_queue_peak,
            self.stats.tx_queue_peak
        );
        self.stats.next_log_cqes = total_cqes.saturating_add(STATS_LOG_INTERVAL);
    }

    pub(crate) fn make_pending_write(&mut self, data: Vec<u8>) -> PendingWrite {
        if data.len() <= BUF_SIZE {
            if let Some(mut handle) = self.arenas.acquire_for_len(data.len()) {
                let len = data.len();
                self.arenas
                    .slice_mut_prefix(handle.class, handle.slot, len)
                    .copy_from_slice(&data);
                handle.off = 0;
                handle.len = len;
                return PendingWrite::Arena(handle);
            }
            self.submit_pressure = true;
        }
        PendingWrite::Heap { data, off: 0 }
    }

    pub(crate) fn deliver_queued_active(&mut self, session_id: u64) {
        loop {
            let Some((handle, owner, mode, next_len)) =
                self.conns.get(&session_id).and_then(|conn| {
                    if !active_enabled(&conn.active_mode) || conn.rx_bytes == 0 {
                        return None;
                    }
                    Some((
                        conn.handle.clone(),
                        conn.owner,
                        conn.active_mode.clone(),
                        rx_front_len(conn),
                    ))
                })
            else {
                break;
            };

            let Some(next_len) = next_len else {
                break;
            };

            let data = {
                let Some(conn) = self.conns.get_mut(&session_id) else {
                    break;
                };
                rx_take(conn, &mut self.arenas, next_len)
            };

            send_active_data_message(&handle, &owner, &data);
            if let Some(conn) = self.conns.get_mut(&session_id) {
                conn.active_mode = mode;
                advance_active_mode(&conn.handle, &conn.owner, &mut conn.active_mode);
            } else {
                break;
            }
        }
    }

    pub(crate) fn deliver_queued_passive(&mut self, session_id: u64) {
        let mailbox_passive_on_submit =
            self.recv_mailbox_snapshot(session_id).unwrap_or_else(|| {
                self.conns
                    .get(&session_id)
                    .and_then(|conn| match &conn.handle.kind {
                        SocketKind::Session(session) => {
                            Some(session.mailbox_passive.load(Ordering::Acquire))
                        }
                        _ => None,
                    })
                    .unwrap_or(false)
            });

        let Some((handle, owner)) = self.conns.get(&session_id).and_then(|conn| {
            let SocketKind::Session(_session) = &conn.handle.kind else {
                return None;
            };
            if active_enabled(&conn.active_mode)
                || conn.pending_recv.is_some()
                || conn.pending_batch.is_some()
                || !mailbox_passive_on_submit
                || conn.rx_bytes == 0
            {
                return None;
            }
            Some((conn.handle.clone(), conn.owner))
        }) else {
            return;
        };

        let data = {
            let Some(conn) = self.conns.get_mut(&session_id) else {
                return;
            };
            let total = conn.rx_bytes;
            rx_take(conn, &mut self.arenas, total)
        };

        send_passive_data_message(&handle, &owner, &data);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ingress_channel, AsyncReplyPayload, Command, ListenerShard, LocalPid, LockedReadBufPool,
        NifError, Op, PendingRecv, PendingRecvReply, PendingWrite, ProvidedRecvPool, RecvDecision,
        ResultBatchSlot, ResultCallbackOutcome, ResultReduceTrigger, ResultTarget, RuntimeArenas,
        ShardControl, ShardPumpStep, ShardRuntimeConfig, ShardState, ShardStopState, SocketKind,
        SubscribeOperation, Subscription, COMMAND_BUDGET, SESSION_ID_SHARD_BITS,
        STATS_LOG_INTERVAL, SUBSCRIBE_INFLIGHT,
    };
    use crate::runtime_reactor::{BufSlotState, RingKind};
    use crate::socket;
    use dataplane_runtime::runtime_scheduler::SchedulerPolicy;
    use std::mem;
    use std::sync::atomic::Ordering;

    fn zero_owner() -> LocalPid {
        // SAFETY: `LocalPid` is an opaque test-only placeholder here; zero value is never
        // dereferenced and is only used to satisfy ownership plumbing in unit tests.
        unsafe { mem::zeroed() }
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    fn test_runtime_config() -> ShardRuntimeConfig {
        ShardRuntimeConfig::from_env(false)
    }

    #[cfg(feature = "exec-strategy-sqpoll")]
    fn test_runtime_config() -> ShardRuntimeConfig {
        ShardRuntimeConfig::from_env(false)
    }

    fn make_shard_state() -> ShardState {
        let latency_ring = io_uring::IoUring::builder()
            .setup_single_issuer()
            .build(64)
            .expect("build latency ring");
        let (tx, rx) = ingress_channel::<Command>();

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let main_ring = io_uring::IoUring::builder()
            .setup_single_issuer()
            .build(64)
            .expect("build main ring");

        let control = std::sync::Arc::new(ShardControl::new());
        let read_pool = LockedReadBufPool::new(0).expect("read pool");
        let arenas = RuntimeArenas::new(0, 0);

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let shard = ShardState::new(
            0,
            0,
            latency_ring,
            main_ring,
            rx,
            0,
            read_pool,
            0,
            Vec::new(),
            false,
            false,
            None,
            Vec::new(),
            false,
            false,
            false,
            test_runtime_config(),
            None,
            arenas,
            control,
        );

        #[cfg(feature = "exec-strategy-sqpoll")]
        let shard = ShardState::new(
            0,
            0,
            latency_ring,
            rx,
            0,
            read_pool,
            0,
            Vec::new(),
            false,
            false,
            None,
            Vec::new(),
            false,
            false,
            test_runtime_config(),
            None,
            arenas,
            control,
        );

        drop(tx);
        shard
    }

    fn make_shard_state_with_tx() -> (ShardState, crate::runtime_ingress::IngressSender<Command>) {
        let latency_ring = io_uring::IoUring::builder()
            .setup_single_issuer()
            .build(64)
            .expect("build latency ring");
        let (tx, rx) = ingress_channel::<Command>();

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let main_ring = io_uring::IoUring::builder()
            .setup_single_issuer()
            .build(64)
            .expect("build main ring");

        let control = std::sync::Arc::new(ShardControl::new());
        let read_pool = LockedReadBufPool::new(0).expect("read pool");
        let arenas = RuntimeArenas::new(0, 0);

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let shard = ShardState::new(
            0,
            0,
            latency_ring,
            main_ring,
            rx,
            0,
            read_pool,
            0,
            Vec::new(),
            false,
            false,
            None,
            Vec::new(),
            false,
            false,
            false,
            test_runtime_config(),
            None,
            arenas,
            control,
        );

        #[cfg(feature = "exec-strategy-sqpoll")]
        let shard = ShardState::new(
            0,
            0,
            latency_ring,
            rx,
            0,
            read_pool,
            0,
            Vec::new(),
            false,
            false,
            None,
            Vec::new(),
            false,
            false,
            test_runtime_config(),
            None,
            arenas,
            control,
        );

        (shard, tx)
    }

    fn make_shard_state_with_provided_pool(pool: ProvidedRecvPool) -> ShardState {
        let latency_ring = io_uring::IoUring::builder()
            .setup_single_issuer()
            .build(64)
            .expect("build latency ring");
        let (tx, rx) = ingress_channel::<Command>();

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let main_ring = io_uring::IoUring::builder()
            .setup_single_issuer()
            .build(64)
            .expect("build main ring");

        let control = std::sync::Arc::new(ShardControl::new());
        let read_pool = LockedReadBufPool::new(0).expect("read pool");
        let arenas = RuntimeArenas::new(0, 0);

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let shard = ShardState::new(
            0,
            0,
            latency_ring,
            main_ring,
            rx,
            0,
            read_pool,
            0,
            Vec::new(),
            false,
            false,
            None,
            Vec::new(),
            false,
            false,
            false,
            test_runtime_config(),
            Some(pool),
            arenas,
            control,
        );

        #[cfg(feature = "exec-strategy-sqpoll")]
        let shard = ShardState::new(
            0,
            0,
            latency_ring,
            rx,
            0,
            read_pool,
            0,
            Vec::new(),
            false,
            false,
            None,
            Vec::new(),
            false,
            false,
            test_runtime_config(),
            Some(pool),
            arenas,
            control,
        );

        drop(tx);
        shard
    }

    fn make_shard_state_with_fixed_read_pool(slots: usize) -> ShardState {
        let latency_ring = io_uring::IoUring::builder()
            .setup_single_issuer()
            .build(64)
            .expect("build latency ring");
        let (tx, rx) = ingress_channel::<Command>();

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let main_ring = io_uring::IoUring::builder()
            .setup_single_issuer()
            .build(64)
            .expect("build main ring");

        let control = std::sync::Arc::new(ShardControl::new());
        let read_pool = LockedReadBufPool::new(slots).expect("read pool");
        let arenas = RuntimeArenas::new(0, 0);

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let shard = ShardState::new(
            0,
            0,
            latency_ring,
            main_ring,
            rx,
            0,
            read_pool,
            0,
            Vec::new(),
            true,
            false,
            None,
            Vec::new(),
            false,
            false,
            false,
            test_runtime_config(),
            None,
            arenas,
            control,
        );

        #[cfg(feature = "exec-strategy-sqpoll")]
        let shard = ShardState::new(
            0,
            0,
            latency_ring,
            rx,
            0,
            read_pool,
            0,
            Vec::new(),
            true,
            false,
            None,
            Vec::new(),
            false,
            false,
            test_runtime_config(),
            None,
            arenas,
            control,
        );

        drop(tx);
        shard
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    fn submit_nops(ring: &mut io_uring::IoUring, count: usize) {
        let entry = io_uring::opcode::Nop::new().build().user_data(1);
        let mut submitted = 0usize;
        while submitted < count {
            let pushed = {
                let mut sq = ring.submission();
                sq.sync();
                // SAFETY: sq is a local IoUring submission queue reference; push is
                // a raw write into the ring's SQE ring buffer with no aliasing.
                unsafe { sq.push(&entry) }.is_ok()
            };
            if !pushed {
                ring.submit().expect("submit nop entries for sq space");
                continue;
            }
            ring.submit().expect("submit nop entries");
            submitted += 1;
        }
    }

    #[test]
    fn shard_is_idle_for_teardown_without_live_resources() {
        let shard = make_shard_state();
        assert!(shard.is_idle_for_teardown());
    }

    #[test]
    fn shard_is_not_idle_for_teardown_with_live_listener() {
        let mut shard = make_shard_state();
        let owner = zero_owner();
        let (accept_tx, _accept_rx) =
            crossbeam_channel::unbounded::<rustler::ResourceArc<socket::SocketRef>>();

        shard.listeners.insert(
            1,
            ListenerShard {
                fd: -1,
                accept_tx,
                owner,
                accept_armed: false,
                accept_token: None,
            },
        );

        assert!(!shard.is_idle_for_teardown());
    }

    #[test]
    fn shard_is_not_idle_for_teardown_with_live_subscription() {
        let mut shard = make_shard_state();
        let target = ResultTarget::Erlang(zero_owner());

        shard.subscriptions.insert(
            1,
            Subscription {
                target,
                accept_consumers: Vec::new(),
                accept_rr_next: 0,
                operation: SubscribeOperation::Read,
                fd: -1,
                choked: false,
                stopped: false,
                in_flight: 0,
                page_slots: [None; SUBSCRIBE_INFLIGHT],
                slot_in_flight: [false; SUBSCRIBE_INFLIGHT],
            },
        );

        assert!(!shard.is_idle_for_teardown());
    }

    #[test]
    fn fixed_buffer_unregister_waits_for_subscription_teardown_state() {
        let mut shard = make_shard_state();
        shard.read_fixed_enabled = true;
        let target = ResultTarget::Erlang(zero_owner());

        shard.subscriptions.insert(
            1,
            Subscription {
                target,
                accept_consumers: Vec::new(),
                accept_rr_next: 0,
                operation: SubscribeOperation::Accept,
                fd: -1,
                choked: true,
                stopped: true,
                in_flight: 0,
                page_slots: [None; SUBSCRIBE_INFLIGHT],
                slot_in_flight: [false; SUBSCRIBE_INFLIGHT],
            },
        );

        assert!(!shard.can_unregister_fixed_buffers_on_exit());

        shard.subscriptions.clear();
        let token = shard.latency_ops.insert(Op::SubscribeAccept {
            subscription_id: 1,
            slot_idx: 0,
            armed_ns: 0,
        });
        assert!(!shard.can_unregister_fixed_buffers_on_exit());
        let _ = shard.latency_ops.remove(token);
        assert!(shard.can_unregister_fixed_buffers_on_exit());
    }

    #[test]
    fn fixed_buffer_unregister_stop_line_waits_for_provided_pool_teardown() {
        let mut shard = make_shard_state_with_provided_pool(ProvidedRecvPool::new(7, 1));
        shard.read_fixed_enabled = true;

        {
            let pool = shard
                .provided_recv_pool
                .as_mut()
                .expect("provided recv pool should exist");
            assert!(pool.provide_entry(0).is_some());
            pool.begin_draining();
            assert!(!pool.advance_retirement());
            assert!(pool.retire_for_remove());
            assert!(!pool.is_teardown_complete());
        }

        assert!(!shard.fixed_buffer_unregister_stop_line_ready());
        assert!(!shard.can_unregister_fixed_buffers_on_exit());

        shard
            .provided_recv_pool
            .as_mut()
            .expect("provided recv pool should exist")
            .mark_remove_completed();
        assert!(shard.fixed_buffer_unregister_stop_line_ready());
        assert!(shard.can_unregister_fixed_buffers_on_exit());
    }

    #[test]
    fn clear_fixed_buffer_registration_state_clears_lookup_tables() {
        let mut shard = make_shard_state();
        shard.read_fixed_enabled = true;
        shard.subscribe_fixed_enabled = true;
        shard.subscribe_fixed_base = Some(3);
        shard.read_fixed_slots = vec![Some(0), Some(2)];
        shard.subscribe_fixed_slots = vec![Some(0), Some(1)];

        shard.clear_fixed_buffer_registration_state();

        assert!(!shard.read_fixed_enabled);
        assert!(shard.read_fixed_slots.is_empty());
        assert!(!shard.subscribe_fixed_enabled);
        assert_eq!(shard.subscribe_fixed_base, None);
        assert!(shard.subscribe_fixed_slots.is_empty());
    }

    #[test]
    fn deferred_close_blocks_idle_teardown() {
        let mut shard = make_shard_state();
        // SAFETY: test creates a standalone eventfd and manages its lifetime in this scope.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        assert!(fd >= 0);

        shard.deferred_close_fds.push_back(fd);
        assert!(!shard.is_idle_for_teardown());

        shard.deferred_close_fds.clear();
        assert!(shard.is_idle_for_teardown());

        // SAFETY: closes the fd created above after assertions complete.
        unsafe { libc::close(fd) };
    }

    #[test]
    fn deferred_close_requeues_to_close_sqe_when_ring_accepts_work() {
        let mut shard = make_shard_state();
        // SAFETY: test creates a standalone eventfd and manages its lifetime in this scope.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        assert!(fd >= 0);
        shard.deferred_close_fds.push_back(fd);

        let _token = shard.latency_ops.insert(Op::Wakeup);
        shard.advance_deferred_close_fds();
        assert!(shard.deferred_close_fds.is_empty());
        // SAFETY: `fd` is valid and open at this point; `F_GETFD` does not mutate memory.
        assert!(unsafe { libc::fcntl(fd, libc::F_GETFD) } >= 0);
        assert!(shard
            .latency_ops
            .values()
            .any(|op| matches!(op, Op::CloseFd { fd: close_fd } if *close_fd == fd)));

        // SAFETY: closes the fd created above after assertions complete.
        unsafe { libc::close(fd) };
    }

    #[test]
    fn recv_async_busy_from_pending_recv_does_not_replace_existing_request_id() {
        let existing_request_id = Some(99u64);
        let decision = ShardState::classify_recv_decision(false, true, 0, 0);
        let next_request_id = match decision {
            RecvDecision::Pending => Some(101),
            _ => existing_request_id,
        };
        assert_eq!(decision, RecvDecision::Busy);
        assert_eq!(next_request_id, existing_request_id);
    }

    #[test]
    fn recv_async_busy_from_pending_batch_does_not_enqueue_pending_recv() {
        let pending_recv = Some(33u64);
        let decision = ShardState::classify_recv_decision(true, false, 0, 0);
        let next_pending_recv = match decision {
            RecvDecision::Pending => Some(55),
            _ => pending_recv,
        };
        assert_eq!(decision, RecvDecision::Busy);
        assert_eq!(next_pending_recv, Some(33u64));
    }

    #[test]
    fn recv_sync_busy_from_pending_recv_does_not_replace_existing_request_id() {
        let existing_request_id = Some(77u64);
        let decision = ShardState::classify_recv_decision(false, true, 0, 0);
        let next_request_id = match decision {
            RecvDecision::Pending => Some(88),
            _ => existing_request_id,
        };
        assert_eq!(decision, RecvDecision::Busy);
        assert_eq!(next_request_id, existing_request_id);
    }

    #[test]
    fn recv_mailbox_snapshot_for_conn_cqe_missing_entry_returns_passive_disabled() {
        let shard = make_shard_state();

        assert!(!shard.recv_mailbox_snapshot_for_conn_cqe(1_234_567));
    }

    #[test]
    fn recv_mailbox_snapshot_for_conn_cqe_prefers_snapshot_value_when_present() {
        let shard = make_shard_state();
        let session_id = 42;

        shard.set_recv_mailbox_snapshot(session_id, true);
        assert!(shard.recv_mailbox_snapshot_for_conn_cqe(session_id));
        shard.set_recv_mailbox_snapshot(session_id, false);
        assert!(!shard.recv_mailbox_snapshot_for_conn_cqe(session_id));
    }

    #[test]
    fn recv_enobufs_begins_draining_without_dropping_pool() {
        let mut shard = make_shard_state_with_provided_pool(ProvidedRecvPool::new(7, 2));

        assert!(shard
            .provided_recv_pool
            .as_ref()
            .is_some_and(|pool| pool.is_active()));
        assert!(!shard.has_latency_queue_work());

        shard.handle_recv_enobufs(99);

        assert!(shard
            .provided_recv_pool
            .as_ref()
            .is_some_and(|pool| pool.is_draining()));
        assert!(shard.provided_recv_pool.is_some());
        assert!(shard.has_latency_queue_work());
    }

    #[test]
    #[ignore = "requires Rustler resource registration from NIF init"]
    fn fixed_read_recv_cqe_returns_data_from_stable_slot() {
        let mut shard = make_shard_state_with_fixed_read_pool(1);
        let owner = zero_owner();
        let handle = shard
            .install_connection_handle_raw(owner, 0)
            .expect("session id should advance");
        let session_id = match &handle.kind {
            SocketKind::Session(session_state) => session_state.session_id,
            _ => unreachable!(),
        };

        let expected = b"pong";
        let (sync_tx, sync_rx) = std::sync::mpsc::sync_channel(1);

        {
            let Some(conn) = shard.conns.get_mut(&session_id) else {
                panic!("connection should exist before recv cqe setup");
            };
            let ptr = conn.read_buf.as_mut_ptr(&mut shard.read_pool);
            assert!(!ptr.is_null());
            // SAFETY: destination points to a writable read buffer with at least
            // `expected.len()` bytes; source is valid for that same length.
            unsafe {
                std::ptr::copy_nonoverlapping(expected.as_ptr(), ptr, expected.len());
            }
            conn.pending_recv = Some(PendingRecv {
                len: expected.len(),
                enqueued_raw: 0,
                reply: PendingRecvReply::Sync(sync_tx),
            });
            conn.read_in_flight = true;
            conn.read_in_flight_token = Some(55);
            conn.read_in_flight_ring = Some(RingKind::Latency);
        }

        shard.handle_recv_cqe(session_id, expected.len() as i32, 0, false, 55);

        let result = sync_rx
            .try_recv()
            .expect("sync recv response should be sent");
        assert_eq!(result.expect("reply should be Ok"), expected.to_vec());
        assert!(shard.conns.get(&session_id).unwrap().pending_recv.is_none());
    }

    #[test]
    fn stale_recv_cqe_after_close_is_ignored_without_touching_connection_table() {
        let mut shard = make_shard_state_with_fixed_read_pool(1);
        let stale_session_id = (1u64 << SESSION_ID_SHARD_BITS) | 7;

        shard.handle_recv_cqe(stale_session_id, 1, 0, false, 55);

        assert!(shard.conns.is_empty());
    }

    #[test]
    fn resolve_provided_recv_slot_without_pool_returns_none() {
        let shard = make_shard_state();

        assert!(shard.resolve_provided_recv_slot(0).is_none());
    }

    #[test]
    #[ignore = "requires Rustler resource registration from NIF init"]
    fn provided_recv_cqe_returns_data_from_stable_slot() {
        let mut shard = make_shard_state_with_provided_pool(ProvidedRecvPool::new(7, 1));
        let owner = zero_owner();
        let handle = shard
            .install_connection_handle_raw(owner, 0)
            .expect("session id should advance");
        let session_id = match &handle.kind {
            SocketKind::Session(session_state) => session_state.session_id,
            _ => unreachable!(),
        };

        let expected = b"pong";
        let (sync_tx, sync_rx) = std::sync::mpsc::sync_channel(1);
        let bid = 0u16;
        let flags = 1u32 | ((bid as u32) << 16);

        {
            let pool = shard
                .provided_recv_pool
                .as_mut()
                .expect("provided recv pool should exist");
            assert!(pool.provide_entry(bid).is_some());
            let slot = pool.resolve_inflight_slot(bid).expect("inflight slot");
            assert!(pool.seed_inflight_slot_bytes(slot, expected));
        }

        {
            let conn = shard.conns.get_mut(&session_id).unwrap();
            conn.pending_recv = Some(PendingRecv {
                len: expected.len(),
                enqueued_raw: 0,
                reply: PendingRecvReply::Sync(sync_tx),
            });
            conn.read_in_flight = true;
            conn.read_in_flight_token = Some(77);
            conn.read_in_flight_ring = Some(RingKind::Latency);
        }

        shard.handle_recv_cqe(session_id, expected.len() as i32, flags, false, 77);

        let result = sync_rx
            .try_recv()
            .expect("sync recv response should be sent");
        assert_eq!(result.expect("reply should be Ok"), expected.to_vec());
        assert!(shard.conns.get(&session_id).unwrap().pending_recv.is_none());
        assert!(shard
            .provided_recv_pool
            .as_ref()
            .and_then(|pool| pool.resolve_inflight_slot(bid))
            .is_none());
    }

    #[test]
    #[ignore = "requires Rustler resource registration from NIF init"]
    fn provided_recv_cqe_without_pool_closes_connection_explicitly() {
        let mut shard = make_shard_state();
        let owner = zero_owner();
        let handle = shard
            .install_connection_handle_raw(owner, 0)
            .expect("session id should advance");
        let session_id = match &handle.kind {
            SocketKind::Session(session_state) => session_state.session_id,
            _ => unreachable!(),
        };

        shard.handle_recv_cqe(session_id, 1, 1, false, 77);

        assert!(shard.conns.get(&session_id).is_none());
    }

    #[test]
    #[ignore = "requires Rustler resource registration from NIF init"]
    fn request_stop_orders_cancel_drain_unregister_before_close_completion() {
        let mut shard = make_shard_state_with_provided_pool(ProvidedRecvPool::new(7, 1));
        shard.read_fixed_enabled = true;
        let owner = zero_owner();
        // SAFETY: test creates a standalone eventfd and transfers/tears it down in this scope.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        assert!(fd >= 0);
        let handle = shard
            .install_connection_handle_raw(owner, fd)
            .expect("session id should advance");
        let session_id = match &handle.kind {
            SocketKind::Session(session_state) => session_state.session_id,
            _ => unreachable!(),
        };

        let bid = 0u16;
        let flags = 1u32 | ((bid as u32) << 16);
        {
            let pool = shard
                .provided_recv_pool
                .as_mut()
                .expect("provided recv pool should exist");
            assert!(pool.provide_entry(bid).is_some());
            let slot = pool.resolve_inflight_slot(bid).expect("inflight slot");
            assert!(pool.seed_inflight_slot_bytes(slot, b"x"));
        }
        {
            let conn = shard.conns.get_mut(&session_id).unwrap();
            conn.read_in_flight = true;
            conn.read_in_flight_token = Some(77);
            conn.read_in_flight_ring = Some(RingKind::Latency);
        }

        shard.request_stop();

        assert!(matches!(
            shard.stop_state,
            ShardStopState::StopPending | ShardStopState::Draining
        ));
        assert!(shard
            .provided_recv_pool
            .as_ref()
            .is_some_and(|pool| pool.is_draining()));
        assert!(shard
            .conns
            .get(&session_id)
            .is_some_and(|conn| conn.stop_close_on_recv_cqe));
        assert!(shard
            .latency_ops
            .values()
            .any(|op| matches!(op, Op::CancelRecv(id) if *id == session_id)));
        assert!(!shard.can_unregister_fixed_buffers_on_exit());

        let cancel_op = shard
            .latency_ops
            .remove_first_matching(|op| matches!(op, Op::CancelRecv(id) if *id == session_id));
        assert!(matches!(cancel_op, Some(Op::CancelRecv(id)) if id == session_id));

        shard.handle_recv_cqe(session_id, 1, flags, false, 77);

        assert!(!shard.conns.contains(&session_id));
        assert!(shard
            .latency_ops
            .values()
            .any(|op| matches!(op, Op::CloseFd { fd: close_fd } if *close_fd == fd)));
        assert!(!shard.can_unregister_fixed_buffers_on_exit());

        shard
            .advance_provided_recv_pool_lifecycle()
            .expect("advance after recv close");
        assert!(shard
            .provided_recv_pool
            .as_ref()
            .is_some_and(|pool| pool.is_removed()));
        assert!(shard
            .latency_ops
            .values()
            .any(|op| matches!(op, Op::ProvidedRecvPoolRemove)));

        let remove_op = shard
            .latency_ops
            .remove_first_matching(|op| matches!(op, Op::ProvidedRecvPoolRemove));
        assert!(matches!(remove_op, Some(Op::ProvidedRecvPoolRemove)));
        shard.handle_provided_recv_pool_remove_cqe(0);

        assert!(shard.provided_recv_pool.is_none());
        assert!(shard.can_unregister_fixed_buffers_on_exit());
        shard
            .try_unregister_fixed_buffers_on_exit()
            .expect("fixed buffers should unregister after remove completion");
        assert!(!shard.read_fixed_enabled);
        assert!(!shard.is_idle_for_teardown());

        let close_op = shard.latency_ops.remove_first_matching(
            |op| matches!(op, Op::CloseFd { fd: close_fd } if *close_fd == fd),
        );
        assert!(matches!(close_op, Some(Op::CloseFd { fd: close_fd }) if close_fd == fd));
        shard.handle_close_fd_cqe(fd, 0);

        assert!(shard.is_idle_for_teardown());
    }

    #[test]
    fn provided_recv_pool_remove_is_queued_only_after_quiescence() {
        let mut shard = make_shard_state_with_provided_pool(ProvidedRecvPool::new(7, 1));

        {
            let pool = shard
                .provided_recv_pool
                .as_mut()
                .expect("provided recv pool should exist");
            assert!(pool.provide_entry(0).is_some());
            pool.begin_draining();
        }

        shard
            .advance_provided_recv_pool_lifecycle()
            .expect("advance with inflight slot");
        assert!(shard
            .provided_recv_pool
            .as_ref()
            .is_some_and(|pool| pool.is_draining()));
        assert!(!shard
            .latency_ops
            .values()
            .any(|op| matches!(op, Op::ProvidedRecvPoolRemove)));

        {
            let pool = shard
                .provided_recv_pool
                .as_mut()
                .expect("provided recv pool should exist");
            let slot = pool.resolve_inflight_slot(0).expect("inflight slot");
            assert!(pool.complete_recv(slot));
        }

        shard
            .advance_provided_recv_pool_lifecycle()
            .expect("advance after inflight drain");
        assert!(shard
            .provided_recv_pool
            .as_ref()
            .is_some_and(|pool| pool.is_removed()));
        assert!(shard
            .latency_ops
            .values()
            .any(|op| matches!(op, Op::ProvidedRecvPoolRemove)));
    }

    #[test]
    fn remove_cqe_releases_provided_recv_pool_storage() {
        let mut shard = make_shard_state_with_provided_pool(ProvidedRecvPool::new(7, 1));

        {
            let pool = shard
                .provided_recv_pool
                .as_mut()
                .expect("provided recv pool should exist");
            assert!(pool.provide_entry(0).is_some());
            pool.begin_draining();
            assert!(pool.retire_for_remove());
        }

        let queued = shard
            .enqueue_provided_recv_pool_remove()
            .expect("remove should queue");
        assert!(queued);
        assert!(shard.provided_recv_pool.is_some());

        shard.handle_provided_recv_pool_remove_cqe(0);

        assert!(shard.provided_recv_pool.is_none());
    }

    #[test]
    fn stopping_shard_delays_provided_recv_pool_remove_until_resources_are_gone() {
        let mut shard = make_shard_state_with_provided_pool(ProvidedRecvPool::new(7, 1));
        let target = ResultTarget::Erlang(zero_owner());

        shard.subscriptions.insert(
            1,
            Subscription {
                target,
                accept_consumers: Vec::new(),
                accept_rr_next: 0,
                operation: SubscribeOperation::Read,
                fd: -1,
                choked: false,
                stopped: false,
                in_flight: 0,
                page_slots: [None; SUBSCRIBE_INFLIGHT],
                slot_in_flight: [false; SUBSCRIBE_INFLIGHT],
            },
        );
        shard.stop_state = ShardStopState::Draining;

        {
            let pool = shard
                .provided_recv_pool
                .as_mut()
                .expect("provided recv pool should exist");
            assert!(pool.provide_entry(0).is_some());
            pool.begin_draining();
            assert!(pool.retire_for_remove());
            assert!(pool.should_queue_remove());
        }

        shard
            .advance_provided_recv_pool_lifecycle()
            .expect("advance while subscription is still live");
        assert!(!shard
            .latency_ops
            .values()
            .any(|op| matches!(op, Op::ProvidedRecvPoolRemove)));

        shard.subscriptions.clear();
        shard
            .advance_provided_recv_pool_lifecycle()
            .expect("advance after live resources are gone");
        assert!(shard
            .latency_ops
            .values()
            .any(|op| matches!(op, Op::ProvidedRecvPoolRemove)));
    }

    #[test]
    #[ignore = "requires Rustler resource registration from NIF init"]
    fn request_stop_defers_close_for_write_in_flight() {
        let mut shard = make_shard_state();
        let owner = zero_owner();
        let handle = shard
            .install_connection_handle_raw(owner, -1)
            .expect("session id should advance");
        let session_id = match &handle.kind {
            SocketKind::Session(session_state) => session_state.session_id,
            _ => unreachable!(),
        };

        {
            let conn = shard.conns.get_mut(&session_id).unwrap();
            conn.write_in_flight = true;
            conn.pending_writes.push_back(PendingWrite::Heap {
                data: vec![1, 2, 3],
                off: 0,
            });
            conn.tx_bytes = 3;
        }

        shard.request_stop();

        assert!(shard.conns.contains(&session_id));
        assert!(shard
            .conns
            .get(&session_id)
            .is_some_and(|conn| conn.stop_close_on_write_cqe));
    }

    #[test]
    #[ignore = "requires Rustler resource registration from NIF init"]
    fn request_stop_defers_close_for_poll_armed_connections() {
        let mut shard = make_shard_state();
        let owner = zero_owner();
        let handle = shard
            .install_connection_handle_raw(owner, -1)
            .expect("session id should advance");
        let session_id = match &handle.kind {
            SocketKind::Session(session_state) => session_state.session_id,
            _ => unreachable!(),
        };

        {
            let conn = shard.conns.get_mut(&session_id).unwrap();
            conn.read_poll_armed = true;
            conn.write_poll_armed = true;
        }

        shard.request_stop();

        assert!(shard.conns.contains(&session_id));
        assert!(shard.conns.get(&session_id).is_some_and(
            |conn| conn.stop_close_on_read_poll_cqe && conn.stop_close_on_write_poll_cqe
        ));
    }

    #[test]
    #[ignore = "requires Rustler resource registration from NIF init"]
    fn cancel_recv_tracks_token_only_when_kernel_cancel_is_requested() {
        let mut shard = make_shard_state();
        let owner = zero_owner();
        let handle = shard
            .install_connection_handle_raw(owner, 0)
            .expect("session id should advance");
        let session_id = match &handle.kind {
            SocketKind::Session(session_state) => session_state.session_id,
            _ => unreachable!(),
        };

        {
            let conn = shard.conns.get_mut(&session_id).unwrap();
            conn.read_in_flight = true;
            conn.read_in_flight_token = Some(123);
            conn.read_in_flight_ring = Some(RingKind::Latency);
            conn.pending_recv = Some(PendingRecv {
                len: 8,
                enqueued_raw: 0,
                reply: PendingRecvReply::Async {
                    request_id: 11,
                    target: ResultTarget::Erlang(zero_owner()),
                },
            });
        }

        shard.cancel_recv(session_id, false);
        assert_eq!(
            shard.conns.get(&session_id).unwrap().canceled_recv_token,
            Some(123)
        );
    }

    #[test]
    #[ignore = "requires Rustler resource registration from NIF init"]
    fn cancel_recv_cqe_marker_does_not_fulfill_new_pending_recv() {
        let mut shard = make_shard_state();
        let owner = zero_owner();
        let handle = shard
            .install_connection_handle_raw(owner, 0)
            .expect("session id should advance");
        let session_id = match &handle.kind {
            SocketKind::Session(session_state) => session_state.session_id,
            _ => unreachable!(),
        };

        let (sync_tx, sync_rx) = std::sync::mpsc::sync_channel(1);
        {
            let conn = shard.conns.get_mut(&session_id).unwrap();
            conn.pending_recv = Some(PendingRecv {
                len: 1,
                enqueued_raw: 0,
                reply: PendingRecvReply::Sync(sync_tx),
            });
            conn.read_in_flight = true;
            conn.read_in_flight_token = Some(42);
            conn.read_in_flight_ring = Some(RingKind::Latency);
            conn.canceled_recv_token = Some(42);
        }

        shard.handle_recv_cqe(session_id, 1, 0, false, 42);
        assert!(shard.conns.get(&session_id).unwrap().pending_recv.is_some());
        assert_eq!(
            shard.conns.get(&session_id).unwrap().canceled_recv_token,
            None
        );

        shard.handle_recv_cqe(session_id, 1, 0, false, 43);
        assert!(shard.conns.get(&session_id).unwrap().pending_recv.is_none());
        let result = sync_rx
            .try_recv()
            .expect("sync recv response should be sent");
        assert!(result.is_ok());
        assert_eq!(result.expect("reply should be Ok"), vec![0]);
    }

    #[test]
    fn provided_recv_slot_remains_inflight_until_successful_recv_completion() {
        let mut pool = ProvidedRecvPool::new(7, 1);
        let slot = pool.slot_id_for_bid(0).expect("slot id");
        assert!(pool.provide_entry_for_slot(slot).is_some());
        assert!(matches!(
            pool.slot_state_for_bid(0),
            Some(BufSlotState::InFlight)
        ));

        assert!(pool.complete_recv(slot));
        assert!(matches!(
            pool.slot_state_for_bid(0),
            Some(BufSlotState::Free)
        ));

        let slot = pool.slot_id_for_bid(0).expect("slot id after free");
        assert!(pool.provide_entry_for_slot(slot).is_some());
        pool.begin_draining();
        assert!(matches!(
            pool.slot_state_for_bid(0),
            Some(BufSlotState::InFlight)
        ));
        assert!(pool.complete_recv(slot));
        assert!(matches!(
            pool.slot_state_for_bid(0),
            Some(BufSlotState::Retired)
        ));
    }

    #[test]
    #[ignore = "requires Rustler resource registration from NIF init"]
    fn request_stop_cancels_recv_before_connection_close() {
        let mut shard = make_shard_state();
        let owner = zero_owner();
        let handle = shard
            .install_connection_handle_raw(owner, 0)
            .expect("session id should advance");
        let session_id = match &handle.kind {
            SocketKind::Session(session_state) => session_state.session_id,
            _ => unreachable!(),
        };

        {
            let conn = shard.conns.get_mut(&session_id).unwrap();
            conn.read_in_flight = true;
            conn.read_in_flight_token = Some(777);
            conn.read_in_flight_ring = Some(RingKind::Latency);
        }

        shard.request_stop();

        assert!(matches!(
            shard.stop_state,
            ShardStopState::StopPending | ShardStopState::Draining
        ));
        assert!(shard.conns.contains(&session_id));
        assert!(shard
            .conns
            .get(&session_id)
            .is_some_and(|conn| conn.stop_close_on_recv_cqe));
        let has_cancel = shard
            .latency_ops
            .values()
            .any(|op| matches!(op, Op::CancelRecv(_)));
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let has_cancel = has_cancel
            || shard
                .main_ops
                .values()
                .any(|op| matches!(op, Op::CancelRecv(_)));
        assert!(has_cancel);
    }

    #[test]
    #[ignore = "requires Rustler resource registration from NIF init"]
    fn close_enqueue_cancels_recv_before_connection_close() {
        let mut shard = make_shard_state();
        let owner = zero_owner();
        let handle = shard
            .install_connection_handle_raw(owner, 0)
            .expect("session id should advance");
        let session_id = match &handle.kind {
            SocketKind::Session(session_state) => session_state.session_id,
            _ => unreachable!(),
        };

        {
            let conn = shard.conns.get_mut(&session_id).unwrap();
            conn.read_in_flight = true;
            conn.read_in_flight_token = Some(31337);
            conn.read_in_flight_ring = Some(RingKind::Latency);
        }

        shard.handle_command(
            Command::CloseEnqueue {
                session_id,
                reason: NifError::Closed,
            },
            false,
        );

        assert!(shard.conns.contains(&session_id));
        assert!(shard
            .conns
            .get(&session_id)
            .is_some_and(|conn| conn.stop_close_on_recv_cqe));
        let has_cancel = shard
            .latency_ops
            .values()
            .any(|op| matches!(op, Op::CancelRecv(id) if *id == session_id));
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let has_cancel = has_cancel
            || shard
                .main_ops
                .values()
                .any(|op| matches!(op, Op::CancelRecv(id) if *id == session_id));
        assert!(has_cancel);
    }

    #[test]
    fn alloc_session_id_exhaustion_returns_none() {
        let mut shard = make_shard_state();
        shard.next_session_id = (1_u64 << 56) - 2;
        assert_eq!(shard.alloc_session_id(), Some((1_u64 << 56) - 2));
        assert_eq!(shard.alloc_session_id(), None);
    }

    #[test]
    fn run_hosted_compile_guard_accepts_shard_loop_signature() {
        fn compile_only_loop(_: &mut ShardState) {}

        let mut shard = make_shard_state();
        let run_loop: fn(&mut ShardState) = compile_only_loop;
        shard.run_hosted(run_loop);
    }

    #[test]
    fn run_hosted_bounded_smoke_runs_single_no_work_hosted_step() {
        let mut shard = make_shard_state();
        let mut hosted_steps = 0usize;

        shard.run_hosted(|state| {
            hosted_steps += 1;
            assert_eq!(state.control.pending_commands.load(Ordering::Acquire), 0);
            assert!(state.listeners.is_empty());
            assert!(state.conns.is_empty());
            assert!(state.subscriptions.is_empty());
            assert!(state.deferred_close_fds.is_empty());
            assert!(!state.local.has_work());
            state.request_stop();
        });

        assert_eq!(hosted_steps, 1);
        assert!(!matches!(shard.stop_state, ShardStopState::Running));
    }

    #[test]
    fn run_hosted_bounded_smoke_processes_single_command_before_stop() {
        let (mut shard, tx) = make_shard_state_with_tx();
        crate::runtime_ingress::ingress_send(&tx, vec![Command::Stop])
            .expect("send single command");
        shard
            .control
            .pending_commands
            .fetch_add(1, Ordering::Release);

        let mut hosted_steps = 0usize;

        shard.run_hosted(|state| {
            hosted_steps += 1;
            assert_eq!(state.control.pending_commands.load(Ordering::Acquire), 1);
            assert_eq!(state.drain_commands(), 1);
            assert_eq!(state.control.pending_commands.load(Ordering::Acquire), 0);
            state.request_stop();
        });

        assert_eq!(hosted_steps, 1);
        assert!(!matches!(shard.stop_state, ShardStopState::Running));
    }

    #[test]
    fn run_hosted_bounded_smoke_teardown_after_one_step() {
        let mut shard = make_shard_state();
        let mut hosted_steps = 0usize;

        shard.run_hosted(|state| {
            hosted_steps += 1;
            state.request_stop();
        });

        assert_eq!(hosted_steps, 1);
        assert!(shard.is_idle_for_teardown());
        assert!(!matches!(shard.stop_state, ShardStopState::Running));
    }

    #[test]
    fn stats_logging_tick_advances_at_the_shard_step_boundary() {
        let mut shard = make_shard_state();
        shard.stats.log_every = true;
        shard.stats.next_log_cqes = 1;
        shard.stats.latency_cqes = 1;

        shard.tick_stats_logging();

        assert_eq!(shard.stats.next_log_cqes, 1 + STATS_LOG_INTERVAL);
    }

    #[test]
    fn drain_commands_preserves_budget_across_oversized_batch() {
        let (mut shard, tx) = make_shard_state_with_tx();
        let batch_len = COMMAND_BUDGET + 1;
        let batch = std::iter::repeat_with(|| Command::Stop)
            .take(batch_len)
            .collect::<Vec<_>>();

        crate::runtime_ingress::ingress_send(&tx, batch).expect("send oversized batch");
        shard
            .control
            .pending_commands
            .fetch_add(batch_len, Ordering::Release);

        assert_eq!(shard.drain_commands(), COMMAND_BUDGET);
        assert_eq!(shard.control.pending_commands.load(Ordering::Acquire), 1);
        assert_eq!(shard.drain_commands(), 1);
        assert_eq!(shard.control.pending_commands.load(Ordering::Acquire), 0);
    }

    #[test]
    fn drain_commands_consumes_a_single_ingress_batch_in_one_hosted_step() {
        let (mut shard, tx) = make_shard_state_with_tx();
        let batch = vec![Command::Stop, Command::Stop, Command::Stop];

        crate::runtime_ingress::ingress_send(&tx, batch).expect("send batch");
        shard
            .control
            .pending_commands
            .fetch_add(3, Ordering::Release);

        assert_eq!(shard.drain_commands(), 3);
        assert_eq!(shard.control.pending_commands.load(Ordering::Acquire), 0);
    }

    #[test]
    fn drain_commands_flushes_deferred_submit_before_exec_lane() {
        let (mut shard, tx) = make_shard_state_with_tx();
        let link = 0xfeed_cafe_u64;
        let batch = vec![
            Command::linked(
                link,
                Command::SendEnqueue {
                    session_id: 9_001,
                    data: vec![1, 2, 3],
                },
            ),
            Command::linked(
                link,
                Command::SendEnqueue {
                    session_id: 9_002,
                    data: vec![4, 5, 6],
                },
            ),
            Command::SetMailboxPassiveEnqueue {
                session_id: 9_003,
                enabled: true,
            },
            Command::Stop,
        ];

        crate::runtime_ingress::ingress_send(&tx, batch).expect("send deferred submit batch");
        shard
            .control
            .pending_commands
            .fetch_add(4, Ordering::Release);

        assert_eq!(shard.drain_commands(), 4);
        assert!(shard.deferred_submit_buf.is_empty());
        assert_eq!(shard.control.pending_commands.load(Ordering::Acquire), 0);
    }

    #[test]
    fn drain_commands_and_ready_preserve_deferred_exec_reply_flow() {
        let (mut shard, tx) = make_shard_state_with_tx();
        shard.result_direct_send = false;
        let batch = vec![
            Command::NoopAsync {
                request_id: 7,
                target: ResultTarget::Erlang(zero_owner()),
            },
            Command::Stop,
        ];

        crate::runtime_ingress::ingress_send(&tx, batch).expect("send deferred exec batch");
        shard
            .control
            .pending_commands
            .fetch_add(2, Ordering::Release);

        assert_eq!(shard.drain_commands(), 2);
        assert!(shard.result_reduce.pending.is_empty());
        assert_eq!(shard.drain_latency_queue().expect("drain latency"), 2);
        assert_eq!(shard.result_reduce.pending.len(), 1);
        assert!(matches!(
            {
                let outcome = shard.maybe_schedule_reduce_results(ResultReduceTrigger::Idle);
                assert_eq!(shard.latency_queue_len, 0);
                assert!(shard.enqueue_result_callback_outcome(outcome));
                outcome
            },
            ResultCallbackOutcome::ReduceResults
        ));
        assert!(shard.result_reduce.reducer_scheduled);
        assert_eq!(shard.latency_queue_len, 1);
        assert_eq!(shard.control.pending_commands.load(Ordering::Acquire), 0);
    }

    #[test]
    fn result_direct_send_ready_requires_idle_reduce_and_empty_result_slots() {
        let mut shard = make_shard_state();
        shard.result_direct_send = true;

        assert!(shard.result_direct_send_ready());

        shard.result_reduce.reducer_scheduled = true;
        assert!(!shard.result_direct_send_ready());

        shard.result_reduce.reducer_scheduled = false;
        shard
            .result_batches
            .push(ResultBatchSlot::new(ResultTarget::Erlang(zero_owner())));
        shard.result_batch_nonempty_slots = 1;
        assert!(!shard.result_direct_send_ready());
    }

    #[test]
    fn queue_reply_defers_when_result_batch_slots_are_busy() {
        let mut shard = make_shard_state();
        shard.result_direct_send = true;
        let target = ResultTarget::Erlang(zero_owner());
        shard
            .result_batches
            .push(ResultBatchSlot::new(target.clone()));
        shard.result_batch_nonempty_slots = 1;
        shard.result_batches[0]
            .entries
            .push_back((1, AsyncReplyPayload::Ok, 0));

        shard.queue_reply_ok(target, 18);

        assert_eq!(shard.result_reduce.pending.len(), 1);
        assert!(shard.result_reduce.first_result_ns.is_some());
        assert_eq!(shard.stats.send_batches_sent, 0);
        assert_eq!(shard.stats.send_items_sent, 0);
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    #[test]
    fn process_completions_fair_budget_preserves_requested_cqe_limit() {
        let mut shard = make_shard_state();
        let budget = 8usize;
        let total_cqes = budget * 2;
        submit_nops(&mut shard.latency_ring, total_cqes);

        let (latency_cqes, main_cqes) = shard.process_completions_fair_budget(budget);
        let (remaining_latency_cqes, remaining_main_cqes) =
            shard.process_completions_fair_budget(budget);

        assert_eq!(latency_cqes + main_cqes, budget);
        assert_eq!(latency_cqes, budget);
        assert_eq!(main_cqes, 0);
        assert_eq!(remaining_latency_cqes + remaining_main_cqes, budget);
        assert_eq!(remaining_latency_cqes, budget);
        assert_eq!(remaining_main_cqes, 0);
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    #[test]
    fn pump_step_compile_guard_accepts_default_step_signature() {
        fn compile_only_step(_: &mut ShardState) -> ShardPumpStep {
            ShardPumpStep::Stop
        }

        let mut shard = make_shard_state();
        let _ = compile_only_step(&mut shard);
    }

    #[cfg(feature = "exec-strategy-sqpoll")]
    #[test]
    fn pump_step_compile_guard_accepts_sqpoll_step_signature() {
        fn compile_only_step(_: &mut ShardState) -> ShardPumpStep {
            ShardPumpStep::Stop
        }

        let mut shard = make_shard_state();
        let _ = compile_only_step(&mut shard);
    }

    // advance_session_id tests moved to runtime_shard_recv::tests (DP-CC-0096)
}
