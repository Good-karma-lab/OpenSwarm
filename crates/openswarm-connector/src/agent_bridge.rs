//! Bridge between the connector and the AI agent.
//!
//! The AgentBridge formats tasks for the AI agent (OpenClaw), intercepts
//! results, and provides MCP (Model Context Protocol) compatibility
//! for standardized tool invocation.
//!
//! Responsibilities:
//! - Convert protocol tasks into agent-readable prompts/instructions
//! - Convert agent outputs into protocol-compliant artifacts
//! - Provide MCP tool definitions for the agent to call swarm operations
//! - Buffer and manage the task queue for the local agent

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use openswarm_protocol::{AgentId, Artifact, Task};

/// A task formatted for the AI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    /// The underlying protocol task.
    pub task: Task,
    /// Human-readable instructions derived from the task description.
    pub instructions: String,
    /// Context about the task's position in the hierarchy.
    pub context: TaskContext,
    /// Available tools/capabilities for this task.
    pub available_tools: Vec<ToolDefinition>,
}

/// Context about the task's hierarchical position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    /// The parent task description (if any).
    pub parent_description: Option<String>,
    /// Sibling tasks being worked on in parallel.
    pub sibling_task_ids: Vec<String>,
    /// The tier at which this task is being executed.
    pub tier_level: u32,
    /// The current epoch.
    pub epoch: u64,
    /// Deadline for this task.
    pub deadline_hint: Option<String>,
}

/// An MCP-compatible tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name.
    pub name: String,
    /// Description of what the tool does.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
}

/// Result from the AI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    /// The task ID this result is for.
    pub task_id: String,
    /// The agent's output content.
    pub content: Vec<u8>,
    /// Content type (MIME).
    pub content_type: String,
    /// Optional structured metadata.
    pub metadata: serde_json::Value,
    /// Whether the task was completed successfully.
    pub success: bool,
    /// Error message if not successful.
    pub error: Option<String>,
}

/// Trait for the AI agent executor.
///
/// Implementations connect to different AI backends for task execution.
/// The connector calls `execute_task()` when a task is assigned to this node.
pub trait AgentExecutor: Send + Sync {
    /// Execute a task and return the result.
    fn execute_task<'a>(
        &'a self,
        task: &'a AgentTask,
    ) -> Pin<Box<dyn Future<Output = Result<AgentResult, AgentBridgeError>> + Send + 'a>>;

    /// Generate a decomposition plan for a task (for coordinator-tier agents).
    fn generate_plan<'a>(
        &'a self,
        task: &'a AgentTask,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, AgentBridgeError>> + Send + 'a>>;
}

/// Errors from the agent bridge.
#[derive(Debug, thiserror::Error)]
pub enum AgentBridgeError {
    #[error("Agent execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Task queue full")]
    QueueFull,

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("MCP protocol error: {0}")]
    McpError(String),
}

/// Bridge between the OpenSwarm connector and the local AI agent.
///
/// Manages the task queue, formats tasks for the agent, and converts
/// agent outputs into protocol-compliant artifacts.
pub struct AgentBridge {
    /// Our agent identity.
    agent_id: AgentId,
    /// Queue of tasks waiting to be executed.
    task_queue: VecDeque<AgentTask>,
    /// Maximum queue size.
    max_queue_size: usize,
    /// Whether MCP compatibility mode is enabled.
    mcp_compatible: bool,
    /// MCP tool definitions available to the agent.
    mcp_tools: Vec<ToolDefinition>,
    /// Current epoch for context.
    current_epoch: u64,
}

impl AgentBridge {
    /// Create a new agent bridge.
    pub fn new(agent_id: AgentId, mcp_compatible: bool) -> Self {
        let mcp_tools = if mcp_compatible {
            Self::build_mcp_tools()
        } else {
            Vec::new()
        };

        Self {
            agent_id,
            task_queue: VecDeque::new(),
            max_queue_size: 100,
            mcp_compatible,
            mcp_tools,
            current_epoch: 1,
        }
    }

    /// Enqueue a task for the agent to execute.
    pub fn enqueue_task(
        &mut self,
        task: Task,
        parent_description: Option<String>,
        sibling_ids: Vec<String>,
    ) -> Result<(), AgentBridgeError> {
        if self.task_queue.len() >= self.max_queue_size {
            return Err(AgentBridgeError::QueueFull);
        }

        let agent_task = AgentTask {
            instructions: self.format_instructions(&task),
            context: TaskContext {
                parent_description,
                sibling_task_ids: sibling_ids,
                tier_level: task.tier_level,
                epoch: self.current_epoch,
                deadline_hint: task.deadline.map(|d| d.to_rfc3339()),
            },
            available_tools: self.mcp_tools.clone(),
            task,
        };

        self.task_queue.push_back(agent_task);
        Ok(())
    }

