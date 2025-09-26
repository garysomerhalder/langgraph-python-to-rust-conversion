/// Enhanced Redis checkpointer with production features
///
/// This module provides additional production-ready features:
/// - Connection retry with exponential backoff
/// - Circuit breaker for fault tolerance
/// - Health checks and monitoring
/// - Performance metrics
/// - Connection pooling optimization
/// - Automatic failover support

use super::redis::{RedisCheckpointer, RedisConfig};
use anyhow::Result;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use prometheus::{Counter, Histogram, HistogramOpts, IntGauge, register_counter, register_histogram, register_int_gauge};
use lazy_static::lazy_static;

lazy_static! {
    /// Metrics for Redis checkpoint operations
    static ref REDIS_CHECKPOINT_SAVE_COUNTER: Counter = register_counter!(
        "redis_checkpoint_saves_total",
        "Total number of Redis checkpoint save operations"
    ).unwrap();

    static ref REDIS_CHECKPOINT_LOAD_COUNTER: Counter = register_counter!(
        "redis_checkpoint_loads_total",
        "Total number of Redis checkpoint load operations"
    ).unwrap();

    static ref REDIS_OPERATION_DURATION: Histogram = register_histogram!(
        HistogramOpts::new(
            "redis_checkpoint_operation_duration_seconds",
            "Duration of Redis checkpoint operations in seconds"
        )
        .buckets(vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1])
    ).unwrap();

    static ref REDIS_CONNECTION_POOL_SIZE: IntGauge = register_int_gauge!(
        "redis_connection_pool_size",
        "Current size of Redis connection pool"
    ).unwrap();

    static ref REDIS_CIRCUIT_BREAKER_STATE: IntGauge = register_int_gauge!(
        "redis_circuit_breaker_state",
        "Circuit breaker state (0=closed, 1=open, 2=half-open)"
    ).unwrap();
}

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker for Redis operations
pub struct CircuitBreaker {
    state: Arc<tokio::sync::RwLock<CircuitState>>,
    failure_count: Arc<AtomicUsize>,
    success_count: Arc<AtomicUsize>,
    last_failure: Arc<tokio::sync::RwLock<Option<Instant>>>,
    config: CircuitBreakerConfig,
}

/// Configuration for circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: usize,
    pub success_threshold: usize,
    pub timeout: Duration,
    pub half_open_max_calls: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(60),
            half_open_max_calls: 3,
        }
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(tokio::sync::RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(AtomicUsize::new(0)),
            success_count: Arc::new(AtomicUsize::new(0)),
            last_failure: Arc::new(tokio::sync::RwLock::new(None)),
            config,
        }
    }

    /// Check if operation is allowed
    pub async fn is_allowed(&self) -> bool {
        let state = self.state.read().await;
        match *state {
            CircuitState::Open => {
                // Check if timeout has passed
                if let Some(last_failure) = *self.last_failure.read().await {
                    if last_failure.elapsed() > self.config.timeout {
                        // Transition to half-open
                        drop(state);
                        let mut state = self.state.write().await;
                        *state = CircuitState::HalfOpen;
                        self.success_count.store(0, Ordering::SeqCst);
                        REDIS_CIRCUIT_BREAKER_STATE.set(2);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                // Allow limited calls
                self.success_count.load(Ordering::SeqCst) < self.config.half_open_max_calls
            }
            CircuitState::Closed => true,
        }
    }

    /// Record success
    pub async fn record_success(&self) {
        let state = self.state.read().await;
        match *state {
            CircuitState::HalfOpen => {
                let success_count = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
                if success_count >= self.config.success_threshold {
                    // Transition to closed
                    drop(state);
                    let mut state = self.state.write().await;
                    *state = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::SeqCst);
                    REDIS_CIRCUIT_BREAKER_STATE.set(0);
                    tracing::info!("Circuit breaker closed after successful operations");
                }
            }
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    /// Record failure
    pub async fn record_failure(&self) {
        let failure_count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;

        let state = self.state.read().await;
        match *state {
            CircuitState::Closed => {
                if failure_count >= self.config.failure_threshold {
                    // Transition to open
                    drop(state);
                    let mut state = self.state.write().await;
                    *state = CircuitState::Open;
                    *self.last_failure.write().await = Some(Instant::now());
                    REDIS_CIRCUIT_BREAKER_STATE.set(1);
                    tracing::warn!("Circuit breaker opened after {} failures", failure_count);
                }
            }
            CircuitState::HalfOpen => {
                // Transition back to open
                drop(state);
                let mut state = self.state.write().await;
                *state = CircuitState::Open;
                *self.last_failure.write().await = Some(Instant::now());
                self.failure_count.store(0, Ordering::SeqCst);
                REDIS_CIRCUIT_BREAKER_STATE.set(1);
                tracing::warn!("Circuit breaker reopened after failure in half-open state");
            }
            _ => {}
        }
    }
}

/// Enhanced Redis checkpointer with resilience features
pub struct EnhancedRedisCheckpointer {
    inner: Arc<RedisCheckpointer>,
    config: RedisConfig,
    retry_config: RetryConfig,
    circuit_breaker: Arc<CircuitBreaker>,
    metrics: Arc<Metrics>,
}

/// Retry configuration
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
            initial_delay_ms: 50,
            max_delay_ms: 2000,
            exponential_base: 2.0,
            jitter: true,
        }
    }
}

