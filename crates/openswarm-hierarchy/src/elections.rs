//! Weighted Reputation Selection for Tier-1 leaders.
//!
//! Implements the election protocol for selecting Tier-1 leaders (High Command)
//! using a Raft-inspired voting mechanism adapted for large swarms:
//!
//! 1. **Candidacy Phase**: Agents with sufficient reputation announce candidacy
//! 2. **Scoring Phase**: Each candidate's composite score is computed from
//!    PoC benchmark, reputation, uptime, and optional stake
//! 3. **Voting Phase**: Agents rank candidates using weighted reputation selection
//! 4. **Selection Phase**: Top-k candidates by vote weight become Tier-1 leaders
//!
//! The election is deterministic given the same set of candidates and votes,
//! ensuring all honest nodes converge on the same leader set.

use std::collections::HashMap;

use openswarm_protocol::{AgentId, CandidacyParams, ElectionVoteParams, NodeScore};

use crate::HierarchyError;

/// Configuration for the election process.
#[derive(Debug, Clone)]
pub struct ElectionConfig {
    /// Minimum composite score to be eligible as a Tier-1 candidate.
    pub min_candidacy_score: f64,
    /// Minimum uptime fraction required for candidacy.
    pub min_uptime: f64,
    /// Number of Tier-1 leaders to elect (set by pyramid allocator).
    pub tier1_slots: u32,
    /// Maximum number of candidates to consider (prevents DoS).
    pub max_candidates: usize,
}

impl Default for ElectionConfig {
    fn default() -> Self {
        Self {
            min_candidacy_score: 0.3,
            min_uptime: 0.5,
            tier1_slots: 10,
            max_candidates: 100,
        }
    }
}

/// A registered election candidate with their score.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub agent_id: AgentId,
    pub score: NodeScore,
    pub composite: f64,
}

/// Result of an election round.
#[derive(Debug, Clone)]
pub struct ElectionResult {
    /// Epoch this election was for.
    pub epoch: u64,
    /// Elected Tier-1 leaders, ordered by composite score (highest first).
    pub leaders: Vec<AgentId>,
    /// All candidates and their final vote tallies.
    pub tallies: HashMap<AgentId, f64>,
    /// Total number of votes cast.
    pub total_votes: usize,
}

/// Manages the Tier-1 election process for a single epoch.
///
/// The election lifecycle:
/// 1. `register_candidate()` - collect candidacy announcements
/// 2. `record_vote()` - collect votes from all agents
/// 3. `tally_and_elect()` - compute results and select leaders
pub struct ElectionManager {
    config: ElectionConfig,
    current_epoch: u64,
    /// Registered candidates for the current election.
    candidates: HashMap<AgentId, Candidate>,
    /// Votes received, keyed by voter ID.
    votes: HashMap<AgentId, ElectionVoteParams>,
    /// Whether this election has been finalized.
    finalized: bool,
    /// The result after finalization.
    result: Option<ElectionResult>,
}

impl ElectionManager {
    /// Create a new election manager for the given epoch.
    pub fn new(config: ElectionConfig, epoch: u64) -> Self {
        Self {
            config,
            current_epoch: epoch,
            candidates: HashMap::new(),
            votes: HashMap::new(),
            finalized: false,
            result: None,
        }
    }

