//! Configuration parsing helpers for the runtime.
//!
//! This file is a private `#[path]` child of `runtime`; helpers are re-exported
//! there for sibling modules. `ShardRuntimeConfig` snapshots environment settings
//! at startup so the shard loop need not repeatedly parse them.

#[cfg(not(feature = "exec-strategy-sqpoll"))]
use crate::runtime_reactor::{SqpollConfig, SqpollMode};

// ---------------------------------------------------------------------------
// Default constants (documented, single-source-of-truth for call sites)
// ---------------------------------------------------------------------------

/// Default idle wait nanoseconds when SQPOLL is accepted (5 µs).
pub const IDLE_WAIT_NS_SQPOLL: u64 = 5_000;
/// Default idle wait nanoseconds when SQPOLL is not accepted (100 ns).
pub const IDLE_WAIT_NS_NO_SQPOLL: u64 = 100;
/// Default main ring submit batch size.
#[cfg(not(feature = "exec-strategy-sqpoll"))]
#[allow(dead_code)]
pub const MAIN_SUBMIT_BATCH_DEFAULT: usize = 8;
/// Default main ring submit max delay nanoseconds (2 µs).
#[cfg(not(feature = "exec-strategy-sqpoll"))]
#[allow(dead_code)]
pub const MAIN_SUBMIT_MAX_DELAY_NS_DEFAULT: u64 = 2_000;
/// Default SQPOLL idle batch size.
pub const SQPOLL_IDLE_BATCH_DEFAULT: usize = 8;
/// Default SQPOLL idle wait microseconds.
pub const SQPOLL_IDLE_USEC_DEFAULT: u32 = 250;

// Variants are only constructed in non-SQPOLL builds.
#[cfg_attr(feature = "exec-strategy-sqpoll", allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecvRingMode {
    Latency,
    Main,
    Both,
}

pub(crate) fn configured_debug_loop_queues() -> bool {
    matches!(
        std::env::var("RANCH_URING_DEBUG_LOOP_QUEUES")
            .ok()
            .as_deref()
            .map(|s| s.trim().to_ascii_lowercase()),
        Some(ref v) if v == "1" || v == "true" || v == "yes" || v == "on"
    )
}

pub(crate) fn configured_debug_loop_every() -> u64 {
    std::env::var("RANCH_URING_DEBUG_LOOP_EVERY")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(1)
}

pub(crate) fn configured_stop_drain_profile() -> bool {
    match std::env::var("RANCH_URING_STOP_DRAIN_PROFILE")
        .ok()
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
    {
        Some(ref s) if s == "1" || s == "true" || s == "yes" || s == "on" => true,
        Some(ref s) if s == "0" || s == "false" || s == "no" || s == "off" => false,
        Some(_) => false,
        None => false,
    }
}

pub(crate) fn configured_result_direct_send() -> bool {
    match std::env::var("RANCH_URING_RESULT_DIRECT_SEND")
        .ok()
        .as_deref()
        .map(|s| s.trim())
    {
        Some("0") => false,
        Some("1") | None => true,
        _ => true,
    }
}

pub(crate) fn configured_idle_wait_ns(sqpoll_accepted: bool) -> u64 {
    std::env::var("RANCH_URING_NO_PROGRESS_WAIT_NS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(if sqpoll_accepted {
            IDLE_WAIT_NS_SQPOLL
        } else {
            IDLE_WAIT_NS_NO_SQPOLL
        })
}

#[cfg(not(feature = "exec-strategy-sqpoll"))]
pub(crate) fn parse_sqpoll_mode_env(name: &str) -> Option<SqpollMode> {
    let value = std::env::var(name).ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Some(SqpollMode::Off),
        "try" => Some(SqpollMode::Try),
        "require" => Some(SqpollMode::Require),
        _ => None,
    }
}

#[cfg(not(feature = "exec-strategy-sqpoll"))]
pub(crate) fn configured_latency_sqpoll_config(base: SqpollConfig) -> SqpollConfig {
    let mode = parse_sqpoll_mode_env("RANCH_URING_SQPOLL_LATENCY").unwrap_or(base.mode);
    SqpollConfig { mode, ..base }
}

#[cfg(not(feature = "exec-strategy-sqpoll"))]
pub(crate) fn configured_main_sqpoll_config(base: SqpollConfig) -> SqpollConfig {
    let mode = parse_sqpoll_mode_env("RANCH_URING_SQPOLL_MAIN")
        .or_else(|| parse_sqpoll_mode_env("RANCH_URING_SQPOLL_SECOND"))
        .unwrap_or(base.mode);
    SqpollConfig { mode, ..base }
}

#[cfg(not(feature = "exec-strategy-sqpoll"))]
pub(crate) fn configured_main_submit_batch() -> usize {
    std::env::var("RANCH_URING_MAIN_SUBMIT_BATCH")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(MAIN_SUBMIT_BATCH_DEFAULT)
}

