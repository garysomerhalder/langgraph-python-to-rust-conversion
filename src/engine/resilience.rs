//! Resilience patterns for robust execution
//! Implements circuit breaker, retry logic, and error recovery

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use async_trait::async_trait;
use thiserror::Error;
use tracing::{debug, error, info, warn, instrument};
use std::collections::VecDeque;

/// Errors that can occur in resilience operations
#[derive(Error, Debug, Clone)]
pub enum ResilienceError {
    #[error("Circuit breaker is open")]
    CircuitBreakerOpen,
    
    #[error("Maximum retry attempts exceeded")]
    MaxRetriesExceeded,
    
    #[error("Operation timeout")]
    OperationTimeout,
    
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    /// Circuit is closed, allowing requests
    Closed,
    /// Circuit is open, blocking requests
    Open,
    /// Circuit is half-open, allowing limited requests for testing
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: usize,
    /// Duration to keep circuit open
    pub timeout_duration: Duration,
    /// Number of successes needed in half-open state
    pub success_threshold: usize,
    /// Time window for tracking failures
    pub failure_window: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout_duration: Duration::from_secs(30),
            success_threshold: 3,
            failure_window: Duration::from_secs(60),
        }
    }
}

/// Circuit breaker implementation
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    config: CircuitBreakerConfig,
    failure_count: Arc<RwLock<usize>>,
    success_count: Arc<RwLock<usize>>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    state_changed_at: Arc<RwLock<Instant>>,
    failure_timestamps: Arc<RwLock<VecDeque<Instant>>>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            config,
            failure_count: Arc::new(RwLock::new(0)),
            success_count: Arc::new(RwLock::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            state_changed_at: Arc::new(RwLock::new(Instant::now())),
            failure_timestamps: Arc::new(RwLock::new(VecDeque::new())),
        }
    }
    
    /// Execute an operation with circuit breaker protection
    #[instrument(skip(self, operation))]
    pub async fn execute<F, T, E>(&self, operation: F) -> Result<T, ResilienceError>
    where
        F: Fn() -> Result<T, E>,
        E: std::error::Error,
    {
        // Check circuit state
        let current_state = self.state.read().await.clone();
        
        match current_state {
            CircuitState::Open => {
                // Check if we should transition to half-open
                let state_changed_at = *self.state_changed_at.read().await;
                if state_changed_at.elapsed() >= self.config.timeout_duration {
                    self.transition_to_half_open().await;
                } else {
                    return Err(ResilienceError::CircuitBreakerOpen);
                }
            }
            CircuitState::HalfOpen => {
                debug!("Circuit breaker in half-open state, allowing limited traffic");
            }
            CircuitState::Closed => {
                // Clean old failure timestamps
                self.clean_old_failures().await;
            }
        }
        
        // Execute the operation
        match operation() {
            Ok(result) => {
                self.record_success().await;
                Ok(result)
            }
            Err(error) => {
                error!("Operation failed: {}", error);
                self.record_failure().await;
                Err(ResilienceError::CircuitBreakerOpen)
            }
        }
    }
    
    /// Record a successful operation
    pub async fn record_success(&self) {
        let mut success_count = self.success_count.write().await;
        *success_count += 1;
        
        let current_state = self.state.read().await.clone();
        
        if current_state == CircuitState::HalfOpen {
            if *success_count >= self.config.success_threshold {
                self.transition_to_closed().await;
            }
        }
    }
    
    /// Record a failed operation
    pub async fn record_failure(&self) {
        let now = Instant::now();
        
        // Add failure timestamp
        let mut timestamps = self.failure_timestamps.write().await;
        timestamps.push_back(now);
        
        // Count recent failures
        let recent_failures = timestamps.iter()
            .filter(|t| now.duration_since(**t) < self.config.failure_window)
            .count();
        
        let mut failure_count = self.failure_count.write().await;
        *failure_count = recent_failures;
        
        let current_state = self.state.read().await.clone();
        
        match current_state {
            CircuitState::Closed => {
                if *failure_count >= self.config.failure_threshold {
                    self.transition_to_open().await;
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open state reopens the circuit
                self.transition_to_open().await;
            }
            _ => {}
        }
        
        let mut last_failure = self.last_failure_time.write().await;
        *last_failure = Some(now);
    }
    
    /// Clean old failure timestamps
    async fn clean_old_failures(&self) {
        let now = Instant::now();
        let mut timestamps = self.failure_timestamps.write().await;
        
        while let Some(front) = timestamps.front() {
            if now.duration_since(*front) > self.config.failure_window {
                timestamps.pop_front();
            } else {
                break;
            }
        }
    }
    
    /// Transition to open state
    async fn transition_to_open(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::Open;
        
        let mut state_changed_at = self.state_changed_at.write().await;
        *state_changed_at = Instant::now();
        
        warn!("Circuit breaker opened due to excessive failures");
    }
    
    /// Transition to half-open state
    async fn transition_to_half_open(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::HalfOpen;
        
        let mut state_changed_at = self.state_changed_at.write().await;
        *state_changed_at = Instant::now();
        
        let mut success_count = self.success_count.write().await;
        *success_count = 0;
        
        info!("Circuit breaker transitioned to half-open state");
    }
    
    /// Transition to closed state
    async fn transition_to_closed(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::Closed;
        
        let mut state_changed_at = self.state_changed_at.write().await;
        *state_changed_at = Instant::now();
        
        let mut failure_count = self.failure_count.write().await;
        *failure_count = 0;
        
        let mut success_count = self.success_count.write().await;
        *success_count = 0;
        
        info!("Circuit breaker closed, normal operation resumed");
    }
    
    /// Get current circuit state
    pub async fn get_state(&self) -> CircuitState {
        self.state.read().await.clone()
    }
    
    /// Get circuit metrics
    pub async fn get_metrics(&self) -> CircuitMetrics {
        CircuitMetrics {
            state: self.state.read().await.clone(),
            failure_count: *self.failure_count.read().await,
            success_count: *self.success_count.read().await,
            last_failure_time: *self.last_failure_time.read().await,
            state_duration: self.state_changed_at.read().await.elapsed(),
        }
    }
}

