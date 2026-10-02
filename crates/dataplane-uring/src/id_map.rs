//! Fast u64-keyed hash map types for runtime shard tables.
//!
//! Provides `U64Hasher` — a `BuildHasherDefault` hasher for u64 keys that avoids
//! the overhead of the default SipHash for the common case of uniform u64 keys —
//! and `U64Map<V>` as a type alias for `HashMap<u64, V, BuildHasherDefault<U64Hasher>>`.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Fast hasher for u64 keys, used as the build-hasher for runtime shard maps.
///
/// This avoids the default SipHash overhead for the typical case where keys are
/// uniformly distributed u64 values. This is an identity hash, not HashDoS
/// protection: use only runtime-generated ids, never attacker-chosen keys.
#[derive(Default)]
pub struct U64Hasher(u64);

impl Hasher for U64Hasher {
    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut acc = self.0;
        for &b in bytes {
            acc = acc.rotate_left(5) ^ (b as u64);
        }
        self.0 = acc;
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

/// Alias for `HashMap<u64, V, BuildHasherDefault<U64Hasher>>` — the standard map
/// type used throughout the runtime for session, listener, and subscription ids.
pub type U64Map<V> = HashMap<u64, V, BuildHasherDefault<U64Hasher>>;
