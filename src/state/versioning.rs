//! State versioning and rollback system for LangGraph
//!
//! Production-ready state versioning with efficient diff storage,
//! snapshot management, and rollback capabilities.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{RwLock, Mutex};
use tracing::{debug, error, info, instrument, warn};

use crate::state::StateData;
use crate::Result;

/// State versioning system with efficient storage
pub struct StateVersioningSystem {
    /// Version storage backend
    storage: Arc<dyn VersionStorage>,
    
    /// Current version pointer
    current_version: Arc<RwLock<VersionId>>,
    
    /// Version cache for fast access
    cache: Arc<RwLock<VersionCache>>,
    
    /// Configuration
    config: VersioningConfig,
    
    /// Metrics
    metrics: Arc<RwLock<VersioningMetrics>>,
}

/// Version identifier
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct VersionId {
    pub id: u64,
    pub timestamp: u64,
    pub hash: String,
}

impl VersionId {
    pub fn new(id: u64) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let hash = format!("{:x}-{:x}", id, timestamp);
        
        Self {
            id,
            timestamp,
            hash,
        }
    }
}

/// Version record with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub id: VersionId,
    pub parent_id: Option<VersionId>,
    pub state: StateSnapshot,
    pub metadata: VersionMetadata,
}

/// State snapshot representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateSnapshot {
    /// Full state snapshot
    Full(StateData),
    
    /// Delta from parent version
    Delta(StateDelta),
    
    /// Compressed snapshot
    Compressed(Vec<u8>),
}

/// State delta for efficient storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDelta {
    /// Added or modified keys
    pub changes: HashMap<String, Value>,
    
    /// Removed keys
    pub removals: Vec<String>,
    
    /// Parent version reference
    pub base_version: VersionId,
}

impl StateDelta {
    /// Apply delta to a base state
    pub fn apply(&self, base: &StateData) -> StateData {
        let mut result = base.clone();
        
        // Apply changes
        for (key, value) in &self.changes {
            result.insert(key.clone(), value.clone());
        }
        
        // Apply removals
        for key in &self.removals {
            result.remove(key);
        }
        
        result
    }
    
    /// Compute delta between two states
    pub fn compute(base: &StateData, target: &StateData) -> Self {
        let mut changes = HashMap::new();
        let mut removals = Vec::new();
        
        // Find changes and additions
        for (key, value) in target {
            match base.get(key) {
                Some(base_value) if base_value != value => {
                    changes.insert(key.clone(), value.clone());
                }
                None => {
                    changes.insert(key.clone(), value.clone());
                }
                _ => {} // No change
            }
        }
        
        // Find removals
        for key in base.keys() {
            if !target.contains_key(key) {
                removals.push(key.clone());
            }
        }
        
        Self {
            changes,
            removals,
            base_version: VersionId::new(0), // Will be set properly
        }
    }
}

/// Version metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMetadata {
    pub author: String,
    pub message: String,
    pub tags: Vec<String>,
    pub branch: String,
    pub is_checkpoint: bool,
}

impl Default for VersionMetadata {
    fn default() -> Self {
        Self {
            author: "system".to_string(),
            message: String::new(),
            tags: Vec::new(),
            branch: "main".to_string(),
            is_checkpoint: false,
        }
    }
}

/// Versioning configuration
#[derive(Debug, Clone)]
pub struct VersioningConfig {
    /// Maximum versions to keep
    pub max_versions: usize,
    
    /// Maximum cache size
    pub max_cache_size: usize,
    
    /// Enable compression for snapshots
    pub enable_compression: bool,
    
    /// Delta threshold (use delta if changes < threshold)
    pub delta_threshold: f64,
    
    /// Auto-checkpoint interval
    pub checkpoint_interval: usize,
}

impl Default for VersioningConfig {
    fn default() -> Self {
        Self {
            max_versions: 100,
            max_cache_size: 10,
            enable_compression: true,
            delta_threshold: 0.3,
            checkpoint_interval: 10,
        }
    }
}

/// Version cache for fast access
struct VersionCache {
    cache: VecDeque<(VersionId, StateData)>,
    max_size: usize,
}

