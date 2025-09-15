//! Graph execution engine
//!
//! This module provides the execution runtime for LangGraph workflows.

use async_trait::async_trait;

use crate::Result;

pub mod executor;
pub mod node_executor;
pub mod context;
pub mod graph_traversal;
pub mod parallel_executor;

pub use executor::{
    ExecutionEngine, ExecutionContext, ExecutionMessage, 
    ExecutionMetadata, ExecutionStatus, ExecutionError
};
pub use node_executor::{NodeExecutor, DefaultNodeExecutor, ParallelNodeExecutor, RetryNodeExecutor};
pub use context::{SharedContext, ContextConfig, RetryConfig, MessageBus, ExecutionScope};
pub use parallel_executor::{
    ParallelExecutor, ExecutionMetrics, DependencyAnalyzer,
    StateVersionManager, StateVersion, DeadlockDetector
};

/// Trait for executable graph components
#[async_trait]
pub trait Executable {
    /// Execute the component
    async fn execute(&self) -> Result<()>;
}