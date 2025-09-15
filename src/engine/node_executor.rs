//! Node execution logic

use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;
use serde_json::Value;

use crate::graph::{Node, NodeType, NodeFunction};
use crate::state::{GraphState, StateData};
use crate::engine::executor::{ExecutionContext, ExecutionMessage, ExecutionError};
use crate::Result;

/// Node executor trait
#[async_trait]
pub trait NodeExecutor: Send + Sync {
    /// Execute a node
    async fn execute(
        &self,
        node: &Node,
        state: &mut StateData,
        context: &ExecutionContext,
    ) -> Result<StateData>;
}

/// Default node executor implementation
pub struct DefaultNodeExecutor;

#[async_trait]
impl NodeExecutor for DefaultNodeExecutor {
    async fn execute(
        &self,
        node: &Node,
        state: &mut StateData,
        context: &ExecutionContext,
    ) -> Result<StateData> {
        match &node.node_type {
            NodeType::Start => {
                // Start node just passes through state
                Ok(state.clone())
            }
            NodeType::End => {
                // End node finalizes state
                Ok(state.clone())
            }
            NodeType::Function(func) => {
                // Execute function node
                self.execute_function(func, state, context).await
            }
            NodeType::Agent(agent_name) => {
                // Execute agent node
                self.execute_agent(&node.id, agent_name, state, context).await
            }
            NodeType::Tool(tool_name) => {
                // Execute tool node
                self.execute_tool(&node.id, tool_name, state, context).await
            }
            NodeType::Conditional => {
                // Execute conditional node
                self.execute_conditional(&node.id, state, context).await
            }
            NodeType::Parallel => {
                // Execute parallel node
                Ok(state.clone())
            }
            NodeType::Subgraph(subgraph_name) => {
                // Execute subgraph
                self.execute_subgraph(&node.id, subgraph_name, state, context).await
            }
            NodeType::Custom(custom_type) => {
                // Execute custom node
                self.execute_custom(&node.id, custom_type, state, context).await
            }
        }
    }
}

impl DefaultNodeExecutor {
    /// Execute a function node
    async fn execute_function(
        &self,
        func: &NodeFunction,
        state: &mut StateData,
        _context: &ExecutionContext,
    ) -> Result<StateData> {
        // Call the function with state
        let result = (func.function)(state);
        Ok(result)
    }
    
    /// Execute an agent node
    async fn execute_agent(
        &self,
        node_id: &str,
        agent_name: &str,
        state: &mut StateData,
        _context: &ExecutionContext,
    ) -> Result<StateData> {
        // TODO: Implement agent execution
        // For now, just return state with agent marker
        state.insert(
            format!("agent_{}_executed", node_id),
            Value::String(agent_name.to_string()),
        );
        Ok(state.clone())
    }
    
    /// Execute a tool node
    async fn execute_tool(
        &self,
        node_id: &str,
        tool_name: &str,
        state: &mut StateData,
        _context: &ExecutionContext,
    ) -> Result<StateData> {
        // TODO: Implement tool execution
        // For now, just return state with tool marker
        state.insert(
            format!("tool_{}_executed", node_id),
            Value::String(tool_name.to_string()),
        );
        Ok(state.clone())
    }
    
    /// Execute a conditional node
    async fn execute_conditional(
        &self,
        node_id: &str,
        state: &mut StateData,
        _context: &ExecutionContext,
    ) -> Result<StateData> {
        // TODO: Implement actual condition evaluation
        // For now, just mark as executed
        state.insert(
            format!("conditional_{}_executed", node_id),
            Value::Bool(true),
        );
        
        Ok(state.clone())
    }
    
    /// Execute a subgraph node
    async fn execute_subgraph(
        &self,
        node_id: &str,
        subgraph_name: &str,
        state: &mut StateData,
        _context: &ExecutionContext,
    ) -> Result<StateData> {
        // TODO: Implement subgraph execution
        state.insert(
            format!("subgraph_{}_executed", node_id),
            Value::String(subgraph_name.to_string()),
        );
        Ok(state.clone())
    }
    
