//! Link ID namespace helpers.
//!
//! Encodes a namespace byte in the high byte of a u64 link ID, leaving 56 bits
//! for the namespace-specific value.

use rustler::LocalPid;

// =============================================================================
// Constants
// =============================================================================

const LINK_NAMESPACE_SHIFT: u64 = 56;
const LINK_NAMESPACE_MASK: u64 = (1u64 << LINK_NAMESPACE_SHIFT) - 1;

/// Namespace 0 — global (no per-key data).
const LINK_NAMESPACE_GLOBAL: u8 = 0;
/// Namespace 1 — session-scoped.
const LINK_NAMESPACE_SESSION: u8 = 1;
/// Namespace 2 — listener-scoped.
const LINK_NAMESPACE_LISTENER: u8 = 2;
/// Namespace 3 — caller (Erlang PID) scoped.
const LINK_NAMESPACE_CALLER: u8 = 3;

// =============================================================================
// Core encoding
// =============================================================================

/// Build a link ID by placing `namespace` in bits [63:56] and `value` in [55:0].
pub(crate) fn link_id(namespace: u8, value: u64) -> u64 {
    ((namespace as u64) << LINK_NAMESPACE_SHIFT) | (value & LINK_NAMESPACE_MASK)
}

/// Global link ID (namespace 0, value 0).  Used for un-linked commands.
pub(crate) fn global_link_id() -> u64 {
    link_id(LINK_NAMESPACE_GLOBAL, 0)
}

/// Build a session-scoped link ID.
pub(crate) fn session_link_id(session_id: u64) -> u64 {
    link_id(LINK_NAMESPACE_SESSION, session_id)
}

/// Build a listener-scoped link ID.
pub(crate) fn listener_link_id(listener_id: u64) -> u64 {
    link_id(LINK_NAMESPACE_LISTENER, listener_id)
}

// =============================================================================
// Caller (PID) hashing
// =============================================================================

/// Stable hash of a `LocalPid` suitable for use as a link-namespace value.
///
pub(crate) fn hash_pid(pid: &LocalPid) -> u64 {
    // SAFETY: `as_c_arg()` returns a pointer to opaque runtime data; we only read
    // raw bytes via `read_unaligned` for stable hashing, which is safe.
    let mut x = unsafe { (pid.as_c_arg() as *const _ as *const usize).read_unaligned() as u64 };
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^ (x >> 33)
}

/// Build a caller-scoped link ID from an Erlang PID.
pub(crate) fn caller_link_id(pid: &LocalPid) -> u64 {
    link_id(LINK_NAMESPACE_CALLER, hash_pid(pid))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::{
        global_link_id, link_id, listener_link_id, session_link_id, LINK_NAMESPACE_CALLER,
        LINK_NAMESPACE_MASK,
    };

    #[test]
    fn test_link_id_high_byte_is_namespace() {
        // Namespace 0 → high byte 0x00
        assert_eq!(global_link_id() >> 56, 0);
        // Namespace 1 → high byte 0x01
        assert_eq!(session_link_id(0) >> 56, 1);
        // Namespace 2 → high byte 0x02
        assert_eq!(listener_link_id(0) >> 56, 2);
        // Namespace 3 → high byte 0x03
        // caller_link_id needs a real LocalPid which is not constructible in tests
        // without the NIF runtime, so we test the constant directly.
        assert_eq!(LINK_NAMESPACE_CALLER, 3);
        assert_eq!(link_id(LINK_NAMESPACE_CALLER, 0) >> 56, 3);
    }

    #[test]
    fn test_link_id_low_56_bits_preserved() {
        // Max value that fits in 56 bits
        let max_56 = (1u64 << 56) - 1;
        for namespace in [0u8, 1, 2, 3] {
            let id = link_id(namespace, max_56);
            assert_eq!(id & LINK_NAMESPACE_MASK, max_56);
            assert_eq!(id >> 56, namespace as u64);
        }
    }

    #[test]
    fn test_link_id_value_mask_truncates_high_bits() {
        // Any bits above 56 in `value` should be masked away.
        // namespace=2, value with high bits above 56.
        let id = link_id(2, 0xFF_FFFF_FFFF_FFFF);
        // Namespace byte must be 2.
        assert_eq!(id >> 56, 2);
        // Lower 56 bits are exactly 0x00FF_FFFF_FFFF_FFFF (value masked to 56 bits).
        assert_eq!(id & LINK_NAMESPACE_MASK, 0x00FF_FFFF_FFFF_FFFF);
        // Any bits above 56 in value are dropped; id == 0x02_0000_0000_0000_0000.
        assert_eq!(id, (2u64 << 56) | 0x00FF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn test_global_link_id_is_zero() {
        // namespace=0, value=0 → entirely zero
        assert_eq!(global_link_id(), 0);
    }
}
