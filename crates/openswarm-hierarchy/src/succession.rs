//! Leader failover: keep-alive monitoring and succession election.
//!
//! Monitors Tier-1 leaders via periodic keep-alive messages. If a leader
//! fails to send a keep-alive within the configured timeout (default 30s),
//! a succession election is triggered among the failed leader's branch.
//!
//! Succession protocol:
//! 1. Leader sends keep-alive every KEEPALIVE_INTERVAL_SECS
//! 2. Subordinates track last-seen timestamp for their leader
//! 3. If timeout exceeded, the highest-scored subordinate in the branch
//!    announces succession
//! 4. Branch agents vote to confirm the new leader
//! 5. New leader inherits the branch and notifies the swarm

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use openswarm_protocol::{AgentId, NodeScore, KEEPALIVE_INTERVAL_SECS, LEADER_TIMEOUT_SECS};

use crate::HierarchyError;

/// Status of a monitored leader.
#[derive(Debug, Clone)]
pub struct LeaderStatus {
    pub leader_id: AgentId,
    /// Last time a keep-alive was received from this leader.
    pub last_seen: DateTime<Utc>,
    /// The leader's node score for succession ranking.
    pub score: Option<NodeScore>,
    /// Whether a succession process is in progress.
    pub succession_in_progress: bool,
}

/// A succession candidate within a branch.
#[derive(Debug, Clone)]
pub struct SuccessionCandidate {
    pub agent_id: AgentId,
    pub score: NodeScore,
    /// Number of confirmation votes received.
    pub confirmation_votes: u32,
}

/// Result of a succession process.
#[derive(Debug, Clone)]
pub struct SuccessionResult {
    /// The failed leader being replaced.
    pub failed_leader: AgentId,
    /// The new leader taking over.
    pub new_leader: AgentId,
    /// Agents in the branch that need to be notified.
    pub branch_agents: Vec<AgentId>,
    /// Epoch in which the succession occurred.
    pub epoch: u64,
}

/// Manages leader keep-alive monitoring and succession elections.
pub struct SuccessionManager {
    /// Timeout duration before declaring a leader failed.
    timeout: Duration,
    /// Keep-alive interval for outgoing heartbeats.
    keepalive_interval: Duration,
    /// Tracked leaders and their last-seen times.
    leaders: HashMap<AgentId, LeaderStatus>,
    /// Active succession processes.
    active_successions: HashMap<AgentId, Vec<SuccessionCandidate>>,
    /// Agents in each leader's branch (for succession voting).
    branches: HashMap<AgentId, Vec<AgentId>>,
}