    /// Register a candidate for Tier-1 election.
    ///
    /// Validates that the candidate meets minimum score and uptime requirements.
    pub fn register_candidate(
        &mut self,
        params: &CandidacyParams,
    ) -> Result<(), HierarchyError> {
        if self.finalized {
            return Err(HierarchyError::ElectionFailed(
                "Election already finalized".into(),
            ));
        }

        if params.epoch != self.current_epoch {
            return Err(HierarchyError::EpochMismatch {
                expected: self.current_epoch,
                got: params.epoch,
            });
        }

        let composite = params.score.composite_score();

        if composite < self.config.min_candidacy_score {
            return Err(HierarchyError::ElectionFailed(format!(
                "Candidate score {:.3} below minimum {:.3}",
                composite, self.config.min_candidacy_score
            )));
        }

        if params.score.uptime < self.config.min_uptime {
            return Err(HierarchyError::ElectionFailed(format!(
                "Candidate uptime {:.3} below minimum {:.3}",
                params.score.uptime, self.config.min_uptime
            )));
        }

        if self.candidates.len() >= self.config.max_candidates {
            // Only replace if new candidate has higher score than the weakest.
            let weakest = self
                .candidates
                .values()
                .min_by(|a, b| a.composite.partial_cmp(&b.composite).unwrap())
                .map(|c| (c.agent_id.clone(), c.composite));

            if let Some((weakest_id, weakest_score)) = weakest {
                if composite > weakest_score {
                    self.candidates.remove(&weakest_id);
                } else {
                    return Err(HierarchyError::ElectionFailed(
                        "Max candidates reached and score is not high enough".into(),
                    ));
                }
            }
        }

        self.candidates.insert(
            params.agent_id.clone(),
            Candidate {
                agent_id: params.agent_id.clone(),
                score: params.score.clone(),
                composite,
            },
        );

        tracing::debug!(
            agent = %params.agent_id,
            score = composite,
            "Registered election candidate"
        );

        Ok(())
    }

    /// Record a vote from an agent.
    ///
    /// Each agent submits a ranked list of preferred candidates.
    /// Duplicate votes from the same agent overwrite previous votes.
    pub fn record_vote(
        &mut self,
        vote: ElectionVoteParams,
    ) -> Result<(), HierarchyError> {
        if self.finalized {
            return Err(HierarchyError::ElectionFailed(
                "Election already finalized".into(),
            ));
        }

        if vote.epoch != self.current_epoch {
            return Err(HierarchyError::EpochMismatch {
                expected: self.current_epoch,
                got: vote.epoch,
            });
        }

        tracing::debug!(
            voter = %vote.voter,
            rankings = vote.candidate_rankings.len(),
            "Recorded election vote"
        );

        self.votes.insert(vote.voter.clone(), vote);
        Ok(())
    }

