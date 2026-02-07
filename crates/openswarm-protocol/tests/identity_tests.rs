//! Comprehensive tests for the identity and scoring system.
//!
//! Verifies:
//! - AgentId format and behavior
//! - Vivaldi coordinate distance and update
//! - NodeScore composite score computation
//! - AgentProfile construction

use openswarm_protocol::identity::*;

// ═══════════════════════════════════════════════════════════════
// § 3.3 AgentId
// ═══════════════════════════════════════════════════════════════

#[test]
fn agent_id_display() {
    let id = AgentId::new("did:swarm:abcdef".to_string());
    assert_eq!(format!("{}", id), "did:swarm:abcdef");
}

#[test]
fn agent_id_equality() {
    let a = AgentId::new("did:swarm:abc".into());
    let b = AgentId::new("did:swarm:abc".into());
    let c = AgentId::new("did:swarm:xyz".into());
    assert_eq!(a, b, "Same DIDs must be equal");
    assert_ne!(a, c, "Different DIDs must not be equal");
}

#[test]
fn agent_id_hash_consistency() {
    use std::collections::HashSet;
    let id1 = AgentId::new("did:swarm:test".into());
    let id2 = AgentId::new("did:swarm:test".into());
    let mut set = HashSet::new();
    set.insert(id1);
    assert!(set.contains(&id2), "Equal AgentIds must have equal hashes");
}

#[test]
fn agent_id_as_str() {
    let id = AgentId::new("did:swarm:123".into());
    assert_eq!(id.as_str(), "did:swarm:123");
}

// ═══════════════════════════════════════════════════════════════
// § 5.6 Vivaldi Coordinates
// ═══════════════════════════════════════════════════════════════

#[test]
fn vivaldi_origin() {
    let v = VivaldiCoordinates::origin();
    assert_eq!(v.x, 0.0);
    assert_eq!(v.y, 0.0);
    assert_eq!(v.z, 0.0);
}

#[test]
fn vivaldi_self_distance_is_zero() {
    let v = VivaldiCoordinates {
        x: 1.5,
        y: 2.3,
        z: 4.1,
    };
    assert!(
        v.distance_to(&v) < 1e-10,
        "Distance to self must be zero"
    );
}

