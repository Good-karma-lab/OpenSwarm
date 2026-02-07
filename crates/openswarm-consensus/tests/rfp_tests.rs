//! Integration tests for the Request for Proposal (RFP) protocol.
//!
//! Verifies (per section 6.3 of the protocol spec):
//! - Full commit-reveal lifecycle (inject -> commit -> reveal -> finalize)
//! - Hash verification (revealed plan must match committed hash)
//! - Duplicate commit detection
//! - Phase transition enforcement
//! - Multiple proposers
//! - Edge cases: reveal without commit, finalize with no reveals

use openswarm_consensus::rfp::{RfpCoordinator, RfpPhase};
use openswarm_consensus::ConsensusError;
use openswarm_protocol::{AgentId, Plan, PlanSubtask, ProposalCommitParams, ProposalRevealParams, Task};

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

/// Create a Plan with one subtask for testing.
fn make_plan(task_id: &str, proposer: &str, epoch: u64) -> Plan {
    let mut plan = Plan::new(
        task_id.to_string(),
        AgentId::new(proposer.to_string()),
        epoch,
    );
    plan.subtasks.push(PlanSubtask {
        index: 0,
        description: "Subtask A".to_string(),
        required_capabilities: vec!["python".to_string()],
        estimated_complexity: 0.5,
    });
    plan.rationale = "Test plan".to_string();
    plan
}

/// Commit a plan and return the hash that was committed.
fn commit_plan(
    rfp: &mut RfpCoordinator,
    task_id: &str,
    proposer: &str,
    epoch: u64,
    plan: &Plan,
) -> String {
    let hash = RfpCoordinator::compute_plan_hash(plan).unwrap();
    rfp.record_commit(&ProposalCommitParams {
        task_id: task_id.to_string(),
        proposer: AgentId::new(proposer.to_string()),
        epoch,
        plan_hash: hash.clone(),
    })
    .unwrap();
    hash
}

// ═══════════════════════════════════════════════════════════════
// Section 6.3  Full Commit-Reveal Lifecycle
// ═══════════════════════════════════════════════════════════════

#[test]
fn rfp_full_lifecycle_single_proposer() {
    let task = Task::new("Test task".into(), 1, 1);
    let task_id = task.task_id.clone();
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 1);

    // Phase starts as Idle.
    assert_eq!(*rfp.phase(), RfpPhase::Idle);
    assert_eq!(rfp.task_id(), task_id);

    // Inject the task to start the commit phase.
    rfp.inject_task(&task).unwrap();
    assert_eq!(*rfp.phase(), RfpPhase::CommitPhase);

    // Create a plan and commit its hash.
    let plan = make_plan(&task_id, "alice", 1);
    let _hash = commit_plan(&mut rfp, &task_id, "alice", 1, &plan);

    // With expected_proposers=1, auto-transitions to RevealPhase.
    assert_eq!(*rfp.phase(), RfpPhase::RevealPhase);
    assert_eq!(rfp.commit_count(), 1);

    // Reveal the full plan.
    rfp.record_reveal(&ProposalRevealParams {
        task_id: task_id.clone(),
        plan: plan.clone(),
    })
    .unwrap();

    // With all commits revealed, auto-transitions to ReadyForVoting.
    assert_eq!(*rfp.phase(), RfpPhase::ReadyForVoting);
    assert_eq!(rfp.reveal_count(), 1);

    // Finalize and retrieve proposals.
    let proposals = rfp.finalize().unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].proposer, AgentId::new("alice".into()));
    assert_eq!(*rfp.phase(), RfpPhase::Completed);
}