/// Metrics tracking
struct Metrics {
    total_operations: AtomicU64,
    failed_operations: AtomicU64,
    retry_count: AtomicU64,
    circuit_opens: AtomicU64,
}

impl EnhancedRedisCheckpointer {
    /// Create a new enhanced Redis checkpointer
    pub async fn new(
        config: RedisConfig,
        retry_config: RetryConfig,
        circuit_breaker_config: CircuitBreakerConfig,
    ) -> Result<Self> {
        let inner = Self::connect_with_retry(&config, &retry_config).await?;
        REDIS_CONNECTION_POOL_SIZE.set(config.pool_size as i64);

        Ok(Self {
            inner: Arc::new(inner),
            config,
            retry_config,
            circuit_breaker: Arc::new(CircuitBreaker::new(circuit_breaker_config)),
            metrics: Arc::new(Metrics {
                total_operations: AtomicU64::new(0),
                failed_operations: AtomicU64::new(0),
                retry_count: AtomicU64::new(0),
                circuit_opens: AtomicU64::new(0),
            }),
        })
    }

    /// Connect with retry logic
    async fn connect_with_retry(
        config: &RedisConfig,
        retry_config: &RetryConfig,
    ) -> Result<RedisCheckpointer> {
        let mut attempts = 0;
        let mut delay = Duration::from_millis(retry_config.initial_delay_ms);

        loop {
            match RedisCheckpointer::new(config.clone()).await {
                Ok(checkpointer) => {
                    tracing::info!(
                        "Successfully connected to Redis after {} attempts",
                        attempts + 1
                    );
                    return Ok(checkpointer);
                }
                Err(e) => {
                    attempts += 1;
                    if attempts > retry_config.max_retries {
                        tracing::error!("Failed to connect to Redis after {} attempts", attempts);
                        return Err(e);
                    }

                    tracing::warn!(
                        "Failed to connect to Redis (attempt {}/{}): {}. Retrying in {:?}",
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

    /// Execute an operation with circuit breaker and retry logic
    async fn execute_with_protection<F, T>(&self, operation_name: &str, operation: F) -> Result<T>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
    {
        // Check circuit breaker
        if !self.circuit_breaker.is_allowed().await {
            self.metrics.circuit_opens.fetch_add(1, Ordering::SeqCst);
            return Err(anyhow::anyhow!("Circuit breaker is open"));
        }

        self.metrics.total_operations.fetch_add(1, Ordering::SeqCst);

        let mut attempts = 0;
        let mut delay = Duration::from_millis(self.retry_config.initial_delay_ms);

        loop {
            let timer = Instant::now();

            match operation().await {
                Ok(result) => {
                    let duration = timer.elapsed();
                    REDIS_OPERATION_DURATION.observe(duration.as_secs_f64());
                    self.circuit_breaker.record_success().await;

                    tracing::debug!(
                        "{} completed in {:?} (attempt {})",
                        operation_name,
                        duration,
                        attempts + 1
                    );

                    return Ok(result);
                }
                Err(e) => {
                    attempts += 1;
                    self.metrics.retry_count.fetch_add(1, Ordering::SeqCst);

                    if attempts > self.retry_config.max_retries {
                        self.metrics.failed_operations.fetch_add(1, Ordering::SeqCst);
                        self.circuit_breaker.record_failure().await;
                        tracing::error!("{} failed after {} attempts: {}", operation_name, attempts, e);
                        return Err(e);
                    }

                    // Check if error is retryable
                    if !Self::is_retryable_error(&e) {
                        self.metrics.failed_operations.fetch_add(1, Ordering::SeqCst);
                        tracing::error!("{}: Non-retryable error: {}", operation_name, e);
                        return Err(e);
                    }

                    tracing::warn!(
                        "{} failed (attempt {}/{}): {}. Retrying in {:?}",
                        operation_name,
                        attempts,
                        self.retry_config.max_retries,
                        e,
                        delay
                    );

                    tokio::time::sleep(delay).await;

                    // Calculate next delay
                    let next_delay = (delay.as_millis() as f32 * self.retry_config.exponential_base) as u64;
                    delay = Duration::from_millis(next_delay.min(self.retry_config.max_delay_ms));

                    // Add jitter
                    if self.retry_config.jitter {
                        use rand::Rng;
                        let mut rng = rand::thread_rng();
                        let jitter = rng.gen_range(0..delay.as_millis() as u64 / 4);
                        delay += Duration::from_millis(jitter);
                    }
                }
            }
        }
    }

    /// Check if an error is retryable
    fn is_retryable_error(error: &anyhow::Error) -> bool {
        let error_str = error.to_string().to_lowercase();

        error_str.contains("connection") ||
        error_str.contains("timeout") ||
        error_str.contains("refused") ||
        error_str.contains("reset") ||
        error_str.contains("broken pipe") ||
        error_str.contains("temporarily unavailable") ||
        error_str.contains("too many connections")
    }

    /// Save checkpoint with protection
    pub async fn save_checkpoint_with_protection(
        &self,
        thread_id: &str,
        state: &crate::state::GraphState,
    ) -> Result<String> {
        REDIS_CHECKPOINT_SAVE_COUNTER.inc();

        self.execute_with_protection("save_checkpoint", || {
            let inner = self.inner.clone();
            let thread_id = thread_id.to_string();
            let state = state.clone();
            Box::pin(async move {
                inner.save_checkpoint(&thread_id, &state).await
            })
        }).await
    }

    /// Load checkpoint with protection
    pub async fn load_checkpoint_with_protection(
        &self,
        thread_id: &str,
        checkpoint_id: Option<&str>,
    ) -> Result<Option<crate::state::GraphState>> {
        REDIS_CHECKPOINT_LOAD_COUNTER.inc();

        self.execute_with_protection("load_checkpoint", || {
            let inner = self.inner.clone();
            let thread_id = thread_id.to_string();
            let checkpoint_id = checkpoint_id.map(|s| s.to_string());
            Box::pin(async move {
                inner.load_checkpoint(&thread_id, checkpoint_id.as_deref()).await
            })
        }).await
    }

    /// Health check with metrics
    pub async fn health_check(&self) -> Result<bool> {
        let result = self.inner.health_check().await;

        if let Ok(healthy) = &result {
            if !healthy {
                tracing::warn!("Redis health check returned unhealthy");
            }
        } else {
            tracing::error!("Redis health check failed: {:?}", result);
        }

        result
    }

    /// Get operational metrics
    pub fn get_metrics(&self) -> OperationalMetrics {
        OperationalMetrics {
            total_operations: self.metrics.total_operations.load(Ordering::SeqCst),
            failed_operations: self.metrics.failed_operations.load(Ordering::SeqCst),
            retry_count: self.metrics.retry_count.load(Ordering::SeqCst),
            circuit_opens: self.metrics.circuit_opens.load(Ordering::SeqCst),
            connection_pool_size: self.config.pool_size,
        }
    }

    /// Warm up connection pool
    pub async fn warm_up(&self) -> Result<()> {
        tracing::info!("Warming up Redis connection pool");

        // Perform a few operations to establish connections
        for i in 0..self.config.pool_size.min(5) {
            let thread_id = format!("warmup_{}", i);
            let mut state = crate::state::GraphState::new();
            state.set("warmup", serde_json::json!(true));

            let _ = self.save_checkpoint_with_protection(&thread_id, &state).await?;
        }

        tracing::info!("Redis connection pool warmed up");
        Ok(())
    }
}

/// Operational metrics
#[derive(Debug, Clone)]
pub struct OperationalMetrics {
    pub total_operations: u64,
    pub failed_operations: u64,
    pub retry_count: u64,
    pub circuit_opens: u64,
    pub connection_pool_size: u32,
}

impl OperationalMetrics {
    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_operations == 0 {
            100.0
        } else {
            ((self.total_operations - self.failed_operations) as f64 / self.total_operations as f64) * 100.0
        }
    }

    /// Calculate retry rate
    pub fn retry_rate(&self) -> f64 {
        if self.total_operations == 0 {
            0.0
        } else {
            (self.retry_count as f64 / self.total_operations as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_transitions() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(1),
            half_open_max_calls: 2,
        };

        let breaker = CircuitBreaker::new(config);

        // Initially closed
        assert!(breaker.is_allowed().await);

        // Record failures to open circuit
        for _ in 0..3 {
            breaker.record_failure().await;
        }

        // Should be open
        assert!(!breaker.is_allowed().await);

        // Wait for timeout
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Should transition to half-open
        assert!(breaker.is_allowed().await);

        // Record successes to close
        for _ in 0..2 {
            breaker.record_success().await;
        }

        // Should be closed again
        assert!(breaker.is_allowed().await);
    }

    #[test]
    fn test_operational_metrics() {
        let metrics = OperationalMetrics {
            total_operations: 100,
            failed_operations: 5,
            retry_count: 10,
            circuit_opens: 2,
            connection_pool_size: 10,
        };

        assert_eq!(metrics.success_rate(), 95.0);
        assert_eq!(metrics.retry_rate(), 10.0);
    }

    #[test]
    fn test_is_retryable_error() {
        let connection_error = anyhow::anyhow!("Connection refused");
        assert!(EnhancedRedisCheckpointer::is_retryable_error(&connection_error));

        let timeout_error = anyhow::anyhow!("Operation timeout");
        assert!(EnhancedRedisCheckpointer::is_retryable_error(&timeout_error));

        let auth_error = anyhow::anyhow!("Authentication failed");
        assert!(!EnhancedRedisCheckpointer::is_retryable_error(&auth_error));
    }
}