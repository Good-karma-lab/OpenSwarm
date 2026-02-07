/// Default branching factor (k) for the pyramidal hierarchy.
/// Each node oversees exactly k subordinate nodes.
pub const DEFAULT_BRANCHING_FACTOR: u32 = 10;

/// Default epoch duration in seconds (1 hour).
pub const DEFAULT_EPOCH_DURATION_SECS: u64 = 3600;

/// Keep-alive interval in seconds.
pub const KEEPALIVE_INTERVAL_SECS: u64 = 10;

/// Leader failover timeout in seconds.
/// If a leader is silent for this duration, succession election triggers.
pub const LEADER_TIMEOUT_SECS: u64 = 30;

/// Commit-Reveal timeout: how long to wait for all proposal hashes.
pub const COMMIT_REVEAL_TIMEOUT_SECS: u64 = 60;

/// Voting phase timeout in seconds.
pub const VOTING_TIMEOUT_SECS: u64 = 120;

/// Maximum hierarchy depth to prevent infinite recursion.
pub const MAX_HIERARCHY_DEPTH: u32 = 10;

/// GossipSub topic prefix.
pub const TOPIC_PREFIX: &str = "/openswarm/1.0.0";

/// JSON-RPC protocol version.
pub const JSONRPC_VERSION: &str = "2.0";

/// Protocol version string.
pub const PROTOCOL_VERSION: &str = "/openswarm/aether/1.0.0";

/// Proof of Work difficulty (number of leading zero bits required).
pub const POW_DIFFICULTY: u32 = 16;
