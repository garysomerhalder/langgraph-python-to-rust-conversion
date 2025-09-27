use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::{mpsc, Mutex};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use crate::batch::{BatchResult, BatchJob, BatchJobStatus};
use crate::LangGraphError;

/// Comprehensive batch error handling and recovery system
#[derive(Debug)]
pub struct BatchErrorHandler {
    retry_strategy: RetryStrategy,
    dead_letter_queue: Arc<DeadLetterQueue>,
    circuit_breaker: Arc<CircuitBreaker>,
    error_reporter: Arc<ErrorReporter>,
    error_classifiers: Vec<Arc<dyn ErrorClassifier>>,
}

/// Error classification for different error types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorType {
    /// Transient error - retry immediately
    Transient(TransientError),
    /// Recoverable error - retry with backoff
    Recoverable(RecoverableError),
    /// Permanent error - move to dead letter queue
    Permanent(PermanentError),
    /// Fatal error - fail entire batch
    Fatal(FatalError),
}

/// Transient error that should be retried immediately
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransientError {
    pub message: String,
    pub code: String,
    pub context: HashMap<String, String>,
}

/// Recoverable error that should be retried with backoff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverableError {
    pub message: String,
    pub code: String,
    pub retry_after: Option<Duration>,
    pub context: HashMap<String, String>,
}

/// Permanent error that should go to dead letter queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermanentError {
    pub message: String,
    pub code: String,
    pub reason: String,
    pub context: HashMap<String, String>,
}

/// Fatal error that should fail the entire batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FatalError {
    pub message: String,
    pub code: String,
    pub severity: String,
    pub context: HashMap<String, String>,
}

/// Retry strategy configuration
#[derive(Clone)]
pub struct RetryStrategy {
    pub max_attempts: u32,
    pub backoff_strategy: BackoffStrategy,
    pub jitter: bool,
    pub retry_predicates: Vec<RetryPredicate>,
}

impl std::fmt::Debug for RetryStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryStrategy")
            .field("max_attempts", &self.max_attempts)
            .field("backoff_strategy", &self.backoff_strategy)
            .field("jitter", &self.jitter)
            .field("retry_predicates", &format!("{} predicates", self.retry_predicates.len()))
            .finish()
    }
}

/// Backoff strategy for retries
#[derive(Debug, Clone)]
pub enum BackoffStrategy {
    Fixed(Duration),
    Linear { base: Duration, increment: Duration },
    Exponential { base: Duration, multiplier: f64, max: Duration },
}

/// Predicate function for determining if an error should be retried
pub type RetryPredicate = Arc<dyn Fn(&ErrorType) -> bool + Send + Sync>;

/// Dead letter queue for persistently failing jobs
#[derive(Debug)]
pub struct DeadLetterQueue {
    failed_jobs: Arc<Mutex<HashMap<String, FailedJob>>>,
    storage: Arc<dyn DeadLetterStorage>,
    max_size: usize,
}

