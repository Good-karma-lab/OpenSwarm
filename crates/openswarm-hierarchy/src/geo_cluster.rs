//! Geo-clustering: agents join the Tier-1 leader with lowest latency.
//!
//! Uses Vivaldi coordinates to estimate network latency between agents
//! and leaders without requiring direct measurements. Each agent is
//! assigned to the geographically closest Tier-1 leader to minimize
//! communication latency within their branch.
//!
//! The algorithm:
//! 1. Each Tier-1 leader advertises their Vivaldi coordinates
//! 2. Each agent computes estimated RTT to every leader
//! 3. Agent joins the leader with the minimum estimated RTT
//! 4. If a leader's branch exceeds capacity, overflow agents
//!    are redirected to the next-closest leader

use std::collections::HashMap;

use openswarm_protocol::{AgentId, VivaldiCoordinates};

use crate::HierarchyError;

/// A Tier-1 leader with their location information.
#[derive(Debug, Clone)]
pub struct LeaderLocation {
    pub agent_id: AgentId,
    pub coordinates: VivaldiCoordinates,
    /// Maximum number of subordinates this leader can handle.
    pub capacity: u64,
    /// Current number of assigned subordinates.
    pub current_load: u64,
}

/// Assignment of an agent to a leader with estimated latency.
#[derive(Debug, Clone)]
pub struct ClusterAssignment {
    pub agent_id: AgentId,
    pub leader_id: AgentId,
    pub estimated_rtt_ms: f64,
}

/// Manages geo-aware clustering of agents to Tier-1 leaders.
///
/// Maintains Vivaldi coordinates for all known agents and leaders,
/// and computes optimal assignments based on estimated latencies.
pub struct GeoCluster {
    /// Known leader locations.
    leaders: HashMap<AgentId, LeaderLocation>,
    /// Known agent coordinates.
    agent_coords: HashMap<AgentId, VivaldiCoordinates>,
    /// Current assignments (agent → leader).
    assignments: HashMap<AgentId, ClusterAssignment>,
    /// Maximum allowed distance (RTT in ms) before an agent is considered orphaned.
    #[allow(dead_code)]
    max_rtt_threshold_ms: f64,
}

impl GeoCluster {
    /// Create a new geo-cluster manager.
    pub fn new(max_rtt_threshold_ms: f64) -> Self {
        Self {
            leaders: HashMap::new(),
            agent_coords: HashMap::new(),
            assignments: HashMap::new(),
            max_rtt_threshold_ms,
        }
    }

    /// Register or update a Tier-1 leader's location.
    pub fn register_leader(
        &mut self,
        agent_id: AgentId,
        coordinates: VivaldiCoordinates,
        capacity: u64,
    ) {
        let location = LeaderLocation {
            agent_id: agent_id.clone(),
            coordinates,
            capacity,
            current_load: self
                .leaders
                .get(&agent_id)
                .map(|l| l.current_load)
                .unwrap_or(0),
        };
        self.leaders.insert(agent_id, location);
    }

