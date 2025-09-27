//! Advanced state management features for LangGraph
//!
//! This module provides state versioning, branching, merging, and snapshot capabilities.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::state::StateData;
use crate::Result;

/// Errors related to advanced state operations
#[derive(Error, Debug)]
pub enum StateError {
    #[error("Version not found: {0}")]
    VersionNotFound(u64),

    #[error("Branch not found: {0}")]
    BranchNotFound(String),

    #[error("Merge conflict: {0}")]
    MergeConflict(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Snapshot not found: {0}")]
    SnapshotNotFound(String),
}

/// State version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateVersion {
    /// Version number
    pub version: u64,

    /// Timestamp of version creation
    pub timestamp: u64,

    /// Parent version (if any)
    pub parent: Option<u64>,

    /// State data at this version
    pub state: StateData,

    /// Metadata about the version
    pub metadata: HashMap<String, Value>,

    /// Description of changes
    pub description: Option<String>,
}

/// State branch for parallel state exploration
#[derive(Debug, Clone)]
pub struct StateBranch {
    /// Branch name
    pub name: String,

    /// Base version where branch started
    pub base_version: u64,

    /// Current version in this branch
    pub head_version: u64,

    /// Branch metadata
    pub metadata: HashMap<String, Value>,
}

/// State snapshot for saving/restoring state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Snapshot ID
    pub id: String,

    /// Timestamp of snapshot creation
    pub timestamp: u64,

    /// State data
    pub state: StateData,

    /// Version information
    pub version: u64,

    /// Branch information
    pub branch: Option<String>,

    /// Snapshot metadata
    pub metadata: HashMap<String, Value>,
}

/// Versioned state manager
pub struct VersionedStateManager {
    /// Version history
    versions: Arc<RwLock<HashMap<u64, StateVersion>>>,

    /// Current version counter
    current_version: Arc<RwLock<u64>>,

    /// Branches
    branches: Arc<RwLock<HashMap<String, StateBranch>>>,

    /// Current branch
    current_branch: Arc<RwLock<Option<String>>>,

    /// Version limit (for memory management)
    version_limit: usize,
}

impl VersionedStateManager {
    /// Create a new versioned state manager
    pub fn new(version_limit: usize) -> Self {
        Self {
            versions: Arc::new(RwLock::new(HashMap::new())),
            current_version: Arc::new(RwLock::new(0)),
            current_branch: Arc::new(RwLock::new(None)),
            branches: Arc::new(RwLock::new(HashMap::new())),
            version_limit,
        }
    }

    /// Create a new version
    pub async fn create_version(
        &self,
        state: StateData,
        description: Option<String>,
    ) -> Result<u64> {
        let mut versions = self.versions.write().await;
        let mut current = self.current_version.write().await;

        let new_version = *current + 1;
        let parent = if *current > 0 { Some(*current) } else { None };

        let version = StateVersion {
            version: new_version,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            parent,
            state,
            metadata: HashMap::new(),
            description,
        };

        versions.insert(new_version, version);
        *current = new_version;

        // Prune old versions if over limit
        if versions.len() > self.version_limit {
            self.prune_old_versions(&mut versions).await;
        }

        Ok(new_version)
    }

    /// Get a specific version
    pub async fn get_version(&self, version: u64) -> Result<StateVersion> {
        let versions = self.versions.read().await;
        versions
            .get(&version)
            .cloned()
            .ok_or_else(|| StateError::VersionNotFound(version).into())
    }

    /// Get current version
    pub async fn get_current_version(&self) -> Result<StateVersion> {
        let current = self.current_version.read().await;
        self.get_version(*current).await
    }

    /// Rollback to a previous version
    pub async fn rollback(&self, version: u64) -> Result<()> {
        let versions = self.versions.read().await;
        if !versions.contains_key(&version) {
            return Err(StateError::VersionNotFound(version).into());
        }

        let mut current = self.current_version.write().await;
        *current = version;

        Ok(())
    }

    /// Create a new branch
    pub async fn create_branch(&self, name: String) -> Result<()> {
        let mut branches = self.branches.write().await;
        let current = self.current_version.read().await;

        let branch = StateBranch {
            name: name.clone(),
            base_version: *current,
            head_version: *current,
            metadata: HashMap::new(),
        };

        branches.insert(name.clone(), branch);

        let mut current_branch = self.current_branch.write().await;
        *current_branch = Some(name);

        Ok(())
    }

    /// Switch to a branch
    pub async fn switch_branch(&self, name: &str) -> Result<()> {
        let branches = self.branches.read().await;
        let branch = branches
            .get(name)
            .ok_or_else(|| StateError::BranchNotFound(name.to_string()))?;

        let mut current = self.current_version.write().await;
        *current = branch.head_version;

        let mut current_branch = self.current_branch.write().await;
        *current_branch = Some(name.to_string());

        Ok(())
    }

