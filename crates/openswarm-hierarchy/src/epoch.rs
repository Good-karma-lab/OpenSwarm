//! Epoch management: tracking epoch boundaries and triggering re-elections.
//!
//! An epoch is a time period during which the hierarchy structure is stable.
//! At epoch boundaries:
//! 1. The pyramid layout is recomputed for the current swarm size
//! 2. New Tier-1 elections are triggered
//! 3. Agents are re-assigned to tiers and leaders
//! 4. Stale state is garbage-collected
//!
//! Epochs are numbered monotonically. The first epoch starts at 1.

use chrono::{DateTime, Utc};

use openswarm_protocol::{AgentId, DEFAULT_EPOCH_DURATION_SECS};

/// Configuration for epoch management.
#[derive(Debug, Clone)]
pub struct EpochConfig {
    /// Duration of each epoch in seconds.
    pub duration_secs: u64,
    /// Grace period after epoch boundary before triggering re-election.
    /// Allows late keep-alives to arrive.
    pub grace_period_secs: u64,
}

impl Default for EpochConfig {
    fn default() -> Self {
        Self {
            duration_secs: DEFAULT_EPOCH_DURATION_SECS,
            grace_period_secs: 10,
        }
    }
}

/// Information about the current epoch.
#[derive(Debug, Clone)]
pub struct EpochInfo {
    /// The epoch number (monotonically increasing, starts at 1).
    pub epoch_number: u64,
    /// When this epoch started.
    pub started_at: DateTime<Utc>,
    /// When this epoch is scheduled to end.
    pub ends_at: DateTime<Utc>,
    /// The Tier-1 leaders elected for this epoch.
    pub tier1_leaders: Vec<AgentId>,
    /// Estimated swarm size at the start of this epoch.
    pub estimated_swarm_size: u64,
}

/// Callback actions that should be taken when an epoch transition occurs.
#[derive(Debug, Clone)]
pub enum EpochAction {
    /// A new epoch has started; trigger re-election.
    TriggerElection {
        new_epoch: u64,
        estimated_swarm_size: u64,
    },
    /// The grace period has passed; finalize the epoch transition.
    FinalizeTransition { epoch: u64 },
}

/// Manages epoch lifecycle and transitions.
///
/// Tracks the current epoch, determines when boundaries are crossed,
/// and signals upper layers to trigger re-elections.
pub struct EpochManager {
    config: EpochConfig,
    /// The current epoch info.
    current: EpochInfo,
    /// Whether a transition is in progress (between epoch end and finalization).
    transition_in_progress: bool,
    /// History of past epochs (kept for verification and debugging).
    history: Vec<EpochInfo>,
    /// Maximum history entries to retain.
    max_history: usize,
}

impl EpochManager {
    /// Create a new epoch manager, starting at epoch 1.
    pub fn new(config: EpochConfig) -> Self {
        let now = Utc::now();
        let duration = chrono::Duration::seconds(config.duration_secs as i64);
        let current = EpochInfo {
            epoch_number: 1,
            started_at: now,
            ends_at: now + duration,
            tier1_leaders: Vec::new(),
            estimated_swarm_size: 1,
        };

        Self {
            config,
            current,
            transition_in_progress: false,
            history: Vec::new(),
            max_history: 100,
        }
    }

    /// Check if the current epoch has expired and return any actions needed.
    ///
    /// Should be called periodically (e.g., every second or on each event loop tick).
    pub fn tick(&mut self, estimated_swarm_size: u64) -> Option<EpochAction> {
        let now = Utc::now();

        if self.transition_in_progress {
            // Check if grace period has passed.
            let grace = chrono::Duration::seconds(self.config.grace_period_secs as i64);
            if now > self.current.ends_at + grace {
                self.transition_in_progress = false;
                return Some(EpochAction::FinalizeTransition {
                    epoch: self.current.epoch_number,
                });
            }
            return None;
        }

        if now >= self.current.ends_at {
            self.transition_in_progress = true;
            let new_epoch = self.current.epoch_number + 1;

            tracing::info!(
                old_epoch = self.current.epoch_number,
                new_epoch,
                swarm_size = estimated_swarm_size,
                "Epoch boundary reached"
            );

            return Some(EpochAction::TriggerElection {
                new_epoch,
                estimated_swarm_size,
            });
        }

        None
    }

