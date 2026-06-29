use super::{EmbeddedProfileLayout, PerformanceProfileLayout};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BalancedParkSlotsConfig {
    pub slot_count: usize,
}

impl Default for BalancedParkSlotsConfig {
    fn default() -> Self {
        Self { slot_count: 256 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BalancedParkSlotId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BalancedWakeToken(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BalancedParkLease {
    pub slot: BalancedParkSlotId,
    pub generation: u64,
    pub wake_token: BalancedWakeToken,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalancedParkSlotState {
    Vacant,
    Armed {
        generation: u64,
        wake_token: BalancedWakeToken,
    },
    Woken {
        generation: u64,
        wake_token: BalancedWakeToken,
    },
    Cancelled {
        generation: u64,
        wake_token: BalancedWakeToken,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BalancedParkSlot {
    generation: u64,
    state: BalancedParkSlotState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalancedParkSlots {
    slots: Vec<BalancedParkSlot>,
    next_probe: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddedParkStore {
    slots: Vec<BalancedParkSlot>,
    next_probe: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerformanceParkStore {
    slots: Vec<BalancedParkSlot>,
    next_probe: usize,
}

pub trait ParkStoreOps {
    fn arm_next(&mut self, wake_token: BalancedWakeToken) -> Option<BalancedParkLease>;
    fn wake(&mut self, lease: BalancedParkLease) -> bool;
    fn cancel(&mut self, lease: BalancedParkLease) -> bool;
    fn release(&mut self, lease: BalancedParkLease) -> bool;
    fn state(&self, slot: BalancedParkSlotId) -> Option<BalancedParkSlotState>;
    fn slot_count(&self) -> usize;
}

impl EmbeddedParkStore {
    #[inline]
    pub fn from_config(config: BalancedParkSlotsConfig) -> Self {
        let mut slots = Vec::with_capacity(config.slot_count);
        for _ in 0..config.slot_count {
            slots.push(BalancedParkSlot {
                generation: 0,
                state: BalancedParkSlotState::Vacant,
            });
        }
        Self {
            slots,
            next_probe: 0,
        }
    }

    #[inline]
    pub fn config_from_layout(layout: &EmbeddedProfileLayout) -> BalancedParkSlotsConfig {
        layout.park_slots_config()
    }

    #[inline]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    pub fn state(&self, slot: BalancedParkSlotId) -> Option<BalancedParkSlotState> {
        self.slots.get(slot.0).map(|slot| slot.state)
    }
}

impl ParkStoreOps for EmbeddedParkStore {
    #[inline]
    fn arm_next(&mut self, wake_token: BalancedWakeToken) -> Option<BalancedParkLease> {
        if self.slots.is_empty() {
            return None;
        }

        for step in 0..self.slots.len() {
            let idx = (self.next_probe + step) % self.slots.len();
            let slot = &mut self.slots[idx];
            if slot.state == BalancedParkSlotState::Vacant {
                slot.generation = slot.generation.wrapping_add(1).max(1);
                let lease = BalancedParkLease {
                    slot: BalancedParkSlotId(idx),
                    generation: slot.generation,
                    wake_token,
                };
                slot.state = BalancedParkSlotState::Armed {
                    generation: lease.generation,
                    wake_token,
                };
                self.next_probe = (idx + 1) % self.slots.len();
                return Some(lease);
            }
        }

        None
    }

    #[inline]
    fn wake(&mut self, lease: BalancedParkLease) -> bool {
        let Some(slot) = self.slots.get_mut(lease.slot.0) else {
            return false;
        };
        match slot.state {
            BalancedParkSlotState::Armed {
                generation,
                wake_token,
            } if generation == lease.generation && wake_token == lease.wake_token => {
                slot.state = BalancedParkSlotState::Woken {
                    generation,
                    wake_token,
                };
                true
            }
            _ => false,
        }
    }

    #[inline]
    fn cancel(&mut self, lease: BalancedParkLease) -> bool {
        let Some(slot) = self.slots.get_mut(lease.slot.0) else {
            return false;
        };
        match slot.state {
            BalancedParkSlotState::Armed {
                generation,
                wake_token,
            } if generation == lease.generation && wake_token == lease.wake_token => {
                slot.state = BalancedParkSlotState::Cancelled {
                    generation,
                    wake_token,
                };
                true
            }
            _ => false,
        }
    }

    #[inline]
    fn release(&mut self, lease: BalancedParkLease) -> bool {
        let Some(slot) = self.slots.get_mut(lease.slot.0) else {
            return false;
        };
        match slot.state {
            BalancedParkSlotState::Woken {
                generation,
                wake_token,
            }
            | BalancedParkSlotState::Cancelled {
                generation,
                wake_token,
            } if generation == lease.generation && wake_token == lease.wake_token => {
                slot.state = BalancedParkSlotState::Vacant;
                true
            }
            _ => false,
        }
    }

    #[inline]
    fn state(&self, slot: BalancedParkSlotId) -> Option<BalancedParkSlotState> {
        self.state(slot)
    }

    #[inline]
    fn slot_count(&self) -> usize {
        self.slot_count()
    }
}

impl PerformanceParkStore {
    #[inline]
    pub fn from_config(config: BalancedParkSlotsConfig) -> Self {
        let mut slots = Vec::with_capacity(config.slot_count);
        for _ in 0..config.slot_count {
            slots.push(BalancedParkSlot {
                generation: 0,
                state: BalancedParkSlotState::Vacant,
            });
        }
        Self {
            slots,
            next_probe: 0,
        }
    }

    #[inline]
    pub fn config_from_layout(layout: &PerformanceProfileLayout) -> BalancedParkSlotsConfig {
        layout.park_slots_config()
    }

    #[inline]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    pub fn state(&self, slot: BalancedParkSlotId) -> Option<BalancedParkSlotState> {
        self.slots.get(slot.0).map(|slot| slot.state)
    }
}

impl ParkStoreOps for PerformanceParkStore {
    #[inline]
    fn arm_next(&mut self, wake_token: BalancedWakeToken) -> Option<BalancedParkLease> {
        if self.slots.is_empty() {
            return None;
        }

        for step in 0..self.slots.len() {
            let idx = (self.next_probe + step) % self.slots.len();
            let slot = &mut self.slots[idx];
            if slot.state == BalancedParkSlotState::Vacant {
                slot.generation = slot.generation.wrapping_add(1).max(1);
                let lease = BalancedParkLease {
                    slot: BalancedParkSlotId(idx),
                    generation: slot.generation,
                    wake_token,
                };
                slot.state = BalancedParkSlotState::Armed {
                    generation: lease.generation,
                    wake_token,
                };
                self.next_probe = (idx + 1) % self.slots.len();
                return Some(lease);
            }
        }

        None
    }

    #[inline]
    fn wake(&mut self, lease: BalancedParkLease) -> bool {
        let Some(slot) = self.slots.get_mut(lease.slot.0) else {
            return false;
        };
        match slot.state {
            BalancedParkSlotState::Armed {
                generation,
                wake_token,
            } if generation == lease.generation && wake_token == lease.wake_token => {
                slot.state = BalancedParkSlotState::Woken {
                    generation,
                    wake_token,
                };
                true
            }
            _ => false,
        }
    }

    #[inline]
    fn cancel(&mut self, lease: BalancedParkLease) -> bool {
        let Some(slot) = self.slots.get_mut(lease.slot.0) else {
            return false;
        };
        match slot.state {
            BalancedParkSlotState::Armed {
                generation,
                wake_token,
            } if generation == lease.generation && wake_token == lease.wake_token => {
                slot.state = BalancedParkSlotState::Cancelled {
                    generation,
                    wake_token,
                };
                true
            }
            _ => false,
        }
    }

    #[inline]
    fn release(&mut self, lease: BalancedParkLease) -> bool {
        let Some(slot) = self.slots.get_mut(lease.slot.0) else {
            return false;
        };
        match slot.state {
            BalancedParkSlotState::Woken {
                generation,
                wake_token,
            }
            | BalancedParkSlotState::Cancelled {
                generation,
                wake_token,
            } if generation == lease.generation && wake_token == lease.wake_token => {
                slot.state = BalancedParkSlotState::Vacant;
                true
            }
            _ => false,
        }
    }

    #[inline]
    fn state(&self, slot: BalancedParkSlotId) -> Option<BalancedParkSlotState> {
        self.state(slot)
    }

    #[inline]
    fn slot_count(&self) -> usize {
        self.slot_count()
    }
}

impl BalancedParkSlots {
    #[inline]
    pub fn from_config(config: BalancedParkSlotsConfig) -> Self {
        let mut slots = Vec::with_capacity(config.slot_count);
        for _ in 0..config.slot_count {
            slots.push(BalancedParkSlot {
                generation: 0,
                state: BalancedParkSlotState::Vacant,
            });
        }
        Self {
            slots,
            next_probe: 0,
        }
    }

    #[inline]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    pub fn arm_next(&mut self, wake_token: BalancedWakeToken) -> Option<BalancedParkLease> {
        if self.slots.is_empty() {
            return None;
        }

        for step in 0..self.slots.len() {
            let idx = (self.next_probe + step) % self.slots.len();
            let slot = &mut self.slots[idx];
            if slot.state == BalancedParkSlotState::Vacant {
                slot.generation = slot.generation.wrapping_add(1).max(1);
                let lease = BalancedParkLease {
                    slot: BalancedParkSlotId(idx),
                    generation: slot.generation,
                    wake_token,
                };
                slot.state = BalancedParkSlotState::Armed {
                    generation: lease.generation,
                    wake_token,
                };
                self.next_probe = (idx + 1) % self.slots.len();
                return Some(lease);
            }
        }

        None
    }

    #[inline]
    pub fn wake(&mut self, lease: BalancedParkLease) -> bool {
        let Some(slot) = self.slots.get_mut(lease.slot.0) else {
            return false;
        };
        match slot.state {
            BalancedParkSlotState::Armed {
                generation,
                wake_token,
            } if generation == lease.generation && wake_token == lease.wake_token => {
                slot.state = BalancedParkSlotState::Woken {
                    generation,
                    wake_token,
                };
                true
            }
            _ => false,
        }
    }

    #[inline]
    pub fn cancel(&mut self, lease: BalancedParkLease) -> bool {
        let Some(slot) = self.slots.get_mut(lease.slot.0) else {
            return false;
        };
        match slot.state {
            BalancedParkSlotState::Armed {
                generation,
                wake_token,
            } if generation == lease.generation && wake_token == lease.wake_token => {
                slot.state = BalancedParkSlotState::Cancelled {
                    generation,
                    wake_token,
                };
                true
            }
            _ => false,
        }
    }

    #[inline]
    pub fn release(&mut self, lease: BalancedParkLease) -> bool {
        let Some(slot) = self.slots.get_mut(lease.slot.0) else {
            return false;
        };
        match slot.state {
            BalancedParkSlotState::Woken {
                generation,
                wake_token,
            }
            | BalancedParkSlotState::Cancelled {
                generation,
                wake_token,
            } if generation == lease.generation && wake_token == lease.wake_token => {
                slot.state = BalancedParkSlotState::Vacant;
                true
            }
            _ => false,
        }
    }

    #[inline]
    pub fn state(&self, slot: BalancedParkSlotId) -> Option<BalancedParkSlotState> {
        self.slots.get(slot.0).map(|slot| slot.state)
    }
}

impl ParkStoreOps for BalancedParkSlots {
    #[inline]
    fn arm_next(&mut self, wake_token: BalancedWakeToken) -> Option<BalancedParkLease> {
        self.arm_next(wake_token)
    }

    #[inline]
    fn wake(&mut self, lease: BalancedParkLease) -> bool {
        self.wake(lease)
    }

    #[inline]
    fn cancel(&mut self, lease: BalancedParkLease) -> bool {
        self.cancel(lease)
    }

    #[inline]
    fn release(&mut self, lease: BalancedParkLease) -> bool {
        self.release(lease)
    }

    #[inline]
    fn state(&self, slot: BalancedParkSlotId) -> Option<BalancedParkSlotState> {
        self.state(slot)
    }

    #[inline]
    fn slot_count(&self) -> usize {
        self.slot_count()
    }
}
