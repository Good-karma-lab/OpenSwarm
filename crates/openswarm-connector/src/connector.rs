//! The OpenSwarmConnector struct that ties everything together.
//!
//! Initializes and orchestrates all subsystems: network, hierarchy,
//! consensus, and state management. Provides the high-level API
//! used by the RPC server and agent bridge.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock};

use openswarm_consensus::{CascadeEngine, RfpCoordinator, VotingEngine};
use openswarm_hierarchy::{
    EpochManager, GeoCluster, PyramidAllocator, SuccessionManager,
    elections::ElectionManager,
    epoch::EpochConfig,
    pyramid::PyramidConfig,
};
use openswarm_network::{
    Multiaddr, PeerId,
    NetworkEvent, SwarmHandle, SwarmHost, SwarmHostConfig,
    discovery::DiscoveryConfig,
    transport::TransportConfig,
};
use openswarm_protocol::*;
use openswarm_state::{ContentStore, GranularityAlgorithm, MerkleDag, OrSet};

use crate::config::ConnectorConfig;
use crate::tui::{LogCategory, LogEntry};

/// Information about a known swarm tracked by this connector.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SwarmRecord {
    /// Swarm ID.
    pub swarm_id: SwarmId,
    /// Human-readable name.
    pub name: String,
    /// Whether the swarm is public.
    pub is_public: bool,
    /// Number of agents last reported in this swarm.
    pub agent_count: u64,
    /// Whether this connector is a member of this swarm.
    pub joined: bool,
    /// Last seen timestamp.
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

/// Status of the connector.
#[derive(Debug, Clone)]
pub enum ConnectorStatus {
    /// Initializing subsystems.
    Initializing,
    /// Connected to the swarm and operational.
    Running,
    /// Participating in an election.
    InElection,
    /// Shutting down.
    ShuttingDown,
}

/// Shared state accessible by the RPC server and event handlers.
pub struct ConnectorState {
    /// Our agent identity.
    pub agent_id: AgentId,
    /// Current status.
    pub status: ConnectorStatus,
    /// Epoch manager.
    pub epoch_manager: EpochManager,
    /// Pyramid allocator.
    pub pyramid: PyramidAllocator,
    /// Election manager (current epoch).
    pub election: Option<ElectionManager>,
    /// Geo-cluster manager.
    pub geo_cluster: GeoCluster,
    /// Succession manager.
    pub succession: SuccessionManager,
    /// Active RFP coordinators, keyed by task ID.
    pub rfp_coordinators: std::collections::HashMap<String, RfpCoordinator>,
    /// Active voting engines, keyed by task ID.
    pub voting_engines: std::collections::HashMap<String, VotingEngine>,
    /// Cascade engine for the current root task.
    pub cascade: CascadeEngine,
    /// CRDT set tracking active tasks.
    pub task_set: OrSet<String>,
    /// CRDT set tracking active agents.
    pub agent_set: OrSet<String>,
    /// Merkle DAG for result verification.
    pub merkle_dag: MerkleDag,
    /// Content-addressed storage.
    pub content_store: ContentStore,
    /// Granularity algorithm.
    pub granularity: GranularityAlgorithm,
    /// Current tier assignment for this agent.
    pub my_tier: Tier,
    /// Our parent agent ID (None if Tier-1).
    pub parent_id: Option<AgentId>,
    /// Network statistics cache.
    pub network_stats: NetworkStats,
    /// Event log for the TUI.
    pub event_log: Vec<LogEntry>,
    /// Timestamp when the connector started.
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// The swarm ID this connector is currently a member of.
    pub current_swarm_id: SwarmId,
    /// Registry of all known swarms (discovered via DHT/GossipSub).
    pub known_swarms: std::collections::HashMap<String, SwarmRecord>,
    /// Swarm token for private swarm authentication (if any).
    pub swarm_token: Option<SwarmToken>,
}

impl ConnectorState {
    /// Push a log entry, capping the log at 1000 entries.
    pub fn push_log(&mut self, category: LogCategory, message: String) {
        if self.event_log.len() >= 1000 {
            self.event_log.remove(0);
        }
        self.event_log.push(LogEntry {
            timestamp: chrono::Utc::now(),
            category,
            message,
        });
    }
}

