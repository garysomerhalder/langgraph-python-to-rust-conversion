//! Graph execution engine implementation

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::graph::{CompiledGraph, StateGraph, Node, NodeType};
use crate::state::{GraphState, StateData};
use crate::Result;

/// Errors specific to execution
#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("Node execution failed: {0}")]
    NodeExecutionFailed(String),
    
    #[error("Execution timeout: {0}")]
    Timeout(String),
    
    #[error("Execution cancelled")]
    Cancelled,
    
    #[error("Invalid execution state: {0}")]
    InvalidState(String),
    
    #[error("Message passing error: {0}")]
    MessageError(String),
}

/// Message passed between nodes during execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMessage {
    /// Source node ID
    pub from: String,
    
    /// Target node ID
    pub to: String,
    
    /// Message payload
    pub payload: StateData,
    
    /// Message metadata
    pub metadata: Option<serde_json::Value>,
}

/// Execution context for a running graph
#[derive(Clone)]
pub struct ExecutionContext {
    /// Graph being executed
    pub graph: Arc<CompiledGraph>,
    
    /// Current execution state
    pub state: Arc<RwLock<GraphState>>,
    
    /// Message channels for nodes
    pub channels: HashMap<String, mpsc::Sender<ExecutionMessage>>,
    
    /// Execution ID
    pub execution_id: String,
    
    /// Execution metadata
    pub metadata: ExecutionMetadata,
}

/// Metadata about an execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    /// Start timestamp
    pub started_at: u64,
    
    /// End timestamp (if completed)
    pub ended_at: Option<u64>,
    
    /// Total nodes executed
    pub nodes_executed: usize,
    
    /// Execution status
    pub status: ExecutionStatus,
    
    /// Error message if failed
    pub error: Option<String>,
}

/// Status of an execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    /// Execution is pending
    Pending,
    
    /// Execution is running
    Running,
    
    /// Execution completed successfully
    Completed,
    
    /// Execution failed
    Failed,
    
    /// Execution was cancelled
    Cancelled,
}

/// Main execution engine for running graphs
pub struct ExecutionEngine {
    /// Active executions
    active_executions: Arc<RwLock<HashMap<String, ExecutionContext>>>,
    
    /// Execution history
    history: Arc<RwLock<Vec<ExecutionMetadata>>>,
}