/// Circuit breaker metrics
#[derive(Debug, Clone)]
pub struct CircuitMetrics {
    pub state: CircuitState,
    pub failure_count: usize,
    pub success_count: usize,
    pub last_failure_time: Option<Instant>,
    pub state_duration: Duration,
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: usize,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Add jitter to prevent thundering herd
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

/// Retry executor
pub struct RetryExecutor {
    config: RetryConfig,
}

impl RetryExecutor {
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }
    
    /// Execute an operation with retry logic
    #[instrument(skip(self, operation))]
    pub async fn execute<F, Fut, T, E>(&self, operation: F) -> Result<T, ResilienceError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::error::Error,
    {
        let mut attempt = 0;
        let mut delay = self.config.initial_delay;
        
        loop {
            attempt += 1;
            
            match operation().await {
                Ok(result) => {
                    if attempt > 1 {
                        info!("Operation succeeded after {} attempts", attempt);
                    }
                    return Ok(result);
                }
                Err(error) => {
                    if attempt >= self.config.max_attempts {
                        error!("Max retry attempts exceeded: {}", error);
                        return Err(ResilienceError::MaxRetriesExceeded);
                    }
                    
                    warn!("Attempt {} failed: {}, retrying in {:?}", attempt, error, delay);
                    
                    // Apply jitter if configured
                    let actual_delay = if self.config.jitter {
                        let jitter_ms = (delay.as_millis() as f64 * rand::random::<f64>() * 0.1) as u64;
                        delay + Duration::from_millis(jitter_ms)
                    } else {
                        delay
                    };
                    
                    tokio::time::sleep(actual_delay).await;
                    
                    // Calculate next delay with exponential backoff
                    delay = Duration::from_millis(
                        (delay.as_millis() as f64 * self.config.backoff_multiplier) as u64
                    ).min(self.config.max_delay);
                }
            }
        }
    }
}

/// Bulkhead pattern for resource isolation
pub struct Bulkhead {
    semaphore: Arc<tokio::sync::Semaphore>,
    max_concurrent: usize,
    queue_size: usize,
    queue: Arc<RwLock<VecDeque<Instant>>>,
}

impl Bulkhead {
    pub fn new(max_concurrent: usize, queue_size: usize) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
            max_concurrent,
            queue_size,
            queue: Arc::new(RwLock::new(VecDeque::with_capacity(queue_size))),
        }
    }
    
    /// Execute an operation with bulkhead protection
    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T, ResilienceError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        // Check queue size
        let queue_len = self.queue.read().await.len();
        if queue_len >= self.queue_size {
            return Err(ResilienceError::OperationTimeout);
        }
        
        // Add to queue
        self.queue.write().await.push_back(Instant::now());
        
        // Acquire permit
        let permit = self.semaphore.acquire().await
            .map_err(|_| ResilienceError::OperationTimeout)?;
        
        // Remove from queue
        self.queue.write().await.pop_front();
        
        // Execute operation
        let result = operation().await;
        
        // Release permit
        drop(permit);
        
        Ok(result)
    }
    
    /// Get bulkhead metrics
    pub async fn get_metrics(&self) -> BulkheadMetrics {
        BulkheadMetrics {
            available_permits: self.semaphore.available_permits(),
            max_concurrent: self.max_concurrent,
            queue_size: self.queue.read().await.len(),
            max_queue_size: self.queue_size,
        }
    }
}

