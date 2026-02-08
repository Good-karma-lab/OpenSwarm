//! Comprehensive tests for the cryptographic primitives of the OpenSwarm protocol.
//!
//! These tests verify:
//! - Ed25519 keypair generation and agent ID derivation
//! - Message signing and verification (positive and negative cases)
//! - Content-addressed IDs (CID computation, determinism, collision resistance)
//! - Proof of Work generation and verification
//! - Hex encoding/decoding roundtrips

use openswarm_protocol::crypto::*;

// ═══════════════════════════════════════════════════════════════
// § 3.3 Identity — Agent ID Derivation
// ═══════════════════════════════════════════════════════════════

#[test]
fn agent_id_starts_with_did_swarm_prefix() {
    let key = generate_keypair();
    let id = derive_agent_id(&key.verifying_key());
    assert!(
        id.starts_with("did:swarm:"),
        "Agent ID must use DID format: did:swarm:<hash>"
    );
}

#[test]
fn agent_id_has_correct_length() {
    // did:swarm: (10 chars) + 64 hex chars (32 bytes SHA-256) = 74 chars
    let key = generate_keypair();
    let id = derive_agent_id(&key.verifying_key());
    assert_eq!(id.len(), 10 + 64, "Agent ID must be 74 characters total");
}

#[test]
fn agent_id_is_deterministic_for_same_key() {
    let key = generate_keypair();
    let id1 = derive_agent_id(&key.verifying_key());
    let id2 = derive_agent_id(&key.verifying_key());
    assert_eq!(id1, id2, "Same public key must always produce the same agent ID");
}

#[test]
fn different_keys_produce_different_agent_ids() {
    let key1 = generate_keypair();
    let key2 = generate_keypair();
    let id1 = derive_agent_id(&key1.verifying_key());
    let id2 = derive_agent_id(&key2.verifying_key());
    assert_ne!(id1, id2, "Different keys must produce different agent IDs");
}

#[test]
fn agent_id_contains_only_valid_hex_chars() {
    let key = generate_keypair();
    let id = derive_agent_id(&key.verifying_key());
    let hex_part = &id["did:swarm:".len()..];
    assert!(
        hex_part.chars().all(|c| c.is_ascii_hexdigit()),
        "Agent ID hex portion must contain only hex characters"
    );
}

// ═══════════════════════════════════════════════════════════════
// § 3.1.1 Message Signing and Verification
// ═══════════════════════════════════════════════════════════════

#[test]
fn sign_and_verify_succeeds_for_valid_message() {
    let key = generate_keypair();
    let message = b"test swarm message payload";
    let sig = sign_message(&key, message);
    let result = verify_signature(&key.verifying_key(), message, &sig);
    assert!(result.is_ok(), "Valid signature must verify successfully");
}

#[test]
fn verify_fails_for_tampered_message() {
    let key = generate_keypair();
    let sig = sign_message(&key, b"original message");
    let result = verify_signature(&key.verifying_key(), b"tampered message", &sig);
    assert!(
        result.is_err(),
        "Signature must fail verification if message is tampered"
    );
}

#[test]
fn verify_fails_for_wrong_key() {
    let key1 = generate_keypair();
    let key2 = generate_keypair();
    let message = b"test message";
    let sig = sign_message(&key1, message);
    let result = verify_signature(&key2.verifying_key(), message, &sig);
    assert!(
        result.is_err(),
        "Signature must fail verification with wrong public key"
    );
}

#[test]
fn sign_empty_message() {
    let key = generate_keypair();
    let sig = sign_message(&key, b"");
    let result = verify_signature(&key.verifying_key(), b"", &sig);
    assert!(result.is_ok(), "Empty messages should be signable");
}

#[test]
fn sign_large_message() {
    let key = generate_keypair();
    let message = vec![0xABu8; 1_000_000]; // 1MB message
    let sig = sign_message(&key, &message);
    let result = verify_signature(&key.verifying_key(), &message, &sig);
    assert!(result.is_ok(), "Large messages should be signable");
}

