//! Node types and implementations for LangGraph

use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::state::StateData;
use crate::Result;

/// Represents a node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Unique identifier for the node
    pub id: String,

    /// Type of the node
    pub node_type: NodeType,

    /// Optional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Types of nodes supported in the graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeType {
    /// Start node of the graph
    Start,

    /// End node of the graph
    End,

    /// Agent node that performs actions
    Agent(String),

    /// Conditional node for branching
    Conditional,

    /// Parallel execution node
    Parallel,

    /// Tool node for external integrations
    Tool(String),

    /// Subgraph node
    Subgraph(String),

    /// Custom node type
    Custom(String),
}

/// Function signature for node execution
pub type NodeFn =
    Box<dyn Fn(StateData) -> Pin<Box<dyn Future<Output = Result<StateData>> + Send>> + Send + Sync>;

/// Trait for executable nodes
#[async_trait]
pub trait NodeFunction: Send + Sync {
    /// Execute the node with given state
    async fn execute(&self, state: StateData) -> Result<StateData>;

    /// Get node metadata
    fn metadata(&self) -> Option<serde_json::Value> {
        None
    }
}

/// Basic node implementation
pub struct BasicNode {
    pub id: String,
    pub function: NodeFn,
}

#[async_trait]
impl NodeFunction for BasicNode {
    async fn execute(&self, state: StateData) -> Result<StateData> {
        (self.function)(state).await
    }
}

/// Agent node implementation
pub struct AgentNode {
    pub id: String,
    pub agent_name: String,
    pub function: NodeFn,
}

#[async_trait]
impl NodeFunction for AgentNode {
    async fn execute(&self, state: StateData) -> Result<StateData> {
        // Add agent-specific logic here
        (self.function)(state).await
    }

    fn metadata(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "agent": self.agent_name,
            "type": "agent"
        }))
    }
}

/// Tool node implementation
pub struct ToolNode {
    pub id: String,
    pub tool_name: String,
    pub function: NodeFn,
}

#[async_trait]
impl NodeFunction for ToolNode {
    async fn execute(&self, state: StateData) -> Result<StateData> {
        // Add tool-specific logic here
        (self.function)(state).await
    }

    fn metadata(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "tool": self.tool_name,
            "type": "tool"
        }))
    }
}

/// Conditional node for branching logic
pub struct ConditionalNode {
    pub id: String,
    pub condition: Box<dyn Fn(&StateData) -> String + Send + Sync>,
}

#[async_trait]
impl NodeFunction for ConditionalNode {
    async fn execute(&self, state: StateData) -> Result<StateData> {
        // The condition determines the next node
        // This is handled by the execution engine
        Ok(state)
    }

    fn metadata(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "conditional"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let node = Node {
            id: "test_node".to_string(),
            node_type: NodeType::Agent("test_agent".to_string()),
            metadata: Some(serde_json::json!({"key": "value"})),
        };

        assert_eq!(node.id, "test_node");
        assert_eq!(node.node_type, NodeType::Agent("test_agent".to_string()));
        assert!(node.metadata.is_some());
    }

    #[test]
    fn test_node_type_equality() {
        assert_eq!(NodeType::Start, NodeType::Start);
        assert_eq!(NodeType::End, NodeType::End);
        assert_eq!(
            NodeType::Agent("agent1".to_string()),
            NodeType::Agent("agent1".to_string())
        );
        assert_ne!(
            NodeType::Agent("agent1".to_string()),
            NodeType::Agent("agent2".to_string())
        );
    }

    #[tokio::test]
    async fn test_basic_node_execution() {
        let node = BasicNode {
            id: "test".to_string(),
            function: Box::new(|mut state| {
                Box::pin(async move {
                    state.insert("result".to_string(), serde_json::json!("success"));
                    Ok(state)
                })
            }),
        };

        let mut state = StateData::new();
        state.insert("input".to_string(), serde_json::json!("test"));

        let result = node.execute(state).await.unwrap();
        assert_eq!(result.get("result"), Some(&serde_json::json!("success")));
    }
}