impl SuccessionManager {
    /// Create a new succession manager with default timeouts.
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(LEADER_TIMEOUT_SECS),
            keepalive_interval: Duration::from_secs(KEEPALIVE_INTERVAL_SECS),
            leaders: HashMap::new(),
            active_successions: HashMap::new(),
            branches: HashMap::new(),
        }
    }

    /// Create with custom timeout values.
    pub fn with_timeouts(timeout: Duration, keepalive_interval: Duration) -> Self {
        Self {
            timeout,
            keepalive_interval,
            leaders: HashMap::new(),
            active_successions: HashMap::new(),
            branches: HashMap::new(),
        }
    }

    /// Register a leader to be monitored.
    pub fn monitor_leader(&mut self, leader_id: AgentId, score: Option<NodeScore>) {
        self.leaders.insert(
            leader_id.clone(),
            LeaderStatus {
                leader_id,
                last_seen: Utc::now(),
                score,
                succession_in_progress: false,
            },
        );
    }

    /// Stop monitoring a leader.
    pub fn unmonitor_leader(&mut self, leader_id: &AgentId) {
        self.leaders.remove(leader_id);
        self.active_successions.remove(leader_id);
        self.branches.remove(leader_id);
    }

    /// Record a keep-alive received from a leader.
    pub fn record_keepalive(&mut self, leader_id: &AgentId) {
        if let Some(status) = self.leaders.get_mut(leader_id) {
            status.last_seen = Utc::now();
            // If a succession was in progress but leader came back, cancel it.
            if status.succession_in_progress {
                tracing::info!(
                    leader = %leader_id,
                    "Leader recovered, cancelling succession"
                );
                status.succession_in_progress = false;
                self.active_successions.remove(leader_id);
            }
        }
    }

    /// Update the branch membership for a leader.
    pub fn set_branch(&mut self, leader_id: AgentId, agents: Vec<AgentId>) {
        self.branches.insert(leader_id, agents);
    }

    /// Check all leaders for timeouts and return those that have timed out.
    ///
    /// Should be called periodically (e.g., every keepalive interval).
    pub fn check_timeouts(&mut self) -> Vec<AgentId> {
        let now = Utc::now();
        let timeout_ms = self.timeout.as_millis() as i64;
        let mut timed_out = Vec::new();

        for (leader_id, status) in &self.leaders {
            if status.succession_in_progress {
                continue; // Already handling this leader's failure.
            }

            let elapsed = now
                .signed_duration_since(status.last_seen)
                .num_milliseconds();

            if elapsed > timeout_ms {
                tracing::warn!(
                    leader = %leader_id,
                    elapsed_ms = elapsed,
                    timeout_ms,
                    "Leader timeout detected"
                );
                timed_out.push(leader_id.clone());
            }
        }

        // Mark timed-out leaders as having succession in progress.
        for leader_id in &timed_out {
            if let Some(status) = self.leaders.get_mut(leader_id) {
                status.succession_in_progress = true;
            }
        }

        timed_out
    }

    /// Initiate a succession process for a failed leader.
    ///
    /// Collects candidates from the branch and ranks them by composite score.
    /// The caller should broadcast a succession announcement for the top candidate.
    pub fn initiate_succession(
        &mut self,
        failed_leader: &AgentId,
        branch_scores: Vec<NodeScore>,
    ) -> Result<AgentId, HierarchyError> {
        if branch_scores.is_empty() {
            return Err(HierarchyError::LeaderTimeout(format!(
                "No candidates in branch of {}",
                failed_leader
            )));
        }

        let mut candidates: Vec<SuccessionCandidate> = branch_scores
            .into_iter()
            .map(|score| SuccessionCandidate {
                agent_id: score.agent_id.clone(),
                score,
                confirmation_votes: 0,
            })
            .collect();

        // Sort by composite score, highest first.
        candidates.sort_by(|a, b| {
            b.score
                .composite_score()
                .partial_cmp(&a.score.composite_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let proposed_leader = candidates[0].agent_id.clone();

        tracing::info!(
            failed = %failed_leader,
            proposed = %proposed_leader,
            candidates = candidates.len(),
            "Succession initiated"
        );

        self.active_successions
            .insert(failed_leader.clone(), candidates);

        Ok(proposed_leader)
    }

    /// Record a confirmation vote for a succession candidate.
    ///
    /// Returns `Some(SuccessionResult)` if the candidate has received
    /// enough votes (simple majority of the branch).
    pub fn record_succession_vote(
        &mut self,
        failed_leader: &AgentId,
        candidate_id: &AgentId,
        epoch: u64,
    ) -> Result<Option<SuccessionResult>, HierarchyError> {
        if !self.active_successions.contains_key(failed_leader) {
            return Err(HierarchyError::ElectionFailed(format!(
                "No active succession for {}",
                failed_leader
            )));
        }

        let branch_size = self
            .branches
            .get(failed_leader)
            .map(|b| b.len())
            .unwrap_or(1);
        let majority_threshold = (branch_size / 2) + 1;

        // Find and increment the candidate's vote count, check if confirmed.
        let mut confirmed_votes = None;
        if let Some(candidates) = self.active_successions.get_mut(failed_leader) {
            for candidate in candidates.iter_mut() {
                if &candidate.agent_id == candidate_id {
                    candidate.confirmation_votes += 1;
                    if candidate.confirmation_votes as usize >= majority_threshold {
                        confirmed_votes = Some(candidate.confirmation_votes);
                    }
                    break;
                }
            }
        }

        if let Some(votes) = confirmed_votes {
            let branch_agents = self
                .branches
                .get(failed_leader)
                .cloned()
                .unwrap_or_default();

            let result = SuccessionResult {
                failed_leader: failed_leader.clone(),
                new_leader: candidate_id.clone(),
                branch_agents,
                epoch,
            };

            // Clean up succession state.
            self.active_successions.remove(failed_leader);
            if let Some(status) = self.leaders.get_mut(failed_leader) {
                status.succession_in_progress = false;
            }

            tracing::info!(
                failed = %failed_leader,
                new_leader = %candidate_id,
                votes,
                "Succession confirmed"
            );

            return Ok(Some(result));
        }

        // Check if candidate was actually found
        let found = self.active_successions.get(failed_leader)
            .map(|candidates| candidates.iter().any(|c| &c.agent_id == candidate_id))
            .unwrap_or(false);

        if found {
            return Ok(None);
        }

        Err(HierarchyError::AgentNotFound(format!(
            "Candidate {} not found in succession for {}",
            candidate_id, failed_leader
        )))
    }

    /// Get the keep-alive interval for this manager.
    pub fn keepalive_interval(&self) -> Duration {
        self.keepalive_interval
    }

    /// Get the timeout duration.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Check if a succession is in progress for a leader.
    pub fn is_succession_in_progress(&self, leader_id: &AgentId) -> bool {
        self.active_successions.contains_key(leader_id)
    }
}

impl Default for SuccessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keepalive_resets_timeout() {
        let mut sm = SuccessionManager::new();
        let leader = AgentId::new("leader1".into());
        sm.monitor_leader(leader.clone(), None);

        // Immediately after monitoring, no timeout.
        let timed_out = sm.check_timeouts();
        assert!(timed_out.is_empty());

        // Record keepalive.
        sm.record_keepalive(&leader);
        let timed_out = sm.check_timeouts();
        assert!(timed_out.is_empty());
    }

    #[test]
    fn test_succession_initiation() {
        let mut sm = SuccessionManager::new();
        let leader = AgentId::new("leader1".into());
        sm.monitor_leader(leader.clone(), None);

        let scores = vec![
            NodeScore {
                agent_id: AgentId::new("agent1".into()),
                proof_of_compute: 0.9,
                reputation: 0.9,
                uptime: 1.0,
                stake: Some(0.5),
            },
            NodeScore {
                agent_id: AgentId::new("agent2".into()),
                proof_of_compute: 0.7,
                reputation: 0.8,
                uptime: 0.9,
                stake: Some(0.3),
            },
        ];

        let proposed = sm.initiate_succession(&leader, scores).unwrap();
        // agent1 should have the highest composite score.
        assert_eq!(proposed, AgentId::new("agent1".into()));
    }
}
