//! Merkle-DAG construction for bottom-up result verification.
//!
//! When executor agents produce results, those results are assembled
//! into a Merkle DAG where:
//! - Leaf nodes are individual executor results (SHA-256 of content bytes)
//! - Branch nodes hash their ordered children's hashes together
//! - The root hash represents the aggregate result of the entire task
//!
//! This enables:
//! - Efficient verification: any coordinator can verify a subtree
//!   by checking hashes without downloading all content
//! - Tamper detection: any modification invalidates the root hash
//! - Incremental assembly: results can be added as they arrive

use std::collections::HashMap;

use sha2::{Digest, Sha256};

/// A node in the Merkle DAG.
#[derive(Debug, Clone)]
pub struct MerkleNode {
    /// The task ID this node represents results for.
    pub task_id: String,
    /// The hash of this node (SHA-256 hex string).
    pub hash: String,
    /// Hashes of child nodes (empty for leaf nodes).
    pub children: Vec<String>,
}

/// A complete Merkle DAG for verifying task results.
///
/// Nodes are stored by their hash. The DAG is built bottom-up as
/// executor results arrive, and the root is computed when all
/// children at each level are present.
pub struct MerkleDag {
    /// All nodes in the DAG, keyed by hash.
    nodes: HashMap<String, MerkleNode>,
}

impl MerkleDag {
    /// Create a new empty Merkle DAG.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    // ── Static hash computation ────────────────────────────────────

    /// Compute the SHA-256 hex hash of raw content bytes (leaf node).
    pub fn leaf_hash(content: &[u8]) -> String {
        let hash = Sha256::digest(content);
        hex_encode(&hash)
    }

    /// Compute the SHA-256 hex hash of an ordered list of child hashes
    /// (branch/internal node).
    ///
    /// The children are concatenated in order and hashed. This means
    /// child ordering matters (task index ordering).
    pub fn branch_hash(children: &[String]) -> String {
        let mut hasher = Sha256::new();
        for child in children {
            hasher.update(child.as_bytes());
        }
        hex_encode(&hasher.finalize())
    }

    /// Verify a Merkle proof.
    ///
    /// A proof is valid if:
    /// 1. The `leaf_hash` is contained in the `proof` path.
    /// 2. Hashing the proof elements produces the `root_hash`.
    pub fn verify_proof(root_hash: &str, proof: &[String], leaf_hash: &str) -> bool {
        // The leaf must be in the proof path.
        if !proof.iter().any(|h| h == leaf_hash) {
            return false;
        }
        // Recompute the root from the proof and compare.
        let computed_root = Self::branch_hash(proof);
        computed_root == root_hash
    }

    // ── Instance methods ───────────────────────────────────────────

    /// Add a leaf node to the DAG.
    ///
    /// The leaf's hash is the SHA-256 of the raw content bytes.
    pub fn add_leaf(&mut self, task_id: String, content: &[u8]) -> MerkleNode {
        let hash = Self::leaf_hash(content);
        let node = MerkleNode {
            task_id,
            hash: hash.clone(),
            children: Vec::new(),
        };
        self.nodes.insert(hash, node.clone());
        node
    }

    /// Add a branch (internal) node to the DAG.
    ///
    /// The branch hash is computed from the ordered child hashes.
    pub fn add_branch(&mut self, task_id: String, child_hashes: Vec<String>) -> MerkleNode {
        let hash = Self::branch_hash(&child_hashes);
        let node = MerkleNode {
            task_id,
            hash: hash.clone(),
            children: child_hashes,
        };
        self.nodes.insert(hash, node.clone());
        node
    }

    /// Get a node by its hash.
    pub fn get_node(&self, hash: &str) -> Option<&MerkleNode> {
        self.nodes.get(hash)
    }

    /// Get the total number of nodes in the DAG.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

impl Default for MerkleDag {
    fn default() -> Self {
        Self::new()
    }
}

/// Hex-encode a byte slice into a lowercase hex string.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leaf_hash_deterministic() {
        let h1 = MerkleDag::leaf_hash(b"hello");
        let h2 = MerkleDag::leaf_hash(b"hello");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn test_leaf_hash_differs() {
        let h1 = MerkleDag::leaf_hash(b"aaa");
        let h2 = MerkleDag::leaf_hash(b"bbb");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_branch_hash_order_matters() {
        let h1 = MerkleDag::branch_hash(&["a".into(), "b".into()]);
        let h2 = MerkleDag::branch_hash(&["b".into(), "a".into()]);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_add_leaf_and_branch() {
        let mut dag = MerkleDag::new();
        let l1 = dag.add_leaf("t1".into(), b"data1");
        let l2 = dag.add_leaf("t2".into(), b"data2");
        let branch = dag.add_branch("root".into(), vec![l1.hash, l2.hash]);
        assert_eq!(branch.children.len(), 2);
        assert_eq!(dag.node_count(), 3);
    }
}
