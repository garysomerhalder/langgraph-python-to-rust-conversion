use crate::checkpoint::{Checkpoint, CheckpointError, Checkpointer, CheckpointerOld};
use crate::state::GraphState;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
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
            Err(CheckpointError::NotFound(format!(
                "No checkpoints for thread {}",
                thread_id
            )))
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
                return Ok(Some(CheckpointerOld::load(self, last_id).await?));
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
        let checkpoint = CheckpointerOld::load(self, checkpoint_id).await?;

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

#[async_trait]
impl Checkpointer for MemoryCheckpointer {
    async fn save(
        &self,
        thread_id: &str,
        checkpoint: HashMap<String, Value>,
        metadata: HashMap<String, Value>,
        parent_checkpoint_id: Option<String>,
    ) -> Result<String> {
        // Generate checkpoint ID
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let checkpoint_id = format!("checkpoint-{}-{}", thread_id, timestamp);

        // Create checkpoint with both data and metadata
        let mut checkpoint_data = checkpoint;
        checkpoint_data.insert("metadata".to_string(), serde_json::to_value(metadata)?);
        if let Some(parent_id) = parent_checkpoint_id {
            checkpoint_data.insert("parent_checkpoint_id".to_string(), Value::String(parent_id));
        }

        // Store the checkpoint
        self.checkpoints.insert(
            checkpoint_id.clone(),
            Checkpoint {
                id: checkpoint_id.clone(),
                thread_id: thread_id.to_string(),
                state: GraphState::new(), // Placeholder - will be set from checkpoint data
                created_at: timestamp,
                metadata: Some(serde_json::to_value(&checkpoint_data)?),
            },
        );

        // Update thread index
        self.thread_index
            .entry(thread_id.to_string())
            .or_insert_with(Vec::new)
            .push(checkpoint_id.clone());

        Ok(checkpoint_id)
    }

    async fn load(
        &self,
        thread_id: &str,
        checkpoint_id: Option<String>,
    ) -> Result<Option<(HashMap<String, Value>, HashMap<String, Value>)>> {
        let id = if let Some(id) = checkpoint_id {
            id
        } else {
            // Load latest checkpoint for thread
            if let Some(checkpoint_ids) = self.thread_index.get(thread_id) {
                if let Some(last_id) = checkpoint_ids.last() {
                    last_id.clone()
                } else {
                    return Ok(None);
                }
            } else {
                return Ok(None);
            }
        };

        if let Some(checkpoint) = self.checkpoints.get(&id) {
            if let Some(ref metadata_value) = checkpoint.metadata {
                let mut checkpoint_data: HashMap<String, Value> =
                    serde_json::from_value(metadata_value.clone())?;

                // Extract metadata if present
                let metadata = if let Some(metadata_val) = checkpoint_data.remove("metadata") {
                    serde_json::from_value(metadata_val)?
                } else {
                    HashMap::new()
                };

                Ok(Some((checkpoint_data, metadata)))
            } else {
                Ok(Some((HashMap::new(), HashMap::new())))
            }
        } else {
            Ok(None)
        }
    }

    async fn list(
        &self,
        thread_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<(String, HashMap<String, Value>)>> {
        let mut results = Vec::new();

        if let Some(thread_id) = thread_id {
            // List checkpoints for specific thread
            if let Some(checkpoint_ids) = self.thread_index.get(thread_id) {
                for id in checkpoint_ids.iter() {
                    if let Some(checkpoint) = self.checkpoints.get(id) {
                        let metadata = if let Some(ref metadata_value) = checkpoint.metadata {
                            let checkpoint_data: HashMap<String, Value> =
                                serde_json::from_value(metadata_value.clone())?;

                            if let Some(metadata_val) = checkpoint_data.get("metadata") {
                                serde_json::from_value(metadata_val.clone())?
                            } else {
                                HashMap::new()
                            }
                        } else {
                            HashMap::new()
                        };

                        results.push((id.clone(), metadata));
                    }
                }
            }
        } else {
            // List all checkpoints
            for entry in self.checkpoints.iter() {
                let id = entry.key().clone();
                let checkpoint = entry.value();

                let metadata = if let Some(ref metadata_value) = checkpoint.metadata {
                    let checkpoint_data: HashMap<String, Value> =
                        serde_json::from_value(metadata_value.clone())?;

                    if let Some(metadata_val) = checkpoint_data.get("metadata") {
                        serde_json::from_value(metadata_val.clone())?
                    } else {
                        HashMap::new()
                    }
                } else {
                    HashMap::new()
                };

                results.push((id, metadata));
            }
        }

        // Apply limit if specified
        if let Some(limit) = limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    async fn delete(&self, thread_id: &str, checkpoint_id: Option<&str>) -> Result<()> {
        if let Some(id) = checkpoint_id {
            // Delete specific checkpoint
            self.checkpoints.remove(id);

            // Remove from thread index
            if let Some(mut checkpoint_ids) = self.thread_index.get_mut(thread_id) {
                checkpoint_ids.retain(|existing_id| existing_id != id);
            }
        } else {
            // Delete all checkpoints for thread
            if let Some(checkpoint_ids) = self.thread_index.remove(thread_id) {
                // Remove all checkpoints
                for id in checkpoint_ids.1 {
                    self.checkpoints.remove(&id);
                }
            }
        }

        Ok(())
    }
}
