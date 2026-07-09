use std::collections::VecDeque;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use crate::errors::{NifError, Result};
use crate::runtime_adapter::{
    send_active_closed_message, send_active_data_message, send_active_passive_message,
    send_batch_reply_to, send_passive_closed_message, send_passive_data_message,
    send_passive_error_message, send_result_batch_to, send_result_single_to, send_session_result,
    send_unit_result, AsyncReplyPayload, ResultTarget,
};
use crate::runtime_arena::{ArenaClass, ArenaHandle, RuntimeArenas};
use crate::runtime_boundary::{ShardControl, ShardReceiver, ShardSender};
use crate::runtime_connection_table::ConnectionTable;
use crate::runtime_id_map::U64Map;
use crate::runtime_ingress::{ingress_channel, ingress_try_recv};
use crate::runtime_listener_table::{ListenerTable, SubscriptionTable};
use crate::runtime_protocol::{BatchOp, BatchResult, RuntimeStatsSnapshot};
use crate::runtime_reactor::{
    configured_chunk_arena_slots, configured_locked_read_bufs_per_shard,
    configured_provided_recv_bufs_per_shard, configured_register_subscribe_arena,
    configured_ring_size, configured_shard_count, configured_sqpoll_config,
    configured_subscribe_arena_slots, configured_subscribe_pages_per_shard,
    configured_topology_profile, local_addr_of_fd, make_listener, memlock_limit_bytes,
    LockedReadBufPool, OpTable, ProvidedRecvPool, SqpollCpu, CHANNEL_CAPACITY,
    LISTEN_BACKLOG_DEFAULT,
};
#[cfg(feature = "exec-strategy-sqpoll")]
#[allow(unused_imports)]
use crate::runtime_reactor::{SqpollConfig, SqpollMode};
use crate::runtime_registration::{build_registration_layout, plan_fixed_registration};
use crate::runtime_ring_pair::{RingPairBorrow, RingPairBorrowMut};
use crate::runtime_scheduler::{
    make_selected_scheduler, ScheduledItem, SchedulerPolicy, SelectedScheduler,
};
use crate::runtime_session::{
    active_enabled, rx_front_len, rx_take, Connection, PendingBatch, PendingRecv, PendingRecvReply,
    PendingWrite, RxChunk,
};
#[cfg(test)]
use crate::runtime_startup::RuntimeProfileDispatchSeam;
use crate::runtime_startup::{
    build_ring, dispatch_runtime_startup_profile_layout, map_feature_error,
    probe_required_runtime_features,
};
use crate::runtime_stats::{
    labels as stats_labels, stats_add, stats_inc, stats_max, ShardStats, STATS_LOG_INTERVAL,
};
use crate::runtime_topology::current_thread_shard;
pub(crate) use crate::socket::SessionState;
use crate::socket::{ActiveMode, ListenerState, NetAddr, SocketKind, SocketOpts, SocketRef};
use crossbeam_channel::{unbounded, Sender};
#[cfg(not(feature = "exec-strategy-sqpoll"))]
#[allow(unused_imports)]
use crossbeam_channel::{Receiver, Select};
#[cfg(debug_assertions)]
use dataplane_runtime::runtime_trace::{DebugPhaseStats, LoopPhase, StallReason};
#[cfg(not(feature = "exec-strategy-sqpoll"))]
#[allow(unused_imports)]
use io_uring::types;
use io_uring::IoUring;
use once_cell::sync::{Lazy, OnceCell};
use rustler::{LocalPid, OwnedBinary, ResourceArc};

// ---------------------------------------------------------------------------
// Module wiring notes
// ---------------------------------------------------------------------------
// runtime_command and runtime_config are #[path] submodules of this module.
// They cannot be moved to lib.rs as top-level modules because:
//   - runtime_shard.rs imports Command via: use crate::runtime::Command
//   - runtime_launch.rs re-exports Command from: crate::runtime::Command
// Both of these import paths would break if the modules were relocated.
// The #[path] attribute keeps them as private siblings while the physical
// files remain adjacent to runtime.rs.
//
// All re-exports from extracted modules use pub(crate) visibility — no type
// crosses the crate boundary publicly.  The public surface of this NIF crate
// is controlled by lib.rs and nif.rs.
// ---------------------------------------------------------------------------
// Re-exports from extracted modules (originals removed from this file)
// ---------------------------------------------------------------------------
pub(crate) use super::runtime_launch::ShardLaunchConfig;
#[cfg(feature = "current-thread-shard-driver")]
#[allow(unused_imports)]
pub(crate) use super::runtime_launch::{
    drive_shard_on_current_thread, run_shard_state_on_current_thread,
};
pub(crate) use super::runtime_launch::{shutdown_started_shards, spawn_shard_thread};
// L3: re-export close helpers from extracted runtime_shard_close module
pub(crate) use self::runtime_shard_close::shutdown_kind_from_how;
pub(crate) use crate::runtime_pending_reply::{PendingStatx, StatxOut};
pub(crate) use crate::runtime_result_queue::{
    ResultBatchSlot, ResultCallbackOutcome, ResultEvent, ResultFacet, ResultReduceState,
    ResultReduceTrigger,
};

const DRIVE_BUDGET: usize = 64;
const COMMAND_BUDGET: usize = 128;
const DEFERRED_EXEC_CAP: usize = 8;
const DEFERRED_SUBMIT_CAP: usize = 16;
const CALLBACK_RESERVE_MIN: usize = 4;
const CALLBACK_LOCAL_CAP: usize = 8;
const CALLBACK_FORCE_EXEC_ABOVE: usize = 8;
const LATENCY_QUEUE_CAP: usize = 32;
const RESULT_SEND_BATCH_LIMIT: usize = crate::runtime_result_queue::RESULT_SEND_BATCH_LIMIT;
const SUBSCRIBE_INFLIGHT: usize = 2;

static RUNTIME: Lazy<Mutex<Option<Arc<Runtime>>>> = Lazy::new(|| Mutex::new(None));
pub(crate) static NEXT_CONTROL_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
pub(crate) static NEXT_SUBSCRIBE_SHARD: AtomicU64 = AtomicU64::new(0);

#[path = "runtime_clock.rs"]
mod runtime_clock;
// Re-export clock functions so runtime_helpers can access them.
pub(crate) use runtime_clock::{recv_clock_raw, recv_wait_ns_since};

#[path = "runtime_config.rs"]
mod runtime_config;
pub(crate) use runtime_config::configured_stop_drain_profile;
// RecvRingMode is used in both SQPOLL and non-SQPOLL builds (for ShardRuntimeConfig field)
// but configured_recv_ring_mode() is only available in non-SQPOLL builds.
#[allow(unused_imports)]
pub(crate) use runtime_config::RecvRingMode;
#[cfg(not(feature = "exec-strategy-sqpoll"))]
pub(crate) use runtime_config::{
    configured_latency_sqpoll_config, configured_main_sqpoll_config, configured_recv_ring_mode,
};

#[path = "runtime_helpers.rs"]
mod runtime_helpers;
#[path = "runtime_link_id.rs"]
mod runtime_link_id;
#[path = "runtime_routing.rs"]
mod runtime_routing;
#[path = "runtime_send_path.rs"]
mod runtime_send_path;
#[path = "runtime_subscription.rs"]
mod runtime_subscription;
pub use self::runtime_subscription::Subscription;
pub use self::runtime_subscription::{SubscribeControl, SubscribeOperation};
#[path = "runtime_limits.rs"]
mod runtime_limits;
pub(crate) use self::runtime_limits::{
    CQE_BUDGET, RX_QUEUE_MAX_BYTES, SQPOLL_CHUNK_ARENA_MULTIPLIER_DEN,
    SQPOLL_CHUNK_ARENA_MULTIPLIER_NUM, SQPOLL_SUBSCRIBE_ARENA_MULTIPLIER, TX_QUEUE_MAX_BYTES,
    WRITEV_BATCH,
};
// Re-export helpers for non-SQPOLL sibling modules.
#[cfg(not(feature = "exec-strategy-sqpoll"))]
#[allow(unused_imports)]
pub(super) use runtime_link_id::{
    caller_link_id, global_link_id, hash_pid, link_id, listener_link_id, session_link_id,
};
#[cfg(not(feature = "exec-strategy-sqpoll"))]
#[allow(unused_imports)]
pub(super) use runtime_routing::{shard_for_fd, shard_for_subscribe, shard_from_routed_id};
#[cfg(not(feature = "exec-strategy-sqpoll"))]
#[allow(unused_imports)]
pub(super) use runtime_send_path::{
    flush_staged_commands_all, send_many_to_shard, send_many_to_shard_linked, send_to_shard,
    send_to_shard_linked,
};

