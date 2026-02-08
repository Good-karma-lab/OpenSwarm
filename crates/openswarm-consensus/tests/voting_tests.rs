//! Comprehensive integration tests for the Ranked Choice Voting (IRV) algorithm.
//!
//! Verifies (per section 6.4 of the protocol spec):
//! - IRV produces a winner when one candidate has >50% first-choice votes
//! - IRV eliminates lowest-vote candidates and redistributes
//! - Self-vote prohibition
//! - Critic score aggregation on the winning plan
//! - Edge cases: single candidate, epoch mismatch, voting after finalized

use std::collections::HashMap;

use openswarm_consensus::voting::{VotingConfig, VotingEngine};
use openswarm_consensus::ConsensusError;
use openswarm_protocol::{AgentId, CriticScore, RankedVote};

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

/// Build a RankedVote with no critic scores.
fn vote(voter: &str, task_id: &str, epoch: u64, rankings: &[&str]) -> RankedVote {
    RankedVote {
        voter: AgentId::new(voter.to_string()),
        task_id: task_id.to_string(),
        epoch,
        rankings: rankings.iter().map(|s| s.to_string()).collect(),
        critic_scores: HashMap::new(),
    }
}

/// Build a RankedVote that includes critic scores for every ranked plan.
fn vote_with_scores(
    voter: &str,
    task_id: &str,
    epoch: u64,
    rankings: &[&str],
    scores: &[(&str, CriticScore)],
) -> RankedVote {
    let mut critic_scores = HashMap::new();
    for (plan_id, score) in scores {
        critic_scores.insert(plan_id.to_string(), score.clone());
    }
    RankedVote {
        voter: AgentId::new(voter.to_string()),
        task_id: task_id.to_string(),
        epoch,
        rankings: rankings.iter().map(|s| s.to_string()).collect(),
        critic_scores,
    }
}

/// Create a VotingEngine with self-vote prohibition disabled and register
/// the given proposals (plan_id -> proposer_id).
fn engine_with_proposals(
    task_id: &str,
    epoch: u64,
    proposals: &[(&str, &str)],
    prohibit_self_vote: bool,
) -> VotingEngine {
    let config = VotingConfig {
        prohibit_self_vote,
        min_votes: 1,
        ..Default::default()
    };
    let mut engine = VotingEngine::new(config, task_id.to_string(), epoch);
    let map: HashMap<String, AgentId> = proposals
        .iter()
        .map(|(plan, proposer)| (plan.to_string(), AgentId::new(proposer.to_string())))
        .collect();
    engine.set_proposals(map);
    engine
}

// ═══════════════════════════════════════════════════════════════
// Section 6.4  Instant Runoff Voting -- Basic Cases
// ═══════════════════════════════════════════════════════════════

#[test]
fn irv_single_candidate_wins() {
    let mut engine = engine_with_proposals("t1", 1, &[("planA", "alice")], false);
    engine
        .record_vote(vote("v1", "t1", 1, &["planA"]))
        .unwrap();
    let result = engine.run_irv().unwrap();
    assert_eq!(result.winner, "planA");
    assert_eq!(result.rounds, 1);
    assert_eq!(result.total_votes, 1);
}

#[test]
fn irv_clear_majority_wins_first_round() {
    // planA gets 6/10 first-choice votes = 60% > 50% threshold.
    let mut engine = engine_with_proposals(
        "t1",
        1,
        &[("planA", "alice"), ("planB", "bob"), ("planC", "carol")],
        false,
    );

    for i in 0..6 {
        engine
            .record_vote(vote(
                &format!("va{}", i),
                "t1",
                1,
                &["planA", "planB", "planC"],
            ))
            .unwrap();
    }
    for i in 0..3 {
        engine
            .record_vote(vote(
                &format!("vb{}", i),
                "t1",
                1,
                &["planB", "planC", "planA"],
            ))
            .unwrap();
    }
    engine
        .record_vote(vote("vc0", "t1", 1, &["planC", "planB", "planA"]))
        .unwrap();

    let result = engine.run_irv().unwrap();
    assert_eq!(
        result.winner, "planA",
        "Clear majority should win in the first round"
    );
    assert_eq!(result.rounds, 1);
    assert_eq!(result.total_votes, 10);
}

#[test]
fn irv_elimination_and_redistribution() {
    // planA: 4, planB: 3, planC: 2 first-choice votes.
    // planC eliminated; its voters prefer planA second.
    // After redistribution planA has 6/9 > 50% and wins.
    let mut engine = engine_with_proposals(
        "t1",
        1,
        &[("planA", "alice"), ("planB", "bob"), ("planC", "carol")],
        false,
    );

    for i in 0..4 {
        engine
            .record_vote(vote(
                &format!("va{}", i),
                "t1",
                1,
                &["planA", "planB", "planC"],
            ))
            .unwrap();
    }
    for i in 0..3 {
        engine
            .record_vote(vote(
                &format!("vb{}", i),
                "t1",
                1,
                &["planB", "planA", "planC"],
            ))
            .unwrap();
    }
    for i in 0..2 {
        engine
            .record_vote(vote(
                &format!("vc{}", i),
                "t1",
                1,
                &["planC", "planA", "planB"],
            ))
            .unwrap();
    }

    let result = engine.run_irv().unwrap();
    assert_eq!(
        result.winner, "planA",
        "After eliminating planC, planA should win with redistributed votes"
    );
    assert_eq!(result.elimination_order, vec!["planC".to_string()]);
    assert!(result.rounds >= 2, "Should take at least two rounds");
}

