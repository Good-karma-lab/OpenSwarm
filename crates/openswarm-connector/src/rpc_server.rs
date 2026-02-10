//! JSON-RPC 2.0 server over TCP implementing the Swarm API.
//!
//! Provides the following methods for the local AI agent:
//! - `swarm.connect()` - Connect to a peer by multiaddress
//! - `swarm.get_network_stats()` - Get current network statistics
//! - `swarm.propose_plan()` - Submit a task decomposition plan
//! - `swarm.submit_result()` - Submit a task execution result
//! - `swarm.receive_task()` - Poll for assigned tasks
//! - `swarm.get_status()` - Get connector and agent status
//! - `swarm.list_swarms()` - List all known swarms with their info
//! - `swarm.create_swarm()` - Create a new private swarm
//! - `swarm.join_swarm()` - Join an existing swarm
//!
//! The server listens on localhost TCP and speaks JSON-RPC 2.0.
//! Each line received is a JSON-RPC request; each line sent is a response.

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use openswarm_protocol::*;

use crate::connector::{ConnectorState, SwarmRecord};

/// The JSON-RPC 2.0 server.
pub struct RpcServer {
    /// TCP listener address.
    bind_addr: String,
    /// Shared connector state.
    state: Arc<RwLock<ConnectorState>>,
    /// Network handle for network operations.
    network_handle: openswarm_network::SwarmHandle,
    /// Maximum concurrent connections.
    max_connections: usize,
}

impl RpcServer {
    /// Create a new RPC server.
    pub fn new(
        bind_addr: String,
        state: Arc<RwLock<ConnectorState>>,
        network_handle: openswarm_network::SwarmHandle,
        max_connections: usize,
    ) -> Self {
        Self {
            bind_addr,
            state,
            network_handle,
            max_connections,
        }
    }

    /// Start the RPC server, listening for connections.
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let listener = TcpListener::bind(&self.bind_addr).await?;
        tracing::info!(addr = %self.bind_addr, "JSON-RPC server listening");

        let state = Arc::clone(&self.state);
        let network_handle = self.network_handle.clone();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.max_connections));

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            tracing::debug!(peer = %peer_addr, "RPC client connected");

            let state = Arc::clone(&state);
            let network_handle = network_handle.clone();
            let permit = semaphore.clone().acquire_owned().await?;

            tokio::spawn(async move {
                if let Err(e) =
                    handle_connection(stream, state, network_handle).await
                {
                    tracing::warn!(
                        peer = %peer_addr,
                        error = %e,
                        "RPC connection error"
                    );
                }
                drop(permit);
            });
        }
    }
}

/// Handle a single RPC client connection.
///
/// Reads newline-delimited JSON-RPC requests and sends back responses.
async fn handle_connection(
    stream: tokio::net::TcpStream,
    state: Arc<RwLock<ConnectorState>>,
    network_handle: openswarm_network::SwarmHandle,
) -> Result<(), anyhow::Error> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let response = process_request(&line, &state, &network_handle).await;
        let response_json = serde_json::to_string(&response)?;
        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}

/// Process a single JSON-RPC request and return a response.
async fn process_request(
    request_str: &str,
    state: &Arc<RwLock<ConnectorState>>,
    network_handle: &openswarm_network::SwarmHandle,
) -> SwarmResponse {
    // Parse the request.
    let request: SwarmMessage = match serde_json::from_str(request_str) {
        Ok(r) => r,
        Err(e) => {
            return SwarmResponse::error(
                None,
                -32700, // Parse error
                format!("Invalid JSON: {}", e),
            );
        }
    };

    let request_id = request.id.clone();

    match request.method.as_str() {
        "swarm.connect" => handle_connect(request_id, &request.params, network_handle).await,
        "swarm.get_network_stats" => handle_get_network_stats(request_id, state).await,
        "swarm.propose_plan" => handle_propose_plan(request_id, &request.params, state).await,
        "swarm.submit_result" => {
            handle_submit_result(request_id, &request.params, state, network_handle).await
        }
        "swarm.receive_task" => handle_receive_task(request_id, state).await,
        "swarm.get_status" => handle_get_status(request_id, state).await,
        "swarm.list_swarms" => handle_list_swarms(request_id, state).await,
        "swarm.create_swarm" => {
            handle_create_swarm(request_id, &request.params, state).await
        }
        "swarm.join_swarm" => {
            handle_join_swarm(request_id, &request.params, state).await
        }
        _ => SwarmResponse::error(
            request_id,
            -32601, // Method not found
            format!("Unknown method: {}", request.method),
        ),
    }
}

