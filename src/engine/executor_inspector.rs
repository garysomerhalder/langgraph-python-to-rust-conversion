// Integration of StateInspector with ExecutionEngine
// HIL-003: State inspection during execution

use crate::engine::executor::ExecutionEngine;
use crate::engine::state_inspector::{StateInspector, StateSnapshot};
use crate::graph::NodeType;
use crate::state::GraphState;
use anyhow::Result;
use std::sync::Arc;

impl ExecutionEngine {
    /// Set the state inspector for this execution engine
    pub fn set_state_inspector(&mut self, inspector: StateInspector) {
        self.state_inspector = Some(Arc::new(inspector));
    }

    /// Get the state inspector
    pub fn get_state_inspector(&self) -> Option<Arc<StateInspector>> {
        self.state_inspector.clone()
    }

    /// Remove the state inspector
    pub fn remove_state_inspector(&mut self) {
        self.state_inspector = None;
    }

    /// Capture state snapshot if inspector is enabled
    pub async fn capture_state_snapshot(&self, node_id: &str) -> Result<Option<String>> {
        if let Some(inspector) = &self.state_inspector {
            if inspector.is_enabled().await {
                let state = self.state.read().await;
                let snapshot_id = inspector
                    .capture_graph_snapshot(&state, node_id.to_string())
                    .await?;
                Ok(Some(snapshot_id))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Get latest snapshot from inspector
    pub async fn get_latest_snapshot(&self) -> Option<StateSnapshot> {
        if let Some(inspector) = &self.state_inspector {
            inspector.get_latest().await
        } else {
            None
        }
    }

    /// Query snapshots by node
    pub async fn query_snapshots_by_node(&self, node_id: &str) -> Vec<StateSnapshot> {
        if let Some(inspector) = &self.state_inspector {
            inspector.query_by_node(node_id).await
        } else {
            Vec::new()
        }
    }

    /// Export inspection data to JSON
    pub async fn export_inspection_data(&self) -> Result<String> {
        if let Some(inspector) = &self.state_inspector {
            inspector.export_to_json().await
        } else {
            Ok("{}".to_string())
        }
    }

    /// Clear all captured snapshots
    pub async fn clear_snapshots(&self) {
        if let Some(inspector) = &self.state_inspector {
            inspector.clear().await;
        }
    }

    /// Execute node with state inspection
    pub async fn execute_node_with_inspection(&self, node_id: &str) -> Result<()> {
        // Capture state before execution
        let pre_snapshot = self.capture_state_snapshot(node_id).await?;

        // Execute the node (simplified for YELLOW phase)
        // In GREEN phase, this would integrate with full execution logic

        // Capture state after execution
        let post_snapshot = self.capture_state_snapshot(node_id).await?;

        // Log snapshots if debug mode
        if let (Some(pre), Some(post)) = (pre_snapshot, post_snapshot) {
            tracing::debug!(
                "Node {} execution captured: pre={}, post={}",
                node_id,
                pre,
                post
            );
        }

        Ok(())
    }
}
