//! Graph execution engine
//!
//! This module provides the execution runtime for LangGraph workflows.

use async_trait::async_trait;

use crate::Result;

/// Trait for executable graph components
#[async_trait]
pub trait Executable {
    /// Execute the component
    async fn execute(&self) -> Result<()>;
}