impl VersionCache {
    fn new(max_size: usize) -> Self {
        Self {
            cache: VecDeque::new(),
            max_size,
        }
    }
    
    fn get(&self, id: &VersionId) -> Option<StateData> {
        self.cache.iter()
            .find(|(vid, _)| vid == id)
            .map(|(_, state)| state.clone())
    }
    
    fn put(&mut self, id: VersionId, state: StateData) {
        // Check if already in cache
        if self.cache.iter().any(|(vid, _)| vid == &id) {
            return;
        }
        
        // Evict if at capacity
        if self.cache.len() >= self.max_size {
            self.cache.pop_front();
        }
        
        self.cache.push_back((id, state));
    }
}

/// Version storage backend trait
#[async_trait]
pub trait VersionStorage: Send + Sync {
    /// Store a version
    async fn store(&self, version: Version) -> Result<()>;
    
    /// Retrieve a version
    async fn get(&self, id: &VersionId) -> Result<Option<Version>>;
    
    /// List versions in range
    async fn list(&self, start: u64, end: u64) -> Result<Vec<VersionId>>;
    
    /// Delete a version
    async fn delete(&self, id: &VersionId) -> Result<()>;
    
    /// Get storage size
    async fn size(&self) -> Result<usize>;
}

/// In-memory version storage
pub struct InMemoryStorage {
    versions: Arc<RwLock<HashMap<VersionId, Version>>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            versions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl VersionStorage for InMemoryStorage {
    async fn store(&self, version: Version) -> Result<()> {
        let mut versions = self.versions.write().await;
        versions.insert(version.id.clone(), version);
        Ok(())
    }
    
    async fn get(&self, id: &VersionId) -> Result<Option<Version>> {
        let versions = self.versions.read().await;
        Ok(versions.get(id).cloned())
    }
    
    async fn list(&self, start: u64, end: u64) -> Result<Vec<VersionId>> {
        let versions = self.versions.read().await;
        let mut ids: Vec<_> = versions.keys()
            .filter(|id| id.id >= start && id.id <= end)
            .cloned()
            .collect();
        ids.sort_by_key(|id| id.id);
        Ok(ids)
    }
    
    async fn delete(&self, id: &VersionId) -> Result<()> {
        let mut versions = self.versions.write().await;
        versions.remove(id);
        Ok(())
    }
    
    async fn size(&self) -> Result<usize> {
        let versions = self.versions.read().await;
        Ok(versions.len())
    }
}

/// Versioning metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersioningMetrics {
    pub total_versions: usize,
    pub total_snapshots: usize,
    pub total_deltas: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub average_delta_size: usize,
    pub storage_bytes: usize,
}

impl StateVersioningSystem {
    /// Create new versioning system
    pub fn new(storage: Arc<dyn VersionStorage>, config: VersioningConfig) -> Self {
        let cache = VersionCache::new(config.max_cache_size);
        
        Self {
            storage,
            current_version: Arc::new(RwLock::new(VersionId::new(0))),
            cache: Arc::new(RwLock::new(cache)),
            config,
            metrics: Arc::new(RwLock::new(VersioningMetrics::default())),
        }
    }
    
    /// Create a new version
    #[instrument(skip(self, state))]
    pub async fn create_version(
        &self,
        state: &StateData,
        metadata: VersionMetadata,
    ) -> Result<VersionId> {
        let mut current = self.current_version.write().await;
        let parent_id = Some(current.clone());
        let new_id = VersionId::new(current.id + 1);
        
        // Determine snapshot type
        let snapshot = if current.id % self.config.checkpoint_interval as u64 == 0 {
            // Create checkpoint (full snapshot)
            StateSnapshot::Full(state.clone())
        } else if let Some(parent_version) = self.storage.get(&current).await? {
            // Try to create delta
            if let StateSnapshot::Full(parent_state) = &parent_version.state {
                let delta = StateDelta::compute(parent_state, state);
                let delta_size = delta.changes.len() + delta.removals.len();
                let state_size = state.len();
                
                if (delta_size as f64) / (state_size as f64) < self.config.delta_threshold {
                    let mut delta = delta;
                    delta.base_version = current.clone();
                    StateSnapshot::Delta(delta)
                } else {
                    StateSnapshot::Full(state.clone())
                }
            } else {
                StateSnapshot::Full(state.clone())
            }
        } else {
            StateSnapshot::Full(state.clone())
        };
        
        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.total_versions += 1;
        match &snapshot {
            StateSnapshot::Full(_) => metrics.total_snapshots += 1,
            StateSnapshot::Delta(_) => metrics.total_deltas += 1,
            StateSnapshot::Compressed(_) => metrics.total_snapshots += 1,
        }
        
        // Create version
        let version = Version {
            id: new_id.clone(),
            parent_id,
            state: snapshot,
            metadata,
        };
        
        // Store version
        self.storage.store(version).await?;
        
        // Update cache
        let mut cache = self.cache.write().await;
        cache.put(new_id.clone(), state.clone());
        
        // Update current version
        *current = new_id.clone();
        
        // Prune old versions if needed
        self.prune_old_versions().await?;
        
        info!("Created version {}", new_id.id);
        Ok(new_id)
    }
    
