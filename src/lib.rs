//! # LangGraph Rust Implementation
//! 
//! A Rust implementation of LangGraph for building stateful, multi-agent applications.
//! 
//! ## Overview
//! 
//! LangGraph is a library for building stateful, multi-agent applications with Large Language Models (LLMs).
//! It provides a graph-based approach to orchestrating complex workflows and agent interactions.
//! 
//! ## Key Features
//! 
//! - **Graph-based workflows**: Define complex agent interactions as directed graphs
//! - **State management**: Built-in state persistence and checkpointing
//! - **Async execution**: Fully async/await compatible with Tokio runtime
//! - **Type safety**: Leverage Rust's type system for compile-time guarantees
//! - **Extensible**: Easy to add custom nodes, edges, and state types

#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

use thiserror::Error;

/// Result type for LangGraph operations
pub type Result<T> = std::result::Result<T, LangGraphError>;

/// Main error type for LangGraph operations
#[derive(Error, Debug)]
pub enum LangGraphError {
    /// Graph structure error (cycles, missing nodes, etc.)
    #[error("Graph structure error: {0}")]
    GraphStructure(String),
    
    /// State management error
    #[error("State error: {0}")]
    State(String),
    
    /// Execution error during graph traversal
    #[error("Execution error: {0}")]
    Execution(String),
    
    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    /// Generic error for unexpected conditions
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Core graph module containing graph structures and algorithms
pub mod graph {
    //! Graph data structures and algorithms
    
    use serde::{Deserialize, Serialize};
    
    /// Represents a node in the graph
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Node {
        /// Unique identifier for the node
        pub id: String,
        /// Node type/kind
        pub node_type: NodeType,
        /// Optional metadata
        pub metadata: Option<serde_json::Value>,
    }
    
    /// Types of nodes supported in the graph
    #[derive(Debug, Clone, Serialize, Deserialize)]
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
    }
}

/// State management module
pub mod state {
    //! State management for graph execution
    
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    
    /// Represents the state of a graph execution
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GraphState {
        /// Current values in the state
        pub values: HashMap<String, serde_json::Value>,
        /// Execution history
        pub history: Vec<StateTransition>,
    }
    
    /// Represents a state transition
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StateTransition {
        /// Source node ID
        pub from: String,
        /// Target node ID
        pub to: String,
        /// Timestamp of transition
        pub timestamp: u64,
        /// Optional transition metadata
        pub metadata: Option<serde_json::Value>,
    }
}

/// Execution engine module
pub mod engine {
    //! Graph execution engine
    
    use async_trait::async_trait;
    
    /// Trait for executable graph components
    #[async_trait]
    pub trait Executable {
        /// Execute the component
        async fn execute(&self) -> crate::Result<()>;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_node_creation() {
        let node = graph::Node {
            id: "test_node".to_string(),
            node_type: graph::NodeType::Start,
            metadata: None,
        };
        
        assert_eq!(node.id, "test_node");
        matches!(node.node_type, graph::NodeType::Start);
    }
    
    #[test]
    fn test_state_initialization() {
        let state = state::GraphState {
            values: std::collections::HashMap::new(),
            history: Vec::new(),
        };
        
        assert!(state.values.is_empty());
        assert!(state.history.is_empty());
    }
}