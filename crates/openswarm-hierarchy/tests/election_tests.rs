//! Tests for the election system.
//!
//! Verifies (per §5.4 of the protocol spec):
//! - Top-k agents by composite score are elected to Tier-1
//! - Latency/centrality is considered
//! - Re-election at epoch boundaries

use openswarm_hierarchy::elections::{ElectionConfig, ElectionManager};
use openswarm_hierarchy::geo_cluster::GeoCluster;
use openswarm_hierarchy::succession::SuccessionManager;
use openswarm_protocol::{AgentId, CandidacyParams, ElectionVoteParams, NodeScore, VivaldiCoordinates};

/// Helper to build a CandidacyParams with the given scores and epoch.
fn make_candidacy(
    agent: &str,
    poc: f64,
    reputation: f64,
    uptime: f64,
    stake: Option<f64>,
    epoch: u64,
) -> CandidacyParams {
    CandidacyParams {
        agent_id: AgentId::new(agent.to_string()),
        epoch,
        score: NodeScore {
            agent_id: AgentId::new(agent.to_string()),
            proof_of_compute: poc,
            reputation,
            uptime,
            stake,
        },
        location_vector: VivaldiCoordinates::origin(),
    }
}

// ═══════════════════════════════════════════════════════════════
// § 5.4 Tier-1 Elections
// ═══════════════════════════════════════════════════════════════

#[test]
fn election_selects_top_k_by_score() {
    let config = ElectionConfig {
        tier1_slots: 10,
        ..Default::default()
    };
    let mut em = ElectionManager::new(config, 1);

    // Register 20 candidates with varying scores (agent-0 highest, agent-19 lowest).
    for i in 0..20u32 {
        let score = (20 - i) as f64 / 20.0;
        let agent_name = format!("did:swarm:agent-{}", i);
        let candidacy = make_candidacy(&agent_name, score, score, 1.0, Some(0.5), 1);
        em.register_candidate(&candidacy).unwrap();
    }

    // A single voter ranks all candidates from agent-0 (best) to agent-19 (worst).
    // Borda count will award the most points to agent-0 and fewest to agent-19.
    let all_candidates: Vec<AgentId> = (0..20u32)
        .map(|i| AgentId::new(format!("did:swarm:agent-{}", i)))
        .collect();

    em.record_vote(ElectionVoteParams {
        voter: AgentId::new("voter1".into()),
        epoch: 1,
        candidate_rankings: all_candidates,
    })
    .unwrap();

    let result = em.tally_and_elect().unwrap();
    assert_eq!(result.leaders.len(), 10, "Must elect exactly k=10 leaders");

    // Top 10 agents (by Borda tally) should win: indices 0..9.
    for winner in &result.leaders {
        let idx: usize = winner
            .as_str()
            .strip_prefix("did:swarm:agent-")
            .unwrap()
            .parse()
            .unwrap();
        assert!(idx < 10, "Winner index {} should be in top 10", idx);
    }
}

#[test]
fn election_fewer_candidates_than_k() {
    let config = ElectionConfig {
        tier1_slots: 10,
        ..Default::default()
    };
    let mut em = ElectionManager::new(config, 1);

    // Register only 5 candidates (fewer than the 10 slots).
    for i in 0..5u32 {
        let agent_name = format!("agent-{}", i);
        let candidacy = make_candidacy(&agent_name, 0.5, 0.5, 1.0, None, 1);
        em.register_candidate(&candidacy).unwrap();
    }

    em.record_vote(ElectionVoteParams {
        voter: AgentId::new("voter1".into()),
        epoch: 1,
        candidate_rankings: (0..5)
            .map(|i| AgentId::new(format!("agent-{}", i)))
            .collect(),
    })
    .unwrap();

    let result = em.tally_and_elect().unwrap();
    assert_eq!(
        result.leaders.len(),
        5,
        "If fewer candidates than k, all should be elected"
    );
}

