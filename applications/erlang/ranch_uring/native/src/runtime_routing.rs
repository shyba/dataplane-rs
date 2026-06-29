//! Shard routing helpers.
//!
//! Determines which shard handles a given file descriptor or subscribe operation.

use super::{NifError, Ordering, Result, SubscribeOperation, NEXT_SUBSCRIBE_SHARD};
use std::os::fd::RawFd;

// =============================================================================
// fd → shard
// =============================================================================

/// Choose a shard by wrapping the file descriptor number.
///
/// # Panics
///
/// `shard_count` must be > 0; the function uses `max(1)` internally so it
/// never panics but will always return 0 when `shard_count == 0`.
pub(crate) fn shard_for_fd(fd: RawFd, shard_count: usize) -> usize {
    (fd as usize) % shard_count.max(1)
}

/// Choose a shard for a subscription operation.
///
/// - `Accept` operations are round-robined across shards using an atomic
///   counter to spread listener accept load evenly.
/// - `Read` operations use `shard_for_fd` so that the same socket always
///   lands on the same shard (preserving in-flight operation ordering).
pub(crate) fn shard_for_subscribe(
    operation: SubscribeOperation,
    fd: RawFd,
    shard_count: usize,
) -> usize {
    if shard_count <= 1 {
        return 0;
    }
    match operation {
        SubscribeOperation::Accept => {
            (NEXT_SUBSCRIBE_SHARD.fetch_add(1, Ordering::Relaxed) as usize) % shard_count
        }
        SubscribeOperation::Read => shard_for_fd(fd, shard_count),
    }
}

/// Decode the encoded shard number from a routed ID.
///
/// The shard is encoded as `namespace byte = shard + 1` (so that 0 can be
/// used as a sentinel).  Decoding reverses that encoding.
///
/// # Errors
///
/// Returns `EINVAL` when the encoded namespace byte is 0 (sentinel) or
/// larger than `shard_count - 1`.
pub(crate) fn shard_from_routed_id(id: u64, shard_count: usize) -> Result<usize> {
    let encoded = ((id >> 56) & 0xff) as usize;
    if encoded == 0 {
        return Err(NifError::from_errno(libc::EINVAL));
    }
    let shard = encoded - 1;
    if shard >= shard_count {
        return Err(NifError::from_errno(libc::EINVAL));
    }
    Ok(shard)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::{shard_for_fd, shard_from_routed_id};

    // -------------------------------------------------------------------------
    // shard_for_fd
    // -------------------------------------------------------------------------

    #[test]
    fn test_shard_for_fd_modulo_is_stable() {
        // Same fd always returns the same shard
        let fd = 42;
        assert_eq!(shard_for_fd(fd, 4), shard_for_fd(fd, 4));
        assert_eq!(shard_for_fd(fd, 4), 42 % 4);
    }

    #[test]
    fn test_shard_for_fd_zero_shard_count_returns_zero() {
        // Guard against division by zero — must not panic
        assert_eq!(shard_for_fd(99, 0), 0);
        assert_eq!(shard_for_fd(0, 0), 0);
    }

    #[test]
    fn test_shard_for_fd_one_shard_returns_zero() {
        assert_eq!(shard_for_fd(0, 1), 0);
        assert_eq!(shard_for_fd(99, 1), 0);
    }

    #[test]
    fn test_shard_for_fd_covers_all_shards() {
        let count = 8;
        // Collect all possible results; with modulo they must all be < count
        for fd in 0..100 {
            let shard = shard_for_fd(fd, count);
            assert!(shard < count, "fd={fd} returned shard {shard} >= {count}");
        }
    }

    // -------------------------------------------------------------------------
    // shard_from_routed_id
    // -------------------------------------------------------------------------

    #[test]
    fn test_shard_from_routed_id_zero_namespace_is_error() {
        // id with high byte 0 should always fail
        let result = shard_from_routed_id(0, 4);
        assert!(result.is_err());
        // Any id with 0 in high byte
        let result = shard_from_routed_id(0x00_0000_0000_0001, 4);
        assert!(result.is_err());
    }

    #[test]
    fn test_shard_from_routed_id_oob_shard_is_error() {
        // Encoded shard 5 with only 4 shards available
        let id = (5 + 1) << 56; // encoded = 5, shard = 4
        let result = shard_from_routed_id(id, 4);
        assert!(result.is_err());
    }

    #[test]
    fn test_shard_from_routed_id_valid() {
        // Encoded = 1 → shard 0
        assert_eq!(shard_from_routed_id(1 << 56, 4).unwrap(), 0);
        // Encoded = 2 → shard 1
        assert_eq!(shard_from_routed_id(2 << 56, 4).unwrap(), 1);
        // Encoded = 3 → shard 2
        assert_eq!(shard_from_routed_id(3 << 56, 4).unwrap(), 2);
        // Encoded = 4 → shard 3
        assert_eq!(shard_from_routed_id(4 << 56, 4).unwrap(), 3);
    }

    #[test]
    fn test_shard_from_routed_id_value_bits_below_56_are_ignored() {
        // The value portion does not affect shard selection
        let base = 3u64 << 56; // encoded=3 → shard 2
        assert_eq!(shard_from_routed_id(base, 4).unwrap(), 2);
        assert_eq!(shard_from_routed_id(base | 0xFFFF_FFFF_FFFF, 4).unwrap(), 2);
    }
}
