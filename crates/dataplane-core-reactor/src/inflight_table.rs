use crate::reactor_model::OpToken;
use crate::wake_handle::WakeHandle;

#[derive(Clone, Copy, Debug)]
struct InflightSlot {
    generation: u32,
    next_free: u32,
    active: bool,
    wake: WakeHandle,
}

impl InflightSlot {
    #[inline(always)]
    fn vacant(next_free: u32) -> Self {
        Self {
            generation: 0,
            next_free,
            active: false,
            wake: WakeHandle::None,
        }
    }

    #[inline(always)]
    fn activate(&mut self, wake: WakeHandle) {
        self.next_free = u32::MAX;
        self.active = true;
        self.wake = wake;
    }

    #[inline(always)]
    fn activate_exact(&mut self, generation: u32, wake: WakeHandle) {
        self.generation = generation;
        self.activate(wake);
    }

    #[inline(always)]
    fn release_to_free_list(&mut self, next_free: u32) -> WakeHandle {
        self.active = false;
        self.generation = self.generation.wrapping_add(1);
        let wake = self.wake;
        self.wake = WakeHandle::None;
        self.next_free = next_free;
        wake
    }
}

pub(crate) struct InflightTable {
    slots: Vec<InflightSlot>,
    free_head: u32,
    active: usize,
}

