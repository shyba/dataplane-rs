use crate::mailbox_future::global_context::GlobalContext;
use crate::shared_wake::SharedTaskWakeHandle;
use slotmap::new_key_type;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

new_key_type! {
    pub struct SignalKey;
}

pub(crate) struct SignalEntry {
    fired: AtomicBool,
    wake: SignalWakeSlot,
}

struct SignalWakeSlot {
    inner: Mutex<Option<SharedTaskWakeHandle>>,
}

impl SignalWakeSlot {
    #[inline(always)]
    fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    #[inline(always)]
    fn register_or_return(
        &self,
        fired: &AtomicBool,
        wake: Option<SharedTaskWakeHandle>,
    ) -> Option<SharedTaskWakeHandle> {
        let mut pending_wake = wake;
        let mut slot = self.inner.lock().expect("signal wake mutex poisoned");
        if fired.load(Ordering::Acquire) {
            return pending_wake;
        }
        *slot = pending_wake.take();
        if fired.load(Ordering::Acquire) {
            pending_wake = slot.take();
        }
        pending_wake
    }

    #[inline(always)]
    fn clear(&self) {
        let _ = self
            .inner
            .lock()
            .expect("signal wake mutex poisoned")
            .take();
    }

    #[inline(always)]
    fn take_registered(&self) -> Option<SharedTaskWakeHandle> {
        self.inner
            .lock()
            .expect("signal wake mutex poisoned")
            .take()
    }
}

#[derive(Clone)]
pub struct RemoteSignalHandle {
    pub(crate) global: Arc<GlobalContext>,
    pub(crate) signal: SignalKey,
}

impl RemoteSignalHandle {
    #[inline(always)]
    pub(crate) fn new(global: Arc<GlobalContext>, signal: SignalKey) -> Self {
        Self { global, signal }
    }

    #[inline(always)]
    pub fn take_fired(&self) -> bool {
        self.global.take_signal(self.signal)
    }
}

impl SignalEntry {
    #[inline(always)]
    pub(crate) fn new() -> Self {
        Self {
            fired: AtomicBool::new(false),
            wake: SignalWakeSlot::new(),
        }
    }

    #[inline(always)]
    pub(crate) fn fire(&self) -> bool {
        !self.fired.swap(true, Ordering::AcqRel)
    }

    #[inline(always)]
    pub(crate) fn take_fired(&self) -> bool {
        self.fired.swap(false, Ordering::AcqRel)
    }

    #[inline(always)]
    pub(crate) fn register(
        &self,
        wake: Option<SharedTaskWakeHandle>,
    ) -> Option<SharedTaskWakeHandle> {
        self.wake.register_or_return(&self.fired, wake)
    }

    #[inline(always)]
    pub(crate) fn clear(&self) {
        self.wake.clear();
    }

    #[inline(always)]
    pub(crate) fn wake_registered(&self) {
        let wake = self.wake.take_registered();
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}