#[test]
fn rfp_full_lifecycle_multiple_proposers() {
    let task = Task::new("Multi-agent task".into(), 1, 1);
    let task_id = task.task_id.clone();
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 3);

    rfp.inject_task(&task).unwrap();

    // Create three plans from different proposers.
    let plan_a = make_plan(&task_id, "alice", 1);
    let plan_b = make_plan(&task_id, "bob", 1);
    let plan_c = make_plan(&task_id, "carol", 1);

    // Commit all three.
    commit_plan(&mut rfp, &task_id, "alice", 1, &plan_a);
    assert_eq!(*rfp.phase(), RfpPhase::CommitPhase); // not yet all committed
    commit_plan(&mut rfp, &task_id, "bob", 1, &plan_b);
    assert_eq!(*rfp.phase(), RfpPhase::CommitPhase);
    commit_plan(&mut rfp, &task_id, "carol", 1, &plan_c);
    // After 3rd commit, auto-transitions.
    assert_eq!(*rfp.phase(), RfpPhase::RevealPhase);
    assert_eq!(rfp.commit_count(), 3);

    // Reveal all three.
    rfp.record_reveal(&ProposalRevealParams {
        task_id: task_id.clone(),
        plan: plan_a,
    })
    .unwrap();
    rfp.record_reveal(&ProposalRevealParams {
        task_id: task_id.clone(),
        plan: plan_b,
    })
    .unwrap();
    rfp.record_reveal(&ProposalRevealParams {
        task_id: task_id.clone(),
        plan: plan_c,
    })
    .unwrap();

    assert_eq!(*rfp.phase(), RfpPhase::ReadyForVoting);
    assert_eq!(rfp.reveal_count(), 3);

    let proposals = rfp.finalize().unwrap();
    assert_eq!(proposals.len(), 3);
}

// ═══════════════════════════════════════════════════════════════
// Section 6.3  Hash Verification
// ═══════════════════════════════════════════════════════════════

#[test]
fn rfp_hash_mismatch_rejected() {
    let task = Task::new("Hash test".into(), 1, 1);
    let task_id = task.task_id.clone();
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 1);
    rfp.inject_task(&task).unwrap();

    // Commit with a fabricated hash.
    rfp.record_commit(&ProposalCommitParams {
        task_id: task_id.clone(),
        proposer: AgentId::new("alice".into()),
        epoch: 1,
        plan_hash: "fake_hash_that_will_not_match".into(),
    })
    .unwrap();

    // Reveal with a real plan whose hash differs from "fake_hash...".
    let plan = make_plan(&task_id, "alice", 1);
    let result = rfp.record_reveal(&ProposalRevealParams {
        task_id: task_id.clone(),
        plan,
    });

    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), ConsensusError::HashMismatch { .. }),
        "Mismatched hash must be rejected"
    );
}

#[test]
fn rfp_correct_hash_accepted() {
    let task = Task::new("Hash OK".into(), 1, 1);
    let task_id = task.task_id.clone();
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 1);
    rfp.inject_task(&task).unwrap();

    let plan = make_plan(&task_id, "alice", 1);
    let hash = RfpCoordinator::compute_plan_hash(&plan).unwrap();

    rfp.record_commit(&ProposalCommitParams {
        task_id: task_id.clone(),
        proposer: AgentId::new("alice".into()),
        epoch: 1,
        plan_hash: hash,
    })
    .unwrap();

    let result = rfp.record_reveal(&ProposalRevealParams {
        task_id: task_id.clone(),
        plan,
    });
    assert!(result.is_ok(), "Correctly hashed reveal must be accepted");
}

// ═══════════════════════════════════════════════════════════════
// Section 6.3  Duplicate Commit Detection
// ═══════════════════════════════════════════════════════════════

#[test]
fn rfp_duplicate_commit_rejected() {
    let task = Task::new("Dup test".into(), 1, 1);
    let task_id = task.task_id.clone();
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 2);
    rfp.inject_task(&task).unwrap();

    let plan = make_plan(&task_id, "alice", 1);
    let hash = RfpCoordinator::compute_plan_hash(&plan).unwrap();

    // First commit succeeds.
    rfp.record_commit(&ProposalCommitParams {
        task_id: task_id.clone(),
        proposer: AgentId::new("alice".into()),
        epoch: 1,
        plan_hash: hash.clone(),
    })
    .unwrap();

    // Second commit from the same proposer must fail.
    let result = rfp.record_commit(&ProposalCommitParams {
        task_id: task_id.clone(),
        proposer: AgentId::new("alice".into()),
        epoch: 1,
        plan_hash: hash,
    });
    assert!(
        matches!(result, Err(ConsensusError::DuplicateCommit(_, _))),
        "Duplicate commit must be rejected"
    );
}

// ═══════════════════════════════════════════════════════════════
// Phase Transition Enforcement
// ═══════════════════════════════════════════════════════════════

