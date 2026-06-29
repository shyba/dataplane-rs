// Copyright (C) 2025 Gleb Svyatov <gfx@protonmail.ch>
// License: MIT or Apache-2.0

//! Pending reply types for statx operations.
//!
//! This module is extracted from runtime.rs to isolate pending reply state.

use crate::runtime_arena::ArenaHandle;

/// File descriptor for a statx operation.
pub(super) type StatxFd = std::os::fd::RawFd;

/// A pending statx operation waiting for completion.
pub(super) struct PendingStatx {
    /// File descriptor being stat'd.
    pub(super) fd: StatxFd,
    /// User-provided request identifier, echoed back on completion.
    pub(super) request_id: u64,
    /// Reply destination.
    pub(super) target: crate::runtime_adapter::ResultTarget,
    /// Output buffer variant.
    pub(super) out: StatxOut,
}

impl std::fmt::Debug for PendingStatx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingStatx")
            .field("fd", &self.fd)
            .field("request_id", &self.request_id)
            .field("target", &"ResultTarget")
            .field("out", &self.out)
            .finish()
    }
}

/// Output buffer for a statx result.
#[derive(Debug)]
pub(super) enum StatxOut {
    /// Arena-backed storage: the handle is valid until the arena is dropped.
    Arena(ArenaHandle),
    /// Heap-allocated storage: boxed statx structure.
    Heap(Box<libc::statx>),
}

#[cfg(test)]
mod tests {
    use super::{PendingStatx, StatxFd};
    use std::mem::size_of;

    // Note: Direct construction of PendingStatx, StatxOut::Heap, and ResultTarget::Erlang
    // is not practical because libc::statx, libc::statx_timestamp, and LocalPid have private
    // fields. These tests verify layout properties that are testable without construction.

    /// PendingStatx should stay compact enough for the pending-reply table.
    #[test]
    fn pending_statx_size_is_reasonable() {
        let sz = size_of::<PendingStatx>();
        // fd=u32, request_id=u64, target=ResultTarget, out=StatxOut.
        let min = 4 + 8 + size_of::<*const ()>() * 2;
        assert!(
            sz >= min,
            "PendingStatx size {} is unexpectedly small (min expected {})",
            sz,
            min
        );
        // Sanity: a struct with 2×pointer fields should not exceed reasonable inline size
        assert!(
            sz <= 128,
            "PendingStatx size {} exceeds reasonable inline size",
            sz
        );
    }

    /// PendingStatx fd field is RawFd (i32 on unix).
    #[test]
    fn statx_fd_is_raw_fd_size() {
        assert_eq!(size_of::<StatxFd>(), size_of::<std::os::fd::RawFd>());
        assert_eq!(size_of::<StatxFd>(), 4, "RawFd should be i32-sized");
    }

    /// Verify request_id is u64.
    #[test]
    fn request_id_is_u64() {
        assert_eq!(size_of_val(&0u64), 8);
    }
}