    /// Advance to a new epoch after election results are known.
    ///
    /// This should be called after the election for the new epoch completes.
    pub fn advance_epoch(
        &mut self,
        tier1_leaders: Vec<AgentId>,
        estimated_swarm_size: u64,
    ) {
        // Archive the current epoch.
        let old = self.current.clone();
        self.history.push(old);
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }

        let now = Utc::now();
        let duration = chrono::Duration::seconds(self.config.duration_secs as i64);
        let new_epoch_number = self.current.epoch_number + 1;

        self.current = EpochInfo {
            epoch_number: new_epoch_number,
            started_at: now,
            ends_at: now + duration,
            tier1_leaders,
            estimated_swarm_size,
        };

        self.transition_in_progress = false;

        tracing::info!(
            epoch = new_epoch_number,
            leaders = self.current.tier1_leaders.len(),
            swarm_size = estimated_swarm_size,
            "Advanced to new epoch"
        );
    }

    /// Force start a specific epoch (used for synchronization).
    pub fn force_epoch(
        &mut self,
        epoch_number: u64,
        tier1_leaders: Vec<AgentId>,
        estimated_swarm_size: u64,
    ) {
        let now = Utc::now();
        let duration = chrono::Duration::seconds(self.config.duration_secs as i64);

        self.current = EpochInfo {
            epoch_number,
            started_at: now,
            ends_at: now + duration,
            tier1_leaders,
            estimated_swarm_size,
        };
        self.transition_in_progress = false;
    }

    /// Get the current epoch number.
    pub fn current_epoch(&self) -> u64 {
        self.current.epoch_number
    }

    /// Get full info about the current epoch.
    pub fn current_info(&self) -> &EpochInfo {
        &self.current
    }

    /// Get the Tier-1 leaders for the current epoch.
    pub fn current_leaders(&self) -> &[AgentId] {
        &self.current.tier1_leaders
    }

    /// Get the remaining time in the current epoch.
    pub fn remaining_time(&self) -> chrono::Duration {
        let now = Utc::now();
        if now >= self.current.ends_at {
            chrono::Duration::zero()
        } else {
            self.current.ends_at - now
        }
    }

    /// Check if a transition is currently in progress.
    pub fn is_transitioning(&self) -> bool {
        self.transition_in_progress
    }

    /// Get the epoch duration configuration.
    pub fn epoch_duration_secs(&self) -> u64 {
        self.config.duration_secs
    }

    /// Look up historical epoch info by epoch number.
    pub fn get_epoch_info(&self, epoch_number: u64) -> Option<&EpochInfo> {
        if epoch_number == self.current.epoch_number {
            return Some(&self.current);
        }
        self.history
            .iter()
            .find(|e| e.epoch_number == epoch_number)
    }

    /// Convert current epoch to the protocol Epoch type.
    pub fn to_protocol_epoch(&self) -> openswarm_protocol::Epoch {
        openswarm_protocol::Epoch {
            epoch_number: self.current.epoch_number,
            started_at: self.current.started_at,
            duration_secs: self.config.duration_secs,
            tier1_leaders: self.current.tier1_leaders.clone(),
            estimated_swarm_size: self.current.estimated_swarm_size,
        }
    }
}

impl Default for EpochManager {
    fn default() -> Self {
        Self::new(EpochConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_epoch() {
        let em = EpochManager::default();
        assert_eq!(em.current_epoch(), 1);
        assert!(em.current_leaders().is_empty());
        assert!(!em.is_transitioning());
    }

    #[test]
    fn test_advance_epoch() {
        let mut em = EpochManager::default();
        let leaders = vec![
            AgentId::new("leader1".into()),
            AgentId::new("leader2".into()),
        ];
        em.advance_epoch(leaders.clone(), 100);
        assert_eq!(em.current_epoch(), 2);
        assert_eq!(em.current_leaders().len(), 2);
    }

    #[test]
    fn test_protocol_epoch_conversion() {
        let em = EpochManager::default();
        let proto = em.to_protocol_epoch();
        assert_eq!(proto.epoch_number, 1);
        assert_eq!(proto.duration_secs, DEFAULT_EPOCH_DURATION_SECS);
    }
}
