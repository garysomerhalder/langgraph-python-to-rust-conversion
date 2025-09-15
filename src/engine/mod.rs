//! Graph execution engine
//!
//! This module provides the execution runtime for LangGraph workflows.

use async_trait::async_trait;

use crate::Result;

pub mod executor;
pub mod node_executor;
pub mod context;
pub mod graph_traversal;

pub use executor::{
    ExecutionEngine, ExecutionContext, ExecutionMessage, 
    ExecutionMetadata, ExecutionStatus, ExecutionError
};
pub use node_executor::{NodeExecutor, DefaultNodeExecutor, ParallelNodeExecutor, RetryNodeExecutor};
pub use context::{SharedContext, ContextConfig, RetryConfig, MessageBus, ExecutionScope};

/// Trait for executable graph components
#[async_trait]
pub trait Executable {
    /// Execute the component
    async fn execute(&self) -> Result<()>;
}