/// Handle `swarm.connect` - connect to a peer by multiaddress.
async fn handle_connect(
    id: Option<String>,
    params: &serde_json::Value,
    network_handle: &openswarm_network::SwarmHandle,
) -> SwarmResponse {
    let addr_str = match params.get("addr").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => {
            return SwarmResponse::error(
                id,
                -32602,
                "Missing 'addr' parameter".into(),
            );
        }
    };

    let addr: openswarm_network::Multiaddr = match addr_str.parse() {
        Ok(a) => a,
        Err(e) => {
            return SwarmResponse::error(
                id,
                -32602,
                format!("Invalid multiaddress: {}", e),
            );
        }
    };

    match network_handle.dial(addr).await {
        Ok(()) => SwarmResponse::success(id, serde_json::json!({"connected": true})),
        Err(e) => SwarmResponse::error(id, -32000, format!("Dial failed: {}", e)),
    }
}

/// Handle `swarm.get_network_stats` - return current network statistics.
async fn handle_get_network_stats(
    id: Option<String>,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let state = state.read().await;
    let stats = &state.network_stats;

    SwarmResponse::success(
        id,
        serde_json::to_value(stats).unwrap_or_default(),
    )
}

/// Handle `swarm.propose_plan` - submit a task decomposition plan.
async fn handle_propose_plan(
    id: Option<String>,
    params: &serde_json::Value,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let plan: Plan = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return SwarmResponse::error(
                id,
                -32602,
                format!("Invalid plan: {}", e),
            );
        }
    };

    let plan_hash = match openswarm_consensus::RfpCoordinator::compute_plan_hash(&plan) {
        Ok(h) => h,
        Err(e) => {
            return SwarmResponse::error(
                id,
                -32000,
                format!("Hash computation failed: {}", e),
            );
        }
    };

    let _state = state.read().await;

    SwarmResponse::success(
        id,
        serde_json::json!({
            "plan_id": plan.plan_id,
            "plan_hash": plan_hash,
            "task_id": plan.task_id,
            "accepted": true,
        }),
    )
}

/// Handle `swarm.submit_result` - submit a task execution result.
async fn handle_submit_result(
    id: Option<String>,
    params: &serde_json::Value,
    state: &Arc<RwLock<ConnectorState>>,
    network_handle: &openswarm_network::SwarmHandle,
) -> SwarmResponse {
    let submission: ResultSubmissionParams = match serde_json::from_value(params.clone()) {
        Ok(s) => s,
        Err(e) => {
            return SwarmResponse::error(
                id,
                -32602,
                format!("Invalid result submission: {}", e),
            );
        }
    };

    // Add to Merkle DAG.
    {
        let mut state = state.write().await;
        state.merkle_dag.add_leaf(
            submission.task_id.clone(),
            submission.artifact.content_cid.as_bytes(),
        );
    }

    // Publish result to the results topic.
    let topic = SwarmTopics::results(&submission.task_id);
    let msg = SwarmMessage::new(
        ProtocolMethod::ResultSubmission.as_str(),
        serde_json::to_value(&submission).unwrap_or_default(),
        String::new(),
    );
    if let Ok(data) = serde_json::to_vec(&msg) {
        if let Err(e) = network_handle.publish(&topic, data).await {
            tracing::warn!(error = %e, "Failed to publish result");
        }
    }

    SwarmResponse::success(
        id,
        serde_json::json!({
            "task_id": submission.task_id,
            "artifact_id": submission.artifact.artifact_id,
            "accepted": true,
        }),
    )
}

