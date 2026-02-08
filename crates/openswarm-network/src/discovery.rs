//! Peer discovery: mDNS local scanning, bootstrap nodes, DHT integration.
//!
//! This module handles the three peer discovery mechanisms:
//! 1. **mDNS**: Automatic discovery of peers on the local network
//! 2. **Bootstrap**: Connecting to well-known seed nodes for initial DHT population
//! 3. **Kademlia DHT**: Ongoing peer discovery through random walks

use std::collections::HashSet;
use std::time::Duration;

use libp2p::{Multiaddr, PeerId};

use crate::NetworkError;

/// Configuration for peer discovery.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Bootstrap peer addresses to connect to on startup.
    pub bootstrap_peers: Vec<(PeerId, Multiaddr)>,
    /// Whether mDNS local discovery is enabled.
    pub mdns_enabled: bool,
    /// Interval between Kademlia random walk queries for peer discovery.
    pub kademlia_walk_interval: Duration,
    /// Maximum number of peers to maintain in the routing table.
    pub max_peers: usize,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            bootstrap_peers: Vec::new(),
            mdns_enabled: true,
            kademlia_walk_interval: Duration::from_secs(30),
            max_peers: 1000,
        }
    }
}

/// Manages peer discovery state and orchestrates the discovery process.
///
/// Tracks discovered peers from all sources (mDNS, bootstrap, DHT) and
/// provides methods for initiating discovery actions.
pub struct DiscoveryManager {
    config: DiscoveryConfig,
    /// Set of all known peer IDs regardless of discovery source.
    known_peers: HashSet<PeerId>,
    /// Peers discovered via mDNS.
    mdns_peers: HashSet<PeerId>,
    /// Peers from bootstrap configuration.
    #[allow(dead_code)]
    bootstrap_peers: HashSet<PeerId>,
    /// Whether the initial bootstrap has completed.
    bootstrap_complete: bool,
}

impl DiscoveryManager {
    /// Create a new discovery manager with the given configuration.
    pub fn new(config: DiscoveryConfig) -> Self {
        let bootstrap_peers: HashSet<PeerId> =
            config.bootstrap_peers.iter().map(|(id, _)| *id).collect();
        Self {
            config,
            known_peers: HashSet::new(),
            mdns_peers: HashSet::new(),
            bootstrap_peers,
            bootstrap_complete: false,
        }
    }

    /// Register bootstrap peers in the Kademlia routing table and start bootstrap.
    ///
    /// This should be called once after the swarm is created and listening.
    /// It adds all configured bootstrap peers to Kademlia's routing table
    /// and triggers a bootstrap query to populate the DHT.
    pub fn initiate_bootstrap(
        &mut self,
        kademlia: &mut libp2p::kad::Behaviour<libp2p::kad::store::MemoryStore>,
    ) -> Result<(), NetworkError> {
        for (peer_id, addr) in &self.config.bootstrap_peers {
            kademlia.add_address(peer_id, addr.clone());
            self.known_peers.insert(*peer_id);
            tracing::info!(
                peer = %peer_id,
                addr = %addr,
                "Added bootstrap peer to Kademlia routing table"
            );
        }

        if !self.config.bootstrap_peers.is_empty() {
            kademlia
                .bootstrap()
                .map_err(|e| NetworkError::DhtError(format!("Bootstrap failed: {e}")))?;
            tracing::info!("Kademlia bootstrap initiated");
        }

        Ok(())
    }

    /// Handle an mDNS discovered event: register newly found peers.
    pub fn on_mdns_discovered(&mut self, peers: impl Iterator<Item = (PeerId, Multiaddr)>) {
        for (peer_id, addr) in peers {
            if self.known_peers.insert(peer_id) {
                tracing::info!(peer = %peer_id, addr = %addr, "mDNS discovered new peer");
            }
            self.mdns_peers.insert(peer_id);
        }
    }

    /// Handle an mDNS expired event: mark peers as potentially unreachable.
    pub fn on_mdns_expired(&mut self, peers: impl Iterator<Item = (PeerId, Multiaddr)>) {
        for (peer_id, _addr) in peers {
            self.mdns_peers.remove(&peer_id);
            tracing::debug!(peer = %peer_id, "mDNS peer expired");
        }
    }

    /// Mark the bootstrap process as complete.
    pub fn on_bootstrap_complete(&mut self) {
        self.bootstrap_complete = true;
        tracing::info!("Kademlia bootstrap complete");
    }

    /// Trigger a Kademlia random walk to discover more peers.
    ///
    /// This generates a random PeerId and queries the DHT for it,
    /// which has the side effect of populating routing table buckets.
    pub fn trigger_random_walk(
        &self,
        kademlia: &mut libp2p::kad::Behaviour<libp2p::kad::store::MemoryStore>,
    ) {
        let random_peer = PeerId::random();
        kademlia.get_closest_peers(random_peer);
        tracing::debug!("Initiated Kademlia random walk for peer discovery");
    }

    /// Register a peer discovered through any mechanism.
    pub fn add_peer(&mut self, peer_id: PeerId) {
        self.known_peers.insert(peer_id);
    }

    /// Remove a peer that is no longer reachable.
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.known_peers.remove(peer_id);
        self.mdns_peers.remove(peer_id);
    }

    /// Get the total number of known peers.
    pub fn known_peer_count(&self) -> usize {
        self.known_peers.len()
    }

    /// Get all known peer IDs.
    pub fn known_peers(&self) -> &HashSet<PeerId> {
        &self.known_peers
    }

    /// Check if the initial bootstrap has completed.
    pub fn is_bootstrap_complete(&self) -> bool {
        self.bootstrap_complete
    }

    /// Get the discovery configuration.
    pub fn config(&self) -> &DiscoveryConfig {
        &self.config
    }
}