// For SQPOLL builds: import routing/link/send_path symbols directly since pub(super) re-exports
// above are inactive for SQPOLL (they are #[cfg(not(feature = "exec-strategy-sqpoll"))]).
#[cfg(feature = "exec-strategy-sqpoll")]
#[allow(unused_imports)]
use runtime_link_id::{
    caller_link_id, global_link_id, hash_pid, link_id, listener_link_id, session_link_id,
};
#[cfg(feature = "exec-strategy-sqpoll")]
#[allow(unused_imports)]
use runtime_routing::{shard_for_fd, shard_for_subscribe, shard_from_routed_id};
#[cfg(feature = "exec-strategy-sqpoll")]
#[allow(unused_imports)]
use runtime_send_path::{
    flush_staged_commands_all, send_many_to_shard, send_many_to_shard_linked, send_to_shard,
    send_to_shard_linked,
};

// retry_eintr and now_monotonic_ns are used in both SQPOLL and non-SQPOLL
// builds. Keep them available before the non-SQPOLL wildcard below.
#[cfg_attr(feature = "exec-strategy-sqpoll", allow(unused_imports))]
use runtime_helpers::{now_monotonic_ns, retry_eintr};

#[path = "runtime_api.rs"]
mod runtime_api;
pub use runtime_api::*;

struct Runtime {
    senders: Vec<ShardSender<Command>>,
    next_listener_id: AtomicU64,
    stopping: AtomicBool,
    join_handles: Mutex<Option<Vec<thread::JoinHandle<()>>>>,
}

fn next_request_id_for_shard(shard: usize) -> Result<u64> {
    let runtime = current_runtime()?;
    let sender = runtime.senders.get(shard).ok_or(NifError::Closed)?;
    let local = sender
        .control
        .next_request_id
        .fetch_add(1, Ordering::Relaxed);
    Ok((((shard + 1) as u64) << 56) | (local & 0x00ff_ffff_ffff_ffff))
}

fn current_runtime() -> Result<Arc<Runtime>> {
    RUNTIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .cloned()
        .ok_or(NifError::Closed)
}

#[inline]
pub(super) fn stop_drain_profile_enabled() -> bool {
    static STOP_DRAIN_PROFILE: OnceCell<bool> = OnceCell::new();
    *STOP_DRAIN_PROFILE.get_or_init(configured_stop_drain_profile)
}

pub(super) fn stat_size_via_fstat(fd: RawFd) -> Result<u64> {
    let mut st = MaybeUninit::<libc::stat>::zeroed();
    // SAFETY: `st` is a valid writable pointer to a `libc::stat` out buffer.
    let rc = unsafe { libc::fstat(fd, st.as_mut_ptr()) };
    if rc == 0 {
        // SAFETY: `fstat` succeeded, so `st` has been fully initialized by libc.
        let st = unsafe { st.assume_init() };
        Ok(st.st_size as u64)
    } else {
        Err(NifError::last_os_error())
    }
}

