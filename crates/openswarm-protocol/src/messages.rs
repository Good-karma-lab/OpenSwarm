use serde::{Deserialize, Serialize};

use crate::constants::JSONRPC_VERSION;
use crate::identity::AgentId;
use crate::types::*;

/// Top-level JSON-RPC 2.0 message envelope.
/// All swarm communications use this format with Ed25519 signatures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmMessage {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub params: serde_json::Value,
    /// Ed25519 signature over the canonical JSON of (method + params)
    pub signature: String,
}

impl SwarmMessage {
    pub fn new(method: &str, params: serde_json::Value, signature: String) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: method.to_string(),
            id: Some(uuid::Uuid::new_v4().to_string()),
            params,
            signature,
        }
    }

    /// Get the canonical bytes for signing: JSON(method + params).
    pub fn signing_payload(method: &str, params: &serde_json::Value) -> Vec<u8> {
        let canonical = serde_json::json!({
            "method": method,
            "params": params,
        });
        serde_json::to_vec(&canonical).unwrap_or_default()
    }
}

/// JSON-RPC response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmResponse {
    pub jsonrpc: String,
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl SwarmResponse {
    pub fn success(id: Option<String>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<String>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ── Specific Message Payloads ──

/// Handshake message sent on peer connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeParams {
    pub agent_id: AgentId,
    pub pub_key: String,
    pub capabilities: Vec<String>,
    pub resources: crate::identity::AgentResources,
    pub location_vector: crate::identity::VivaldiCoordinates,
    pub proof_of_work: ProofOfWork,
    pub protocol_version: String,
}

/// Candidacy announcement for Tier-1 election.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidacyParams {
    pub agent_id: AgentId,
    pub epoch: u64,
    pub score: crate::identity::NodeScore,
    pub location_vector: crate::identity::VivaldiCoordinates,
}

/// Election vote for a Tier-1 candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectionVoteParams {
    pub voter: AgentId,
    pub epoch: u64,
    pub candidate_rankings: Vec<AgentId>,
}

/// Tier assignment notification from parent to subordinate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierAssignmentParams {
    pub assigned_agent: AgentId,
    pub tier: Tier,
    pub parent_id: AgentId,
    pub epoch: u64,
    pub branch_size: u64,
}

/// Task injection from external source or parent agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInjectionParams {
    pub task: Task,
    pub originator: AgentId,
}

/// Commit phase of proposal (hash only, plan hidden).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalCommitParams {
    pub task_id: String,
    pub proposer: AgentId,
    pub epoch: u64,
    /// SHA-256 hash of the full plan JSON
    pub plan_hash: String,
}

/// Reveal phase of proposal (full plan disclosed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalRevealParams {
    pub task_id: String,
    pub plan: Plan,
}

/// Ranked Choice Vote for plan selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusVoteParams {
    pub task_id: String,
    pub epoch: u64,
    pub voter: AgentId,
    pub rankings: Vec<String>,
    pub critic_scores: std::collections::HashMap<String, CriticScore>,
}

/// Task assignment from coordinator to subordinate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignmentParams {
    pub task: Task,
    pub assignee: AgentId,
    pub parent_task_id: String,
    pub winning_plan_id: String,
}

/// Result submission from executor to coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultSubmissionParams {
    pub task_id: String,
    pub agent_id: AgentId,
    pub artifact: Artifact,
    pub merkle_proof: Vec<String>,
}

/// Verification result from coordinator back to subordinate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResultParams {
    pub task_id: String,
    pub agent_id: AgentId,
    pub accepted: bool,
    pub reason: Option<String>,
}

/// Keep-alive ping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeepAliveParams {
    pub agent_id: AgentId,
    pub epoch: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Succession announcement when a leader fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessionParams {
    pub failed_leader: AgentId,
    pub new_leader: AgentId,
    pub epoch: u64,
    pub branch_agents: Vec<AgentId>,
}

// ── Swarm Identity Messages ──