    /// Tally all votes and elect Tier-1 leaders.
    ///
    /// Uses a weighted Borda count where:
    /// - A candidate ranked #1 gets (C-1) points, #2 gets (C-2), etc.
    ///   where C is the number of candidates ranked by the voter.
    /// - Each voter's points are weighted by their own composite score
    ///   (if they are also a candidate) or weight 1.0 otherwise.
    ///
    /// The top `tier1_slots` candidates by total weighted score are elected.
    pub fn tally_and_elect(&mut self) -> Result<ElectionResult, HierarchyError> {
        if self.candidates.is_empty() {
            return Err(HierarchyError::NoCandidates);
        }

        let mut tallies: HashMap<AgentId, f64> = HashMap::new();

        // Initialize tallies for all candidates.
        for candidate_id in self.candidates.keys() {
            tallies.insert(candidate_id.clone(), 0.0);
        }

        // Process each vote.
        for vote in self.votes.values() {
            // Voter weight: use their candidate score if they are a candidate, else 1.0.
            let voter_weight = self
                .candidates
                .get(&vote.voter)
                .map(|c| c.composite)
                .unwrap_or(1.0);

            let num_rankings = vote.candidate_rankings.len();

            for (rank, candidate_id) in vote.candidate_rankings.iter().enumerate() {
                // Only count votes for registered candidates.
                if let Some(tally) = tallies.get_mut(candidate_id) {
                    // Borda score: (N - rank - 1) points, weighted by voter weight.
                    let borda_points =
                        (num_rankings.saturating_sub(rank + 1)) as f64 * voter_weight;
                    *tally += borda_points;
                }
            }
        }

        // Sort candidates by tally (descending), break ties by composite score.
        let mut ranked: Vec<(AgentId, f64)> = tallies.clone().into_iter().collect();
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| {
                let score_a = self.candidates.get(&a.0).map(|c| c.composite).unwrap_or(0.0);
                let score_b = self.candidates.get(&b.0).map(|c| c.composite).unwrap_or(0.0);
                score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
            })
        });

        // Select the top tier1_slots candidates.
        let leaders: Vec<AgentId> = ranked
            .iter()
            .take(self.config.tier1_slots as usize)
            .map(|(id, _)| id.clone())
            .collect();

        let result = ElectionResult {
            epoch: self.current_epoch,
            leaders,
            tallies,
            total_votes: self.votes.len(),
        };

        self.finalized = true;
        self.result = Some(result.clone());

        tracing::info!(
            epoch = self.current_epoch,
            leaders = result.leaders.len(),
            votes = result.total_votes,
            "Election completed"
        );

        Ok(result)
    }

    /// Get the election result if finalized.
    pub fn result(&self) -> Option<&ElectionResult> {
        self.result.as_ref()
    }

    /// Check if the election has been finalized.
    pub fn is_finalized(&self) -> bool {
        self.finalized
    }

    /// Get the number of registered candidates.
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// Get the number of votes received.
    pub fn vote_count(&self) -> usize {
        self.votes.len()
    }

    /// Get the current epoch of this election.
    pub fn epoch(&self) -> u64 {
        self.current_epoch
    }

    /// Update the number of Tier-1 slots (called when pyramid layout changes).
    pub fn set_tier1_slots(&mut self, slots: u32) {
        self.config.tier1_slots = slots;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openswarm_protocol::VivaldiCoordinates;

    fn make_score(agent: &str, reputation: f64, uptime: f64) -> NodeScore {
        NodeScore {
            agent_id: AgentId::new(agent.to_string()),
            proof_of_compute: 0.8,
            reputation,
            uptime,
            stake: Some(0.5),
        }
    }

    fn make_candidacy(agent: &str, reputation: f64, uptime: f64, epoch: u64) -> CandidacyParams {
        CandidacyParams {
            agent_id: AgentId::new(agent.to_string()),
            epoch,
            score: make_score(agent, reputation, uptime),
            location_vector: VivaldiCoordinates::origin(),
        }
    }

    #[test]
    fn test_register_candidate() {
        let mut em = ElectionManager::new(ElectionConfig::default(), 1);
        let candidacy = make_candidacy("alice", 0.9, 0.8, 1);
        assert!(em.register_candidate(&candidacy).is_ok());
        assert_eq!(em.candidate_count(), 1);
    }

    #[test]
    fn test_reject_low_score() {
        let mut em = ElectionManager::new(ElectionConfig::default(), 1);
        let candidacy = make_candidacy("weak", 0.0, 0.1, 1);
        assert!(em.register_candidate(&candidacy).is_err());
    }

    #[test]
    fn test_election_basic() {
        let config = ElectionConfig {
            tier1_slots: 2,
            ..Default::default()
        };
        let mut em = ElectionManager::new(config, 1);

        // Register 3 candidates
        em.register_candidate(&make_candidacy("alice", 0.9, 0.9, 1)).unwrap();
        em.register_candidate(&make_candidacy("bob", 0.8, 0.8, 1)).unwrap();
        em.register_candidate(&make_candidacy("carol", 0.7, 0.7, 1)).unwrap();

        // Votes
        em.record_vote(ElectionVoteParams {
            voter: AgentId::new("voter1".into()),
            epoch: 1,
            candidate_rankings: vec![
                AgentId::new("alice".into()),
                AgentId::new("bob".into()),
                AgentId::new("carol".into()),
            ],
        }).unwrap();

        em.record_vote(ElectionVoteParams {
            voter: AgentId::new("voter2".into()),
            epoch: 1,
            candidate_rankings: vec![
                AgentId::new("bob".into()),
                AgentId::new("alice".into()),
                AgentId::new("carol".into()),
            ],
        }).unwrap();

        let result = em.tally_and_elect().unwrap();
        assert_eq!(result.leaders.len(), 2);
        assert!(result.total_votes == 2);
    }
}
