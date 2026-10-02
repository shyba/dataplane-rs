//! Registration layout and planning for fixed buffer registration with io_uring.
//!
//! This module extracts the registration planning logic from `runtime.rs` including:
//! - [`RegistrationLayout`]: Layout of registered fixed buffers for read and subscribe rings.
//! - [`FixedRegistrationPlan`]: Planned configuration for fixed registration.
//! - [`plan_fixed_registration()`]: Calculate locked read buffers and subscribe arena registration.
//! - [`build_registration_layout()`]: Build the full registration layout from pools and arenas.

use crate::runtime_arena::RuntimeArenas;
use crate::runtime_reactor::{BufSlotId, LockedReadBufPool, BUF_SIZE, SUBSCRIBE_PAGE_SIZE};

/// Layout of registered fixed buffers for a runtime shard.
///
/// Organizes registered buffers into two segments:
/// - Read fixed buffers (prefix): contiguous range at fixed buffer index zero.
/// - Subscribe fixed buffers (suffix): optional arena-backed buffers following read buffers.
#[derive(Clone, Debug)]
pub(crate) struct RegistrationLayout {
    /// Iov array for all registered descriptors.
    pub(super) descriptors: Vec<libc::iovec>,
    /// Slot IDs for registered read buffers in stable order.
    pub(super) read_fixed_slots: Vec<BufSlotId>,
    /// Fixed buffer index for first read buffer (always 0).
    pub(super) read_fixed_base: u16,
    /// Number of read fixed descriptors.
    pub(super) read_fixed_len: usize,
    /// Fixed buffer index for first subscribe buffer, if registered.
    pub(super) subscribe_fixed_base: Option<u16>,
    /// Number of subscribe fixed descriptors.
    pub(super) subscribe_fixed_len: usize,
    /// Maps read buffer slot index to registered fixed buffer index.
    pub(super) read_fixed_slot_table: Vec<Option<u16>>,
    /// Maps subscribe slot index to its suffix-relative fixed buffer index.
    pub(super) subscribe_fixed_slot_table: Vec<Option<u16>>,
}

/// Planned configuration for fixed buffer registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FixedRegistrationPlan {
    /// Number of locked read buffers to register.
    pub(super) locked_read_bufs: usize,
    /// Whether to register the subscribe arena.
    pub(super) register_subscribe_arena: bool,
}

/// Calculate the fixed registration plan based on resource limits.
///
/// Determines how many read buffers to lock and whether the subscribe arena
/// can be registered given the memlock limit and shard count.
pub(crate) fn plan_fixed_registration(
    requested_locked_read_bufs: usize,
    shard_count: usize,
    memlock_limit: usize,
    register_subscribe_arena: bool,
    subscribe_slots: usize,
) -> FixedRegistrationPlan {
    let per_shard_budget = memlock_limit / shard_count.max(1);
    let locked_read_bufs = if requested_locked_read_bufs == 0 || memlock_limit == usize::MAX {
        requested_locked_read_bufs
    } else {
        let max_locked_per_shard = per_shard_budget / BUF_SIZE;
        requested_locked_read_bufs.min(max_locked_per_shard)
    };

    if !register_subscribe_arena || memlock_limit == usize::MAX {
        return FixedRegistrationPlan {
            locked_read_bufs,
            register_subscribe_arena,
        };
    }

    let read_registration_bytes = locked_read_bufs.saturating_mul(BUF_SIZE);
    let subscribe_registration_bytes = subscribe_slots.saturating_mul(SUBSCRIBE_PAGE_SIZE);
    let register_subscribe_arena =
        read_registration_bytes.saturating_add(subscribe_registration_bytes) <= per_shard_budget;

    FixedRegistrationPlan {
        locked_read_bufs,
        register_subscribe_arena,
    }
}

/// Build the complete registration layout from read buffer pool and arenas.
///
/// Combines the read fixed buffers and optional subscribe arena into a single
/// contiguous descriptor array, populating slot tables for efficient lookup.
pub(crate) fn build_registration_layout(
    read_pool: &LockedReadBufPool,
    arenas: &RuntimeArenas,
    register_subscribe_arena: bool,
) -> RegistrationLayout {
    let read_fixed_base = 0u16;
    let read_descriptors = read_pool.registered_iovecs_stable_with_ids();
    let read_fixed_slots = read_descriptors
        .iter()
        .map(|(slot_id, _)| *slot_id)
        .collect::<Vec<_>>();
    let read_fixed_slot_table_len = read_fixed_slots
        .iter()
        .map(|slot_id| slot_id.index as usize)
        .max()
        .map(|max_index| max_index + 1)
        .unwrap_or(0);
    let mut read_fixed_slot_table = vec![None; read_fixed_slot_table_len];
    for (buffer_index, slot_id) in read_fixed_slots.iter().enumerate() {
        let Some(registered_index) = u16::try_from(buffer_index).ok() else {
            continue;
        };
        if let Some(entry) = read_fixed_slot_table.get_mut(slot_id.index as usize) {
            *entry = Some(registered_index);
        }
    }
    let mut descriptors = read_descriptors
        .into_iter()
        .map(|(_, iovec)| iovec)
        .collect::<Vec<_>>();
    let read_fixed_len = descriptors.len();
    let subscribe_fixed_len = if register_subscribe_arena {
        arenas.small_len()
    } else {
        0
    };
    let mut subscribe_fixed_slot_table = vec![None; subscribe_fixed_len];
    if register_subscribe_arena {
        for page_slot in 0..subscribe_fixed_len {
            let Some(registered_index) = u16::try_from(page_slot).ok() else {
                continue;
            };
            if let Some(entry) = subscribe_fixed_slot_table.get_mut(page_slot) {
                *entry = Some(registered_index);
            }
        }
    }
    let subscribe_fixed_base = if register_subscribe_arena
        && descriptors.len() + subscribe_fixed_len <= u16::MAX as usize
    {
        Some(descriptors.len() as u16)
    } else {
        None
    };
    if register_subscribe_arena {
        descriptors.extend(arenas.small_registered_iovecs());
    }
    RegistrationLayout {
        descriptors,
        read_fixed_slots,
        read_fixed_base,
        read_fixed_len,
        subscribe_fixed_base,
        subscribe_fixed_len,
        read_fixed_slot_table,
        subscribe_fixed_slot_table,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_registration_shares_the_process_budget_across_shards() {
        let plan = plan_fixed_registration(1, 2, 2 * BUF_SIZE, true, 1);
        assert_eq!(plan.locked_read_bufs, 1);
        assert!(!plan.register_subscribe_arena);
        let plan = plan_fixed_registration(1, 2, 2 * (BUF_SIZE + SUBSCRIBE_PAGE_SIZE), true, 1);
        assert!(plan.register_subscribe_arena);
    }
}
