use crate::runtime_reactor::{BUF_SIZE, SUBSCRIBE_PAGE_SIZE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ArenaClass {
    Small4K,
    Large64K,
}

impl ArenaClass {
    #[inline]
    pub(super) const fn slot_size(self) -> usize {
        match self {
            ArenaClass::Small4K => SUBSCRIBE_PAGE_SIZE,
            ArenaClass::Large64K => BUF_SIZE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ArenaHandle {
    pub(super) class: ArenaClass,
    pub(super) slot: usize,
    pub(super) off: usize,
    pub(super) len: usize,
}

impl ArenaHandle {
    #[inline]
    pub(super) fn new(class: ArenaClass, slot: usize, len: usize) -> Self {
        Self {
            class,
            slot,
            off: 0,
            len,
        }
    }
}

#[repr(align(64))]
struct CacheAligned<T>(T);

struct FixedSlotArena<const SLOT_SIZE: usize> {
    slots: Vec<Box<CacheAligned<[u8; SLOT_SIZE]>>>,
    free: Vec<usize>,
}

impl<const SLOT_SIZE: usize> FixedSlotArena<SLOT_SIZE> {
    fn new(slots: usize) -> Self {
        let mut arena_slots = Vec::with_capacity(slots);
        let mut free = Vec::with_capacity(slots);
        for idx in 0..slots {
            arena_slots.push(Box::new(CacheAligned([0u8; SLOT_SIZE])));
            free.push(slots - idx - 1);
        }
        Self {
            slots: arena_slots,
            free,
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    fn acquire(&mut self) -> Option<usize> {
        self.free.pop()
    }

    #[inline]
    fn release(&mut self, slot: usize) {
        self.free.push(slot);
    }

    #[inline]
    fn slice(&self, slot: usize, off: usize, len: usize) -> &[u8] {
        &self.slots[slot].0[off..off + len]
    }

    #[inline]
    fn slice_mut_prefix(&mut self, slot: usize, len: usize) -> &mut [u8] {
        &mut self.slots[slot].0[..len]
    }

    #[inline]
    fn slice_mut_full(&mut self, slot: usize) -> &mut [u8] {
        &mut self.slots[slot].0[..]
    }

    fn registered_iovecs(&self) -> Vec<libc::iovec> {
        self.slots
            .iter()
            .map(|buf| libc::iovec {
                iov_base: buf.0.as_ptr() as *mut libc::c_void,
                iov_len: buf.0.len(),
            })
            .collect()
    }
}

#[repr(align(64))]
pub(super) struct RuntimeArenas {
    small: FixedSlotArena<SUBSCRIBE_PAGE_SIZE>,
    large: FixedSlotArena<BUF_SIZE>,
}

impl RuntimeArenas {
    pub(super) fn new(large_slots: usize, small_slots: usize) -> Self {
        Self {
            small: FixedSlotArena::new(small_slots),
            large: FixedSlotArena::new(large_slots),
        }
    }

    #[inline]
    pub(super) fn acquire_class(&mut self, class: ArenaClass) -> Option<ArenaHandle> {
        let slot = match class {
            ArenaClass::Small4K => self.small.acquire()?,
            ArenaClass::Large64K => self.large.acquire()?,
        };
        Some(ArenaHandle::new(class, slot, class.slot_size()))
    }

    #[inline]
    pub(super) fn acquire_for_len(&mut self, len: usize) -> Option<ArenaHandle> {
        if len <= SUBSCRIBE_PAGE_SIZE {
            if let Some(mut handle) = self.acquire_class(ArenaClass::Small4K) {
                handle.len = len;
                return Some(handle);
            }
        }
        if len <= BUF_SIZE {
            let mut handle = self.acquire_class(ArenaClass::Large64K)?;
            handle.len = len;
            return Some(handle);
        }
        None
    }

    #[inline]
    pub(super) fn release(&mut self, handle: ArenaHandle) {
        match handle.class {
            ArenaClass::Small4K => self.small.release(handle.slot),
            ArenaClass::Large64K => self.large.release(handle.slot),
        }
    }

    #[inline]
    pub(super) fn slice_handle(&self, handle: ArenaHandle) -> &[u8] {
        self.slice(handle.class, handle.slot, handle.off, handle.len)
    }

    #[inline]
    pub(super) fn slice(&self, class: ArenaClass, slot: usize, off: usize, len: usize) -> &[u8] {
        match class {
            ArenaClass::Small4K => self.small.slice(slot, off, len),
            ArenaClass::Large64K => self.large.slice(slot, off, len),
        }
    }

    #[inline]
    pub(super) fn slice_mut_prefix(
        &mut self,
        class: ArenaClass,
        slot: usize,
        len: usize,
    ) -> &mut [u8] {
        match class {
            ArenaClass::Small4K => self.small.slice_mut_prefix(slot, len),
            ArenaClass::Large64K => self.large.slice_mut_prefix(slot, len),
        }
    }

    #[inline]
    pub(super) fn slice_mut_full(&mut self, handle: ArenaHandle) -> &mut [u8] {
        match handle.class {
            ArenaClass::Small4K => self.small.slice_mut_full(handle.slot),
            ArenaClass::Large64K => self.large.slice_mut_full(handle.slot),
        }
    }

    #[inline]
    pub(super) fn small_len(&self) -> usize {
        self.small.len()
    }

    pub(super) fn small_registered_iovecs(&self) -> Vec<libc::iovec> {
        self.small.registered_iovecs()
    }
}

#[cfg(test)]
mod tests {
    use super::{ArenaClass, RuntimeArenas};
    use crate::runtime_reactor::{BUF_SIZE, SUBSCRIBE_PAGE_SIZE};

    #[test]
    fn acquire_for_len_uses_small_then_large_classes() {
        let mut arenas = RuntimeArenas::new(2, 2);

        let small = arenas.acquire_for_len(SUBSCRIBE_PAGE_SIZE).unwrap();
        assert_eq!(small.class, ArenaClass::Small4K);
        assert_eq!(small.len, SUBSCRIBE_PAGE_SIZE);

        let large = arenas.acquire_for_len(SUBSCRIBE_PAGE_SIZE + 1).unwrap();
        assert_eq!(large.class, ArenaClass::Large64K);
        assert_eq!(large.len, SUBSCRIBE_PAGE_SIZE + 1);
    }

    #[test]
    fn write_then_read_roundtrip_from_handle() {
        let mut arenas = RuntimeArenas::new(1, 1);
        let mut handle = arenas.acquire_for_len(64).unwrap();
        assert_eq!(handle.class, ArenaClass::Small4K);
        arenas
            .slice_mut_prefix(handle.class, handle.slot, 64)
            .copy_from_slice(&[7u8; 64]);
        handle.off = 16;
        handle.len = 32;
        let view = arenas.slice_handle(handle);
        assert_eq!(view.len(), 32);
        assert!(view.iter().all(|b| *b == 7u8));
    }

    #[test]
    fn release_returns_slot_to_same_class_pool() {
        let mut arenas = RuntimeArenas::new(1, 1);
        let small = arenas.acquire_class(ArenaClass::Small4K).unwrap();
        let large = arenas.acquire_class(ArenaClass::Large64K).unwrap();
        arenas.release(small);
        arenas.release(large);
        let small_again = arenas.acquire_class(ArenaClass::Small4K).unwrap();
        let large_again = arenas.acquire_class(ArenaClass::Large64K).unwrap();
        assert_eq!(small.slot, small_again.slot);
        assert_eq!(large.slot, large_again.slot);
        assert_eq!(large_again.len, BUF_SIZE);
    }
}