/// Information about a failed job in the dead letter queue
#[derive(Debug, Clone)]
pub struct FailedJob {
    pub job: BatchJob,
    pub error: ErrorType,
    pub failure_count: u32,
    pub first_failure: DateTime<Utc>,
    pub last_failure: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

/// Storage interface for dead letter queue
pub trait DeadLetterStorage: Send + Sync + Debug {
    fn store_failed_job(&self, job_id: &str, failed_job: &FailedJob) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>>;
    fn retrieve_failed_job(&self, job_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<FailedJob>, LangGraphError>> + Send + '_>>;
    fn list_failed_jobs(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>, LangGraphError>> + Send + '_>>;
    fn remove_failed_job(&self, job_id: &str) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>>;
}

/// Circuit breaker for external dependencies
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<Mutex<CircuitBreakerState>>,
    failure_threshold: u32,
    recovery_timeout: Duration,
    half_open_max_calls: u32,
}

/// Circuit breaker state
#[derive(Debug, Clone)]
pub enum CircuitBreakerState {
    Closed { failure_count: u32 },
    Open { opened_at: DateTime<Utc> },
    HalfOpen { test_calls: u32 },
}

/// Error reporting and aggregation
#[derive(Debug)]
pub struct ErrorReporter {
    error_aggregator: Arc<Mutex<ErrorAggregator>>,
    alerts: Vec<Arc<dyn AlertHandler>>,
}

/// Error aggregation for reporting
#[derive(Debug, Default)]
pub struct ErrorAggregator {
    pub error_counts: HashMap<String, u64>,
    pub error_trends: Vec<ErrorTrend>,
    pub failure_rates: HashMap<String, f64>,
}

/// Error trend over time
#[derive(Debug, Clone)]
pub struct ErrorTrend {
    pub timestamp: DateTime<Utc>,
    pub error_type: String,
    pub count: u64,
}

/// Alert handler for error notifications
pub trait AlertHandler: Send + Sync + Debug {
    fn handle_alert(&self, alert: &ErrorAlert) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>>;
}

/// Error alert information
#[derive(Debug, Clone)]
pub struct ErrorAlert {
    pub severity: AlertSeverity,
    pub message: String,
    pub error_type: String,
    pub metadata: HashMap<String, String>,
}

/// Alert severity levels
#[derive(Debug, Clone)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Error classification interface
pub trait ErrorClassifier: Send + Sync + Debug {
    fn classify_error(&self, error: &LangGraphError, context: &ErrorContext) -> ErrorType;
}

/// Context information for error classification
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub job_id: String,
    pub attempt_number: u32,
    pub total_attempts: u32,
    pub execution_duration: Duration,
    pub metadata: HashMap<String, String>,
}

/// Batch recovery mechanisms
#[derive(Debug)]
pub struct BatchRecovery {
    recovery_strategies: Vec<Arc<dyn RecoveryStrategy>>,
    recovery_history: Arc<Mutex<Vec<RecoveryAttempt>>>,
}

/// Recovery strategy interface
pub trait RecoveryStrategy: Send + Sync + Debug {
    fn can_recover(&self, error: &ErrorType, context: &ErrorContext) -> Pin<Box<dyn Future<Output = bool> + Send + '_>>;
    fn recover(&self, job: &BatchJob, error: &ErrorType) -> Pin<Box<dyn Future<Output = Result<BatchJob, LangGraphError>> + Send + '_>>;
}

/// Recovery attempt record
#[derive(Debug, Clone)]
pub struct RecoveryAttempt {
    pub job_id: String,
    pub strategy_used: String,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub error: Option<String>,
}

impl BatchErrorHandler {
    /// Create a new batch error handler
    pub fn new(retry_strategy: RetryStrategy) -> Self {
        Self {
            retry_strategy,
            dead_letter_queue: Arc::new(DeadLetterQueue::new()),
            circuit_breaker: Arc::new(CircuitBreaker::new()),
            error_reporter: Arc::new(ErrorReporter::new()),
            error_classifiers: vec![Arc::new(DefaultErrorClassifier)],
        }
    }

