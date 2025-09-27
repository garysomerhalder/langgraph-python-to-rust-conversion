//! Rate limiting for graph execution
//!
//! Provides rate limiting capabilities to prevent resource exhaustion
//! and ensure fair resource usage across multiple graph executions.

use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};
use tokio::time;

/// Errors that can occur during rate limiting
#[derive(Error, Debug, Clone)]
pub enum RateLimitError {
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    #[error("Timeout waiting for rate limit permit")]
    Timeout,

    #[error("Rate limiter is shutting down")]
    ShuttingDown,

    #[error("Semaphore has been closed")]
    SemaphoreClosed,
}

/// Rate limiter for controlling execution frequency
pub struct RateLimiter {
    /// Maximum number of executions per window
    max_executions: usize,

    /// Time window for rate limiting
    window: Duration,

    /// Semaphore for controlling concurrent access
    semaphore: Arc<Semaphore>,

    /// Token bucket for rate limiting
    token_bucket: Arc<Mutex<TokenBucket>>,
}

/// Token bucket for rate limiting algorithm
struct TokenBucket {
    /// Current number of tokens
    tokens: f64,

    /// Maximum tokens in the bucket
    capacity: f64,

    /// Refill rate (tokens per second)
    refill_rate: f64,

    /// Last refill timestamp
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a new token bucket
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Try to consume tokens
    #[inline]
    fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();

        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    #[inline]
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let tokens_to_add = elapsed.as_secs_f64() * self.refill_rate;

        self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
        self.last_refill = now;
    }

    /// Calculate wait time for required tokens
    fn wait_time(&self, tokens: f64) -> Duration {
        if self.tokens >= tokens {
            return Duration::ZERO;
        }

        let tokens_needed = tokens - self.tokens;
        let seconds_to_wait = tokens_needed / self.refill_rate;
        Duration::from_secs_f64(seconds_to_wait)
    }
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(max_executions: usize, window: Duration) -> Self {
        let refill_rate = max_executions as f64 / window.as_secs_f64();

        Self {
            max_executions,
            window,
            semaphore: Arc::new(Semaphore::new(max_executions)),
            token_bucket: Arc::new(Mutex::new(TokenBucket::new(
                max_executions as f64,
                refill_rate,
            ))),
        }
    }

    /// Acquire a permit for execution
    pub async fn acquire(&self) -> Result<RateLimitPermit, RateLimitError> {
        // Try to acquire immediately
        let mut bucket = self.token_bucket.lock().await;

        if bucket.try_consume(1.0) {
            let permit = self
                .semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| RateLimitError::SemaphoreClosed)?;
            return Ok(RateLimitPermit { _permit: permit });
        }

        // Calculate wait time
        let wait_time = bucket.wait_time(1.0);
        drop(bucket);

        // Wait and retry
        time::sleep(wait_time).await;

        let mut bucket = self.token_bucket.lock().await;
        if bucket.try_consume(1.0) {
            let permit = self
                .semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| RateLimitError::SemaphoreClosed)?;
            Ok(RateLimitPermit { _permit: permit })
        } else {
            Err(RateLimitError::RateLimitExceeded(
                "Unable to acquire rate limit permit".to_string(),
            ))
        }
    }

    /// Try to acquire a permit without waiting
    pub async fn try_acquire(&self) -> Result<RateLimitPermit, RateLimitError> {
        let mut bucket = self.token_bucket.lock().await;

        if bucket.try_consume(1.0) {
            let permit = self
                .semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| RateLimitError::SemaphoreClosed)?;
            Ok(RateLimitPermit { _permit: permit })
        } else {
            Err(RateLimitError::RateLimitExceeded(format!(
                "Rate limit exceeded: {}/{} per {:?}",
                0, self.max_executions, self.window
            )))
        }
    }

    /// Acquire with timeout
    pub async fn acquire_timeout(
        &self,
        timeout: Duration,
    ) -> Result<RateLimitPermit, RateLimitError> {
        match time::timeout(timeout, self.acquire()).await {
            Ok(result) => result,
            Err(_) => Err(RateLimitError::Timeout),
        }
    }
}

/// Permit for rate-limited execution
pub struct RateLimitPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

/// Adaptive rate limiter that adjusts based on system load
pub struct AdaptiveRateLimiter {
    /// Base rate limiter
    base_limiter: RateLimiter,

    /// Load threshold for reducing rate
    load_threshold: f64,

    /// Current load factor (0.0 to 1.0)
    load_factor: Arc<Mutex<f64>>,
}

impl AdaptiveRateLimiter {
    /// Create a new adaptive rate limiter
    pub fn new(max_executions: usize, window: Duration, load_threshold: f64) -> Self {
        Self {
            base_limiter: RateLimiter::new(max_executions, window),
            load_threshold,
            load_factor: Arc::new(Mutex::new(0.0)),
        }
    }

    /// Update the current load factor
    pub async fn update_load(&self, load: f64) {
        let mut factor = self.load_factor.lock().await;
        *factor = load.clamp(0.0, 1.0);
    }

    /// Acquire a permit with adaptive rate limiting
    pub async fn acquire(&self) -> Result<RateLimitPermit, RateLimitError> {
        let load = *self.load_factor.lock().await;

        // If load is high, add extra delay
        if load > self.load_threshold {
            let extra_delay = Duration::from_millis(((load - self.load_threshold) * 1000.0) as u64);
            time::sleep(extra_delay).await;
        }

        self.base_limiter.acquire().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_basic() {
        let limiter = RateLimiter::new(5, Duration::from_secs(1));

        // Should be able to acquire 5 permits immediately
        for _ in 0..5 {
            assert!(limiter.try_acquire().await.is_ok());
        }

        // 6th should fail
        assert!(limiter.try_acquire().await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_refill() {
        let limiter = RateLimiter::new(2, Duration::from_secs(1));

        // Consume all tokens
        let _p1 = limiter.acquire().await.unwrap();
        let _p2 = limiter.acquire().await.unwrap();

        // Should fail immediately
        assert!(limiter.try_acquire().await.is_err());

        // Wait for refill
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Should succeed after refill
        assert!(limiter.try_acquire().await.is_ok());
    }

    #[tokio::test]
    async fn test_adaptive_rate_limiter() {
        let limiter = AdaptiveRateLimiter::new(5, Duration::from_secs(1), 0.7);

        // Normal load
        limiter.update_load(0.5).await;
        assert!(limiter.acquire().await.is_ok());

        // High load
        limiter.update_load(0.9).await;
        let start = Instant::now();
        assert!(limiter.acquire().await.is_ok());

        // Should have added extra delay
        assert!(start.elapsed() >= Duration::from_millis(200));
    }
}
