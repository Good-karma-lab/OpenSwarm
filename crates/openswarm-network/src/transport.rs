//! Transport configuration using TCP + Noise + Yamux.
//!
//! Builds a libp2p Swarm using the SwarmBuilder API with:
//! - TCP transport for reliable connections
//! - Noise protocol for authenticated encryption
//! - Yamux for stream multiplexing
//! - Optional idle connection timeout

use std::time::Duration;

use libp2p::Swarm;

use crate::behaviour::{BehaviourConfig, SwarmBehaviour};
use crate::NetworkError;

/// Configuration for the transport layer.
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// How long an idle connection stays open before being closed.
    pub idle_connection_timeout: Duration,
    /// Behaviour configuration.
    pub behaviour_config: BehaviourConfig,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            idle_connection_timeout: Duration::from_secs(60),
            behaviour_config: BehaviourConfig::default(),
        }
    }
}

/// Build a fully configured libp2p Swarm with TCP + Noise + Yamux transport
/// and the composite OpenSwarm behaviour.
///
/// The swarm is created with a fresh identity. Returns the swarm ready
/// for listening and dialing.
pub fn build_swarm(config: TransportConfig) -> Result<Swarm<SwarmBehaviour>, NetworkError> {
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let behaviour = SwarmBehaviour::new(&keypair, &config.behaviour_config)?;

    let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .map_err(|e| NetworkError::Transport(format!("TCP transport error: {e}")))?
        .with_behaviour(|_key| behaviour)
        .expect("infallible: behaviour is provided directly")
        .with_swarm_config(|c| {
            c.with_idle_connection_timeout(config.idle_connection_timeout)
        })
        .build();

    Ok(swarm)
}

/// Build a swarm with an existing identity keypair.
///
/// Useful when restoring a node's identity from persistent storage.
pub fn build_swarm_with_keypair(
    keypair: libp2p::identity::Keypair,
    config: TransportConfig,
) -> Result<Swarm<SwarmBehaviour>, NetworkError> {
    let behaviour = SwarmBehaviour::new(&keypair, &config.behaviour_config)?;

    let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .map_err(|e| NetworkError::Transport(format!("TCP transport error: {e}")))?
        .with_behaviour(|_key| behaviour)
        .expect("infallible: behaviour is provided directly")
        .with_swarm_config(|c| {
            c.with_idle_connection_timeout(config.idle_connection_timeout)
        })
        .build();

    Ok(swarm)
}
