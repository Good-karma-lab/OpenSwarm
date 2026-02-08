//! OR-Set CRDT (Observed-Remove Set) for hot state management.
//!
//! The OR-Set is a conflict-free replicated data type that supports both
//! add and remove operations without coordination. It is used for
//! maintaining shared mutable state across the swarm:
//! - Task status tracking
//! - Active agent lists per tier/branch
//! - Proposal and vote tracking
//!
//! Each element is tagged with a unique identifier when added. Remove
//! operations only affect currently observed tags, allowing concurrent
//! adds and removes to be merged without conflict.
//!
//! **Add-wins semantics**: when one node adds an element concurrently
//! with another node removing it, the add wins because the new unique
//! tag is not present in the remote tombstone set.

use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::hash::Hash;

/// A globally unique tag identifying a specific add operation.
///
/// Each tag is a pair of (node_id, counter) which is guaranteed
/// to be unique across the entire swarm as long as node IDs are unique
/// and counters are monotonically increasing per node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UniqueTag {
    pub node_id: String,
    pub counter: u64,
}

/// OR-Set CRDT (Observed-Remove Set).
///
/// Supports concurrent add and remove operations across distributed
/// nodes with guaranteed convergence.
///
/// # Type Parameters
/// - `T`: The element type. Must be `Clone + Eq + Hash + Display`.
///
/// # Semantics
/// - Each `add` creates a fresh `UniqueTag` for the value.
/// - `remove` moves all currently-known tags for a value into
///   the tombstone set.
/// - An element is *present* if it has at least one tag that is
///   **not** in the tombstone set.
/// - On merge, both entries and tombstones are unioned. Because a
///   concurrent add creates a tag the remote side has never seen,
///   that tag survives the merge (add wins).
#[derive(Debug)]
pub struct OrSet<T: Clone + Eq + Hash + Display> {
    /// This node's unique identifier.
    node_id: String,
    /// Map from values to the set of unique tags currently associated.
    entries: HashMap<T, HashSet<UniqueTag>>,
    /// Set of tombstoned (removed) tags.
    tombstones: HashSet<UniqueTag>,
    /// Monotonically increasing counter for generating unique tags.
    counter: u64,
}

impl<T: Clone + Eq + Hash + Display> Clone for OrSet<T> {
    fn clone(&self) -> Self {
        Self {
            node_id: self.node_id.clone(),
            entries: self.entries.clone(),
            tombstones: self.tombstones.clone(),
            counter: self.counter,
        }
    }
}

impl<T: Clone + Eq + Hash + Display> OrSet<T> {
    /// Create a new empty OR-Set for the given node.
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            entries: HashMap::new(),
            tombstones: HashSet::new(),
            counter: 0,
        }
    }

    /// Add a value to the set.
    ///
    /// Each add generates a fresh unique tag, ensuring that concurrent
    /// adds are never lost even if a concurrent remove is in flight.
    pub fn add(&mut self, value: T) {
        self.counter += 1;
        let tag = UniqueTag {
            node_id: self.node_id.clone(),
            counter: self.counter,
        };
        self.entries
            .entry(value)
            .or_insert_with(HashSet::new)
            .insert(tag);
    }

    /// Remove a value from the set.
    ///
    /// All currently observed tags for the value are moved into the
    /// tombstone set. If a concurrent add on another node created a
    /// tag we have not yet seen, that tag will survive the merge
    /// (add-wins semantics).
    pub fn remove<Q: ?Sized>(&mut self, value: &Q)
    where
        T: std::borrow::Borrow<Q>,
        Q: Hash + Eq,
    {
        if let Some(tags) = self.entries.get(value) {
            for tag in tags.iter() {
                self.tombstones.insert(tag.clone());
            }
        }
    }

    /// Check whether a value is present in the set.
    ///
    /// A value is present if it has at least one tag that has not
    /// been tombstoned.
    pub fn contains<Q: ?Sized>(&self, value: &Q) -> bool
    where
        T: std::borrow::Borrow<Q>,
        Q: Hash + Eq,
    {
        if let Some(tags) = self.entries.get(value) {
            tags.iter().any(|tag| !self.tombstones.contains(tag))
        } else {
            false
        }
    }

    /// Return all currently present elements (owned).
    ///
    /// An element is present if it has at least one non-tombstoned tag.
    pub fn elements(&self) -> Vec<T> {
        self.entries
            .iter()
            .filter(|(_, tags)| tags.iter().any(|tag| !self.tombstones.contains(tag)))
            .map(|(value, _)| value.clone())
            .collect()
    }

    /// Get the number of present elements in the set.
    pub fn len(&self) -> usize {
        self.entries
            .iter()
            .filter(|(_, tags)| tags.iter().any(|tag| !self.tombstones.contains(tag)))
            .count()
    }

    /// Check if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Merge another OR-Set replica into this one.
    ///
    /// The merge is commutative, associative, and idempotent (CRDT properties).
    ///
    /// Merge rules:
    /// - Entries are unioned: all tags from the remote set are added.
    /// - Tombstones are unioned: all tombstones from the remote set are added.
    ///
    /// After merge, a value is present if it has at least one tag that
    /// is not in the combined tombstone set. This gives add-wins semantics
    /// because a new add creates a fresh tag the remote tombstone does not cover.
    pub fn merge(&mut self, other: &OrSet<T>) {
        // Union of entries.
        for (value, tags) in &other.entries {
            let entry = self.entries.entry(value.clone()).or_insert_with(HashSet::new);
            for tag in tags {
                entry.insert(tag.clone());
            }
        }

        // Union of tombstones.
        for tag in &other.tombstones {
            self.tombstones.insert(tag.clone());
        }
    }

    /// Get the node ID of this replica.
    pub fn node_id(&self) -> &str {
        &self.node_id
    }
}

/// Convenience type for tracking task statuses across the swarm.
pub type TaskStatusSet = OrSet<String>;

/// Convenience type for tracking active agents.
pub type AgentSet = OrSet<String>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_contains() {
        let mut set = OrSet::new("n1".into());
        set.add("hello".to_string());
        assert!(set.contains("hello"));
        assert!(!set.contains("world"));
    }

    #[test]
    fn test_remove() {
        let mut set = OrSet::new("n1".into());
        set.add("x".to_string());
        set.remove("x");
        assert!(!set.contains("x"));
    }

    #[test]
    fn test_add_after_remove() {
        let mut set = OrSet::new("n1".into());
        set.add("x".to_string());
        set.remove("x");
        set.add("x".to_string());
        assert!(set.contains("x"), "Re-add must restore element");
    }

    #[test]
    fn test_merge_basic() {
        let mut a = OrSet::new("a".into());
        let mut b = OrSet::new("b".into());
        a.add("1".to_string());
        b.add("2".to_string());
        a.merge(&b);
        assert!(a.contains("1"));
        assert!(a.contains("2"));
    }

    #[test]
    fn test_concurrent_add_wins() {
        let mut a = OrSet::new("a".into());
        let mut b = OrSet::new("b".into());

        a.add("x".to_string());
        b.add("x".to_string());

        a.merge(&b);
        b.merge(&a);

        a.add("x".to_string());
        b.remove("x");

        a.merge(&b);
        assert!(a.contains("x"), "Concurrent add must win");
    }
}