    /// Dequeue the next task for the agent.
    pub fn dequeue_task(&mut self) -> Option<AgentTask> {
        self.task_queue.pop_front()
    }

    /// Peek at the next task without removing it.
    pub fn peek_task(&self) -> Option<&AgentTask> {
        self.task_queue.front()
    }

    /// Get the number of tasks in the queue.
    pub fn queue_len(&self) -> usize {
        self.task_queue.len()
    }

    /// Convert an agent result into a protocol Artifact.
    pub fn result_to_artifact(&self, result: &AgentResult) -> Artifact {
        let content_cid = openswarm_protocol::crypto::compute_cid(&result.content);
        let merkle_hash = content_cid.clone(); // Simplified; full implementation chains hashes.

        Artifact {
            artifact_id: uuid::Uuid::new_v4().to_string(),
            task_id: result.task_id.clone(),
            producer: self.agent_id.clone(),
            content_cid,
            merkle_hash,
            content_type: result.content_type.clone(),
            size_bytes: result.content.len() as u64,
            created_at: Utc::now(),
        }
    }

    /// Format a task description into agent-readable instructions.
    fn format_instructions(&self, task: &Task) -> String {
        let mut instructions = format!(
            "## Task: {}\n\nDescription: {}\n",
            task.task_id, task.description
        );

        if !task.subtasks.is_empty() {
            instructions.push_str("\nSubtask IDs:\n");
            for st in &task.subtasks {
                instructions.push_str(&format!("  - {}\n", st));
            }
        }

        if let Some(deadline) = &task.deadline {
            instructions.push_str(&format!("\nDeadline: {}\n", deadline));
        }

        instructions
    }

    /// Build MCP-compatible tool definitions for swarm operations.
    fn build_mcp_tools() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "swarm_submit_result".to_string(),
                description: "Submit the result of task execution to the swarm".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_id": { "type": "string", "description": "The task ID" },
                        "content": { "type": "string", "description": "The result content" },
                        "content_type": { "type": "string", "description": "MIME type of the content" }
                    },
                    "required": ["task_id", "content"]
                }),
            },
            ToolDefinition {
                name: "swarm_get_status".to_string(),
                description: "Get the current swarm status and agent information".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "swarm_propose_plan".to_string(),
                description: "Propose a task decomposition plan".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_id": { "type": "string", "description": "The task to decompose" },
                        "subtasks": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "description": { "type": "string" },
                                    "capabilities": { "type": "array", "items": { "type": "string" } },
                                    "complexity": { "type": "number" }
                                }
                            },
                            "description": "Proposed subtasks"
                        },
                        "rationale": { "type": "string", "description": "Explanation of the plan" }
                    },
                    "required": ["task_id", "subtasks"]
                }),
            },
            ToolDefinition {
                name: "swarm_query_peers".to_string(),
                description: "Query information about connected peers in the swarm".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        ]
    }

    /// Update the current epoch (for context in formatted tasks).
    pub fn set_epoch(&mut self, epoch: u64) {
        self.current_epoch = epoch;
    }

    /// Check if MCP compatibility mode is enabled.
    pub fn is_mcp_compatible(&self) -> bool {
        self.mcp_compatible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enqueue_and_dequeue() {
        let mut bridge = AgentBridge::new(
            AgentId::new("test-agent".into()),
            false,
        );

        let task = Task::new("Test task".into(), 1, 1);
        bridge.enqueue_task(task, None, vec![]).unwrap();

        assert_eq!(bridge.queue_len(), 1);

        let agent_task = bridge.dequeue_task().unwrap();
        assert!(agent_task.instructions.contains("Test task"));
        assert_eq!(bridge.queue_len(), 0);
    }

    #[test]
    fn test_result_to_artifact() {
        let bridge = AgentBridge::new(
            AgentId::new("test-agent".into()),
            false,
        );

        let result = AgentResult {
            task_id: "task1".into(),
            content: b"Hello, world!".to_vec(),
            content_type: "text/plain".into(),
            metadata: serde_json::Value::Null,
            success: true,
            error: None,
        };

        let artifact = bridge.result_to_artifact(&result);
        assert_eq!(artifact.task_id, "task1");
        assert!(!artifact.content_cid.is_empty());
        assert_eq!(artifact.size_bytes, 13);
    }

    #[test]
    fn test_mcp_tools_generated() {
        let bridge = AgentBridge::new(
            AgentId::new("test-agent".into()),
            true,
        );

        assert!(!bridge.mcp_tools.is_empty());
        assert!(bridge.mcp_tools.iter().any(|t| t.name == "swarm_submit_result"));
        assert!(bridge.mcp_tools.iter().any(|t| t.name == "swarm_propose_plan"));
    }
}
