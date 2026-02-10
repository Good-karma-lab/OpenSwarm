//! GossipSub topic management for all protocol topics.
//!
//! Manages subscription and lifecycle of GossipSub topics corresponding
//! to the OpenSwarm protocol channels: elections, proposals, voting,
//! tasks, results, keepalive, and hierarchy.

use std::collections::HashMap;

use libp2p::gossipsub::{self, IdentTopic, TopicHash};

use crate::NetworkError;

/// Manages GossipSub topic subscriptions for an OpenSwarm node.
///
/// Topics are organized by protocol function. Each topic is tracked
/// by its hash for efficient lookup during message routing.
pub struct TopicManager {
    /// Map from topic hash to the topic itself for reverse lookup.
    subscribed: HashMap<TopicHash, IdentTopic>,
}

impl TopicManager {
    /// Create a new empty topic manager.
    pub fn new() -> Self {
        Self {
            subscribed: HashMap::new(),
        }
    }

    /// Subscribe to a topic on the given GossipSub behaviour.
    ///
    /// Returns the topic hash on success, or an error if subscription fails.
    pub fn subscribe(
        &mut self,
        gossipsub: &mut gossipsub::Behaviour,
        topic_str: &str,
    ) -> Result<TopicHash, NetworkError> {
        let topic = IdentTopic::new(topic_str);
        let hash = topic.hash();
        gossipsub
            .subscribe(&topic)
            .map_err(|e| NetworkError::SubscriptionError(format!("{e}")))?;
        self.subscribed.insert(hash.clone(), topic);
        tracing::info!(topic = %topic_str, "Subscribed to GossipSub topic");
        Ok(hash)
    }

    /// Unsubscribe from a topic.
    pub fn unsubscribe(
        &mut self,
        gossipsub: &mut gossipsub::Behaviour,
        topic_str: &str,
    ) -> Result<(), NetworkError> {
        let topic = IdentTopic::new(topic_str);
        let hash = topic.hash();
        gossipsub
            .unsubscribe(&topic)
            .map_err(|e| NetworkError::SubscriptionError(format!("{e}")))?;
        self.subscribed.remove(&hash);
        tracing::info!(topic = %topic_str, "Unsubscribed from GossipSub topic");
        Ok(())
    }

    /// Look up the topic string for a given topic hash.
    pub fn resolve_topic(&self, hash: &TopicHash) -> Option<&IdentTopic> {
        self.subscribed.get(hash)
    }

    /// Check if we are subscribed to a given topic hash.
    pub fn is_subscribed(&self, hash: &TopicHash) -> bool {
        self.subscribed.contains_key(hash)
    }

    /// Get all currently subscribed topic hashes.
    pub fn subscribed_topics(&self) -> Vec<TopicHash> {
        self.subscribed.keys().cloned().collect()
    }

    /// Subscribe to the core set of protocol topics that every node needs.
    ///
    /// This includes: global swarm discovery, plus the default public swarm's
    /// election, keepalive, and hierarchy topics.
    pub fn subscribe_core_topics(
        &mut self,
        gossipsub: &mut gossipsub::Behaviour,
    ) -> Result<(), NetworkError> {
        use openswarm_protocol::SwarmTopics;

        // Global swarm discovery topic (shared across all swarms).
        self.subscribe(gossipsub, &SwarmTopics::swarm_discovery())?;

        // Default public swarm core topics.
        self.subscribe(gossipsub, &SwarmTopics::election_tier1())?;
        self.subscribe(gossipsub, &SwarmTopics::keepalive())?;
        self.subscribe(gossipsub, &SwarmTopics::hierarchy())?;

        tracing::info!("Subscribed to core protocol topics");
        Ok(())
    }

    /// Subscribe to all protocol topics for a specific swarm.
    ///
    /// This includes the swarm's election, keepalive, hierarchy, and
    /// announcement topics.
    pub fn subscribe_swarm_topics(
        &mut self,
        gossipsub: &mut gossipsub::Behaviour,
        swarm_id: &str,
    ) -> Result<(), NetworkError> {
        use openswarm_protocol::SwarmTopics;

        self.subscribe(gossipsub, &SwarmTopics::swarm_announce(swarm_id))?;
        self.subscribe(gossipsub, &SwarmTopics::election_tier1_for(swarm_id))?;
        self.subscribe(gossipsub, &SwarmTopics::keepalive_for(swarm_id))?;
        self.subscribe(gossipsub, &SwarmTopics::hierarchy_for(swarm_id))?;

        tracing::info!(swarm_id, "Subscribed to swarm-specific topics");
        Ok(())
    }

    /// Subscribe to task-related topics for a specific tier level.
    pub fn subscribe_tier_topics(
        &mut self,
        gossipsub: &mut gossipsub::Behaviour,
        tier: u32,
    ) -> Result<(), NetworkError> {
        use openswarm_protocol::SwarmTopics;

        self.subscribe(gossipsub, &SwarmTopics::tasks(tier))?;
        tracing::info!(tier, "Subscribed to tier task topics");
        Ok(())
    }

    /// Subscribe to topics for a specific task's proposal and voting phases.
    pub fn subscribe_task_topics(
        &mut self,
        gossipsub: &mut gossipsub::Behaviour,
        task_id: &str,
    ) -> Result<(), NetworkError> {
        use openswarm_protocol::SwarmTopics;

        self.subscribe(gossipsub, &SwarmTopics::proposals(task_id))?;
        self.subscribe(gossipsub, &SwarmTopics::voting(task_id))?;
        self.subscribe(gossipsub, &SwarmTopics::results(task_id))?;
        tracing::info!(task_id, "Subscribed to task-specific topics");
        Ok(())
    }

    /// Unsubscribe from a specific task's topics (cleanup after task completion).
    pub fn unsubscribe_task_topics(
        &mut self,
        gossipsub: &mut gossipsub::Behaviour,
        task_id: &str,
    ) -> Result<(), NetworkError> {
        use openswarm_protocol::SwarmTopics;

        // Best-effort unsubscribe; ignore errors for topics we may not be subscribed to.
        let _ = self.unsubscribe(gossipsub, &SwarmTopics::proposals(task_id));
        let _ = self.unsubscribe(gossipsub, &SwarmTopics::voting(task_id));
        let _ = self.unsubscribe(gossipsub, &SwarmTopics::results(task_id));
        tracing::debug!(task_id, "Unsubscribed from task-specific topics");
        Ok(())
    }
}

impl Default for TopicManager {
    fn default() -> Self {
        Self::new()
    }
}