#[test]
fn election_no_candidates() {
    let config = ElectionConfig {
        tier1_slots: 10,
        ..Default::default()
    };
    let mut em = ElectionManager::new(config, 1);
    let result = em.tally_and_elect();
    assert!(result.is_err(), "No candidates means election should fail");
}

#[test]
fn election_single_candidate() {
    let config = ElectionConfig {
        tier1_slots: 10,
        ..Default::default()
    };
    let mut em = ElectionManager::new(config, 1);

    let candidacy = make_candidacy("sole-agent", 0.5, 0.5, 1.0, None, 1);
    em.register_candidate(&candidacy).unwrap();

    em.record_vote(ElectionVoteParams {
        voter: AgentId::new("voter1".into()),
        epoch: 1,
        candidate_rankings: vec![AgentId::new("sole-agent".into())],
    })
    .unwrap();

    let result = em.tally_and_elect().unwrap();
    assert_eq!(result.leaders.len(), 1);
    assert_eq!(result.leaders[0].as_str(), "sole-agent");
}

// ═══════════════════════════════════════════════════════════════
// § 5.6 Geo-Clustering
// ═══════════════════════════════════════════════════════════════

#[test]
fn geo_cluster_assigns_to_nearest_leader() {
    let mut gc = GeoCluster::default();

    gc.register_leader(
        AgentId::new("leader-east".into()),
        VivaldiCoordinates {
            x: 10.0,
            y: 0.0,
            z: 0.0,
        },
        100,
    );
    gc.register_leader(
        AgentId::new("leader-west".into()),
        VivaldiCoordinates {
            x: -10.0,
            y: 0.0,
            z: 0.0,
        },
        100,
    );

    // Agent at x=8 should go to leader-east (distance 2 vs 18).
    let agent_loc = VivaldiCoordinates {
        x: 8.0,
        y: 0.0,
        z: 0.0,
    };
    let (assigned, _rtt) = gc.find_best_leader(&agent_loc).unwrap();
    assert_eq!(assigned, AgentId::new("leader-east".into()));

    // Agent at x=-7 should go to leader-west (distance 3 vs 17).
    let agent_loc2 = VivaldiCoordinates {
        x: -7.0,
        y: 0.0,
        z: 0.0,
    };
    let (assigned2, _rtt2) = gc.find_best_leader(&agent_loc2).unwrap();
    assert_eq!(assigned2, AgentId::new("leader-west".into()));
}

#[test]
fn geo_cluster_empty_leaders() {
    let gc = GeoCluster::default();
    let agent_loc = VivaldiCoordinates {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let result = gc.find_best_leader(&agent_loc);
    assert!(result.is_err(), "Empty leaders should return an error");
}

// ═══════════════════════════════════════════════════════════════
// § 5.7 Succession
// ═══════════════════════════════════════════════════════════════

#[test]
fn succession_selects_highest_score_tier2() {
    let mut sm = SuccessionManager::new();
    let failed_leader = AgentId::new("leader1".into());
    sm.monitor_leader(failed_leader.clone(), None);

    let branch_scores = vec![
        NodeScore {
            agent_id: AgentId::new("t2-low".into()),
            proof_of_compute: 0.3,
            reputation: 0.3,
            uptime: 0.5,
            stake: None,
        },
        NodeScore {
            agent_id: AgentId::new("t2-high".into()),
            proof_of_compute: 0.9,
            reputation: 0.9,
            uptime: 1.0,
            stake: Some(0.5),
        },
    ];

    let proposed = sm.initiate_succession(&failed_leader, branch_scores).unwrap();
    assert_eq!(
        proposed,
        AgentId::new("t2-high".into()),
        "Highest composite score must become successor"
    );
}

#[test]
fn succession_empty_branch() {
    let mut sm = SuccessionManager::new();
    let failed_leader = AgentId::new("leader1".into());
    sm.monitor_leader(failed_leader.clone(), None);

    let result = sm.initiate_succession(&failed_leader, vec![]);
    assert!(result.is_err(), "Empty branch has no successor");
}
