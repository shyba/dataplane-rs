use crate::runtime::{ListenerShard, Subscription};
use crate::runtime_id_map::U64Map;

/// Listener ownership table with explicit ownership seam methods.
///
/// This table owns the live listeners for a shard. The methods on this type
/// form the ownership seam: callers must use these methods rather than
/// accessing the inner map directly, so that lifetime and ownership
/// invariants are enforced at the boundary.
#[derive(Default)]
pub(super) struct ListenerTable {
    inner: U64Map<ListenerShard>,
}

impl ListenerTable {
    /// Insert a listener into the table.
    #[inline]
    pub(super) fn insert(&mut self, listener_id: u64, listener: ListenerShard) {
        self.inner.insert(listener_id, listener);
    }

    /// Remove and return the listener for `listener_id`, if it exists.
    #[inline]
    pub(super) fn remove(&mut self, listener_id: &u64) -> Option<ListenerShard> {
        self.inner.remove(listener_id)
    }

    /// Get a mutable reference to the listener for `listener_id`, if it exists.
    #[inline]
    pub(super) fn get_mut(&mut self, listener_id: &u64) -> Option<&mut ListenerShard> {
        self.inner.get_mut(listener_id)
    }

    /// Get an immutable reference to the listener for `listener_id`, if it exists.
    #[inline]
    pub(super) fn get(&self, listener_id: &u64) -> Option<&ListenerShard> {
        self.inner.get(listener_id)
    }

    /// Get the number of live listeners in the table.
    #[inline]
    pub(super) fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the table has no live listeners.
    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Snapshot all listener ids currently in the table.
    #[inline]
    pub(super) fn keys(&self) -> Vec<u64> {
        self.inner.keys().copied().collect()
    }

    /// Returns true if a listener with the given `listener_id` exists.
    #[cfg(test)]
    #[inline]
    pub(super) fn contains(&self, listener_id: &u64) -> bool {
        self.inner.contains_key(listener_id)
    }
}

/// Subscription ownership table with explicit ownership seam methods.
///
/// This table owns the live subscriptions for a shard. The methods on this type
/// form the ownership seam: callers must use these methods rather than
/// accessing the inner map directly, so that lifetime and ownership
/// invariants are enforced at the boundary.
#[derive(Default)]
pub(super) struct SubscriptionTable {
    inner: U64Map<Subscription>,
}

impl SubscriptionTable {
    /// Insert a subscription into the table.
    #[inline]
    pub(super) fn insert(&mut self, subscription_id: u64, subscription: Subscription) {
        self.inner.insert(subscription_id, subscription);
    }

    /// Remove and return the subscription for `subscription_id`, if it exists.
    #[inline]
    pub(super) fn remove(&mut self, subscription_id: &u64) -> Option<Subscription> {
        self.inner.remove(subscription_id)
    }

    /// Get a mutable reference to the subscription for `subscription_id`, if it exists.
    #[inline]
    pub(super) fn get_mut(&mut self, subscription_id: &u64) -> Option<&mut Subscription> {
        self.inner.get_mut(subscription_id)
    }

    /// Get an immutable reference to the subscription for `subscription_id`, if it exists.
    #[inline]
    pub(super) fn get(&self, subscription_id: &u64) -> Option<&Subscription> {
        self.inner.get(subscription_id)
    }

    /// Get the number of live subscriptions in the table.
    #[inline]
    pub(super) fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the table has no live subscriptions.
    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Snapshot all subscription ids currently in the table.
    #[inline]
    pub(super) fn keys(&self) -> Vec<u64> {
        self.inner.keys().copied().collect()
    }

    /// Returns true if a subscription with the given `subscription_id` exists.
    #[inline]
    pub(super) fn contains(&self, subscription_id: &u64) -> bool {
        self.inner.contains_key(subscription_id)
    }

    /// Remove all subscriptions from the table.
    #[cfg(test)]
    #[inline]
    pub(super) fn clear(&mut self) {
        self.inner.clear()
    }
}

#[cfg(test)]
mod listener_table_tests {
    use super::*;

    #[test]
    fn listener_table_default_is_empty() {
        let mut t: ListenerTable = ListenerTable::default();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert!(t.keys().is_empty());
        assert!(t.get(&1).is_none());
        assert!(t.get_mut(&1).is_none());
        assert!(t.remove(&1).is_none());
        assert!(!t.contains(&1));
    }
}

#[cfg(test)]
mod subscription_table_tests {
    use super::*;

    #[test]
    fn subscription_table_default_is_empty() {
        let mut t: SubscriptionTable = SubscriptionTable::default();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert!(t.keys().is_empty());
        assert!(t.get(&1).is_none());
        assert!(t.get_mut(&1).is_none());
        assert!(t.remove(&1).is_none());
        assert!(!t.contains(&1));
    }
}
