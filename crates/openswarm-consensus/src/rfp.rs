//! Request for Proposal (RFP) protocol: task injection, plan generation,
//! and commit-reveal scheme.
//!
//! The RFP protocol ensures fair plan selection by using a two-phase
//! commit-reveal approach:
//!
//! 1. **Commit Phase**: Each Tier-1 agent generates a plan and publishes
//!    only the SHA-256 hash of the plan. This prevents copying.
//! 2. **Reveal Phase**: After all commits are received (or timeout),
//!    agents reveal their full plans. Plans must match their committed hash.
//! 3. **Evaluation**: Plans are passed to voting for selection.
//!
//! Plan generation is delegated to a `PlanGenerator` trait that abstracts
//! the LLM/AI component, allowing different backends.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use openswarm_protocol::{
    AgentId, Plan, ProposalCommitParams, ProposalRevealParams, Task,
    COMMIT_REVEAL_TIMEOUT_SECS,
};

use crate::ConsensusError;

// ---------------------------------------------------------------------------
// Plan Generator trait
// ---------------------------------------------------------------------------

/// Context provided to the plan generator for creating decomposition plans.
#[derive(Debug, Clone)]
pub struct PlanContext {
    /// The task to decompose.
    pub task: Task,
    /// Current epoch number.
    pub epoch: u64,
    /// Number of agents available at the next tier level.
    pub available_agents: u64,
    /// Branching factor (k) of the hierarchy.
    pub branching_factor: u32,
    /// Capabilities of known agents (for informed decomposition).
    pub known_capabilities: Vec<String>,
}

/// Trait for plan generation, abstracting the LLM/AI component.
///
/// Implementations connect to different AI backends (e.g., GPT-4, Claude, local models)
/// to generate task decomposition plans.
pub trait PlanGenerator: Send + Sync {
    /// Generate a decomposition plan for the given task and context.
    ///
    /// The implementation should:
    /// 1. Analyze the task description
    /// 2. Consider available agents and their capabilities
    /// 3. Produce a set of subtasks that collectively solve the task
    /// 4. Estimate parallelism and complexity
    fn generate_plan<'a>(
        &'a self,
        context: &'a PlanContext,
    ) -> Pin<Box<dyn Future<Output = Result<Plan, ConsensusError>> + Send + 'a>>;
}

// ---------------------------------------------------------------------------
// RFP State Machine
// ---------------------------------------------------------------------------

/// State of an RFP round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RfpPhase {
    /// Waiting for task injection.
    Idle,
    /// Commit phase: collecting plan hashes.
    CommitPhase,
    /// Reveal phase: collecting full plans.
    RevealPhase,
    /// All plans revealed; ready for voting.
    ReadyForVoting,
    /// RFP completed (plan selected).
    Completed,
}

/// A committed but not yet revealed proposal.
#[derive(Debug, Clone)]
struct PendingCommit {
    #[allow(dead_code)]
    proposer: AgentId,
    plan_hash: String,
    #[allow(dead_code)]
    committed_at: DateTime<Utc>,
}

/// A fully revealed proposal.
#[derive(Debug, Clone)]
pub struct RevealedProposal {
    pub proposer: AgentId,
    pub plan: Plan,
    pub plan_hash: String,
}

/// Coordinates the Request for Proposal process for a single task.
///
/// Lifecycle:
/// 1. `inject_task()` - start the RFP
/// 2. `record_commit()` - collect plan hash commits
/// 3. `transition_to_reveal()` - move to reveal phase
/// 4. `record_reveal()` - collect and verify revealed plans
/// 5. `finalize()` - get all verified proposals for voting
pub struct RfpCoordinator {
    task_id: String,
    epoch: u64,
    phase: RfpPhase,
    /// Pending commits (hash only).
    commits: HashMap<AgentId, PendingCommit>,
    /// Verified revealed proposals.
    reveals: HashMap<AgentId, RevealedProposal>,
    /// When the commit phase started.
    commit_started_at: Option<DateTime<Utc>>,
    /// Timeout duration for commit phase.
    commit_timeout_secs: u64,
    /// Expected number of proposers (Tier-1 agents).
    expected_proposers: usize,
}

impl RfpCoordinator {
    /// Create a new RFP coordinator for a task.
    pub fn new(task_id: String, epoch: u64, expected_proposers: usize) -> Self {
        Self {
            task_id,
            epoch,
            phase: RfpPhase::Idle,
            commits: HashMap::new(),
            reveals: HashMap::new(),
            commit_started_at: None,
            commit_timeout_secs: COMMIT_REVEAL_TIMEOUT_SECS,
            expected_proposers,
        }
    }