#[test]
fn rfp_cannot_commit_before_inject() {
    let rfp_task_id = "static-task-id".to_string();
    let mut rfp = RfpCoordinator::new(rfp_task_id.clone(), 1, 1);

    // Phase is Idle, so commit should fail.
    let result = rfp.record_commit(&ProposalCommitParams {
        task_id: rfp_task_id,
        proposer: AgentId::new("alice".into()),
        epoch: 1,
        plan_hash: "somehash".into(),
    });
    assert!(result.is_err(), "Cannot commit before injecting a task");
}

#[test]
fn rfp_cannot_inject_task_twice() {
    let task = Task::new("Double inject".into(), 1, 1);
    let task_id = task.task_id.clone();
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 1);

    rfp.inject_task(&task).unwrap();
    let result = rfp.inject_task(&task);
    assert!(
        result.is_err(),
        "Injecting a task when already in CommitPhase must fail"
    );
}

#[test]
fn rfp_cannot_reveal_before_commit_phase_ends() {
    let task = Task::new("Early reveal".into(), 1, 1);
    let task_id = task.task_id.clone();
    // Expect 2 proposers, so one commit will not end the commit phase.
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 2);
    rfp.inject_task(&task).unwrap();

    let plan = make_plan(&task_id, "alice", 1);
    let hash = RfpCoordinator::compute_plan_hash(&plan).unwrap();

    rfp.record_commit(&ProposalCommitParams {
        task_id: task_id.clone(),
        proposer: AgentId::new("alice".into()),
        epoch: 1,
        plan_hash: hash,
    })
    .unwrap();

    // Still in CommitPhase (1 of 2 expected), so reveal should fail.
    assert_eq!(*rfp.phase(), RfpPhase::CommitPhase);
    let result = rfp.record_reveal(&ProposalRevealParams {
        task_id: task_id.clone(),
        plan,
    });
    assert!(
        result.is_err(),
        "Reveal must fail when still in commit phase"
    );
}

#[test]
fn rfp_manual_transition_to_reveal() {
    let task = Task::new("Manual transition".into(), 1, 1);
    let task_id = task.task_id.clone();
    // Expect 3 proposers but only 1 will commit (simulating a timeout scenario).
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 3);
    rfp.inject_task(&task).unwrap();

    let plan = make_plan(&task_id, "alice", 1);
    let hash = RfpCoordinator::compute_plan_hash(&plan).unwrap();

    rfp.record_commit(&ProposalCommitParams {
        task_id: task_id.clone(),
        proposer: AgentId::new("alice".into()),
        epoch: 1,
        plan_hash: hash,
    })
    .unwrap();

    // Still in commit phase since only 1 of 3 committed.
    assert_eq!(*rfp.phase(), RfpPhase::CommitPhase);

    // Manually transition (e.g., on timeout).
    rfp.transition_to_reveal().unwrap();
    assert_eq!(*rfp.phase(), RfpPhase::RevealPhase);

    // Now reveal works.
    rfp.record_reveal(&ProposalRevealParams {
        task_id: task_id.clone(),
        plan,
    })
    .unwrap();
    assert_eq!(*rfp.phase(), RfpPhase::ReadyForVoting);
}

#[test]
fn rfp_transition_to_reveal_without_any_commits_fails() {
    let task = Task::new("No commits".into(), 1, 1);
    let task_id = task.task_id.clone();
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 2);
    rfp.inject_task(&task).unwrap();

    // No commits recorded -- transitioning must fail.
    let result = rfp.transition_to_reveal();
    assert!(
        matches!(result, Err(ConsensusError::NoProposals(_))),
        "Cannot transition to reveal with zero commits"
    );
}

// ═══════════════════════════════════════════════════════════════
// Reveal Without Commit
// ═══════════════════════════════════════════════════════════════

#[test]
fn rfp_reveal_without_commit_rejected() {
    let task = Task::new("No commit reveal".into(), 1, 1);
    let task_id = task.task_id.clone();
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 2);
    rfp.inject_task(&task).unwrap();

    // alice commits.
    let plan_a = make_plan(&task_id, "alice", 1);
    commit_plan(&mut rfp, &task_id, "alice", 1, &plan_a);

    // bob commits so we reach reveal phase.
    let plan_b = make_plan(&task_id, "bob", 1);
    commit_plan(&mut rfp, &task_id, "bob", 1, &plan_b);
    assert_eq!(*rfp.phase(), RfpPhase::RevealPhase);

    // carol (who never committed) tries to reveal.
    let plan_c = make_plan(&task_id, "carol", 1);
    let result = rfp.record_reveal(&ProposalRevealParams {
        task_id: task_id.clone(),
        plan: plan_c,
    });
    assert!(
        result.is_err(),
        "Reveal from an agent who did not commit must be rejected"
    );
}