    /// Get a specific version
    #[instrument(skip(self))]
    pub async fn get_version(&self, id: &VersionId) -> Result<Option<StateData>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(state) = cache.get(id) {
                let mut metrics = self.metrics.write().await;
                metrics.cache_hits += 1;
                debug!("Cache hit for version {}", id.id);
                return Ok(Some(state));
            }
        }
        
        // Cache miss - reconstruct from storage
        let mut metrics = self.metrics.write().await;
        metrics.cache_misses += 1;
        drop(metrics);
        
        // Get version from storage
        if let Some(version) = self.storage.get(id).await? {
            let state = self.reconstruct_state(&version).await?;
            
            // Update cache
            let mut cache = self.cache.write().await;
            cache.put(id.clone(), state.clone());
            
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }
    
    /// Reconstruct state from version
    fn reconstruct_state<'a>(&'a self, version: &'a Version) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<StateData>> + Send + 'a>> {
        Box::pin(async move {
            match &version.state {
                StateSnapshot::Full(state) => Ok(state.clone()),
                
                StateSnapshot::Delta(delta) => {
                    // Get base state
                    if let Some(base_state) = self.get_version(&delta.base_version).await? {
                        Ok(delta.apply(&base_state))
                    } else {
                        Err(crate::graph::GraphError::RuntimeError(
                            format!("Base version {} not found", delta.base_version.id)
                        ).into())
                    }
                }
                
                StateSnapshot::Compressed(data) => {
                    // Decompress (simplified - would use actual compression)
                    let json = String::from_utf8(data.clone())
                        .map_err(|e| crate::graph::GraphError::RuntimeError(e.to_string()))?;
                    let state: StateData = serde_json::from_str(&json)
                        .map_err(|e| crate::graph::GraphError::RuntimeError(e.to_string()))?;
                    Ok(state)
                }
            }
        })
    }
    
    /// Rollback to a specific version
    #[instrument(skip(self))]
    pub async fn rollback(&self, id: &VersionId) -> Result<StateData> {
        if let Some(state) = self.get_version(id).await? {
            // Update current version
            let mut current = self.current_version.write().await;
            *current = id.clone();
            
            info!("Rolled back to version {}", id.id);
            Ok(state)
        } else {
            Err(crate::graph::GraphError::RuntimeError(
                format!("Version {} not found", id.id)
            ).into())
        }
    }
    
    /// Get current version
    pub async fn current(&self) -> VersionId {
        self.current_version.read().await.clone()
    }
    
    /// List versions in range
    pub async fn list_versions(&self, start: u64, end: u64) -> Result<Vec<VersionId>> {
        self.storage.list(start, end).await
    }
    
    /// Prune old versions
    async fn prune_old_versions(&self) -> Result<()> {
        let size = self.storage.size().await?;
        
        if size > self.config.max_versions {
            let to_remove = size - self.config.max_versions;
            let versions = self.storage.list(0, to_remove as u64).await?;
            
            for id in versions {
                // Don't remove checkpoints
                if let Some(version) = self.storage.get(&id).await? {
                    if !version.metadata.is_checkpoint {
                        self.storage.delete(&id).await?;
                        debug!("Pruned version {}", id.id);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Get metrics
    pub async fn metrics(&self) -> VersioningMetrics {
        self.metrics.read().await.clone()
    }
}

/// Branch management for versioning
pub struct BranchManager {
    /// Branch heads
    branches: Arc<RwLock<HashMap<String, VersionId>>>,
    
    /// Current branch
    current_branch: Arc<RwLock<String>>,
}

impl BranchManager {
    pub fn new() -> Self {
        let mut branches = HashMap::new();
        branches.insert("main".to_string(), VersionId::new(0));
        
        Self {
            branches: Arc::new(RwLock::new(branches)),
            current_branch: Arc::new(RwLock::new("main".to_string())),
        }
    }
    
    /// Create a new branch
    pub async fn create_branch(&self, name: String, from: VersionId) -> Result<()> {
        let mut branches = self.branches.write().await;
        branches.insert(name, from);
        Ok(())
    }
    
    /// Switch branch
    pub async fn switch_branch(&self, name: String) -> Result<()> {
        let branches = self.branches.read().await;
        if branches.contains_key(&name) {
            let mut current = self.current_branch.write().await;
            *current = name;
            Ok(())
        } else {
            Err(crate::graph::GraphError::RuntimeError(
                format!("Branch {} not found", name)
            ).into())
        }
    }
    
    /// Get current branch
    pub async fn current_branch(&self) -> String {
        self.current_branch.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_state_delta() {
        let mut base = StateData::new();
        base.insert("key1".to_string(), serde_json::json!("value1"));
        base.insert("key2".to_string(), serde_json::json!("value2"));
        
        let mut target = StateData::new();
        target.insert("key1".to_string(), serde_json::json!("modified"));
        target.insert("key3".to_string(), serde_json::json!("new"));
        
        let delta = StateDelta::compute(&base, &target);
        
        assert_eq!(delta.changes.len(), 2); // key1 modified, key3 added
        assert_eq!(delta.removals.len(), 1); // key2 removed
        
        let reconstructed = delta.apply(&base);
        assert_eq!(reconstructed, target);
    }
    
    #[tokio::test]
    async fn test_versioning_system() {
        let storage = Arc::new(InMemoryStorage::new());
        let system = StateVersioningSystem::new(storage, VersioningConfig::default());
        
        let mut state1 = StateData::new();
        state1.insert("key1".to_string(), serde_json::json!("value1"));
        
        let v1 = system.create_version(&state1, VersionMetadata::default()).await.unwrap();
        
        let mut state2 = StateData::new();
        state2.insert("key1".to_string(), serde_json::json!("modified"));
        state2.insert("key2".to_string(), serde_json::json!("value2"));
        
        let v2 = system.create_version(&state2, VersionMetadata::default()).await.unwrap();
        
        // Get version 1
        let retrieved1 = system.get_version(&v1).await.unwrap().unwrap();
        assert_eq!(retrieved1, state1);
        
        // Get version 2
        let retrieved2 = system.get_version(&v2).await.unwrap().unwrap();
        assert_eq!(retrieved2, state2);
        
        // Rollback to version 1
        let rolled_back = system.rollback(&v1).await.unwrap();
        assert_eq!(rolled_back, state1);
    }
    
    #[tokio::test]
    async fn test_branch_manager() {
        let manager = BranchManager::new();
        
        let version = VersionId::new(1);
        manager.create_branch("feature".to_string(), version.clone()).await.unwrap();
        
        manager.switch_branch("feature".to_string()).await.unwrap();
        
        let current = manager.current_branch().await;
        assert_eq!(current, "feature");
    }
    
    #[tokio::test]
    async fn test_cache_efficiency() {
        let storage = Arc::new(InMemoryStorage::new());
        let system = StateVersioningSystem::new(storage, VersioningConfig::default());
        
        let mut state = StateData::new();
        state.insert("key".to_string(), serde_json::json!("value"));
        
        let v1 = system.create_version(&state, VersionMetadata::default()).await.unwrap();
        
        // First access - cache miss
        let _ = system.get_version(&v1).await.unwrap();
        
        // Second access - cache hit
        let _ = system.get_version(&v1).await.unwrap();
        
        let metrics = system.metrics().await;
        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_misses, 1);
    }
}