    /// Handle a batch job error
    pub async fn handle_error(
        &self,
        job: &BatchJob,
        error: LangGraphError,
        context: ErrorContext,
    ) -> Result<ErrorHandlingDecision, LangGraphError> {
        // Classify error type
        let error_type = self.error_classifiers[0].classify_error(&error, &context);

        // Report the error
        let _ = self.error_reporter.report_error(&error_type, &context).await;

        // Determine handling strategy based on error type
        match error_type {
            ErrorType::Transient(_) => {
                if context.attempt_number < self.retry_strategy.max_attempts {
                    let delay = self.calculate_retry_delay(context.attempt_number);
                    Ok(ErrorHandlingDecision::Retry {
                        delay,
                        attempt: context.attempt_number + 1
                    })
                } else {
                    // Max attempts reached, move to dead letter queue
                    let _ = self.dead_letter_queue.add_failed_job(job.clone(), error_type).await;
                    Ok(ErrorHandlingDecision::MoveToDeadLetter)
                }
            },
            ErrorType::Recoverable(ref recoverable_error) => {
                if context.attempt_number < self.retry_strategy.max_attempts {
                    let delay = recoverable_error.retry_after
                        .unwrap_or_else(|| self.calculate_retry_delay(context.attempt_number));
                    Ok(ErrorHandlingDecision::Retry {
                        delay,
                        attempt: context.attempt_number + 1
                    })
                } else {
                    let _ = self.dead_letter_queue.add_failed_job(job.clone(), error_type).await;
                    Ok(ErrorHandlingDecision::MoveToDeadLetter)
                }
            },
            ErrorType::Permanent(_) => {
                let _ = self.dead_letter_queue.add_failed_job(job.clone(), error_type).await;
                Ok(ErrorHandlingDecision::MoveToDeadLetter)
            },
            ErrorType::Fatal(_) => {
                let _ = self.dead_letter_queue.add_failed_job(job.clone(), error_type).await;
                Ok(ErrorHandlingDecision::FailBatch)
            }
        }
    }

    /// Check if a job should be retried
    pub async fn should_retry(&self, error: &ErrorType, attempt: u32) -> bool {
        // Check if we've exceeded max attempts
        if attempt >= self.retry_strategy.max_attempts {
            return false;
        }

        // Check error type for retry eligibility
        match error {
            ErrorType::Transient(_) => true,
            ErrorType::Recoverable(_) => true,
            ErrorType::Permanent(_) => false,
            ErrorType::Fatal(_) => false,
        }
    }

    /// Calculate the next retry delay
    pub fn calculate_retry_delay(&self, attempt: u32) -> Duration {
        match &self.retry_strategy.backoff_strategy {
            BackoffStrategy::Fixed(duration) => *duration,
            BackoffStrategy::Linear { base, increment } => {
                *base + (*increment * attempt)
            },
            BackoffStrategy::Exponential { base, multiplier, max } => {
                let delay = *base * (*multiplier as u32).pow(attempt - 1);
                let delay_duration = Duration::from_millis(delay.as_millis() as u64);
                delay_duration.min(*max)
            }
        }
    }
}

/// Decision made after handling an error
#[derive(Debug, Clone)]
pub enum ErrorHandlingDecision {
    Retry { delay: Duration, attempt: u32 },
    MoveToDeadLetter,
    FailBatch,
    Ignore,
}

impl DeadLetterQueue {
    /// Create a new dead letter queue
    pub fn new() -> Self {
        Self {
            failed_jobs: Arc::new(Mutex::new(HashMap::new())),
            storage: Arc::new(MemoryDeadLetterStorage::new()),
            max_size: 10000,
        }
    }

    /// Add a job to the dead letter queue
    pub async fn add_failed_job(&self, job: BatchJob, error: ErrorType) -> Result<(), LangGraphError> {
        let failed_job = FailedJob {
            job: job.clone(),
            error,
            failure_count: 1,
            first_failure: Utc::now(),
            last_failure: Utc::now(),
            metadata: HashMap::new(),
        };

        // Store in persistent storage
        self.storage.store_failed_job(&job.id, &failed_job).await?;

        // Add to in-memory cache
        let mut cache = self.failed_jobs.lock().await;
        cache.insert(job.id, failed_job);

        Ok(())
    }

    /// Retrieve a job from the dead letter queue
    pub async fn get_failed_job(&self, job_id: &str) -> Result<Option<FailedJob>, LangGraphError> {
        // Check in-memory cache first
        let cache = self.failed_jobs.lock().await;
        if let Some(failed_job) = cache.get(job_id) {
            return Ok(Some(failed_job.clone()));
        }
        drop(cache);

        // Check persistent storage
        self.storage.retrieve_failed_job(job_id).await
    }