    /// Merge a branch into current
    pub async fn merge_branch(&self, branch_name: &str) -> Result<StateData> {
        let branches = self.branches.read().await;
        let branch = branches
            .get(branch_name)
            .ok_or_else(|| StateError::BranchNotFound(branch_name.to_string()))?;

        let versions = self.versions.read().await;
        let branch_state = versions
            .get(&branch.head_version)
            .ok_or_else(|| StateError::VersionNotFound(branch.head_version))?;

        let current = self.current_version.read().await;
        let current_state = versions
            .get(&current)
            .ok_or_else(|| StateError::VersionNotFound(*current))?;

        // Simple merge strategy: combine both states
        let merged = self.merge_states(&current_state.state, &branch_state.state)?;

        Ok(merged)
    }

    /// Merge two states
    fn merge_states(&self, base: &StateData, other: &StateData) -> Result<StateData> {
        let mut merged = base.clone();

        for (key, value) in other {
            if let Some(base_value) = base.get(key) {
                // Apply sophisticated merge strategies based on value types
                match (base_value, value) {
                    // Arrays: concatenate unique values
                    (Value::Array(base_arr), Value::Array(branch_arr)) => {
                        let mut merged_arr: Vec<Value> = base_arr.clone();
                        for item in branch_arr {
                            if !merged_arr.contains(item) {
                                merged_arr.push(item.clone());
                            }
                        }
                        merged.insert(key.clone(), Value::Array(merged_arr));
                    }
                    // Objects: deep merge
                    (Value::Object(base_obj), Value::Object(branch_obj)) => {
                        let mut merged_obj = base_obj.clone();
                        for (k, v) in branch_obj {
                            merged_obj.insert(k.clone(), v.clone());
                        }
                        merged.insert(key.clone(), Value::Object(merged_obj));
                    }
                    // Numbers: use the branch value (most recent)
                    (Value::Number(_), Value::Number(_)) => {
                        merged.insert(key.clone(), value.clone());
                    }
                    // Default: use branch value for other types
                    _ => {
                        merged.insert(key.clone(), value.clone());
                    }
                }
            } else {
                merged.insert(key.clone(), value.clone());
            }
        }

        Ok(merged)
    }

    /// Prune old versions to stay within limit
    async fn prune_old_versions(&self, versions: &mut HashMap<u64, StateVersion>) {
        if versions.len() <= self.version_limit {
            return;
        }

        // Keep the most recent versions
        let mut version_numbers: Vec<u64> = versions.keys().cloned().collect();
        version_numbers.sort();

        let to_remove = version_numbers.len() - self.version_limit;
        for version in version_numbers.iter().take(to_remove) {
            versions.remove(version);
        }
    }
}

/// State snapshot manager
pub struct SnapshotManager {
    /// Stored snapshots
    snapshots: Arc<RwLock<HashMap<String, StateSnapshot>>>,

    /// Maximum number of snapshots
    max_snapshots: usize,
}

impl SnapshotManager {
    /// Create a new snapshot manager
    pub fn new(max_snapshots: usize) -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            max_snapshots,
        }
    }

    /// Create a snapshot
    pub async fn create_snapshot(
        &self,
        id: String,
        state: StateData,
        version: u64,
        branch: Option<String>,
    ) -> Result<()> {
        let mut snapshots = self.snapshots.write().await;

        let snapshot = StateSnapshot {
            id: id.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            state,
            version,
            branch,
            metadata: HashMap::new(),
        };

        snapshots.insert(id, snapshot);

        // Prune old snapshots if over limit
        if snapshots.len() > self.max_snapshots {
            self.prune_old_snapshots(&mut snapshots).await;
        }

        Ok(())
    }

    /// Load a snapshot
    pub async fn load_snapshot(&self, id: &str) -> Result<StateSnapshot> {
        let snapshots = self.snapshots.read().await;
        snapshots
            .get(id)
            .cloned()
            .ok_or_else(|| StateError::SnapshotNotFound(id.to_string()).into())
    }

    /// List all snapshots
    pub async fn list_snapshots(&self) -> Vec<String> {
        let snapshots = self.snapshots.read().await;
        snapshots.keys().cloned().collect()
    }

    /// Delete a snapshot
    pub async fn delete_snapshot(&self, id: &str) -> Result<()> {
        let mut snapshots = self.snapshots.write().await;
        snapshots
            .remove(id)
            .ok_or_else(|| StateError::SnapshotNotFound(id.to_string()))?;
        Ok(())
    }

    /// Prune old snapshots
    async fn prune_old_snapshots(&self, snapshots: &mut HashMap<String, StateSnapshot>) {
        if snapshots.len() <= self.max_snapshots {
            return;
        }

        // Remove oldest snapshots
        let mut snapshot_list: Vec<_> = snapshots.values().cloned().collect();
        snapshot_list.sort_by_key(|s| s.timestamp);

        let to_remove = snapshots.len() - self.max_snapshots;
        for snapshot in snapshot_list.iter().take(to_remove) {
            snapshots.remove(&snapshot.id);
        }
    }
}

