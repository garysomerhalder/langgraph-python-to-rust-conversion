use crate::checkpoint::Checkpointer;
use crate::state::GraphState;
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use redis::aio::{ConnectionManager, PubSub};
use redis::{AsyncCommands, Client, Pipeline, Script};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Redis configuration for checkpointer
#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub redis_url: String,
    pub pool_size: u32,
    pub default_ttl_secs: u64,
    pub key_prefix: String,
    pub enable_cluster: bool,
    pub compression: bool,
    pub enable_pubsub: bool,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://localhost:6379/0".to_string(),
            pool_size: 10,
            default_ttl_secs: 86400, // 24 hours
            key_prefix: "langgraph:".to_string(),
            enable_cluster: false,
            compression: false,
            enable_pubsub: false,
            max_retries: 3,
            retry_delay_ms: 100,
        }
    }
}

/// Statistics for a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointStats {
    pub checkpoint_id: String,
    pub size_bytes: usize,
    pub compressed: bool,
    pub compression_ratio: f64,
    pub created_at: i64,
    pub ttl_remaining: i64,
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub active_connections: usize,
    pub pool_size: usize,
    pub total_commands: u64,
    pub failed_commands: u64,
}

/// Notification for state changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChangeNotification {
    pub thread_id: String,
    pub checkpoint_id: String,
    pub timestamp: i64,
}

/// Redis-based checkpointer for high-speed caching
pub struct RedisCheckpointer {
    client: Client,
    connection_manager: Arc<RwLock<ConnectionManager>>,
    config: RedisConfig,
    pubsub_client: Option<Client>,
}

impl RedisCheckpointer {
    /// Create a new Redis checkpointer
    pub async fn new(config: RedisConfig) -> Result<Self> {
        let client = Client::open(config.redis_url.as_str())
            .context("Failed to create Redis client")?;

        let connection_manager = ConnectionManager::new(client.clone()).await
            .context("Failed to create connection manager")?;

        let pubsub_client = if config.enable_pubsub {
            Some(Client::open(config.redis_url.as_str())?)
        } else {
            None
        };

        Ok(Self {
            client,
            connection_manager: Arc::new(RwLock::new(connection_manager)),
            config,
            pubsub_client,
        })
    }

    /// Get key for checkpoint
    fn get_checkpoint_key(&self, thread_id: &str, checkpoint_id: &str) -> String {
        format!("{}checkpoint:{}:{}", self.config.key_prefix, thread_id, checkpoint_id)
    }

    /// Get key for thread's latest checkpoint
    fn get_thread_latest_key(&self, thread_id: &str) -> String {
        format!("{}thread:{}:latest", self.config.key_prefix, thread_id)
    }

    /// Get key for thread's checkpoint list
    fn get_thread_list_key(&self, thread_id: &str) -> String {
        format!("{}thread:{}:checkpoints", self.config.key_prefix, thread_id)
    }

    /// Get pubsub channel for thread
    fn get_pubsub_channel(&self, thread_id: &str) -> String {
        format!("{}pubsub:{}", self.config.key_prefix, thread_id)
    }

    /// Compress state if configured
    fn compress_state(&self, state: &GraphState) -> Result<Vec<u8>> {
        let json = serde_json::to_vec(&state.values)?;

        if self.config.compression {
            // Simple compression using flate2 or similar
            // For now, just return JSON bytes
            Ok(json)
        } else {
            Ok(json)
        }
    }

    /// Decompress state if needed
    fn decompress_state(&self, data: Vec<u8>) -> Result<GraphState> {
        let values: HashMap<String, Value> = if self.config.compression {
            // Decompress if needed
            serde_json::from_slice(&data)?
        } else {
            serde_json::from_slice(&data)?
        };

        let mut state = GraphState::new();
        for (key, value) in values {
            state.set(key, value);
        }

        Ok(state)
    }

