//! Tests for the OR-Set CRDT implementation.
//!
//! Verifies (per §8.1 of the protocol spec):
//! - Add and remove operations
//! - Concurrent add/remove semantics (add wins over concurrent remove)
//! - Merge of divergent replicas
//! - Idempotent merge operations

use openswarm_state::crdt::OrSet;

// ═══════════════════════════════════════════════════════════════
// § 8.1.2 OR-Set CRDT Operations
// ═══════════════════════════════════════════════════════════════

#[test]
fn orset_add_element() {
    let mut set = OrSet::<String>::new("node-1".into());
    set.add("task-1".to_string());
    assert!(set.contains("task-1"));
    assert!(!set.contains("task-2"));
}

#[test]
fn orset_remove_element() {
    let mut set = OrSet::<String>::new("node-1".into());
    set.add("task-1".to_string());
    assert!(set.contains("task-1"));
    set.remove("task-1");
    assert!(!set.contains("task-1"));
}

#[test]
fn orset_remove_nonexistent_is_noop() {
    let mut set = OrSet::<String>::new("node-1".into());
    set.remove("nonexistent");
    assert!(!set.contains("nonexistent"));
}

#[test]
fn orset_add_after_remove() {
    let mut set = OrSet::<String>::new("node-1".into());
    set.add("item".to_string());
    set.remove("item");
    set.add("item".to_string());
    assert!(
        set.contains("item"),
        "Add after remove must make element present"
    );
}

#[test]
fn orset_elements_returns_all() {
    let mut set = OrSet::<String>::new("node-1".into());
    set.add("a".to_string());
    set.add("b".to_string());
    set.add("c".to_string());
    let elements = set.elements();
    assert_eq!(elements.len(), 3);
    assert!(elements.contains(&"a".to_string()));
    assert!(elements.contains(&"b".to_string()));
    assert!(elements.contains(&"c".to_string()));
}

#[test]
fn orset_duplicate_add() {
    let mut set = OrSet::<String>::new("node-1".into());
    set.add("x".to_string());
    set.add("x".to_string());
    let elements = set.elements();
    assert_eq!(
        elements.len(),
        1,
        "Duplicate add should not create duplicates in elements view"
    );
}

// ═══════════════════════════════════════════════════════════════
// Merge Semantics
// ═══════════════════════════════════════════════════════════════

#[test]
fn orset_merge_union_of_adds() {
    let mut set_a = OrSet::<String>::new("node-a".into());
    let mut set_b = OrSet::<String>::new("node-b".into());

    set_a.add("task-1".to_string());
    set_b.add("task-2".to_string());

    set_a.merge(&set_b);
    assert!(set_a.contains("task-1"), "Local adds must be preserved");
    assert!(
        set_a.contains("task-2"),
        "Remote adds must be merged in"
    );
}

#[test]
fn orset_merge_concurrent_add_remove() {
    // Concurrent scenario: node-a adds "x", node-b removes "x" (was previously added)
    // Per OR-Set semantics: add wins over concurrent remove
    let mut set_a = OrSet::<String>::new("node-a".into());
    let mut set_b = OrSet::<String>::new("node-b".into());

    // Both start with "x"
    set_a.add("x".to_string());
    set_b.add("x".to_string());

    // Sync state
    set_a.merge(&set_b);
    set_b.merge(&set_a);

    // Now diverge: a re-adds, b removes
    set_a.add("x".to_string()); // new unique tag
    set_b.remove("x"); // removes all known tags

    // Merge: a's new add should win
    set_a.merge(&set_b);
    assert!(
        set_a.contains("x"),
        "Concurrent add must win over concurrent remove in OR-Set"
    );
}

#[test]
fn orset_merge_is_commutative() {
    let mut set_a = OrSet::<String>::new("node-a".into());
    let mut set_b = OrSet::<String>::new("node-b".into());

    set_a.add("1".to_string());
    set_a.add("2".to_string());
    set_b.add("3".to_string());
    set_b.add("4".to_string());

    let mut merged_ab = set_a.clone();
    merged_ab.merge(&set_b);

    let mut merged_ba = set_b.clone();
    merged_ba.merge(&set_a);

    assert_eq!(
        merged_ab.elements().len(),
        merged_ba.elements().len(),
        "Merge must be commutative"
    );
    for elem in merged_ab.elements() {
        assert!(
            merged_ba.contains(&elem),
            "Commutative merge: element {} missing",
            elem
        );
    }
}

#[test]
fn orset_merge_is_idempotent() {
    let mut set_a = OrSet::<String>::new("node-a".into());
    let mut set_b = OrSet::<String>::new("node-b".into());

    set_a.add("x".to_string());
    set_b.add("y".to_string());

    set_a.merge(&set_b);
    let elements_after_first = set_a.elements();

    set_a.merge(&set_b);
    let elements_after_second = set_a.elements();

    assert_eq!(
        elements_after_first.len(),
        elements_after_second.len(),
        "Merge must be idempotent"
    );
}

#[test]
fn orset_merge_is_associative() {
    let mut set_a = OrSet::<String>::new("a".into());
    let mut set_b = OrSet::<String>::new("b".into());
    let mut set_c = OrSet::<String>::new("c".into());

    set_a.add("1".to_string());
    set_b.add("2".to_string());
    set_c.add("3".to_string());

    // (A merge B) merge C
    let mut ab_c = set_a.clone();
    ab_c.merge(&set_b);
    ab_c.merge(&set_c);

    // A merge (B merge C)
    let mut a_bc = set_a.clone();
    let mut bc = set_b.clone();
    bc.merge(&set_c);
    a_bc.merge(&bc);

    assert_eq!(ab_c.elements().len(), a_bc.elements().len());
    for elem in ab_c.elements() {
        assert!(a_bc.contains(&elem));
    }
}

// ═══════════════════════════════════════════════════════════════
// Empty sets
// ═══════════════════════════════════════════════════════════════

#[test]
fn orset_empty_has_no_elements() {
    let set = OrSet::<String>::new("node".into());
    assert!(set.elements().is_empty());
}

#[test]
fn orset_merge_empty_sets() {
    let mut set_a = OrSet::<String>::new("a".into());
    let set_b = OrSet::<String>::new("b".into());
    set_a.merge(&set_b);
    assert!(set_a.elements().is_empty());
}
