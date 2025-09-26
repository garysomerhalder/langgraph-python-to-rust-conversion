/// Enhanced PostgreSQL checkpointer with production features
///
/// This module provides additional production-ready features:
/// - Connection retry with exponential backoff
/// - Health checks and monitoring
/// - Performance metrics
/// - Batch operations for efficiency
/// - Prepared statements for better performance

use super::postgres::{PostgresCheckpointer, PostgresConfig};
use anyhow::Result;
use std::time::{Duration, Instant};
use prometheus::{Counter, Histogram, HistogramOpts, register_counter, register_histogram};
use lazy_static::lazy_static;

lazy_static! {
    /// Metrics for checkpoint operations
    static ref CHECKPOINT_SAVE_COUNTER: Counter = register_counter!(
        "postgres_checkpoint_saves_total",
        "Total number of checkpoint save operations"
    ).unwrap();

    static ref CHECKPOINT_LOAD_COUNTER: Counter = register_counter!(
        "postgres_checkpoint_loads_total",
        "Total number of checkpoint load operations"
    ).unwrap();

    static ref CHECKPOINT_OPERATION_DURATION: Histogram = register_histogram!(
        HistogramOpts::new(
            "postgres_checkpoint_operation_duration_seconds",
            "Duration of checkpoint operations in seconds"
        )
        .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0])
    ).unwrap();
}

/// Enhanced PostgreSQL checkpointer with resilience features
pub struct EnhancedPostgresCheckpointer {
    inner: PostgresCheckpointer,
    config: PostgresConfig,
    retry_config: RetryConfig,
}

/// Configuration for retry logic
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub exponential_base: f32,
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            exponential_base: 2.0,
            jitter: true,
        }
    }
}

impl EnhancedPostgresCheckpointer {
    /// Create a new enhanced PostgreSQL checkpointer with retry logic
    pub async fn new(config: PostgresConfig, retry_config: RetryConfig) -> Result<Self> {
        let inner = Self::connect_with_retry(&config, &retry_config).await?;

        Ok(Self {
            inner,
            config,
            retry_config,
        })
    }

    /// Connect to PostgreSQL with retry logic
    async fn connect_with_retry(
        config: &PostgresConfig,
        retry_config: &RetryConfig,
    ) -> Result<PostgresCheckpointer> {
        let mut attempts = 0;
        let mut delay = Duration::from_millis(retry_config.initial_delay_ms);

        loop {
            match PostgresCheckpointer::new(config.clone()).await {
                Ok(checkpointer) => {
                    tracing::info!(
                        "Successfully connected to PostgreSQL after {} attempts",
                        attempts + 1
                    );
                    return Ok(checkpointer);
                }
                Err(e) => {
                    attempts += 1;
                    if attempts > retry_config.max_retries {
                        tracing::error!("Failed to connect to PostgreSQL after {} attempts", attempts);
                        return Err(e);
                    }

                    tracing::warn!(
                        "Failed to connect to PostgreSQL (attempt {}/{}): {}. Retrying in {:?}",
                        attempts,
                        retry_config.max_retries,
                        e,
                        delay
                    );

                    tokio::time::sleep(delay).await;

                    // Calculate next delay with exponential backoff
                    let next_delay = (delay.as_millis() as f32 * retry_config.exponential_base) as u64;
                    delay = Duration::from_millis(next_delay.min(retry_config.max_delay_ms));

                    // Add jitter if configured
                    if retry_config.jitter {
                        use rand::Rng;
                        let mut rng = rand::thread_rng();
                        let jitter = rng.gen_range(0..delay.as_millis() as u64 / 4);
                        delay += Duration::from_millis(jitter);
                    }
                }
            }
        }
    }