impl Runtime {
    fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }

    fn start() -> Result<Self> {
        let mut shard_count = configured_shard_count();
        loop {
            match Self::start_with_shards(shard_count) {
                Ok(runtime) => return Ok(runtime),
                Err(NifError::Errno(libc::ENOMEM)) if shard_count > 1 => {
                    shard_count = (shard_count / 2).max(1);
                }
                Err(err) => return Err(err),
            }
        }
    }

    fn start_with_shards(shard_count: usize) -> Result<Self> {
        let ring_size = configured_ring_size();
        let requested_locked_read_bufs = configured_locked_read_bufs_per_shard();
        let (profile_layout, _profile_dispatch_seam) =
            dispatch_runtime_startup_profile_layout(configured_topology_profile(shard_count))?;
        let resolved_topology = profile_layout
            .profile()
            .resolve()
            .map_err(|_| NifError::StartupProfileLayout)?;
        let topology = resolved_topology.topology;
        let shard_count = topology.shard_count();
        let cpus = topology.cpu_plan();
        #[cfg(feature = "exec-strategy-sqpoll")]
        let sqpoll_config = {
            let config = configured_sqpoll_config();
            SqpollConfig {
                mode: SqpollMode::Require,
                cpu: config.cpu,
                idle_ms: config.idle_ms,
            }
        };
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let sqpoll_config = configured_sqpoll_config();
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let latency_sqpoll_config = configured_latency_sqpoll_config(sqpoll_config);
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let main_sqpoll_config = configured_main_sqpoll_config(sqpoll_config);
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let recv_ring_mode = configured_recv_ring_mode();
        let requested_register_subscribe_arena = configured_register_subscribe_arena();
        let memlock_limit = memlock_limit_bytes()?;
        let configured_provided_recv_bufs = configured_provided_recv_bufs_per_shard();
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let provided_recv_bufs = if matches!(recv_ring_mode, RecvRingMode::Latency) {
            configured_provided_recv_bufs
        } else {
            0
        };
        #[cfg(feature = "exec-strategy-sqpoll")]
        let provided_recv_bufs = configured_provided_recv_bufs;
        let require_provided_buffers = provided_recv_bufs > 0;
        let subscribe_pages = configured_subscribe_pages_per_shard();
        let mut senders = Vec::with_capacity(shard_count);
        let mut join_handles = Vec::with_capacity(shard_count);
        let mut setups = Vec::with_capacity(shard_count);
        let mut features_checked = false;

        for (shard, shard_cpu) in cpus.iter().copied().enumerate().take(shard_count) {
            let sqpoll_cpu = match sqpoll_config.cpu {
                SqpollCpu::Shard => Some(shard_cpu),
                SqpollCpu::Fixed(cpu) => Some(cpu),
                SqpollCpu::None => None,
            };
            #[cfg(feature = "exec-strategy-sqpoll")]
            let latency_ring = build_ring(ring_size, sqpoll_config, sqpoll_cpu)?;
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            let latency_ring = build_ring(ring_size, latency_sqpoll_config, sqpoll_cpu)?;
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            let main_ring = build_ring(ring_size, main_sqpoll_config, sqpoll_cpu)?;
            #[cfg(feature = "exec-strategy-sqpoll")]
            let sqpoll_accepted = latency_ring.params().is_setup_sqpoll();
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            let latency_sqpoll_accepted = latency_ring.params().is_setup_sqpoll();
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            let main_sqpoll_accepted = main_ring.params().is_setup_sqpoll();
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            let sqpoll_accepted = latency_sqpoll_accepted || main_sqpoll_accepted;
            // SAFETY: `eventfd` takes only plain integer arguments and returns an owned fd.
            let wakeup_fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
            if wakeup_fd < 0 {
                return Err(NifError::last_os_error());
            }
            let default_chunk_slots = if sqpoll_accepted {
                CHANNEL_CAPACITY.saturating_mul(SQPOLL_CHUNK_ARENA_MULTIPLIER_NUM)
                    / SQPOLL_CHUNK_ARENA_MULTIPLIER_DEN
            } else {
                CHANNEL_CAPACITY
            };
            let default_subscribe_slots = if sqpoll_accepted {
                subscribe_pages
                    .max(SUBSCRIBE_INFLIGHT)
                    .saturating_mul(SQPOLL_SUBSCRIBE_ARENA_MULTIPLIER)
            } else {
                subscribe_pages.max(SUBSCRIBE_INFLIGHT)
            };
            let chunk_slots = configured_chunk_arena_slots()
                .unwrap_or(default_chunk_slots)
                .max(1);
            let subscribe_slots = configured_subscribe_arena_slots()
                .unwrap_or(default_subscribe_slots)
                .max(SUBSCRIBE_INFLIGHT);
            let fixed_registration_plan = plan_fixed_registration(
                requested_locked_read_bufs,
                shard_count,
                memlock_limit,
                requested_register_subscribe_arena,
                subscribe_slots,
            );
            let locked_read_bufs = fixed_registration_plan.locked_read_bufs;
            let register_subscribe_arena = fixed_registration_plan.register_subscribe_arena;
            let require_fixed_buffers = locked_read_bufs > 0 || register_subscribe_arena;
            if !features_checked {
                probe_required_runtime_features(
                    &latency_ring,
                    require_fixed_buffers,
                    require_provided_buffers,
                )?;
                features_checked = true;
            }
            let read_pool = LockedReadBufPool::new(locked_read_bufs)?;
            let provided_recv_pool = if provided_recv_bufs > 0 {
                Some(ProvidedRecvPool::new(shard as u16, provided_recv_bufs))
            } else {
                None
            };
            let arenas = RuntimeArenas::new(chunk_slots, subscribe_slots);
            let registration_layout =
                build_registration_layout(&read_pool, &arenas, register_subscribe_arena);
            debug_assert_eq!(registration_layout.read_fixed_base, 0);
            debug_assert_eq!(
                registration_layout.read_fixed_len,
                registration_layout.read_fixed_slots.len()
            );
            debug_assert_eq!(
                registration_layout
                    .read_fixed_slot_table
                    .iter()
                    .filter(|entry| entry.is_some())
                    .count(),
                registration_layout.read_fixed_slots.len()
            );
            debug_assert_eq!(
                registration_layout.subscribe_fixed_len,
                if register_subscribe_arena {
                    arenas.small_len()
                } else {
                    0
                }
            );
            debug_assert_eq!(
                registration_layout
                    .subscribe_fixed_slot_table
                    .iter()
                    .filter(|entry| entry.is_some())
                    .count(),
                registration_layout.subscribe_fixed_len
            );
            let fixed_buffers_enabled = if require_fixed_buffers {
                // SAFETY: descriptors point to stable allocations that live until explicit
                // unregister on shard teardown.
                unsafe {
                    latency_ring
                        .submitter()
                        .register_buffers(&registration_layout.descriptors)
                        .map_err(map_feature_error)?;
                }
                true
            } else {
                false
            };
            #[cfg(feature = "exec-strategy-sqpoll")]
            let read_fixed_enabled = fixed_buffers_enabled;
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            let read_fixed_enabled =
                fixed_buffers_enabled && matches!(recv_ring_mode, RecvRingMode::Latency);
            let subscribe_fixed_enabled =
                fixed_buffers_enabled && registration_layout.subscribe_fixed_base.is_some();
            let read_prefix_base = registration_layout.read_fixed_base;
            let read_prefix_slots = registration_layout.read_fixed_slot_table;
            let subscribe_suffix_base = if subscribe_fixed_enabled {
                registration_layout.subscribe_fixed_base
            } else {
                None
            };
            let subscribe_suffix_slots = if subscribe_fixed_enabled {
                registration_layout.subscribe_fixed_slot_table
            } else {
                Vec::new()
            };
            let (tx, rx) = ingress_channel::<Command>();
            let control = Arc::new(ShardControl::new());
            let sender = ShardSender {
                tx,
                wakeup_fd,
                control: control.clone(),
            };
            senders.push(sender);
            setups.push((
                shard,
                cpus[shard],
                sqpoll_accepted,
                #[cfg(not(feature = "exec-strategy-sqpoll"))]
                latency_sqpoll_accepted,
                ShardRuntimeConfig::from_env(
                    #[cfg(not(feature = "exec-strategy-sqpoll"))]
                    latency_sqpoll_accepted,
                    #[cfg(feature = "exec-strategy-sqpoll")]
                    sqpoll_accepted,
                ),
                latency_ring,
                #[cfg(not(feature = "exec-strategy-sqpoll"))]
                main_ring,
                rx,
                wakeup_fd,
                read_pool,
                read_prefix_base,
                read_prefix_slots,
                read_fixed_enabled,
                subscribe_fixed_enabled,
                subscribe_suffix_base,
                subscribe_suffix_slots,
                false,
                provided_recv_pool,
                arenas,
                control,
            ));
        }

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        for (
            shard,
            cpu,
            sqpoll_accepted,
            latency_sqpoll_accepted,
            shard_runtime_config,
            latency_ring,
            main_ring,
            rx,
            wakeup_fd,
            read_pool,
            read_prefix_base,
            read_prefix_slots,
            read_fixed_enabled,
            subscribe_fixed_enabled,
            subscribe_suffix_base,
            subscribe_suffix_slots,
            submit_pressure,
            provided_recv_pool,
            arenas,
            control,
        ) in setups
        {
            let launch_config = ShardLaunchConfig::new(
                cpu,
                latency_ring,
                main_ring,
                rx,
                wakeup_fd,
                read_pool,
                read_prefix_base,
                read_prefix_slots,
                read_fixed_enabled,
                subscribe_fixed_enabled,
                subscribe_suffix_base,
                subscribe_suffix_slots,
                submit_pressure,
                sqpoll_accepted,
                latency_sqpoll_accepted,
                shard_runtime_config,
                provided_recv_pool,
                arenas,
                control,
            );
            let join_handle =
                match spawn_shard_thread(shard, move || launch_config.into_state(shard)) {
                    Ok(handle) => handle,
                    Err(err) => {
                        let _ = shutdown_started_shards(&senders, join_handles);
                        return Err(err);
                    }
                };
            join_handles.push(join_handle);
        }

        #[cfg(feature = "exec-strategy-sqpoll")]
        for (
            shard,
            cpu,
            sqpoll_accepted,
            shard_runtime_config,
            latency_ring,
            rx,
            wakeup_fd,
            read_pool,
            read_prefix_base,
            read_prefix_slots,
            read_fixed_enabled,
            subscribe_fixed_enabled,
            subscribe_suffix_base,
            subscribe_suffix_slots,
            submit_pressure,
            provided_recv_pool,
            arenas,
            control,
        ) in setups
        {
            let launch_config = ShardLaunchConfig::new(
                cpu,
                latency_ring,
                rx,
                wakeup_fd,
                read_pool,
                read_prefix_base,
                read_prefix_slots,
                read_fixed_enabled,
                subscribe_fixed_enabled,
                subscribe_suffix_base,
                subscribe_suffix_slots,
                submit_pressure,
                sqpoll_accepted,
                shard_runtime_config,
                provided_recv_pool,
                arenas,
                control,
            );
            let join_handle =
                match spawn_shard_thread(shard, move || launch_config.into_state(shard)) {
                    Ok(handle) => handle,
                    Err(err) => {
                        let _ = shutdown_started_shards(&senders, join_handles);
                        return Err(err);
                    }
                };
            join_handles.push(join_handle);
        }

        Ok(Runtime {
            senders,
            next_listener_id: AtomicU64::new(1),
            stopping: AtomicBool::new(false),
            join_handles: Mutex::new(Some(join_handles)),
        })
    }

    fn stop(&self) -> Result<()> {
        if self.stopping.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let profile = stop_drain_profile_enabled();
        let stop_start_ns = if profile {
            Some(now_monotonic_ns())
        } else {
            None
        };
        if let Some(stop_start_ns) = stop_start_ns {
            eprintln!(
                "{} stop=profile start_ns={} senders={}",
                stats_labels::LOOP,
                stop_start_ns,
                self.senders.len()
            );
        }
        let mut first_err = None;
        let mut stop_request_failed = 0usize;
        for sender in &self.senders {
            if let Err(err) = sender.send(Command::Stop) {
                stop_request_failed = stop_request_failed.saturating_add(1);
                if first_err.is_none() {
                    first_err = Some(err);
                }
            }
        }
        if profile {
            eprintln!(
                "{} stop=profile commands_sent={} stop_request_failed={}",
                stats_labels::LOOP,
                self.senders.len().saturating_sub(stop_request_failed),
                stop_request_failed
            );
        }
        let mut join_handles = self
            .join_handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let handles = join_handles.take().unwrap_or_default();
        for (idx, handle) in handles.into_iter().enumerate() {
            let join_start_ns = now_monotonic_ns();
            if handle.join().is_err() && first_err.is_none() {
                first_err = Some(NifError::from_errno(libc::EIO));
            }
            if profile {
                eprintln!(
                    "{} stop=shard_join shard={} ns={}",
                    stats_labels::LOOP,
                    idx,
                    now_monotonic_ns().saturating_sub(join_start_ns)
                );
            }
        }
        if profile {
            let stop_wait_elapsed =
                now_monotonic_ns().saturating_sub(stop_start_ns.unwrap_or_default());
            eprintln!(
                "{} stop=profile elapsed_ns={} elapsed_ms={:.3}",
                stats_labels::LOOP,
                stop_wait_elapsed,
                stop_wait_elapsed as f64 / 1_000_000.0f64
            );
        }
        match first_err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    fn listen(&self, owner: LocalPid, port: u16, backlog: i32) -> Result<ResourceArc<SocketRef>> {
        let listener_id = self.next_listener_id.fetch_add(1, Ordering::Relaxed);
        let mut accept_rxs = Vec::with_capacity(self.senders.len());
        let backlog = if backlog > 0 {
            backlog
        } else {
            LISTEN_BACKLOG_DEFAULT
        };

        let first = make_listener(port, backlog)?;
        let first_raw_fd = first.as_raw_fd();
        let local = local_addr_of_fd(first.as_raw_fd())?;
        let actual_port = match local {
            NetAddr::V4(_, port) | NetAddr::V6(_, port) => port,
        };

        let first_fd = first.into_raw_fd();
        let (tx0, rx0) = mpsc::sync_channel(1);
        let (accept_tx0, accept_rx0) = unbounded::<ResourceArc<SocketRef>>();
        accept_rxs.push(accept_rx0);
        let install_first = self.senders[0].send(Command::InstallListener {
            listener_id,
            fd: first_fd,
            accept_tx: accept_tx0,
            owner,
            reply: tx0,
        });
        if let Err(err) = install_first {
            let _ = self.close_listener(listener_id);
            // SAFETY: ownership of this fd was never transferred on send failure.
            let _ = unsafe { libc::close(first_raw_fd) };
            return Err(err);
        }
        if let Err(err) = rx0.recv().map_err(|_| NifError::Closed)? {
            let _ = self.close_listener(listener_id);
            // SAFETY: shard install failed, so this process still owns the fd.
            let _ = unsafe { libc::close(first_raw_fd) };
            return Err(err);
        }

        for shard in 1..self.senders.len() {
            let sock = match make_listener(actual_port, backlog) {
                Ok(sock) => sock,
                Err(err) => {
                    let _ = self.close_listener(listener_id);
                    return Err(err);
                }
            };
            let fd = sock.into_raw_fd();
            let (tx, rx) = mpsc::sync_channel(1);
            let (accept_tx, accept_rx) = unbounded::<ResourceArc<SocketRef>>();
            accept_rxs.push(accept_rx);
            let install = self.senders[shard].send(Command::InstallListener {
                listener_id,
                fd,
                accept_tx,
                owner,
                reply: tx,
            });
            if let Err(err) = install {
                // SAFETY: ownership of `fd` was not transferred when send failed.
                let _ = unsafe { libc::close(fd) };
                let _ = self.close_listener(listener_id);
                return Err(err);
            }
            if let Err(err) = rx.recv().map_err(|_| NifError::Closed)? {
                let _ = self.close_listener(listener_id);
                // SAFETY: shard rejected install, so this process still owns `fd`.
                let _ = unsafe { libc::close(fd) };
                return Err(err);
            }
        }

        Ok(ResourceArc::new(SocketRef {
            kind: SocketKind::Listener(ListenerState {
                listener_id,
                accept_rxs,
                local,
                closed: AtomicBool::new(false),
            }),
        }))
    }

    fn close_listener(&self, listener_id: u64) -> Result<()> {
        for shard in 0..self.senders.len() {
            let (tx, rx) = mpsc::sync_channel(1);
            send_to_shard_linked(
                shard,
                listener_link_id(listener_id),
                Command::CloseListener {
                    listener_id,
                    reply: tx,
                },
            )?;
            let _ = rx.recv_timeout(Duration::from_secs(1));
        }
        Ok(())
    }

    fn debug_runtime_stats(&self) -> Result<Vec<RuntimeStatsSnapshot>> {
        let mut out = Vec::with_capacity(self.senders.len());
        for shard in 0..self.senders.len() {
            let (tx, rx) = mpsc::sync_channel(1);
            send_to_shard_linked(shard, global_link_id(), Command::GetStats { reply: tx })?;
            out.push(
                rx.recv_timeout(Duration::from_secs(1))
                    .map_err(|_| NifError::Closed)?,
            );
        }
        Ok(out)
    }
}