impl InflightTable {
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        let mut slots = Vec::with_capacity(capacity);
        let mut idx = 0usize;
        while idx < capacity {
            let next = if idx + 1 < capacity {
                (idx + 1) as u32
            } else {
                u32::MAX
            };
            slots.push(InflightSlot::vacant(next));
            idx += 1;
        }
        Self {
            slots,
            free_head: 0,
            active: 0,
        }
    }

    /// Allocates a token. Panics only when the table would exceed
    /// `u32::MAX` simultaneously in-flight ops — an intentional hard bound,
    /// unreachable before the process runs out of memory for the ops
    /// themselves.
    #[inline(always)]
    pub fn alloc(&mut self, wake: WakeHandle) -> OpToken {
        if self.free_head == u32::MAX {
            self.grow();
        }
        let idx = self.free_head as usize;
        let slot = &mut self.slots[idx];
        self.free_head = slot.next_free;
        slot.activate(wake);
        self.active += 1;
        OpToken(((slot.generation as u64) << 32) | (idx as u64 + 1))
    }

    /// Claims the exact slot a previously-minted token names. The external
    /// token protocol allows re-inserting the most recently removed token
    /// (whose generation is one behind the slot after release), but tokens
    /// from earlier lives are rejected: a stale token must not resurrect a
    /// recycled slot and roll its generation further backwards.
    #[inline(always)]
    pub fn insert_exact(&mut self, token: OpToken, wake: WakeHandle) -> bool {
        let raw_idx = token.0 as u32;
        if raw_idx == 0 {
            return false;
        }
        let idx = raw_idx as usize - 1;
        let generation = (token.0 >> 32) as u32;
        let Some(slot) = self.slots.get(idx) else {
            return false;
        };
        if slot.active || slot.generation.wrapping_sub(generation) > 1 {
            return false;
        }
        if self.unlink_free(idx as u32).is_none() {
            return false;
        }
        let slot = &mut self.slots[idx];
        slot.activate_exact(generation, wake);
        self.active += 1;
        true
    }

    #[inline(always)]
    pub fn remove(&mut self, token: OpToken) -> Option<WakeHandle> {
        let raw_idx = token.0 as u32;
        if raw_idx == 0 {
            return None;
        }
        let idx = raw_idx as usize - 1;
        let generation = (token.0 >> 32) as u32;
        let slot = self.slots.get_mut(idx)?;
        if !slot.active || slot.generation != generation {
            return None;
        }
        let wake = slot.release_to_free_list(self.free_head);
        self.free_head = idx as u32;
        self.active = self.active.saturating_sub(1);
        Some(wake)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.active == 0
    }

    #[cfg(test)]
    fn force_slot_generation(&mut self, idx: usize, generation: u32) {
        self.slots[idx].generation = generation;
    }

    fn grow(&mut self) {
        let old_len = self.slots.len();
        let new_len = (old_len.max(1) * 2).min(u32::MAX as usize);
        assert!(new_len > old_len, "inflight table exhausted");
        let mut idx = old_len;
        while idx < new_len {
            let next = if idx + 1 < new_len {
                (idx + 1) as u32
            } else {
                self.free_head
            };
            self.slots.push(InflightSlot::vacant(next));
            idx += 1;
        }
        self.free_head = old_len as u32;
    }

    fn unlink_free(&mut self, target: u32) -> Option<()> {
        let mut current = self.free_head;
        let mut prev = u32::MAX;
        while current != u32::MAX {
            if current == target {
                let next = self.slots[current as usize].next_free;
                if prev == u32::MAX {
                    self.free_head = next;
                } else {
                    self.slots[prev as usize].next_free = next;
                }
                self.slots[current as usize].next_free = u32::MAX;
                return Some(());
            }
            prev = current;
            current = self.slots[current as usize].next_free;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{InflightSlot, InflightTable};
    use crate::reactor_model::OpToken;
    use crate::wake_handle::WakeHandle;
    use proptest::prelude::*;

    #[test]
    fn alloc_remove_roundtrip() {
        let mut table = InflightTable::with_capacity(2);
        let token = table.alloc(WakeHandle::None);
        assert_ne!(token, crate::reactor_model::OpToken(0));
        assert_eq!(table.remove(token), Some(WakeHandle::None));
        assert_eq!(table.active, 0);
    }

    #[test]
    fn stale_token_does_not_match_reused_slot() {
        let mut table = InflightTable::with_capacity(1);
        let first = table.alloc(WakeHandle::None);
        assert_eq!(table.remove(first), Some(WakeHandle::None));
        let second = table.alloc(WakeHandle::None);
        assert_ne!(first, second);
        assert_eq!(table.remove(first), None);
        assert_eq!(table.remove(second), Some(WakeHandle::None));
    }

    #[test]
    fn insert_exact_activates_known_free_slot() {
        let mut table = InflightTable::with_capacity(1);
        let token = table.alloc(WakeHandle::None);
        assert_eq!(table.remove(token), Some(WakeHandle::None));
        assert!(table.insert_exact(token, WakeHandle::None));
        assert_eq!(table.remove(token), Some(WakeHandle::None));
    }

    #[test]
    fn slot_lifecycle_helpers_preserve_generation_and_reset_wake() {
        let mut slot = InflightSlot::vacant(9);
        assert!(!slot.active);
        assert_eq!(slot.next_free, 9);
        assert_eq!(slot.wake, WakeHandle::None);

        slot.activate(WakeHandle::None);
        assert!(slot.active);
        assert_eq!(slot.next_free, u32::MAX);

        let task = WakeHandle::None;
        slot.activate_exact(7, task);
        assert_eq!(slot.generation, 7);
        assert!(slot.active);

        let wake = slot.release_to_free_list(5);
        assert_eq!(wake, WakeHandle::None);
        assert!(!slot.active);
        assert_eq!(slot.generation, 8);
        assert_eq!(slot.next_free, 5);
        assert_eq!(slot.wake, WakeHandle::None);
    }

    #[test]
    fn zero_token_is_never_valid() {
        let mut table = InflightTable::with_capacity(1);
        assert!(!table.insert_exact(OpToken(0), WakeHandle::None));
        assert_eq!(table.remove(OpToken(0)), None);
        let token = table.alloc(WakeHandle::None);
        assert_ne!(token, OpToken(0));
    }

    proptest! {
        #[test]
        fn single_slot_tokens_are_non_zero_and_stale_tokens_never_match(cycles in 1usize..64) {
            let mut table = InflightTable::with_capacity(1);
            let mut previous = None;

            for _ in 0..cycles {
                let token = table.alloc(WakeHandle::None);
                prop_assert_ne!(token, OpToken(0));
                if let Some(stale) = previous {
                    prop_assert_ne!(token, stale);
                    prop_assert_eq!(table.remove(stale), None);
                }
                prop_assert_eq!(table.remove(token), Some(WakeHandle::None));
                previous = Some(token);
            }
        }

        #[test]
        fn removed_token_can_be_reinserted_exactly_once(rounds in 1usize..32) {
            let mut table = InflightTable::with_capacity(1);
            let token = table.alloc(WakeHandle::None);
            prop_assert_eq!(table.remove(token), Some(WakeHandle::None));

            for _ in 0..rounds {
                prop_assert!(table.insert_exact(token, WakeHandle::None));
                prop_assert_eq!(table.remove(token), Some(WakeHandle::None));
                prop_assert!(!table.insert_exact(OpToken(0), WakeHandle::None));
            }
        }
    }
}

#[cfg(test)]
mod generation_wrap_tests {
    use super::InflightTable;
    use crate::wake_handle::WakeHandle;

    #[test]
    fn stale_token_from_earlier_life_cannot_resurrect_slot() {
        let mut table = InflightTable::with_capacity(1);
        let first_life = table.alloc(WakeHandle::None);
        assert!(table.remove(first_life).is_some());
        // Advance the slot two lives past the first token.
        let second_life = table.alloc(WakeHandle::None);
        assert!(table.remove(second_life).is_some());

        assert!(
            !table.insert_exact(first_life, WakeHandle::None),
            "token two generations behind must be rejected"
        );
        assert!(table.insert_exact(second_life, WakeHandle::None));
    }

    /// Documents the ABA bound: after 2^32 release cycles a slot's
    /// generation wraps and a token from its first life aliases a live
    /// token. Callers rely on op lifetimes being far shorter than 2^32
    /// recycles of one slot; this test pins the boundary behavior so the
    /// assumption is explicit rather than silent.
    #[test]
    fn generation_wrap_aliases_first_life_token() {
        let mut table = InflightTable::with_capacity(1);
        let first_life = table.alloc(WakeHandle::None);
        assert!(table.remove(first_life).is_some());

        table.force_slot_generation(0, u32::MAX);
        let last = table.alloc(WakeHandle::None);
        assert!(table.remove(last).is_some());

        let wrapped = table.alloc(WakeHandle::None);
        assert_eq!(
            wrapped, first_life,
            "generation wrapped back to the first-life token (known 2^32 bound)"
        );
    }
}
