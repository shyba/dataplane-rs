use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Result state constants for RemoteTaskResult.
///
/// ## State Diagram
/// ```text
///  RESULT_WAITING (0)
///       │
///       ▼ complete() succeeds (Release semantics on `state`)
///       │
///  RESULT_READY (1)
///       │
///       ▼ take_ready() wins swap to RESULT_CLOSED (AcqRel semantics)
///       │
///  RESULT_CLOSED (2) ◄── close() forces swap (AcqRel)
/// ```
///
/// ## Legal Transitions
/// - RESULT_WAITING → RESULT_READY  : sender calls `complete()` before receiver takes
/// - RESULT_WAITING → RESULT_CLOSED : sender drops (or `close()` called) before `complete()`
/// - RESULT_READY   → RESULT_CLOSED : exactly one `take_ready()` call wins via `swap(CLOSED, AcqRel)`
/// - RESULT_CLOSED  → RESULT_CLOSED : subsequent `close()` calls are idempotent (no double-drop)
/// - RESULT_READY   → RESULT_CLOSED : `close()` after `complete()` drops the initialized payload
///
/// ## Illegal Transitions
/// - RESULT_READY → RESULT_WAITING : once fired a result cannot revert
/// - RESULT_CLOSED → RESULT_READY  : closed is terminal
/// - RESULT_CLOSED → RESULT_WAITING: closed is terminal
pub(crate) const RESULT_WAITING: u8 = 0;
pub(crate) const RESULT_READY: u8 = 1;
pub(crate) const RESULT_CLOSED: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteTaskRecvError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteWaitCapacityError;

pub(crate) struct RemoteTaskResult<T: Send + 'static> {
    pub(crate) state: AtomicU8,
    pub(crate) receiver_open: AtomicBool,
    result: ResultCell<T>,
}

// SAFETY: Cross-thread access is synchronized by `state` and `receiver_open`.
// Producer path: `complete` writes `result` and then publishes `RESULT_READY` with
// `Release`, so any consumer that observes ready with `Acquire` sees the initialized
// value. Consumer path: `take_ready` uses `swap(RESULT_CLOSED, AcqRel)` so exactly one
// caller can win the ready-to-closed transition and perform `assume_init_read()`.
// Drop path: value destruction is guarded by state transitions (`close`, `take_ready`,
// and `Drop`), so the initialized payload is dropped at most once.
unsafe impl<T: Send + 'static> Sync for RemoteTaskResult<T> {}

struct ResultCell<T> {
    value: UnsafeCell<MaybeUninit<T>>,
}

impl<T> ResultCell<T> {
    #[inline(always)]
    fn uninit() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    #[inline(always)]
    fn write(&self, value: T) {
        // SAFETY: `RemoteTaskResult::complete` is the only writer and it publishes
        // `RESULT_READY` only after this write completes.
        unsafe {
            (*self.value.get()).write(value);
        }
    }

    #[inline(always)]
    fn read(&self) -> T {
        // SAFETY: callers either win the `RESULT_READY -> RESULT_CLOSED` transition
        // or are the producer recovering a just-written value after `close()` won
        // before publish. In both cases exactly one reader owns the initialized value.
        unsafe { (*self.value.get()).assume_init_read() }
    }

    #[inline(always)]
    fn drop_in_place(&self) {
        // SAFETY: callers only drop after observing `RESULT_READY` and closing the
        // cell, so the initialized value is destroyed at most once.
        unsafe {
            (*self.value.get()).assume_init_drop();
        }
    }
}

impl<T: Send + 'static> RemoteTaskResult<T> {
    #[inline(always)]
    pub(crate) fn new(owner_shard: usize) -> Self {
        let _ = owner_shard;
        Self {
            state: AtomicU8::new(RESULT_WAITING),
            receiver_open: AtomicBool::new(true),
            result: ResultCell::uninit(),
        }
    }

    #[inline(always)]
    pub(crate) fn complete(&self, value: T) -> Result<(), T> {
        if !self.receiver_open.load(Ordering::Acquire) {
            return Err(value);
        }
        if self.state.load(Ordering::Acquire) != RESULT_WAITING {
            return Err(value);
        }
        self.result.write(value);
        match self.state.compare_exchange(
            RESULT_WAITING,
            RESULT_READY,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(_) => Err(self.result.read()),
        }
    }

    #[inline(always)]
    pub(crate) fn close(&self) {
        let prev = self.state.swap(RESULT_CLOSED, Ordering::AcqRel);
        if prev == RESULT_READY {
            self.result.drop_in_place();
        }
    }

    #[inline(always)]
    pub(crate) fn take_ready(&self) -> Option<T> {
        if self.state.load(Ordering::Acquire) != RESULT_READY {
            return None;
        }
        let prev = self.state.swap(RESULT_CLOSED, Ordering::AcqRel);
        if prev != RESULT_READY {
            return None;
        }
        let value = self.result.read();
        self.receiver_open.store(false, Ordering::Release);
        Some(value)
    }
}

impl<T: Send + 'static> Drop for RemoteTaskResult<T> {
    fn drop(&mut self) {
        if self.state.load(Ordering::Acquire) == RESULT_READY {
            self.result.drop_in_place();
        }
    }
}