#[test]
fn irv_multiple_elimination_rounds() {
    // 5 candidates with vote distribution requiring multiple eliminations.
    // plan0:3, plan1:3, plan2:2, plan3:1, plan4:1  (total 10)
    let plans: Vec<(&str, &str)> = vec![
        ("plan0", "a0"),
        ("plan1", "a1"),
        ("plan2", "a2"),
        ("plan3", "a3"),
        ("plan4", "a4"),
    ];
    let mut engine = engine_with_proposals("t1", 1, &plans, false);

    for i in 0..3 {
        engine
            .record_vote(vote(
                &format!("v0_{}", i),
                "t1",
                1,
                &["plan0", "plan2", "plan1"],
            ))
            .unwrap();
    }
    for i in 0..3 {
        engine
            .record_vote(vote(
                &format!("v1_{}", i),
                "t1",
                1,
                &["plan1", "plan3", "plan0"],
            ))
            .unwrap();
    }
    for i in 0..2 {
        engine
            .record_vote(vote(
                &format!("v2_{}", i),
                "t1",
                1,
                &["plan2", "plan0", "plan1"],
            ))
            .unwrap();
    }
    engine
        .record_vote(vote("v3_0", "t1", 1, &["plan3", "plan1", "plan0"]))
        .unwrap();
    engine
        .record_vote(vote("v4_0", "t1", 1, &["plan4", "plan0", "plan1"]))
        .unwrap();

    let result = engine.run_irv().unwrap();
    assert!(
        !result.winner.is_empty(),
        "Must produce a winner after multiple elimination rounds"
    );
    assert!(
        result.rounds >= 2,
        "Multiple elimination rounds expected, got {}",
        result.rounds
    );
    assert_eq!(result.total_votes, 10);
}

#[test]
fn irv_no_ballots_returns_error() {
    let mut engine = engine_with_proposals(
        "t1",
        1,
        &[("planA", "alice"), ("planB", "bob")],
        false,
    );
    let result = engine.run_irv();
    assert!(result.is_err(), "Empty election must return an error");
    assert!(matches!(result.unwrap_err(), ConsensusError::NoVotes(_)));
}

// ═══════════════════════════════════════════════════════════════
// Section 6.4  Self-Vote Prohibition
// ═══════════════════════════════════════════════════════════════

#[test]
fn self_vote_is_rejected() {
    // With self-vote prohibition enabled, alice cannot rank planA (her own) first.
    let mut engine = engine_with_proposals(
        "t1",
        1,
        &[("planA", "alice"), ("planB", "bob")],
        true, // prohibit self-vote
    );
    let result = engine.record_vote(vote("alice", "t1", 1, &["planA", "planB"]));
    assert!(
        matches!(result, Err(ConsensusError::SelfVoteProhibited(_))),
        "Self-vote must be rejected"
    );
}

#[test]
fn voting_for_others_plan_is_allowed() {
    // alice ranks bob's plan first -- this should be accepted.
    let mut engine = engine_with_proposals(
        "t1",
        1,
        &[("planA", "alice"), ("planB", "bob")],
        true,
    );
    let result = engine.record_vote(vote("alice", "t1", 1, &["planB", "planA"]));
    assert!(result.is_ok(), "Voting for another agent's plan must be allowed");
}

#[test]
fn self_vote_prohibition_disabled_allows_own_plan_first() {
    // With prohibition disabled, alice CAN rank her own plan first.
    let mut engine = engine_with_proposals(
        "t1",
        1,
        &[("planA", "alice"), ("planB", "bob")],
        false,
    );
    let result = engine.record_vote(vote("alice", "t1", 1, &["planA", "planB"]));
    assert!(
        result.is_ok(),
        "Self-vote should be allowed when prohibition is disabled"
    );
}

// ═══════════════════════════════════════════════════════════════
// Critic Score Aggregation
// ═══════════════════════════════════════════════════════════════

#[test]
fn winner_has_aggregated_critic_scores() {
    let mut engine = engine_with_proposals(
        "t1",
        1,
        &[("planA", "alice"), ("planB", "bob")],
        false,
    );

    let score_a = CriticScore {
        feasibility: 0.9,
        parallelism: 0.8,
        completeness: 0.85,
        risk: 0.1,
    };

    // Three voters all prefer planA and provide critic scores.
    for i in 0..3 {
        engine
            .record_vote(vote_with_scores(
                &format!("v{}", i),
                "t1",
                1,
                &["planA", "planB"],
                &[("planA", score_a.clone())],
            ))
            .unwrap();
    }

    let result = engine.run_irv().unwrap();
    assert_eq!(result.winner, "planA");
    let critic = result
        .winner_critic_score
        .expect("Winner should have aggregated critic scores");
    // Since all three voters gave the same score, the aggregate should match.
    assert!(
        (critic.feasibility - 0.9).abs() < 1e-6,
        "Feasibility should be 0.9, got {}",
        critic.feasibility
    );
    assert!(
        (critic.risk - 0.1).abs() < 1e-6,
        "Risk should be 0.1, got {}",
        critic.risk
    );
}