    /// Remove a leader (e.g., after failover).
    pub fn remove_leader(&mut self, agent_id: &AgentId) {
        self.leaders.remove(agent_id);
        // Orphan all agents assigned to this leader.
        let orphaned: Vec<AgentId> = self
            .assignments
            .iter()
            .filter(|(_, a)| &a.leader_id == agent_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in orphaned {
            self.assignments.remove(&id);
        }
    }

    /// Update an agent's Vivaldi coordinates.
    pub fn update_agent_coordinates(
        &mut self,
        agent_id: AgentId,
        coordinates: VivaldiCoordinates,
    ) {
        self.agent_coords.insert(agent_id, coordinates);
    }

    /// Find the best leader for an agent based on Vivaldi distance.
    ///
    /// Returns the leader with the lowest estimated RTT that still has capacity.
    /// If all leaders are at capacity, returns the closest leader regardless.
    pub fn find_best_leader(
        &self,
        agent_coords: &VivaldiCoordinates,
    ) -> Result<(AgentId, f64), HierarchyError> {
        if self.leaders.is_empty() {
            return Err(HierarchyError::ElectionFailed(
                "No leaders available for clustering".into(),
            ));
        }

        // Compute distance to each leader.
        let mut distances: Vec<(&AgentId, f64, bool)> = self
            .leaders
            .iter()
            .map(|(id, leader)| {
                let dist = agent_coords.distance_to(&leader.coordinates);
                let has_capacity = leader.current_load < leader.capacity;
                (id, dist, has_capacity)
            })
            .collect();

        // Sort by distance.
        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Prefer leaders with capacity.
        if let Some(&(id, dist, _)) = distances.iter().find(|(_, _, cap)| *cap) {
            return Ok((id.clone(), dist));
        }

        // Fallback: closest leader regardless of capacity.
        let (id, dist, _) = distances[0];
        Ok((id.clone(), dist))
    }

    /// Assign an agent to their optimal leader.
    ///
    /// Computes the best leader based on Vivaldi coordinates and updates
    /// the assignment map and leader load counts.
    pub fn assign_agent(
        &mut self,
        agent_id: AgentId,
    ) -> Result<ClusterAssignment, HierarchyError> {
        let coords = self
            .agent_coords
            .get(&agent_id)
            .cloned()
            .unwrap_or_else(VivaldiCoordinates::origin);

        let (leader_id, estimated_rtt) = self.find_best_leader(&coords)?;

        // Update load on the old leader (if reassigning).
        if let Some(old_assignment) = self.assignments.get(&agent_id) {
            if let Some(old_leader) = self.leaders.get_mut(&old_assignment.leader_id) {
                old_leader.current_load = old_leader.current_load.saturating_sub(1);
            }
        }

        // Update load on the new leader.
        if let Some(new_leader) = self.leaders.get_mut(&leader_id) {
            new_leader.current_load += 1;
        }

        let assignment = ClusterAssignment {
            agent_id: agent_id.clone(),
            leader_id,
            estimated_rtt_ms: estimated_rtt,
        };

        self.assignments.insert(agent_id, assignment.clone());

        Ok(assignment)
    }

    /// Rebalance all agent assignments.
    ///
    /// Useful after leader changes (new election, failover) to
    /// re-optimize all assignments.
    pub fn rebalance_all(&mut self) -> Result<Vec<ClusterAssignment>, HierarchyError> {
        // Reset all leader loads.
        for leader in self.leaders.values_mut() {
            leader.current_load = 0;
        }
        self.assignments.clear();

        let agent_ids: Vec<AgentId> = self.agent_coords.keys().cloned().collect();
        let mut results = Vec::with_capacity(agent_ids.len());

        for agent_id in agent_ids {
            let assignment = self.assign_agent(agent_id)?;
            results.push(assignment);
        }

        Ok(results)
    }

    /// Get the current assignment for an agent.
    pub fn get_assignment(&self, agent_id: &AgentId) -> Option<&ClusterAssignment> {
        self.assignments.get(agent_id)
    }

    /// Get all agents assigned to a specific leader.
    pub fn get_branch(&self, leader_id: &AgentId) -> Vec<AgentId> {
        self.assignments
            .iter()
            .filter(|(_, a)| &a.leader_id == leader_id)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get the number of registered leaders.
    pub fn leader_count(&self) -> usize {
        self.leaders.len()
    }

    /// Get all leader IDs.
    pub fn leader_ids(&self) -> Vec<AgentId> {
        self.leaders.keys().cloned().collect()
    }
}

impl Default for GeoCluster {
    fn default() -> Self {
        Self::new(500.0) // 500ms max RTT threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_closest_leader() {
        let mut gc = GeoCluster::default();

        gc.register_leader(
            AgentId::new("leader1".into()),
            VivaldiCoordinates { x: 10.0, y: 0.0, z: 0.0 },
            100,
        );
        gc.register_leader(
            AgentId::new("leader2".into()),
            VivaldiCoordinates { x: -10.0, y: 0.0, z: 0.0 },
            100,
        );

        // Agent at (8, 0, 0) should be closest to leader1
        let agent_coords = VivaldiCoordinates { x: 8.0, y: 0.0, z: 0.0 };
        let (leader, rtt) = gc.find_best_leader(&agent_coords).unwrap();
        assert_eq!(leader, AgentId::new("leader1".into()));
        assert!((rtt - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_assignment_and_load() {
        let mut gc = GeoCluster::default();

        gc.register_leader(
            AgentId::new("leader1".into()),
            VivaldiCoordinates::origin(),
            2,
        );

        gc.update_agent_coordinates(
            AgentId::new("agent1".into()),
            VivaldiCoordinates { x: 1.0, y: 0.0, z: 0.0 },
        );
        gc.update_agent_coordinates(
            AgentId::new("agent2".into()),
            VivaldiCoordinates { x: 2.0, y: 0.0, z: 0.0 },
        );

        gc.assign_agent(AgentId::new("agent1".into())).unwrap();
        gc.assign_agent(AgentId::new("agent2".into())).unwrap();

        let branch = gc.get_branch(&AgentId::new("leader1".into()));
        assert_eq!(branch.len(), 2);
    }
}