#[path = "runtime_command.rs"]
mod runtime_command;
pub(crate) use runtime_command::{
    Command, CommandLane, CqeRef, DeferredSubmitCommand, LatencyItem, ListenerShard, ReadyOp,
};

// Re-export ShardRuntimeConfig from runtime_config for use within runtime.
pub(crate) use runtime_config::ShardRuntimeConfig;

pub(crate) use super::runtime_stop_state::{ShardStopState, StopDrainSnapshot};

#[repr(align(64))]
pub(super) struct ShardState {
    pub(super) shard: usize,
    pub(super) cpu: usize,
    pub(super) latency_ring: IoUring,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) main_ring: IoUring,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) main_submit_batch: usize,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) main_submit_max_delay_ns: u64,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) main_last_submit_ns: u64,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) recv_ring_mode: RecvRingMode,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) next_cqe_main_first: bool,
    pub(super) rx: ShardReceiver<Command>,
    pub(super) read_pool: LockedReadBufPool,
    pub(super) read_fixed_base: u16,
    pub(super) read_fixed_slots: Vec<Option<u16>>,
    pub(super) read_fixed_enabled: bool,
    pub(super) subscribe_fixed_enabled: bool,
    pub(super) subscribe_fixed_base: Option<u16>,
    pub(super) subscribe_fixed_slots: Vec<Option<u16>>,
    pub(super) submit_pressure: bool,
    pub(super) sqpoll_accepted: bool,
    pub(super) provided_recv_pool: Option<ProvidedRecvPool>,
    pub(super) arenas: RuntimeArenas,
    pub(super) wakeup_fd: RawFd,
    pub(super) control: Arc<ShardControl>,
    pub(super) wakeup_buf: [u8; 8],
    pub(super) latency_ops: OpTable,
    pub(super) main_ops: OpTable,
    pub(super) listeners: ListenerTable,
    pub(super) conns: ConnectionTable,
    pub(super) subscriptions: SubscriptionTable,
    pub(super) pending_statx: U64Map<PendingStatx>,
    pub(super) local: SelectedScheduler<ReadyOp, u64>,
    deferred_submit_buf: Vec<DeferredSubmitCommand>,
    deferred_exec_buf: Vec<Command>,
    deferred_command_buf: VecDeque<Command>,
    pub(super) deferred_close_fds: VecDeque<RawFd>,
    pub(super) pending_soft_link_sqe: bool,
    result_reduce: ResultReduceState,
    result_reduce_cqes: usize,
    result_batches: Vec<ResultBatchSlot>,
    result_batch_nonempty_slots: usize,
    result_direct_send: bool,
    callback_local_fifo: VecDeque<ReadyOp>,
    latency_queue: [Option<LatencyItem>; LATENCY_QUEUE_CAP],
    latency_queue_len: usize,
    stop_state: ShardStopState,
    stop_drain_snapshot: Option<StopDrainSnapshot>,
    stop_drain_tick: u64,
    debug_loop_queues: bool,
    debug_loop_every: u64,
    debug_loop_tick: u64,
    #[cfg(debug_assertions)]
    debug_phase_stats: DebugPhaseStats,
    last_progress_ns: u64,
    idle_wait_ns: u64,
    sqpoll_idle_batch: usize,
    sqpoll_idle_usec: u32,
    pub(super) next_session_id: u64,
    pub(super) stats: ShardStats,
}

// L3: Close helpers extracted from runtime_shard.rs
#[path = "runtime_shard_close.rs"]
mod runtime_shard_close;

// L4: Recv decision helpers extracted from runtime_shard.rs
#[path = "runtime_shard_recv.rs"]
mod runtime_shard_recv;

#[path = "runtime_shard.rs"]
mod runtime_shard;

impl ShardState {
    #[inline]
    fn try_arm_recv_inline_or_fallback(&mut self, session_id: u64) {
        let (should_try_arm, mailbox_passive_on_submit) = self
            .conns
            .get(&session_id)
            .map(|conn| {
                let live_mailbox_passive = match &conn.handle.kind {
                    SocketKind::Session(session) => session.mailbox_passive.load(Ordering::Acquire),
                    _ => false,
                };
                (
                    conn.rx_bytes < RX_QUEUE_MAX_BYTES
                        && !conn.read_in_flight
                        && !conn.read_poll_armed,
                    live_mailbox_passive,
                )
            })
            .unwrap_or((false, false));

        if should_try_arm {
            self.set_recv_mailbox_snapshot(session_id, mailbox_passive_on_submit);
            if self.arm_conn_recv(session_id).is_err() {
                self.clear_recv_mailbox_snapshot(session_id);
                self.local.push_ready(ReadyOp::Conn(session_id));
            }
        }
    }

    fn observe_recv_ingress_wait(&mut self, enqueue_raw: u64) {
        self.stats
            .record_recv_ingress_wait(recv_wait_ns_since(enqueue_raw));
    }

    fn observe_recv_sync_ingress_wait(&mut self, enqueue_raw: u64) {
        self.stats
            .record_recv_sync_ingress_wait(recv_wait_ns_since(enqueue_raw));
    }

    fn observe_send_sync_ingress_wait(&mut self, enqueue_raw: u64) {
        self.stats
            .record_send_sync_ingress_wait(recv_wait_ns_since(enqueue_raw));
    }

    fn observe_pending_recv_completed(&mut self, pending: &PendingRecv, is_ok: bool) {
        let waited_ns = recv_wait_ns_since(pending.enqueued_raw);
        self.stats.record_recv_wait_completed(waited_ns, is_ok);
    }

    fn observe_pending_recv_canceled(&mut self, pending: &PendingRecv, timed_out: bool) {
        let waited_ns = recv_wait_ns_since(pending.enqueued_raw);
        self.stats.record_recv_wait_canceled(waited_ns, timed_out);
    }

    fn observe_result_send_wait(&mut self, enqueued_raw: u64) {
        self.stats
            .record_result_send_wait(recv_wait_ns_since(enqueued_raw));
    }

    pub(super) fn reply_pending_recv(&mut self, pending: PendingRecv, result: Result<Vec<u8>>) {
        self.observe_pending_recv_completed(&pending, result.is_ok());
        match pending.reply {
            PendingRecvReply::Async { request_id, target } => match result {
                Ok(data) => self.queue_reply_data(target, request_id, data),
                Err(err) => self.queue_reply_error(target, request_id, err),
            },
            PendingRecvReply::Sync(reply) => {
                let _ = reply.send(result);
            }
        }
    }

    pub(super) fn queue_reply_ok(&mut self, target: ResultTarget, request_id: u64) {
        self.queue_reply(target, request_id, AsyncReplyPayload::Ok);
    }