/// The main ASCP Connector that orchestrates all subsystems.
///
/// Created from a configuration, it initializes the network, hierarchy,
/// consensus, and state modules, then runs the event loop that ties
/// them together.
pub struct OpenSwarmConnector {
    /// Shared mutable state.
    pub state: Arc<RwLock<ConnectorState>>,
    /// Network handle for sending commands to the swarm.
    pub network_handle: SwarmHandle,
    /// Channel for receiving network events.
    event_rx: Option<mpsc::Receiver<NetworkEvent>>,
    /// The swarm host (to be spawned).
    swarm_host: Option<SwarmHost>,
    /// Configuration.
    config: ConnectorConfig,
}

impl OpenSwarmConnector {
    /// Create a new connector from configuration.
    ///
    /// Initializes all subsystems but does not start the event loop.
    /// Call `run()` to start processing.
    pub fn new(config: ConnectorConfig) -> Result<Self, anyhow::Error> {
        // Build network configuration.
        let listen_addr = config.network.listen_addr.parse()
            .map_err(|e| anyhow::anyhow!("Invalid listen address: {}", e))?;

        // Parse bootstrap peer multiaddresses into (PeerId, Multiaddr) pairs.
        let bootstrap_peers = Self::parse_bootstrap_peers(&config.network.bootstrap_peers);

        let swarm_config = SwarmHostConfig {
            listen_addr,
            transport: TransportConfig::default(),
            discovery: DiscoveryConfig {
                mdns_enabled: config.network.mdns_enabled,
                bootstrap_peers,
                ..Default::default()
            },
            ..Default::default()
        };

        let (swarm_host, network_handle, event_rx) = SwarmHost::new(swarm_config)?;
        let local_peer_id = network_handle.local_peer_id();
        let agent_id = AgentId::new(format!("did:swarm:{}", local_peer_id));

        // Initialize hierarchy.
        let pyramid_config = PyramidConfig {
            branching_factor: config.hierarchy.branching_factor,
            ..Default::default()
        };
        let epoch_config = EpochConfig {
            duration_secs: config.hierarchy.epoch_duration_secs,
            ..Default::default()
        };

        // Build swarm identity.
        let current_swarm_id = SwarmId::new(config.swarm.swarm_id.clone());
        let swarm_token = config.swarm.token.as_ref().map(|t| SwarmToken::new(t.clone()));

        // Initialize known swarms with our own swarm.
        let mut known_swarms = std::collections::HashMap::new();
        known_swarms.insert(
            current_swarm_id.as_str().to_string(),
            SwarmRecord {
                swarm_id: current_swarm_id.clone(),
                name: config.swarm.name.clone(),
                is_public: config.swarm.is_public,
                agent_count: 1,
                joined: true,
                last_seen: chrono::Utc::now(),
            },
        );

        let state = ConnectorState {
            agent_id: agent_id.clone(),
            status: ConnectorStatus::Initializing,
            epoch_manager: EpochManager::new(epoch_config),
            pyramid: PyramidAllocator::new(pyramid_config),
            election: None,
            geo_cluster: GeoCluster::default(),
            succession: SuccessionManager::new(),
            rfp_coordinators: std::collections::HashMap::new(),
            voting_engines: std::collections::HashMap::new(),
            cascade: CascadeEngine::new(),
            task_set: OrSet::new(agent_id.to_string()),
            agent_set: OrSet::new(agent_id.to_string()),
            merkle_dag: MerkleDag::new(),
            content_store: ContentStore::new(),
            granularity: GranularityAlgorithm::default(),
            my_tier: Tier::Executor,
            parent_id: None,
            network_stats: NetworkStats {
                total_agents: 1,
                hierarchy_depth: 1,
                branching_factor: config.hierarchy.branching_factor,
                current_epoch: 1,
                my_tier: Tier::Executor,
                subordinate_count: 0,
                parent_id: None,
            },
            event_log: Vec::new(),
            start_time: chrono::Utc::now(),
            current_swarm_id,
            known_swarms,
            swarm_token,
        };

        Ok(Self {
            state: Arc::new(RwLock::new(state)),
            network_handle,
            event_rx: Some(event_rx),
            swarm_host: Some(swarm_host),
            config,
        })
    }