    /// Save a checkpoint
    pub async fn save_checkpoint(&self, thread_id: &str, state: &GraphState) -> Result<String> {
        let checkpoint_id = Uuid::new_v4().to_string();
        let key = self.get_checkpoint_key(thread_id, &checkpoint_id);
        let latest_key = self.get_thread_latest_key(thread_id);
        let list_key = self.get_thread_list_key(thread_id);

        let data = self.compress_state(state)?;

        let mut conn = self.connection_manager.write().await;

        // Save checkpoint with TTL
        conn.set_ex(&key, data, self.config.default_ttl_secs).await
            .context("Failed to save checkpoint")?;

        // Update latest pointer
        conn.set_ex(&latest_key, &checkpoint_id, self.config.default_ttl_secs).await
            .context("Failed to update latest pointer")?;

        // Add to checkpoint list
        conn.lpush(&list_key, &checkpoint_id).await
            .context("Failed to add to checkpoint list")?;

        // Trim list to reasonable size
        conn.ltrim(&list_key, 0, 99).await
            .context("Failed to trim checkpoint list")?;

        // Publish notification if enabled
        if self.config.enable_pubsub {
            let channel = self.get_pubsub_channel(thread_id);
            let notification = StateChangeNotification {
                thread_id: thread_id.to_string(),
                checkpoint_id: checkpoint_id.clone(),
                timestamp: chrono::Utc::now().timestamp(),
            };

            let notification_json = serde_json::to_string(&notification)?;
            let _: () = conn.publish(channel, notification_json).await?;
        }

        Ok(checkpoint_id)
    }

