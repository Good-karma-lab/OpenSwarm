use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::identity::AgentId;

/// Tier in the dynamic pyramid hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    /// Top-level orchestrators (High Command)
    Tier1,
    /// Mid-level coordinators
    Tier2,
    /// General tier at specified depth
    TierN(u32),
    /// Leaf executors (bottom of hierarchy)
    Executor,
}

impl Tier {
    pub fn depth(&self) -> u32 {
        match self {
            Tier::Tier1 => 1,
            Tier::Tier2 => 2,
            Tier::TierN(n) => *n,
            Tier::Executor => u32::MAX,
        }
    }
}

/// Current status of a task in the swarm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task has been created but not yet assigned
    Pending,
    /// RFP phase: proposals being collected
    ProposalPhase,
    /// Voting phase: ranked choice voting in progress
    VotingPhase,
    /// Task has been assigned and is being executed
    InProgress,
    /// Task completed successfully
    Completed,
    /// Task failed and may be reassigned
    Failed,
    /// Task was rejected during verification
    Rejected,
}

/// A task in the swarm hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub parent_task_id: Option<String>,
    pub epoch: u64,
    pub status: TaskStatus,
    pub description: String,
    pub assigned_to: Option<AgentId>,
    pub tier_level: u32,
    pub subtasks: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
}

impl Task {
    pub fn new(description: String, tier_level: u32, epoch: u64) -> Self {
        Self {
            task_id: Uuid::new_v4().to_string(),
            parent_task_id: None,
            epoch,
            status: TaskStatus::Pending,
            description,
            assigned_to: None,
            tier_level,
            subtasks: Vec::new(),
            created_at: chrono::Utc::now(),
            deadline: None,
        }
    }
}

/// A High-Level Decomposition Plan proposed by a Tier-1 agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub plan_id: String,
    pub task_id: String,
    pub proposer: AgentId,
    pub epoch: u64,
    pub subtasks: Vec<PlanSubtask>,
    pub rationale: String,
    pub estimated_parallelism: f64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Plan {
    pub fn new(task_id: String, proposer: AgentId, epoch: u64) -> Self {
        Self {
            plan_id: Uuid::new_v4().to_string(),
            task_id,
            proposer,
            epoch,
            subtasks: Vec::new(),
            rationale: String::new(),
            estimated_parallelism: 1.0,
            created_at: chrono::Utc::now(),
        }
    }
}

/// A subtask within a decomposition plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSubtask {
    pub index: u32,
    pub description: String,
    pub required_capabilities: Vec<String>,
    pub estimated_complexity: f64,
}

/// Result artifact from task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub artifact_id: String,
    pub task_id: String,
    pub producer: AgentId,
    /// Content-addressed ID (SHA-256 hash of content)
    pub content_cid: String,
    /// Merkle hash for verification chain
    pub merkle_hash: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Critic evaluation scores for a plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticScore {
    pub feasibility: f64,
    pub parallelism: f64,
    pub completeness: f64,
    pub risk: f64,
}

impl CriticScore {
    /// Compute a weighted aggregate score.
    pub fn aggregate(&self) -> f64 {
        0.30 * self.feasibility + 0.25 * self.parallelism + 0.30 * self.completeness
            + 0.15 * (1.0 - self.risk)
    }
}

/// Ranked choice vote from an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedVote {
    pub voter: AgentId,
    pub task_id: String,
    pub epoch: u64,
    /// Plan IDs ranked from most preferred to least preferred
    pub rankings: Vec<String>,
    pub critic_scores: std::collections::HashMap<String, CriticScore>,
}

/// Epoch metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Epoch {
    pub epoch_number: u64,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub duration_secs: u64,
    pub tier1_leaders: Vec<AgentId>,
    pub estimated_swarm_size: u64,
}

/// Network statistics observable by any agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    /// Estimated total agents in the swarm (N)
    pub total_agents: u64,
    /// Current hierarchy depth
    pub hierarchy_depth: u32,
    /// Branching factor (k)
    pub branching_factor: u32,
    /// Current epoch
    pub current_epoch: u64,
    /// This agent's tier assignment
    pub my_tier: Tier,
    /// Number of direct subordinates
    pub subordinate_count: u32,
    /// Parent agent ID (None if Tier-1)
    pub parent_id: Option<AgentId>,
}

/// Proof of Work entry proof submitted during handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofOfWork {
    pub nonce: u64,
    pub hash: String,
    pub difficulty: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task::new("Test task".into(), 1, 1);
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.parent_task_id.is_none());
        assert!(task.subtasks.is_empty());
    }

    #[test]
    fn test_critic_score_aggregate() {
        let score = CriticScore {
            feasibility: 0.9,
            parallelism: 0.8,
            completeness: 0.85,
            risk: 0.2,
        };
        let expected = 0.30 * 0.9 + 0.25 * 0.8 + 0.30 * 0.85 + 0.15 * 0.8;
        assert!((score.aggregate() - expected).abs() < 1e-10);
    }

    #[test]
    fn test_tier_ordering() {
        assert!(Tier::Tier1.depth() < Tier::Tier2.depth());
        assert!(Tier::Tier2.depth() < Tier::TierN(3).depth());
    }
}
