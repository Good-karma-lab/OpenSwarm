use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Unknown method: {0}")]
    UnknownMethod(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Invalid agent ID: {0}")]
    InvalidAgentId(String),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Duplicate proposal")]
    DuplicateProposal,

    #[error("Self-vote not allowed")]
    SelfVoteNotAllowed,

    #[error("Epoch mismatch: expected {expected}, got {got}")]
    EpochMismatch { expected: u64, got: u64 },

    #[error("Insufficient reputation: {0}")]
    InsufficientReputation(f64),

    #[error("Proof of work invalid")]
    InvalidProofOfWork,
}
