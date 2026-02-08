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
    NetworkEvent, SwarmHandle, SwarmHost, SwarmHostConfig,
    discovery::DiscoveryConfig,
    transport::TransportConfig,
};
use openswarm_protocol::*;
use openswarm_state::{ContentStore, GranularityAlgorithm, MerkleDag, OrSet};

use crate::config::ConnectorConfig;

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
}

/// The main OpenSwarm Connector that orchestrates all subsystems.
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

        let swarm_config = SwarmHostConfig {
            listen_addr,
            transport: TransportConfig::default(),
            discovery: DiscoveryConfig {
                mdns_enabled: config.network.mdns_enabled,
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

        // Update status.
        {
            let mut state = self.state.write().await;
            state.status = ConnectorStatus::Running;
        }

        tracing::info!("OpenSwarm Connector is running");

        // Take the event receiver out of self so we can use both in the loop.
        let mut event_rx = self
            .event_rx
            .take()
            .ok_or_else(|| anyhow::anyhow!("Event receiver already consumed"))?;
        let keepalive_secs = self.config.hierarchy.keepalive_interval_secs;
        let mut keepalive_interval =
            tokio::time::interval(Duration::from_secs(keepalive_secs));
        let mut epoch_tick = tokio::time::interval(Duration::from_secs(1));

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
            }
        }
    }

    /// Handle a network event from the swarm.
    async fn handle_network_event(&self, event: NetworkEvent) {
        match event {
            NetworkEvent::MessageReceived { topic, data, source, .. } => {
                self.handle_message(&topic, &data, source).await;
            }
            NetworkEvent::PeerConnected(peer) => {
                tracing::debug!(peer = %peer, "Peer connected");
                let mut state = self.state.write().await;
                state.agent_set.add(peer.to_string());
                // Update swarm size estimate.
                if let Ok(size) = self.network_handle.estimated_swarm_size().await {
                    state.network_stats.total_agents = size;
                }
            }
            NetworkEvent::PeerDisconnected(peer) => {
                tracing::debug!(peer = %peer, "Peer disconnected");
                let mut state = self.state.write().await;
                state.agent_set.remove(&peer.to_string());
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
                return;
            }
        };

        match ProtocolMethod::from_str(&message.method) {
            Some(ProtocolMethod::KeepAlive) => {
                if let Ok(params) = serde_json::from_value::<KeepAliveParams>(message.params) {
                    let mut state = self.state.write().await;
                    state.succession.record_keepalive(&params.agent_id);
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
                    let mut state = self.state.write().await;
                    if let Some(voting) = state.voting_engines.get_mut(&params.task_id) {
                        let ranked_vote = RankedVote {
                            voter: params.voter,
                            task_id: params.task_id,
                            epoch: params.epoch,
                            rankings: params.rankings,
                            critic_scores: params.critic_scores,
                        };
                        if let Err(e) = voting.record_vote(ranked_vote) {
                            tracing::warn!(error = %e, "Failed to record consensus vote");
                        }
                    }
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
            _ => {
                tracing::debug!(
                    method = %message.method,
                    topic = %topic,
                    "Unhandled protocol message"
                );
            }
        }
    }

    /// Send a keep-alive message to the swarm.
    async fn send_keepalive(&self) {
        let state = self.state.read().await;
        let params = KeepAliveParams {
            agent_id: state.agent_id.clone(),
            epoch: state.epoch_manager.current_epoch(),
            timestamp: chrono::Utc::now(),
        };

        let msg = SwarmMessage::new(
            ProtocolMethod::KeepAlive.as_str(),
            serde_json::to_value(&params).unwrap_or_default(),
            String::new(), // Signature would be computed in production.
        );

        if let Ok(data) = serde_json::to_vec(&msg) {
            let topic = SwarmTopics::keepalive();
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
                }
                openswarm_hierarchy::epoch::EpochAction::FinalizeTransition { epoch } => {
                    tracing::info!(epoch, "Finalizing epoch transition");
                    // In production, this would tally votes and advance the epoch.
                    state.status = ConnectorStatus::Running;
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
