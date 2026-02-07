//! Tests for the content-addressed storage system.
//!
//! Verifies (per §8.2 of the protocol spec):
//! - Content storage and CID generation
//! - Content retrieval by CID
//! - Provider record publishing

use openswarm_state::content_store::ContentStore;

// ═══════════════════════════════════════════════════════════════
// § 8.2 Content-Addressed Storage
// ═══════════════════════════════════════════════════════════════

#[test]
fn store_and_retrieve() {
    let mut store = ContentStore::new();
    let data = b"hello swarm world";
    let cid = store.store(data);
    assert!(!cid.is_empty());
    let retrieved = store.get(&cid);
    assert_eq!(retrieved, Some(data.to_vec()));
}

#[test]
fn store_same_content_returns_same_cid() {
    let mut store = ContentStore::new();
    let cid1 = store.store(b"identical");
    let cid2 = store.store(b"identical");
    assert_eq!(cid1, cid2, "Same content must produce same CID");
}

#[test]
fn store_different_content_different_cid() {
    let mut store = ContentStore::new();
    let cid1 = store.store(b"content A");
    let cid2 = store.store(b"content B");
    assert_ne!(cid1, cid2);
}

#[test]
fn get_nonexistent_returns_none() {
    let store = ContentStore::new();
    assert_eq!(store.get("nonexistent-cid"), None);
}

#[test]
fn store_empty_content() {
    let mut store = ContentStore::new();
    let cid = store.store(b"");
    assert!(!cid.is_empty(), "Empty content should have a valid CID");
    assert_eq!(store.get(&cid), Some(vec![]));
}

#[test]
fn store_large_content() {
    let mut store = ContentStore::new();
    let data = vec![0xABu8; 1_000_000]; // 1MB
    let cid = store.store(&data);
    let retrieved = store.get(&cid).unwrap();
    assert_eq!(retrieved.len(), 1_000_000);
}

#[test]
fn provider_records() {
    let mut store = ContentStore::new();
    let cid = store.store(b"data");
    store.publish_provider(&cid, "agent-1".into());
    store.publish_provider(&cid, "agent-2".into());
    let providers = store.get_providers(&cid);
    assert_eq!(providers.len(), 2);
    assert!(providers.contains(&"agent-1".to_string()));
    assert!(providers.contains(&"agent-2".to_string()));
}

#[test]
fn provider_for_unknown_cid() {
    let store = ContentStore::new();
    let providers = store.get_providers("unknown");
    assert!(providers.is_empty());
}

#[test]
fn content_exists_check() {
    let mut store = ContentStore::new();
    let cid = store.store(b"exists");
    assert!(store.exists(&cid));
    assert!(!store.exists("nope"));
}
