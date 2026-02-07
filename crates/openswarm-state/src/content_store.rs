//! Content-addressed storage: CID generation, local storage, DHT publishing.
//!
//! All artifacts produced by agents are stored content-addressed:
//! - The Content ID (CID) is the SHA-256 hex digest of the content
//! - Content is stored locally and its CID is published to the DHT
//! - Other agents can retrieve content by CID from the DHT
//!
//! This provides:
//! - Deduplication: identical content has the same CID
//! - Integrity: any bit flip changes the CID
//! - Location-independence: content is found by hash, not by location

use std::collections::{HashMap, HashSet};

use sha2::{Digest, Sha256};

/// Content-addressed storage for artifacts.
///
/// Stores content locally with optional DHT publishing for
/// distributed retrieval. All content is identified by its
/// SHA-256 hash (CID).
pub struct ContentStore {
    /// Local content storage: CID -> content bytes.
    data: HashMap<String, Vec<u8>>,
    /// Provider records: CID -> set of agent IDs that have the content.
    providers: HashMap<String, HashSet<String>>,
}

impl ContentStore {
    /// Create a new empty content store.
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            providers: HashMap::new(),
        }
    }

    /// Store content and return its CID (SHA-256 hex).
    ///
    /// If content with the same CID already exists, this is a no-op
    /// (deduplication). Returns the CID in either case.
    pub fn store(&mut self, data: &[u8]) -> String {
        let cid = Self::compute_cid(data);
        // Deduplicate: only insert if not already present.
        self.data.entry(cid.clone()).or_insert_with(|| data.to_vec());
        cid
    }

    /// Retrieve content by CID.
    ///
    /// Returns `None` if the CID is not found in local storage.
    pub fn get(&self, cid: &str) -> Option<Vec<u8>> {
        self.data.get(cid).cloned()
    }

    /// Check if content exists locally.
    pub fn exists(&self, cid: &str) -> bool {
        self.data.contains_key(cid)
    }

    /// Publish a provider record for a CID.
    ///
    /// Registers `agent_id` as a provider of the content identified
    /// by `cid`. Multiple agents can provide the same content.
    pub fn publish_provider(&mut self, cid: &str, agent_id: String) {
        self.providers
            .entry(cid.to_string())
            .or_insert_with(HashSet::new)
            .insert(agent_id);
    }

    /// Get all provider agent IDs for a CID.
    ///
    /// Returns an empty list if no providers are known.
    pub fn get_providers(&self, cid: &str) -> Vec<String> {
        self.providers
            .get(cid)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Compute the CID for content without storing it.
    pub fn compute_cid(content: &[u8]) -> String {
        let hash = Sha256::digest(content);
        hex_encode(&hash)
    }

    /// Get the total number of stored items.
    pub fn item_count(&self) -> usize {
        self.data.len()
    }

    /// Get all CIDs in the store.
    pub fn all_cids(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }
}

impl Default for ContentStore {
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
    fn test_store_and_retrieve() {
        let mut store = ContentStore::new();
        let data = b"Hello, Swarm!";
        let cid = store.store(data);
        assert!(store.exists(&cid));
        assert_eq!(store.get(&cid), Some(data.to_vec()));
    }

    #[test]
    fn test_deduplication() {
        let mut store = ContentStore::new();
        let cid1 = store.store(b"same");
        let cid2 = store.store(b"same");
        assert_eq!(cid1, cid2);
        assert_eq!(store.item_count(), 1);
    }

    #[test]
    fn test_cid_deterministic() {
        let cid1 = ContentStore::compute_cid(b"test");
        let cid2 = ContentStore::compute_cid(b"test");
        assert_eq!(cid1, cid2);
    }

    #[test]
    fn test_providers() {
        let mut store = ContentStore::new();
        let cid = store.store(b"data");
        store.publish_provider(&cid, "agent-1".into());
        store.publish_provider(&cid, "agent-2".into());
        let providers = store.get_providers(&cid);
        assert_eq!(providers.len(), 2);
    }
}