/// Handle `swarm.receive_task` - poll for assigned tasks.
async fn handle_receive_task(
    id: Option<String>,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let state = state.read().await;
    let tasks: Vec<String> = state.task_set.elements();

    SwarmResponse::success(
        id,
        serde_json::json!({
            "pending_tasks": tasks,
            "agent_id": state.agent_id.to_string(),
            "tier": format!("{:?}", state.my_tier),
        }),
    )
}

/// Handle `swarm.get_status` - get connector and agent status.
async fn handle_get_status(
    id: Option<String>,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let state = state.read().await;

    SwarmResponse::success(
        id,
        serde_json::json!({
            "agent_id": state.agent_id.to_string(),
            "status": format!("{:?}", state.status),
            "tier": format!("{:?}", state.my_tier),
            "epoch": state.epoch_manager.current_epoch(),
            "parent_id": state.parent_id.as_ref().map(|p| p.to_string()),
            "active_tasks": state.task_set.len(),
            "known_agents": state.agent_set.len(),
            "content_items": state.content_store.item_count(),
        }),
    )
}

/// Handle `swarm.list_swarms` - list all known swarms with their info.
async fn handle_list_swarms(
    id: Option<String>,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let state = state.read().await;

    let swarms: Vec<serde_json::Value> = state
        .known_swarms
        .values()
        .map(|record| {
            serde_json::json!({
                "swarm_id": record.swarm_id.as_str(),
                "name": record.name,
                "is_public": record.is_public,
                "agent_count": record.agent_count,
                "joined": record.joined,
            })
        })
        .collect();

    SwarmResponse::success(
        id,
        serde_json::json!({
            "swarms": swarms,
            "current_swarm": state.current_swarm_id.as_str(),
        }),
    )
}

/// Handle `swarm.create_swarm` - create a new private swarm.
async fn handle_create_swarm(
    id: Option<String>,
    params: &serde_json::Value,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => {
            return SwarmResponse::error(
                id,
                -32602,
                "Missing 'name' parameter".into(),
            );
        }
    };

    let secret = match params.get("secret").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return SwarmResponse::error(
                id,
                -32602,
                "Missing 'secret' parameter".into(),
            );
        }
    };

    let swarm_id = SwarmId::generate();
    let token = SwarmToken::generate(&swarm_id, &secret);

    let record = SwarmRecord {
        swarm_id: swarm_id.clone(),
        name: name.clone(),
        is_public: false,
        agent_count: 1,
        joined: true,
        last_seen: chrono::Utc::now(),
    };

    {
        let mut state = state.write().await;
        state
            .known_swarms
            .insert(swarm_id.as_str().to_string(), record);
    }

    SwarmResponse::success(
        id,
        serde_json::json!({
            "swarm_id": swarm_id.as_str(),
            "token": token.as_str(),
            "name": name,
        }),
    )
}

/// Handle `swarm.join_swarm` - join an existing swarm.
async fn handle_join_swarm(
    id: Option<String>,
    params: &serde_json::Value,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let swarm_id_str = match params.get("swarm_id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return SwarmResponse::error(
                id,
                -32602,
                "Missing 'swarm_id' parameter".into(),
            );
        }
    };

    let token = params.get("token").and_then(|v| v.as_str()).map(String::from);

    let mut state = state.write().await;

    let record = match state.known_swarms.get_mut(&swarm_id_str) {
        Some(r) => r,
        None => {
            return SwarmResponse::error(
                id,
                -32001,
                format!("Unknown swarm: {}", swarm_id_str),
            );
        }
    };

    // Private swarms require a token.
    if !record.is_public && token.is_none() {
        return SwarmResponse::error(
            id,
            -32602,
            "Token required for private swarm".into(),
        );
    }

    record.joined = true;

    SwarmResponse::success(
        id,
        serde_json::json!({
            "swarm_id": swarm_id_str,
            "joined": true,
        }),
    )
}
