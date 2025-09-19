//! Workflow Resumption System for suspended execution
//! YELLOW Phase: Minimal implementation to make tests pass

use crate::engine::{ExecutionEngine, ExecutionStatus};
use crate::graph::CompiledGraph;
use crate::state::StateData;
use crate::checkpoint::Checkpointer;
use crate::{Result, LangGraphError};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Snapshot of workflow execution state for resumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSnapshot {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub graph_name: String,
    pub last_completed_node: String,
    pub next_node: Option<String>,
    pub state: StateData,
    pub execution_path: Vec<String>,
    pub timestamp: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl WorkflowSnapshot {
    /// Create a new workflow snapshot
    pub fn new(
        execution_id: Uuid,
        graph_name: String,
        last_node: String,
        state: StateData,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            execution_id,
            graph_name,
            last_completed_node: last_node,
            next_node: None,
            state,
            execution_path: Vec::new(),
            timestamp: Utc::now(),
            metadata: serde_json::json!({}),
        }
    }

    /// Update the snapshot with execution progress
    pub fn update_progress(&mut self, node: String) {
        self.execution_path.push(self.last_completed_node.clone());
        self.last_completed_node = node;
        self.timestamp = Utc::now();
    }
}

/// Point where workflow can be resumed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumptionPoint {
    pub node_id: String,
    pub state_snapshot: StateData,
    pub can_modify_state: bool,
    pub created_at: DateTime<Utc>,
}

/// Manager for workflow suspension and resumption
#[derive(Debug, Clone)]
pub struct ResumptionManager {
    snapshots: Arc<DashMap<Uuid, WorkflowSnapshot>>,
    resumption_points: Arc<DashMap<String, ResumptionPoint>>,
    active_executions: Arc<DashMap<Uuid, ExecutionStatus>>,
}

impl ResumptionManager {
    /// Create a new resumption manager
    pub fn new() -> Self {
        Self {
            snapshots: Arc::new(DashMap::new()),
            resumption_points: Arc::new(DashMap::new()),
            active_executions: Arc::new(DashMap::new()),
        }
    }

    /// Save a resumption point during execution
    pub async fn save_resumption_point(
        &self,
        execution_id: &Uuid,
        node_id: &str,
        engine: &ExecutionEngine,
    ) -> Result<WorkflowSnapshot> {
        // Create snapshot of current state
        let state = engine.get_current_state().await?;

        let snapshot = WorkflowSnapshot::new(
            *execution_id,
            "workflow".to_string(),
            node_id.to_string(),
            state.clone(),
        );

        // Store snapshot
        self.snapshots.insert(snapshot.id, snapshot.clone());

        // Create resumption point
        let resumption_point = ResumptionPoint {
            node_id: node_id.to_string(),
            state_snapshot: state,
            can_modify_state: true,
            created_at: Utc::now(),
        };

        self.resumption_points.insert(node_id.to_string(), resumption_point);

        Ok(snapshot)
    }

    /// Load a saved workflow snapshot
    pub async fn load_snapshot(&self, snapshot_id: &Uuid) -> Option<WorkflowSnapshot> {
        self.snapshots.get(snapshot_id).map(|entry| entry.clone())
    }

    /// List all available snapshots
    pub async fn list_snapshots(&self) -> Vec<WorkflowSnapshot> {
        self.snapshots
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Delete a snapshot
    pub async fn delete_snapshot(&self, snapshot_id: &Uuid) -> bool {
        self.snapshots.remove(snapshot_id).is_some()
    }

    /// Get resumption point for a node
    pub async fn get_resumption_point(&self, node_id: &str) -> Option<ResumptionPoint> {
        self.resumption_points.get(node_id).map(|entry| entry.clone())
    }

    /// Mark execution as suspended
    pub async fn suspend_execution(&self, execution_id: Uuid) {
        self.active_executions.insert(execution_id, ExecutionStatus::Suspended);
    }

    /// Mark execution as resumed
    pub async fn mark_resumed(&self, execution_id: Uuid) {
        self.active_executions.insert(execution_id, ExecutionStatus::Running);
    }

    /// Check if execution is suspended
    pub async fn is_suspended(&self, execution_id: &Uuid) -> bool {
        self.active_executions
            .get(execution_id)
            .map(|entry| *entry.value() == ExecutionStatus::Suspended)
            .unwrap_or(false)
    }

    /// Export all snapshots for persistence
    pub async fn export_snapshots(&self) -> Vec<WorkflowSnapshot> {
        self.snapshots
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Import snapshots from persistence
    pub async fn import_snapshots(&self, snapshots: Vec<WorkflowSnapshot>) -> Result<()> {
        for snapshot in snapshots {
            self.snapshots.insert(snapshot.id, snapshot);
        }
        Ok(())
    }

    /// Clean up old snapshots
    pub async fn cleanup_old_snapshots(&self, max_age_hours: i64) {
        let cutoff = Utc::now() - chrono::Duration::hours(max_age_hours);

        self.snapshots.retain(|_, snapshot| {
            snapshot.timestamp > cutoff
        });
    }

    /// Create snapshot from checkpointer
    pub async fn create_from_checkpoint(
        &self,
        checkpointer: &dyn Checkpointer,
        checkpoint_id: &str,
        execution_id: Uuid,
    ) -> Result<WorkflowSnapshot> {
        let checkpoint = checkpointer
            .load(checkpoint_id)
            .await
            .map_err(|e| LangGraphError::Execution(format!("Failed to load checkpoint: {}", e)))?;

        // Convert GraphState to StateData
        let state = StateData::new(); // TODO: Convert from GraphState

        let snapshot = WorkflowSnapshot {
            id: Uuid::new_v4(),
            execution_id,
            graph_name: checkpoint_id.to_string(),
            last_completed_node: checkpoint.thread_id, // Use thread_id as a placeholder
            next_node: None,
            state,
            execution_path: Vec::new(),
            timestamp: Utc::now(),
            metadata: checkpoint.metadata.unwrap_or_else(|| serde_json::json!({})),
        };

        self.snapshots.insert(snapshot.id, snapshot.clone());
        Ok(snapshot)
    }
}

impl Default for ResumptionManager {
    fn default() -> Self {
        Self::new()
    }
}

// Use ExecutionStatus from the executor module