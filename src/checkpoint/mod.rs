//! Checkpointing and state persistence for LangGraph

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::state::{GraphState, StateData};

/// Errors related to checkpointing
#[derive(Error, Debug)]
pub enum CheckpointError {
    #[error("Checkpoint not found: {0}")]
    NotFound(String),
    
    #[error("Failed to save checkpoint: {0}")]
    SaveFailed(String),
    
    #[error("Failed to load checkpoint: {0}")]
    LoadFailed(String),
    
    #[error("Invalid checkpoint data: {0}")]
    InvalidData(String),
}

/// A checkpoint representing a saved state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique checkpoint ID
    pub id: String,
    
    /// Thread ID this checkpoint belongs to
    pub thread_id: String,
    
    /// The saved state
    pub state: GraphState,
    
    /// Timestamp when checkpoint was created
    pub created_at: u64,
    
    /// Optional checkpoint metadata
    pub metadata: Option<serde_json::Value>,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(thread_id: impl Into<String>, state: GraphState) -> Self {
        let thread_id_str = thread_id.into();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            id: format!("checkpoint-{}-{}", thread_id_str, timestamp),
            thread_id: thread_id_str,
            state,
            created_at: timestamp,
            metadata: None,
        }
    }
    
    /// Create a checkpoint with custom ID
    pub fn with_id(
        id: impl Into<String>,
        thread_id: impl Into<String>,
        state: GraphState,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            id: id.into(),
            thread_id: thread_id.into(),
            state,
            created_at: timestamp,
            metadata: None,
        }
    }
    
    /// Add metadata to the checkpoint
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Trait for checkpoint storage implementations
#[async_trait]
pub trait Checkpointer: Send + Sync {
    /// Save a checkpoint
    async fn save(&self, checkpoint: Checkpoint) -> Result<String, CheckpointError>;
    
    /// Load a checkpoint by ID
    async fn load(&self, checkpoint_id: &str) -> Result<Checkpoint, CheckpointError>;
    
    /// Load the latest checkpoint for a thread
    async fn load_latest(&self, thread_id: &str) -> Result<Option<Checkpoint>, CheckpointError>;
    
    /// List all checkpoints for a thread
    async fn list(&self, thread_id: &str) -> Result<Vec<String>, CheckpointError>;
    
    /// Delete a checkpoint
    async fn delete(&self, checkpoint_id: &str) -> Result<(), CheckpointError>;
    
    /// Delete all checkpoints for a thread
    async fn delete_thread(&self, thread_id: &str) -> Result<(), CheckpointError>;
}

/// In-memory checkpoint storage
pub struct InMemoryCheckpointer {
    checkpoints: Arc<dashmap::DashMap<String, Checkpoint>>,
    thread_index: Arc<dashmap::DashMap<String, Vec<String>>>,
}

impl InMemoryCheckpointer {
    /// Create a new in-memory checkpointer
    pub fn new() -> Self {
        Self {
            checkpoints: Arc::new(dashmap::DashMap::new()),
            thread_index: Arc::new(dashmap::DashMap::new()),
        }
    }
}

impl Default for InMemoryCheckpointer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Checkpointer for InMemoryCheckpointer {
    async fn save(&self, checkpoint: Checkpoint) -> Result<String, CheckpointError> {
        let id = checkpoint.id.clone();
        let thread_id = checkpoint.thread_id.clone();
        
        // Save checkpoint
        self.checkpoints.insert(id.clone(), checkpoint);
        
        // Update thread index
        self.thread_index
            .entry(thread_id)
            .and_modify(|ids| ids.push(id.clone()))
            .or_insert_with(|| vec![id.clone()]);
        
        Ok(id)
    }
    
    async fn load(&self, checkpoint_id: &str) -> Result<Checkpoint, CheckpointError> {
        self.checkpoints
            .get(checkpoint_id)
            .map(|entry| entry.clone())
            .ok_or_else(|| CheckpointError::NotFound(checkpoint_id.to_string()))
    }
    
    async fn load_latest(&self, thread_id: &str) -> Result<Option<Checkpoint>, CheckpointError> {
        if let Some(ids) = self.thread_index.get(thread_id) {
            if let Some(latest_id) = ids.last() {
                return Ok(self.checkpoints.get(latest_id).map(|entry| entry.clone()));
            }
        }
        Ok(None)
    }
    
    async fn list(&self, thread_id: &str) -> Result<Vec<String>, CheckpointError> {
        Ok(self.thread_index
            .get(thread_id)
            .map(|entry| entry.clone())
            .unwrap_or_default())
    }
    
    async fn delete(&self, checkpoint_id: &str) -> Result<(), CheckpointError> {
        if let Some((_, checkpoint)) = self.checkpoints.remove(checkpoint_id) {
            // Remove from thread index
            if let Some(mut ids) = self.thread_index.get_mut(&checkpoint.thread_id) {
                ids.retain(|id| id != checkpoint_id);
            }
            Ok(())
        } else {
            Err(CheckpointError::NotFound(checkpoint_id.to_string()))
        }
    }
    
    async fn delete_thread(&self, thread_id: &str) -> Result<(), CheckpointError> {
        if let Some((_, ids)) = self.thread_index.remove(thread_id) {
            for id in ids {
                self.checkpoints.remove(&id);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_checkpoint_creation() {
        let state = GraphState::new();
        let checkpoint = Checkpoint::new("thread-1", state.clone());
        
        assert_eq!(checkpoint.thread_id, "thread-1");
        assert!(checkpoint.id.starts_with("checkpoint-thread-1-"));
        assert!(checkpoint.created_at > 0);
    }
    
    #[tokio::test]
    async fn test_in_memory_checkpointer() {
        let checkpointer = InMemoryCheckpointer::new();
        let state = GraphState::new();
        let checkpoint = Checkpoint::new("thread-1", state);
        
        // Save checkpoint
        let id = checkpointer.save(checkpoint.clone()).await.unwrap();
        assert_eq!(id, checkpoint.id);
        
        // Load checkpoint
        let loaded = checkpointer.load(&id).await.unwrap();
        assert_eq!(loaded.id, checkpoint.id);
        assert_eq!(loaded.thread_id, checkpoint.thread_id);
        
        // List checkpoints
        let ids = checkpointer.list("thread-1").await.unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], id);
        
        // Load latest
        let latest = checkpointer.load_latest("thread-1").await.unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().id, id);
        
        // Delete checkpoint
        checkpointer.delete(&id).await.unwrap();
        let ids = checkpointer.list("thread-1").await.unwrap();
        assert_eq!(ids.len(), 0);
    }
    
    #[tokio::test]
    async fn test_delete_thread() {
        let checkpointer = InMemoryCheckpointer::new();
        
        // Create multiple checkpoints for same thread
        for i in 0..3 {
            let state = GraphState::new();
            let checkpoint = Checkpoint::with_id(
                format!("checkpoint-{}", i),
                "thread-1",
                state,
            );
            checkpointer.save(checkpoint).await.unwrap();
        }
        
        // Verify all saved
        let ids = checkpointer.list("thread-1").await.unwrap();
        assert_eq!(ids.len(), 3);
        
        // Delete entire thread
        checkpointer.delete_thread("thread-1").await.unwrap();
        
        // Verify all deleted
        let ids = checkpointer.list("thread-1").await.unwrap();
        assert_eq!(ids.len(), 0);
    }
}