/// Bulkhead metrics
#[derive(Debug, Clone)]
pub struct BulkheadMetrics {
    pub available_permits: usize,
    pub max_concurrent: usize,
    pub queue_size: usize,
    pub max_queue_size: usize,
}

/// Health check trait
#[async_trait]
pub trait HealthCheck: Send + Sync {
    async fn check(&self) -> Result<HealthStatus, ResilienceError>;
}

/// Health status
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub message: String,
    pub details: std::collections::HashMap<String, String>,
}

/// Composite resilience manager
pub struct ResilienceManager {
    circuit_breaker: Arc<CircuitBreaker>,
    retry_executor: Arc<RetryExecutor>,
    bulkhead: Arc<Bulkhead>,
}

impl ResilienceManager {
    pub fn new(
        circuit_config: CircuitBreakerConfig,
        retry_config: RetryConfig,
        max_concurrent: usize,
    ) -> Self {
        Self {
            circuit_breaker: Arc::new(CircuitBreaker::new(circuit_config)),
            retry_executor: Arc::new(RetryExecutor::new(retry_config)),
            bulkhead: Arc::new(Bulkhead::new(max_concurrent, max_concurrent * 2)),
        }
    }
    
    /// Execute with full resilience protection
    pub async fn execute_with_resilience<F, Fut, T, E>(
        &self,
        operation: F,
    ) -> Result<T, ResilienceError>
    where
        F: Fn() -> Fut + Clone,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::error::Error,
    {
        // Apply bulkhead first  
        self.bulkhead.execute(|| async {
            // Then apply circuit breaker protection
            // Note: Circuit breaker execute is sync and expects a Result-returning function
            let retry_result = self.retry_executor.execute(operation).await;
            match retry_result {
                Ok(value) => {
                    // Record success in circuit breaker manually
                    self.circuit_breaker.record_success().await;
                    Ok(value)
                }
                Err(e) => {
                    // Record failure in circuit breaker manually
                    self.circuit_breaker.record_failure().await;
                    Err(e)
                }
            }
        }).await?
    }
    
    /// Get circuit breaker state
    pub async fn circuit_breaker_state(&self) -> CircuitState {
        self.circuit_breaker.get_state().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_circuit_breaker() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout_duration: Duration::from_millis(100),
            success_threshold: 2,
            failure_window: Duration::from_secs(1),
        };
        
        let breaker = CircuitBreaker::new(config);
        
        // Should start closed
        assert_eq!(breaker.get_state().await, CircuitState::Closed);
        
        // Simulate failures
        let _ = breaker.execute(|| Err::<(), std::io::Error>(std::io::Error::new(
            std::io::ErrorKind::Other,
            "test error"
        ))).await;
        
        let _ = breaker.execute(|| Err::<(), std::io::Error>(std::io::Error::new(
            std::io::ErrorKind::Other,
            "test error"
        ))).await;
        
        // Should be open after failures
        assert_eq!(breaker.get_state().await, CircuitState::Open);
        
        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Should allow a test request (half-open)
        let _ = breaker.execute(|| Ok::<_, std::io::Error>(())).await;
        assert_eq!(breaker.get_state().await, CircuitState::HalfOpen);
    }
    
    #[tokio::test]
    async fn test_retry_executor() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            jitter: false,
        };
        
        let executor = RetryExecutor::new(config);
        
        let counter = Arc::new(RwLock::new(0));
        let counter_clone = counter.clone();
        
        let result = executor.execute(|| async {
            let mut count = counter_clone.write().await;
            *count += 1;
            
            if *count < 3 {
                Err(std::io::Error::new(std::io::ErrorKind::Other, "retry test"))
            } else {
                Ok(42)
            }
        }).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(*counter.read().await, 3);
    }
    
    #[tokio::test]
    async fn test_bulkhead() {
        let bulkhead = Arc::new(Bulkhead::new(2, 4));
        
        let handle1 = tokio::spawn({
            let bulkhead = bulkhead.clone();
            async move {
                bulkhead.execute(|| async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    42
                }).await
            }
        });
        
        let handle2 = tokio::spawn({
            let bulkhead = bulkhead.clone();
            async move {
                bulkhead.execute(|| async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    43
                }).await
            }
        });
        
        let result1 = handle1.await.unwrap().unwrap();
        let result2 = handle2.await.unwrap().unwrap();
        
        assert_eq!(result1, 42);
        assert_eq!(result2, 43);
    }
}