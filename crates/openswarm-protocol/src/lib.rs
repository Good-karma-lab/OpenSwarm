//! ASCP - Core types and message definitions
//!
//! Implements the Agent Swarm Communication Protocol (ASCP) message specification
//! using JSON-RPC 2.0 envelope format with Ed25519 signatures.

pub mod identity;
pub mod messages;
pub mod types;
pub mod error;
pub mod constants;
pub mod crypto;

pub use identity::*;
pub use messages::*;
pub use types::*;
pub use error::*;
pub use constants::*;
