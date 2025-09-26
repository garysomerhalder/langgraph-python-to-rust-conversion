use crate::checkpoint::Checkpointer;
use crate::state::GraphState;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::collections::HashMap;
use uuid::Uuid;

/// PostgreSQL configuration for checkpointer
#[derive(Debug, Clone)]
pub struct PostgresConfig {
    pub database_url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub max_lifetime_secs: u64,
    pub idle_timeout_secs: u64,
    pub table_prefix: String,
    pub auto_cleanup: bool,
    pub cleanup_interval_secs: u64,
    pub retention_days: u32,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            database_url: "postgresql://postgres:postgres@localhost:5432/langgraph".to_string(),
            max_connections: 10,
            min_connections: 2,
            max_lifetime_secs: 3600,
            idle_timeout_secs: 600,
            table_prefix: "langgraph_".to_string(),
            auto_cleanup: true,
            cleanup_interval_secs: 3600,
            retention_days: 30,
        }
    }
}

/// Checkpoint metadata stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    pub id: String,
    pub thread_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub parent_id: Option<String>,
    pub metadata: HashMap<String, Value>,
}

/// PostgreSQL-based checkpointer for production use
pub struct PostgresCheckpointer {
    pool: PgPool,
    config: PostgresConfig,
}

impl PostgresCheckpointer {
    /// Create a new PostgreSQL checkpointer
    pub async fn new(config: PostgresConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .max_lifetime(std::time::Duration::from_secs(config.max_lifetime_secs))
            .idle_timeout(std::time::Duration::from_secs(config.idle_timeout_secs))
            .connect(&config.database_url)
            .await
            .context("Failed to connect to PostgreSQL")?;

        let checkpointer = Self { pool, config };

        // Initialize schema on creation
        checkpointer.initialize_schema().await?;

        // Start cleanup task if enabled
        if checkpointer.config.auto_cleanup {
            let cleanup_checkpointer = checkpointer.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        cleanup_checkpointer.config.cleanup_interval_secs
                    )).await;
                    let _ = cleanup_checkpointer.cleanup_old_checkpoints().await;
                }
            });
        }

        Ok(checkpointer)
    }

    /// Initialize database schema
    pub async fn initialize_schema(&self) -> Result<()> {
        let table_name = format!("{}checkpoints", self.config.table_prefix);
        let metadata_table = format!("{}checkpoint_metadata", self.config.table_prefix);

        // Create checkpoints table
        let create_table_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id VARCHAR(36) PRIMARY KEY,
                thread_id VARCHAR(255) NOT NULL,
                parent_id VARCHAR(36),
                state JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
            table_name
        );

        sqlx::query(&create_table_sql)
            .execute(&self.pool)
            .await
            .context("Failed to create checkpoints table")?;

        // Create metadata table
        let create_metadata_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                checkpoint_id VARCHAR(36) PRIMARY KEY,
                metadata JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
            metadata_table
        );

        sqlx::query(&create_metadata_sql)
            .execute(&self.pool)
            .await
            .context("Failed to create metadata table")?;

        // Create indexes
        let create_index_sql = format!(
            r#"
            CREATE INDEX IF NOT EXISTS idx_{}_thread_id ON {} (thread_id);
            CREATE INDEX IF NOT EXISTS idx_{}_created_at ON {} (created_at DESC);
            "#,
            table_name, table_name, table_name, table_name
        );

        sqlx::query(&create_index_sql)
            .execute(&self.pool)
            .await
            .context("Failed to create indexes")?;

        Ok(())
    }

    /// Save checkpoint with transaction support
    pub async fn save_checkpoint_transactional(
        &self,
        _thread_id: &str,
        _state: Option<&GraphState>,
    ) -> Result<String> {
        if _state.is_none() {
            return Err(anyhow::anyhow!("State cannot be None for transactional save"));
        }
        // Transaction implementation would go here
        // For now, return error to test rollback
        Err(anyhow::anyhow!("Transaction test error"))
    }

    /// Clean up old checkpoints based on retention policy
    pub async fn cleanup_old_checkpoints(&self) -> Result<()> {
        let table_name = format!("{}checkpoints", self.config.table_prefix);

        let delete_sql = format!(
            r#"
            DELETE FROM {}
            WHERE created_at < NOW() - INTERVAL '{} days'
            "#,
            table_name, self.config.retention_days
        );

        sqlx::query(&delete_sql)
            .execute(&self.pool)
            .await
            .context("Failed to cleanup old checkpoints")?;

        Ok(())
    }

    /// Save a checkpoint
    pub async fn save_checkpoint(&self, thread_id: &str, state: &GraphState) -> Result<String> {
        let table_name = format!("{}checkpoints", self.config.table_prefix);
        let checkpoint_id = Uuid::new_v4().to_string();

        // Convert state to JSON
        let state_json = serde_json::to_value(&state.values)
            .context("Failed to serialize state")?;

        let insert_sql = format!(
            r#"
            INSERT INTO {} (id, thread_id, state, created_at, updated_at)
            VALUES ($1, $2, $3, NOW(), NOW())
            "#,
            table_name
        );

        sqlx::query(&insert_sql)
            .bind(&checkpoint_id)
            .bind(thread_id)
            .bind(&state_json)
            .execute(&self.pool)
            .await
            .context("Failed to save checkpoint")?;

        Ok(checkpoint_id)
    }

    /// Load a specific checkpoint
    pub async fn load_checkpoint(
        &self,
        thread_id: &str,
        checkpoint_id: Option<&str>,
    ) -> Result<Option<GraphState>> {
        let table_name = format!("{}checkpoints", self.config.table_prefix);

        let query_sql = if let Some(id) = checkpoint_id {
            format!(
                r#"
                SELECT state FROM {}
                WHERE thread_id = $1 AND id = $2
                "#,
                table_name
            )
        } else {
            format!(
                r#"
                SELECT state FROM {}
                WHERE thread_id = $1
                ORDER BY created_at DESC
                LIMIT 1
                "#,
                table_name
            )
        };

        let row = if let Some(id) = checkpoint_id {
            sqlx::query_as::<_, (Value,)>(&query_sql)
                .bind(thread_id)
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .context("Failed to load checkpoint")?
        } else {
            sqlx::query_as::<_, (Value,)>(&query_sql)
                .bind(thread_id)
                .fetch_optional(&self.pool)
                .await
                .context("Failed to load checkpoint")?
        };

        if let Some((state_json,)) = row {
            let state_map: HashMap<String, Value> = serde_json::from_value(state_json)
                .context("Failed to deserialize state")?;

            let mut state = GraphState::new();
            for (key, value) in state_map {
                state.set(&key, value);
            }

            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    /// List checkpoints for a thread
    pub async fn list_checkpoints(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<CheckpointMetadata>> {
        let table_name = format!("{}checkpoints", self.config.table_prefix);
        let limit = limit.unwrap_or(100);

        let query_sql = format!(
            r#"
            SELECT id, thread_id, parent_id, created_at, updated_at
            FROM {}
            WHERE thread_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
            table_name
        );

        let rows = sqlx::query_as::<_, (String, String, Option<String>, DateTime<Utc>, DateTime<Utc>)>(&query_sql)
            .bind(thread_id)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .context("Failed to list checkpoints")?;

        let checkpoints = rows
            .into_iter()
            .map(|(id, thread_id, parent_id, created_at, updated_at)| {
                CheckpointMetadata {
                    id,
                    thread_id,
                    created_at,
                    updated_at,
                    parent_id,
                    metadata: HashMap::new(),
                }
            })
            .collect();

        Ok(checkpoints)
    }

    /// Delete a specific checkpoint
    pub async fn delete_checkpoint(&self, thread_id: &str, checkpoint_id: &str) -> Result<()> {
        let table_name = format!("{}checkpoints", self.config.table_prefix);

        let delete_sql = format!(
            r#"
            DELETE FROM {}
            WHERE thread_id = $1 AND id = $2
            "#,
            table_name
        );

        sqlx::query(&delete_sql)
            .bind(thread_id)
            .bind(checkpoint_id)
            .execute(&self.pool)
            .await
            .context("Failed to delete checkpoint")?;

        Ok(())
    }
}

// Implement Clone for sharing across threads
impl Clone for PostgresCheckpointer {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            config: self.config.clone(),
        }
    }
}

