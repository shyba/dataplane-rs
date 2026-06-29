use std::sync::OnceLock;
use std::time::Instant;

use crate::runtime_protocol::RuntimeStatsSnapshot;

pub(super) const STATS_LOG_INTERVAL: u64 = 100_000;

pub(super) mod labels {
    pub(crate) const STATS: &str = "ranch_uring";
    pub(crate) const LOOP: &str = "ranch_uring loop";
}

macro_rules! stats_inc {
    ($stats:expr, $field:ident) => {{
        $stats.$field = $stats.$field.saturating_add(1);
    }};
}

macro_rules! stats_add {
    ($stats:expr, $field:ident, $value:expr) => {{
        let value = $value;
        $stats.$field = $stats.$field.saturating_add(value);
    }};
}

macro_rules! stats_max {
    ($stats:expr, $field:ident, $value:expr) => {{
        let value = $value;
        if value > $stats.$field {
            $stats.$field = value;
        }
    }};
}

macro_rules! stats_add_max {
    ($stats:expr, $count_field:ident, $total_field:ident, $max_field:ident, $value:expr) => {{
        let value = $value;
        $crate::runtime_stats::stats_inc!($stats, $count_field);
        $crate::runtime_stats::stats_add!($stats, $total_field, value);
        $crate::runtime_stats::stats_max!($stats, $max_field, value);
    }};
}

pub(crate) use stats_add;
pub(crate) use stats_add_max;
pub(crate) use stats_inc;
pub(crate) use stats_max;

#[derive(Default)]
#[repr(align(64))]
pub(super) struct ShardStats {
    pub(super) commands_drained: u64,
    pub(super) ready_driven: u64,
    pub(super) write_ready_enqueued: u64,
    pub(super) write_ready_drained: u64,
    pub(super) latency_cqes: u64,
    pub(super) main_cqes: u64,
    pub(super) cqe_overflow_events: u64,
    pub(super) latency_cq_overflow: u64,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) main_cq_overflow: u64,
    pub(super) last_latency_cq_overflow: u32,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) last_main_cq_overflow: u32,
    pub(super) cqe_overflow_backoff_until_ns: u64,
    pub(super) recv_sqes: u64,
    pub(super) recv_cqes: u64,
    pub(super) recv_wouldblock: u64,
    pub(super) recv_bytes: u64,
    pub(super) read_poll_from_recv_eagain: u64,
    pub(super) recv_wait_completed: u64,
    pub(super) recv_wait_completed_error: u64,
    pub(super) recv_wait_canceled: u64,
    pub(super) recv_wait_timeout_canceled: u64,
    pub(super) recv_wait_ns_total: u64,
    pub(super) recv_wait_ns_max: u64,
    pub(super) recv_wait_le_1us: u64,
    pub(super) recv_wait_le_5us: u64,
    pub(super) recv_wait_le_20us: u64,
    pub(super) recv_wait_le_100us: u64,
    pub(super) recv_wait_le_500us: u64,
    pub(super) recv_wait_le_1ms: u64,
    pub(super) recv_wait_le_5ms: u64,
    pub(super) recv_wait_gt_5ms: u64,
    pub(super) recv_ingress_wait_count: u64,
    pub(super) recv_ingress_wait_ns_total: u64,
    pub(super) recv_ingress_wait_ns_max: u64,
    pub(super) recv_sync_ingress_wait_count: u64,
    pub(super) recv_sync_ingress_wait_ns_total: u64,
    pub(super) recv_sync_ingress_wait_ns_max: u64,
    pub(super) send_sync_ingress_wait_count: u64,
    pub(super) send_sync_ingress_wait_ns_total: u64,
    pub(super) send_sync_ingress_wait_ns_max: u64,
    pub(super) listener_accept_wait_count: u64,
    pub(super) listener_accept_wait_ns_total: u64,
    pub(super) listener_accept_wait_ns_max: u64,
    pub(super) subscribe_accept_wait_count: u64,
    pub(super) subscribe_accept_wait_ns_total: u64,
    pub(super) subscribe_accept_wait_ns_max: u64,
    pub(super) recv_buf_append_count: u64,
    pub(super) recv_buf_append_bytes: u64,
    pub(super) recv_buf_tail_append_count: u64,
    pub(super) recv_buf_tail_append_bytes: u64,
    pub(super) write_sqes: u64,
    pub(super) write_cqes: u64,
    pub(super) read_polls: u64,
    pub(super) write_polls: u64,
    pub(super) cancel_recv_cqes: u64,
    pub(super) cancel_listener_accept_cqes: u64,
    pub(super) close_fd_cqes: u64,
    pub(super) reprovide_sqes: u64,
    pub(super) rx_queue_peak: usize,
    pub(super) tx_queue_peak: usize,
    pub(super) result_events_enqueued: u64,
    pub(super) result_reduce_runs_size: u64,
    pub(super) result_reduce_runs_age: u64,
    pub(super) result_reduce_runs_idle: u64,
    pub(super) result_events_reduced: u64,
    pub(super) send_tasks_enqueued: u64,
    pub(super) send_batches_sent: u64,
    pub(super) send_items_sent: u64,
    pub(super) send_failures: u64,
    pub(super) result_send_wait_count: u64,
    pub(super) result_send_wait_ns_total: u64,
    pub(super) result_send_wait_ns_max: u64,
    pub(super) callback_fifo_peak: usize,
    pub(super) callback_fifo_spill_count: u64,
    pub(super) log_every: bool,
    pub(super) next_log_cqes: u64,
}

