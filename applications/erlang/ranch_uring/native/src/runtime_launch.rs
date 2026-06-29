//! Shard launch configuration — extracted from runtime.rs
//!
//! Contains the non-SQPOLL and SQPOLL variants of `ShardLaunchConfig` along
//! with their constructors and `into_state` converters, plus the thread-spawn
//! helpers that were previously in runtime.rs.

use std::os::fd::RawFd;
use std::sync::Arc;

use crate::runtime::ShardRuntimeConfig;
use crate::runtime::ShardState;
use crate::runtime_arena::RuntimeArenas;
use crate::runtime_boundary::ShardControl;
use crate::runtime_reactor::LockedReadBufPool;
use io_uring::IoUring;

// ---------------------------------------------------------------------------
// Re-export Command so ShardReceiver can name it
// ---------------------------------------------------------------------------
pub(crate) use crate::runtime::Command;

// ---------------------------------------------------------------------------
// Thread spawn helpers (extracted from runtime.rs)
// ---------------------------------------------------------------------------

/// Runs the shard loop on the current thread.
pub(crate) fn run_shard_state(mut shard_state: ShardState) {
    shard_state.run();
}

/// Runs the shard loop on the current thread (current-thread-shard-driver variant).
#[cfg(feature = "current-thread-shard-driver")]
#[allow(dead_code)]
pub(crate) fn run_shard_state_on_current_thread(shard_state: ShardState) {
    run_shard_state(shard_state);
}

/// Drive shard on current thread by constructing state and running it.
#[cfg(feature = "current-thread-shard-driver")]
#[allow(dead_code)]
pub(crate) fn drive_shard_on_current_thread<F>(make_state: F)
where
    F: FnOnce() -> ShardState,
{
    run_shard_state_on_current_thread(make_state());
}

/// Spawns a new OS thread for a shard and returns its join handle.
pub(crate) fn spawn_shard_thread<F>(
    shard: usize,
    make_state: F,
) -> crate::errors::Result<std::thread::JoinHandle<()>>
where
    F: FnOnce() -> ShardState + Send + 'static,
{
    let thread_name = shard_thread_name(shard);
    std::thread::Builder::new()
        .name(thread_name.clone())
        .spawn(move || run_shard_state(make_state()))
        .map_err(|err| {
            let errno = err.raw_os_error().unwrap_or(libc::EIO);
            eprintln!(
                "ranch_uring shard spawn failed shard={} thread={} errno={} kind={:?}",
                shard,
                thread_name,
                errno,
                err.kind()
            );
            crate::errors::NifError::from_errno(errno)
        })
}

// ---------------------------------------------------------------------------
// Non-SQPOLL variant
// ---------------------------------------------------------------------------

#[cfg(not(feature = "exec-strategy-sqpoll"))]
pub(super) struct ShardLaunchConfig {
    pub(super) cpu: usize,
    pub(super) latency_ring: IoUring,
    pub(super) main_ring: IoUring,
    pub(super) rx: crate::runtime_boundary::ShardReceiver<Command>,
    pub(super) wakeup_fd: RawFd,
    pub(super) read_pool: LockedReadBufPool,
    pub(super) read_prefix_base: u16,
    pub(super) read_prefix_slots: Vec<Option<u16>>,
    pub(super) read_fixed_enabled: bool,
    pub(super) subscribe_fixed_enabled: bool,
    pub(super) subscribe_suffix_base: Option<u16>,
    pub(super) subscribe_suffix_slots: Vec<Option<u16>>,
    pub(super) submit_pressure: bool,
    pub(super) sqpoll_accepted: bool,
    pub(super) latency_sqpoll_accepted: bool,
    pub(super) runtime_config: ShardRuntimeConfig,
    pub(super) provided_recv_pool: Option<crate::runtime_reactor::ProvidedRecvPool>,
    pub(super) arenas: RuntimeArenas,
    pub(super) control: Arc<ShardControl>,
}

#[cfg(not(feature = "exec-strategy-sqpoll"))]
#[allow(clippy::too_many_arguments)]
impl ShardLaunchConfig {
    pub(super) fn new(
        cpu: usize,
        latency_ring: IoUring,
        main_ring: IoUring,
        rx: crate::runtime_boundary::ShardReceiver<Command>,
        wakeup_fd: RawFd,
        read_pool: LockedReadBufPool,
        read_prefix_base: u16,
        read_prefix_slots: Vec<Option<u16>>,
        read_fixed_enabled: bool,
        subscribe_fixed_enabled: bool,
        subscribe_suffix_base: Option<u16>,
        subscribe_suffix_slots: Vec<Option<u16>>,
        submit_pressure: bool,
        sqpoll_accepted: bool,
        latency_sqpoll_accepted: bool,
        runtime_config: ShardRuntimeConfig,
        provided_recv_pool: Option<crate::runtime_reactor::ProvidedRecvPool>,
        arenas: RuntimeArenas,
        control: Arc<ShardControl>,
    ) -> Self {
        Self {
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
            runtime_config,
            provided_recv_pool,
            arenas,
            control,
        }
    }

