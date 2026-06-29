use crate::mailbox_future::remote_task::{ShardMessage, ShardRuntimeQueues};
use crate::mailbox_future::result_cell::RemoteWaitCapacityError;
use crate::mailbox_future::shard_handle::ShardRuntimeHandle;
use crate::mailbox_future::signal::{SignalEntry, SignalKey};
use rtrb::{Consumer, Producer, RingBuffer};
use slotmap::SlotMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "linux")]
use rustix::thread::futex;
#[cfg(target_os = "linux")]
use rustix::thread::futex::Timespec;

const REMOTE_QUEUE_CAPACITY: usize = 65_536;
const REMOTE_WAIT_CAPACITY: u32 = 1 << 15;
#[cfg(target_os = "linux")]
const ONE_MILLISECOND: Timespec = Timespec {
    tv_sec: 0,
    tv_nsec: 1_000_000,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShardWaitTimeout {
    OneMillisecond,
    None,
}

pub struct GlobalContext {
    pub(crate) shard_count: usize,
    shard_signals: Vec<AtomicU32>,
    pub(crate) runtime_outbound: Vec<Vec<Mutex<Option<Producer<ShardMessage>>>>>,
    runtime_locals: Vec<Mutex<Option<ShardRuntimeQueues>>>,
    signals: Mutex<SlotMap<SignalKey, Arc<SignalEntry>>>,
    next_remote_index: AtomicU32,
}

impl GlobalContext {
    #[inline(always)]
    pub fn new(shard_count: usize) -> Self {
        assert!(shard_count > 0, "GlobalContext requires at least one shard");
        let (runtime_outbound, runtime_locals) = Self::build_runtime_queues(shard_count);
        Self {
            shard_count,
            shard_signals: (0..shard_count).map(|_| AtomicU32::new(0)).collect(),
            runtime_outbound,
            runtime_locals,
            signals: Mutex::new(SlotMap::with_key()),
            next_remote_index: AtomicU32::new(0),
        }
    }

    fn build_runtime_queues(shard_count: usize) -> RuntimeQueuesBuild {
        let mut outbound: Vec<Vec<Option<Producer<ShardMessage>>>> = (0..shard_count)
            .map(|_| (0..shard_count).map(|_| None).collect())
            .collect();
        let mut inbound: Vec<Vec<Option<Consumer<ShardMessage>>>> = (0..shard_count)
            .map(|_| (0..shard_count).map(|_| None).collect())
            .collect();

        for source in 0..shard_count {
            for target in 0..shard_count {
                if source == target {
                    continue;
                }
                let (prod, cons) = RingBuffer::<ShardMessage>::new(REMOTE_QUEUE_CAPACITY);
                outbound[source][target] = Some(prod);
                inbound[target][source] = Some(cons);
            }
        }

        let runtime_outbound = outbound
            .into_iter()
            .map(|row| row.into_iter().map(Mutex::new).collect())
            .collect();
        let runtime_locals = (0..shard_count)
            .map(|shard| {
                Mutex::new(Some(ShardRuntimeQueues {
                    inbound: inbound[shard].drain(..).collect(),
                }))
            })
            .collect();
        (runtime_outbound, runtime_locals)
    }

    #[inline(always)]
    pub fn shard_count(&self) -> usize {
        self.shard_count
    }

    #[inline(always)]
    pub fn target_shard_for_index(&self, index: u16) -> usize {
        index as usize % self.shard_count
    }

    #[inline(always)]
    pub fn shard_signal_epoch(&self, shard: usize) -> u32 {
        self.shard_signals[shard].load(Ordering::Acquire)
    }

    #[inline(always)]
    pub fn notify_shard(&self, shard: usize) {
        self.shard_signals[shard].fetch_add(1, Ordering::Release);
        // Current queue-based reactor paths do not block on the shard futex.
        // Keep the epoch update for polling paths, but skip the kernel wake.
    }

    #[inline(always)]
    pub fn wait_shard_signal(&self, shard: usize, seen: u32) {
        #[cfg(target_os = "linux")]
        self.wait_shard_signal_with_timeout(shard, seen, Some(&ONE_MILLISECOND));
        #[cfg(not(target_os = "linux"))]
        let _ = (shard, seen);
    }

    #[inline(always)]
    pub fn wait_shard_signal_mode(&self, shard: usize, seen: u32, mode: ShardWaitTimeout) {
        match mode {
            ShardWaitTimeout::OneMillisecond => self.wait_shard_signal(shard, seen),
            ShardWaitTimeout::None => {
                #[cfg(target_os = "linux")]
                self.wait_shard_signal_with_timeout(shard, seen, None);
                #[cfg(not(target_os = "linux"))]
                self.wait_shard_signal(shard, seen);
            }
        }
    }

    #[inline(always)]
    pub fn wait_shard_signal_with_timeout(
        &self,
        shard: usize,
        seen: u32,
        #[cfg(target_os = "linux")] timeout: Option<&Timespec>,
    ) {
        #[cfg(target_os = "linux")]
        {
            match futex::wait(
                &self.shard_signals[shard],
                futex::Flags::PRIVATE,
                seen,
                timeout,
            ) {
                Ok(()) => {}
                Err(err)
                    if err.raw_os_error() == libc::EAGAIN
                        || err.raw_os_error() == libc::ETIMEDOUT => {}
                Err(err) => {
                    // Treat unexpected futex failures as a lost wake and fall back to the
                    // normal polling path instead of unwinding through the reactor.
                    let _ = err;
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = (shard, seen);
    }

    #[inline(always)]
    pub(crate) fn register_signal(&self, signal: Arc<SignalEntry>) -> SignalKey {
        self.signals
            .lock()
            .expect("signal registry mutex poisoned")
            .insert(signal)
    }

    #[inline(always)]
    pub(crate) fn signal_entry(&self, key: SignalKey) -> Option<Arc<SignalEntry>> {
        self.signals
            .lock()
            .expect("signal registry mutex poisoned")
            .get(key)
            .cloned()
    }

    #[inline(always)]
    pub(crate) fn unregister_signal(&self, key: SignalKey) {
        let _ = self
            .signals
            .lock()
            .expect("signal registry mutex poisoned")
            .remove(key);
    }

    #[inline(always)]
    pub fn take_signal(&self, key: SignalKey) -> bool {
        self.signal_entry(key)
            .map(|entry| entry.take_fired())
            .unwrap_or(false)
    }

    #[inline(always)]
    pub(crate) fn fire_signal(&self, key: SignalKey, owner_shard: usize) {
        if let Some(entry) = self.signal_entry(key) {
            let _ = entry.fire();
            entry.wake_registered();
        }
        self.notify_shard(owner_shard);
    }

    #[inline(always)]
    pub fn shard_handle(self: &Arc<Self>, current_shard: usize) -> ShardRuntimeHandle {
        assert!(
            current_shard < self.shard_count,
            "current shard out of range"
        );
        let runtime = self.runtime_locals[current_shard]
            .lock()
            .expect("runtime handle registry mutex poisoned")
            .take()
            .expect("shard runtime handle already taken");
        ShardRuntimeHandle::new(self.clone(), current_shard, runtime, self.shard_count)
    }

    #[inline(always)]
    pub(crate) fn allocate_remote_index(
        &self,
        current_shard: usize,
    ) -> Result<u16, RemoteWaitCapacityError> {
        for _ in 0..REMOTE_WAIT_CAPACITY {
            let raw = self.next_remote_index.fetch_add(1, Ordering::Relaxed) % REMOTE_WAIT_CAPACITY;
            let index = raw as u16;
            if self.target_shard_for_index(index) != current_shard {
                return Ok(index);
            }
        }
        Err(RemoteWaitCapacityError)
    }

    #[inline(always)]
    pub(crate) fn choose_remote_shard(&self, current_shard: usize) -> usize {
        assert!(
            self.shard_count > 1,
            "spawn_any_detached requires at least two shards"
        );
        let offset = (self.next_remote_index.fetch_add(1, Ordering::Relaxed) as usize)
            % (self.shard_count - 1);
        let target = offset + 1;
        (current_shard + target) % self.shard_count
    }
}

type RuntimeOutboundQueues = Vec<Vec<Mutex<Option<Producer<ShardMessage>>>>>;
type RuntimeLocalQueues = Vec<Mutex<Option<ShardRuntimeQueues>>>;
type RuntimeQueuesBuild = (RuntimeOutboundQueues, RuntimeLocalQueues);