// ═══════════════════════════════════════════════════════════════
// Finalize Edge Cases
// ═══════════════════════════════════════════════════════════════

#[test]
fn rfp_finalize_in_idle_fails() {
    let mut rfp = RfpCoordinator::new("task-1".to_string(), 1, 1);
    let result = rfp.finalize();
    assert!(result.is_err(), "Finalize in Idle phase must fail");
}

#[test]
fn rfp_finalize_with_no_reveals_fails() {
    let task = Task::new("No reveals".into(), 1, 1);
    let task_id = task.task_id.clone();
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 1);
    rfp.inject_task(&task).unwrap();

    let plan = make_plan(&task_id, "alice", 1);
    commit_plan(&mut rfp, &task_id, "alice", 1, &plan);
    // Phase is RevealPhase now (auto-transitioned), but no reveals yet.

    let result = rfp.finalize();
    assert!(
        matches!(result, Err(ConsensusError::NoProposals(_))),
        "Finalize with no reveals must fail"
    );
}

// ═══════════════════════════════════════════════════════════════
// Epoch Mismatch
// ═══════════════════════════════════════════════════════════════

#[test]
fn rfp_epoch_mismatch_on_commit_rejected() {
    let task = Task::new("Epoch test".into(), 1, 1);
    let task_id = task.task_id.clone();
    let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 1);
    rfp.inject_task(&task).unwrap();

    // Commit with wrong epoch.
    let result = rfp.record_commit(&ProposalCommitParams {
        task_id: task_id.clone(),
        proposer: AgentId::new("alice".into()),
        epoch: 99, // wrong epoch
        plan_hash: "hash".into(),
    });
    assert!(
        matches!(result, Err(ConsensusError::EpochMismatch { .. })),
        "Epoch mismatch on commit must be rejected"
    );
}

// ═══════════════════════════════════════════════════════════════
// Compute Plan Hash Utility
// ═══════════════════════════════════════════════════════════════

#[test]
fn compute_plan_hash_is_deterministic() {
    let plan = make_plan("task-1", "alice", 1);
    let hash1 = RfpCoordinator::compute_plan_hash(&plan).unwrap();
    let hash2 = RfpCoordinator::compute_plan_hash(&plan).unwrap();
    assert_eq!(
        hash1, hash2,
        "Same plan must produce the same hash every time"
    );
    // SHA-256 hex encoding should be 64 characters.
    assert_eq!(hash1.len(), 64, "SHA-256 hex hash must be 64 characters");
}

#[test]
fn different_plans_produce_different_hashes() {
    let plan_a = make_plan("task-1", "alice", 1);
    let plan_b = make_plan("task-1", "bob", 1);
    let hash_a = RfpCoordinator::compute_plan_hash(&plan_a).unwrap();
    let hash_b = RfpCoordinator::compute_plan_hash(&plan_b).unwrap();
    assert_ne!(
        hash_a, hash_b,
        "Different plans must produce different hashes"
    );
}

// ═══════════════════════════════════════════════════════════════
// Metadata Accessors
// ═══════════════════════════════════════════════════════════════

#[test]
fn rfp_metadata_accessors() {
    let rfp = RfpCoordinator::new("task-42".to_string(), 5, 3);
    assert_eq!(rfp.task_id(), "task-42");
    assert_eq!(*rfp.phase(), RfpPhase::Idle);
    assert_eq!(rfp.commit_count(), 0);
    assert_eq!(rfp.reveal_count(), 0);

    let task = Task::new("Meta test".into(), 1, 5);
    // Create a coordinator with the task's generated ID.
    let task_id = task.task_id.clone();
    let mut rfp2 = RfpCoordinator::new(task_id.clone(), 5, 1);
    rfp2.inject_task(&task).unwrap();

    let plan = make_plan(&task_id, "alice", 5);
    commit_plan(&mut rfp2, &task_id, "alice", 5, &plan);
    assert_eq!(rfp2.commit_count(), 1);
}
