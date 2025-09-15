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
    
    /// Checkpoint error
    #[error("Checkpoint error: {0}")]
    Checkpoint(String),
    
    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    /// Graph-specific errors
    #[error("Graph error: {0}")]
    Graph(#[from] graph::GraphError),
    
    /// Engine execution errors
    #[error("Engine error: {0}")]
    Engine(#[from] engine::ExecutionError),
    
    /// Generic error for unexpected conditions
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Core graph module containing graph structures and algorithms
pub mod graph;

/// State management module
pub mod state;

/// Execution engine module
pub mod engine;

/// Checkpointing and persistence module
pub mod checkpoint;

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
        let state = state::GraphState::new();
        
        assert!(state.values.is_empty());
        assert!(state.history.is_empty());
        assert!(state.current_node.is_none());
        assert!(state.thread_id.is_none());
    }
}