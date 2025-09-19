//! Human-in-the-loop implementation for ExecutionEngine
//! This file extends ExecutionEngine with interrupt capabilities

use async_trait::async_trait;
use tokio::sync::oneshot;
use std::sync::Arc;
use std::time::Duration;

use crate::engine::{
    ExecutionEngine, ExecutionContext, ExecutionStatus,
    InterruptManager, InterruptMode, InterruptCallback, HumanInLoopExecution,
    ExecutionHandle, ApprovalDecision,
};
use crate::graph::CompiledGraph;
use crate::state::StateData;
use crate::Result;

/// Implementation of HumanInLoopExecution for ExecutionEngine
#[async_trait]
impl HumanInLoopExecution for ExecutionEngine {
    /// Execute with interrupt support
    async fn execute_with_interrupts(
        &self,
        input: StateData,
        callback: InterruptCallback,
    ) -> Result<ExecutionHandle> {
        // Create a channel for the result
        let (result_tx, result_rx) = oneshot::channel();

        // Register the callback with the interrupt manager
        {
            let manager = self.interrupt_manager.read().await;
            manager.register_callback(callback).await;
        }

        // Create execution handle
        let handle = ExecutionHandle::new(result_rx);

        // Clone necessary data for async execution
        let engine = self.clone();
        let input_clone = input.clone();

        // Spawn the execution task
        tokio::spawn(async move {
            // Execute the graph with interrupt checks
            let result = engine.execute_with_interrupt_checks(input_clone).await;

            // Send the result back
            let _ = result_tx.send(result);
        });

        Ok(handle)
    }

    /// Execute with interrupts and timeout
    async fn execute_with_interrupts_and_timeout(
        &self,
        input: StateData,
        callback: InterruptCallback,
        timeout: Duration,
    ) -> Result<ExecutionHandle> {
        // Create a channel for the result
        let (result_tx, result_rx) = oneshot::channel();

        // Register the callback with the interrupt manager
        {
            let manager = self.interrupt_manager.read().await;
            manager.register_callback(callback).await;
        }

        // Create execution handle
        let handle = ExecutionHandle::new(result_rx);

        // Clone necessary data for async execution
        let engine = self.clone();
        let input_clone = input.clone();

        // Spawn the execution task with timeout
        tokio::spawn(async move {
            let result = tokio::time::timeout(timeout, async {
                engine.execute_with_interrupt_checks(input_clone).await
            })
            .await;

            // Handle timeout and send result
            let final_result = match result {
                Ok(execution_result) => execution_result,
                Err(_) => Err(crate::engine::InterruptError::Timeout(timeout).into()),
            };

            let _ = result_tx.send(final_result);
        });

        Ok(handle)
    }
}

impl ExecutionEngine {
    /// Internal method to execute with interrupt checks
    pub(crate) async fn execute_with_interrupt_checks(
        &self,
        input: StateData,
    ) -> Result<StateData> {
        // This is a simplified implementation for YELLOW phase
        // In production, this would integrate with the actual graph execution

        // For now, just simulate execution with state
        let mut state = input.clone();

        // Check for interrupts at a simulated node
        {
            let manager = self.interrupt_manager.read().await;

            // Simulate checking for interrupts
            if manager.should_interrupt("review", InterruptMode::Before) {
                let decision = manager.create_interrupt(
                    "review".to_string(),
                    state.clone(),
                    None,
                ).await?;

                match decision {
                    ApprovalDecision::Continue => {
                        // Continue execution
                    }
                    ApprovalDecision::Abort(reason) => {
                        return Err(crate::engine::InterruptError::Cancelled(reason).into());
                    }
                    ApprovalDecision::Redirect(node) => {
                        // In full implementation, would redirect to different node
                        state.insert("redirected".to_string(), serde_json::json!(true));
                        state.insert("redirect_to".to_string(), serde_json::json!(node));
                    }
                    _ => {
                        // Handle other decisions
                    }
                }
            }
        }

        // Simulate successful execution
        state.insert("completed".to_string(), serde_json::json!(true));

        Ok(state)
    }
}

// Note: ExecutionEngine already has Clone derived or implemented elsewhere