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

    /// Clean up old snapshots and return count of removed items
    pub async fn cleanup_old_snapshots(&self, max_age: chrono::Duration) -> Result<usize> {
        let cutoff = Utc::now() - max_age;
        let mut removed_count = 0;

        let to_remove: Vec<Uuid> = self.snapshots
            .iter()
            .filter(|entry| entry.value().timestamp < cutoff)
            .map(|entry| *entry.key())
            .collect();

        for id in to_remove {
            if self.snapshots.remove(&id).is_some() {
                removed_count += 1;
            }
        }

        Ok(removed_count)
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

    /// Resume workflow from a snapshot
    pub async fn resume_workflow(
        &self,
        snapshot_id: &Uuid,
        engine: &ExecutionEngine,
        graph: Arc<CompiledGraph>,
    ) -> Result<StateData> {
        let snapshot = self.load_snapshot(snapshot_id).await
            .ok_or_else(|| LangGraphError::Execution("Snapshot not found".to_string()))?;

        // Mark as resumed
        self.mark_resumed(snapshot.execution_id).await;

        // Resume execution from the next node
        let next_node = snapshot.next_node.unwrap_or_else(|| "end".to_string());
        engine.resume_from_node(
            graph,
            snapshot.state.clone(),
            &next_node,
        ).await?;

        Ok(snapshot.state)
    }

    /// Resume workflow with modified state
    pub async fn resume_with_modified_state(
        &self,
        snapshot_id: &Uuid,
        modified_state: StateData,
        engine: &ExecutionEngine,
        graph: Arc<CompiledGraph>,
    ) -> Result<StateData> {
        let mut snapshot = self.load_snapshot(snapshot_id).await
            .ok_or_else(|| LangGraphError::Execution("Snapshot not found".to_string()))?;

        // Update snapshot with modified state
        snapshot.state = modified_state.clone();
        snapshot.timestamp = Utc::now();
        self.snapshots.insert(*snapshot_id, snapshot.clone());

        // Mark as resumed
        self.mark_resumed(snapshot.execution_id).await;

        // Resume execution with modified state
        let next_node = snapshot.next_node.unwrap_or_else(|| "end".to_string());
        engine.resume_from_node(
            graph,
            modified_state.clone(),
            &next_node,
        ).await?;

        Ok(modified_state)
    }

    /// Get resumption history with optional filters
    pub async fn get_resumption_history(
        &self,
        execution_id: Option<Uuid>,
        limit: Option<usize>,
    ) -> Vec<WorkflowSnapshot> {
        let mut snapshots: Vec<WorkflowSnapshot> = if let Some(exec_id) = execution_id {
            self.snapshots
                .iter()
                .filter(|entry| entry.value().execution_id == exec_id)
                .map(|entry| entry.value().clone())
                .collect()
        } else {
            self.snapshots
                .iter()
                .map(|entry| entry.value().clone())
                .collect()
        };

        // Sort by timestamp (newest first)
        snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Apply limit if specified
        if let Some(limit) = limit {
            snapshots.truncate(limit);
        }

        snapshots
    }

    /// Save error state for recovery
    pub async fn save_error_state(
        &self,
        execution_id: Uuid,
        node_id: &str,
        error: &str,
        state: StateData,
    ) -> Result<WorkflowSnapshot> {
        let mut snapshot = WorkflowSnapshot::new(
            execution_id,
            "workflow".to_string(),
            node_id.to_string(),
            state,
        );

        // Add error metadata
        snapshot.metadata = serde_json::json!({
            "error": error,
            "error_node": node_id,
            "error_time": Utc::now().to_rfc3339(),
        });

        self.snapshots.insert(snapshot.id, snapshot.clone());
        Ok(snapshot)
    }

    /// Resume from multiple snapshots (for parallel execution)
    pub async fn resume_multiple(
        &self,
        snapshot_ids: Vec<Uuid>,
        engine: &ExecutionEngine,
        graph: Arc<CompiledGraph>,
    ) -> Result<Vec<StateData>> {
        let mut results = Vec::new();

        for snapshot_id in snapshot_ids {
            let result = self.resume_workflow(&snapshot_id, engine, graph.clone()).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Record a resumption event
    pub async fn record_resumption(&self, snapshot: WorkflowSnapshot) {
        self.snapshots.insert(snapshot.id, snapshot);
    }

    /// Alias for cleanup_old_snapshots for backwards compatibility
    pub async fn cleanup_old_resumptions(&self, max_age: chrono::Duration) -> Result<usize> {
        self.cleanup_old_snapshots(max_age).await
    }

    /// List all resumption points
    pub async fn list_resumption_points(&self) -> Vec<ResumptionPoint> {
        self.resumption_points
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Create a ResumptionManager with a checkpointer
    pub fn with_checkpointer(_checkpointer: Arc<dyn Checkpointer>) -> Self {
        // For YELLOW phase: just create a standard manager
        Self::new()
    }
}

impl Default for ResumptionManager {
    fn default() -> Self {
        Self::new()
    }
}