use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

// ── Swarm Identity ──

/// Unique identifier for a swarm. The default public swarm uses "public".
/// Private swarms use a generated UUID-based ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SwarmId(pub String);

impl SwarmId {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Create the default public swarm ID.
    pub fn default_public() -> Self {
        Self(crate::constants::DEFAULT_SWARM_ID.to_string())
    }

    /// Generate a new unique swarm ID.
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_public(&self) -> bool {
        self.0 == crate::constants::DEFAULT_SWARM_ID
    }
}

impl std::fmt::Display for SwarmId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Authentication token for joining a private swarm.
/// Generated from HMAC-SHA256(swarm_id, creator_secret).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SwarmToken(pub String);

impl SwarmToken {
    pub fn new(token: String) -> Self {
        Self(token)
    }

    /// Generate a token from a swarm ID and a secret passphrase.
    pub fn generate(swarm_id: &SwarmId, secret: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(swarm_id.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(secret.as_bytes());
        let hash = hasher.finalize();
        Self(hex::encode(hash))
    }

    /// Verify that a token matches a swarm ID and secret.
    pub fn verify(&self, swarm_id: &SwarmId, secret: &str) -> bool {
        let expected = Self::generate(swarm_id, secret);
        self.0 == expected.0
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SwarmToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Only show first 8 chars for security
        if self.0.len() > 8 {
            write!(f, "{}...", &self.0[..8])
        } else {
            write!(f, "{}", self.0)
        }
    }
}

/// Metadata about a swarm, stored in DHT and tracked locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmInfo {
    /// Unique swarm identifier.
    pub swarm_id: SwarmId,
    /// Human-readable name of the swarm.
    pub name: String,
    /// Whether the swarm is public (joinable without token).
    pub is_public: bool,
    /// Number of agents currently in this swarm.
    pub agent_count: u64,
    /// The agent who created this swarm.
    pub creator: AgentId,
    /// When the swarm was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Optional description.
    pub description: String,
}

impl SwarmInfo {
    /// Create a new public swarm info.
    pub fn new_public(creator: AgentId) -> Self {
        Self {
            swarm_id: SwarmId::default_public(),
            name: crate::constants::DEFAULT_SWARM_NAME.to_string(),
            is_public: true,
            agent_count: 1,
            creator,
            created_at: chrono::Utc::now(),
            description: "Default public swarm - open to all agents".to_string(),
        }
    }

    /// Create a new private swarm info.
    pub fn new_private(name: String, creator: AgentId, description: String) -> Self {
        Self {
            swarm_id: SwarmId::generate(),
            name,
            is_public: false,
            agent_count: 1,
            creator,
            created_at: chrono::Utc::now(),
            description,
        }
    }
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

    #[test]
    fn test_swarm_id_default_public() {
        let id = SwarmId::default_public();
        assert_eq!(id.as_str(), "public");
        assert!(id.is_public());
    }

    #[test]
    fn test_swarm_id_generate() {
        let id1 = SwarmId::generate();
        let id2 = SwarmId::generate();
        assert_ne!(id1, id2);
        assert!(!id1.is_public());
    }

    #[test]
    fn test_swarm_token_generate_and_verify() {
        let swarm_id = SwarmId::new("test-swarm".to_string());
        let secret = "my-secret-passphrase";
        let token = SwarmToken::generate(&swarm_id, secret);

        assert!(token.verify(&swarm_id, secret));
        assert!(!token.verify(&swarm_id, "wrong-secret"));
        assert!(!token.verify(&SwarmId::new("other-swarm".to_string()), secret));
    }

    #[test]
    fn test_swarm_token_deterministic() {
        let swarm_id = SwarmId::new("test-swarm".to_string());
        let secret = "my-secret";
        let token1 = SwarmToken::generate(&swarm_id, secret);
        let token2 = SwarmToken::generate(&swarm_id, secret);
        assert_eq!(token1, token2);
    }

    #[test]
    fn test_swarm_info_public() {
        let creator = AgentId::new("did:swarm:test".to_string());
        let info = SwarmInfo::new_public(creator);
        assert!(info.is_public);
        assert!(info.swarm_id.is_public());
        assert_eq!(info.agent_count, 1);
    }

    #[test]
    fn test_swarm_info_private() {
        let creator = AgentId::new("did:swarm:test".to_string());
        let info = SwarmInfo::new_private("My Swarm".to_string(), creator, "desc".to_string());
        assert!(!info.is_public);
        assert!(!info.swarm_id.is_public());
    }
}
