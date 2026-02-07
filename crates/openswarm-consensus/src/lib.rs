//! OpenSwarm Consensus - Competitive planning and Ranked Choice Voting
//!
//! Implements the consensus mechanisms for the OpenSwarm protocol:
//! - Request for Proposal (RFP) protocol with commit-reveal scheme
//! - Ranked Choice Voting with Instant Runoff Voting (IRV)
//! - Recursive decomposition cascade for multi-tier task distribution

pub mod cascade;
pub mod rfp;
pub mod voting;

pub use cascade::CascadeEngine;
pub use rfp::{PlanGenerator, RfpCoordinator};
pub use voting::VotingEngine;

use thiserror::Error;

/// Errors originating from the consensus layer.
#[derive(Error, Debug)]
pub enum ConsensusError {
    #[error("RFP failed: {0}")]
    RfpFailed(String),

    #[error("Proposal already committed for task {0} by agent {1}")]
    DuplicateCommit(String, String),

    #[error("Proposal hash mismatch: expected {expected}, got {got}")]
    HashMismatch { expected: String, got: String },

    #[error("Commit-reveal timeout for task {0}")]
    CommitRevealTimeout(String),

    #[error("Voting error: {0}")]
    VotingError(String),

    #[error("Self-vote not allowed: agent {0} cannot vote for own proposal")]
    SelfVoteProhibited(String),

    #[error("No proposals available for voting on task {0}")]
    NoProposals(String),

    #[error("No votes received for task {0}")]
    NoVotes(String),

    #[error("Cascade error: {0}")]
    CascadeError(String),

    #[error("Plan generation failed: {0}")]
    PlanGenerationFailed(String),

    #[error("Epoch mismatch: expected {expected}, got {got}")]
    EpochMismatch { expected: u64, got: u64 },

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}