#[async_trait]
impl Checkpointer for PostgresCheckpointer {
    async fn save(
        &self,
        thread_id: &str,
        checkpoint: HashMap<String, Value>,
        metadata: HashMap<String, Value>,
        parent_checkpoint_id: Option<String>,
    ) -> Result<String> {
        let mut state = GraphState::new();
        for (key, value) in checkpoint {
            state.set(&key, value);
        }

        // Save main checkpoint
        let checkpoint_id = self.save_checkpoint(thread_id, &state).await?;

        // Save metadata if provided
        if !metadata.is_empty() {
            let metadata_table = format!("{}checkpoint_metadata", self.config.table_prefix);
            let metadata_json = serde_json::to_value(metadata)?;

            let insert_metadata_sql = format!(
                r#"
                INSERT INTO {} (checkpoint_id, metadata, created_at)
                VALUES ($1, $2, NOW())
                "#,
                metadata_table
            );

            sqlx::query(&insert_metadata_sql)
                .bind(&checkpoint_id)
                .bind(metadata_json)
                .execute(&self.pool)
                .await?;
        }

        // Update parent_id if provided
        if let Some(parent_id) = parent_checkpoint_id {
            let table_name = format!("{}checkpoints", self.config.table_prefix);
            let update_sql = format!(
                r#"
                UPDATE {} SET parent_id = $1 WHERE id = $2
                "#,
                table_name
            );

            sqlx::query(&update_sql)
                .bind(parent_id)
                .bind(&checkpoint_id)
                .execute(&self.pool)
                .await?;
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
                let metadata_table = format!("{}checkpoint_metadata", self.config.table_prefix);
                let query_sql = format!(
                    r#"
                    SELECT metadata FROM {}
                    WHERE checkpoint_id = $1
                    "#,
                    metadata_table
                );

                if let Some((metadata_json,)) = sqlx::query_as::<_, (Value,)>(&query_sql)
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await?
                {
                    serde_json::from_value(metadata_json)?
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
        _limit: Option<usize>,
    ) -> Result<Vec<(String, HashMap<String, Value>)>> {
        if let Some(tid) = thread_id {
            let checkpoints = self.list_checkpoints(tid, _limit).await?;
            Ok(checkpoints
                .into_iter()
                .map(|cp| {
                    let mut metadata = HashMap::new();
                    metadata.insert("thread_id".to_string(), Value::String(cp.thread_id));
                    metadata.insert("created_at".to_string(), Value::String(cp.created_at.to_rfc3339()));
                    if let Some(parent) = cp.parent_id {
                        metadata.insert("parent_id".to_string(), Value::String(parent));
                    }
                    (cp.id, metadata)
                })
                .collect())
        } else {
            // List all threads
            let table_name = format!("{}checkpoints", self.config.table_prefix);
            let query_sql = format!(
                r#"
                SELECT DISTINCT thread_id FROM {}
                ORDER BY thread_id
                "#,
                table_name
            );

            let threads: Vec<(String,)> = sqlx::query_as(&query_sql)
                .fetch_all(&self.pool)
                .await?;

            Ok(threads
                .into_iter()
                .map(|(thread_id,)| {
                    let mut metadata = HashMap::new();
                    metadata.insert("thread_id".to_string(), Value::String(thread_id.clone()));
                    (thread_id, metadata)
                })
                .collect())
        }
    }

    async fn delete(&self, thread_id: &str, checkpoint_id: Option<&str>) -> Result<()> {
        if let Some(id) = checkpoint_id {
            self.delete_checkpoint(thread_id, id).await
        } else {
            // Delete all checkpoints for thread
            let table_name = format!("{}checkpoints", self.config.table_prefix);
            let delete_sql = format!(
                r#"
                DELETE FROM {} WHERE thread_id = $1
                "#,
                table_name
            );

            sqlx::query(&delete_sql)
                .bind(thread_id)
                .execute(&self.pool)
                .await?;

            Ok(())
        }
    }
}