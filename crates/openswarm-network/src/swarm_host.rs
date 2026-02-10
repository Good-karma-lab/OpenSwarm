//! Main SwarmHost that manages the libp2p Swarm, handles events, and
//! provides the API for upper layers (hierarchy, consensus, state).
//!
//! Architecture:
//! - `SwarmHost` owns the libp2p `Swarm` and runs the event loop in a tokio task.
//! - `SwarmHandle` is a cheaply cloneable handle providing async methods
//!   for publishing, subscribing, dialing, and querying the network.
//! - Communication between the handle and the host uses bounded MPSC channels
//!   for commands and a broadcast-style channel for events.

use std::collections::HashMap;
use std::time::Duration;

use futures::StreamExt;
use libp2p::gossipsub::IdentTopic;
use libp2p::swarm::SwarmEvent;
use libp2p::{gossipsub, identify, kad, mdns, ping, Multiaddr, PeerId, Swarm};
use tokio::sync::{mpsc, oneshot};

use crate::behaviour::{SwarmBehaviour, SwarmBehaviourEvent};
use crate::discovery::{DiscoveryConfig, DiscoveryManager};
use crate::size_estimator::SwarmSizeEstimator;
use crate::topics::TopicManager;
use crate::transport::{self, TransportConfig};
use crate::NetworkError;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the SwarmHost.
#[derive(Debug, Clone)]
pub struct SwarmHostConfig {
    /// Address to listen on (e.g. "/ip4/0.0.0.0/tcp/0").
    pub listen_addr: Multiaddr,
    /// Transport configuration.
    pub transport: TransportConfig,
    /// Discovery configuration.
    pub discovery: DiscoveryConfig,
    /// Command channel buffer size.
    pub command_buffer: usize,
    /// Event channel buffer size.
    pub event_buffer: usize,
    /// Interval between Kademlia random walks.
    pub random_walk_interval: Duration,
}

impl Default for SwarmHostConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/tcp/0"
                .parse()
                .expect("valid default listen addr"),
            transport: TransportConfig::default(),
            discovery: DiscoveryConfig::default(),
            command_buffer: 256,
            event_buffer: 256,
            random_walk_interval: Duration::from_secs(30),
        }
    }
}

// ---------------------------------------------------------------------------
// Events emitted to upper layers
// ---------------------------------------------------------------------------

/// Events from the network layer forwarded to upper layers.
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// A GossipSub message was received.
    MessageReceived {
        source: PeerId,
        topic: String,
        data: Vec<u8>,
    },
    /// A new peer connected.
    PeerConnected(PeerId),
    /// A peer disconnected.
    PeerDisconnected(PeerId),
    /// Kademlia routing table was updated.
    RoutingUpdated {
        peer: PeerId,
        is_new_peer: bool,
    },
    /// A peer was identified via the Identify protocol.
    PeerIdentified {
        peer: PeerId,
        agent_version: String,
        listen_addrs: Vec<Multiaddr>,
    },
    /// Ping round-trip time measured.
    PingRtt {
        peer: PeerId,
        rtt: Duration,
    },
    /// Swarm is now listening on an address.
    Listening(Multiaddr),
}

// ---------------------------------------------------------------------------
// Commands from upper layers to the swarm
// ---------------------------------------------------------------------------

