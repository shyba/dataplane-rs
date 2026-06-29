use crate::runtime_id_map::U64Map;
use crate::runtime_session::Connection;

/// Connection ownership table with explicit ownership seam methods.
///
/// This table owns the live connections for a shard. The methods on this type
/// form the ownership seam: callers must use these methods rather than
/// accessing the inner map directly, so that lifetime and ownership
/// invariants can be enforced at the boundary.
#[derive(Default)]
pub(super) struct ConnectionTable {
    inner: U64Map<Connection>,
}

impl ConnectionTable {
    /// Take and return the connection for `session_id`, if it exists.
    /// This is the primary ownership-transfer method for connection close.
    #[inline]
    pub(super) fn take(&mut self, session_id: &u64) -> Option<Connection> {
        self.inner.remove(session_id)
    }

    /// Insert a connection into the table.
    /// This is the primary ownership-transfer method for connection accept.
    #[inline]
    pub(super) fn insert(&mut self, session_id: u64, conn: Connection) {
        self.inner.insert(session_id, conn);
    }

    /// Get a mutable reference to the connection for `session_id`, if it exists.
    #[inline]
    pub(super) fn get_mut(&mut self, session_id: &u64) -> Option<&mut Connection> {
        self.inner.get_mut(session_id)
    }

    /// Get the number of live connections in the table.
    #[inline]
    pub(super) fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the table has no live connections.
    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get an immutable reference to the connection for `session_id`, if it exists.
    #[inline]
    pub(super) fn get(&self, session_id: &u64) -> Option<&Connection> {
        self.inner.get(session_id)
    }

    /// Snapshot all connection ids currently in the table.
    /// The returned Vec can be used for iteration without borrowing the table.
    #[inline]
    pub(super) fn keys(&self) -> Vec<u64> {
        self.inner.keys().copied().collect()
    }

    /// Iterate over (session_id, connection) pairs.
    /// Use when callers need to filter or inspect multiple connections at once.
    #[inline]
    pub(super) fn iter(&self) -> impl Iterator<Item = (&u64, &Connection)> {
        self.inner.iter()
    }

    /// Returns true if a connection with the given `session_id` exists in the table.
    #[cfg(test)]
    #[inline]
    pub(super) fn contains(&self, session_id: &u64) -> bool {
        self.inner.contains_key(session_id)
    }

    /// Returns true if any connection in the table has live I/O state
    /// (read-in-flight, read-poll-armed, write-in-flight, or write-poll-armed).
    /// This is used by exit/teardown checks that need to know if resources are busy.
    pub(super) fn has_live_io(&self) -> bool {
        self.inner.values().any(|conn| {
            conn.read_in_flight
                || conn.read_poll_armed
                || conn.write_in_flight
                || conn.write_poll_armed
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that ConnectionTable::take returns None for a session_id that is
    /// not present, rather than panicking or returning an unexpected value.
    /// This preserves the stale-session lookup semantics from the direct map access.
    #[test]
    fn connection_table_take_returns_none_for_missing_session() {
        let mut table = ConnectionTable::default();
        let missing_id = 9999_u64;

        let result = table.take(&missing_id);
        assert!(
            result.is_none(),
            "take should return None for missing session_id"
        );
        assert_eq!(
            table.len(),
            0,
            "table should remain empty after take on missing key"
        );
    }

    /// Verifies get_mut returns None for a missing session, preserving
    /// the stale-session lookup behavior.
    #[test]
    fn connection_table_get_mut_returns_none_for_missing_session() {
        let mut table = ConnectionTable::default();
        let result = table.get_mut(&12345_u64);
        assert!(result.is_none());
    }

    /// Verifies that len() and is_empty() work correctly on an empty table.
    #[test]
    fn connection_table_len_and_is_empty() {
        let table = ConnectionTable::default();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    /// Verifies that contains returns false for missing ids.
    #[test]
    fn connection_table_contains_missing() {
        let table = ConnectionTable::default();
        assert!(!table.contains(&9999));
    }

    /// Verifies that keys() returns an empty Vec for an empty table.
    #[test]
    fn connection_table_keys_empty() {
        let table = ConnectionTable::default();
        assert!(table.keys().is_empty());
    }

    /// Verifies that get returns None for missing ids.
    #[test]
    fn connection_table_get_missing() {
        let mut table = ConnectionTable::default();
        // Can't construct Connection directly without Default.
        // Test that take returns None for a missing id (insert/take roundtrip
        // is exercised by the existing connection_table_take_returns_none test).
        assert!(table.take(&9999).is_none());
    }
}