impl ShardStats {
    pub(super) fn new() -> Self {
        Self {
            log_every: std::env::var("RANCH_URING_DEBUG_STATS").ok().as_deref() == Some("1"),
            next_log_cqes: STATS_LOG_INTERVAL,
            ..Self::default()
        }
    }

    pub(super) fn total_cqes(&self) -> u64 {
        self.latency_cqes + self.main_cqes
    }

    #[inline]
    pub(super) fn record_recv_wait_completed(&mut self, waited_ns: u64, is_ok: bool) {
        stats_inc!(self, recv_wait_completed);
        if !is_ok {
            stats_inc!(self, recv_wait_completed_error);
        }
        self.record_recv_wait(waited_ns);
    }

    #[inline]
    pub(super) fn record_recv_wait_canceled(&mut self, waited_ns: u64, timed_out: bool) {
        stats_inc!(self, recv_wait_canceled);
        if timed_out {
            stats_inc!(self, recv_wait_timeout_canceled);
        }
        self.record_recv_wait(waited_ns);
    }

    #[inline]
    fn record_recv_wait(&mut self, waited_ns: u64) {
        stats_add!(self, recv_wait_ns_total, waited_ns);
        stats_max!(self, recv_wait_ns_max, waited_ns);
        if waited_ns <= 1_000 {
            stats_inc!(self, recv_wait_le_1us);
        } else if waited_ns <= 5_000 {
            stats_inc!(self, recv_wait_le_5us);
        } else if waited_ns <= 20_000 {
            stats_inc!(self, recv_wait_le_20us);
        } else if waited_ns <= 100_000 {
            stats_inc!(self, recv_wait_le_100us);
        } else if waited_ns <= 500_000 {
            stats_inc!(self, recv_wait_le_500us);
        } else if waited_ns <= 1_000_000 {
            stats_inc!(self, recv_wait_le_1ms);
        } else if waited_ns <= 5_000_000 {
            stats_inc!(self, recv_wait_le_5ms);
        } else {
            stats_inc!(self, recv_wait_gt_5ms);
        }
    }

    #[inline]
    pub(super) fn record_recv_ingress_wait(&mut self, waited_ns: u64) {
        stats_add_max!(
            self,
            recv_ingress_wait_count,
            recv_ingress_wait_ns_total,
            recv_ingress_wait_ns_max,
            waited_ns
        );
    }

    #[inline]
    pub(super) fn record_recv_sync_ingress_wait(&mut self, waited_ns: u64) {
        stats_add_max!(
            self,
            recv_sync_ingress_wait_count,
            recv_sync_ingress_wait_ns_total,
            recv_sync_ingress_wait_ns_max,
            waited_ns
        );
    }

    #[inline]
    pub(super) fn record_send_sync_ingress_wait(&mut self, waited_ns: u64) {
        stats_add_max!(
            self,
            send_sync_ingress_wait_count,
            send_sync_ingress_wait_ns_total,
            send_sync_ingress_wait_ns_max,
            waited_ns
        );
    }

    #[inline]
    pub(super) fn record_listener_accept_wait(&mut self, waited_ns: u64) {
        stats_add_max!(
            self,
            listener_accept_wait_count,
            listener_accept_wait_ns_total,
            listener_accept_wait_ns_max,
            waited_ns
        );
    }

    #[inline]
    pub(super) fn record_subscribe_accept_wait(&mut self, waited_ns: u64) {
        stats_add_max!(
            self,
            subscribe_accept_wait_count,
            subscribe_accept_wait_ns_total,
            subscribe_accept_wait_ns_max,
            waited_ns
        );
    }

    #[inline]
    pub(super) fn record_result_send_wait(&mut self, waited_ns: u64) {
        stats_add_max!(
            self,
            result_send_wait_count,
            result_send_wait_ns_total,
            result_send_wait_ns_max,
            waited_ns
        );
    }