    /// Load a checkpoint
    pub async fn load_checkpoint(
        &self,
        thread_id: &str,
        checkpoint_id: Option<&str>,
    ) -> Result<Option<GraphState>> {
        let mut conn = self.connection_manager.write().await;

        let actual_checkpoint_id = if let Some(id) = checkpoint_id {
            id.to_string()
        } else {
            // Load latest
            let latest_key = self.get_thread_latest_key(thread_id);
            match conn.get::<_, Option<String>>(&latest_key).await? {
                Some(id) => id,
                None => return Ok(None),
            }
        };

        let key = self.get_checkpoint_key(thread_id, &actual_checkpoint_id);
        let data: Option<Vec<u8>> = conn.get(&key).await?;

        if let Some(data) = data {
            let state = self.decompress_state(data)?;
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    /// Batch save multiple checkpoints using pipeline
    pub async fn batch_save_checkpoints(
        &self,
        checkpoints: Vec<(String, GraphState)>,
    ) -> Result<Vec<String>> {
        let mut pipe = Pipeline::new();
        let mut ids = Vec::new();

        for (thread_id, state) in checkpoints {
            let checkpoint_id = Uuid::new_v4().to_string();
            let key = self.get_checkpoint_key(&thread_id, &checkpoint_id);
            let data = self.compress_state(&state)?;

            pipe.set_ex(&key, data, self.config.default_ttl_secs);
            ids.push(checkpoint_id);
        }

        let mut conn = self.connection_manager.write().await;
        pipe.query_async(&mut *conn).await
            .context("Failed to execute pipeline")?;

        Ok(ids)
    }

    /// Batch load multiple checkpoints
    pub async fn batch_load_checkpoints(
        &self,
        checkpoint_ids: &[String],
    ) -> Result<Vec<Option<GraphState>>> {
        let mut pipe = Pipeline::new();

        for checkpoint_id in checkpoint_ids {
            // Parse thread_id from checkpoint_id (assuming format)
            let thread_id = checkpoint_id.split('-').next().unwrap_or("default");
            let key = self.get_checkpoint_key(thread_id, checkpoint_id);
            pipe.get(&key);
        }

        let mut conn = self.connection_manager.write().await;
        let results: Vec<Option<Vec<u8>>> = pipe.query_async(&mut *conn).await?;

        let mut states = Vec::new();
        for data_opt in results {
            if let Some(data) = data_opt {
                states.push(Some(self.decompress_state(data)?));
            } else {
                states.push(None);
            }
        }

        Ok(states)
    }

    /// Get checkpoint statistics
    pub async fn get_checkpoint_stats(&self, checkpoint_id: &str) -> Result<CheckpointStats> {
        // Parse thread_id from checkpoint_id
        let thread_id = checkpoint_id.split('-').next().unwrap_or("default");
        let key = self.get_checkpoint_key(thread_id, checkpoint_id);

        let mut conn = self.connection_manager.write().await;

        let data: Option<Vec<u8>> = conn.get(&key).await?;
        let ttl: i64 = conn.ttl(&key).await?;

        if let Some(data) = data {
            let size = data.len();
            let original_size = if self.config.compression {
                // Estimate original size (simplified)
                size * 2
            } else {
                size
            };

            Ok(CheckpointStats {
                checkpoint_id: checkpoint_id.to_string(),
                size_bytes: size,
                compressed: self.config.compression,
                compression_ratio: if self.config.compression {
                    original_size as f64 / size as f64
                } else {
                    1.0
                },
                created_at: chrono::Utc::now().timestamp(),
                ttl_remaining: ttl,
            })
        } else {
            Err(anyhow::anyhow!("Checkpoint not found"))
        }
    }

    /// Subscribe to state changes for a thread
    pub async fn subscribe_to_changes(&self, thread_id: &str) -> Result<PubSubReceiver> {
        if let Some(client) = &self.pubsub_client {
            let mut pubsub = client.get_async_pubsub().await?;
            let channel = self.get_pubsub_channel(thread_id);
            pubsub.subscribe(&channel).await?;

            Ok(PubSubReceiver {
                pubsub: Arc::new(RwLock::new(pubsub)),
            })
        } else {
            Err(anyhow::anyhow!("PubSub not enabled"))
        }
    }

    /// Compare and swap operation for atomic updates
    pub async fn compare_and_swap(
        &self,
        thread_id: &str,
        expected_checkpoint_id: Option<&str>,
        new_state: &GraphState,
    ) -> Result<bool> {
        // Lua script for atomic CAS
        let script = Script::new(
            r"
            local latest_key = KEYS[1]
            local expected = ARGV[1]
            local new_id = ARGV[2]
            local new_key = ARGV[3]
            local new_data = ARGV[4]
            local ttl = ARGV[5]

            local current = redis.call('GET', latest_key)

            if current == expected or (expected == '' and current == false) then
                redis.call('SETEX', new_key, ttl, new_data)
                redis.call('SET', latest_key, new_id)
                return 1
            else
                return 0
            end
            "
        );

        let latest_key = self.get_thread_latest_key(thread_id);
        let new_checkpoint_id = Uuid::new_v4().to_string();
        let new_key = self.get_checkpoint_key(thread_id, &new_checkpoint_id);
        let new_data = self.compress_state(new_state)?;

        let mut conn = self.connection_manager.write().await;

        let result: i32 = script
            .key(latest_key)
            .arg(expected_checkpoint_id.unwrap_or(""))
            .arg(&new_checkpoint_id)
            .arg(new_key)
            .arg(new_data)
            .arg(self.config.default_ttl_secs)
            .invoke_async(&mut *conn)
            .await?;

        Ok(result == 1)
    }

    /// Health check
    pub async fn health_check(&self) -> Result<bool> {
        let mut conn = self.connection_manager.write().await;
        let pong: String = redis::cmd("PING").query_async(&mut *conn).await?;
        Ok(pong == "PONG")
    }

    /// Get connection statistics
    pub async fn get_connection_stats(&self) -> Result<ConnectionStats> {
        let mut conn = self.connection_manager.write().await;
        let info: String = redis::cmd("INFO").arg("stats").query_async(&mut *conn).await?;

        // Parse INFO output (simplified)
        let mut total_commands = 0u64;
        let mut failed_commands = 0u64;

        for line in info.lines() {
            if line.starts_with("total_commands_processed:") {
                total_commands = line.split(':').nth(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0);
            }
        }

        Ok(ConnectionStats {
            active_connections: 1, // Simplified
            pool_size: self.config.pool_size as usize,
            total_commands,
            failed_commands,
        })
    }
}

// Implement Clone for sharing across threads
impl Clone for RedisCheckpointer {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            connection_manager: self.connection_manager.clone(),
            config: self.config.clone(),
            pubsub_client: self.pubsub_client.clone(),
        }
    }
}

/// Receiver for PubSub notifications
pub struct PubSubReceiver {
    pubsub: Arc<RwLock<PubSub>>,
}

impl PubSubReceiver {
    /// Receive next notification
    pub async fn recv(&mut self) -> Result<StateChangeNotification> {
        let mut pubsub = self.pubsub.write().await;
        let msg = pubsub.on_message().next().await
            .ok_or_else(|| anyhow::anyhow!("No message received"))?;

        let payload: String = msg.get_payload()?;
        let notification: StateChangeNotification = serde_json::from_str(&payload)?;
        Ok(notification)
    }
}

