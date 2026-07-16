use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

struct SharedTaskWake {
    pending: Arc<AtomicBool>,
    reactor_pending: Arc<AtomicBool>,
}

/// Cross-thread wake handle for a shard loop: `wake()` sets a task-pending
/// flag and, on the first pending transition, a reactor-pending flag the
/// host's wait loop is expected to poll. Construct one per shard thread and
/// install it with `runtime_tls::enter_current_shared_wake` so cross-shard
/// receivers polled on that thread can register reactor wakes.
#[derive(Clone)]
pub struct SharedTaskWakeHandle(Arc<SharedTaskWake>);

// Manual PartialEq: compare pointer identity of the Arc, not the contents.
// Two SharedTaskWakeHandles are equal iff they point to the same allocation.
impl PartialEq for SharedTaskWakeHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for SharedTaskWakeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SharedTaskWakeHandle").finish()
    }
}

impl SharedTaskWakeHandle {
    #[inline(always)]
    pub fn new(
        pending: Arc<AtomicBool>,
        reactor_pending: Arc<AtomicBool>,
    ) -> SharedTaskWakeHandle {
        SharedTaskWakeHandle(Arc::new(SharedTaskWake {
            pending,
            reactor_pending,
        }))
    }

    #[inline(always)]
    pub fn wake(&self) {
        if !self.0.pending.swap(true, Ordering::AcqRel) {
            self.0.reactor_pending.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SharedTaskWakeHandle;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};

    fn new_handle() -> (SharedTaskWakeHandle, Arc<AtomicBool>, Arc<AtomicBool>) {
        let pending = Arc::new(AtomicBool::new(false));
        let reactor_pending = Arc::new(AtomicBool::new(false));
        let handle = SharedTaskWakeHandle::new(pending.clone(), reactor_pending.clone());
        (handle, pending, reactor_pending)
    }

    #[test]
    fn first_wake_sets_both_flags() {
        let (handle, pending, reactor_pending) = new_handle();

        handle.wake();

        assert!(pending.load(Ordering::Acquire));
        assert!(reactor_pending.load(Ordering::Acquire));
    }

    #[test]
    fn repeated_wake_is_idempotent() {
        let (handle, pending, reactor_pending) = new_handle();

        handle.wake();
        reactor_pending.store(false, Ordering::Release);
        handle.wake();

        assert!(pending.load(Ordering::Acquire));
        assert!(!reactor_pending.load(Ordering::Acquire));
    }

    #[test]
    fn clones_share_the_same_wake_state() {
        let (handle, pending, reactor_pending) = new_handle();
        let clone = handle.clone();

        clone.wake();

        assert!(pending.load(Ordering::Acquire));
        assert!(reactor_pending.load(Ordering::Acquire));
        reactor_pending.store(false, Ordering::Release);

        handle.wake();

        assert!(pending.load(Ordering::Acquire));
        assert!(!reactor_pending.load(Ordering::Acquire));
    }
}
