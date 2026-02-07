//! OpenSwarm Hierarchy - Dynamic Pyramid Allocation and role management
//!
//! Implements the hierarchical organization of agents in the swarm:
//! - Dynamic pyramid allocation based on swarm size N and branching factor k
//! - Weighted reputation-based elections for Tier-1 leaders
//! - Geo-clustering via Vivaldi coordinates for latency-optimal assignment
//! - Leader failover with 30-second succession timeout
//! - Epoch management for periodic re-elections

pub mod elections;
pub mod epoch;
pub mod geo_cluster;
pub mod pyramid;
pub mod succession;

pub use elections::ElectionManager;
pub use epoch::EpochManager;
pub use geo_cluster::GeoCluster;
pub use pyramid::PyramidAllocator;
pub use succession::SuccessionManager;

use thiserror::Error;

/// Errors originating from the hierarchy layer.
#[derive(Error, Debug)]
pub enum HierarchyError {
    #[error("Election failed: {0}")]
    ElectionFailed(String),

    #[error("No candidates available for election")]
    NoCandidates,

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Invalid tier assignment: {0}")]
    InvalidTier(String),

    #[error("Epoch mismatch: expected {expected}, got {got}")]
    EpochMismatch { expected: u64, got: u64 },

    #[error("Leader timeout for agent: {0}")]
    LeaderTimeout(String),

    #[error("Succession in progress")]
    SuccessionInProgress,

    #[error("Hierarchy depth exceeded maximum of {0}")]
    MaxDepthExceeded(u32),

    #[error("Serialization error: {0}")]
    Serialization(String),
}
