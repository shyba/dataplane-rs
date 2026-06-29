use io_uring::{opcode, squeue};
use slab::Slab;
use socket2::{Domain, Protocol, SockRef, Socket, Type};
use std::os::fd::{BorrowedFd, RawFd};

use crate::errors::{NifError, Result};
use crate::runtime_topology::{ProfileKind, TopologyProfile};
use crate::socket::NetAddr;

pub(super) const DEFAULT_SHARDS_MAX: usize = 256;
pub(super) const DEFAULT_RING_SIZE: u32 = 4096;
pub(super) const BUF_SIZE: usize = 64 * 1024;
pub(super) const SUBSCRIBE_PAGE_SIZE: usize = 4096;
pub(super) const DEFAULT_SUBSCRIBE_PAGES_PER_SHARD: usize = 256;
pub(super) const DEFAULT_LOCKED_READ_BUFS_PER_SHARD: usize = 8;
pub(super) const DEFAULT_PROVIDED_RECV_BUFS_PER_SHARD: usize = 128;
pub(super) const LISTEN_BACKLOG_DEFAULT: i32 = 1024;
pub(super) const CHANNEL_CAPACITY: usize = 4096;
pub(super) const CONN_READ_POLL_MASK: u32 =
    (libc::POLLIN | libc::POLLERR | libc::POLLHUP | libc::POLLRDHUP) as u32;
pub(super) const CONN_WRITE_POLL_MASK: u32 =
    (libc::POLLOUT | libc::POLLERR | libc::POLLHUP | libc::POLLRDHUP) as u32;
const OP_SLOT_GENERATION_MIN: u32 = 1;
const LOCKED_READ_BUF_MIN_GENERATION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SqpollMode {
    Off,
    Try,
    Require,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SqpollCpu {
    None,
    Shard,
    Fixed(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SqpollConfig {
    pub(super) mode: SqpollMode,
    pub(super) cpu: SqpollCpu,
    pub(super) idle_ms: u32,
}

pub(super) fn configured_shard_count() -> usize {
    std::env::var("RANCH_URING_SHARDS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .map(|n| n.clamp(1, DEFAULT_SHARDS_MAX))
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|p| p.get().clamp(1, DEFAULT_SHARDS_MAX))
                .unwrap_or(1)
        })
}

pub(super) fn configured_topology_profile(shards: usize) -> TopologyProfile {
    let profile_kind = match std::env::var("RANCH_URING_PROFILE")
        .ok()
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("embedded") | Some("esp32") => ProfileKind::Embedded,
        Some("performance") | Some("perf") | Some("server") => ProfileKind::Performance,
        _ => ProfileKind::Balanced,
    };

    let mut profile = match profile_kind {
        ProfileKind::Embedded => TopologyProfile::embedded_dual_shard(),
        ProfileKind::Balanced => TopologyProfile::balanced_dual_shard(),
        ProfileKind::Performance => TopologyProfile::performance_dual_shard(),
    };
    let _ = shards;
    profile.cpu_allowlist = configured_cpu_allowlist();
    profile
}

fn configured_cpu_allowlist() -> Option<Vec<usize>> {
    let value = std::env::var("RANCH_URING_CPU_ALLOWLIST").ok()?;
    let mut cpus = value
        .split(',')
        .filter_map(|part| part.trim().parse::<usize>().ok())
        .collect::<Vec<_>>();
    cpus.sort_unstable();
    cpus.dedup();
    if cpus.is_empty() {
        None
    } else {
        Some(cpus)
    }
}

pub(super) fn configured_ring_size() -> u32 {
    std::env::var("RANCH_URING_RING_DEPTH")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(DEFAULT_RING_SIZE)
}

