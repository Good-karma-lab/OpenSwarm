//! Tests for the Merkle-DAG verification system.
//!
//! Verifies (per §7.3 of the protocol spec):
//! - Leaf hash computation
//! - Branch hash from children (ordered concatenation)
//! - Root hash computation
//! - Proof verification
//! - Tamper detection

use openswarm_state::merkle_dag::MerkleDag;

// ═══════════════════════════════════════════════════════════════
// § 7.3 Merkle-DAG Construction
// ═══════════════════════════════════════════════════════════════

#[test]
fn leaf_hash_is_sha256_of_content() {
    let content = b"artifact data";
    let hash = MerkleDag::leaf_hash(content);
    // Must be 64 hex chars (SHA-256)
    assert_eq!(hash.len(), 64);
}

#[test]
fn leaf_hash_is_deterministic() {
    let h1 = MerkleDag::leaf_hash(b"same data");
    let h2 = MerkleDag::leaf_hash(b"same data");
    assert_eq!(h1, h2);
}

#[test]
fn leaf_hash_differs_for_different_content() {
    let h1 = MerkleDag::leaf_hash(b"data A");
    let h2 = MerkleDag::leaf_hash(b"data B");
    assert_ne!(h1, h2);
}

#[test]
fn branch_hash_from_ordered_children() {
    let children = vec![
        "aaa".to_string(),
        "bbb".to_string(),
        "ccc".to_string(),
    ];
    let hash = MerkleDag::branch_hash(&children);
    assert_eq!(hash.len(), 64);
}

#[test]
fn branch_hash_order_matters() {
    let h1 = MerkleDag::branch_hash(&["a".into(), "b".into()]);
    let h2 = MerkleDag::branch_hash(&["b".into(), "a".into()]);
    assert_ne!(
        h1, h2,
        "Branch hash must depend on child order (task index ordering)"
    );
}

#[test]
fn branch_hash_single_child() {
    let hash = MerkleDag::branch_hash(&["child-hash".into()]);
    assert_eq!(hash.len(), 64);
}

#[test]
fn branch_hash_empty_children() {
    let hash = MerkleDag::branch_hash(&[]);
    assert_eq!(hash.len(), 64, "Empty branch should still produce a valid hash");
}

// ═══════════════════════════════════════════════════════════════
// Full DAG Construction
// ═══════════════════════════════════════════════════════════════

#[test]
fn dag_build_single_leaf() {
    let mut dag = MerkleDag::new();
    let leaf = dag.add_leaf("task-1".into(), b"result data");
    assert!(!leaf.hash.is_empty());
}

#[test]
fn dag_build_branch_with_children() {
    let mut dag = MerkleDag::new();
    let leaf1 = dag.add_leaf("task-1-1".into(), b"result 1");
    let leaf2 = dag.add_leaf("task-1-2".into(), b"result 2");
    let branch = dag.add_branch(
        "task-1".into(),
        vec![leaf1.hash.clone(), leaf2.hash.clone()],
    );
    assert_ne!(branch.hash, leaf1.hash);
    assert_ne!(branch.hash, leaf2.hash);
}

#[test]
fn dag_root_hash_changes_if_leaf_changes() {
    let mut dag1 = MerkleDag::new();
    let l1 = dag1.add_leaf("t-1".into(), b"data A");
    let l2 = dag1.add_leaf("t-2".into(), b"data B");
    let root1 = dag1.add_branch("root".into(), vec![l1.hash, l2.hash]);

    let mut dag2 = MerkleDag::new();
    let l1_tampered = dag2.add_leaf("t-1".into(), b"data A TAMPERED");
    let l2_same = dag2.add_leaf("t-2".into(), b"data B");
    let root2 = dag2.add_branch("root".into(), vec![l1_tampered.hash, l2_same.hash]);

    assert_ne!(
        root1.hash, root2.hash,
        "Root hash must change if any leaf is tampered"
    );
}

// ═══════════════════════════════════════════════════════════════
// Proof Verification
// ═══════════════════════════════════════════════════════════════

#[test]
fn verify_valid_proof() {
    let mut dag = MerkleDag::new();
    let leaf = dag.add_leaf("task-leaf".into(), b"content");
    let branch = dag.add_branch("task-parent".into(), vec![leaf.hash.clone()]);

    let proof = vec![leaf.hash.clone()];
    assert!(
        MerkleDag::verify_proof(&branch.hash, &proof, &leaf.hash),
        "Valid proof must verify"
    );
}

#[test]
fn verify_invalid_proof_wrong_leaf() {
    let mut dag = MerkleDag::new();
    let leaf = dag.add_leaf("real".into(), b"real content");
    let branch = dag.add_branch("parent".into(), vec![leaf.hash.clone()]);

    let fake_hash = MerkleDag::leaf_hash(b"fake content");
    assert!(
        !MerkleDag::verify_proof(&branch.hash, &[fake_hash.clone()], &fake_hash),
        "Proof with wrong leaf must fail"
    );
}

// ═══════════════════════════════════════════════════════════════
// Three-level DAG (simulating Tier-1 → Tier-2 → Tier-3)
// ═══════════════════════════════════════════════════════════════

#[test]
fn three_level_dag_integrity() {
    let mut dag = MerkleDag::new();

    // Tier-3: 4 leaf executors
    let l1 = dag.add_leaf("t3-1".into(), b"result 1");
    let l2 = dag.add_leaf("t3-2".into(), b"result 2");
    let l3 = dag.add_leaf("t3-3".into(), b"result 3");
    let l4 = dag.add_leaf("t3-4".into(), b"result 4");

    // Tier-2: 2 coordinators
    let b1 = dag.add_branch("t2-1".into(), vec![l1.hash, l2.hash]);
    let b2 = dag.add_branch("t2-2".into(), vec![l3.hash, l4.hash]);

    // Tier-1: root
    let root = dag.add_branch("t1-root".into(), vec![b1.hash.clone(), b2.hash.clone()]);

    // Root hash should be deterministic
    let expected_root = MerkleDag::branch_hash(&[b1.hash, b2.hash]);
    assert_eq!(root.hash, expected_root);
}