    /// Resurrect a job from the dead letter queue for retry
    pub async fn resurrect_job(&self, job_id: &str) -> Result<Option<BatchJob>, LangGraphError> {
        // Get the failed job
        if let Some(failed_job) = self.get_failed_job(job_id).await? {
            // Remove from dead letter queue
            self.storage.remove_failed_job(job_id).await?;
            let mut cache = self.failed_jobs.lock().await;
            cache.remove(job_id);

            Ok(Some(failed_job.job))
        } else {
            Ok(None)
        }
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(CircuitBreakerState::Closed { failure_count: 0 })),
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            half_open_max_calls: 3,
        }
    }

    /// Check if the circuit breaker allows the call
    pub async fn allow_call(&self) -> bool {
        let state = self.state.lock().await;
        match &*state {
            CircuitBreakerState::Closed { .. } => true,
            CircuitBreakerState::Open { opened_at } => {
                // Check if recovery timeout has passed
                Utc::now().signed_duration_since(*opened_at) > chrono::Duration::from_std(self.recovery_timeout).unwrap_or_default()
            },
            CircuitBreakerState::HalfOpen { test_calls } => {
                *test_calls < self.half_open_max_calls
            }
        }
    }

    /// Record a successful call
    pub async fn record_success(&self) {
        let mut state = self.state.lock().await;
        match &*state {
            CircuitBreakerState::Closed { .. } => {
                // Already closed, do nothing
            },
            CircuitBreakerState::HalfOpen { .. } => {
                // Success in half-open state closes the circuit
                *state = CircuitBreakerState::Closed { failure_count: 0 };
            },
            CircuitBreakerState::Open { .. } => {
                // Should not happen if allow_call is respected
                *state = CircuitBreakerState::Closed { failure_count: 0 };
            }
        }
    }

    /// Record a failed call
    pub async fn record_failure(&self) {
        let mut state = self.state.lock().await;
        match &*state {
            CircuitBreakerState::Closed { failure_count } => {
                let new_count = failure_count + 1;
                if new_count >= self.failure_threshold {
                    *state = CircuitBreakerState::Open { opened_at: Utc::now() };
                } else {
                    *state = CircuitBreakerState::Closed { failure_count: new_count };
                }
            },
            CircuitBreakerState::HalfOpen { test_calls } => {
                // Failure in half-open state opens the circuit again
                *state = CircuitBreakerState::Open { opened_at: Utc::now() };
            },
            CircuitBreakerState::Open { .. } => {
                // Already open, do nothing
            }
        }
    }
}

impl ErrorReporter {
    /// Create a new error reporter
    pub fn new() -> Self {
        Self {
            error_aggregator: Arc::new(Mutex::new(ErrorAggregator::default())),
            alerts: vec![],
        }
    }

    /// Report an error
    pub async fn report_error(&self, error: &ErrorType, context: &ErrorContext) -> Result<(), LangGraphError> {
        let mut aggregator = self.error_aggregator.lock().await;

        // Get error type string
        let error_type_str = match error {
            ErrorType::Transient(e) => format!("transient:{}", e.code),
            ErrorType::Recoverable(e) => format!("recoverable:{}", e.code),
            ErrorType::Permanent(e) => format!("permanent:{}", e.code),
            ErrorType::Fatal(e) => format!("fatal:{}", e.code),
        };

        // Update error counts
        *aggregator.error_counts.entry(error_type_str.clone()).or_insert(0) += 1;

        // Add error trend
        aggregator.error_trends.push(ErrorTrend {
            timestamp: Utc::now(),
            error_type: error_type_str,
            count: 1,
        });

        Ok(())
    }

    /// Get error statistics
    pub async fn get_error_stats(&self) -> ErrorAggregator {
        let aggregator = self.error_aggregator.lock().await;
        aggregator.clone()
    }
}

