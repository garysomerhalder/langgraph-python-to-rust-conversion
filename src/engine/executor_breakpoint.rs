//! Breakpoint integration for ExecutionEngine
//! YELLOW Phase: Minimal implementation for breakpoint support

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::engine::{
    ExecutionEngine, BreakpointManager, BreakpointExecution,
    BreakpointCallback, BreakpointHit, BreakpointAction,
    ExecutionHandle, InterruptCallback,
};
use crate::graph::CompiledGraph;
use crate::state::StateData;
use crate::Result;

/// Implementation of BreakpointExecution for ExecutionEngine
impl BreakpointExecution for ExecutionEngine {
    /// Get the breakpoint manager
    fn get_breakpoint_manager(&self) -> Arc<BreakpointManager> {
        // For YELLOW phase, create a new manager each time
        // In GREEN phase, this would be stored as a field in ExecutionEngine
        Arc::new(BreakpointManager::new())
    }

    /// Execute with breakpoint support
    async fn execute_with_breakpoints(
        &self,
        graph: CompiledGraph,
        input: StateData,
        callback: BreakpointCallback,
    ) -> Result<ExecutionHandle> {
        // Create a channel for the result
        let (result_tx, result_rx) = oneshot::channel();

        // Get breakpoint manager and register callback
        let bp_manager = self.get_breakpoint_manager();
        bp_manager.register_callback(callback).await;

        // Create execution handle
        let handle = ExecutionHandle::new(result_rx);

        // Clone necessary data for async execution
        let engine = self.clone();
        let bp_manager_clone = bp_manager.clone();

        // Spawn the execution task
        tokio::spawn(async move {
            // Execute the graph with breakpoint checks
            let result = engine.execute_graph_with_breakpoints_internal(
                graph,
                input,
                bp_manager_clone,
            ).await;

            // Send the result back
            let _ = result_tx.send(result);
        });

        Ok(handle)
    }

    /// Execute with both breakpoints and interrupts
    async fn execute_with_breakpoints_and_interrupts(
        &self,
        graph: CompiledGraph,
        input: StateData,
        breakpoint_callback: BreakpointCallback,
        interrupt_callback: InterruptCallback,
    ) -> Result<ExecutionHandle> {
        // Create a channel for the result
        let (result_tx, result_rx) = oneshot::channel();

        // Register callbacks
        let bp_manager = self.get_breakpoint_manager();
        bp_manager.register_callback(breakpoint_callback).await;

        {
            let int_manager = self.interrupt_manager.read().await;
            int_manager.register_callback(interrupt_callback).await;
        }

        // Create execution handle
        let handle = ExecutionHandle::new(result_rx);

        // Clone necessary data
        let engine = self.clone();
        let bp_manager_clone = bp_manager.clone();

        // Spawn the execution task
        tokio::spawn(async move {
            // Execute with both systems active
            let result = engine.execute_graph_with_breakpoints_internal(
                graph,
                input,
                bp_manager_clone,
            ).await;

            // Send the result back
            let _ = result_tx.send(result);
        });

        Ok(handle)
    }
}

impl ExecutionEngine {
    /// Internal method to execute with breakpoint checks
    async fn execute_graph_with_breakpoints_internal(
        &self,
        graph: CompiledGraph,
        input: StateData,
        bp_manager: Arc<BreakpointManager>,
    ) -> Result<StateData> {
        use crate::state::GraphState;
        use crate::engine::node_executor::{DefaultNodeExecutor, NodeExecutor};
        use crate::engine::graph_traversal::{GraphTraverser, TraversalStrategy};
        use tokio::sync::RwLock;
        use std::collections::HashMap;

        // Initialize execution state
        let mut graph_state = GraphState::new();
        graph_state.update(input.clone());
        let state_arc = Arc::new(RwLock::new(graph_state));

        // Create execution context
        let execution_id = format!("exec-{}", uuid::Uuid::new_v4());
        let context = crate::engine::ExecutionContext {
            graph: Arc::new(graph.clone()),
            state: state_arc.clone(),
            channels: HashMap::new(),
            execution_id: execution_id.clone(),
            metadata: crate::engine::ExecutionMetadata {
                started_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                ended_at: None,
                nodes_executed: 0,
                status: crate::engine::ExecutionStatus::Running,
                error: None,
            },
            resilience_manager: Arc::new(crate::engine::resilience::ResilienceManager::new(
                crate::engine::resilience::CircuitBreakerConfig::default(),
                crate::engine::resilience::RetryConfig::default(),
                10,
            )),
            tracer: Arc::new(crate::engine::tracing::Tracer::new(&execution_id)),
        };

        // Get execution order
        let traverser = GraphTraverser::new(TraversalStrategy::Topological);
        let execution_order = traverser.get_execution_order(&graph)?;

        let node_executor = DefaultNodeExecutor;

        // Execute nodes with breakpoint checks
        for node_id in execution_order {
            // Skip special nodes
            if node_id == "__start__" || node_id == "__end__" {
                continue;
            }

            // Check for breakpoint BEFORE node execution
            let current_state = {
                let state = state_arc.read().await;
                state.snapshot()
            };

            if bp_manager.is_breakpoint(&node_id, &current_state).await {
                let action = bp_manager.handle_breakpoint(&node_id, current_state.clone()).await?;

                match action {
                    BreakpointAction::Continue => {
                        // Continue to execute the node
                    }
                    BreakpointAction::StepOver => {
                        // Skip this node and continue
                        continue;
                    }
                    BreakpointAction::StepInto => {
                        // For YELLOW phase, just continue
                        // GREEN phase would handle subgraph stepping
                    }
                    BreakpointAction::StepOut => {
                        // For YELLOW phase, just continue
                        // GREEN phase would handle scope management
                    }
                    BreakpointAction::Abort(reason) => {
                        return Err(crate::engine::BreakpointError::OperationFailed(reason).into());
                    }
                }
            }

            // Get and execute the node
            let node = graph.graph().get_node(&node_id)
                .ok_or_else(|| crate::engine::ExecutionError::NodeExecutionFailed(
                    format!("Node not found: {}", node_id)
                ))?;

            let mut state_data = {
                let state = state_arc.read().await;
                state.snapshot()
            };

            let result = node_executor.execute(node, &mut state_data, &context).await?;

            // Update state with result
            {
                let mut state = state_arc.write().await;
                state.update(result);
            }
        }

        // Return final state
        let final_state = state_arc.read().await;
        Ok(final_state.snapshot())
    }
}