// ═══════════════════════════════════════════════════════════════
// Error Handling
// ═══════════════════════════════════════════════════════════════

#[test]
fn epoch_mismatch_rejected() {
    let mut engine = engine_with_proposals(
        "t1",
        1, // engine epoch = 1
        &[("planA", "alice")],
        false,
    );
    // Vote with epoch = 99 should fail.
    let result = engine.record_vote(vote("v1", "t1", 99, &["planA"]));
    assert!(
        matches!(result, Err(ConsensusError::EpochMismatch { .. })),
        "Epoch mismatch must be rejected"
    );
}

#[test]
fn task_id_mismatch_rejected() {
    let mut engine = engine_with_proposals("t1", 1, &[("planA", "alice")], false);
    let result = engine.record_vote(vote("v1", "wrong-task", 1, &["planA"]));
    assert!(
        matches!(result, Err(ConsensusError::TaskNotFound(_))),
        "Task ID mismatch must be rejected"
    );
}

#[test]
fn vote_after_finalization_rejected() {
    let mut engine = engine_with_proposals(
        "t1",
        1,
        &[("planA", "alice"), ("planB", "bob")],
        false,
    );
    engine
        .record_vote(vote("v1", "t1", 1, &["planA", "planB"]))
        .unwrap();
    let _result = engine.run_irv().unwrap();
    assert!(engine.is_finalized());

    // After finalization, recording a new vote must fail.
    let result = engine.record_vote(vote("v2", "t1", 1, &["planB", "planA"]));
    assert!(
        result.is_err(),
        "Recording votes after finalization must fail"
    );
}

#[test]
fn invalid_proposal_in_rankings_filtered() {
    let mut engine = engine_with_proposals(
        "t1",
        1,
        &[("planA", "alice"), ("planB", "bob")],
        false,
    );
    // "planX" is not a registered proposal -- it should be filtered out,
    // but planA remains valid so the vote is accepted.
    let result = engine.record_vote(vote("v1", "t1", 1, &["planX", "planA"]));
    assert!(result.is_ok());
    assert_eq!(engine.ballot_count(), 1);
}

#[test]
fn all_invalid_proposals_rejected() {
    let mut engine = engine_with_proposals("t1", 1, &[("planA", "alice")], false);
    // No valid proposals in rankings.
    let result = engine.record_vote(vote("v1", "t1", 1, &["planX", "planY"]));
    assert!(result.is_err(), "Vote with no valid proposals must fail");
}

// ═══════════════════════════════════════════════════════════════
// Senate Sampling
// ═══════════════════════════════════════════════════════════════

#[test]
fn senate_restricts_eligible_voters() {
    let config = VotingConfig {
        senate_size: 2,
        prohibit_self_vote: false,
        min_votes: 1,
        senate_seed: Some(42),
    };
    let mut engine = VotingEngine::new(config, "t1".to_string(), 1);
    let mut proposals = HashMap::new();
    proposals.insert("planA".to_string(), AgentId::new("alice".into()));
    engine.set_proposals(proposals);

    // Only v1 and v2 are in the eligible voter pool of 3, but senate_size=2
    // so only 2 are selected.
    let voters = vec![
        AgentId::new("v1".into()),
        AgentId::new("v2".into()),
        AgentId::new("v3".into()),
    ];
    engine.select_senate(&voters);

    // At least one of the three voters should be excluded from the senate.
    // We try all three and expect at least one rejection.
    let mut accepted = 0;
    let mut rejected = 0;
    for name in &["v1", "v2", "v3"] {
        match engine.record_vote(vote(name, "t1", 1, &["planA"])) {
            Ok(()) => accepted += 1,
            Err(ConsensusError::VotingError(_)) => rejected += 1,
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }
    assert_eq!(accepted, 2, "Senate of size 2 should accept exactly 2 voters");
    assert_eq!(rejected, 1, "One voter should be rejected by the senate");
}

// ═══════════════════════════════════════════════════════════════
// Engine Metadata
// ═══════════════════════════════════════════════════════════════

#[test]
fn proposal_and_ballot_counts() {
    let mut engine = engine_with_proposals(
        "t1",
        1,
        &[("planA", "alice"), ("planB", "bob")],
        false,
    );
    assert_eq!(engine.proposal_count(), 2);
    assert_eq!(engine.ballot_count(), 0);
    assert!(!engine.is_finalized());

    engine
        .record_vote(vote("v1", "t1", 1, &["planA", "planB"]))
        .unwrap();
    assert_eq!(engine.ballot_count(), 1);
}