impl ExecutionEngine {
    /// Create a new execution engine
    pub fn new() -> Self {
        Self {
            active_executions: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Execute a compiled graph with input
    pub async fn execute(
        &self,
        graph: CompiledGraph,
        input: StateData,
    ) -> Result<StateData> {
        let execution_id = generate_execution_id();
        let context = self.create_context(graph, input, execution_id.clone()).await?;
        
        // Store active execution
        {
            let mut active = self.active_executions.write().await;
            active.insert(execution_id.clone(), context);
        }
        
        // Run execution
        let result = self.run_execution(&execution_id).await;
        
        // Clean up
        {
            let mut active = self.active_executions.write().await;
            if let Some(context) = active.remove(&execution_id) {
                let mut history = self.history.write().await;
                history.push(context.metadata);
            }
        }
        
        result
    }
    
    /// Stream execution of a graph
    pub async fn stream(
        &self,
        graph: CompiledGraph,
        input: StateData,
    ) -> Result<mpsc::Receiver<ExecutionMessage>> {
        let (tx, rx) = mpsc::channel(100);
        let execution_id = generate_execution_id();
        let context = self.create_context(graph, input, execution_id.clone()).await?;
        
        // Store active execution
        {
            let mut active = self.active_executions.write().await;
            active.insert(execution_id.clone(), context);
        }
        
        // Spawn execution task
        let engine = self.clone();
        tokio::spawn(async move {
            let _ = engine.run_streaming_execution(&execution_id, tx).await;
        });
        
        Ok(rx)
    }
    
    /// Create execution context
    async fn create_context(
        &self,
        graph: CompiledGraph,
        input: StateData,
        execution_id: String,
    ) -> Result<ExecutionContext> {
        let mut state = GraphState::new();
        state.update(input);
        
        let metadata = ExecutionMetadata {
            started_at: current_timestamp(),
            ended_at: None,
            nodes_executed: 0,
            status: ExecutionStatus::Pending,
            error: None,
        };
        
        Ok(ExecutionContext {
            graph: Arc::new(graph),
            state: Arc::new(RwLock::new(state)),
            channels: HashMap::new(),
            execution_id,
            metadata,
        })
    }
    
    /// Run an execution
    async fn run_execution(&self, execution_id: &str) -> Result<StateData> {
        // Get context
        let context = {
            let active = self.active_executions.read().await;
            active.get(execution_id).cloned()
        };
        
        let context = context.ok_or_else(|| {
            ExecutionError::InvalidState(format!("Execution {} not found", execution_id))
        })?;
        
        // Update status
        self.update_execution_status(execution_id, ExecutionStatus::Running).await?;
        
        // Execute graph
        // TODO: Implement actual graph traversal and node execution
        let result = self.execute_graph(&context).await?;
        
        // Update status
        self.update_execution_status(execution_id, ExecutionStatus::Completed).await?;
        
        Ok(result)
    }
    
    /// Run streaming execution
    async fn run_streaming_execution(
        &self,
        execution_id: &str,
        tx: mpsc::Sender<ExecutionMessage>,
    ) -> Result<()> {
        // Get context
        let context = {
            let active = self.active_executions.read().await;
            active.get(execution_id).cloned()
        };
        
        let context = context.ok_or_else(|| {
            ExecutionError::InvalidState(format!("Execution {} not found", execution_id))
        })?;
        
        // Update status
        self.update_execution_status(execution_id, ExecutionStatus::Running).await?;
        
        // Execute graph with streaming
        // TODO: Implement streaming execution
        self.execute_graph_streaming(&context, tx).await?;
        
        // Update status
        self.update_execution_status(execution_id, ExecutionStatus::Completed).await?;
        
        Ok(())
    }
    
    /// Execute the graph
    async fn execute_graph(&self, context: &ExecutionContext) -> Result<StateData> {
        use crate::engine::node_executor::{DefaultNodeExecutor, NodeExecutor};
        
        let executor = DefaultNodeExecutor;
        let mut current_node = "__start__".to_string();
        let mut visited = std::collections::HashSet::new();
        let max_iterations = 100; // Prevent infinite loops
        let mut iterations = 0;
        
        while current_node != "__end__" && iterations < max_iterations {
            iterations += 1;
            
            // Prevent cycles
            if visited.contains(&current_node) {
                return Err(ExecutionError::InvalidState(
                    format!("Cycle detected at node: {}", current_node)
                ).into());
            }
            visited.insert(current_node.clone());
            
            // Get the node from the graph
            let node = context.graph.graph().get_node(&current_node)
                .ok_or_else(|| ExecutionError::NodeExecutionFailed(
                    format!("Node not found: {}", current_node)
                ))?;
            
            // Execute the node
            let mut state = context.state.write().await;
            let mut state_data = state.snapshot();
            let result = executor.execute(node, &mut state_data, context).await?;
            state.update(result);
            
            // Find next node(s) based on edges
            let edges = context.graph.graph().get_edges_from(&current_node);
            
            if edges.is_empty() {
                // No more edges, we're done
                break;
            } else if edges.len() == 1 {
                // Single edge, follow it
                current_node = edges[0].0.id.clone();
            } else {
                // Multiple edges - would need conditional logic here
                // For now, just take the first one
                current_node = edges[0].0.id.clone();
            }
            
            // Update context metadata
            let mut active = self.active_executions.write().await;
            if let Some(ctx) = active.get_mut(&context.execution_id) {
                ctx.metadata.nodes_executed += 1;
            }
        }
        
        if iterations >= max_iterations {
            return Err(ExecutionError::Timeout(
                "Maximum iterations reached in graph execution".to_string()
            ).into());
        }
        
        // Return final state
        let state = context.state.read().await;
        Ok(state.snapshot())
    }
    
    /// Execute graph with streaming
    async fn execute_graph_streaming(
        &self,
        context: &ExecutionContext,
        tx: mpsc::Sender<ExecutionMessage>,
    ) -> Result<()> {
        use crate::engine::node_executor::{DefaultNodeExecutor, NodeExecutor};
        
        let executor = DefaultNodeExecutor;
        let mut current_node = "__start__".to_string();
        let mut visited = HashSet::new();
        let max_iterations = 100;
        let mut iterations = 0;
        
        while current_node != "__end__" && iterations < max_iterations {
            iterations += 1;
            
            // Prevent cycles
            if visited.contains(&current_node) {
                return Err(ExecutionError::InvalidState(
                    format!("Cycle detected at node: {}", current_node)
                ).into());
            }
            visited.insert(current_node.clone());
            
            // Get the node from the graph
            let node = context.graph.graph().get_node(&current_node)
                .ok_or_else(|| ExecutionError::NodeExecutionFailed(
                    format!("Node not found: {}", current_node)
                ))?;
            
            // Execute the node
            let mut state = context.state.write().await;
            let mut state_data = state.snapshot();
            let result = executor.execute(node, &mut state_data, context).await?;
            state.update(result.clone());
            
            // Find next node(s) based on edges
            let edges = context.graph.graph().get_edges_from(&current_node);
            
            let next_node = if edges.is_empty() {
                "__end__".to_string()
            } else if edges.len() == 1 {
                edges[0].0.id.clone()
            } else {
                // Multiple edges - would need conditional logic here
                edges[0].0.id.clone()
            };
            
            // Send execution message
            let msg = ExecutionMessage {
                from: current_node.clone(),
                to: next_node.clone(),
                payload: result,
                metadata: Some(serde_json::json!({
                    "iteration": iterations,
                    "node_type": format!("{:?}", node.node_type)
                })),
            };
            
            tx.send(msg).await.map_err(|e| {
                ExecutionError::MessageError(e.to_string())
            })?;
            
            // Update context metadata
            let mut active = self.active_executions.write().await;
            if let Some(ctx) = active.get_mut(&context.execution_id) {
                ctx.metadata.nodes_executed += 1;
            }
            
            if edges.is_empty() {
                break;
            }
            
            current_node = next_node;
        }
        
        if iterations >= max_iterations {
            return Err(ExecutionError::Timeout(
                "Maximum iterations reached in streaming execution".to_string()
            ).into());
        }
        
        Ok(())
    }
    
    /// Update execution status
    async fn update_execution_status(
        &self,
        execution_id: &str,
        status: ExecutionStatus,
    ) -> Result<()> {
        let mut active = self.active_executions.write().await;
        if let Some(context) = active.get_mut(execution_id) {
            context.metadata.status = status.clone();
            if status == ExecutionStatus::Completed || status == ExecutionStatus::Failed {
                context.metadata.ended_at = Some(current_timestamp());
            }
        }
        Ok(())
    }
}

impl Clone for ExecutionEngine {
    fn clone(&self) -> Self {
        Self {
            active_executions: self.active_executions.clone(),
            history: self.history.clone(),
        }
    }
}

impl Default for ExecutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a unique execution ID
fn generate_execution_id() -> String {
    use uuid::Uuid;
    format!("exec-{}", Uuid::new_v4())
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_execution_engine_creation() {
        let engine = ExecutionEngine::new();
        assert!(engine.active_executions.read().await.is_empty());
        assert!(engine.history.read().await.is_empty());
    }
    
    #[tokio::test]
    async fn test_execution_id_generation() {
        let id1 = generate_execution_id();
        let id2 = generate_execution_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("exec-"));
        assert!(id2.starts_with("exec-"));
    }
}