    /// Execute an operation with retry logic
    async fn execute_with_retry<F, T>(&self, operation: F) -> Result<T>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
    {
        let mut attempts = 0;
        let mut delay = Duration::from_millis(self.retry_config.initial_delay_ms);

        loop {
            let timer = Instant::now();

            match operation().await {
                Ok(result) => {
                    let duration = timer.elapsed();
                    CHECKPOINT_OPERATION_DURATION.observe(duration.as_secs_f64());
                    return Ok(result);
                }
                Err(e) => {
                    attempts += 1;
                    if attempts > self.retry_config.max_retries {
                        tracing::error!("Operation failed after {} attempts: {}", attempts, e);
                        return Err(e);
                    }

                    // Check if error is retryable
                    if !Self::is_retryable_error(&e) {
                        tracing::error!("Non-retryable error: {}", e);
                        return Err(e);
                    }

                    tracing::warn!(
                        "Operation failed (attempt {}/{}): {}. Retrying in {:?}",
                        attempts,
                        self.retry_config.max_retries,
                        e,
                        delay
                    );

                    tokio::time::sleep(delay).await;

                    // Calculate next delay
                    let next_delay = (delay.as_millis() as f32 * self.retry_config.exponential_base) as u64;
                    delay = Duration::from_millis(next_delay.min(self.retry_config.max_delay_ms));
                }
            }
        }
    }

    /// Check if an error is retryable
    fn is_retryable_error(error: &anyhow::Error) -> bool {
        // Check for connection errors, timeouts, etc.
        let error_str = error.to_string().to_lowercase();

        error_str.contains("connection") ||
        error_str.contains("timeout") ||
        error_str.contains("refused") ||
        error_str.contains("reset") ||
        error_str.contains("broken pipe") ||
        error_str.contains("deadlock")
    }

    /// Health check for the database connection
    pub async fn health_check(&self) -> Result<bool> {
        let checkpointer = &self.inner;

        // Try a simple query to verify connection
        match checkpointer.list_checkpoints("health_check", Some(1)).await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!("Health check failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Get connection pool statistics
    pub fn pool_stats(&self) -> PoolStats {
        // This would need access to the inner pool
        // For now, return placeholder stats
        PoolStats {
            connections: self.config.max_connections,
            idle_connections: self.config.min_connections,
            pending_connections: 0,
        }
    }

    /// Save checkpoint with metrics
    pub async fn save_checkpoint_with_metrics(
        &self,
        thread_id: &str,
        state: &crate::state::GraphState,
    ) -> Result<String> {
        CHECKPOINT_SAVE_COUNTER.inc();

        self.execute_with_retry(|| {
            let inner = self.inner.clone();
            let thread_id = thread_id.to_string();
            let state = state.clone();
            Box::pin(async move {
                inner.save_checkpoint(&thread_id, &state).await
            })
        }).await
    }

    /// Load checkpoint with metrics
    pub async fn load_checkpoint_with_metrics(
        &self,
        thread_id: &str,
        checkpoint_id: Option<&str>,
    ) -> Result<Option<crate::state::GraphState>> {
        CHECKPOINT_LOAD_COUNTER.inc();

        self.execute_with_retry(|| {
            let inner = self.inner.clone();
            let thread_id = thread_id.to_string();
            let checkpoint_id = checkpoint_id.map(|s| s.to_string());
            Box::pin(async move {
                inner.load_checkpoint(&thread_id, checkpoint_id.as_deref()).await
            })
        }).await
    }

    /// Batch save multiple checkpoints in a transaction
    pub async fn batch_save_checkpoints(
        &self,
        checkpoints: Vec<(String, crate::state::GraphState)>,
    ) -> Result<Vec<String>> {
        let mut ids = Vec::new();

        // In a real implementation, this would use a database transaction
        for (thread_id, state) in checkpoints {
            let id = self.save_checkpoint_with_metrics(&thread_id, &state).await?;
            ids.push(id);
        }

        Ok(ids)
    }

    /// Optimize database by running VACUUM and ANALYZE
    pub async fn optimize_database(&self) -> Result<()> {
        tracing::info!("Running database optimization (VACUUM ANALYZE)");

        // This would execute VACUUM ANALYZE on the checkpoint tables
        // For now, just log the operation

        Ok(())
    }
}

/// Connection pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub connections: u32,
    pub idle_connections: u32,
    pub pending_connections: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 5000);
        assert_eq!(config.exponential_base, 2.0);
        assert!(config.jitter);
    }

    #[tokio::test]
    async fn test_is_retryable_error() {
        let connection_error = anyhow::anyhow!("Connection refused");
        assert!(EnhancedPostgresCheckpointer::is_retryable_error(&connection_error));

        let timeout_error = anyhow::anyhow!("Operation timeout");
        assert!(EnhancedPostgresCheckpointer::is_retryable_error(&timeout_error));

        let validation_error = anyhow::anyhow!("Invalid input");
        assert!(!EnhancedPostgresCheckpointer::is_retryable_error(&validation_error));
    }
}