    /// Start the RFP by injecting a task. Moves to CommitPhase.
    pub fn inject_task(&mut self, task: &Task) -> Result<(), ConsensusError> {
        if self.phase != RfpPhase::Idle {
            return Err(ConsensusError::RfpFailed(format!(
                "Cannot inject task in phase {:?}",
                self.phase
            )));
        }

        if task.task_id != self.task_id {
            return Err(ConsensusError::TaskNotFound(self.task_id.clone()));
        }

        self.phase = RfpPhase::CommitPhase;
        self.commit_started_at = Some(Utc::now());

        tracing::info!(
            task_id = %self.task_id,
            epoch = self.epoch,
            expected_proposers = self.expected_proposers,
            "RFP commit phase started"
        );

        Ok(())
    }

    /// Record a commit (plan hash) from a proposer.
    pub fn record_commit(
        &mut self,
        params: &ProposalCommitParams,
    ) -> Result<(), ConsensusError> {
        if self.phase != RfpPhase::CommitPhase {
            return Err(ConsensusError::RfpFailed(format!(
                "Not in commit phase (currently {:?})",
                self.phase
            )));
        }

        if params.task_id != self.task_id {
            return Err(ConsensusError::TaskNotFound(self.task_id.clone()));
        }

        if params.epoch != self.epoch {
            return Err(ConsensusError::EpochMismatch {
                expected: self.epoch,
                got: params.epoch,
            });
        }

        if self.commits.contains_key(&params.proposer) {
            return Err(ConsensusError::DuplicateCommit(
                self.task_id.clone(),
                params.proposer.to_string(),
            ));
        }

        self.commits.insert(
            params.proposer.clone(),
            PendingCommit {
                proposer: params.proposer.clone(),
                plan_hash: params.plan_hash.clone(),
                committed_at: Utc::now(),
            },
        );

        tracing::debug!(
            task_id = %self.task_id,
            proposer = %params.proposer,
            commits = self.commits.len(),
            expected = self.expected_proposers,
            "Recorded proposal commit"
        );

        // Auto-transition if all expected commits received.
        if self.commits.len() >= self.expected_proposers {
            self.phase = RfpPhase::RevealPhase;
            tracing::info!(
                task_id = %self.task_id,
                "All commits received, transitioning to reveal phase"
            );
        }

        Ok(())
    }

    /// Manually transition to reveal phase (e.g., on timeout).
    pub fn transition_to_reveal(&mut self) -> Result<(), ConsensusError> {
        if self.phase != RfpPhase::CommitPhase {
            return Err(ConsensusError::RfpFailed(format!(
                "Cannot transition to reveal from {:?}",
                self.phase
            )));
        }

        if self.commits.is_empty() {
            return Err(ConsensusError::NoProposals(self.task_id.clone()));
        }

        self.phase = RfpPhase::RevealPhase;
        tracing::info!(
            task_id = %self.task_id,
            commits = self.commits.len(),
            "Transitioning to reveal phase (timeout or manual)"
        );
        Ok(())
    }

    /// Check if the commit phase has timed out.
    pub fn is_commit_timed_out(&self) -> bool {
        if let Some(started) = self.commit_started_at {
            let elapsed = Utc::now()
                .signed_duration_since(started)
                .num_seconds() as u64;
            elapsed >= self.commit_timeout_secs
        } else {
            false
        }
    }

    /// Record a reveal (full plan) from a proposer.
    ///
    /// Verifies that the plan's hash matches the previously committed hash.
    pub fn record_reveal(
        &mut self,
        params: &ProposalRevealParams,
    ) -> Result<(), ConsensusError> {
        if self.phase != RfpPhase::RevealPhase {
            return Err(ConsensusError::RfpFailed(format!(
                "Not in reveal phase (currently {:?})",
                self.phase
            )));
        }

        if params.task_id != self.task_id {
            return Err(ConsensusError::TaskNotFound(self.task_id.clone()));
        }

        let proposer = &params.plan.proposer;

        // Verify the reveal matches the commit.
        let commit = self.commits.get(proposer).ok_or_else(|| {
            ConsensusError::RfpFailed(format!(
                "No commit found for proposer {}",
                proposer
            ))
        })?;

        // Compute hash of the revealed plan.
        let plan_json = serde_json::to_vec(&params.plan)
            .map_err(|e| ConsensusError::Serialization(e.to_string()))?;
        let computed_hash = hex_encode(&Sha256::digest(&plan_json));

        if computed_hash != commit.plan_hash {
            return Err(ConsensusError::HashMismatch {
                expected: commit.plan_hash.clone(),
                got: computed_hash,
            });
        }

        self.reveals.insert(
            proposer.clone(),
            RevealedProposal {
                proposer: proposer.clone(),
                plan: params.plan.clone(),
                plan_hash: computed_hash,
            },
        );

        tracing::debug!(
            task_id = %self.task_id,
            proposer = %proposer,
            reveals = self.reveals.len(),
            "Recorded proposal reveal"
        );

        // Auto-transition if all committed proposals have been revealed.
        if self.reveals.len() >= self.commits.len() {
            self.phase = RfpPhase::ReadyForVoting;
            tracing::info!(
                task_id = %self.task_id,
                proposals = self.reveals.len(),
                "All proposals revealed, ready for voting"
            );
        }

        Ok(())
    }

