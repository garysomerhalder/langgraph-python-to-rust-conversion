// State inspection system for debugging and monitoring execution
// HIL-003: State inspection during execution

use crate::engine::state_diff::{ExportFormat, StateDiff, StateFilter};
use crate::state::{GraphState, StateData};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Snapshot of state at a specific point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub node_id: String,
    pub state: StateData,
    pub metadata: HashMap<String, String>,
}

impl StateSnapshot {
    /// Create a new snapshot from current state
    pub fn from_state(state: &GraphState, node_id: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            node_id,
            state: state.values.clone(),
            metadata: HashMap::new(),
        }
    }

    /// Convert snapshot to JSON
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Get a specific field from the snapshot
    pub fn get_field(&self, key: &str) -> Option<&Value> {
        self.state.get(key)
    }
}

/// State inspector for debugging and monitoring
#[derive(Clone)]
pub struct StateInspector {
    snapshots: Arc<RwLock<Vec<StateSnapshot>>>,
    enabled: Arc<RwLock<bool>>,
    max_snapshots: usize,
}

impl StateInspector {
    /// Create a new state inspector
    pub fn new() -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(Vec::new())),
            enabled: Arc::new(RwLock::new(true)),
            max_snapshots: 1000,
        }
    }

    /// Create with custom max snapshots
    pub fn with_max_snapshots(max_snapshots: usize) -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(Vec::new())),
            enabled: Arc::new(RwLock::new(true)),
            max_snapshots,
        }
    }

    /// Enable or disable inspection
    pub async fn set_enabled(&self, enabled: bool) {
        *self.enabled.write().await = enabled;
    }

    /// Check if inspection is enabled
    pub async fn is_enabled(&self) -> bool {
        *self.enabled.read().await
    }

    /// Capture a snapshot of the current state (from StateData)
    pub async fn capture_snapshot(&self, node_id: &str, state: &StateData) -> String {
        let snapshot = StateSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            node_id: node_id.to_string(),
            state: state.clone(),
            metadata: HashMap::new(),
        };
        let snapshot_id = snapshot.id.clone();

        let mut snapshots = self.snapshots.write().await;

        // Maintain max snapshots limit
        if snapshots.len() >= self.max_snapshots {
            snapshots.remove(0);
        }

        snapshots.push(snapshot);
        snapshot_id
    }

    /// Capture a snapshot from GraphState
    pub async fn capture_graph_snapshot(
        &self,
        state: &GraphState,
        node_id: String,
    ) -> Result<String> {
        if !self.is_enabled().await {
            return Ok(String::new());
        }

        let snapshot = StateSnapshot::from_state(state, node_id);
        let snapshot_id = snapshot.id.clone();

        let mut snapshots = self.snapshots.write().await;

        // Maintain max snapshots limit
        if snapshots.len() >= self.max_snapshots {
            snapshots.remove(0);
        }

        snapshots.push(snapshot);
        Ok(snapshot_id)
    }

    /// Query snapshots by node ID
    pub async fn query_by_node(&self, node_id: &str) -> Vec<StateSnapshot> {
        let snapshots = self.snapshots.read().await;
        snapshots
            .iter()
            .filter(|s| s.node_id == node_id)
            .cloned()
            .collect()
    }

    /// Query snapshots by time range
    pub async fn query_by_time(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<StateSnapshot> {
        let snapshots = self.snapshots.read().await;
        snapshots
            .iter()
            .filter(|s| s.timestamp >= start && s.timestamp <= end)
            .cloned()
            .collect()
    }

    /// Get snapshot by ID (returns the snapshot directly)
    pub async fn get_snapshot(&self, snapshot_id: &str) -> Option<StateSnapshot> {
        let snapshots = self.snapshots.read().await;
        snapshots.iter().find(|s| s.id == snapshot_id).cloned()
    }

    /// Query state value by path (supports nested queries like "user.name")
    pub async fn query_state(&self, snapshot_id: &str, path: &str) -> Option<Value> {
        let snapshot = self.get_snapshot(snapshot_id).await?;

        // Split path and navigate nested structure
        let parts: Vec<&str> = path.split('.').collect();

        // Start with looking at the whole state
        let mut current_value: Option<&Value> = None;

        for (i, part) in parts.iter().enumerate() {
            if i == 0 {
                // First part - get from state
                current_value = snapshot.state.get(*part);
            } else {
                // Nested parts - navigate into object
                if let Some(Value::Object(obj)) = current_value {
                    current_value = obj.get(*part);
                } else {
                    return None;
                }
            }
        }

        current_value.cloned()
    }

    /// Get latest snapshot
    pub async fn get_latest(&self) -> Option<StateSnapshot> {
        let snapshots = self.snapshots.read().await;
        snapshots.last().cloned()
    }

    /// Get all snapshots
    pub async fn get_all_snapshots(&self) -> Vec<StateSnapshot> {
        self.snapshots.read().await.clone()
    }

    /// Compute diff between two snapshots
    pub async fn diff_snapshots(
        &self,
        id1: &str,
        id2: &str,
    ) -> Result<HashMap<String, (Option<Value>, Option<Value>)>> {
        let snapshot1 = self
            .get_snapshot(id1)
            .await
            .ok_or_else(|| anyhow::anyhow!("Snapshot {} not found", id1))?;
        let snapshot2 = self
            .get_snapshot(id2)
            .await
            .ok_or_else(|| anyhow::anyhow!("Snapshot {} not found", id2))?;

        let mut diff = HashMap::new();

        // Find all keys in both snapshots
        let mut all_keys = std::collections::HashSet::new();
        all_keys.extend(snapshot1.state.keys().cloned());
        all_keys.extend(snapshot2.state.keys().cloned());

        // Compare values for each key
        for key in all_keys {
            let val1 = snapshot1.state.get(&key).cloned();
            let val2 = snapshot2.state.get(&key).cloned();

            if val1 != val2 {
                diff.insert(key, (val1, val2));
            }
        }

        Ok(diff)
    }

    /// Export snapshots to JSON
    pub async fn export_to_json(&self) -> Result<String> {
        let snapshots = self.snapshots.read().await;
        Ok(serde_json::to_string_pretty(&*snapshots)?)
    }

    /// Search for snapshots containing specific state values
    pub async fn search(&self, field: &str, value: &Value) -> Vec<StateSnapshot> {
        let snapshots = self.snapshots.read().await;
        snapshots
            .iter()
            .filter(|s| s.state.get(field).map_or(false, |v| v == value))
            .cloned()
            .collect()
    }

    /// Clear all snapshots
    pub async fn clear(&self) {
        self.snapshots.write().await.clear();
    }

    /// Get history of changes for a specific field
    pub async fn get_field_history(
        &self,
        field: &str,
    ) -> Vec<(DateTime<Utc>, String, Option<Value>)> {
        let snapshots = self.snapshots.read().await;
        snapshots
            .iter()
            .map(|s| (s.timestamp, s.node_id.clone(), s.state.get(field).cloned()))
            .collect()
    }

    /// Diff two states by snapshot IDs
    pub async fn diff_states(&self, id1: &str, id2: &str) -> StateDiff {
        let snapshot1 = self.get_snapshot(id1).await;
        let snapshot2 = self.get_snapshot(id2).await;

        let mut diff = StateDiff::new();

        if let (Some(s1), Some(s2)) = (snapshot1, snapshot2) {
            // Check for added fields
            for (key, value) in &s2.state {
                if !s1.state.contains_key(key) {
                    diff.added.insert(key.clone(), value.clone());
                } else if s1.state.get(key) != Some(value) {
                    // Modified fields
                    diff.modified.insert(
                        key.clone(),
                        (s1.state.get(key).cloned().unwrap(), value.clone()),
                    );
                }
            }

            // Check for removed fields
            for (key, value) in &s1.state {
                if !s2.state.contains_key(key) {
                    diff.removed.insert(key.clone(), value.clone());
                }
            }
        }

        diff
    }

    /// Export a snapshot in the specified format
    pub async fn export_snapshot(&self, snapshot_id: &str, format: ExportFormat) -> String {
        if let Some(snapshot) = self.get_snapshot(snapshot_id).await {
            match format {
                ExportFormat::Json => {
                    serde_json::to_string_pretty(&snapshot.state).unwrap_or_default()
                }
                ExportFormat::Yaml => {
                    // Simplified for YELLOW phase - would use serde_yaml in GREEN
                    let mut yaml = String::new();
                    for (k, v) in &snapshot.state {
                        match v {
                            Value::String(s) => yaml.push_str(&format!("{}: {}\n", k, s)),
                            Value::Number(n) => yaml.push_str(&format!("{}: {}\n", k, n)),
                            Value::Bool(b) => yaml.push_str(&format!("{}: {}\n", k, b)),
                            _ => yaml.push_str(&format!("{}: {}\n", k, v)),
                        }
                    }
                    yaml
                }
                ExportFormat::Csv => {
                    // Simplified for YELLOW phase
                    String::new()
                }
            }
        } else {
            String::new()
        }
    }

    /// Search for fields matching a pattern in a snapshot
    pub async fn search_state(&self, snapshot_id: &str, pattern: &str) -> Vec<(String, Value)> {
        if let Some(snapshot) = self.get_snapshot(snapshot_id).await {
            let pattern_lower = pattern.to_lowercase();
            snapshot
                .state
                .iter()
                .filter(|(k, v)| {
                    // Search in keys (case-insensitive)
                    k.to_lowercase().contains(&pattern_lower) ||
                    // Search in string values (case-insensitive)
                    if let Value::String(s) = v {
                        s.to_lowercase().contains(&pattern_lower)
                    } else {
                        false
                    }
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get history of snapshots with optional filters
    pub async fn get_history(
        &self,
        node_filter: Option<&str>,
        limit: Option<usize>,
    ) -> Vec<StateSnapshot> {
        let snapshots = self.snapshots.read().await;
        let mut result: Vec<StateSnapshot> = if let Some(node) = node_filter {
            snapshots
                .iter()
                .filter(|s| s.node_id == node)
                .cloned()
                .collect()
        } else {
            snapshots.clone()
        };

        if let Some(limit) = limit {
            result.truncate(limit);
        }

        result
    }

    /// Watch for state changes (returns a session ID for tracking)
    pub async fn watch_for_changes(&self, _node_pattern: Option<&str>) -> String {
        // Simplified for GREEN phase - would implement actual watching
        uuid::Uuid::new_v4().to_string()
    }

    /// Get changes from a watch session
    pub async fn get_watch_changes(&self, _session_id: &str) -> Vec<StateSnapshot> {
        // Simplified for GREEN phase - would track actual changes
        Vec::new()
    }

    /// Enable inspection for specific nodes
    pub async fn enable_for_nodes(&self, _nodes: Vec<String>) {
        // Would implement node-specific enabling
        self.set_enabled(true).await;
    }

    /// Disable inspection for specific nodes
    pub async fn disable_for_nodes(&self, _nodes: Vec<String>) {
        // Would implement node-specific disabling
        self.set_enabled(false).await;
    }

    /// Capture a snapshot with a filter
    pub async fn capture_snapshot_with_filter(
        &self,
        node_id: &str,
        state: &StateData,
        filter: &StateFilter,
    ) -> String {
        let filtered_state = filter.apply(state);

        let snapshot = StateSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            node_id: node_id.to_string(),
            state: filtered_state,
            metadata: HashMap::new(),
        };
        let snapshot_id = snapshot.id.clone();

        let mut snapshots = self.snapshots.write().await;

        // Maintain max snapshots limit
        if snapshots.len() >= self.max_snapshots {
            snapshots.remove(0);
        }

        snapshots.push(snapshot);
        snapshot_id
    }
}

impl Default for StateInspector {
    fn default() -> Self {
        Self::new()
    }
}