#[async_trait]
impl Checkpointer for RedisCheckpointer {
    async fn save(
        &self,
        thread_id: &str,
        checkpoint: HashMap<String, Value>,
        metadata: HashMap<String, Value>,
        parent_checkpoint_id: Option<String>,
    ) -> Result<String> {
        let mut state = GraphState::new();
        for (key, value) in checkpoint {
            state.set(key, value);
        }

        // Save main checkpoint
        let checkpoint_id = self.save_checkpoint(thread_id, &state).await?;

        // Save metadata separately if provided
        if !metadata.is_empty() {
            let metadata_key = format!("{}metadata:{}", self.config.key_prefix, checkpoint_id);
            let metadata_json = serde_json::to_vec(&metadata)?;

            let mut conn = self.connection_manager.write().await;
            conn.set_ex(metadata_key, metadata_json, self.config.default_ttl_secs).await?;
        }

        // Store parent reference if provided
        if let Some(parent_id) = parent_checkpoint_id {
            let parent_key = format!("{}parent:{}", self.config.key_prefix, checkpoint_id);
            let mut conn = self.connection_manager.write().await;
            conn.set_ex(parent_key, parent_id, self.config.default_ttl_secs).await?;
        }

        Ok(checkpoint_id)
    }

    async fn load(
        &self,
        thread_id: &str,
        checkpoint_id: Option<String>,
    ) -> Result<Option<(HashMap<String, Value>, HashMap<String, Value>)>> {
        if let Some(state) = self.load_checkpoint(thread_id, checkpoint_id.as_deref()).await? {
            let checkpoint = state.values.clone();

            // Load metadata if checkpoint_id is provided
            let metadata = if let Some(id) = checkpoint_id {
                let metadata_key = format!("{}metadata:{}", self.config.key_prefix, id);
                let mut conn = self.connection_manager.write().await;

                if let Some(metadata_json) = conn.get::<_, Option<Vec<u8>>>(metadata_key).await? {
                    serde_json::from_slice(&metadata_json)?
                } else {
                    HashMap::new()
                }
            } else {
                HashMap::new()
            };

            Ok(Some((checkpoint, metadata)))
        } else {
            Ok(None)
        }
    }

    async fn list(
        &self,
        thread_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<(String, HashMap<String, Value>)>> {
        if let Some(tid) = thread_id {
            let list_key = self.get_thread_list_key(tid);
            let mut conn = self.connection_manager.write().await;

            let checkpoint_ids: Vec<String> = conn.lrange(list_key, 0, limit.unwrap_or(100) as isize).await?;

            let mut results = Vec::new();
            for checkpoint_id in checkpoint_ids {
                let mut metadata = HashMap::new();
                metadata.insert("thread_id".to_string(), Value::String(tid.to_string()));
                metadata.insert("checkpoint_id".to_string(), Value::String(checkpoint_id.clone()));
                results.push((checkpoint_id, metadata));
            }

            Ok(results)
        } else {
            // List all threads (simplified - would need to track separately)
            Ok(Vec::new())
        }
    }

    async fn delete(&self, thread_id: &str, checkpoint_id: Option<&str>) -> Result<()> {
        let mut conn = self.connection_manager.write().await;

        if let Some(id) = checkpoint_id {
            // Delete specific checkpoint
            let key = self.get_checkpoint_key(thread_id, id);
            conn.del(key).await?;

            // Remove from list
            let list_key = self.get_thread_list_key(thread_id);
            conn.lrem(list_key, 0, id).await?;
        } else {
            // Delete all checkpoints for thread
            let list_key = self.get_thread_list_key(thread_id);
            let checkpoint_ids: Vec<String> = conn.lrange(&list_key, 0, -1).await?;

            for checkpoint_id in checkpoint_ids {
                let key = self.get_checkpoint_key(thread_id, &checkpoint_id);
                conn.del(key).await?;
            }

            // Delete the list and latest pointer
            conn.del(list_key).await?;
            conn.del(self.get_thread_latest_key(thread_id)).await?;
        }

        Ok(())
    }
}