    /// Finalize the RFP and get all verified proposals for voting.
    pub fn finalize(&mut self) -> Result<Vec<RevealedProposal>, ConsensusError> {
        if self.phase != RfpPhase::ReadyForVoting && self.phase != RfpPhase::RevealPhase {
            return Err(ConsensusError::RfpFailed(format!(
                "Cannot finalize in phase {:?}",
                self.phase
            )));
        }

        if self.reveals.is_empty() {
            return Err(ConsensusError::NoProposals(self.task_id.clone()));
        }

        self.phase = RfpPhase::Completed;

        let proposals: Vec<RevealedProposal> = self.reveals.values().cloned().collect();

        tracing::info!(
            task_id = %self.task_id,
            proposals = proposals.len(),
            "RFP finalized"
        );

        Ok(proposals)
    }

    /// Get the current phase.
    pub fn phase(&self) -> &RfpPhase {
        &self.phase
    }

    /// Get the task ID.
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Get the number of commits received.
    pub fn commit_count(&self) -> usize {
        self.commits.len()
    }

    /// Get the number of reveals received.
    pub fn reveal_count(&self) -> usize {
        self.reveals.len()
    }

    /// Compute the commit hash for a plan (for use by proposers).
    pub fn compute_plan_hash(plan: &Plan) -> Result<String, ConsensusError> {
        let plan_json = serde_json::to_vec(plan)
            .map_err(|e| ConsensusError::Serialization(e.to_string()))?;
        Ok(hex_encode(&Sha256::digest(&plan_json)))
    }
}

/// Hex-encode a byte slice.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openswarm_protocol::PlanSubtask;

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

    #[test]
    fn test_rfp_lifecycle() {
        let task = Task::new("Test task".into(), 1, 1);
        let task_id = task.task_id.clone();
        let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 1);

        // Inject task.
        rfp.inject_task(&task).unwrap();
        assert_eq!(*rfp.phase(), RfpPhase::CommitPhase);

        // Create and commit a plan.
        let plan = make_plan(&task_id, "alice", 1);
        let hash = RfpCoordinator::compute_plan_hash(&plan).unwrap();

        rfp.record_commit(&ProposalCommitParams {
            task_id: task_id.clone(),
            proposer: AgentId::new("alice".into()),
            epoch: 1,
            plan_hash: hash,
        })
        .unwrap();

        // Should auto-transition since expected_proposers = 1.
        assert_eq!(*rfp.phase(), RfpPhase::RevealPhase);

        // Reveal.
        rfp.record_reveal(&ProposalRevealParams {
            task_id: task_id.clone(),
            plan,
        })
        .unwrap();

        assert_eq!(*rfp.phase(), RfpPhase::ReadyForVoting);

        // Finalize.
        let proposals = rfp.finalize().unwrap();
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].proposer, AgentId::new("alice".into()));
    }

    #[test]
    fn test_hash_mismatch_rejected() {
        let task = Task::new("Test".into(), 1, 1);
        let task_id = task.task_id.clone();
        let mut rfp = RfpCoordinator::new(task_id.clone(), 1, 1);
        rfp.inject_task(&task).unwrap();

        // Commit with a fake hash.
        rfp.record_commit(&ProposalCommitParams {
            task_id: task_id.clone(),
            proposer: AgentId::new("alice".into()),
            epoch: 1,
            plan_hash: "fake_hash".into(),
        })
        .unwrap();

        // Reveal with a different plan.
        let plan = make_plan(&task_id, "alice", 1);
        let result = rfp.record_reveal(&ProposalRevealParams {
            task_id: task_id.clone(),
            plan,
        });

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConsensusError::HashMismatch { .. }));
    }
}