/// Default error classifier implementation
#[derive(Debug)]
pub struct DefaultErrorClassifier;

impl ErrorClassifier for DefaultErrorClassifier {
    fn classify_error(&self, error: &LangGraphError, context: &ErrorContext) -> ErrorType {
        match error {
            LangGraphError::Execution(msg) if msg.contains("timeout") => {
                ErrorType::Transient(TransientError {
                    message: msg.clone(),
                    code: "TIMEOUT".to_string(),
                    context: HashMap::new(),
                })
            },
            LangGraphError::Execution(msg) if msg.contains("Connection") => {
                ErrorType::Transient(TransientError {
                    message: msg.clone(),
                    code: "CONNECTION_ERROR".to_string(),
                    context: HashMap::new(),
                })
            },
            LangGraphError::StateError(msg) if msg.contains("rate limit") => {
                ErrorType::Recoverable(RecoverableError {
                    message: msg.clone(),
                    code: "RATE_LIMIT".to_string(),
                    retry_after: Some(Duration::from_secs(5)),
                    context: HashMap::new(),
                })
            },
            LangGraphError::State(msg) if msg.contains("rate limit") => {
                ErrorType::Recoverable(RecoverableError {
                    message: msg.clone(),
                    code: "RATE_LIMIT".to_string(),
                    retry_after: Some(Duration::from_secs(5)),
                    context: HashMap::new(),
                })
            },
            LangGraphError::GraphValidation(msg) => {
                ErrorType::Permanent(PermanentError {
                    message: msg.clone(),
                    code: "VALIDATION_ERROR".to_string(),
                    reason: "Invalid job configuration".to_string(),
                    context: HashMap::new(),
                })
            },
            LangGraphError::Internal(msg) => {
                ErrorType::Fatal(FatalError {
                    message: msg.clone(),
                    code: "SYSTEM_ERROR".to_string(),
                    severity: "high".to_string(),
                    context: HashMap::new(),
                })
            },
            _ => {
                // Default to recoverable for unknown errors
                ErrorType::Recoverable(RecoverableError {
                    message: error.to_string(),
                    code: "UNKNOWN_ERROR".to_string(),
                    retry_after: Some(Duration::from_secs(1)),
                    context: HashMap::new(),
                })
            }
        }
    }
}

/// Memory-based dead letter storage for testing
#[derive(Debug)]
pub struct MemoryDeadLetterStorage {
    storage: Arc<Mutex<HashMap<String, FailedJob>>>,
}

impl MemoryDeadLetterStorage {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl DeadLetterStorage for MemoryDeadLetterStorage {
    fn store_failed_job(&self, job_id: &str, failed_job: &FailedJob) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>> {
        let job_id = job_id.to_string();
        let failed_job = failed_job.clone();
        Box::pin(async move {
            let mut storage = self.storage.lock().await;
            storage.insert(job_id, failed_job);
            Ok(())
        })
    }

    fn retrieve_failed_job(&self, job_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<FailedJob>, LangGraphError>> + Send + '_>> {
        let job_id = job_id.to_string();
        Box::pin(async move {
            let storage = self.storage.lock().await;
            Ok(storage.get(&job_id).cloned())
        })
    }

    fn list_failed_jobs(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>, LangGraphError>> + Send + '_>> {
        Box::pin(async move {
            let storage = self.storage.lock().await;
            Ok(storage.keys().cloned().collect())
        })
    }

    fn remove_failed_job(&self, job_id: &str) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>> {
        let job_id = job_id.to_string();
        Box::pin(async move {
            let mut storage = self.storage.lock().await;
            storage.remove(&job_id);
            Ok(())
        })
    }
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_strategy: BackoffStrategy::Exponential {
                base: Duration::from_millis(100),
                multiplier: 2.0,
                max: Duration::from_secs(60),
            },
            jitter: true,
            retry_predicates: vec![],
        }
    }
}