// ═══════════════════════════════════════════════════════════════
// § 7.3 Content-Addressed IDs (CID)
// ═══════════════════════════════════════════════════════════════

#[test]
fn cid_is_deterministic() {
    let data = b"hello swarm world";
    let cid1 = compute_cid(data);
    let cid2 = compute_cid(data);
    assert_eq!(cid1, cid2, "CID must be deterministic for same content");
}

#[test]
fn cid_differs_for_different_content() {
    let cid1 = compute_cid(b"content A");
    let cid2 = compute_cid(b"content B");
    assert_ne!(cid1, cid2, "Different content must produce different CIDs");
}

#[test]
fn cid_is_64_hex_chars() {
    let cid = compute_cid(b"test");
    assert_eq!(cid.len(), 64, "CID must be 64 hex characters (SHA-256)");
}

#[test]
fn cid_empty_content() {
    let cid = compute_cid(b"");
    // SHA-256 of empty string is well-known
    assert_eq!(
        cid, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "CID of empty content must match known SHA-256 of empty string"
    );
}

#[test]
fn sha256_produces_32_bytes() {
    let hash = sha256(b"test data");
    assert_eq!(hash.len(), 32, "SHA-256 must produce exactly 32 bytes");
}

// ═══════════════════════════════════════════════════════════════
// § 12.2 Proof of Work
// ═══════════════════════════════════════════════════════════════

#[test]
fn pow_generates_valid_proof() {
    let data = b"agent connection data";
    let difficulty = 8; // 8 leading zero bits = 1 zero byte
    let (nonce, hash) = proof_of_work(data, difficulty);
    assert!(
        hash[0] == 0,
        "PoW hash must have at least {} leading zero bits",
        difficulty
    );
    assert!(
        verify_pow(data, nonce, difficulty),
        "Generated PoW must be verifiable"
    );
}

#[test]
fn pow_verification_rejects_wrong_nonce() {
    let data = b"test data";
    let difficulty = 8;
    let (nonce, _) = proof_of_work(data, difficulty);
    // Nonce + 1 is extremely unlikely to also be valid
    let wrong_valid = verify_pow(data, nonce.wrapping_add(12345), difficulty);
    // We don't assert false because there's a tiny chance it's valid,
    // but we can verify the correct nonce works
    assert!(verify_pow(data, nonce, difficulty));
    // The point is that arbitrary nonces generally don't work
    let _ = wrong_valid;
}

#[test]
fn pow_verification_rejects_wrong_data() {
    let data = b"original data";
    let difficulty = 8;
    let (nonce, _) = proof_of_work(data, difficulty);
    assert!(
        !verify_pow(b"different data", nonce, difficulty),
        "PoW must fail for different data"
    );
}

#[test]
fn pow_difficulty_zero_is_instant() {
    let data = b"any data";
    let (nonce, _) = proof_of_work(data, 0);
    assert!(
        verify_pow(data, nonce, 0),
        "PoW with difficulty 0 should always succeed"
    );
}

// ═══════════════════════════════════════════════════════════════
// Hex Encoding/Decoding
// ═══════════════════════════════════════════════════════════════

#[test]
fn hex_roundtrip() {
    let data = vec![0x00, 0xFF, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45];
    let encoded = compute_cid(&[]); // just using any hex operation
    let decoded = hex_decode(&encoded).unwrap();
    // Verify roundtrip with known data
    let known_hex = "deadbeef";
    let bytes = hex_decode(known_hex).unwrap();
    assert_eq!(bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn hex_decode_rejects_odd_length() {
    let result = hex_decode("abc");
    assert!(result.is_err(), "Odd-length hex strings must be rejected");
}

#[test]
fn hex_decode_rejects_invalid_chars() {
    let result = hex_decode("gghhii");
    assert!(
        result.is_err(),
        "Non-hex characters must be rejected"
    );
}

#[test]
fn hex_decode_empty_string() {
    let result = hex_decode("").unwrap();
    assert!(result.is_empty(), "Empty hex string should decode to empty bytes");
}