#[cfg(not(feature = "exec-strategy-sqpoll"))]
pub(crate) fn configured_main_submit_max_delay_ns() -> u64 {
    std::env::var("RANCH_URING_MAIN_SUBMIT_MAX_DELAY_NS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(MAIN_SUBMIT_MAX_DELAY_NS_DEFAULT)
}

#[cfg(not(feature = "exec-strategy-sqpoll"))]
pub(crate) fn configured_recv_ring_mode() -> RecvRingMode {
    match std::env::var("RANCH_URING_RECV_RING")
        .ok()
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("main") => RecvRingMode::Main,
        Some("both") => RecvRingMode::Both,
        _ => RecvRingMode::Latency,
    }
}

pub(crate) fn configured_sqpoll_idle_batch() -> usize {
    std::env::var("RANCH_URING_SQPOLL_WAIT_BATCH")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(SQPOLL_IDLE_BATCH_DEFAULT)
}

pub(crate) fn configured_sqpoll_idle_usec() -> u32 {
    std::env::var("RANCH_URING_SQPOLL_WAIT_USEC")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(SQPOLL_IDLE_USEC_DEFAULT)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ShardRuntimeConfig {
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    main_submit_batch: usize,

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    main_submit_max_delay_ns: u64,

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    recv_ring_mode: RecvRingMode,

    result_direct_send: bool,
    debug_loop_queues: bool,
    debug_loop_every: u64,
    idle_wait_ns: u64,
    sqpoll_idle_batch: usize,
    sqpoll_idle_usec: u32,
}

impl ShardRuntimeConfig {
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(crate) fn from_env(latency_sqpoll_accepted: bool) -> Self {
        use crate::runtime_reactor::configured_ring_size;
        let ring_entries = configured_ring_size().max(1) as usize;
        Self {
            main_submit_batch: configured_main_submit_batch().clamp(1, ring_entries),
            main_submit_max_delay_ns: configured_main_submit_max_delay_ns(),
            recv_ring_mode: configured_recv_ring_mode(),
            result_direct_send: configured_result_direct_send(),
            debug_loop_queues: configured_debug_loop_queues(),
            debug_loop_every: configured_debug_loop_every(),
            idle_wait_ns: configured_idle_wait_ns(latency_sqpoll_accepted),
            sqpoll_idle_batch: configured_sqpoll_idle_batch(),
            sqpoll_idle_usec: configured_sqpoll_idle_usec(),
        }
    }

    #[cfg(feature = "exec-strategy-sqpoll")]
    pub(crate) fn from_env(sqpoll_accepted: bool) -> Self {
        Self {
            result_direct_send: configured_result_direct_send(),
            debug_loop_queues: configured_debug_loop_queues(),
            debug_loop_every: configured_debug_loop_every(),
            idle_wait_ns: configured_idle_wait_ns(sqpoll_accepted),
            sqpoll_idle_batch: configured_sqpoll_idle_batch(),
            sqpoll_idle_usec: configured_sqpoll_idle_usec(),
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn test_new(
        #[cfg(not(feature = "exec-strategy-sqpoll"))] main_submit_batch: usize,
        #[cfg(not(feature = "exec-strategy-sqpoll"))] main_submit_max_delay_ns: u64,
        result_direct_send: bool,
        debug_loop_queues: bool,
        debug_loop_every: u64,
        idle_wait_ns: u64,
        sqpoll_idle_batch: usize,
        sqpoll_idle_usec: u32,
    ) -> Self {
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let recv_ring_mode = RecvRingMode::Latency;
        Self {
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            main_submit_batch,
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            main_submit_max_delay_ns,
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            recv_ring_mode,
            result_direct_send,
            debug_loop_queues,
            debug_loop_every,
            idle_wait_ns,
            sqpoll_idle_batch,
            sqpoll_idle_usec,
        }
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(crate) fn main_submit_batch(&self) -> usize {
        self.main_submit_batch
    }
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(crate) fn main_submit_max_delay_ns(&self) -> u64 {
        self.main_submit_max_delay_ns
    }
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(crate) fn recv_ring_mode(&self) -> RecvRingMode {
        self.recv_ring_mode
    }
    pub(crate) fn result_direct_send(&self) -> bool {
        self.result_direct_send
    }
    pub(crate) fn debug_loop_queues(&self) -> bool {
        self.debug_loop_queues
    }
    pub(crate) fn debug_loop_every(&self) -> u64 {
        self.debug_loop_every
    }
    pub(crate) fn idle_wait_ns(&self) -> u64 {
        self.idle_wait_ns
    }
    pub(crate) fn sqpoll_idle_batch(&self) -> usize {
        self.sqpoll_idle_batch
    }
    pub(crate) fn sqpoll_idle_usec(&self) -> u32 {
        self.sqpoll_idle_usec
    }
}