    pub(super) fn queue_reply_u64(&mut self, target: ResultTarget, request_id: u64, value: u64) {
        self.queue_reply(target, request_id, AsyncReplyPayload::U64(value));
    }

    pub(super) fn queue_reply_data(
        &mut self,
        target: ResultTarget,
        request_id: u64,
        data: Vec<u8>,
    ) {
        self.queue_reply(target, request_id, AsyncReplyPayload::Data(data));
    }

    pub(super) fn queue_reply_data_subscribe_slot(
        &mut self,
        target: ResultTarget,
        request_id: u64,
        page_slot: ArenaHandle,
        len: usize,
    ) {
        let payload = {
            let data = self.arenas.slice(page_slot.class, page_slot.slot, 0, len);
            let Some(mut bin) = OwnedBinary::new(data.len()) else {
                self.queue_reply_error(target, request_id, NifError::from_errno(libc::ENOMEM));
                return;
            };
            bin.as_mut_slice().copy_from_slice(data);
            AsyncReplyPayload::DataOwned(bin)
        };
        self.queue_reply(target, request_id, payload);
    }

    pub(super) fn queue_reply_session(
        &mut self,
        target: ResultTarget,
        request_id: u64,
        session: ResourceArc<SocketRef>,
    ) {
        self.queue_reply(target, request_id, AsyncReplyPayload::Session(session));
    }

    pub(super) fn queue_reply_error(
        &mut self,
        target: ResultTarget,
        request_id: u64,
        err: NifError,
    ) {
        self.queue_reply(target, request_id, AsyncReplyPayload::Error(err));
    }

    fn queue_reply(&mut self, target: ResultTarget, request_id: u64, payload: AsyncReplyPayload) {
        let enqueued_raw = recv_clock_raw();
        stats_inc!(self.stats, result_events_enqueued);

        if self.result_direct_send_ready() {
            // Hot-path optimization for request/response flows with no batching backlog:
            // send directly and skip reduce->send task scheduling.
            self.observe_result_send_wait(enqueued_raw);
            stats_inc!(self.stats, send_batches_sent);
            stats_inc!(self.stats, send_items_sent);
            if !send_result_single_to(&target, request_id, payload) {
                stats_inc!(self.stats, send_failures);
            }
            return;
        }

        self.result_reduce.pending.push_back(ResultEvent {
            target,
            request_id,
            payload,
            enqueued_raw,
        });
        if self.result_reduce.first_result_ns.is_none() {
            self.result_reduce.first_result_ns = Some(now_monotonic_ns());
        }
    }

    pub(super) fn result_direct_send_ready(&self) -> bool {
        let facet = ResultFacet::new(
            self.result_direct_send,
            self.result_reduce.pending.len(),
            self.result_reduce.reducer_scheduled,
            self.result_batch_nonempty_slots,
        );
        facet.direct_send_ready()
    }

    fn has_callback_work(&self) -> bool {
        !self.callback_local_fifo.is_empty()
    }

    fn pop_callback_local(&mut self) -> Option<ReadyOp> {
        self.callback_local_fifo.pop_front()
    }

    fn enqueue_callback_local(&mut self, op: ReadyOp) {
        if self.latency_queue_len < LATENCY_QUEUE_CAP {
            self.latency_queue[self.latency_queue_len] = Some(LatencyItem::Task(op));
            self.latency_queue_len += 1;
            return;
        }
        self.callback_local_fifo.push_back(op);
        stats_max!(
            self.stats,
            callback_fifo_peak,
            self.callback_local_fifo.len()
        );
    }

    fn enqueue_result_callback_outcome(&mut self, outcome: ResultCallbackOutcome) -> bool {
        match outcome {
            ResultCallbackOutcome::None => false,
            ResultCallbackOutcome::ReduceResults => {
                self.enqueue_callback_local(ReadyOp::ReduceResults);
                true
            }
            ResultCallbackOutcome::SendResults(slot) => {
                self.enqueue_callback_local(ReadyOp::SendResults(slot));
                true
            }
        }
    }

    fn maybe_schedule_reduce_results(
        &mut self,
        trigger: ResultReduceTrigger,
    ) -> ResultCallbackOutcome {
        let facet = ResultFacet::new(
            self.result_direct_send,
            self.result_reduce.pending.len(),
            self.result_reduce.reducer_scheduled,
            self.result_batch_nonempty_slots,
        );
        let outcome = facet.schedule_outcome(
            trigger,
            self.result_reduce_cqes,
            self.result_reduce.first_result_ns,
            now_monotonic_ns(),
        );
        if !bool::from(outcome) {
            return ResultCallbackOutcome::None;
        }
        self.result_reduce.reducer_scheduled = true;
        match trigger {
            ResultReduceTrigger::Cqe => stats_inc!(self.stats, result_reduce_runs_size),
            ResultReduceTrigger::Age => stats_inc!(self.stats, result_reduce_runs_age),
            ResultReduceTrigger::Idle => stats_inc!(self.stats, result_reduce_runs_idle),
        }
        ResultCallbackOutcome::ReduceResults
    }

    fn drive_reduce_results(&mut self) {
        self.result_reduce.reducer_scheduled = false;
        if self.result_reduce.pending.is_empty() {
            self.result_reduce.first_result_ns = None;
            return;
        }
        while let Some(event) = self.result_reduce.pending.pop_front() {
            stats_inc!(self.stats, result_events_reduced);
            let slot = self.find_or_create_result_batch_slot(event.target);
            let was_empty = self.result_batches[slot].entries.is_empty();
            self.result_batches[slot].entries.push_back((
                event.request_id,
                event.payload,
                event.enqueued_raw,
            ));
            if was_empty {
                self.result_batch_nonempty_slots =
                    self.result_batch_nonempty_slots.saturating_add(1);
            }
            if self.result_batches[slot].entries.len() >= RESULT_SEND_BATCH_LIMIT {
                let outcome = self.enqueue_send_result_slot(slot);
                self.enqueue_result_callback_outcome(outcome);
            }
        }
        self.result_reduce.first_result_ns = None;
        self.result_reduce_cqes = 0;

        for slot in 0..self.result_batches.len() {
            if self.result_batches[slot].queue_ready() {
                let outcome = self.enqueue_send_result_slot(slot);
                self.enqueue_result_callback_outcome(outcome);
            }
        }
    }

    fn drive_send_results(&mut self, slot: usize) {
        let (target, mut drained, has_more) = {
            let Some(batch_slot) = self.result_batches.get_mut(slot) else {
                return;
            };
            batch_slot.send_queued = false;
            if batch_slot.entries.is_empty() {
                return;
            }
            let take = batch_slot.entries.len().min(RESULT_SEND_BATCH_LIMIT);
            let mut drained = Vec::with_capacity(take);
            for _ in 0..take {
                if let Some(item) = batch_slot.entries.pop_front() {
                    drained.push(item);
                }
            }
            (
                batch_slot.target.clone(),
                drained,
                !batch_slot.entries.is_empty(),
            )
        };

        if drained.is_empty() {
            return;
        }

        stats_inc!(self.stats, send_batches_sent);
        if drained.len() == 1 {
            let Some((request_id, payload, enqueued_raw)) = drained.pop() else {
                return;
            };
            self.observe_result_send_wait(enqueued_raw);
            stats_inc!(self.stats, send_items_sent);
            if !send_result_single_to(&target, request_id, payload) {
                stats_inc!(self.stats, send_failures);
            }
        } else {
            let mut encoded = Vec::with_capacity(drained.len());
            for (request_id, payload, enqueued_raw) in drained {
                self.observe_result_send_wait(enqueued_raw);
                encoded.push((request_id, payload));
            }
            stats_add!(self.stats, send_items_sent, encoded.len() as u64);
            if !send_result_batch_to(&target, encoded) {
                stats_inc!(self.stats, send_failures);
            }
        }

        if has_more {
            let outcome = self.enqueue_send_result_slot(slot);
            self.enqueue_result_callback_outcome(outcome);
        } else {
            self.result_batch_nonempty_slots = self.result_batch_nonempty_slots.saturating_sub(1);
        }
    }

    fn find_or_create_result_batch_slot(&mut self, target: ResultTarget) -> usize {
        if let Some(slot) = self
            .result_batches
            .iter()
            .position(|entry| entry.target == target)
        {
            return slot;
        }
        self.result_batches.push(ResultBatchSlot::new(target));
        self.result_batches.len() - 1
    }

    fn enqueue_send_result_slot(&mut self, slot: usize) -> ResultCallbackOutcome {
        let Some(batch_slot) = self.result_batches.get_mut(slot) else {
            return ResultCallbackOutcome::None;
        };
        if batch_slot.send_queued || batch_slot.entries.is_empty() {
            return ResultCallbackOutcome::None;
        }
        batch_slot.send_queued = true;
        stats_inc!(self.stats, send_tasks_enqueued);
        ResultCallbackOutcome::SendResults(slot)
    }