    /// Start the connector, running the swarm and event loop.
    ///
    /// This spawns the swarm host as a background task and runs
    /// the main event processing loop.
    pub async fn run(mut self) -> Result<(), anyhow::Error> {
        // Take and spawn the swarm host.
        let swarm_host = self
            .swarm_host
            .take()
            .ok_or_else(|| anyhow::anyhow!("SwarmHost already consumed"))?;

        tokio::spawn(async move {
            if let Err(e) = swarm_host.run().await {
                tracing::error!(error = %e, "Swarm host error");
            }
        });

        // Subscribe to core topics.
        self.network_handle.subscribe_core_topics().await?;

        // Subscribe to our swarm's topics (if not the default public swarm,
        // since core topics already include the public swarm).
        let swarm_id_str = self.config.swarm.swarm_id.clone();
        if swarm_id_str != openswarm_protocol::DEFAULT_SWARM_ID {
            self.network_handle
                .subscribe_swarm_topics(&swarm_id_str)
                .await?;
        }

        // Connect to bootstrap peers to join the swarm network immediately.
        self.connect_to_bootstrap_peers().await;

        // Initiate Kademlia bootstrap to populate the DHT routing table.
        if !self.config.network.bootstrap_peers.is_empty() {
            if let Err(e) = self.network_handle.bootstrap().await {
                tracing::warn!(error = %e, "Kademlia bootstrap initiation failed");
            }
        }

        // Update status.
        {
            let mut state = self.state.write().await;
            state.status = ConnectorStatus::Running;
            state.push_log(
                LogCategory::System,
                format!(
                    "ASCP Connector started (swarm: {} [{}])",
                    self.config.swarm.name, swarm_id_str
                ),
            );
        }

        tracing::info!("ASCP Connector is running");

        // Take the event receiver out of self so we can use both in the loop.
        let mut event_rx = self
            .event_rx
            .take()
            .ok_or_else(|| anyhow::anyhow!("Event receiver already consumed"))?;
        let keepalive_secs = self.config.hierarchy.keepalive_interval_secs;
        let mut keepalive_interval =
            tokio::time::interval(Duration::from_secs(keepalive_secs));
        let mut epoch_tick = tokio::time::interval(Duration::from_secs(1));
        let announce_secs = self.config.swarm.announce_interval_secs;
        let mut swarm_announce_interval =
            tokio::time::interval(Duration::from_secs(announce_secs));

        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    self.handle_network_event(event).await;
                }
                _ = keepalive_interval.tick() => {
                    self.send_keepalive().await;
                }
                _ = epoch_tick.tick() => {
                    self.check_epoch_transition().await;
                }
                _ = swarm_announce_interval.tick() => {
                    self.announce_swarm().await;
                }
            }
        }
    }

    /// Handle a network event from the swarm.
    async fn handle_network_event(&self, event: NetworkEvent) {
        match event {
            NetworkEvent::MessageReceived { topic, data, source, .. } => {
                {
                    let mut state = self.state.write().await;
                    state.push_log(
                        LogCategory::Message,
                        format!("Message received on {} from {}", topic, source),
                    );
                }
                self.handle_message(&topic, &data, source).await;
            }
            NetworkEvent::PeerConnected(peer) => {
                tracing::debug!(peer = %peer, "Peer connected");
                let mut state = self.state.write().await;
                state.agent_set.add(peer.to_string());
                state.push_log(
                    LogCategory::Peer,
                    format!("Connected: {}", peer),
                );
                // Update swarm size estimate.
                if let Ok(size) = self.network_handle.estimated_swarm_size().await {
                    state.network_stats.total_agents = size;
                }
            }
            NetworkEvent::PeerDisconnected(peer) => {
                tracing::debug!(peer = %peer, "Peer disconnected");
                let mut state = self.state.write().await;
                state.agent_set.remove(&peer.to_string());
                state.push_log(
                    LogCategory::Peer,
                    format!("Disconnected: {}", peer),
                );
            }
            NetworkEvent::PingRtt { peer, rtt } => {
                tracing::trace!(peer = %peer, rtt_ms = rtt.as_millis(), "Ping RTT");
            }
            _ => {}
        }
    }

    /// Handle a protocol message received on a topic.
    async fn handle_message(
        &self,
        topic: &str,
        data: &[u8],
        _source: openswarm_network::PeerId,
    ) {
        let message: SwarmMessage = match serde_json::from_slice(data) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to parse swarm message");
                let mut state = self.state.write().await;
                state.push_log(
                    LogCategory::Error,
                    format!("Failed to parse message on {}: {}", topic, e),
                );
                return;
            }
        };

        match ProtocolMethod::from_str(&message.method) {
            Some(ProtocolMethod::KeepAlive) => {
                if let Ok(params) = serde_json::from_value::<KeepAliveParams>(message.params) {
                    let mut state = self.state.write().await;
                    state.succession.record_keepalive(&params.agent_id);
                    state.push_log(
                        LogCategory::Message,
                        format!("KeepAlive from {}", params.agent_id),
                    );
                }
            }
            Some(ProtocolMethod::Candidacy) => {
                if let Ok(params) = serde_json::from_value::<CandidacyParams>(message.params) {
                    let mut state = self.state.write().await;
                    if let Some(ref mut election) = state.election {
                        if let Err(e) = election.register_candidate(&params) {
                            tracing::warn!(error = %e, "Failed to register candidate");
                        }
                    }
                }
            }
            Some(ProtocolMethod::ElectionVote) => {
                if let Ok(params) = serde_json::from_value::<ElectionVoteParams>(message.params) {
                    let mut state = self.state.write().await;
                    if let Some(ref mut election) = state.election {
                        if let Err(e) = election.record_vote(params) {
                            tracing::warn!(error = %e, "Failed to record election vote");
                        }
                    }
                }
            }
            Some(ProtocolMethod::TierAssignment) => {
                if let Ok(params) = serde_json::from_value::<TierAssignmentParams>(message.params)
                {
                    let mut state = self.state.write().await;
                    if params.assigned_agent == state.agent_id {
                        state.my_tier = params.tier;
                        state.parent_id = Some(params.parent_id);
                        state.network_stats.my_tier = params.tier;
                        tracing::info!(tier = ?params.tier, "Tier assignment received");
                    }
                }
            }
            Some(ProtocolMethod::TaskInjection) => {
                if let Ok(params) = serde_json::from_value::<TaskInjectionParams>(message.params) {
                    let mut state = self.state.write().await;
                    state.task_set.add(params.task.task_id.clone());
                    state.push_log(
                        LogCategory::Task,
                        format!("New task assigned: {}", params.task.task_id),
                    );
                    tracing::info!(
                        task_id = %params.task.task_id,
                        "Task injected"
                    );
                }
            }
            Some(ProtocolMethod::ProposalCommit) => {
                if let Ok(params) =
                    serde_json::from_value::<ProposalCommitParams>(message.params)
                {
                    let mut state = self.state.write().await;
                    if let Some(rfp) = state.rfp_coordinators.get_mut(&params.task_id) {
                        if let Err(e) = rfp.record_commit(&params) {
                            tracing::warn!(error = %e, "Failed to record proposal commit");
                        }
                    }
                }
            }
            Some(ProtocolMethod::ProposalReveal) => {
                if let Ok(params) =
                    serde_json::from_value::<ProposalRevealParams>(message.params)
                {
                    let mut state = self.state.write().await;
                    if let Some(rfp) = state.rfp_coordinators.get_mut(&params.task_id) {
                        if let Err(e) = rfp.record_reveal(&params) {
                            tracing::warn!(error = %e, "Failed to record proposal reveal");
                        }
                    }
                }
            }
            Some(ProtocolMethod::ConsensusVote) => {
                if let Ok(params) =
                    serde_json::from_value::<ConsensusVoteParams>(message.params)
                {
                    let task_id = params.task_id.clone();
                    let voter = params.voter.clone();
                    let mut state = self.state.write().await;
                    if let Some(voting) = state.voting_engines.get_mut(&task_id) {
                        let ranked_vote = RankedVote {
                            voter: voter.clone(),
                            task_id: params.task_id,
                            epoch: params.epoch,
                            rankings: params.rankings,
                            critic_scores: params.critic_scores,
                        };
                        if let Err(e) = voting.record_vote(ranked_vote) {
                            tracing::warn!(error = %e, "Failed to record consensus vote");
                        }
                    }
                    state.push_log(
                        LogCategory::Vote,
                        format!("Consensus vote recorded for {} from {}", task_id, voter),
                    );
                }
            }
            Some(ProtocolMethod::ResultSubmission) => {
                if let Ok(params) =
                    serde_json::from_value::<ResultSubmissionParams>(message.params)
                {
                    let mut state = self.state.write().await;
                    // Store the artifact content CID as leaf content bytes in the DAG.
                    state.merkle_dag.add_leaf(
                        params.task_id.clone(),
                        params.artifact.content_cid.as_bytes(),
                    );
                }
            }
            Some(ProtocolMethod::Succession) => {
                if let Ok(params) = serde_json::from_value::<SuccessionParams>(message.params) {
                    tracing::info!(
                        failed = %params.failed_leader,
                        new = %params.new_leader,
                        "Succession notification received"
                    );
                }
            }
            Some(ProtocolMethod::SwarmAnnounce) => {
                if let Ok(params) =
                    serde_json::from_value::<SwarmAnnounceParams>(message.params)
                {
                    let mut state = self.state.write().await;
                    let swarm_key = params.swarm_id.as_str().to_string();
                    let is_new = !state.known_swarms.contains_key(&swarm_key);

                    let record = state
                        .known_swarms
                        .entry(swarm_key.clone())
                        .or_insert_with(|| SwarmRecord {
                            swarm_id: params.swarm_id.clone(),
                            name: params.name.clone(),
                            is_public: params.is_public,
                            agent_count: params.agent_count,
                            joined: false,
                            last_seen: chrono::Utc::now(),
                        });

                    record.agent_count = params.agent_count;
                    record.last_seen = chrono::Utc::now();
                    record.name = params.name.clone();

                    if is_new {
                        state.push_log(
                            LogCategory::System,
                            format!(
                                "Discovered swarm: {} ({}, {} agents)",
                                params.name,
                                if params.is_public { "public" } else { "private" },
                                params.agent_count
                            ),
                        );
                        tracing::info!(
                            swarm_id = %params.swarm_id,
                            name = %params.name,
                            public = params.is_public,
                            agents = params.agent_count,
                            "Discovered new swarm"
                        );
                    }
                }
            }
            Some(ProtocolMethod::SwarmJoin) => {
                if let Ok(params) =
                    serde_json::from_value::<SwarmJoinParams>(message.params)
                {
                    let state = self.state.read().await;
                    // Only process join requests for our swarm.
                    if params.swarm_id == state.current_swarm_id {
                        tracing::info!(
                            agent = %params.agent_id,
                            swarm = %params.swarm_id,
                            "Join request for our swarm"
                        );
                    }
                }
            }
            Some(ProtocolMethod::SwarmLeave) => {
                if let Ok(params) =
                    serde_json::from_value::<SwarmLeaveParams>(message.params)
                {
                    let mut state = self.state.write().await;
                    if let Some(record) = state.known_swarms.get_mut(params.swarm_id.as_str()) {
                        record.agent_count = record.agent_count.saturating_sub(1);
                    }
                    state.push_log(
                        LogCategory::Peer,
                        format!("{} left swarm {}", params.agent_id, params.swarm_id),
                    );
                }
            }
            _ => {
                tracing::debug!(
                    method = %message.method,
                    topic = %topic,
                    "Unhandled protocol message"
                );
            }
        }
    }

    /// Announce this node's swarm to the network via GossipSub.
    ///
    /// Periodically broadcasts a SwarmAnnounce message on the global
    /// swarm discovery topic and the swarm-specific announcement topic.
    /// Also publishes the swarm info to the Kademlia DHT for internet-wide
    /// discovery.
    async fn announce_swarm(&self) {
        let state = self.state.read().await;
        let agent_count = state.agent_set.len() as u64 + 1; // +1 for self
        let params = SwarmAnnounceParams {
            swarm_id: state.current_swarm_id.clone(),
            name: self.config.swarm.name.clone(),
            is_public: self.config.swarm.is_public,
            agent_id: state.agent_id.clone(),
            agent_count,
            description: String::new(),
            timestamp: chrono::Utc::now(),
        };
        drop(state);

        let msg = SwarmMessage::new(
            ProtocolMethod::SwarmAnnounce.as_str(),
            serde_json::to_value(&params).unwrap_or_default(),
            String::new(),
        );

        if let Ok(data) = serde_json::to_vec(&msg) {
            // Publish to the global discovery topic.
            let discovery_topic = SwarmTopics::swarm_discovery();
            if let Err(e) = self.network_handle.publish(&discovery_topic, data.clone()).await {
                tracing::debug!(error = %e, "Failed to publish swarm announcement to discovery topic");
            }

            // Publish to the swarm-specific announcement topic.
            let announce_topic = SwarmTopics::swarm_announce(params.swarm_id.as_str());
            if let Err(e) = self.network_handle.publish(&announce_topic, data).await {
                tracing::debug!(error = %e, "Failed to publish swarm announcement to swarm topic");
            }
        }

        // Also register in DHT for internet-wide discovery.
        let dht_key = format!(
            "{}{}",
            openswarm_protocol::SWARM_REGISTRY_PREFIX,
            params.swarm_id
        );
        let dht_value = serde_json::json!({
            "swarm_id": params.swarm_id.as_str(),
            "name": params.name,
            "is_public": params.is_public,
            "agent_count": params.agent_count,
            "timestamp": params.timestamp.to_rfc3339(),
        });
        if let Ok(value_bytes) = serde_json::to_vec(&dht_value) {
            if let Err(e) = self
                .network_handle
                .put_dht_record(dht_key.into_bytes(), value_bytes)
                .await
            {
                tracing::debug!(error = %e, "Failed to publish swarm info to DHT");
            }
        }
    }

    /// Send a keep-alive message to the swarm.
    async fn send_keepalive(&self) {
        let state = self.state.read().await;
        let swarm_id = state.current_swarm_id.clone();
        let params = KeepAliveParams {
            agent_id: state.agent_id.clone(),
            epoch: state.epoch_manager.current_epoch(),
            timestamp: chrono::Utc::now(),
        };
        drop(state);

        let msg = SwarmMessage::new(
            ProtocolMethod::KeepAlive.as_str(),
            serde_json::to_value(&params).unwrap_or_default(),
            String::new(), // Signature would be computed in production.
        );

        if let Ok(data) = serde_json::to_vec(&msg) {
            let topic = SwarmTopics::keepalive_for(swarm_id.as_str());
            if let Err(e) = self.network_handle.publish(&topic, data).await {
                tracing::debug!(error = %e, "Failed to send keepalive");
            }
        }
    }

    /// Check for epoch transitions and trigger elections if needed.
    async fn check_epoch_transition(&self) {
        let swarm_size = self
            .network_handle
            .estimated_swarm_size()
            .await
            .unwrap_or(1);

        let mut state = self.state.write().await;
        if let Some(action) = state.epoch_manager.tick(swarm_size) {
            match action {
                openswarm_hierarchy::epoch::EpochAction::TriggerElection {
                    new_epoch,
                    estimated_swarm_size,
                } => {
                    tracing::info!(
                        new_epoch,
                        swarm_size = estimated_swarm_size,
                        "Triggering new epoch election"
                    );
                    // Recompute pyramid layout.
                    if let Ok(layout) = state.pyramid.compute_layout(estimated_swarm_size) {
                        state.network_stats.hierarchy_depth = layout.depth;
                    }
                    // Initialize election for new epoch.
                    let election_config = openswarm_hierarchy::elections::ElectionConfig::default();
                    state.election = Some(ElectionManager::new(election_config, new_epoch));
                    state.status = ConnectorStatus::InElection;
                    state.push_log(
                        LogCategory::Epoch,
                        format!("Epoch {} election triggered (swarm size: {})", new_epoch, estimated_swarm_size),
                    );
                }
                openswarm_hierarchy::epoch::EpochAction::FinalizeTransition { epoch } => {
                    tracing::info!(epoch, "Finalizing epoch transition");
                    // In production, this would tally votes and advance the epoch.
                    state.status = ConnectorStatus::Running;
                    state.push_log(
                        LogCategory::Epoch,
                        format!("Epoch {} transition finalized", epoch),
                    );
                }
            }
        }
    }

    /// Parse bootstrap peer multiaddresses (e.g. "/ip4/1.2.3.4/tcp/9000/p2p/12D3...")
    /// into (PeerId, Multiaddr) pairs for the discovery layer.
    fn parse_bootstrap_peers(addrs: &[String]) -> Vec<(PeerId, Multiaddr)> {
        let mut peers = Vec::new();
        for addr_str in addrs {
            let addr_str = addr_str.trim();
            if addr_str.is_empty() {
                continue;
            }
            match addr_str.parse::<Multiaddr>() {
                Ok(addr) => {
                    // Extract the PeerId from the /p2p/<peer_id> component of the multiaddr string.
                    if let Some(peer_id) = Self::extract_peer_id_from_addr(addr_str) {
                        peers.push((peer_id, addr));
                    } else {
                        tracing::warn!(
                            addr = %addr_str,
                            "Bootstrap address missing /p2p/<peer_id> component, skipping"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        addr = %addr_str,
                        error = %e,
                        "Failed to parse bootstrap multiaddress, skipping"
                    );
                }
            }
        }
        peers
    }

    /// Extract a PeerId from a multiaddress string by finding the /p2p/<id> segment.
    fn extract_peer_id_from_addr(addr: &str) -> Option<PeerId> {
        let parts: Vec<&str> = addr.split('/').collect();
        // Find the "p2p" component and take the next element as the peer ID.
        for (i, part) in parts.iter().enumerate() {
            if *part == "p2p" {
                if let Some(id_str) = parts.get(i + 1) {
                    return id_str.parse::<PeerId>().ok();
                }
            }
        }
        None
    }

    /// Dial bootstrap peers to establish connections immediately on startup.
    async fn connect_to_bootstrap_peers(&self) {
        for addr_str in &self.config.network.bootstrap_peers {
            let addr_str = addr_str.trim();
            if addr_str.is_empty() {
                continue;
            }
            if let Ok(addr) = addr_str.parse::<Multiaddr>() {
                match self.network_handle.dial(addr.clone()).await {
                    Ok(()) => {
                        tracing::info!(addr = %addr, "Dialing bootstrap peer");
                        let mut state = self.state.write().await;
                        state.push_log(
                            LogCategory::System,
                            format!("Dialing bootstrap peer: {}", addr),
                        );
                    }
                    Err(e) => {
                        tracing::warn!(addr = %addr, error = %e, "Failed to dial bootstrap peer");
                    }
                }
            }
        }
    }

    /// Get the current network statistics.
    pub async fn get_network_stats(&self) -> NetworkStats {
        let state = self.state.read().await;
        state.network_stats.clone()
    }

    /// Get the shared state for use by the RPC server.
    pub fn shared_state(&self) -> Arc<RwLock<ConnectorState>> {
        Arc::clone(&self.state)
    }

    /// Get the network handle for use by the RPC server.
    pub fn network_handle(&self) -> SwarmHandle {
        self.network_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bootstrap_peers_valid_multiaddr_with_peer_id() {
        // Use a valid Ed25519 peer ID (base58btc encoded).
        let peer_id = PeerId::random();
        let addr = format!("/ip4/192.168.1.1/tcp/9000/p2p/{}", peer_id);
        let result = OpenSwarmConnector::parse_bootstrap_peers(&[addr]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, peer_id);
    }

    #[test]
    fn parse_bootstrap_peers_missing_peer_id() {
        let addr = "/ip4/192.168.1.1/tcp/9000".to_string();
        let result = OpenSwarmConnector::parse_bootstrap_peers(&[addr]);
        assert!(result.is_empty(), "Should skip addrs without /p2p/ component");
    }

    #[test]
    fn parse_bootstrap_peers_invalid_multiaddr() {
        let addr = "not-a-valid-multiaddr".to_string();
        let result = OpenSwarmConnector::parse_bootstrap_peers(&[addr]);
        assert!(result.is_empty(), "Should skip unparseable addrs");
    }

    #[test]
    fn parse_bootstrap_peers_empty_and_whitespace_skipped() {
        let addrs = vec![
            "".to_string(),
            "   ".to_string(),
        ];
        let result = OpenSwarmConnector::parse_bootstrap_peers(&addrs);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_bootstrap_peers_multiple_valid() {
        let peer1 = PeerId::random();
        let peer2 = PeerId::random();
        let addrs = vec![
            format!("/ip4/10.0.0.1/tcp/4001/p2p/{}", peer1),
            format!("/ip4/10.0.0.2/tcp/4001/p2p/{}", peer2),
        ];
        let result = OpenSwarmConnector::parse_bootstrap_peers(&addrs);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, peer1);
        assert_eq!(result[1].0, peer2);
    }

    #[test]
    fn parse_bootstrap_peers_mixed_valid_and_invalid() {
        let peer1 = PeerId::random();
        let addrs = vec![
            format!("/ip4/10.0.0.1/tcp/4001/p2p/{}", peer1),
            "/ip4/10.0.0.2/tcp/4001".to_string(), // no peer id
            "garbage".to_string(),                  // unparseable
        ];
        let result = OpenSwarmConnector::parse_bootstrap_peers(&addrs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, peer1);
    }

    #[test]
    fn extract_peer_id_from_valid_addr() {
        let peer_id = PeerId::random();
        let addr = format!("/ip4/127.0.0.1/tcp/8080/p2p/{}", peer_id);
        let extracted = OpenSwarmConnector::extract_peer_id_from_addr(&addr);
        assert_eq!(extracted, Some(peer_id));
    }

    #[test]
    fn extract_peer_id_from_addr_without_p2p() {
        let addr = "/ip4/127.0.0.1/tcp/8080";
        let extracted = OpenSwarmConnector::extract_peer_id_from_addr(addr);
        assert!(extracted.is_none());
    }

    #[tokio::test]
    #[ignore = "Requires networking support"]
    async fn connector_new_with_default_config() {
        let config = ConnectorConfig::default();
        let connector = OpenSwarmConnector::new(config);
        assert!(connector.is_ok(), "Connector should initialize with default config");
    }

    #[tokio::test]
    #[ignore = "Requires networking support"]
    async fn connector_new_passes_bootstrap_peers_to_discovery() {
        let mut config = ConnectorConfig::default();
        let peer_id = PeerId::random();
        config.network.bootstrap_peers = vec![
            format!("/ip4/10.0.0.1/tcp/9000/p2p/{}", peer_id),
        ];
        let connector = OpenSwarmConnector::new(config);
        assert!(connector.is_ok(), "Connector should initialize with bootstrap peers");
    }

    #[tokio::test]
    #[ignore = "Requires networking support"]
    async fn connector_run_connects_to_swarm_on_start() {
        let config = ConnectorConfig::default();
        let connector = OpenSwarmConnector::new(config).unwrap();
        let state = connector.shared_state();

        // Run the connector with a timeout; it will reach Running status
        // within the timeout, then we abort via select.
        let state_clone = state.clone();
        tokio::select! {
            _ = connector.run() => {}
            _ = async {
                loop {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let s = state_clone.read().await;
                    if matches!(s.status, ConnectorStatus::Running) {
                        break;
                    }
                }
            } => {}
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                panic!("Timed out waiting for connector to reach Running status");
            }
        }

        let s = state.read().await;
        assert!(
            matches!(s.status, ConnectorStatus::Running),
            "Connector should be Running after start"
        );
        assert!(
            s.event_log.iter().any(|e| e.message.contains("ASCP Connector started")),
            "Should have startup log entry"
        );
    }
}