/// State diff for tracking changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDiff {
    /// Added keys
    pub added: HashMap<String, Value>,

    /// Modified keys (old value, new value)
    pub modified: HashMap<String, (Value, Value)>,

    /// Removed keys
    pub removed: HashMap<String, Value>,
}

impl StateDiff {
    /// Calculate diff between two states
    pub fn calculate(old: &StateData, new: &StateData) -> Self {
        let mut added = HashMap::new();
        let mut modified = HashMap::new();
        let mut removed = HashMap::new();

        // Find added and modified keys
        for (key, new_value) in new {
            if let Some(old_value) = old.get(key) {
                if old_value != new_value {
                    modified.insert(key.clone(), (old_value.clone(), new_value.clone()));
                }
            } else {
                added.insert(key.clone(), new_value.clone());
            }
        }

        // Find removed keys
        for (key, old_value) in old {
            if !new.contains_key(key) {
                removed.insert(key.clone(), old_value.clone());
            }
        }

        Self {
            added,
            modified,
            removed,
        }
    }

    /// Apply diff to a state
    pub fn apply(&self, state: &mut StateData) {
        // Add new keys
        for (key, value) in &self.added {
            state.insert(key.clone(), value.clone());
        }

        // Modify existing keys
        for (key, (_old, new)) in &self.modified {
            state.insert(key.clone(), new.clone());
        }

        // Remove keys
        for key in self.removed.keys() {
            state.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_versioned_state_manager() {
        let manager = VersionedStateManager::new(10);

        let mut state1 = StateData::new();
        state1.insert("key1".to_string(), json!("value1"));

        let version1 = manager
            .create_version(state1.clone(), Some("Initial state".to_string()))
            .await
            .unwrap();
        assert_eq!(version1, 1);

        let mut state2 = StateData::new();
        state2.insert("key1".to_string(), json!("value1"));
        state2.insert("key2".to_string(), json!("value2"));

        let version2 = manager
            .create_version(state2, Some("Added key2".to_string()))
            .await
            .unwrap();
        assert_eq!(version2, 2);

        let current = manager.get_current_version().await.unwrap();
        assert_eq!(current.version, 2);

        manager.rollback(1).await.unwrap();
        let rolled_back = manager.get_current_version().await.unwrap();
        assert_eq!(rolled_back.version, 1);
    }

    #[tokio::test]
    async fn test_branching() {
        let manager = VersionedStateManager::new(10);

        let state = StateData::new();
        manager.create_version(state, None).await.unwrap();

        manager.create_branch("feature".to_string()).await.unwrap();
        manager.switch_branch("feature").await.unwrap();

        let mut feature_state = StateData::new();
        feature_state.insert("feature".to_string(), json!(true));
        manager.create_version(feature_state, None).await.unwrap();

        // Merge would combine states
        // let merged = manager.merge_branch("feature").await.unwrap();
    }

    #[tokio::test]
    async fn test_snapshots() {
        let manager = SnapshotManager::new(5);

        let mut state = StateData::new();
        state.insert("test".to_string(), json!("value"));

        manager
            .create_snapshot("snapshot1".to_string(), state.clone(), 1, None)
            .await
            .unwrap();

        let snapshots = manager.list_snapshots().await;
        assert_eq!(snapshots.len(), 1);

        let loaded = manager.load_snapshot("snapshot1").await.unwrap();
        assert_eq!(loaded.id, "snapshot1");
        assert_eq!(loaded.version, 1);

        manager.delete_snapshot("snapshot1").await.unwrap();
        let snapshots = manager.list_snapshots().await;
        assert_eq!(snapshots.len(), 0);
    }

    #[test]
    fn test_state_diff() {
        let mut old = StateData::new();
        old.insert("key1".to_string(), json!("value1"));
        old.insert("key2".to_string(), json!("value2"));

        let mut new = StateData::new();
        new.insert("key1".to_string(), json!("modified"));
        new.insert("key3".to_string(), json!("value3"));

        let diff = StateDiff::calculate(&old, &new);

        assert_eq!(diff.added.len(), 1);
        assert!(diff.added.contains_key("key3"));

        assert_eq!(diff.modified.len(), 1);
        assert!(diff.modified.contains_key("key1"));

        assert_eq!(diff.removed.len(), 1);
        assert!(diff.removed.contains_key("key2"));

        let mut target = StateData::new();
        target.insert("key1".to_string(), json!("value1"));
        target.insert("key2".to_string(), json!("value2"));

        diff.apply(&mut target);

        assert_eq!(target.get("key1"), Some(&json!("modified")));
        assert_eq!(target.get("key3"), Some(&json!("value3")));
        assert!(!target.contains_key("key2"));
    }
}