pub(super) fn configured_locked_read_bufs_per_shard() -> usize {
    std::env::var("RANCH_URING_LOCKED_READ_BUFS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_LOCKED_READ_BUFS_PER_SHARD)
}

pub(super) fn configured_provided_recv_bufs_per_shard() -> usize {
    std::env::var("RANCH_URING_PROVIDED_RECV_BUFS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_PROVIDED_RECV_BUFS_PER_SHARD)
}

pub(super) fn configured_subscribe_pages_per_shard() -> usize {
    std::env::var("RANCH_URING_SUBSCRIBE_PAGES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_SUBSCRIBE_PAGES_PER_SHARD)
}

pub(super) fn configured_chunk_arena_slots() -> Option<usize> {
    std::env::var("RANCH_URING_CHUNK_ARENA_SLOTS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
}

pub(super) fn configured_subscribe_arena_slots() -> Option<usize> {
    std::env::var("RANCH_URING_SUBSCRIBE_ARENA_SLOTS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
}

pub(super) fn configured_sqpoll_config() -> SqpollConfig {
    let mode_env = std::env::var("RANCH_URING_SQPOLL")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase());
    let mode = match mode_env.as_deref() {
        Some("try") => SqpollMode::Try,
        Some("require") => SqpollMode::Require,
        _ => SqpollMode::Off,
    };
    let cpu_env = std::env::var("RANCH_URING_SQPOLL_CPU")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase());
    let cpu = match cpu_env.as_deref() {
        Some("shard") => SqpollCpu::Shard,
        Some(value) => match value.parse::<usize>() {
            Ok(cpu) => SqpollCpu::Fixed(cpu),
            Err(_) => SqpollCpu::None,
        },
        None => SqpollCpu::None,
    };
    let idle_ms = std::env::var("RANCH_URING_SQPOLL_IDLE_MS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(2000);
    SqpollConfig { mode, cpu, idle_ms }
}

pub(super) fn configured_register_subscribe_arena() -> bool {
    match std::env::var("RANCH_URING_REGISTER_SUBSCRIBE_ARENA")
        .ok()
        .as_deref()
        .map(|s| s.trim())
    {
        Some("0") => false,
        Some("1") | None => true,
        _ => true,
    }
}

pub(super) fn memlock_limit_bytes() -> Result<usize> {
    let mut lim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `lim` is a valid, writable `rlimit` pointer for the duration of the call.
    let rc = unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut lim) };
    if rc != 0 {
        return Err(NifError::last_os_error());
    }
    if lim.rlim_cur == libc::RLIM_INFINITY {
        Ok(usize::MAX)
    } else {
        Ok(lim.rlim_cur as usize)
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BufPoolKind {
    RegisteredRead,
    ProvidedRecv,
}

#[allow(dead_code)]
impl BufPoolKind {
    #[inline]
    pub(super) const fn tag(self) -> u8 {
        match self {
            Self::RegisteredRead => 1,
            Self::ProvidedRecv => 2,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BufSlotId {
    pub(super) pool: BufPoolKind,
    pub(super) index: u16,
    pub(super) generation: u32,
}

#[allow(dead_code)]
impl BufSlotId {
    #[inline]
    pub(super) const fn new(pool: BufPoolKind, index: u16, generation: u32) -> Self {
        Self {
            pool,
            index,
            generation,
        }
    }

    #[inline]
    pub(super) const fn is_valid(self) -> bool {
        self.generation != 0
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BufSlotState {
    Free,
    Leased,
    InFlight,
    Retired,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PoolLifetimeState {
    Active,
    Draining,
    Removed,
}

pub(super) struct LockedReadBufPool {
    slots: Vec<LockedReadBufSlot>,
    free: Vec<u16>,
    slot_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LockedReadReleaseError {
    InvalidSlotIdentity,
}

struct LockedReadBufSlot {
    buf: Box<[u8]>,
    generation: u32,
    leased: bool,
}

#[derive(Clone, Copy, Debug)]
struct RegisteredReadSlotDescriptor {
    slot_id: BufSlotId,
    iov: libc::iovec,
}

impl LockedReadBufPool {
    pub(super) fn new(slots: usize) -> Result<Self> {
        let mut storage = Vec::with_capacity(slots);
        let mut free = Vec::with_capacity(slots);
        for idx in 0..slots {
            let buf = vec![0u8; BUF_SIZE].into_boxed_slice();
            // SAFETY: `buf` points to stable boxed storage and `buf.len()` is the exact mapped
            // length; this call only pins those bytes in memory.
            let rc = unsafe { libc::mlock(buf.as_ptr() as *const libc::c_void, buf.len()) };
            if rc != 0 {
                return Err(NifError::last_os_error());
            }
            storage.push(LockedReadBufSlot {
                buf,
                generation: LOCKED_READ_BUF_MIN_GENERATION,
                leased: false,
            });
            free.push((slots - idx - 1) as u16);
        }
        Ok(Self {
            slots: storage,
            free,
            slot_bytes: BUF_SIZE,
        })
    }

    pub(super) fn acquire_fixed_slot(&mut self) -> Option<BufSlotId> {
        let index = self.free.pop()?;
        let slot = self.slots.get_mut(index as usize)?;
        if slot.leased {
            return None;
        }
        slot.leased = true;
        Some(BufSlotId::new(
            BufPoolKind::RegisteredRead,
            index,
            slot.generation,
        ))
    }

    pub(super) fn release_fixed_slot(
        &mut self,
        slot_id: BufSlotId,
    ) -> std::result::Result<(), LockedReadReleaseError> {
        let Some(slot) = self.fixed_slot_mut(slot_id) else {
            return Err(LockedReadReleaseError::InvalidSlotIdentity);
        };
        slot.leased = false;
        slot.generation = next_generation(slot.generation);
        self.free.push(slot_id.index);
        Ok(())
    }

    pub(super) fn fallback_buf(&self) -> Vec<u8> {
        vec![0u8; self.slot_bytes]
    }

    pub(super) fn fixed_slot_ptr(&mut self, slot_id: BufSlotId) -> Option<*mut u8> {
        Some(self.fixed_slot_mut(slot_id)?.buf.as_mut_ptr())
    }

    pub(super) fn fixed_slot_slice(&self, slot_id: BufSlotId) -> Option<&[u8]> {
        Some(&self.fixed_slot(slot_id)?.buf[..])
    }

    pub(super) fn fixed_slot_len(&self, slot_id: BufSlotId) -> Option<usize> {
        Some(self.fixed_slot(slot_id)?.buf.len())
    }

    #[inline]
    pub(super) fn registered_read_index(&self, slot_id: BufSlotId) -> Option<u16> {
        let _ = self.fixed_slot(slot_id)?;
        Some(slot_id.index)
    }

    fn fixed_slot(&self, slot_id: BufSlotId) -> Option<&LockedReadBufSlot> {
        if slot_id.pool != BufPoolKind::RegisteredRead {
            return None;
        }
        let slot = self.slots.get(slot_id.index as usize)?;
        if slot.generation != slot_id.generation || !slot.leased {
            return None;
        }
        Some(slot)
    }

    fn fixed_slot_mut(&mut self, slot_id: BufSlotId) -> Option<&mut LockedReadBufSlot> {
        if slot_id.pool != BufPoolKind::RegisteredRead {
            return None;
        }
        let slot = self.slots.get_mut(slot_id.index as usize)?;
        if slot.generation != slot_id.generation || !slot.leased {
            return None;
        }
        Some(slot)
    }

    fn registered_read_slot_descriptors(&self) -> Vec<RegisteredReadSlotDescriptor> {
        let mut entries = self
            .slots
            .iter()
            .enumerate()
            .map(|(index, slot)| RegisteredReadSlotDescriptor {
                slot_id: BufSlotId::new(BufPoolKind::RegisteredRead, index as u16, slot.generation),
                iov: libc::iovec {
                    iov_base: slot.buf.as_ptr() as *mut libc::c_void,
                    iov_len: slot.buf.len(),
                },
            })
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(|descriptor| descriptor.slot_id.index);
        entries
    }

    pub(super) fn registered_iovecs_stable_with_ids(&self) -> Vec<(BufSlotId, libc::iovec)> {
        self.registered_read_slot_descriptors()
            .into_iter()
            .map(|descriptor| (descriptor.slot_id, descriptor.iov))
            .collect()
    }

    #[cfg(test)]
    pub(super) fn registered_iovecs_stable(&self) -> Vec<libc::iovec> {
        self.registered_read_slot_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.iov)
            .collect()
    }
}

impl Drop for LockedReadBufPool {
    fn drop(&mut self) {
        for slot in &self.slots {
            // SAFETY: every slot owns its boxed storage and this drop path runs once, so each
            // `munlock` uses a valid pointer/length pair that previously succeeded in `mlock`.
            unsafe {
                libc::munlock(slot.buf.as_ptr() as *const libc::c_void, slot.buf.len());
            }
        }
    }
}

pub(super) struct ProvidedRecvPool {
    pub(super) bgid: u16,
    lifetime: PoolLifetimeState,
    remove_queued: bool,
    remove_completed: bool,
    slots: Vec<ProvidedRecvBufSlot>,
}

struct ProvidedRecvBufSlot {
    buf: Box<[u8]>,
    state: BufSlotState,
    generation: u32,
}

impl ProvidedRecvPool {
    pub(super) fn new(bgid: u16, count: usize) -> Self {
        let mut slots = Vec::with_capacity(count);
        for _ in 0..count {
            slots.push(ProvidedRecvBufSlot {
                buf: vec![0u8; BUF_SIZE].into_boxed_slice(),
                state: BufSlotState::Free,
                generation: OP_SLOT_GENERATION_MIN,
            });
        }
        Self {
            bgid,
            lifetime: PoolLifetimeState::Active,
            remove_queued: false,
            remove_completed: false,
            slots,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn slot_id_for_bid(&self, bid: u16) -> Option<BufSlotId> {
        self.slots
            .get(bid as usize)
            .map(|_| BufSlotId::new(BufPoolKind::ProvidedRecv, bid, self.slot_generation(bid)))
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn bid_is_valid(&self, bid: u16) -> bool {
        self.slots.get(bid as usize).is_some()
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn slot_state_for_bid(&self, bid: u16) -> Option<BufSlotState> {
        self.slots.get(bid as usize).map(|slot| slot.state)
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn slot_is_reusable(&self, bid: u16) -> bool {
        matches!(self.slot_state_for_bid(bid), Some(BufSlotState::Free))
    }

    pub(super) fn is_active(&self) -> bool {
        self.lifetime == PoolLifetimeState::Active
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn is_draining(&self) -> bool {
        self.lifetime == PoolLifetimeState::Draining
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn is_removed(&self) -> bool {
        self.lifetime == PoolLifetimeState::Removed
    }

    pub(super) fn begin_draining(&mut self) {
        if self.lifetime == PoolLifetimeState::Active {
            self.lifetime = PoolLifetimeState::Draining;
            for slot in &mut self.slots {
                if matches!(slot.state, BufSlotState::Free) {
                    slot.state = BufSlotState::Retired;
                }
            }
        }
    }

    pub(super) fn has_inflight_entries(&self) -> bool {
        self.slots
            .iter()
            .any(|slot| matches!(slot.state, BufSlotState::InFlight))
    }

    #[inline]
    pub(super) fn can_retire(&self) -> bool {
        self.lifetime == PoolLifetimeState::Draining && !self.has_inflight_entries()
    }

    pub(super) fn advance_retirement(&mut self) -> bool {
        if self.can_retire() {
            self.lifetime = PoolLifetimeState::Removed;
            return true;
        }
        false
    }

    pub(super) fn should_queue_remove(&self) -> bool {
        self.lifetime == PoolLifetimeState::Removed && !self.remove_queued && !self.remove_completed
    }

    pub(super) fn mark_remove_queued(&mut self) -> bool {
        if !self.should_queue_remove() {
            return false;
        }
        self.remove_queued = true;
        true
    }

    pub(super) fn clear_remove_queued(&mut self) {
        if !self.remove_completed {
            self.remove_queued = false;
        }
    }

    pub(super) fn mark_remove_completed(&mut self) {
        self.remove_completed = true;
    }

    pub(super) fn is_teardown_complete(&self) -> bool {
        self.lifetime == PoolLifetimeState::Removed && self.remove_completed
    }

    pub(super) fn resolve_inflight_slot(&self, bid: u16) -> Option<BufSlotId> {
        let slot = self.slot(bid)?;
        if !matches!(slot.state, BufSlotState::InFlight) {
            return None;
        }
        Some(BufSlotId::new(
            BufPoolKind::ProvidedRecv,
            bid,
            slot.generation,
        ))
    }

    pub(super) fn complete_recv(&mut self, slot_id: BufSlotId) -> bool {
        let lifetime = self.lifetime;
        let Some(slot) = self.provided_slot_mut(slot_id) else {
            return false;
        };
        if !matches!(slot.state, BufSlotState::InFlight) {
            return false;
        }
        slot.state = match lifetime {
            PoolLifetimeState::Active => BufSlotState::Free,
            PoolLifetimeState::Draining | PoolLifetimeState::Removed => BufSlotState::Retired,
        };
        slot.generation = next_generation(slot.generation);
        true
    }

    pub(super) fn slice(&self, slot_id: BufSlotId, len: usize) -> Option<&[u8]> {
        self.provided_slot(slot_id).map(|slot| &slot.buf[..len])
    }

    #[cfg(test)]
    pub(super) fn seed_inflight_slot_bytes(&mut self, slot_id: BufSlotId, data: &[u8]) -> bool {
        let Some(slot) = self.provided_slot_mut(slot_id) else {
            return false;
        };
        if !matches!(slot.state, BufSlotState::InFlight) || data.len() > slot.buf.len() {
            return false;
        }
        slot.buf[..data.len()].copy_from_slice(data);
        true
    }

    #[cfg(test)]
    pub(super) fn provide_entry(&mut self, bid: u16) -> Option<squeue::Entry> {
        let slot_id = self.slot_id_for_bid(bid)?;
        self.provide_entry_for_slot(slot_id)
    }

    pub(super) fn provide_entry_for_slot(&mut self, slot_id: BufSlotId) -> Option<squeue::Entry> {
        if self.lifetime != PoolLifetimeState::Active {
            return None;
        }
        let bid = slot_id.index;
        let slot = self.provided_slot_mut(slot_id)?;
        if !matches!(slot.state, BufSlotState::Free) {
            return None;
        }
        slot.state = BufSlotState::InFlight;
        Some(
            opcode::ProvideBuffers::new(slot.buf.as_mut_ptr(), BUF_SIZE as i32, 1, self.bgid, bid)
                .build(),
        )
    }

    #[inline]
    fn slot(&self, bid: u16) -> Option<&ProvidedRecvBufSlot> {
        self.slots.get(bid as usize)
    }

    #[inline]
    fn slot_mut(&mut self, bid: u16) -> Option<&mut ProvidedRecvBufSlot> {
        self.slots.get_mut(bid as usize)
    }

    #[inline]
    fn provided_slot(&self, slot_id: BufSlotId) -> Option<&ProvidedRecvBufSlot> {
        if slot_id.pool != BufPoolKind::ProvidedRecv {
            return None;
        }
        let slot = self.slot(slot_id.index)?;
        if slot.generation != slot_id.generation {
            return None;
        }
        Some(slot)
    }

    #[inline]
    fn provided_slot_mut(&mut self, slot_id: BufSlotId) -> Option<&mut ProvidedRecvBufSlot> {
        if slot_id.pool != BufPoolKind::ProvidedRecv {
            return None;
        }
        let slot = self.slot_mut(slot_id.index)?;
        if slot.generation != slot_id.generation {
            return None;
        }
        Some(slot)
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    fn slot_generation(&self, bid: u16) -> u32 {
        self.slot(bid).map(|slot| slot.generation).unwrap_or(0)
    }
}

#[derive(Clone, Copy)]
pub(super) enum Op {
    ListenerAccept {
        listener_id: u64,
        armed_ns: u64,
    },
    ConnRecv(u64),
    CancelRecv(u64),
    CancelListenerAccept(u64),
    ConnWrite(u64),
    ConnReadPoll(u64),
    ConnWritePoll(u64),
    ProvidedRecvPoolRemove,
    CloseFd {
        fd: RawFd,
    },
    SubscribeRead {
        subscription_id: u64,
        slot_idx: u8,
    },
    SubscribeAccept {
        subscription_id: u64,
        slot_idx: u8,
        armed_ns: u64,
    },
    Statx(u64),
    Wakeup,
}

#[derive(Clone, Copy)]
pub(super) enum RingKind {
    Latency,
    Main,
}

struct OpSlot {
    generation: u32,
    op: Op,
}

#[repr(align(64))]
pub(super) struct OpTable {
    slots: Slab<OpSlot>,
    generations: Vec<u32>,
}

impl OpTable {
    pub(super) fn new() -> Self {
        Self {
            slots: Slab::new(),
            generations: Vec::new(),
        }
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    #[inline]
    pub(super) fn values(&self) -> impl Iterator<Item = &Op> {
        self.slots.iter().map(|(_, slot)| &slot.op)
    }

    pub(super) fn insert(&mut self, op: Op) -> u64 {
        let vacant = self.slots.vacant_entry();
        let idx = vacant.key();
        if idx >= self.generations.len() {
            self.generations.resize(idx + 1, 1);
        }
        let generation = non_zero_generation(self.generations[idx]);
        vacant.insert(OpSlot { generation, op });
        encode_token(idx, generation)
    }

    pub(super) fn remove(&mut self, token: u64) -> Option<Op> {
        let (idx, generation) = decode_token(token)?;
        self.generations.get(idx)?;
        let slot_generation = self.slots.get(idx)?.generation;
        if slot_generation != generation {
            return None;
        }
        let removed = self.slots.remove(idx);
        debug_assert_eq!(slot_generation, removed.generation);
        self.generations[idx] = next_generation(removed.generation);
        Some(removed.op)
    }

    #[cfg(test)]
    pub(super) fn remove_first_matching(
        &mut self,
        mut predicate: impl FnMut(&Op) -> bool,
    ) -> Option<Op> {
        let token = self.slots.iter().find_map(|(idx, slot)| {
            predicate(&slot.op).then_some(encode_token(idx, slot.generation))
        })?;
        self.remove(token)
    }
}

#[inline]
fn non_zero_generation(generation: u32) -> u32 {
    if generation == 0 {
        OP_SLOT_GENERATION_MIN
    } else {
        generation
    }
}

#[inline]
fn next_generation(generation: u32) -> u32 {
    generation
        .checked_add(1)
        .filter(|next| *next != 0)
        .unwrap_or(OP_SLOT_GENERATION_MIN)
}

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod slot_id_tests {
    use super::{
        decode_token, encode_token, BufPoolKind, BufSlotId, BufSlotState, LockedReadBufPool,
        LockedReadReleaseError, Op, OpTable, PoolLifetimeState, ProvidedRecvPool, BUF_SIZE,
    };

    #[test]
    fn buf_pool_kind_tags_are_stable() {
        assert_eq!(BufPoolKind::RegisteredRead.tag(), 1);
        assert_eq!(BufPoolKind::ProvidedRecv.tag(), 2);
    }

    #[test]
    fn buf_slot_id_treats_zero_generation_as_invalid() {
        let invalid = BufSlotId::new(BufPoolKind::RegisteredRead, 7, 0);
        let valid = BufSlotId::new(BufPoolKind::ProvidedRecv, 9, 1);
        assert!(!invalid.is_valid());
        assert!(valid.is_valid());
        assert_eq!(valid.index, 9);
    }

    #[test]
    fn slot_and_pool_states_are_explicit() {
        let slot_states = [
            BufSlotState::Free,
            BufSlotState::Leased,
            BufSlotState::InFlight,
            BufSlotState::Retired,
        ];
        let pool_states = [
            PoolLifetimeState::Active,
            PoolLifetimeState::Draining,
            PoolLifetimeState::Removed,
        ];
        assert_eq!(slot_states.len(), 4);
        assert_eq!(pool_states.len(), 3);
    }

    #[test]
    fn locked_read_pool_fixed_slots_roundtrip() {
        let mut pool = LockedReadBufPool::new(2).expect("locked read pool");
        let slot = pool.acquire_fixed_slot().expect("fixed slot");
        assert!(slot.is_valid());
        let ptr = pool.fixed_slot_ptr(slot).expect("slot ptr");
        let len = pool.fixed_slot_len(slot).expect("slot len");
        assert!(!ptr.is_null());
        assert_eq!(len, BUF_SIZE);
        pool.release_fixed_slot(slot).expect("release fixed slot");
        let reacquired = pool.acquire_fixed_slot().expect("reacquired slot");
        assert_eq!(reacquired.index, slot.index);
        let reacquired_ptr = pool.fixed_slot_ptr(reacquired).expect("reacquired ptr");
        assert_eq!(ptr, reacquired_ptr);
        assert_eq!(
            pool.fixed_slot_len(reacquired).expect("reacquired len"),
            BUF_SIZE
        );
    }

    #[test]
    fn registered_read_descriptors_stay_slot_ordered_after_reuse() {
        let mut pool = LockedReadBufPool::new(3).expect("locked read pool");
        let first = pool.acquire_fixed_slot().expect("first slot");
        let second = pool.acquire_fixed_slot().expect("second slot");
        let third = pool.acquire_fixed_slot().expect("third slot");

        pool.release_fixed_slot(second)
            .expect("release second slot");
        pool.release_fixed_slot(first).expect("release first slot");

        let descriptors = pool.registered_iovecs_stable_with_ids();
        let indices = descriptors
            .iter()
            .map(|(slot_id, _)| slot_id.index)
            .collect::<Vec<_>>();
        assert_eq!(indices, vec![0, 1, 2]);
        assert_eq!(descriptors[0].0.index, first.index);
        assert_eq!(descriptors[1].0.index, second.index);
        assert_eq!(descriptors[2].0.index, third.index);

        let pointer_order = descriptors
            .iter()
            .map(|(_, iov)| iov.iov_base)
            .collect::<Vec<_>>();
        assert_eq!(pointer_order.len(), 3);
    }

    #[test]
    fn registered_read_descriptor_pointers_stay_stable_across_slot_reuse() {
        let mut pool = LockedReadBufPool::new(3).expect("locked read pool");
        let before = pool
            .registered_iovecs_stable_with_ids()
            .into_iter()
            .map(|(slot_id, iov)| (slot_id.index, iov.iov_base, iov.iov_len))
            .collect::<Vec<_>>();

        let first = pool.acquire_fixed_slot().expect("first slot");
        let second = pool.acquire_fixed_slot().expect("second slot");
        pool.release_fixed_slot(first).expect("release first slot");
        pool.release_fixed_slot(second)
            .expect("release second slot");
        let _reacquired_first = pool.acquire_fixed_slot().expect("reacquired first");
        let _reacquired_second = pool.acquire_fixed_slot().expect("reacquired second");

        let after = pool
            .registered_iovecs_stable_with_ids()
            .into_iter()
            .map(|(slot_id, iov)| (slot_id.index, iov.iov_base, iov.iov_len))
            .collect::<Vec<_>>();

        assert_eq!(before, after);
    }

    #[test]
    fn provided_recv_pool_drains_and_stops_reprovide() {
        let mut pool = ProvidedRecvPool::new(7, 2);
        assert!(pool.is_active());
        assert!(pool.bid_is_valid(0));
        assert_eq!(
            pool.slot_id_for_bid(0),
            Some(BufSlotId::new(BufPoolKind::ProvidedRecv, 0, 1))
        );
        assert!(pool.slot_is_reusable(0));
        assert!(pool.provide_entry(0).is_some());
        assert_eq!(pool.slot_state_for_bid(0), Some(BufSlotState::InFlight));
        pool.begin_draining();
        assert!(pool.is_draining());
        assert_eq!(pool.slot_state_for_bid(1), Some(BufSlotState::Retired));
        let slot = pool.resolve_inflight_slot(0).expect("inflight slot");
        assert!(pool.complete_recv(slot));
        assert_eq!(pool.slot_state_for_bid(0), Some(BufSlotState::Retired));
        assert!(pool.provide_entry(0).is_none());
    }

    #[test]
    fn provided_recv_pool_retires_only_after_drain_and_completion() {
        let mut pool = ProvidedRecvPool::new(7, 1);
        assert!(pool.provide_entry(0).is_some());
        pool.begin_draining();
        assert!(!pool.can_retire());
        assert!(!pool.advance_retirement());
        let slot = pool.resolve_inflight_slot(0).expect("inflight slot");
        assert!(pool.complete_recv(slot));
        assert!(pool.can_retire());
        assert!(pool.advance_retirement());
        assert!(pool.is_removed());
        assert!(pool.provide_entry(0).is_none());
    }

    #[test]
    fn provided_recv_pool_remove_queue_is_one_way() {
        let mut pool = ProvidedRecvPool::new(7, 1);
        assert!(pool.provide_entry(0).is_some());
        pool.begin_draining();
        let slot = pool.resolve_inflight_slot(0).expect("inflight slot");
        assert!(pool.complete_recv(slot));
        assert!(pool.advance_retirement());
        assert!(pool.should_queue_remove());
        assert!(pool.mark_remove_queued());
        assert!(!pool.should_queue_remove());
        pool.clear_remove_queued();
        assert!(pool.should_queue_remove());
        assert!(pool.mark_remove_queued());
        pool.mark_remove_completed();
        pool.clear_remove_queued();
        assert!(!pool.should_queue_remove());
        assert!(pool.is_teardown_complete());
    }

    #[test]
    fn provided_recv_pool_rejects_unknown_bids_without_mutating_lifetime() {
        let mut pool = ProvidedRecvPool::new(7, 1);
        assert_eq!(pool.slot_id_for_bid(1), None);
        assert!(!pool.bid_is_valid(1));
        assert_eq!(pool.slot_state_for_bid(1), None);
        assert!(pool.provide_entry(1).is_none());
        assert_eq!(pool.lifetime, PoolLifetimeState::Active);
        assert!(pool.resolve_inflight_slot(1).is_none());
    }

    #[test]
    fn provided_recv_pool_generation_advances_on_reuse() {
        let mut pool = ProvidedRecvPool::new(7, 1);
        let first = pool.slot_id_for_bid(0).expect("first slot id");
        assert!(pool.provide_entry(0).is_some());
        let inflight = pool.resolve_inflight_slot(0).expect("inflight slot");
        assert_eq!(inflight.generation, first.generation);
        assert!(pool.complete_recv(inflight));
        let second = pool.slot_id_for_bid(0).expect("second slot id");
        assert!(second.generation > first.generation);
        assert!(pool.provide_entry(0).is_some());
        assert!(pool.slice(first, 1).is_none());
    }

    #[test]
    fn provided_recv_pool_resolves_only_inflight_slots() {
        let mut pool = ProvidedRecvPool::new(7, 1);
        assert!(pool.resolve_inflight_slot(0).is_none());
        assert!(pool.provide_entry(0).is_some());
        let inflight = pool.resolve_inflight_slot(0).expect("inflight slot");
        assert!(pool.slice(inflight, 1).is_some());
        assert!(pool.complete_recv(inflight));
        assert!(pool.resolve_inflight_slot(0).is_none());
    }

    #[test]
    fn provided_recv_pool_bid_lookup_respects_lifetime_transitions() {
        let mut pool = ProvidedRecvPool::new(7, 1);
        let active_slot = pool.slot_id_for_bid(0).expect("active slot");
        assert_eq!(active_slot.generation, 1);
        assert!(pool.provide_entry(0).is_some());

        pool.begin_draining();
        assert!(pool.is_draining());
        assert!(pool.provide_entry(0).is_none());
        let inflight = pool.resolve_inflight_slot(0).expect("inflight slot");
        assert_eq!(inflight.generation, active_slot.generation);
        assert!(pool.complete_recv(inflight));
        assert!(pool.advance_retirement());
        assert!(pool.is_removed());

        assert!(pool.slot_id_for_bid(0).is_some());
        assert!(pool.resolve_inflight_slot(0).is_none());
        assert!(pool.provide_entry(0).is_none());
        assert!(pool.slice(active_slot, 1).is_none());
    }

    #[test]
    fn provided_recv_pool_rejects_stale_cqe_after_slot_reuse() {
        let mut pool = ProvidedRecvPool::new(7, 1);
        assert!(pool.provide_entry(0).is_some());
        let old_inflight = pool.resolve_inflight_slot(0).expect("old inflight slot");
        assert!(pool.complete_recv(old_inflight));

        assert!(pool.provide_entry(0).is_some());
        let new_inflight = pool.resolve_inflight_slot(0).expect("new inflight slot");
        assert_ne!(new_inflight.generation, old_inflight.generation);

        assert!(!pool.complete_recv(old_inflight));
        assert!(pool.slice(new_inflight, 1).is_some());
        assert!(pool.complete_recv(new_inflight));
        assert!(pool.resolve_inflight_slot(0).is_none());
    }

    #[test]
    fn op_table_remove_rejects_stale_generation_token() {
        let mut table = OpTable::new();
        let token = table.insert(Op::Wakeup);
        let (idx, generation) = decode_token(token).expect("decoded token");
        let stale_generation = encode_token(idx, generation.wrapping_add(1));

        assert!(table.remove(stale_generation).is_none());
        assert!(matches!(table.remove(token), Some(Op::Wakeup)));
    }

    #[test]
    fn op_table_remove_rejects_token_after_slot_reuse() {
        let mut table = OpTable::new();
        let stale = table.insert(Op::Wakeup);
        assert!(matches!(table.remove(stale), Some(Op::Wakeup)));

        let fresh = table.insert(Op::Wakeup);
        assert_ne!(stale, fresh);
        assert!(table.remove(stale).is_none());
        assert!(matches!(table.remove(fresh), Some(Op::Wakeup)));
    }

    #[test]
    fn op_table_remove_rejects_index_and_generation_mismatch() {
        let mut table = OpTable::new();
        let token = table.insert(Op::Wakeup);
        let (idx, generation) = decode_token(token).expect("decoded token");

        let stale_idx = encode_token(idx + 1, generation);
        assert!(table.remove(stale_idx).is_none());
        assert!(matches!(table.remove(token), Some(Op::Wakeup)));
    }

    #[test]
    fn locked_read_pool_release_wraps_to_min_generation() {
        let mut pool = LockedReadBufPool::new(1).expect("locked read pool");
        let mut slot = pool.acquire_fixed_slot().expect("fixed slot");
        {
            let inner = pool.slots.get_mut(slot.index as usize).expect("slot");
            inner.generation = u32::MAX;
        }
        slot.generation = u32::MAX;
        pool.release_fixed_slot(slot)
            .expect("release wraps generation");
        let reacquired = pool.acquire_fixed_slot().expect("fixed slot");
        assert_eq!(reacquired.generation, 1);
    }

    #[test]
    fn locked_read_pool_release_rejects_stale_slot_identity() {
        let mut pool = LockedReadBufPool::new(1).expect("locked read pool");
        let slot = pool.acquire_fixed_slot().expect("fixed slot");
        pool.release_fixed_slot(slot)
            .expect("first release succeeds");
        assert_eq!(
            pool.release_fixed_slot(slot),
            Err(LockedReadReleaseError::InvalidSlotIdentity)
        );
    }

    #[test]
    fn op_table_remove_advances_wrap_to_min_generation() {
        let mut table = OpTable::new();
        let token = table.insert(Op::Wakeup);
        let (idx, _) = decode_token(token).expect("decoded token");
        table.slots.get_mut(idx).expect("slot").generation = u32::MAX;
        let wrapped = encode_token(idx, u32::MAX);
        let _ = table.remove(wrapped);
        assert_eq!(table.insert(Op::Wakeup), encode_token(idx, 1));
    }
}

#[inline]
fn encode_token(idx: usize, generation: u32) -> u64 {
    debug_assert!(idx <= u32::MAX as usize);
    ((generation as u64) << 32) | (idx as u64)
}

#[inline]
fn decode_token(token: u64) -> Option<(usize, u32)> {
    let generation = (token >> 32) as u32;
    if generation == 0 {
        return None;
    }
    let idx = (token & 0xffff_ffff) as usize;
    Some((idx, generation))
}

pub(super) fn make_listener(port: u16, backlog: i32) -> Result<Socket> {
    let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))
        .map_err(|e| NifError::from_errno(e.raw_os_error().unwrap_or(libc::EIO)))?;
    socket
        .set_reuse_address(true)
        .map_err(|e| NifError::from_errno(e.raw_os_error().unwrap_or(libc::EIO)))?;
    socket
        .set_only_v6(false)
        .map_err(|e| NifError::from_errno(e.raw_os_error().unwrap_or(libc::EIO)))?;
    socket
        .set_reuse_port(true)
        .map_err(|e| NifError::from_errno(e.raw_os_error().unwrap_or(libc::EIO)))?;
    socket
        .set_nonblocking(true)
        .map_err(|e| NifError::from_errno(e.raw_os_error().unwrap_or(libc::EIO)))?;
    socket
        .bind(&std::net::SocketAddrV6::new(std::net::Ipv6Addr::UNSPECIFIED, port, 0, 0).into())
        .map_err(|e| NifError::from_errno(e.raw_os_error().unwrap_or(libc::EIO)))?;
    socket
        .listen(backlog)
        .map_err(|e| NifError::from_errno(e.raw_os_error().unwrap_or(libc::EIO)))?;
    Ok(socket)
}

pub(super) fn local_addr_of_fd(fd: RawFd) -> Result<NetAddr> {
    // SAFETY: caller passes a live socket fd; `BorrowedFd` is used only within this function
    // and never outlives the raw fd ownership held by the caller.
    let borrowed_fd = unsafe { BorrowedFd::borrow_raw(fd) };
    let sock_ref = SockRef::from(&borrowed_fd);
    let addr = sock_ref
        .local_addr()
        .map_err(|e| NifError::from_errno(e.raw_os_error().unwrap_or(libc::EIO)))?;
    net_addr_from_sockaddr(&addr)
}

pub(super) fn peer_addr_of_fd(fd: RawFd) -> Result<NetAddr> {
    // SAFETY: caller passes a live socket fd; `BorrowedFd` is used only within this function
    // and never outlives the raw fd ownership held by the caller.
    let borrowed_fd = unsafe { BorrowedFd::borrow_raw(fd) };
    let sock_ref = SockRef::from(&borrowed_fd);
    let addr = sock_ref
        .peer_addr()
        .map_err(|e| NifError::from_errno(e.raw_os_error().unwrap_or(libc::EIO)))?;
    net_addr_from_sockaddr(&addr)
}

fn net_addr_from_sockaddr(addr: &socket2::SockAddr) -> Result<NetAddr> {
    match addr.as_socket() {
        Some(std::net::SocketAddr::V4(v4)) => Ok(NetAddr::V4(v4.ip().octets(), v4.port())),
        Some(std::net::SocketAddr::V6(v6)) => Ok(NetAddr::V6(v6.ip().segments(), v6.port())),
        None => Err(NifError::from_errno(libc::EINVAL)),
    }
}
