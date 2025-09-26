use crate::checkpoint::{Checkpoint, CheckpointError, CheckpointerOld};
use async_trait::async_trait;
use std::sync::Arc;

/// Alias for InMemoryCheckpointer
pub type InMemoryCheckpointer = MemoryCheckpointer;

/// In-memory checkpoint storage
#[derive(Clone)]
pub struct MemoryCheckpointer {
    checkpoints: Arc<dashmap::DashMap<String, Checkpoint>>,
    thread_index: Arc<dashmap::DashMap<String, Vec<String>>>,
}

impl MemoryCheckpointer {
    /// Create a new in-memory checkpointer
    pub fn new() -> Self {
        Self {
            checkpoints: Arc::new(dashmap::DashMap::new()),
            thread_index: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Get latest checkpoint ID for a thread (test compatibility)
    pub async fn get_latest_checkpoint(&self, thread_id: &str) -> Result<String, CheckpointError> {
        if let Some(checkpoint) = self.load_latest(thread_id).await? {
            Ok(checkpoint.id)
        } else {
            Err(CheckpointError::NotFound(format!("No checkpoints for thread {}", thread_id)))
        }
    }
}

impl Default for MemoryCheckpointer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CheckpointerOld for MemoryCheckpointer {
    async fn save(&self, checkpoint: Checkpoint) -> Result<String, CheckpointError> {
        let id = checkpoint.id.clone();
        let thread_id = checkpoint.thread_id.clone();

        // Store the checkpoint
        self.checkpoints.insert(id.clone(), checkpoint);

        // Update thread index
        self.thread_index
            .entry(thread_id)
            .or_insert_with(Vec::new)
            .push(id.clone());

        Ok(id)
    }

    async fn load(&self, checkpoint_id: &str) -> Result<Checkpoint, CheckpointError> {
        self.checkpoints
            .get(checkpoint_id)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| CheckpointError::NotFound(checkpoint_id.to_string()))
    }

    async fn load_latest(&self, thread_id: &str) -> Result<Option<Checkpoint>, CheckpointError> {
        if let Some(checkpoint_ids) = self.thread_index.get(thread_id) {
            if let Some(last_id) = checkpoint_ids.last() {
                return Ok(Some(self.load(last_id).await?));
            }
        }
        Ok(None)
    }

    async fn list(&self, thread_id: &str) -> Result<Vec<String>, CheckpointError> {
        Ok(self
            .thread_index
            .get(thread_id)
            .map(|entry| entry.value().clone())
            .unwrap_or_default())
    }

    async fn delete(&self, checkpoint_id: &str) -> Result<(), CheckpointError> {
        // Get the checkpoint to find its thread_id
        let checkpoint = self.load(checkpoint_id).await?;

        // Remove from checkpoints
        self.checkpoints.remove(checkpoint_id);

        // Remove from thread index
        if let Some(mut checkpoint_ids) = self.thread_index.get_mut(&checkpoint.thread_id) {
            checkpoint_ids.retain(|id| id != checkpoint_id);
        }

        Ok(())
    }

    async fn delete_thread(&self, thread_id: &str) -> Result<(), CheckpointError> {
        // Get all checkpoint IDs for the thread
        if let Some(checkpoint_ids) = self.thread_index.remove(thread_id) {
            // Remove all checkpoints
            for id in checkpoint_ids.1 {
                self.checkpoints.remove(&id);
            }
        }
        Ok(())
    }
}