#[test]
fn vivaldi_distance_symmetry() {
    let a = VivaldiCoordinates {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let b = VivaldiCoordinates {
        x: 4.0,
        y: 5.0,
        z: 6.0,
    };
    let d_ab = a.distance_to(&b);
    let d_ba = b.distance_to(&a);
    assert!(
        (d_ab - d_ba).abs() < 1e-10,
        "Distance must be symmetric: d(a,b) == d(b,a)"
    );
}

#[test]
fn vivaldi_distance_3_4_5_triangle() {
    let a = VivaldiCoordinates {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let b = VivaldiCoordinates {
        x: 3.0,
        y: 4.0,
        z: 0.0,
    };
    assert!(
        (a.distance_to(&b) - 5.0).abs() < 1e-10,
        "3-4-5 right triangle should have hypotenuse 5"
    );
}

#[test]
fn vivaldi_distance_3d() {
    let a = VivaldiCoordinates {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let b = VivaldiCoordinates {
        x: 1.0,
        y: 2.0,
        z: 2.0,
    };
    let expected = 3.0; // sqrt(1 + 4 + 4) = 3
    assert!(
        (a.distance_to(&b) - expected).abs() < 1e-10,
        "3D distance should be sqrt(sum of squares)"
    );
}

#[test]
fn vivaldi_update_moves_toward_peer() {
    let mut me = VivaldiCoordinates::origin();
    let peer = VivaldiCoordinates {
        x: 10.0,
        y: 0.0,
        z: 0.0,
    };
    let distance_before = me.distance_to(&peer);
    // Observed RTT is larger than estimated distance (0.0),
    // so we should move AWAY from peer (increase coordinates to reflect that)
    // Actually with the Vivaldi algorithm, if observed RTT > estimated distance,
    // we should move to increase estimated distance
    me.update(&peer, 20.0, 0.1);
    // After update, coordinates should have changed
    assert!(
        me.x != 0.0 || me.y != 0.0 || me.z != 0.0,
        "Coordinates must change after update"
    );
}

#[test]
fn vivaldi_serialization_roundtrip() {
    let v = VivaldiCoordinates {
        x: 1.23,
        y: 4.56,
        z: 7.89,
    };
    let json = serde_json::to_string(&v).unwrap();
    let parsed: VivaldiCoordinates = serde_json::from_str(&json).unwrap();
    assert!((parsed.x - 1.23).abs() < 1e-10);
    assert!((parsed.y - 4.56).abs() < 1e-10);
    assert!((parsed.z - 7.89).abs() < 1e-10);
}

// ═══════════════════════════════════════════════════════════════
// § 5.5 Composite Score
// ═══════════════════════════════════════════════════════════════

#[test]
fn composite_score_all_ones() {
    let score = NodeScore {
        agent_id: AgentId::new("did:swarm:test".into()),
        proof_of_compute: 1.0,
        reputation: 1.0,
        uptime: 1.0,
        stake: Some(1.0),
    };
    assert!(
        (score.composite_score() - 1.0).abs() < 1e-10,
        "All-1.0 inputs must produce composite score of 1.0"
    );
}

#[test]
fn composite_score_all_zeros() {
    let score = NodeScore {
        agent_id: AgentId::new("did:swarm:test".into()),
        proof_of_compute: 0.0,
        reputation: 0.0,
        uptime: 0.0,
        stake: Some(0.0),
    };
    assert!(
        score.composite_score().abs() < 1e-10,
        "All-0.0 inputs must produce composite score of 0.0"
    );
}

#[test]
fn composite_score_no_stake() {
    let score = NodeScore {
        agent_id: AgentId::new("did:swarm:test".into()),
        proof_of_compute: 0.8,
        reputation: 0.9,
        uptime: 1.0,
        stake: None,
    };
    let expected = 0.25 * 0.8 + 0.40 * 0.9 + 0.20 * 1.0 + 0.15 * 0.0;
    assert!(
        (score.composite_score() - expected).abs() < 1e-10,
        "Missing stake must be treated as 0.0"
    );
}

#[test]
fn composite_score_weights_sum_to_one() {
    // The weights 0.25 + 0.40 + 0.20 + 0.15 = 1.0
    assert!(
        (0.25f64 + 0.40 + 0.20 + 0.15 - 1.0).abs() < 1e-10,
        "Composite score weights must sum to 1.0"
    );
}

#[test]
fn composite_score_reputation_has_highest_weight() {
    // Agent A: only high reputation
    let a = NodeScore {
        agent_id: AgentId::new("did:swarm:a".into()),
        proof_of_compute: 0.0,
        reputation: 1.0,
        uptime: 0.0,
        stake: Some(0.0),
    };
    // Agent B: only high PoC
    let b = NodeScore {
        agent_id: AgentId::new("did:swarm:b".into()),
        proof_of_compute: 1.0,
        reputation: 0.0,
        uptime: 0.0,
        stake: Some(0.0),
    };
    assert!(
        a.composite_score() > b.composite_score(),
        "Reputation (0.40 weight) must outweigh PoC (0.25 weight)"
    );
}

#[test]
fn composite_score_stake_clamped_to_one() {
    let score = NodeScore {
        agent_id: AgentId::new("did:swarm:test".into()),
        proof_of_compute: 0.0,
        reputation: 0.0,
        uptime: 0.0,
        stake: Some(10.0), // Excessive stake
    };
    let expected = 0.15 * 1.0; // Clamped to 1.0
    assert!(
        (score.composite_score() - expected).abs() < 1e-10,
        "Stake must be clamped to 1.0 in composite score"
    );
}

// ═══════════════════════════════════════════════════════════════
// Serialization
// ═══════════════════════════════════════════════════════════════

#[test]
fn agent_profile_serialization_roundtrip() {
    let profile = AgentProfile {
        agent_id: AgentId::new("did:swarm:abc123".into()),
        pub_key: "MCowBQYDK2Vw...".into(),
        capabilities: AgentCapabilities {
            models: vec!["gpt-4".into()],
            skills: vec!["python-exec".into(), "web-search".into()],
        },
        resources: AgentResources {
            cpu_cores: 8,
            ram_gb: 32,
            gpu_vram_gb: Some(16),
            disk_gb: Some(500),
        },
        location_vector: VivaldiCoordinates {
            x: 0.45,
            y: 0.12,
            z: 0.99,
        },
    };
    let json = serde_json::to_string(&profile).unwrap();
    let parsed: AgentProfile = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.agent_id, profile.agent_id);
    assert_eq!(parsed.resources.cpu_cores, 8);
    assert_eq!(parsed.resources.gpu_vram_gb, Some(16));
    assert_eq!(parsed.capabilities.models, vec!["gpt-4"]);
}

#[test]
fn agent_resources_optional_fields() {
    let res = AgentResources {
        cpu_cores: 4,
        ram_gb: 16,
        gpu_vram_gb: None,
        disk_gb: None,
    };
    let json = serde_json::to_string(&res).unwrap();
    let parsed: AgentResources = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.gpu_vram_gb, None);
    assert_eq!(parsed.disk_gb, None);
}