/// Announce the existence of a swarm to the network (via DHT + GossipSub).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmAnnounceParams {
    pub swarm_id: SwarmId,
    pub name: String,
    pub is_public: bool,
    pub agent_id: AgentId,
    pub agent_count: u64,
    pub description: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Request to join a swarm. For private swarms, includes token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmJoinParams {
    pub swarm_id: SwarmId,
    pub agent_id: AgentId,
    /// Token for private swarm authentication (None for public swarms).
    pub token: Option<SwarmToken>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Response to a join request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmJoinResponseParams {
    pub swarm_id: SwarmId,
    pub agent_id: AgentId,
    pub accepted: bool,
    pub reason: Option<String>,
}

/// Leave a swarm notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmLeaveParams {
    pub swarm_id: SwarmId,
    pub agent_id: AgentId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Enumeration of all protocol methods for pattern matching.
#[derive(Debug, Clone)]
pub enum ProtocolMethod {
    Handshake,
    Candidacy,
    ElectionVote,
    TierAssignment,
    TaskInjection,
    ProposalCommit,
    ProposalReveal,
    ConsensusVote,
    TaskAssignment,
    ResultSubmission,
    VerificationResult,
    KeepAlive,
    AgentKeepAlive,
    Succession,
    SwarmAnnounce,
    SwarmJoin,
    SwarmJoinResponse,
    SwarmLeave,
}

impl ProtocolMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Handshake => "swarm.handshake",
            Self::Candidacy => "election.candidacy",
            Self::ElectionVote => "election.vote",
            Self::TierAssignment => "hierarchy.assign_tier",
            Self::TaskInjection => "task.inject",
            Self::ProposalCommit => "consensus.proposal_commit",
            Self::ProposalReveal => "consensus.proposal_reveal",
            Self::ConsensusVote => "consensus.vote",
            Self::TaskAssignment => "task.assign",
            Self::ResultSubmission => "task.submit_result",
            Self::VerificationResult => "task.verification",
            Self::KeepAlive => "swarm.keepalive",
            Self::AgentKeepAlive => "agent.keepalive",
            Self::Succession => "hierarchy.succession",
            Self::SwarmAnnounce => "swarm.announce",
            Self::SwarmJoin => "swarm.join",
            Self::SwarmJoinResponse => "swarm.join_response",
            Self::SwarmLeave => "swarm.leave",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "swarm.handshake" => Some(Self::Handshake),
            "election.candidacy" => Some(Self::Candidacy),
            "election.vote" => Some(Self::ElectionVote),
            "hierarchy.assign_tier" => Some(Self::TierAssignment),
            "task.inject" => Some(Self::TaskInjection),
            "consensus.proposal_commit" => Some(Self::ProposalCommit),
            "consensus.proposal_reveal" => Some(Self::ProposalReveal),
            "consensus.vote" => Some(Self::ConsensusVote),
            "task.assign" => Some(Self::TaskAssignment),
            "task.submit_result" => Some(Self::ResultSubmission),
            "task.verification" => Some(Self::VerificationResult),
            "swarm.keepalive" => Some(Self::KeepAlive),
            "agent.keepalive" => Some(Self::AgentKeepAlive),
            "hierarchy.succession" => Some(Self::Succession),
            "swarm.announce" => Some(Self::SwarmAnnounce),
            "swarm.join" => Some(Self::SwarmJoin),
            "swarm.join_response" => Some(Self::SwarmJoinResponse),
            "swarm.leave" => Some(Self::SwarmLeave),
            _ => None,
        }
    }
}

/// GossipSub topics used by the protocol.
///
/// All topics are namespaced by swarm_id to isolate communication between
/// different swarms on the same network. The default public swarm uses
/// "public" as its swarm_id.
pub struct SwarmTopics;

impl SwarmTopics {
    /// Global swarm discovery topic (shared across all swarms).
    pub fn swarm_discovery() -> String {
        format!("{}/swarm/discovery", crate::constants::TOPIC_PREFIX)
    }

    /// Swarm-specific announcement topic.
    pub fn swarm_announce(swarm_id: &str) -> String {
        format!("{}/swarm/{}/announce", crate::constants::TOPIC_PREFIX, swarm_id)
    }

    pub fn election_tier1() -> String {
        Self::election_tier1_for(crate::constants::DEFAULT_SWARM_ID)
    }

    pub fn election_tier1_for(swarm_id: &str) -> String {
        format!("{}/s/{}/election/tier1", crate::constants::TOPIC_PREFIX, swarm_id)
    }