    pub(super) fn snapshot(
        &self,
        shard: usize,
        conn_count: usize,
        ready_depth: usize,
        write_ready_depth: usize,
    ) -> RuntimeStatsSnapshot {
        RuntimeStatsSnapshot {
            shard,
            commands_drained: self.commands_drained,
            ready_driven: self.ready_driven,
            write_ready_enqueued: self.write_ready_enqueued,
            write_ready_drained: self.write_ready_drained,
            latency_cqes: self.latency_cqes,
            main_cqes: self.main_cqes,
            recv_sqes: self.recv_sqes,
            recv_cqes: self.recv_cqes,
            recv_wouldblock: self.recv_wouldblock,
            recv_bytes: self.recv_bytes,
            read_poll_from_recv_eagain: self.read_poll_from_recv_eagain,
            recv_wait_completed: self.recv_wait_completed,
            recv_wait_completed_error: self.recv_wait_completed_error,
            recv_wait_canceled: self.recv_wait_canceled,
            recv_wait_timeout_canceled: self.recv_wait_timeout_canceled,
            recv_wait_ns_total: self.recv_wait_ns_total,
            recv_wait_ns_max: self.recv_wait_ns_max,
            recv_wait_le_1us: self.recv_wait_le_1us,
            recv_wait_le_5us: self.recv_wait_le_5us,
            recv_wait_le_20us: self.recv_wait_le_20us,
            recv_wait_le_100us: self.recv_wait_le_100us,
            recv_wait_le_500us: self.recv_wait_le_500us,
            recv_wait_le_1ms: self.recv_wait_le_1ms,
            recv_wait_le_5ms: self.recv_wait_le_5ms,
            recv_wait_gt_5ms: self.recv_wait_gt_5ms,
            recv_ingress_wait_count: self.recv_ingress_wait_count,
            recv_ingress_wait_ns_total: self.recv_ingress_wait_ns_total,
            recv_ingress_wait_ns_max: self.recv_ingress_wait_ns_max,
            recv_sync_ingress_wait_count: self.recv_sync_ingress_wait_count,
            recv_sync_ingress_wait_ns_total: self.recv_sync_ingress_wait_ns_total,
            recv_sync_ingress_wait_ns_max: self.recv_sync_ingress_wait_ns_max,
            send_sync_ingress_wait_count: self.send_sync_ingress_wait_count,
            send_sync_ingress_wait_ns_total: self.send_sync_ingress_wait_ns_total,
            send_sync_ingress_wait_ns_max: self.send_sync_ingress_wait_ns_max,
            listener_accept_wait_count: self.listener_accept_wait_count,
            listener_accept_wait_ns_total: self.listener_accept_wait_ns_total,
            listener_accept_wait_ns_max: self.listener_accept_wait_ns_max,
            subscribe_accept_wait_count: self.subscribe_accept_wait_count,
            subscribe_accept_wait_ns_total: self.subscribe_accept_wait_ns_total,
            subscribe_accept_wait_ns_max: self.subscribe_accept_wait_ns_max,
            recv_buf_append_count: self.recv_buf_append_count,
            recv_buf_append_bytes: self.recv_buf_append_bytes,
            recv_buf_tail_append_count: self.recv_buf_tail_append_count,
            recv_buf_tail_append_bytes: self.recv_buf_tail_append_bytes,
            write_sqes: self.write_sqes,
            write_cqes: self.write_cqes,
            read_polls: self.read_polls,
            write_polls: self.write_polls,
            cancel_recv_cqes: self.cancel_recv_cqes,
            cancel_listener_accept_cqes: self.cancel_listener_accept_cqes,
            close_fd_cqes: self.close_fd_cqes,
            reprovide_sqes: self.reprovide_sqes,
            rx_queue_peak: self.rx_queue_peak,
            tx_queue_peak: self.tx_queue_peak,
            conn_count,
            ready_depth,
            write_ready_depth,
            result_events_enqueued: self.result_events_enqueued,
            result_reduce_runs_size: self.result_reduce_runs_size,
            result_reduce_runs_age: self.result_reduce_runs_age,
            result_reduce_runs_idle: self.result_reduce_runs_idle,
            result_events_reduced: self.result_events_reduced,
            send_tasks_enqueued: self.send_tasks_enqueued,
            send_batches_sent: self.send_batches_sent,
            send_items_sent: self.send_items_sent,
            send_failures: self.send_failures,
            result_send_wait_count: self.result_send_wait_count,
            result_send_wait_ns_total: self.result_send_wait_ns_total,
            result_send_wait_ns_max: self.result_send_wait_ns_max,
            callback_fifo_peak: self.callback_fifo_peak,
            callback_fifo_spill_count: self.callback_fifo_spill_count,
        }
    }
}

#[inline]
pub(super) fn monotonic_ns() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recv_wait_canceled_timeout_updates_timeout_counter_and_histogram() {
        let mut stats = ShardStats::default();

        stats.record_recv_wait_canceled(1_500, true);

        assert_eq!(stats.recv_wait_canceled, 1);
        assert_eq!(stats.recv_wait_timeout_canceled, 1);
        assert_eq!(stats.recv_wait_ns_total, 1_500);
        assert_eq!(stats.recv_wait_ns_max, 1_500);
        assert_eq!(stats.recv_wait_le_5us, 1);
    }

    #[test]
    fn recv_wait_canceled_without_timeout_preserves_timeout_counter() {
        let mut stats = ShardStats::default();

        stats.record_recv_wait_canceled(6_000_000, false);

        assert_eq!(stats.recv_wait_canceled, 1);
        assert_eq!(stats.recv_wait_timeout_canceled, 0);
        assert_eq!(stats.recv_wait_ns_total, 6_000_000);
        assert_eq!(stats.recv_wait_ns_max, 6_000_000);
        assert_eq!(stats.recv_wait_gt_5ms, 1);
    }
}
