use crate::mailbox_future::SignalKey;
use crate::native_task::TaskRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WakeHandle {
    None,
    LocalTask(TaskRef),
    Signal(SignalKey),
}

impl WakeHandle {
    #[inline(always)]
    pub const fn is_none(self) -> bool {
        matches!(self, Self::None)
    }

    #[inline(always)]
    pub const fn local_task(self) -> Option<TaskRef> {
        match self {
            Self::LocalTask(task) => Some(task),
            Self::None | Self::Signal(_) => None,
        }
    }

    #[inline(always)]
    pub const fn signal(self) -> Option<SignalKey> {
        match self {
            Self::Signal(signal) => Some(signal),
            Self::None | Self::LocalTask(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WakeHandle;
    use crate::mailbox_future::SignalKey;
    use crate::native_task::TaskRef;
    use slotmap::Key;

    #[test]
    fn wake_handle_accessors_match_variants() {
        let none = WakeHandle::None;
        assert!(none.is_none());
        assert_eq!(none.local_task(), None);
        assert_eq!(none.signal(), None);

        let task = TaskRef::new(7, 3);
        let local = WakeHandle::LocalTask(task);
        assert!(!local.is_none());
        assert_eq!(local.local_task(), Some(task));
        assert_eq!(local.signal(), None);

        let signal = SignalKey::null();
        let wake = WakeHandle::Signal(signal);
        assert!(!wake.is_none());
        assert_eq!(wake.local_task(), None);
        assert_eq!(wake.signal(), Some(signal));
    }
}
