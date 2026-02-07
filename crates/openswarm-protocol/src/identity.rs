use serde::{Deserialize, Serialize};

/// Unique identifier for an agent in the swarm.
/// Format: did:swarm:<sha256_hex_of_public_key>
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Agent capabilities advertised during handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilities {
    /// List of supported model providers (e.g., "gpt-4", "claude-3")
    pub models: Vec<String>,
    /// List of executable skills (e.g., "python-exec", "web-search")
    pub skills: Vec<String>,
}

/// Hardware resources available on this agent's host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResources {
    pub cpu_cores: u32,
    pub ram_gb: u32,
    pub gpu_vram_gb: Option<u32>,
    pub disk_gb: Option<u32>,
}

/// Vivaldi network coordinates for latency estimation.
/// 3D vector where distance approximates RTT between nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VivaldiCoordinates {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl VivaldiCoordinates {
    pub fn origin() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }

    /// Euclidean distance to another coordinate (estimates RTT in ms).
    pub fn distance_to(&self, other: &VivaldiCoordinates) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Update coordinates based on observed RTT to a peer.
    /// Uses simplified Vivaldi algorithm with adaptive timestep.
    pub fn update(&mut self, peer: &VivaldiCoordinates, observed_rtt_ms: f64, weight: f64) {
        let estimated = self.distance_to(peer);
        let error = observed_rtt_ms - estimated;
        let delta = if estimated > 0.0 {
            weight * error / estimated
        } else {
            weight * 0.1
        };
        self.x += delta * (self.x - peer.x);
        self.y += delta * (self.y - peer.y);
        self.z += delta * (self.z - peer.z);
    }
}

/// Full agent profile broadcast during handshake and elections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub agent_id: AgentId,
    /// Base58-encoded Ed25519 public key
    pub pub_key: String,
    pub capabilities: AgentCapabilities,
    pub resources: AgentResources,
    pub location_vector: VivaldiCoordinates,
}

/// Weighted score used for elections and role allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeScore {
    pub agent_id: AgentId,
    /// Proof of Compute benchmark score (0.0 - 1.0)
    pub proof_of_compute: f64,
    /// Historical task success rate (0.0 - 1.0)
    pub reputation: f64,
    /// Uptime fraction in current epoch (0.0 - 1.0)
    pub uptime: f64,
    /// Optional stake to prevent Sybil attacks
    pub stake: Option<f64>,
}

impl NodeScore {
    /// Compute the composite weighted score used for elections.
    /// Weights: PoC=0.25, Reputation=0.40, Uptime=0.20, Stake=0.15
    pub fn composite_score(&self) -> f64 {
        let stake_val = self.stake.unwrap_or(0.0).min(1.0);
        0.25 * self.proof_of_compute
            + 0.40 * self.reputation
            + 0.20 * self.uptime
            + 0.15 * stake_val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vivaldi_distance() {
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
        assert!((a.distance_to(&b) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_composite_score() {
        let score = NodeScore {
            agent_id: AgentId::new("did:swarm:test".into()),
            proof_of_compute: 0.8,
            reputation: 0.9,
            uptime: 1.0,
            stake: Some(0.5),
        };
        let composite = score.composite_score();
        let expected = 0.25 * 0.8 + 0.40 * 0.9 + 0.20 * 1.0 + 0.15 * 0.5;
        assert!((composite - expected).abs() < 1e-10);
    }
}