    pub(super) fn into_state(self, shard: usize) -> ShardState {
        ShardState::new(
            shard,
            self.cpu,
            self.latency_ring,
            self.main_ring,
            self.rx,
            self.wakeup_fd,
            self.read_pool,
            self.read_prefix_base,
            self.read_prefix_slots,
            self.read_fixed_enabled,
            self.subscribe_fixed_enabled,
            self.subscribe_suffix_base,
            self.subscribe_suffix_slots,
            self.submit_pressure,
            self.sqpoll_accepted,
            self.latency_sqpoll_accepted,
            self.runtime_config,
            self.provided_recv_pool,
            self.arenas,
            self.control,
        )
    }
}

// ---------------------------------------------------------------------------
// SQPOLL variant
// ---------------------------------------------------------------------------

#[cfg(feature = "exec-strategy-sqpoll")]
pub(super) struct ShardLaunchConfig {
    pub(super) cpu: usize,
    pub(super) latency_ring: IoUring,
    pub(super) rx: crate::runtime_boundary::ShardReceiver<Command>,
    pub(super) wakeup_fd: RawFd,
    pub(super) read_pool: LockedReadBufPool,
    pub(super) read_prefix_base: u16,
    pub(super) read_prefix_slots: Vec<Option<u16>>,
    pub(super) read_fixed_enabled: bool,
    pub(super) subscribe_fixed_enabled: bool,
    pub(super) subscribe_suffix_base: Option<u16>,
    pub(super) subscribe_suffix_slots: Vec<Option<u16>>,
    pub(super) submit_pressure: bool,
    pub(super) sqpoll_accepted: bool,
    pub(super) runtime_config: ShardRuntimeConfig,
    pub(super) provided_recv_pool: Option<crate::runtime_reactor::ProvidedRecvPool>,
    pub(super) arenas: RuntimeArenas,
    pub(super) control: Arc<ShardControl>,
}

#[cfg(feature = "exec-strategy-sqpoll")]
#[allow(clippy::too_many_arguments)]
impl ShardLaunchConfig {
    pub(super) fn new(
        cpu: usize,
        latency_ring: IoUring,
        rx: crate::runtime_boundary::ShardReceiver<Command>,
        wakeup_fd: RawFd,
        read_pool: LockedReadBufPool,
        read_prefix_base: u16,
        read_prefix_slots: Vec<Option<u16>>,
        read_fixed_enabled: bool,
        subscribe_fixed_enabled: bool,
        subscribe_suffix_base: Option<u16>,
        subscribe_suffix_slots: Vec<Option<u16>>,
        submit_pressure: bool,
        sqpoll_accepted: bool,
        runtime_config: ShardRuntimeConfig,
        provided_recv_pool: Option<crate::runtime_reactor::ProvidedRecvPool>,
        arenas: RuntimeArenas,
        control: Arc<ShardControl>,
    ) -> Self {
        Self {
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
            runtime_config,
            provided_recv_pool,
            arenas,
            control,
        }
    }

    pub(super) fn into_state(self, shard: usize) -> ShardState {
        ShardState::new(
            shard,
            self.cpu,
            self.latency_ring,
            self.rx,
            self.wakeup_fd,
            self.read_pool,
            self.read_prefix_base,
            self.read_prefix_slots,
            self.read_fixed_enabled,
            self.subscribe_fixed_enabled,
            self.subscribe_suffix_base,
            self.subscribe_suffix_slots,
            self.submit_pressure,
            self.sqpoll_accepted,
            self.runtime_config,
            self.provided_recv_pool,
            self.arenas,
            self.control,
        )
    }
}

// ---------------------------------------------------------------------------
// Thread naming and shutdown helpers
// ---------------------------------------------------------------------------

/// Returns the OS thread name for a shard.
pub(crate) fn shard_thread_name(shard: usize) -> String {
    format!("ranch_uring_rt_{shard}")
}

/// Shuts down all started shards by sending stop and joining threads.
pub(crate) fn shutdown_started_shards(
    senders: &[crate::runtime_boundary::ShardSender<crate::runtime::Command>],
    join_handles: Vec<std::thread::JoinHandle<()>>,
) -> crate::errors::Result<()> {
    let mut first_err = None;
    for sender in senders {
        if let Err(err) = sender.send(crate::runtime::Command::Stop) {
            if first_err.is_none() {
                first_err = Some(err);
            }
        }
    }
    for handle in join_handles {
        if handle.join().is_err() && first_err.is_none() {
            first_err = Some(crate::errors::NifError::from_errno(libc::EIO));
        }
    }
    match first_err {
        Some(err) => Err(err),
        None => Ok(()),
    }
}