    /// Execute a custom node
    async fn execute_custom(
        &self,
        node_id: &str,
        custom_type: &str,
        state: &mut StateData,
        _context: &ExecutionContext,
    ) -> Result<StateData> {
        // TODO: Implement custom node execution
        state.insert(
            format!("custom_{}_executed", node_id),
            Value::String(custom_type.to_string()),
        );
        Ok(state.clone())
    }
}

/// Parallel node executor for executing multiple nodes concurrently
pub struct ParallelNodeExecutor {
    executor: Arc<dyn NodeExecutor>,
}

impl ParallelNodeExecutor {
    /// Create a new parallel executor
    pub fn new(executor: Arc<dyn NodeExecutor>) -> Self {
        Self { executor }
    }
    
    /// Execute multiple nodes in parallel
    pub async fn execute_parallel(
        &self,
        nodes: Vec<&Node>,
        state: Arc<RwLock<StateData>>,
        context: &ExecutionContext,
    ) -> Result<Vec<StateData>> {
        let mut handles = Vec::new();
        
        for node in nodes {
            let executor = self.executor.clone();
            let state_clone = state.read().await.clone();
            let context_clone = context.clone();
            let node_clone = node.clone();
            
            let handle = tokio::spawn(async move {
                let mut local_state = state_clone;
                executor.execute(&node_clone, &mut local_state, &context_clone).await
            });
            
            handles.push(handle);
        }
        
        // Collect results
        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(Ok(result)) => results.push(result),
                Ok(Err(e)) => return Err(e),
                Err(e) => {
                    return Err(ExecutionError::NodeExecutionFailed(e.to_string()).into())
                }
            }
        }
        
        Ok(results)
    }
}

/// Node execution with retry logic
pub struct RetryNodeExecutor {
    executor: Arc<dyn NodeExecutor>,
    max_retries: u32,
    retry_delay_ms: u64,
}

impl RetryNodeExecutor {
    /// Create a new retry executor
    pub fn new(executor: Arc<dyn NodeExecutor>, max_retries: u32, retry_delay_ms: u64) -> Self {
        Self {
            executor,
            max_retries,
            retry_delay_ms,
        }
    }
    
    /// Execute node with retry logic
    pub async fn execute_with_retry(
        &self,
        node: &Node,
        state: &mut StateData,
        context: &ExecutionContext,
    ) -> Result<StateData> {
        let mut last_error = None;
        
        for attempt in 0..=self.max_retries {
            match self.executor.execute(node, state, context).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.max_retries {
                        tokio::time::sleep(
                            tokio::time::Duration::from_millis(self.retry_delay_ms)
                        ).await;
                    }
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| {
            ExecutionError::NodeExecutionFailed(
                format!("Failed after {} retries", self.max_retries)
            ).into()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    
    #[tokio::test]
    async fn test_default_executor_start_node() {
        let executor = DefaultNodeExecutor;
        let node = Node {
            id: "start".to_string(),
            node_type: NodeType::Start,
            metadata: None,
        };
        
        let mut state = HashMap::new();
        state.insert("test".to_string(), Value::String("value".to_string()));
        
        // Create a mock context
        let graph = crate::graph::StateGraph::new("test");
        let compiled = graph.compile().unwrap();
        let context = ExecutionContext {
            graph: Arc::new(compiled),
            state: Arc::new(RwLock::new(GraphState::new())),
            channels: HashMap::new(),
            execution_id: "test-exec".to_string(),
            metadata: crate::engine::executor::ExecutionMetadata {
                started_at: 0,
                ended_at: None,
                nodes_executed: 0,
                status: crate::engine::executor::ExecutionStatus::Running,
                error: None,
            },
        };
        
        let result = executor.execute(&node, &mut state, &context).await.unwrap();
        assert_eq!(result, state);
    }
}