enum SwarmCommand {
    Publish {
        topic: String,
        data: Vec<u8>,
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
    Subscribe {
        topic: String,
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
    Unsubscribe {
        topic: String,
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
    Dial {
        addr: Multiaddr,
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
    AddPeerAddress {
        peer_id: PeerId,
        addr: Multiaddr,
    },
    GetConnectedPeers {
        reply: oneshot::Sender<Vec<PeerId>>,
    },
    GetEstimatedSwarmSize {
        reply: oneshot::Sender<u64>,
    },
    PutDhtRecord {
        key: Vec<u8>,
        value: Vec<u8>,
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
    Bootstrap {
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
    #[allow(dead_code)]
    GetLocalPeerId {
        reply: oneshot::Sender<PeerId>,
    },
    SubscribeCoreTopic {
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
    SubscribeTaskTopics {
        task_id: String,
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
    UnsubscribeTaskTopics {
        task_id: String,
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
    SubscribeTierTopics {
        tier: u32,
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
    GetDhtRecord {
        key: Vec<u8>,
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
    SubscribeSwarmTopics {
        swarm_id: String,
        reply: oneshot::Sender<Result<(), NetworkError>>,
    },
}

// ---------------------------------------------------------------------------
// SwarmHandle (clonable, Send-able API for upper layers)
// ---------------------------------------------------------------------------

/// A cheaply cloneable handle for sending commands to the SwarmHost.
///
/// All methods are async and return when the command has been processed.
#[derive(Clone)]
pub struct SwarmHandle {
    command_tx: mpsc::Sender<SwarmCommand>,
    local_peer_id: PeerId,
}

impl SwarmHandle {
    /// Get the local peer ID of this node.
    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    /// Publish data to a GossipSub topic.
    pub async fn publish(&self, topic: &str, data: Vec<u8>) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::Publish {
                topic: topic.to_string(),
                data,
                reply: tx,
            })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Subscribe to a GossipSub topic.
    pub async fn subscribe(&self, topic: &str) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::Subscribe {
                topic: topic.to_string(),
                reply: tx,
            })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Unsubscribe from a GossipSub topic.
    pub async fn unsubscribe(&self, topic: &str) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::Unsubscribe {
                topic: topic.to_string(),
                reply: tx,
            })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Dial a remote peer by multiaddress.
    pub async fn dial(&self, addr: Multiaddr) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::Dial { addr, reply: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Add a known address for a peer (without dialing).
    pub async fn add_peer_address(
        &self,
        peer_id: PeerId,
        addr: Multiaddr,
    ) -> Result<(), NetworkError> {
        self.command_tx
            .send(SwarmCommand::AddPeerAddress { peer_id, addr })
            .await
            .map_err(|_| NetworkError::ChannelClosed)
    }

    /// Get the list of currently connected peer IDs.
    pub async fn connected_peers(&self) -> Result<Vec<PeerId>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::GetConnectedPeers { reply: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)
    }

    /// Get the estimated total swarm size (N).
    pub async fn estimated_swarm_size(&self) -> Result<u64, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::GetEstimatedSwarmSize { reply: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)
    }

    /// Store a key-value record in the Kademlia DHT.
    pub async fn put_dht_record(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::PutDhtRecord {
                key,
                value,
                reply: tx,
            })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Initiate Kademlia bootstrap.
    pub async fn bootstrap(&self) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::Bootstrap { reply: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Subscribe to the core protocol topics.
    pub async fn subscribe_core_topics(&self) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::SubscribeCoreTopic { reply: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Subscribe to task-specific proposal/voting/result topics.
    pub async fn subscribe_task_topics(&self, task_id: &str) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::SubscribeTaskTopics {
                task_id: task_id.to_string(),
                reply: tx,
            })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Unsubscribe from task-specific topics after task completion.
    pub async fn unsubscribe_task_topics(&self, task_id: &str) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::UnsubscribeTaskTopics {
                task_id: task_id.to_string(),
                reply: tx,
            })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Subscribe to tier-level task topics.
    pub async fn subscribe_tier_topics(&self, tier: u32) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::SubscribeTierTopics { tier, reply: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Initiate a DHT get_record query. The result will arrive asynchronously
    /// via Kademlia events. This is a fire-and-forget initiation.
    pub async fn get_dht_record(&self, key: Vec<u8>) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::GetDhtRecord { key, reply: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Subscribe to all topics for a specific swarm (election, keepalive, hierarchy, discovery).
    pub async fn subscribe_swarm_topics(&self, swarm_id: &str) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::SubscribeSwarmTopics {
                swarm_id: swarm_id.to_string(),
                reply: tx,
            })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;
        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }
}

// ---------------------------------------------------------------------------
// SwarmHost (owns the Swarm, runs the event loop)
// ---------------------------------------------------------------------------

/// The main network host that owns and drives the libp2p Swarm.
///
/// Created via `SwarmHost::new()`, which also returns a `SwarmHandle`
/// for sending commands. Call `swarm_host.run()` to start the event
/// loop (typically spawned as a tokio task).
pub struct SwarmHost {
    swarm: Swarm<SwarmBehaviour>,
    command_rx: mpsc::Receiver<SwarmCommand>,
    event_tx: mpsc::Sender<NetworkEvent>,
    topic_manager: TopicManager,
    discovery_manager: DiscoveryManager,
    size_estimator: SwarmSizeEstimator,
    /// Mapping from PeerId to observed RTT for Vivaldi coordinate updates.
    peer_rtt: HashMap<PeerId, Duration>,
    /// Interval timer for Kademlia random walks.
    random_walk_interval: Duration,
}

impl SwarmHost {
    /// Create a new SwarmHost and its associated handle.
    ///
    /// The returned `SwarmHandle` is used by upper layers to interact
    /// with the network. The `mpsc::Receiver<NetworkEvent>` receives
    /// events from the network for upper-layer processing.
    pub fn new(
        config: SwarmHostConfig,
    ) -> Result<(Self, SwarmHandle, mpsc::Receiver<NetworkEvent>), NetworkError> {
        let mut swarm = transport::build_swarm(config.transport)?;

        // Start listening.
        swarm
            .listen_on(config.listen_addr.clone())
            .map_err(|e| NetworkError::ListenError(e.to_string()))?;

        let local_peer_id = *swarm.local_peer_id();
        tracing::info!(peer_id = %local_peer_id, "Local peer ID");

        let (command_tx, command_rx) = mpsc::channel(config.command_buffer);
        let (event_tx, event_rx) = mpsc::channel(config.event_buffer);

        let discovery_manager = DiscoveryManager::new(config.discovery);
        let topic_manager = TopicManager::new();
        let size_estimator = SwarmSizeEstimator::default();

        let host = Self {
            swarm,
            command_rx,
            event_tx,
            topic_manager,
            discovery_manager,
            size_estimator,
            peer_rtt: HashMap::new(),
            random_walk_interval: config.random_walk_interval,
        };

        let handle = SwarmHandle {
            command_tx,
            local_peer_id,
        };

        Ok((host, handle, event_rx))
    }

    /// Run the swarm event loop.
    ///
    /// This drives the libp2p Swarm and processes commands from the handle.
    /// It should be spawned as a long-running tokio task.
    pub async fn run(mut self) -> Result<(), NetworkError> {
        // Initiate bootstrap with configured peers.
        self.discovery_manager
            .initiate_bootstrap(&mut self.swarm.behaviour_mut().kademlia)?;

        let mut walk_interval = tokio::time::interval(self.random_walk_interval);
        walk_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await;
                }
                Some(cmd) = self.command_rx.recv() => {
                    self.handle_command(cmd).await;
                }
                _ = walk_interval.tick() => {
                    self.discovery_manager.trigger_random_walk(
                        &mut self.swarm.behaviour_mut().kademlia,
                    );
                    // Update size estimate from connected peer count.
                    let peer_count = self.swarm.connected_peers().count();
                    self.size_estimator.update_from_peer_count(peer_count);
                }
            }
        }
    }

    // ---- Event Handling ----

    async fn handle_swarm_event(&mut self, event: SwarmEvent<SwarmBehaviourEvent>) {
        match event {
            SwarmEvent::Behaviour(behaviour_event) => {
                self.handle_behaviour_event(behaviour_event).await;
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                tracing::info!(
                    peer = %peer_id,
                    endpoint = ?endpoint,
                    "Connection established"
                );
                self.discovery_manager.add_peer(peer_id);
                let _ = self.event_tx.send(NetworkEvent::PeerConnected(peer_id)).await;
            }
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                tracing::info!(
                    peer = %peer_id,
                    cause = ?cause,
                    "Connection closed"
                );
                self.peer_rtt.remove(&peer_id);
                let _ = self
                    .event_tx
                    .send(NetworkEvent::PeerDisconnected(peer_id))
                    .await;
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                tracing::info!(addr = %address, "Now listening");
                let _ = self.event_tx.send(NetworkEvent::Listening(address)).await;
            }
            _ => {}
        }
    }

    async fn handle_behaviour_event(&mut self, event: SwarmBehaviourEvent) {
        match event {
            SwarmBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source,
                message,
                ..
            }) => {
                let topic_str = self
                    .topic_manager
                    .resolve_topic(&message.topic)
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| message.topic.to_string());

                tracing::debug!(
                    source = %propagation_source,
                    topic = %topic_str,
                    bytes = message.data.len(),
                    "GossipSub message received"
                );

                let _ = self
                    .event_tx
                    .send(NetworkEvent::MessageReceived {
                        source: propagation_source,
                        topic: topic_str,
                        data: message.data,
                    })
                    .await;
            }
            SwarmBehaviourEvent::Mdns(mdns::Event::Discovered(list)) => {
                let peers: Vec<_> = list.into_iter().collect();
                for (peer_id, addr) in &peers {
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(peer_id, addr.clone());
                }
                self.discovery_manager
                    .on_mdns_discovered(peers.into_iter());
            }
            SwarmBehaviourEvent::Mdns(mdns::Event::Expired(list)) => {
                self.discovery_manager
                    .on_mdns_expired(list.into_iter());
            }
            SwarmBehaviourEvent::Kademlia(kad::Event::RoutingUpdated {
                peer, is_new_peer, ..
            }) => {
                tracing::debug!(peer = %peer, new = is_new_peer, "Kademlia routing updated");
                let _ = self
                    .event_tx
                    .send(NetworkEvent::RoutingUpdated {
                        peer,
                        is_new_peer,
                    })
                    .await;
            }
            SwarmBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {
                result, ..
            }) => {
                if let kad::QueryResult::Bootstrap(Ok(_)) = result {
                    self.discovery_manager.on_bootstrap_complete();
                }
            }
            SwarmBehaviourEvent::Identify(identify::Event::Received {
                peer_id, info, ..
            }) => {
                tracing::debug!(
                    peer = %peer_id,
                    agent = %info.agent_version,
                    "Peer identified"
                );
                // Add identified addresses to Kademlia.
                for addr in &info.listen_addrs {
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr.clone());
                }
                let _ = self
                    .event_tx
                    .send(NetworkEvent::PeerIdentified {
                        peer: peer_id,
                        agent_version: info.agent_version,
                        listen_addrs: info.listen_addrs,
                    })
                    .await;
            }
            SwarmBehaviourEvent::Ping(ping::Event {
                peer,
                result: Ok(rtt),
                ..
            }) => {
                self.peer_rtt.insert(peer, rtt);
                let _ = self
                    .event_tx
                    .send(NetworkEvent::PingRtt { peer, rtt })
                    .await;
            }
            _ => {}
        }
    }

    // ---- Command Handling ----

    async fn handle_command(&mut self, cmd: SwarmCommand) {
        match cmd {
            SwarmCommand::Publish { topic, data, reply } => {
                let result = self.publish_message(&topic, data);
                let _ = reply.send(result);
            }
            SwarmCommand::Subscribe { topic, reply } => {
                let result = self
                    .topic_manager
                    .subscribe(&mut self.swarm.behaviour_mut().gossipsub, &topic)
                    .map(|_| ());
                let _ = reply.send(result);
            }
            SwarmCommand::Unsubscribe { topic, reply } => {
                let result = self
                    .topic_manager
                    .unsubscribe(&mut self.swarm.behaviour_mut().gossipsub, &topic);
                let _ = reply.send(result);
            }
            SwarmCommand::Dial { addr, reply } => {
                let result = self
                    .swarm
                    .dial(addr)
                    .map_err(|e| NetworkError::DialError(e.to_string()));
                let _ = reply.send(result);
            }
            SwarmCommand::AddPeerAddress { peer_id, addr } => {
                self.swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, addr);
            }
            SwarmCommand::GetConnectedPeers { reply } => {
                let peers: Vec<PeerId> = self.swarm.connected_peers().copied().collect();
                let _ = reply.send(peers);
            }
            SwarmCommand::GetEstimatedSwarmSize { reply } => {
                let _ = reply.send(self.size_estimator.estimated_size());
            }
            SwarmCommand::PutDhtRecord { key, value, reply } => {
                let record = libp2p::kad::Record {
                    key: libp2p::kad::RecordKey::new(&key),
                    value,
                    publisher: None,
                    expires: None,
                };
                let result = self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .put_record(record, libp2p::kad::Quorum::One)
                    .map(|_| ())
                    .map_err(|e| NetworkError::DhtError(e.to_string()));
                let _ = reply.send(result);
            }
            SwarmCommand::Bootstrap { reply } => {
                let result = self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .bootstrap()
                    .map(|_| ())
                    .map_err(|e| NetworkError::DhtError(format!("Bootstrap failed: {e}")));
                let _ = reply.send(result);
            }
            SwarmCommand::GetLocalPeerId { reply } => {
                let _ = reply.send(*self.swarm.local_peer_id());
            }
            SwarmCommand::SubscribeCoreTopic { reply } => {
                let result = self
                    .topic_manager
                    .subscribe_core_topics(&mut self.swarm.behaviour_mut().gossipsub);
                let _ = reply.send(result);
            }
            SwarmCommand::SubscribeTaskTopics { task_id, reply } => {
                let result = self
                    .topic_manager
                    .subscribe_task_topics(
                        &mut self.swarm.behaviour_mut().gossipsub,
                        &task_id,
                    );
                let _ = reply.send(result);
            }
            SwarmCommand::UnsubscribeTaskTopics { task_id, reply } => {
                let result = self
                    .topic_manager
                    .unsubscribe_task_topics(
                        &mut self.swarm.behaviour_mut().gossipsub,
                        &task_id,
                    );
                let _ = reply.send(result);
            }
            SwarmCommand::SubscribeTierTopics { tier, reply } => {
                let result = self
                    .topic_manager
                    .subscribe_tier_topics(
                        &mut self.swarm.behaviour_mut().gossipsub,
                        tier,
                    );
                let _ = reply.send(result);
            }
            SwarmCommand::GetDhtRecord { key, reply } => {
                let record_key = libp2p::kad::RecordKey::new(&key);
                self.swarm
                    .behaviour_mut()
                    .kademlia
                    .get_record(record_key);
                let _ = reply.send(Ok(()));
            }
            SwarmCommand::SubscribeSwarmTopics { swarm_id, reply } => {
                let result = self
                    .topic_manager
                    .subscribe_swarm_topics(
                        &mut self.swarm.behaviour_mut().gossipsub,
                        &swarm_id,
                    );
                let _ = reply.send(result);
            }
        }
    }

    /// Internal helper to publish a message to a GossipSub topic.
    fn publish_message(&mut self, topic_str: &str, data: Vec<u8>) -> Result<(), NetworkError> {
        let topic = IdentTopic::new(topic_str);
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, data)
            .map(|_| ())
            .map_err(|e| NetworkError::PublishError(e.to_string()))
    }

    /// Get a reference to the peer RTT map for Vivaldi coordinate updates.
    pub fn peer_rtt(&self) -> &HashMap<PeerId, Duration> {
        &self.peer_rtt
    }
}