    pub fn proposals(task_id: &str) -> String {
        Self::proposals_for(crate::constants::DEFAULT_SWARM_ID, task_id)
    }

    pub fn proposals_for(swarm_id: &str, task_id: &str) -> String {
        format!("{}/s/{}/proposals/{}", crate::constants::TOPIC_PREFIX, swarm_id, task_id)
    }

    pub fn voting(task_id: &str) -> String {
        Self::voting_for(crate::constants::DEFAULT_SWARM_ID, task_id)
    }

    pub fn voting_for(swarm_id: &str, task_id: &str) -> String {
        format!("{}/s/{}/voting/{}", crate::constants::TOPIC_PREFIX, swarm_id, task_id)
    }

    pub fn tasks(tier: u32) -> String {
        Self::tasks_for(crate::constants::DEFAULT_SWARM_ID, tier)
    }

    pub fn tasks_for(swarm_id: &str, tier: u32) -> String {
        format!("{}/s/{}/tasks/tier{}", crate::constants::TOPIC_PREFIX, swarm_id, tier)
    }

    pub fn results(task_id: &str) -> String {
        Self::results_for(crate::constants::DEFAULT_SWARM_ID, task_id)
    }

    pub fn results_for(swarm_id: &str, task_id: &str) -> String {
        format!("{}/s/{}/results/{}", crate::constants::TOPIC_PREFIX, swarm_id, task_id)
    }

    pub fn keepalive() -> String {
        Self::keepalive_for(crate::constants::DEFAULT_SWARM_ID)
    }

    pub fn keepalive_for(swarm_id: &str) -> String {
        format!("{}/s/{}/keepalive", crate::constants::TOPIC_PREFIX, swarm_id)
    }

    pub fn hierarchy() -> String {
        Self::hierarchy_for(crate::constants::DEFAULT_SWARM_ID)
    }

    pub fn hierarchy_for(swarm_id: &str) -> String {
        format!("{}/s/{}/hierarchy", crate::constants::TOPIC_PREFIX, swarm_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swarm_message_serialization() {
        let msg = SwarmMessage::new(
            "swarm.handshake",
            serde_json::json!({"agent_id": "did:swarm:abc"}),
            "sig_placeholder".to_string(),
        );
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("swarm.handshake"));

        let parsed: SwarmMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.method, "swarm.handshake");
    }

    #[test]
    fn test_protocol_method_roundtrip() {
        let methods = vec![
            ProtocolMethod::Handshake,
            ProtocolMethod::Candidacy,
            ProtocolMethod::ConsensusVote,
            ProtocolMethod::ResultSubmission,
            ProtocolMethod::AgentKeepAlive,
        ];
        for method in methods {
            let s = method.as_str();
            let parsed = ProtocolMethod::from_str(s);
            assert!(parsed.is_some(), "Failed to parse: {}", s);
        }
    }

    #[test]
    fn test_response_success() {
        let resp = SwarmResponse::success(Some("id-1".into()), serde_json::json!({"ok": true}));
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }

    #[test]
    fn test_response_error() {
        let resp = SwarmResponse::error(Some("id-2".into()), -32600, "Invalid Request".into());
        assert!(resp.result.is_none());
        assert_eq!(resp.error.as_ref().unwrap().code, -32600);
    }

    #[test]
    fn test_swarm_protocol_methods_roundtrip() {
        let methods = vec![
            ProtocolMethod::SwarmAnnounce,
            ProtocolMethod::SwarmJoin,
            ProtocolMethod::SwarmJoinResponse,
            ProtocolMethod::SwarmLeave,
        ];
        for method in methods {
            let s = method.as_str();
            let parsed = ProtocolMethod::from_str(s);
            assert!(parsed.is_some(), "Failed to parse: {}", s);
        }
    }

    #[test]
    fn test_swarm_namespaced_topics() {
        let default_keepalive = SwarmTopics::keepalive();
        let custom_keepalive = SwarmTopics::keepalive_for("my-swarm");

        assert!(default_keepalive.contains("/s/public/"));
        assert!(custom_keepalive.contains("/s/my-swarm/"));
        assert_ne!(default_keepalive, custom_keepalive);
    }

    #[test]
    fn test_swarm_discovery_topic() {
        let topic = SwarmTopics::swarm_discovery();
        assert!(topic.contains("swarm/discovery"));
    }
}