    #[cfg(debug_assertions)]
    fn maybe_log_phase_stats(&mut self, mode: &'static str) {
        let pending_cmds = self.control.pending_commands.load(Ordering::Acquire);
        let ready_depth = self.local.ready_depth();
        let write_ready_depth = self.local.write_ready_depth();
        let queued_sqes = self.queued_sqes();
        let latency_inflight = self.latency_ops.len();
        let main_inflight = self.main_ops.len();
        self.debug_phase_stats.maybe_log(
            self.shard,
            mode,
            self.sqpoll_accepted,
            pending_cmds,
            ready_depth,
            write_ready_depth,
            queued_sqes,
            latency_inflight,
            main_inflight,
        );
    }

    #[cfg(not(debug_assertions))]
    fn maybe_log_phase_stats(&mut self, _mode: &'static str) {}

    fn maybe_log_loop_queues(&mut self, mode: &'static str) {
        if !self.debug_loop_queues {
            return;
        }
        self.debug_loop_tick = self.debug_loop_tick.wrapping_add(1);
        if !self.debug_loop_tick.is_multiple_of(self.debug_loop_every) {
            return;
        }
        let tick = self.debug_loop_tick;
        let pending_cmds = self.control.pending_commands.load(Ordering::Acquire);
        let ready_depth = self.local.ready_depth();
        let write_ready_depth = self.local.write_ready_depth();
        let callback_fifo = self.callback_local_fifo.len();
        let latency_queue = self.latency_queue_len;
        let result_pending = self.result_reduce.pending.len();
        let queued_sqes = self.queued_sqes();
        let latency_inflight = self.latency_ops.len();
        let main_inflight = self.main_ops.len();
        let conns = self.conns.len();
        let subs = self.subscriptions.len();
        let result_batch_slots = self.result_batches.len();
        let result_batch_nonempty = self
            .result_batches
            .iter()
            .filter(|slot| !slot.entries.is_empty())
            .count();
        let result_batch_items: usize = self
            .result_batches
            .iter()
            .map(|slot| slot.entries.len())
            .sum();
        eprintln!(
            "{} shard={} mode={} sqpoll={} tick={} pending_cmds={} ready={} write_ready={} callback_fifo={} latency_queue={} result_pending={} result_slots={} result_slots_nonempty={} result_items={} queued_sqes={} latency_inflight={} main_inflight={} conns={} subs={}",
            stats_labels::LOOP,
            self.shard,
            mode,
            self.sqpoll_accepted,
            tick,
            pending_cmds,
            ready_depth,
            write_ready_depth,
            callback_fifo,
            latency_queue,
            result_pending,
            result_batch_slots,
            result_batch_nonempty,
            result_batch_items,
            queued_sqes,
            latency_inflight,
            main_inflight,
            conns,
            subs
        );
    }
}

pub(super) fn reply_pending_batch(mut batch: PendingBatch, result: Result<()>) {
    if let Err(err) = result {
        if batch.results.is_empty() {
            batch.results.push((0, BatchResult::Error(err.into())));
        } else {
            for (_, item) in &mut batch.results {
                if matches!(item, BatchResult::Ok) {
                    *item = BatchResult::Error(err.clone().into());
                }
            }
        }
    }
    send_batch_reply_to(&batch.target, batch.request_id, batch.results);
}

pub(super) fn advance_active_mode(
    handle: &ResourceArc<SocketRef>,
    owner: &LocalPid,
    mode: &mut ActiveMode,
) {
    match mode {
        ActiveMode::False | ActiveMode::True => {}
        ActiveMode::Once => {
            *mode = ActiveMode::False;
            if let SocketKind::Session(session) = &handle.kind {
                let mut opts = session
                    .opts
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                opts.active = ActiveMode::False;
            }
        }
        ActiveMode::N(n) => {
            if *n <= 1 {
                *mode = ActiveMode::False;
                if let SocketKind::Session(session) = &handle.kind {
                    let mut opts = session
                        .opts
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    opts.active = ActiveMode::False;
                }
                send_active_passive_message(handle, owner);
            } else {
                *n -= 1;
                if let SocketKind::Session(session) = &handle.kind {
                    let mut opts = session
                        .opts
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    opts.active = ActiveMode::N(*n);
                }
            }
        }
    }
}

#[cfg(test)]
mod runtime_lane_tests {
    use super::runtime_config::ShardRuntimeConfig;
    use super::{
        build_registration_layout, dispatch_runtime_startup_profile_layout, ingress_try_recv,
        plan_fixed_registration, shard_from_routed_id, shutdown_started_shards, Command,
        RuntimeProfileDispatchSeam, ShardControl, ShardSender,
    };
    use crate::runtime_arena::RuntimeArenas;
    use crate::runtime_ingress::ingress_channel;
    use crate::runtime_reactor::{LockedReadBufPool, BUF_SIZE, SUBSCRIBE_PAGE_SIZE};
    use dataplane_runtime::runtime_profiles::{layout_for_profile, ProfileKind, TopologyProfile};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Lane {
        Submit,
        Exec,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Stream {
        A,
        B,
        Bg,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Op {
        lane: Lane,
        stream: Stream,
        seq: u32,
        linked: bool,
    }

    fn simulate<const SUBMIT_CAP: usize, const EXEC_CAP: usize>(input: &[Op]) -> Vec<Op> {
        let mut submit = Vec::new();
        let mut exec = Vec::new();
        let mut out = Vec::with_capacity(input.len());

        let flush = |submit: &mut Vec<Op>, exec: &mut Vec<Op>, out: &mut Vec<Op>| {
            if !submit.is_empty() {
                out.append(submit);
            }
            if !exec.is_empty() {
                out.append(exec);
            }
        };

        for op in input.iter().copied() {
            if op.linked {
                flush(&mut submit, &mut exec, &mut out);
                out.push(op);
                continue;
            }
            match op.lane {
                Lane::Submit => {
                    if submit.len() == SUBMIT_CAP {
                        flush(&mut submit, &mut exec, &mut out);
                    }
                    submit.push(op);
                }
                Lane::Exec => {
                    if exec.len() == EXEC_CAP {
                        flush(&mut submit, &mut exec, &mut out);
                    }
                    exec.push(op);
                }
            }
        }

        flush(&mut submit, &mut exec, &mut out);
        out
    }

    fn assert_stream_is_ordered(out: &[Op], stream: Stream) {
        let mut expect = 1u32;
        for op in out {
            if op.stream == stream {
                assert_eq!(op.seq, expect, "stream {:?} out-of-order", stream);
                expect += 1;
            }
        }
    }

    fn linked(stream: Stream, seq: u32, lane: Lane) -> Op {
        Op {
            lane,
            stream,
            seq,
            linked: true,
        }
    }

    fn bg(seq: u32, lane: Lane) -> Op {
        Op {
            lane,
            stream: Stream::Bg,
            seq,
            linked: false,
        }
    }

    #[test]
    fn interleaving_two_linked_streams_preserves_per_stream_order() {
        let order = simulate::<16, 8>(&[
            linked(Stream::A, 1, Lane::Submit),
            linked(Stream::B, 1, Lane::Exec),
            linked(Stream::A, 2, Lane::Exec),
            linked(Stream::B, 2, Lane::Submit),
            linked(Stream::A, 3, Lane::Submit),
            linked(Stream::B, 3, Lane::Exec),
        ]);
        assert_stream_is_ordered(&order, Stream::A);
        assert_stream_is_ordered(&order, Stream::B);
        assert_eq!(order.len(), 6);
    }

    #[test]
    fn interleaving_two_linked_streams_with_background_work_stays_ordered() {
        let order = simulate::<2, 2>(&[
            linked(Stream::A, 1, Lane::Submit),
            bg(1, Lane::Exec),
            bg(2, Lane::Submit),
            linked(Stream::B, 1, Lane::Exec),
            bg(3, Lane::Submit),
            linked(Stream::A, 2, Lane::Exec),
            bg(4, Lane::Exec),
            linked(Stream::B, 2, Lane::Submit),
            linked(Stream::A, 3, Lane::Submit),
            linked(Stream::B, 3, Lane::Exec),
        ]);
        assert_stream_is_ordered(&order, Stream::A);
        assert_stream_is_ordered(&order, Stream::B);
    }

    #[test]
    fn linked_forces_submit_then_exec_flush_before_immediate_op() {
        let out = simulate::<2, 2>(&[
            bg(1, Lane::Exec),
            bg(2, Lane::Submit),
            linked(Stream::A, 1, Lane::Submit),
        ]);
        assert_eq!(
            out,
            vec![
                bg(2, Lane::Submit),
                bg(1, Lane::Exec),
                linked(Stream::A, 1, Lane::Submit)
            ]
        );
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum LatencySim {
        Task(u8),
        Command(u8, Option<u64>),
    }

    fn drain_latency_sim<const CAP: usize>(items: [Option<LatencySim>; CAP]) -> Vec<LatencySim> {
        let mut out = Vec::new();
        for item in items.iter().flatten() {
            if let LatencySim::Command(id, cqe_ref) = item {
                out.push(LatencySim::Command(*id, *cqe_ref));
            }
        }
        for item in items.iter().flatten() {
            if let LatencySim::Task(id) = item {
                out.push(LatencySim::Task(*id));
            }
        }
        out
    }

    #[test]
    fn latency_queue_drains_commands_before_tasks() {
        let items = [
            Some(LatencySim::Task(1)),
            Some(LatencySim::Command(10, Some(123))),
            Some(LatencySim::Task(2)),
            Some(LatencySim::Command(11, None)),
        ];
        let out = drain_latency_sim(items);
        assert_eq!(
            out,
            vec![
                LatencySim::Command(10, Some(123)),
                LatencySim::Command(11, None),
                LatencySim::Task(1),
                LatencySim::Task(2),
            ]
        );
    }

    #[test]
    fn routed_request_ids_use_nonzero_namespace() {
        let shard0_id = (1u64 << 56) | 7;
        let shard1_id = (2u64 << 56) | 9;
        assert_eq!(shard_from_routed_id(shard0_id, 4).unwrap(), 0);
        assert_eq!(shard_from_routed_id(shard1_id, 4).unwrap(), 1);
        assert!(shard_from_routed_id(7, 4).is_err());
    }

    fn assert_iovec_fields_match(actual: &[libc::iovec], expected: &[libc::iovec]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected.iter()) {
            assert_eq!(actual.iov_base, expected.iov_base);
            assert_eq!(actual.iov_len, expected.iov_len);
        }
    }

    #[test]
    fn registration_layout_uses_read_prefix_then_subscribe_suffix() {
        let read_pool = LockedReadBufPool::new(2).unwrap();
        let arenas = RuntimeArenas::new(1, 2);

        let read_iovecs = read_pool.registered_iovecs_stable();
        let subscribe_iovecs = arenas.small_registered_iovecs();
        let layout = build_registration_layout(&read_pool, &arenas, true);

        assert_eq!(layout.read_fixed_slots.len(), read_iovecs.len());
        assert_eq!(layout.read_fixed_slots[0].index, 0);
        assert_eq!(layout.read_fixed_slots[1].index, 1);
        assert_eq!(layout.read_fixed_base, 0);
        assert_eq!(layout.read_fixed_len, read_iovecs.len());
        assert_eq!(layout.read_fixed_slot_table, vec![Some(0), Some(1)]);
        assert_eq!(layout.subscribe_fixed_base, Some(read_iovecs.len() as u16));
        assert_eq!(layout.subscribe_fixed_len, subscribe_iovecs.len());
        assert_eq!(layout.subscribe_fixed_slot_table, vec![Some(0), Some(1)]);
        assert_eq!(
            layout.descriptors.len(),
            read_iovecs.len() + subscribe_iovecs.len()
        );
        assert_iovec_fields_match(&layout.descriptors[..read_iovecs.len()], &read_iovecs);
        assert_iovec_fields_match(&layout.descriptors[read_iovecs.len()..], &subscribe_iovecs);
    }

    #[test]
    fn registration_layout_skips_subscribe_suffix_when_disabled() {
        let read_pool = LockedReadBufPool::new(2).unwrap();
        let arenas = RuntimeArenas::new(1, 2);

        let read_iovecs = read_pool.registered_iovecs_stable();
        let layout = build_registration_layout(&read_pool, &arenas, false);

        assert_eq!(layout.read_fixed_slots.len(), read_iovecs.len());
        assert_eq!(layout.read_fixed_base, 0);
        assert_eq!(layout.read_fixed_len, read_iovecs.len());
        assert_eq!(layout.read_fixed_slot_table, vec![Some(0), Some(1)]);
        assert_eq!(layout.subscribe_fixed_base, None);
        assert_eq!(layout.subscribe_fixed_len, 0);
        assert!(layout.subscribe_fixed_slot_table.is_empty());
        assert_eq!(layout.descriptors.len(), read_iovecs.len());
        assert_iovec_fields_match(&layout.descriptors, &read_iovecs);
    }

    #[test]
    fn startup_profile_layout_dispatch_routes_embedded_typed_closure() {
        let (layout, seam) =
            dispatch_runtime_startup_profile_layout(TopologyProfile::embedded_reference())
                .expect("embedded startup layout dispatch");

        assert_eq!(layout.profile().profile_kind, ProfileKind::Embedded);
        assert_eq!(seam, RuntimeProfileDispatchSeam::Embedded);
    }

    #[test]
    fn startup_profile_layout_dispatch_matches_canonical_layout_for_all_profiles() {
        let profiles = [
            (
                TopologyProfile::balanced_dual_shard(),
                RuntimeProfileDispatchSeam::Balanced,
            ),
            (
                TopologyProfile::embedded_reference(),
                RuntimeProfileDispatchSeam::Embedded,
            ),
            (
                TopologyProfile::performance_dual_shard(),
                RuntimeProfileDispatchSeam::Performance,
            ),
        ];

        for (profile, expected_seam) in profiles {
            let expected_layout =
                layout_for_profile(profile.clone()).expect("canonical startup profile layout");
            let (dispatched_layout, seam) = dispatch_runtime_startup_profile_layout(profile)
                .expect("dispatched startup layout");

            assert_eq!(seam, expected_seam);
            assert_eq!(dispatched_layout, expected_layout);
        }
    }

    #[test]
    fn fixed_registration_plan_drops_subscribe_suffix_when_memlock_is_too_small() {
        let plan = plan_fixed_registration(1, 1, BUF_SIZE, true, 1);

        assert_eq!(plan.locked_read_bufs, 1);
        assert!(!plan.register_subscribe_arena);
    }

    #[test]
    fn fixed_registration_plan_disables_all_fixed_registration_when_nothing_fits() {
        let plan = plan_fixed_registration(8, 32, 8 * 1024, true, 256);

        assert_eq!(plan.locked_read_bufs, 0);
        assert!(!plan.register_subscribe_arena);
    }

    #[test]
    fn fixed_registration_plan_keeps_subscribe_suffix_when_budget_covers_full_layout() {
        let memlock_limit = BUF_SIZE + SUBSCRIBE_PAGE_SIZE;
        let plan = plan_fixed_registration(1, 1, memlock_limit, true, 1);

        assert_eq!(plan.locked_read_bufs, 1);
        assert!(plan.register_subscribe_arena);
    }

    #[test]
    fn startup_shutdowns_started_shards_when_a_later_spawn_fails() {
        let (tx0, rx0) = ingress_channel::<Command>();
        // SAFETY: eventfd syscall on a freshly allocated fd with CLOEXEC+NONBLOCK.
        // Called from test code only; fd is never aliased or exposed outside this fn.
        let wakeup_fd0 = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        assert!(wakeup_fd0 >= 0);
        let sender0 = ShardSender {
            tx: tx0,
            wakeup_fd: wakeup_fd0,
            control: Arc::new(ShardControl::new()),
        };
        let stopped0 = Arc::new(AtomicBool::new(false));
        let thread_stopped0 = stopped0.clone();
        let join_handle0 = std::thread::spawn(move || loop {
            if let Some(batch) = ingress_try_recv(&rx0) {
                if batch.iter().any(|cmd| matches!(cmd, Command::Stop)) {
                    thread_stopped0.store(true, Ordering::Release);
                    break;
                }
            } else {
                std::thread::yield_now();
            }
        });

        let (tx1, rx1) = ingress_channel::<Command>();
        // SAFETY: same reasoning as wakeup_fd0 above.
        let wakeup_fd1 = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        assert!(wakeup_fd1 >= 0);
        let sender1 = ShardSender {
            tx: tx1,
            wakeup_fd: wakeup_fd1,
            control: Arc::new(ShardControl::new()),
        };
        let stopped1 = Arc::new(AtomicBool::new(false));
        let thread_stopped1 = stopped1.clone();
        let join_handle1 = std::thread::spawn(move || loop {
            if let Some(batch) = ingress_try_recv(&rx1) {
                if batch.iter().any(|cmd| matches!(cmd, Command::Stop)) {
                    thread_stopped1.store(true, Ordering::Release);
                    break;
                }
            } else {
                std::thread::yield_now();
            }
        });

        let result = shutdown_started_shards(&[sender0, sender1], vec![join_handle0, join_handle1]);

        assert!(result.is_ok());
        assert!(stopped0.load(Ordering::Acquire));
        assert!(stopped1.load(Ordering::Acquire));
        // SAFETY: fd was opened in this test, now closed. No aliasing remains.
        let _ = unsafe { libc::close(wakeup_fd0) };
        // SAFETY: fd was opened in this test, now closed. No aliasing remains.
        let _ = unsafe { libc::close(wakeup_fd1) };
    }

    #[test]
    fn shard_launch_config_into_state_keeps_stats_snapshot_wiring() {
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
        let control = Arc::new(ShardControl::new());
        let read_pool = LockedReadBufPool::new(3).expect("read pool");
        let arenas = RuntimeArenas::new(2, 4);
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let runtime_config = ShardRuntimeConfig::test_new(3, 99, true, true, 17, 23, 5, 11);
        #[cfg(feature = "exec-strategy-sqpoll")]
        let runtime_config = ShardRuntimeConfig::test_new(true, true, 17, 23, 5, 11);

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let launch = super::ShardLaunchConfig::new(
            7,
            latency_ring,
            main_ring,
            rx,
            11,
            read_pool,
            17,
            vec![Some(1), None, Some(3)],
            true,
            true,
            Some(23),
            vec![None, Some(5)],
            true,
            false,
            true,
            runtime_config,
            None,
            arenas,
            control.clone(),
        );

        #[cfg(feature = "exec-strategy-sqpoll")]
        let launch = super::ShardLaunchConfig::new(
            7,
            latency_ring,
            rx,
            11,
            read_pool,
            17,
            vec![Some(1), None, Some(3)],
            true,
            true,
            Some(23),
            vec![None, Some(5)],
            true,
            false,
            runtime_config,
            None,
            arenas,
            control.clone(),
        );

        let mut shard = launch.into_state(9);
        drop(tx);
        shard.stats.commands_drained = 7;
        shard.stats.ready_driven = 2;
        shard.stats.write_ready_enqueued = 3;

        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        shard.handle_command(Command::GetStats { reply: reply_tx }, false);
        let snapshot = reply_rx.recv().expect("receive stats snapshot");

        assert_eq!(snapshot.shard, 9);
        assert_eq!(snapshot.commands_drained, 7);
        assert_eq!(snapshot.ready_driven, 2);
        assert_eq!(snapshot.write_ready_enqueued, 3);
        assert_eq!(snapshot.conn_count, 0);
        assert_eq!(snapshot.ready_depth, 0);
        assert_eq!(snapshot.write_ready_depth, 0);
    }

    #[test]
    fn shard_launch_config_into_state_preserves_construction_fields() {
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
        let control = Arc::new(ShardControl::new());
        let read_pool = LockedReadBufPool::new(3).expect("read pool");
        let arenas = RuntimeArenas::new(2, 4);
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let runtime_config = ShardRuntimeConfig::test_new(3, 99, true, true, 17, 23, 5, 11);
        #[cfg(feature = "exec-strategy-sqpoll")]
        let runtime_config = ShardRuntimeConfig::test_new(true, true, 17, 23, 5, 11);

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let launch = super::ShardLaunchConfig::new(
            7,
            latency_ring,
            main_ring,
            rx,
            11,
            read_pool,
            17,
            vec![Some(1), None, Some(3)],
            true,
            true,
            Some(23),
            vec![None, Some(5)],
            true,
            false,
            true,
            runtime_config,
            None,
            arenas,
            control.clone(),
        );

        #[cfg(feature = "exec-strategy-sqpoll")]
        let launch = super::ShardLaunchConfig::new(
            7,
            latency_ring,
            rx,
            11,
            read_pool,
            17,
            vec![Some(1), None, Some(3)],
            true,
            true,
            Some(23),
            vec![None, Some(5)],
            true,
            false,
            runtime_config,
            None,
            arenas,
            control.clone(),
        );

        let shard = launch.into_state(9);
        drop(tx);

        assert_eq!(shard.shard, 9);
        assert_eq!(shard.cpu, 7);
        assert_eq!(shard.wakeup_fd, 11);
        assert_eq!(shard.read_fixed_base, 17);
        assert_eq!(shard.read_fixed_slots, vec![Some(1), None, Some(3)]);
        assert!(shard.read_fixed_enabled);
        assert!(shard.subscribe_fixed_enabled);
        assert_eq!(shard.subscribe_fixed_base, Some(23));
        assert_eq!(shard.subscribe_fixed_slots, vec![None, Some(5)]);
        assert!(shard.submit_pressure);
        assert!(!shard.sqpoll_accepted);
        assert!(shard.provided_recv_pool.is_none());
        assert_eq!(shard.arenas.small_len(), 4);
        assert_eq!(
            Arc::strong_count(&shard.control),
            Arc::strong_count(&control)
        );
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        {
            assert_eq!(shard.main_submit_batch, runtime_config.main_submit_batch());
            assert_eq!(
                shard.main_submit_max_delay_ns,
                runtime_config.main_submit_max_delay_ns()
            );
            assert_eq!(shard.recv_ring_mode, runtime_config.recv_ring_mode());
        }
        assert_eq!(
            shard.result_direct_send,
            runtime_config.result_direct_send()
        );
        assert_eq!(shard.debug_loop_queues, runtime_config.debug_loop_queues());
        assert_eq!(shard.debug_loop_every, runtime_config.debug_loop_every());
        assert_eq!(shard.idle_wait_ns, runtime_config.idle_wait_ns());
        assert_eq!(shard.sqpoll_idle_batch, runtime_config.sqpoll_idle_batch());
        assert_eq!(shard.sqpoll_idle_usec, runtime_config.sqpoll_idle_usec());
    }

    #[test]
    fn shard_launch_seam_pre_post_proof_preserves_launch_snapshot_and_shard_state() {
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
        let control = Arc::new(ShardControl::new());
        let read_pool = LockedReadBufPool::new(2).expect("read pool");
        let arenas = RuntimeArenas::new(1, 3);
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let runtime_config = ShardRuntimeConfig::test_new(4, 101, false, true, 19, 29, 7, 13);
        #[cfg(feature = "exec-strategy-sqpoll")]
        let runtime_config = ShardRuntimeConfig::test_new(false, true, 19, 29, 7, 13);

        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let launch = super::ShardLaunchConfig::new(
            5,
            latency_ring,
            main_ring,
            rx,
            17,
            read_pool,
            23,
            vec![Some(2), Some(4)],
            true,
            false,
            None,
            vec![Some(6)],
            false,
            true,
            false,
            runtime_config,
            None,
            arenas,
            control.clone(),
        );

        #[cfg(feature = "exec-strategy-sqpoll")]
        let launch = super::ShardLaunchConfig::new(
            5,
            latency_ring,
            rx,
            17,
            read_pool,
            23,
            vec![Some(2), Some(4)],
            true,
            false,
            None,
            vec![Some(6)],
            false,
            true,
            runtime_config,
            None,
            arenas,
            control.clone(),
        );

        let expected_cpu = 5usize;
        let expected_wakeup_fd = 17i32;
        let expected_read_fixed_slots = vec![Some(2), Some(4)];
        let expected_read_fixed_enabled = true;
        let expected_subscribe_fixed_enabled = false;
        let expected_subscribe_fixed_base = None;
        let expected_subscribe_fixed_slots = vec![Some(6)];
        let expected_submit_pressure = false;
        let expected_sqpoll_accepted = true;
        let expected_provided_recv_pool_is_none = true;
        let expected_result_direct_send = runtime_config.result_direct_send();
        let expected_debug_loop_queues = runtime_config.debug_loop_queues();
        let expected_debug_loop_every = runtime_config.debug_loop_every();
        let expected_idle_wait_ns = runtime_config.idle_wait_ns();
        let expected_sqpoll_idle_batch = runtime_config.sqpoll_idle_batch();
        let expected_sqpoll_idle_usec = runtime_config.sqpoll_idle_usec();
        let expected_control_refs = Arc::strong_count(&control);

        assert_eq!(expected_control_refs, Arc::strong_count(&control));

        let shard = launch.into_state(11);
        drop(tx);

        assert_eq!(shard.cpu, expected_cpu);
        assert_eq!(shard.wakeup_fd, expected_wakeup_fd);
        assert_eq!(shard.read_fixed_slots, expected_read_fixed_slots);
        assert_eq!(shard.read_fixed_enabled, expected_read_fixed_enabled);
        assert_eq!(
            shard.subscribe_fixed_enabled,
            expected_subscribe_fixed_enabled
        );
        assert_eq!(shard.subscribe_fixed_base, expected_subscribe_fixed_base);
        assert_eq!(shard.subscribe_fixed_slots, expected_subscribe_fixed_slots);
        assert_eq!(shard.submit_pressure, expected_submit_pressure);
        assert_eq!(shard.sqpoll_accepted, expected_sqpoll_accepted);
        assert_eq!(
            shard.provided_recv_pool.is_none(),
            expected_provided_recv_pool_is_none
        );
        assert_eq!(shard.result_direct_send, expected_result_direct_send);
        assert_eq!(shard.debug_loop_queues, expected_debug_loop_queues);
        assert_eq!(shard.debug_loop_every, expected_debug_loop_every);
        assert_eq!(shard.idle_wait_ns, expected_idle_wait_ns);
        assert_eq!(shard.sqpoll_idle_batch, expected_sqpoll_idle_batch);
        assert_eq!(shard.sqpoll_idle_usec, expected_sqpoll_idle_usec);
        assert_eq!(shard.shard, 11);
        assert_eq!(Arc::strong_count(&shard.control), expected_control_refs);
    }
}
