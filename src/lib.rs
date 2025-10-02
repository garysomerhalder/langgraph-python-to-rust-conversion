//! # LangGraph Rust Implementation
//! 
//! A high-performance Rust implementation of LangGraph for building stateful, multi-agent applications
//! with Large Language Models (LLMs).
//! 
//! ## Overview
//! 
//! LangGraph provides a graph-based approach to orchestrating complex workflows and agent interactions.
//! This Rust implementation offers type safety, high performance, and seamless async execution.
//! 
//! ## Quick Start
//! 
//! ```rust
//! use langgraph::graph::{GraphBuilder, NodeType};
//! use langgraph::state::GraphState;
//! use serde_json::json;
//! 
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Build a simple workflow
//! let graph = GraphBuilder::new("my_workflow")
//!     .add_node("__start__", NodeType::Start)
//!     .add_node("process", NodeType::Agent("processor".to_string()))
//!     .add_node("__end__", NodeType::End)
//!     .set_entry_point("__start__")
//!     .add_edge("__start__", "process")
//!     .add_edge("process", "__end__")
//!     .build()?
//!     .compile()?;
//! 
//! // Execute with state
//! let mut state = GraphState::new();
//! state.set("input", json!("Hello, World!"));
//! 
//! let result = graph.invoke(state.values).await?;
//! # Ok(())
//! # }
//! ```
//! 
//! ## Key Features
//! 
//! - **Graph-based workflows**: Define complex agent interactions as directed graphs
//! - **State management**: Built-in state persistence with versioning and snapshots
//! - **Async execution**: Fully async/await compatible with Tokio runtime
//! - **Type safety**: Leverage Rust's type system for compile-time guarantees
//! - **Conditional routing**: Dynamic graph traversal based on state
//! - **Tool integration**: Extensible framework for external tools
//! - **Multi-agent coordination**: Build collaborative agent systems
//! 
//! ## Modules
//! 
//! - [`graph`]: Core graph structures and builders
//! - [`state`]: State management with channels and reducers
//! - [`engine`]: Execution engine for graph traversal
//! - [`stream`]: Async streaming with backpressure
//! - [`tools`]: Tool integration framework
//! - [`agents`]: Intelligent agents with reasoning
//! - [`checkpoint`]: State persistence and recovery

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

    /// Schema validation error
    #[error("Schema validation error: {0}")]
    Schema(#[from] state::SchemaError),
    
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
    
    /// Tool errors
    #[error("Tool error: {0}")]
    Tool(#[from] tools::ToolError),
    
    /// Agent errors
    #[error("Agent error: {0}")]
    Agent(#[from] agents::AgentError),
    
    /// Advanced state errors
    #[error("Advanced state error: {0}")]
    AdvancedState(#[from] state::advanced::StateError),
    
    /// Join error from async tasks
    #[error("Async join error: {0}")]
    Join(#[from] tokio::task::JoinError),
    
    /// Generic error for unexpected conditions
    #[error("Internal error: {0}")]
    Internal(String),
    
    /// Graph validation error
    #[error("Graph validation error: {0}")]
    GraphValidation(String),
    
    /// State error
    #[error("State error: {0}")]
    StateError(String),

    /// Human-in-the-loop interrupt error
    #[error("Interrupt error: {0}")]
    Interrupt(#[from] engine::InterruptError),

    /// Breakpoint error
    #[error("Breakpoint error: {0}")]
    Breakpoint(#[from] engine::BreakpointError),
}

/// Core graph module containing graph structures and algorithms
pub mod graph;

/// State management module
pub mod state;

/// Execution engine module
pub mod engine;

/// Checkpointing and persistence module
pub mod checkpoint;

/// Streaming and channels module
pub mod stream;

/// Tool integration module
pub mod tools;

/// Agent capabilities module
pub mod agents;

/// Utility functions and helpers
pub mod utils;

/// Message-based graph execution
pub mod message;

/// Security (authentication and authorization)
pub mod security;

#[cfg(feature = "s3")]
pub mod backup;

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